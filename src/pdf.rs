//! PDFs sent in a chat, read inside the app.
//!
//! hayro parses and rasterizes on the CPU in pure Rust, so a file from a
//! stranger meets memory-safe code and no C++ engine ships beside the app.
//! A document gets one render thread, which owns the parsed file and hayro's
//! caches: both borrow the file and use `Rc`, so they never leave that
//! thread. A text thread starts with the first search, reads every page's
//! glyphs once (invisible ones too, so a scan's OCR layer is found), and
//! sends each page's text back as it goes.
//!
//! The window says which pages it wants each frame, nearest first, and the
//! worker takes the head of that list, so pages scrolled past are never
//! rendered. Finished pages come back as pixels, which the window uploads a
//! couple per frame. It keeps only what it asked for: the pages on screen
//! at their size, those just off it at half that, and, past the size a
//! whole page is rendered at (`BASE_PIXELS`), the part of each page on
//! screen at full sharpness. So what it holds follows the window's size, not
//! the zoom or the document's length. The text read for search goes when
//! the search closes, and everything when the viewer does.
//!
//! The file is mapped rather than read where the system allows it, so its
//! bytes are the file's own pages, which the system can drop and read again,
//! not a copy held by the app.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

use egui::{Rect, Vec2, pos2, vec2};

use crate::model::ChatId;

/// Display points around the pages, and between two of them.
pub const MARGIN: f32 = 24.0;
pub const GAP: f32 = 14.0;
/// The widest a page is drawn when it fits: a reading width, however wide
/// the window is.
pub const FIT_MAX: f32 = 900.0;
pub const ZOOM_MIN: f32 = 0.25;
pub const ZOOM_MAX: f32 = 5.0;
/// Each zoom step, by button, key or wheel notch.
pub const ZOOM_STEP: f32 = 1.2;
/// How long the render thread waits with nothing to do before it hands back
/// what it freed: by then the window has uploaded and dropped its pages.
const IDLE: std::time::Duration = std::time::Duration::from_millis(300);
/// The most pixels a whole page is rendered with: a page at the fit zoom on
/// a 2x display ([`FIT_MAX`] wide, 1,878 pixels after the step up) takes
/// 4.6 million for US Letter and 5.0 million for A4. Past it the page is
/// drawn from this, and the part on screen rendered again at full sharpness
/// as a detail.
const BASE_PIXELS: u64 = 5_200_000;
/// The most pixels a detail is rendered with, for a window larger than any
/// screen; past it the detail is the part on screen alone.
const DETAIL_PIXELS: u64 = 16 << 20;
/// A detail reaches this far past the screen, as a share of the view's
/// height above and below and of its width each side, so a scroll does not
/// render it again at once.
const DETAIL_REACH: (f32, f32) = (0.15, 0.3);
/// Details are snapped out to this grid, in pixels, so a small scroll keeps
/// asking for the same one.
const DETAIL_GRID: u32 = 128;
/// Pages rendered ahead of the view, each way, at half the width of those on
/// screen.
const PREFETCH: usize = 2;
/// Pages uploaded to the GPU in one frame, so a burst never drops one.
const UPLOADS_PER_FRAME: usize = 2;
/// hayro's caches only grow; a fresh one after this many pages keeps a long
/// read from holding every font and image it ever met.
const CACHE_PAGES: usize = 32;
/// Seconds a zoom must rest before pages are rendered at the new size; until
/// then the pages already held are stretched.
const ZOOM_SETTLE: f64 = 0.18;
/// Stack for the worker threads: a deeply nested file recurses.
const STACK: usize = 16 << 20;

/// Whether `path` holds a PDF: its header, `%PDF-`, within the first
/// kilobyte, where the format allows junk before it. Reads that kilobyte.
pub fn sniff(path: &Path) -> bool {
    use std::io::Read;
    let mut head = [0u8; 1024];
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut read = 0;
    while read < head.len() {
        match file.read(&mut head[read..]) {
            Ok(0) => break,
            Ok(count) => read += count,
            Err(_) => return false,
        }
    }
    has_header(&head[..read])
}

fn has_header(bytes: &[u8]) -> bool {
    bytes.windows(5).any(|window| window == b"%PDF-")
}

/// Why a document could not be shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// It needs a password, or uses encryption hayro does not read.
    Locked,
    /// It is damaged, empty, or not a PDF after all.
    Unreadable,
}

/// A page's text for search: each character folded for matching, and the
/// box of the glyph it came from, in fractions of the page.
#[derive(Clone, Debug, Default)]
pub struct PageText {
    /// Folded with [`crate::util::search_key`], whitespace as single spaces,
    /// with spaces and line breaks inferred where the file left them out.
    chars: Vec<char>,
    /// For each of `chars`, its glyph in `boxes`, or `NONE` for an inferred
    /// space.
    owners: Vec<u32>,
    /// Each glyph's box in 65,536ths of the page, half the size of a `Rect`:
    /// a long document's text stays in memory while it is searched.
    boxes: Vec<[u16; 4]>,
}

const NONE: u32 = u32::MAX;

/// `rect`, in fractions of the page, as 65,536ths.
fn pack(rect: Rect) -> [u16; 4] {
    let unit = |value: f32| (value.clamp(0.0, 1.0) * f32::from(u16::MAX)).round() as u16;
    [
        unit(rect.min.x),
        unit(rect.min.y),
        unit(rect.max.x),
        unit(rect.max.y),
    ]
}

fn unpack(packed: [u16; 4]) -> Rect {
    let unit = |value: u16| f32::from(value) / f32::from(u16::MAX);
    Rect::from_min_max(
        pos2(unit(packed[0]), unit(packed[1])),
        pos2(unit(packed[2]), unit(packed[3])),
    )
}

/// One place the query was found: its page, and the boxes to light, in
/// fractions of the page, one per line it spans.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub page: usize,
    pub rects: Vec<Rect>,
}

impl PageText {
    /// Builds the text from glyphs in drawing order, each its text and its
    /// box in fractions of the page.
    fn from_glyphs(glyphs: &[(String, Rect)]) -> Self {
        let mut text = Self::default();
        let mut last: Option<Rect> = None;
        for (unicode, rect) in glyphs {
            if let Some(previous) = last {
                let height = previous.height().max(rect.height()).max(f32::EPSILON);
                let same_line = (previous.center().y - rect.center().y).abs() < height * 0.5;
                let gap = rect.left() - previous.right();
                // A new line, or a gap wider than a thin space: the file
                // placed the words apart without a space between them.
                if !same_line || gap > height * 0.12 {
                    text.push_space();
                }
            }
            let owner = text.boxes.len() as u32;
            text.boxes.push(pack(*rect));
            for c in unicode.chars() {
                if c.is_whitespace() {
                    text.push_space();
                    continue;
                }
                let mut buffer = [0u8; 4];
                for folded in crate::util::search_key(c.encode_utf8(&mut buffer)).chars() {
                    text.chars.push(folded);
                    text.owners.push(owner);
                }
            }
            last = Some(*rect);
        }
        text
    }

    fn push_space(&mut self) {
        if self.chars.last().is_some_and(|c| *c != ' ') {
            self.chars.push(' ');
            self.owners.push(NONE);
        }
    }

    /// Every place `query`, already folded by [`fold_query`], occurs, in
    /// order.
    fn find(&self, page: usize, query: &[char], hits: &mut Vec<Hit>) {
        if query.is_empty() || query.len() > self.chars.len() {
            return;
        }
        let mut start = 0;
        while start + query.len() <= self.chars.len() {
            if self.chars[start..start + query.len()] == *query {
                hits.push(Hit {
                    page,
                    rects: self.rects(start..start + query.len()),
                });
                start += query.len();
            } else {
                start += 1;
            }
        }
    }

    /// The boxes of the glyphs behind `range`, joined into one per line.
    fn rects(&self, range: std::ops::Range<usize>) -> Vec<Rect> {
        let mut rects: Vec<Rect> = Vec::new();
        let mut previous = NONE;
        for &owner in &self.owners[range] {
            if owner == NONE || owner == previous {
                continue;
            }
            previous = owner;
            let rect = unpack(self.boxes[owner as usize]);
            match rects.last_mut() {
                Some(line)
                    if (line.center().y - rect.center().y).abs()
                        < line.height().max(rect.height()) * 0.5 =>
                {
                    *line = line.union(rect);
                }
                _ => rects.push(rect),
            }
        }
        rects
    }
}

/// `query` as the pages' text is folded: case and accents dropped, runs of
/// whitespace as one space, none at either end.
pub fn fold_query(query: &str) -> Vec<char> {
    let folded = crate::util::search_key(query.trim());
    let mut chars = Vec::with_capacity(folded.len());
    for c in folded.chars() {
        if c.is_whitespace() {
            if chars.last().is_some_and(|last| *last != ' ') {
                chars.push(' ');
            }
        } else {
            chars.push(c);
        }
    }
    chars
}

/// A part of a page, in pixels at the width it is rendered at: left, top,
/// right and bottom.
pub type Region = [u32; 4];

fn covers(outer: Region, inner: Region) -> bool {
    outer[0] <= inner[0] && outer[1] <= inner[1] && outer[2] >= inner[2] && outer[3] >= inner[3]
}

/// A page to render, at a width in pixels: whole, or only `region` of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Job {
    page: usize,
    width: u32,
    region: Option<Region>,
}

enum Reply {
    Opened(Vec<Vec2>),
    Failed(Failure),
    Page {
        job: Job,
        /// Where the pixels lie on the page, in fractions of it.
        place: Rect,
        image: egui::ColorImage,
    },
    PageFailed(usize),
    /// A page's text, from the text thread started `generation`th.
    Text {
        generation: u32,
        page: usize,
        text: PageText,
    },
}

#[derive(Default)]
struct Queue {
    /// What the window wants, most wanted first. Replaced every frame.
    jobs: Vec<Job>,
    /// The job being rendered, so the window does not ask for it again.
    current: Option<Job>,
}

struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
    closed: AtomicBool,
    /// Asks the idle render thread to hand back what was freed since it
    /// last did, as after the search lets its text go.
    release: AtomicBool,
}

impl Shared {
    fn lock(&self) -> std::sync::MutexGuard<'_, Queue> {
        self.queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Waits for the next job and marks it current; `None` once closed.
    /// Once the thread has waited [`IDLE`] with nothing to do, it hands the
    /// memory it freed back to the system (see [`release_freed`]).
    fn next(&self) -> Option<Job> {
        let mut queue = self.lock();
        let mut released = false;
        loop {
            if self.closed.load(Ordering::Acquire) {
                return None;
            }
            if !queue.jobs.is_empty() {
                let job = queue.jobs.remove(0);
                queue.current = Some(job);
                return Some(job);
            }
            if self.release.swap(false, Ordering::AcqRel) {
                release_freed();
            }
            let (guard, wait) = self
                .wake
                .wait_timeout(queue, IDLE)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            queue = guard;
            if wait.timed_out() && !released {
                released = true;
                release_freed();
            }
        }
    }
}

/// A document's bytes: the file mapped into memory where the system allows
/// it, else read into it.
pub enum Bytes {
    #[cfg(unix)]
    Mapped(memmap2::Mmap),
    Read(Vec<u8>),
}

impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        match self {
            #[cfg(unix)]
            Self::Mapped(map) => map,
            Self::Read(bytes) => bytes,
        }
    }
}

impl Bytes {
    /// The file at `path`. On Unix it is mapped: the pages the parser reads
    /// are the file's own, which the system drops under pressure and reads
    /// again, so a document of hundreds of megabytes costs the app none of
    /// its own memory. Windows reads it, since a mapped file there could not
    /// be replaced or deleted while the viewer is open.
    pub fn load(path: &Path) -> std::io::Result<Self> {
        #[cfg(unix)]
        {
            let file = std::fs::File::open(path)?;
            if file.metadata()?.len() > 0 {
                // SAFETY: a mapping is undefined behaviour only if the file
                // changes under it. Zeus never rewrites an attachment in
                // place: downloads are written to a temporary file and
                // renamed over it, which leaves the mapped file untouched.
                // A file shrunk by another program could fault a read, which
                // is the risk any mapped reader of a user's files takes.
                if let Ok(map) = unsafe { memmap2::Mmap::map(&file) } {
                    return Ok(Self::Mapped(map));
                }
            }
        }
        std::fs::read(path).map(Self::Read)
    }
}

/// The threads behind one open document.
struct Worker {
    bytes: Arc<Bytes>,
    shared: Arc<Shared>,
    replies: Receiver<Reply>,
    sender: Sender<Reply>,
    ctx: egui::Context,
    /// The text thread's stop flag, while one is reading or its text is kept.
    text: Option<Arc<AtomicBool>>,
    /// How many text threads have started; replies from older ones are stale.
    text_generation: u32,
}

impl Worker {
    fn start(bytes: Arc<Bytes>, ctx: egui::Context) -> Self {
        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue::default()),
            wake: Condvar::new(),
            closed: AtomicBool::new(false),
            release: AtomicBool::new(false),
        });
        let (sender, replies) = channel();
        {
            let bytes = bytes.clone();
            let shared = shared.clone();
            let replies = sender.clone();
            let ctx = ctx.clone();
            let spawned = std::thread::Builder::new()
                .name("pdf-render".into())
                .stack_size(STACK)
                .spawn(move || render_thread(&bytes, &shared, &replies, &ctx));
            if let Err(error) = spawned {
                log::warn!("pdf: could not start the render thread: {error}");
                let _ = sender.send(Reply::Failed(Failure::Unreadable));
            }
        }
        Self {
            bytes,
            shared,
            replies,
            sender,
            ctx,
            text: None,
            text_generation: 0,
        }
    }

    fn start_text(&mut self, pages: usize) {
        if self.text.is_some() {
            return;
        }
        self.text_generation += 1;
        let stop = Arc::new(AtomicBool::new(false));
        self.text = Some(stop.clone());
        let bytes = self.bytes.clone();
        let shared = self.shared.clone();
        let sender = self.sender.clone();
        let ctx = self.ctx.clone();
        let generation = self.text_generation;
        let spawned = std::thread::Builder::new()
            .name("pdf-text".into())
            .stack_size(STACK)
            .spawn(move || {
                let stopped =
                    || shared.closed.load(Ordering::Acquire) || stop.load(Ordering::Acquire);
                text_thread(&bytes, pages, generation, &stopped, &sender, &ctx);
            });
        if let Err(error) = spawned {
            log::warn!("pdf: could not start the text thread: {error}");
        }
    }

    /// Stops reading the text, so the next search starts a new thread.
    fn stop_text(&mut self) {
        if let Some(stop) = self.text.take() {
            stop.store(true, Ordering::Release);
        }
    }

    /// Has the render thread hand back what was freed, once it is idle. A
    /// forced collection there also takes back what the finished text
    /// thread left (see [`release_freed`]).
    fn release(&self) {
        self.shared.release.store(true, Ordering::Release);
        self.shared.wake.notify_one();
    }

    /// Replaces what the render thread will do next with `jobs`, leaving out
    /// the one it is on, and takes every reply sent so far. Both under the
    /// queue's lock, which the thread holds as it replies and lets its job
    /// go, so a finished page is never asked for twice.
    fn exchange(
        &self,
        jobs: impl FnOnce(Option<Job>, &mut VecDeque<Reply>) -> Vec<Job>,
        arrived: &mut VecDeque<Reply>,
    ) {
        let mut queue = self.shared.lock();
        arrived.extend(self.replies.try_iter());
        let current = queue.current;
        queue.jobs = jobs(current, arrived);
        drop(queue);
        self.shared.wake.notify_one();
    }

    fn stop(&self) {
        self.shared.closed.store(true, Ordering::Release);
        self.shared.lock().jobs.clear();
        self.shared.wake.notify_all();
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop();
    }
}

fn open(bytes: &Arc<Bytes>) -> Result<hayro::hayro_syntax::Pdf, Failure> {
    use hayro::hayro_syntax::{LoadPdfError, Pdf};
    match Pdf::new(bytes.clone()) {
        Ok(pdf) if !pdf.pages().is_empty() => Ok(pdf),
        Ok(_) => Err(Failure::Unreadable),
        Err(LoadPdfError::Decryption(_)) => Err(Failure::Locked),
        Err(_) => Err(Failure::Unreadable),
    }
}

/// Hands the memory this thread freed, and what the window freed of the
/// pages it sent, back to the system. mimalloc keeps a thread's freed pages
/// for its next allocation, and an idle worker makes none, so a document's
/// renders (each page's pixels, hayro's scratch) would otherwise stay
/// counted against the app for as long as the viewer is open. Forced, it
/// also takes back the heaps of threads that have ended, such as the text
/// thread's once its text is let go. Does nothing where mimalloc is not the
/// allocator, as in tests.
fn release_freed() {
    // SAFETY: mi_collect only walks this thread's own heap.
    unsafe { libmimalloc_sys::mi_collect(true) };
}

fn contained<T>(work: impl FnOnce() -> T) -> Option<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).ok()
}

fn render_thread(bytes: &Arc<Bytes>, shared: &Shared, sender: &Sender<Reply>, ctx: &egui::Context) {
    use hayro::{RenderCache, RenderSettings};
    let reply = |reply: Reply| {
        let _ = sender.send(reply);
        ctx.request_repaint();
    };
    let started = Instant::now();
    let pdf = match contained(|| open(bytes)).unwrap_or(Err(Failure::Unreadable)) {
        Ok(pdf) => pdf,
        Err(failure) => {
            log::info!("pdf: could not open a document ({failure:?})");
            reply(Reply::Failed(failure));
            return;
        }
    };
    let sizes: Vec<Vec2> = pdf
        .pages()
        .iter()
        .map(|page| {
            let (width, height) = page.render_dimensions();
            vec2(width.max(1.0), height.max(1.0))
        })
        .collect();
    log::debug!(
        "pdf: opened {} pages in {} ms",
        sizes.len(),
        started.elapsed().as_millis()
    );
    reply(Reply::Opened(sizes));
    let interpreter = hayro::hayro_interpret::InterpreterSettings::default();
    let settings = RenderSettings::default();
    let mut cache = RenderCache::new();
    let mut since_fresh = 0;
    while let Some(job) = shared.next() {
        let Some(page) = pdf.pages().get(job.page) else {
            shared.lock().current = None;
            continue;
        };
        let started = Instant::now();
        let rendered =
            contained(|| rasterize(page, &cache, &interpreter, &settings, job.width, job.region))
                .flatten();
        let outcome = match rendered {
            Some((image, place)) => {
                log::debug!(
                    "pdf: page {} at {} px{} in {} ms",
                    job.page + 1,
                    job.width,
                    if job.region.is_some() {
                        ", a detail,"
                    } else {
                        ""
                    },
                    started.elapsed().as_millis()
                );
                Reply::Page { job, place, image }
            }
            None => {
                // A panic may have left the caches half written.
                cache = RenderCache::new();
                since_fresh = 0;
                Reply::PageFailed(job.page)
            }
        };
        {
            let mut queue = shared.lock();
            let _ = sender.send(outcome);
            queue.current = None;
        }
        ctx.request_repaint();
        since_fresh += 1;
        if since_fresh >= CACHE_PAGES {
            cache = RenderCache::new();
            since_fresh = 0;
        }
    }
}

/// Renders `page` scaled to `width` pixels across: all of it, or only
/// `region` (in pixels at that width). Returns the pixels and where they
/// lie on the page, in fractions of it. The pixels go to the window as
/// rendered, without a copy.
fn rasterize<'a>(
    page: &'a hayro::hayro_syntax::page::Page<'a>,
    cache: &hayro::RenderCache<'a>,
    interpreter: &hayro::hayro_interpret::InterpreterSettings,
    settings: &hayro::RenderSettings,
    width: u32,
    region: Option<Region>,
) -> Option<(egui::ColorImage, Rect)> {
    use hayro::hayro_interpret::util::TransformExt;
    use hayro::kurbo::Affine;
    use hayro::vello_cpu::{
        Pixmap, RasterizerSettings, RenderContext, Resources, TargetInit, peniko::ImageAlphaType,
    };
    let (page_width, page_height) = page.render_dimensions();
    if page_width <= 0.0 || width == 0 {
        return None;
    }
    let scale = width as f32 / page_width;
    // Whole pixels, truncated, as hayro sizes a page it renders whole.
    let full = [(page_width * scale) as u32, (page_height * scale) as u32];
    let [left, top, right, bottom] = region.unwrap_or([0, 0, full[0], full[1]]);
    let (right, bottom) = (right.min(full[0]), bottom.min(full[1]));
    let size = [right.saturating_sub(left), bottom.saturating_sub(top)];
    if size[0] == 0 || size[1] == 0 || size.iter().any(|side| *side > u32::from(u16::MAX)) {
        return None;
    }
    let mut context = RenderContext::new(size[0] as u16, size[1] as u16);
    let transform = Affine::translate((-f64::from(left), -f64::from(top)))
        * Affine::scale(f64::from(scale))
        * page.initial_transform(true).to_kurbo();
    hayro::render_into(page, cache, interpreter, settings, &mut context, transform);
    context.flush();
    let mut pixmap = Pixmap::new(context.width(), context.height());
    context.render_with(
        &mut pixmap,
        &mut Resources::default(),
        RasterizerSettings {
            target_init: TargetInit::Clear(hayro::vello_cpu::color::palette::css::WHITE),
            ..Default::default()
        },
    );
    drop(context);
    // Over an opaque white page, premultiplied and straight alpha agree.
    let rgba = pixmap.take_rgba8(ImageAlphaType::AlphaPremultiplied);
    let pixels: Vec<egui::Color32> = match bytemuck::try_cast_vec(rgba) {
        Ok(pixels) => pixels,
        Err((_, rgba)) => bytemuck::cast_slice(&rgba).to_vec(),
    };
    let image = egui::ColorImage::new([size[0] as usize, size[1] as usize], pixels);
    let (across, down) = (full[0].max(1) as f32, full[1].max(1) as f32);
    let place = Rect::from_min_max(
        pos2(left as f32 / across, top as f32 / down),
        pos2(right as f32 / across, bottom as f32 / down),
    );
    Some((image, place))
}

fn text_thread(
    bytes: &Arc<Bytes>,
    pages: usize,
    generation: u32,
    stopped: &dyn Fn() -> bool,
    sender: &Sender<Reply>,
    ctx: &egui::Context,
) {
    let Some(Ok(pdf)) = contained(|| open(bytes)) else {
        return;
    };
    let interpreter = hayro::hayro_interpret::InterpreterSettings::default();
    let started = Instant::now();
    for index in 0..pages.min(pdf.pages().len()) {
        if stopped() {
            release_freed();
            return;
        }
        let page = &pdf.pages()[index];
        let text = contained(|| page_text(page, &interpreter)).unwrap_or_default();
        let _ = sender.send(Reply::Text {
            generation,
            page: index,
            text,
        });
        // Results show as pages come in, without a repaint for every one.
        if index % 8 == 7 || index + 1 == pages {
            ctx.request_repaint();
        }
    }
    log::debug!(
        "pdf: read the text of {pages} pages in {} ms",
        started.elapsed().as_millis()
    );
    release_freed();
    ctx.request_repaint();
}

/// The glyphs a page draws, in order, each its text and its box in
/// fractions of the page. Images are never decoded: only glyph runs reach
/// the device.
fn page_text<'a>(
    page: &'a hayro::hayro_syntax::page::Page<'a>,
    interpreter: &hayro::hayro_interpret::InterpreterSettings,
) -> PageText {
    use hayro::hayro_interpret::util::TransformExt;
    use hayro::hayro_interpret::{Context, InterpreterCache, interpret_page};
    let (width, height) = page.render_dimensions();
    if width <= 0.0 || height <= 0.0 {
        return PageText::default();
    }
    let transform = page.initial_transform(true).to_kurbo();
    let cache = InterpreterCache::new();
    let mut context = Context::new(
        transform,
        hayro::kurbo::Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
        &cache,
        page.xref(),
        interpreter.clone(),
    );
    let mut device = TextDevice {
        size: vec2(width, height),
        glyphs: Vec::new(),
    };
    interpret_page(page, &mut context, &mut device);
    PageText::from_glyphs(&device.glyphs)
}

struct TextDevice {
    size: Vec2,
    glyphs: Vec<(String, Rect)>,
}

impl<'a> hayro::hayro_interpret::Device<'a> for TextDevice {
    fn draw_glyph_run(
        &mut self,
        run: &hayro::hayro_interpret::font::GlyphRun<'_, 'a>,
        props: hayro::hayro_interpret::DrawProps<'a>,
        _: &hayro::hayro_interpret::DrawMode,
    ) {
        use hayro::hayro_interpret::font::Glyph;
        use hayro::hayro_interpret::hayro_cmap::BfString;
        for glyph in run.glyphs() {
            let Some(unicode) = glyph.as_unicode() else {
                continue;
            };
            let unicode = match unicode {
                BfString::Char(c) => c.to_string(),
                BfString::String(text) => text,
            };
            let advance = match &**glyph {
                Glyph::Outline(outline) => outline.advance_width(),
                Glyph::Type3(glyph) => glyph.advance_width(),
            }
            .filter(|advance| advance.is_finite() && *advance > 0.0)
            .unwrap_or(500.0);
            // Glyph space is a thousand units to the em, from the baseline:
            // the box runs from the descender to the ascender.
            let bounds = (props.transform * glyph.transform()).transform_rect_bbox(
                hayro::kurbo::Rect::new(0.0, -220.0, f64::from(advance), 780.0),
            );
            let rect = Rect::from_min_max(
                pos2(
                    bounds.x0 as f32 / self.size.x,
                    bounds.y0 as f32 / self.size.y,
                ),
                pos2(
                    bounds.x1 as f32 / self.size.x,
                    bounds.y1 as f32 / self.size.y,
                ),
            );
            if rect.is_finite() {
                self.glyphs.push((unicode, rect));
            }
        }
    }

    fn draw_path(
        &mut self,
        _: &hayro::kurbo::BezPath,
        _: hayro::hayro_interpret::DrawProps<'a>,
        _: &hayro::hayro_interpret::DrawMode,
    ) {
    }
    fn push_clip_path(&mut self, _: &hayro::hayro_interpret::ClipPath) {}
    fn push_clip_rect(&mut self, _: &hayro::kurbo::Rect) {}
    fn push_transparency_group(
        &mut self,
        _: f32,
        _: Option<hayro::hayro_interpret::SoftMask<'a>>,
        _: hayro::hayro_interpret::BlendMode,
    ) {
    }
    fn draw_image(
        &mut self,
        _: hayro::hayro_interpret::Image<'a, '_>,
        _: hayro::hayro_interpret::ImageDrawProps<'a>,
    ) {
    }
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}
    fn draw_rect(
        &mut self,
        _: &hayro::kurbo::Rect,
        _: hayro::hayro_interpret::DrawProps<'a>,
        _: &hayro::hayro_interpret::DrawMode,
    ) {
    }
}

/// Where the pages sit for a stage width and zoom, in display points.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    /// Display points per PDF point.
    pub scale: f32,
    /// Each page's top, from the top of the content.
    pub tops: Vec<f32>,
    /// The whole content, pages and margins.
    pub size: Vec2,
    /// Room above the first page, under the header.
    pub top: f32,
}

impl Layout {
    /// The pages of `sizes` (in PDF points) one under another, the widest
    /// fitting `width` (up to [`FIT_MAX`]) at zoom 1, with `top` points kept
    /// above the first.
    pub fn new(sizes: &[Vec2], width: f32, zoom: f32, top: f32) -> Self {
        let widest = sizes.iter().map(|size| size.x).fold(1.0_f32, f32::max);
        let fit = (width - 2.0 * MARGIN).clamp(120.0, FIT_MAX);
        let scale = fit / widest * zoom;
        let mut tops = Vec::with_capacity(sizes.len());
        let mut y = top + MARGIN;
        for size in sizes {
            tops.push(y);
            y += size.y * scale + GAP;
        }
        let height = if sizes.is_empty() { y } else { y - GAP } + MARGIN;
        Self {
            scale,
            tops,
            size: vec2(width.max(widest * scale + 2.0 * MARGIN), height),
            top,
        }
    }

    /// Page `index`'s rect in content coordinates, centred across.
    pub fn page_rect(&self, sizes: &[Vec2], index: usize) -> Rect {
        let size = sizes[index] * self.scale;
        Rect::from_min_size(pos2((self.size.x - size.x) / 2.0, self.tops[index]), size)
    }

    /// The pages any part of which lies between `top` and `bottom`.
    pub fn visible(&self, sizes: &[Vec2], top: f32, bottom: f32) -> std::ops::Range<usize> {
        let first = self
            .tops
            .partition_point(|start| *start <= top)
            .saturating_sub(1);
        let mut last = first;
        while last < self.tops.len() && self.tops[last] < bottom {
            last += 1;
        }
        // The page found first may end above the view, in the gap.
        let first = if first < last && self.tops[first] + sizes[first].y * self.scale < top {
            first + 1
        } else {
            first
        };
        first..last.max(first)
    }

    /// The page at height `y`: the last one starting at or above it.
    pub fn page_at(&self, y: f32) -> usize {
        self.tops
            .partition_point(|start| *start <= y)
            .saturating_sub(1)
    }
}

/// The width in pixels for a page drawn `points` wide: a step on a scale of
/// eighth octaves (at most 9% wider than drawn, 19% more pixels), so a zoom
/// renders again only when it moves a step.
fn stepped(points: f32, pixels_per_point: f32) -> u32 {
    let wanted = (points * pixels_per_point).max(32.0);
    let step = (wanted.log2() * 8.0).ceil() / 8.0;
    2f32.powf(step).round().min(f32::from(u16::MAX) * 16.0) as u32
}

/// How a page drawn `points` wide is rendered, `aspect` its height over its
/// width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    /// The whole page's width, in pixels: as sharp as the display wants, up
    /// to `BASE_PIXELS` and the largest texture. A page that needs a
    /// detail is drawn from half that many pixels under it: it shows only
    /// for the moment a scroll takes to render the next detail.
    pub base: u32,
    /// The width the display wants. Above `base`, the part of the page on
    /// screen is rendered at this width as a detail.
    pub full: u32,
}

impl Plan {
    pub fn new(points: f32, pixels_per_point: f32, aspect: f32, max_side: usize) -> Self {
        let full = stepped(points, pixels_per_point);
        let aspect = aspect.max(f32::EPSILON);
        let max_side = max_side.min(usize::from(u16::MAX)) as f32;
        let mut base = (full as f32).min(max_side).min(max_side / aspect);
        let pixels = f64::from(base) * f64::from(base) * f64::from(aspect);
        if pixels > BASE_PIXELS as f64 {
            base = ((BASE_PIXELS as f64 / 2.0 / f64::from(aspect)).sqrt()) as f32;
        }
        Self {
            base: (base.max(1.0) as u32).min(full),
            full,
        }
    }

    /// Whether the page on screen needs a detail to be sharp.
    pub fn detailed(self) -> bool {
        self.full > self.base
    }

    /// The width a page just off the screen is rendered at ahead of time.
    fn prefetch(self) -> u32 {
        (self.base / 2).max(32)
    }
}

/// The part of a page to render as a detail: what of `page` (its rect in the
/// content, in points) lies within `view`, reaching `DETAIL_REACH` past it
/// and snapped to `DETAIL_GRID`, in pixels at `full` wide. `None` when
/// none of the page is in view.
pub fn detail_region(page: Rect, view: Rect, full: u32, aspect: f32) -> Option<Detail> {
    let shown = page.intersect(view);
    if !shown.is_positive() || !page.is_positive() {
        return None;
    }
    let across = full as f32;
    let down = (full as f32 * aspect).floor().max(1.0);
    let to_pixels = |rect: Rect| {
        let x = |value: f32| ((value - page.left()) / page.width() * across).clamp(0.0, across);
        let y = |value: f32| ((value - page.top()) / page.height() * down).clamp(0.0, down);
        [
            x(rect.left()),
            y(rect.top()),
            x(rect.right()),
            y(rect.bottom()),
        ]
    };
    let outward = |[left, top, right, bottom]: [f32; 4], grid: u32| -> Region {
        let grid = grid.max(1) as f32;
        [
            ((left / grid).floor() * grid) as u32,
            ((top / grid).floor() * grid) as u32,
            ((right / grid).ceil() * grid).min(across) as u32,
            ((bottom / grid).ceil() * grid).min(down) as u32,
        ]
    };
    let core = to_pixels(shown);
    let shown_region = outward(core, 1);
    let reach = shown.expand2(vec2(
        view.width() * DETAIL_REACH.0,
        view.height() * DETAIL_REACH.1,
    ));
    let mut render = outward(to_pixels(reach.intersect(page)), DETAIL_GRID);
    let area = |region: Region| u64::from(region[2] - region[0]) * u64::from(region[3] - region[1]);
    if area(render) > DETAIL_PIXELS {
        render = outward(core, DETAIL_GRID);
    }
    if area(render) > DETAIL_PIXELS {
        render = shown_region;
    }
    Some(Detail {
        render,
        shown: shown_region,
    })
}

/// What a detail is to cover: the part to render, and the part on screen,
/// which a detail already held must cover to be kept, both in pixels at the
/// width it is rendered at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Detail {
    pub render: Region,
    pub shown: Region,
}

/// A page the window wants: whole at `width`, or a detail of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Want {
    pub page: usize,
    pub width: u32,
    pub detail: Option<Detail>,
}

/// The document's state, as far as the window knows.
pub enum Load {
    Opening,
    Ready(Vec<Vec2>),
    Failed(Failure),
}

/// A rendered page on the GPU: the whole page, or a detail of it.
pub struct PageTexture {
    pub texture: egui::TextureHandle,
    /// The whole page's width it was rendered at, in pixels.
    pub width: u32,
    /// Where it lies on the page, in fractions of it.
    pub place: Rect,
    /// For a detail, what it covers in pixels at `width`.
    region: Option<Region>,
    bytes: usize,
}

/// The search bar's state.
#[derive(Default)]
pub struct Search {
    pub open: bool,
    pub query: String,
    /// Every hit, page by page, for what `query` held when last counted.
    pub hits: Vec<Hit>,
    /// Which of `hits` is lit as the current one.
    pub current: usize,
    /// Whether the field should take the keyboard next frame.
    pub focus: bool,
    /// The query `hits` were counted for, and how many pages had text then.
    counted: Option<(Vec<char>, usize)>,
}

/// An open PDF: the worker, what has come back from it, and how the reader
/// is looking at it. The view moves `zoom`, the search and the scroll; the
/// app opens and closes it.
pub struct Viewer {
    pub path: PathBuf,
    pub title: String,
    pub chat: ChatId,
    pub message: String,
    worker: Worker,
    pub load: Load,
    /// 1 fits the widest page to the window, up to [`FIT_MAX`].
    pub zoom: f32,
    /// When `zoom` last changed, in input time.
    zoomed_at: f64,
    /// Whole pages.
    textures: HashMap<usize, PageTexture>,
    /// The part of a page on screen, sharper than its whole page.
    details: HashMap<usize, PageTexture>,
    /// Pages that could not be rendered, not asked for again.
    failed: std::collections::HashSet<usize>,
    /// Replies taken from the worker and not yet applied.
    arrived: VecDeque<Reply>,
    text: Vec<Option<PageText>>,
    pub search: Search,
    /// A place the view should scroll to next frame: a page, and how far
    /// down it, as a fraction.
    pub scroll_to: Option<(usize, f32)>,
    /// Where the pages are scrolled, as the view last drew them.
    pub offset: Vec2,
    /// A place to scroll to once the pages are known: where this document
    /// was left the last time it was open.
    pub restore: Option<Vec2>,
    /// Until the view first draws it, so its flight starts from the message.
    pub fresh: bool,
}

impl Viewer {
    /// Maps (or reads) `path` and starts rendering it. That is the only work
    /// on this thread: a chat's PDF is already on disk.
    pub fn open(
        path: PathBuf,
        title: String,
        chat: ChatId,
        message: String,
        ctx: &egui::Context,
    ) -> std::io::Result<Self> {
        let bytes = Arc::new(Bytes::load(&path)?);
        Ok(Self::from_bytes(bytes, path, title, chat, message, ctx))
    }

    fn from_bytes(
        bytes: Arc<Bytes>,
        path: PathBuf,
        title: String,
        chat: ChatId,
        message: String,
        ctx: &egui::Context,
    ) -> Self {
        Self {
            path,
            title,
            chat,
            message,
            worker: Worker::start(bytes, ctx.clone()),
            load: Load::Opening,
            zoom: 1.0,
            zoomed_at: f64::NEG_INFINITY,
            textures: HashMap::new(),
            details: HashMap::new(),
            failed: Default::default(),
            arrived: VecDeque::new(),
            text: Vec::new(),
            search: Search::default(),
            scroll_to: None,
            offset: Vec2::ZERO,
            restore: None,
            fresh: true,
        }
    }

    /// Stops the threads, keeping the textures for the closing fade.
    pub fn stop(&self) {
        self.worker.stop();
    }

    pub fn pages(&self) -> &[Vec2] {
        match &self.load {
            Load::Ready(sizes) => sizes,
            _ => &[],
        }
    }

    pub fn texture(&self, page: usize) -> Option<&PageTexture> {
        self.textures.get(&page)
    }

    /// The sharper part of `page` on screen, when it is zoomed past what a
    /// whole page is rendered at.
    pub fn detail(&self, page: usize) -> Option<&PageTexture> {
        self.details.get(&page)
    }

    /// The bytes of every texture held, pages and details.
    pub fn texture_bytes(&self) -> usize {
        self.textures
            .values()
            .chain(self.details.values())
            .map(|held| held.bytes)
            .sum()
    }

    /// Whether `page` could not be rendered.
    pub fn page_failed(&self, page: usize) -> bool {
        self.failed.contains(&page)
    }

    /// Sets the zoom, within its limits, and notes when.
    pub fn set_zoom(&mut self, zoom: f32, now: f64) {
        let zoom = zoom.clamp(ZOOM_MIN, ZOOM_MAX);
        if (zoom - self.zoom).abs() > f32::EPSILON {
            self.zoom = zoom;
            self.zoomed_at = now;
        }
    }

    /// Whether the zoom is still moving, so pages are stretched rather than
    /// rendered again.
    pub fn zooming(&self, now: f64) -> bool {
        now - self.zoomed_at < ZOOM_SETTLE
    }

    /// One frame's exchange with the worker: takes what came back, asks for
    /// `wanted` (most wanted first) less what is held or on its way, uploads
    /// a couple of finished pages, and lets go of every page and detail not
    /// wanted. `visible` pages are never let go.
    pub fn exchange(
        &mut self,
        ctx: &egui::Context,
        wanted: &[Want],
        visible: std::ops::Range<usize>,
        now: f64,
    ) {
        let zooming = self.zooming(now);
        let textures = &self.textures;
        let details = &self.details;
        let failed = &self.failed;
        let mut arrived = std::mem::take(&mut self.arrived);
        self.worker.exchange(
            |current, arrived| {
                let coming: Vec<Job> = arrived
                    .iter()
                    .filter_map(|reply| match reply {
                        Reply::Page { job, .. } => Some(*job),
                        _ => None,
                    })
                    .chain(current)
                    .collect();
                wanted
                    .iter()
                    .filter(|want| {
                        if failed.contains(&want.page) {
                            return false;
                        }
                        match want.detail {
                            None => {
                                let on_its_way = coming.iter().any(|job| {
                                    job.page == want.page
                                        && job.width == want.width
                                        && job.region.is_none()
                                });
                                !on_its_way
                                    && textures.get(&want.page).is_none_or(|held| {
                                        // Sharp enough and at most half as wide
                                        // again as needed; while zooming,
                                        // anything held will do.
                                        !(zooming
                                            || (held.width >= want.width
                                                && u64::from(held.width) * 2
                                                    <= u64::from(want.width) * 3))
                                    })
                            }
                            // Details wait for the zoom to rest.
                            Some(_) if zooming => false,
                            Some(detail) => {
                                let enough = |width: u32, region: Region| {
                                    width == want.width && covers(region, detail.shown)
                                };
                                let on_its_way = coming.iter().any(|job| {
                                    job.page == want.page
                                        && job
                                            .region
                                            .is_some_and(|region| enough(job.width, region))
                                });
                                let held = details.get(&want.page).is_some_and(|held| {
                                    held.region.is_some_and(|region| enough(held.width, region))
                                });
                                !on_its_way && !held
                            }
                        }
                    })
                    .map(|want| Job {
                        page: want.page,
                        width: want.width,
                        region: want.detail.map(|detail| detail.render),
                    })
                    .collect()
            },
            &mut arrived,
        );
        self.arrived = arrived;
        self.apply(ctx);
        // Pages before they are known ask for nothing, and keep nothing.
        if !self.pages().is_empty() {
            self.evict(wanted, visible, zooming);
        }
    }

    fn apply(&mut self, ctx: &egui::Context) {
        let mut uploads = 0;
        while let Some(reply) = self.arrived.front() {
            if matches!(reply, Reply::Page { .. }) && uploads >= UPLOADS_PER_FRAME {
                ctx.request_repaint();
                break;
            }
            let Some(reply) = self.arrived.pop_front() else {
                break;
            };
            match reply {
                Reply::Opened(sizes) => {
                    self.text = vec![None; sizes.len()];
                    self.load = Load::Ready(sizes);
                    // Search asked for before the pages were known.
                    if self.search.open {
                        self.start_text();
                    }
                }
                Reply::Failed(failure) => self.load = Load::Failed(failure),
                Reply::Page { job, place, image } => {
                    uploads += 1;
                    // Drawn from the next frame on.
                    ctx.request_repaint();
                    let bytes = image.width() * image.height() * 4;
                    let name = match job.region {
                        Some(_) => format!("pdf-detail-{}", job.page),
                        None => format!("pdf-page-{}", job.page),
                    };
                    let texture = ctx.load_texture(name, image, egui::TextureOptions::LINEAR);
                    let held = PageTexture {
                        texture,
                        width: job.width,
                        place,
                        region: job.region,
                        bytes,
                    };
                    match job.region {
                        Some(_) => self.details.insert(job.page, held),
                        None => self.textures.insert(job.page, held),
                    };
                }
                Reply::PageFailed(page) => {
                    self.failed.insert(page);
                }
                Reply::Text {
                    generation,
                    page,
                    text,
                } => {
                    // A search closed since is not read for.
                    let current =
                        self.worker.text.is_some() && generation == self.worker.text_generation;
                    if current && let Some(slot) = self.text.get_mut(page) {
                        *slot = Some(text);
                    }
                }
            }
        }
    }

    /// Lets go of every page not wanted and not on screen, and of every
    /// detail not wanted for a page on screen (while zooming, of those off
    /// it only).
    fn evict(&mut self, wanted: &[Want], visible: std::ops::Range<usize>, zooming: bool) {
        self.textures.retain(|page, _| {
            visible.contains(page) || wanted.iter().any(|want| want.page == *page)
        });
        self.details.retain(|page, _| {
            visible.contains(page)
                && (zooming
                    || wanted
                        .iter()
                        .any(|want| want.page == *page && want.detail.is_some()))
        });
    }

    /// Starts reading the text for search, unless it is read already.
    pub fn start_text(&mut self) {
        let pages = self.pages().len();
        if pages > 0 {
            self.worker.start_text(pages);
        }
    }

    /// Closes the search and lets go of the text read for it; opening it
    /// again reads the text again.
    pub fn close_search(&mut self) {
        self.search.open = false;
        self.worker.stop_text();
        for slot in &mut self.text {
            *slot = None;
        }
        self.search.hits = Vec::new();
        self.search.current = 0;
        self.search.counted = None;
        self.worker.release();
    }

    /// How many pages' text has been read, of how many.
    pub fn text_progress(&self) -> (usize, usize) {
        (
            self.text.iter().filter(|text| text.is_some()).count(),
            self.text.len(),
        )
    }

    /// Counts the hits again when the query or the pages read have changed
    /// since last time. Returns whether they did, so the view can bring the
    /// first hit into view as the query is typed.
    pub fn recount(&mut self) -> bool {
        let query = fold_query(&self.search.query);
        let read = self.text_progress().0;
        if self.search.counted.as_ref() == Some(&(query.clone(), read)) {
            return false;
        }
        let query_changed = self
            .search
            .counted
            .as_ref()
            .is_none_or(|(previous, _)| *previous != query);
        let mut hits = Vec::new();
        for (page, text) in self.text.iter().enumerate() {
            if let Some(text) = text {
                text.find(page, &query, &mut hits);
            }
        }
        if query_changed {
            self.search.current = 0;
        } else {
            self.search.current = self.search.current.min(hits.len().saturating_sub(1));
        }
        self.search.hits = hits;
        self.search.counted = Some((query, read));
        query_changed
    }

    /// Lights the next hit (or the previous, with `back`) and scrolls to it.
    pub fn step_hit(&mut self, back: bool) {
        let count = self.search.hits.len();
        if count == 0 {
            return;
        }
        self.search.current = if back {
            (self.search.current + count - 1) % count
        } else {
            (self.search.current + 1) % count
        };
        self.reveal_current();
    }

    /// Scrolls to the current hit.
    pub fn reveal_current(&mut self) {
        if let Some(hit) = self.search.hits.get(self.search.current) {
            let top = hit.rects.first().map_or(0.0, |rect| rect.top());
            self.scroll_to = Some((hit.page, top));
        }
    }
}

/// A document card's picture of its first page.
#[derive(Clone)]
pub enum Preview {
    /// Still rendering, or waiting for a free thread.
    Pending,
    Ready(egui::TextureHandle),
    /// Not a PDF hayro reads, or too large to read for a picture.
    Failed,
}

/// Previews rendered at once, each on a short-lived thread of its own.
const PREVIEW_THREADS: usize = 2;
/// Previews kept; past this the one seen longest ago goes.
const PREVIEW_KEEP: usize = 48;
/// Passes a preview may go unseen before its texture is let go.
const PREVIEW_UNSEEN: u64 = 600;
/// Files larger than this are not opened for a picture.
const PREVIEW_MAX_BYTES: u64 = 64 << 20;

enum PreviewState {
    Queued,
    Running,
    Done(Option<egui::ColorImage>),
    Ready(egui::TextureHandle),
    Failed,
}

#[derive(Default)]
struct Previews {
    entries: HashMap<PathBuf, (PreviewState, u64)>,
    running: usize,
    swept: u64,
}

type SharedPreviews = Arc<Mutex<Previews>>;

/// The top of the first page of the PDF at `path`, `width` pixels wide and
/// `shown` times that tall (the part its card shows), for its card in the
/// chat: rendered once off the interface thread, kept while cards show it,
/// and let go once none has for a while.
pub fn preview(ctx: &egui::Context, path: &Path, width: u32, shown: f32) -> Preview {
    let pass = ctx.cumulative_pass_nr();
    let shared = ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<SharedPreviews>(egui::Id::new("pdf-previews"))
            .clone()
    });
    let mut previews = shared
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if previews.swept != pass {
        previews.swept = pass;
        previews.entries.retain(|_, (state, seen)| {
            matches!(state, PreviewState::Running) || *seen + PREVIEW_UNSEEN >= pass
        });
        while previews.entries.len() > PREVIEW_KEEP {
            let Some(oldest) = previews
                .entries
                .iter()
                .filter(|(_, (state, _))| !matches!(state, PreviewState::Running))
                .min_by_key(|(_, (_, seen))| *seen)
                .map(|(path, _)| path.clone())
            else {
                break;
            };
            previews.entries.remove(&oldest);
        }
    }
    let running = previews.running;
    let (state, seen) = previews
        .entries
        .entry(path.to_owned())
        .or_insert((PreviewState::Queued, pass));
    *seen = pass;
    let (preview, start) = match state {
        PreviewState::Queued if running < PREVIEW_THREADS => {
            *state = PreviewState::Running;
            (Preview::Pending, true)
        }
        PreviewState::Queued | PreviewState::Running => (Preview::Pending, false),
        PreviewState::Done(image) => match image.take() {
            Some(image) => {
                let texture = ctx.load_texture(
                    format!("pdf-preview-{}", path.display()),
                    image,
                    egui::TextureOptions::LINEAR,
                );
                *state = PreviewState::Ready(texture.clone());
                (Preview::Ready(texture), false)
            }
            None => {
                *state = PreviewState::Failed;
                (Preview::Failed, false)
            }
        },
        PreviewState::Ready(texture) => (Preview::Ready(texture.clone()), false),
        PreviewState::Failed => (Preview::Failed, false),
    };
    if start {
        previews.running += 1;
        let owned = path.to_owned();
        let shared = shared.clone();
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("pdf-render".into())
            .stack_size(STACK)
            .spawn(move || {
                let image = contained(|| first_page(&owned, width, shown)).flatten();
                release_freed();
                let mut previews = shared
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                previews.running -= 1;
                if let Some((state, _)) = previews.entries.get_mut(&owned) {
                    *state = PreviewState::Done(image);
                }
                drop(previews);
                ctx.request_repaint();
            });
        if spawned.is_err() {
            previews.running -= 1;
            if let Some((state, _)) = previews.entries.get_mut(path) {
                *state = PreviewState::Failed;
            }
        }
    }
    preview
}

/// The top of the first page, `width` pixels wide and `shown` times that
/// tall at most: only what the card shows is rendered and kept.
fn first_page(path: &Path, width: u32, shown: f32) -> Option<egui::ColorImage> {
    if std::fs::metadata(path).ok()?.len() > PREVIEW_MAX_BYTES {
        return None;
    }
    let bytes = Arc::new(Bytes::load(path).ok()?);
    let pdf = open(&bytes).ok()?;
    let cache = hayro::RenderCache::new();
    let height = (width as f32 * shown.max(0.0)).ceil() as u32;
    rasterize(
        &pdf.pages()[0],
        &cache,
        &hayro::hayro_interpret::InterpreterSettings::default(),
        &hayro::RenderSettings::default(),
        width,
        Some([0, 0, width, height.max(1)]),
    )
    .map(|(image, _)| image)
}

/// The pages to ask for whole: those on screen nearest the middle first, at
/// their plan's base width, then `PREFETCH` each way at half that.
pub fn wanted_pages(
    visible: std::ops::Range<usize>,
    count: usize,
    middle: usize,
    plan_of: impl Fn(usize) -> Plan,
) -> Vec<Want> {
    let mut pages: Vec<usize> = visible.clone().collect();
    pages.sort_by_key(|page| page.abs_diff(middle));
    let mut wanted: Vec<Want> = pages
        .into_iter()
        .map(|page| Want {
            page,
            width: plan_of(page).base,
            detail: None,
        })
        .collect();
    for step in 1..=PREFETCH {
        let after = visible
            .end
            .checked_add(step - 1)
            .filter(|page| *page < count);
        let before = visible.start.checked_sub(step);
        for page in after.into_iter().chain(before) {
            if !wanted.iter().any(|want| want.page == page) {
                wanted.push(Want {
                    page,
                    width: plan_of(page).prefetch(),
                    detail: None,
                });
            }
        }
    }
    wanted
}

/// A PDF of `pages`, each a content stream drawn on a US Letter page with
/// Helvetica as `/F1`, Helvetica-Bold as `/F2` and Times as `/F3`, its
/// cross-reference table at the right offsets. For tests and the demo.
#[cfg(any(test, feature = "demo"))]
fn write_pdf(pages: &[String]) -> Vec<u8> {
    let pages: Vec<(String, Option<Picture>)> =
        pages.iter().map(|page| (page.clone(), None)).collect();
    write_pdf_with(&pages)
}

/// An uncompressed RGB picture a page draws as `/Im1`.
#[cfg(any(test, feature = "demo"))]
struct Picture {
    width: u32,
    height: u32,
    rgb: Vec<u8>,
}

/// [`write_pdf`], with a picture of its own on any page.
#[cfg(any(test, feature = "demo"))]
fn write_pdf_with(pages: &[(String, Option<Picture>)]) -> Vec<u8> {
    // Each page's object, then its contents, then its picture if it has one.
    let mut firsts = Vec::with_capacity(pages.len());
    let mut next = 3;
    for (_, picture) in pages {
        firsts.push(next);
        next += if picture.is_some() { 3 } else { 2 };
    }
    let fonts = next;
    let kids: Vec<String> = firsts.iter().map(|first| format!("{first} 0 R")).collect();
    let mut objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids.join(" "),
            pages.len()
        )
        .into_bytes(),
    ];
    for ((content, picture), first) in pages.iter().zip(&firsts) {
        let pictures = if picture.is_some() {
            format!(" /XObject << /Im1 {} 0 R >>", first + 2)
        } else {
            String::new()
        };
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents {} 0 R \
                 /Resources << /Font << /F1 {fonts} 0 R /F2 {} 0 R /F3 {} 0 R >>{pictures} >> >>",
                first + 1,
                fonts + 1,
                fonts + 2,
            )
            .into_bytes(),
        );
        objects.push(
            format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            )
            .into_bytes(),
        );
        if let Some(picture) = picture {
            let mut object = format!(
                "<< /Type /XObject /Subtype /Image /Width {} /Height {} \
                 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length {} >>\nstream\n",
                picture.width,
                picture.height,
                picture.rgb.len()
            )
            .into_bytes();
            object.extend_from_slice(&picture.rgb);
            object.extend_from_slice(b"\nendstream");
            objects.push(object);
        }
    }
    for font in ["Helvetica", "Helvetica-Bold", "Times-Roman"] {
        objects.push(format!("<< /Type /Font /Subtype /Type1 /BaseFont /{font} >>").into_bytes());
    }
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        pdf.extend_from_slice(object);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    pdf
}

/// The demo's document: a few pages of a made-up report, a heading, an
/// accent rule and paragraphs, so the viewer and its search have something
/// to show that is nobody's.
#[cfg(any(test, feature = "demo"))]
pub fn demo_document() -> Vec<u8> {
    let paragraph = [
        "The orbital workshop met every milestone this quarter. Crews logged",
        "two hundred hours of experiments, and the new solar arrays delivered",
        "more power than the design asked for. Ground teams shortened the",
        "turnaround between missions by nine days.",
        "",
        "Supply flights carried fresh water, spare filters and a replacement",
        "pump for the cooling loop. The pump was installed on the second day",
        "and has run without a fault since. Telemetry shows the cabin holding",
        "a steady twenty-one degrees through every pass into shadow.",
        "",
        "Next quarter the workshop adds a second docking port and begins the",
        "long-duration plant growth study. Budget and schedule remain on track.",
    ];
    let titles = [
        ("Quarterly mission report", "Summary of operations"),
        ("Experiments", "Results from the science racks"),
        ("Outlook", "What comes next for the workshop"),
    ];
    let pages: Vec<String> = titles
        .iter()
        .map(|(title, subtitle)| {
            let mut content = String::new();
            content.push_str("0.16 0.47 0.96 rg 72 690 468 3 re f 0 g ");
            content.push_str(&format!("BT /F2 26 Tf 72 712 Td ({title}) Tj ET "));
            content.push_str(&format!(
                "0.4 g BT /F1 13 Tf 72 664 Td ({subtitle}) Tj ET 0 g "
            ));
            content.push_str("BT /F3 12 Tf 17 TL 72 628 Td ");
            for _ in 0..2 {
                for line in paragraph {
                    content.push_str(&format!("({line}) Tj T* "));
                }
                content.push_str("T* ");
            }
            content.push_str("ET ");
            content.push_str("0.93 g 72 96 468 64 re f 0.3 g ");
            content.push_str(
                "BT /F1 10 Tf 88 132 Td (Synthetic document made for the Zeus demo.) Tj ET",
            );
            content
        })
        .collect();
    write_pdf(&pages)
}

/// The memory benchmark's document (`--demo-tour-script pdf`): `pages`
/// pages, each a heading, a picture of its own and two paragraphs, so the
/// viewer renders, decodes and searches as it would a real report. Every
/// picture differs, and none is compressed, so the file is a few megabytes.
#[cfg(any(test, feature = "demo"))]
pub fn bench_document(pages: usize) -> Vec<u8> {
    const WIDTH: u32 = 320;
    const HEIGHT: u32 = 200;
    let lines = [
        "Supply flights carried fresh water, spare filters and a replacement",
        "pump for the cooling loop. The pump was installed on the second day",
        "and has run without a fault since. Telemetry shows the cabin holding",
        "a steady twenty-one degrees through every pass into shadow.",
        "",
        "Next quarter the workshop adds a second docking port and begins the",
        "long-duration plant growth study. Budget and schedule remain on track.",
    ];
    let pages: Vec<(String, Option<Picture>)> = (0..pages)
        .map(|index| {
            let mut content = String::new();
            content.push_str("0.16 0.47 0.96 rg 72 690 468 3 re f 0 g ");
            content.push_str(&format!(
                "BT /F2 24 Tf 72 712 Td (Section {}) Tj ET ",
                index + 1
            ));
            content.push_str("q 468 0 0 292 72 372 cm /Im1 Do Q ");
            content.push_str("BT /F3 12 Tf 17 TL 72 340 Td ");
            for _ in 0..2 {
                for line in lines {
                    content.push_str(&format!("({line}) Tj T* "));
                }
                content.push_str("T* ");
            }
            content.push_str("ET ");
            content.push_str(
                "0.3 g BT /F1 10 Tf 72 60 Td (Synthetic document made for the Zeus benchmark.) Tj ET",
            );
            let mut rgb = Vec::with_capacity((WIDTH * HEIGHT * 3) as usize);
            let tint = (index * 37 % 256) as u8;
            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    rgb.extend_from_slice(&[
                        (x * 255 / WIDTH) as u8,
                        (y * 255 / HEIGHT) as u8,
                        tint ^ ((x / 16 + y / 16) % 2 * 64) as u8,
                    ]);
                }
            }
            let picture = Picture {
                width: WIDTH,
                height: HEIGHT,
                rgb,
            };
            (content, Some(picture))
        })
        .collect();
    write_pdf_with(&pages)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-page PDF in Helvetica with `lines` set 24 points tall, the
    /// first 72 points in and 92 down, each next one 30 lower.
    pub(crate) fn sample(lines: &[&str]) -> Vec<u8> {
        let mut content = String::from("BT /F1 24 Tf 72 700 Td ");
        for (index, line) in lines.iter().enumerate() {
            if index > 0 {
                content.push_str("0 -30 Td ");
            }
            content.push_str(&format!("({line}) Tj "));
        }
        content.push_str("ET");
        write_pdf(&[content])
    }

    fn text_of(bytes: Vec<u8>) -> PageText {
        let bytes = Arc::new(Bytes::Read(bytes));
        let pdf = open(&bytes).expect("the sample opens");
        let interpreter = hayro::hayro_interpret::InterpreterSettings::default();
        page_text(&pdf.pages()[0], &interpreter)
    }

    #[test]
    fn the_header_is_found_after_junk_but_not_elsewhere() {
        assert!(has_header(b"%PDF-1.7\n"));
        assert!(has_header(b"\xEF\xBB\xBF junk %PDF-1.4"));
        assert!(!has_header(b"PK\x03\x04 not a pdf"));
        assert!(!has_header(b"%PD"));
    }

    #[test]
    fn a_sample_page_renders_white_with_ink() {
        let bytes = Arc::new(Bytes::Read(sample(&["Hello world"])));
        let pdf = open(&bytes).expect("the sample opens");
        let page = &pdf.pages()[0];
        assert_eq!(page.render_dimensions(), (612.0, 792.0));
        let cache = hayro::RenderCache::new();
        let (image, place) = rasterize(
            page,
            &cache,
            &hayro::hayro_interpret::InterpreterSettings::default(),
            &hayro::RenderSettings::default(),
            306,
            None,
        )
        .expect("it renders");
        assert_eq!(image.size, [306, 396]);
        assert_eq!(place, Rect::from_min_max(egui::Pos2::ZERO, pos2(1.0, 1.0)));
        assert_eq!(image.pixels[0], egui::Color32::WHITE);
        assert!(
            image.pixels.iter().any(|pixel| pixel.r() < 128),
            "the text leaves dark pixels"
        );
    }

    #[test]
    fn the_bench_document_draws_its_pictures_and_has_text_to_find() {
        let bytes = Arc::new(Bytes::Read(bench_document(3)));
        let pdf = open(&bytes).expect("the bench document opens");
        assert_eq!(pdf.pages().len(), 3);
        let cache = hayro::RenderCache::new();
        let (image, _) = rasterize(
            &pdf.pages()[1],
            &cache,
            &hayro::hayro_interpret::InterpreterSettings::default(),
            &hayro::RenderSettings::default(),
            306,
            None,
        )
        .expect("it renders");
        // The picture fills the page's middle with colour.
        let middle = image.pixels[image.size[0] * image.size[1] / 3 + image.size[0] / 2];
        assert_ne!(middle, egui::Color32::WHITE, "the picture is drawn");
        let interpreter = hayro::hayro_interpret::InterpreterSettings::default();
        let mut hits = Vec::new();
        page_text(&pdf.pages()[2], &interpreter).find(2, &fold_query("pump"), &mut hits);
        assert_eq!(hits.len(), 4, "twice in each of two paragraphs");
    }

    #[test]
    fn something_that_is_not_a_pdf_is_unreadable() {
        let bytes = Arc::new(Bytes::Read(b"%PDF-1.4 and nothing else".to_vec()));
        assert_eq!(open(&bytes).err(), Some(Failure::Unreadable));
    }

    #[test]
    fn search_finds_words_across_lines_case_and_accents_aside() {
        let text = text_of(sample(&["Hello World", "Cafe creme"]));
        let mut hits = Vec::new();
        text.find(0, &fold_query("hello"), &mut hits);
        assert_eq!(hits.len(), 1);
        let rect = hits[0].rects[0];
        // Near the top left, where the first line was set: 72 points in,
        // 92 down, on a 612 by 792 page.
        assert!(rect.left() > 0.10 && rect.left() < 0.13, "{rect:?}");
        assert!(rect.center().y > 0.08 && rect.center().y < 0.13, "{rect:?}");
        hits.clear();
        text.find(0, &fold_query("  WORLD   café "), &mut hits);
        assert_eq!(hits.len(), 1, "a line break reads as a space");
        assert_eq!(hits[0].rects.len(), 2, "one box per line");
        hits.clear();
        text.find(0, &fold_query("missing"), &mut hits);
        assert!(hits.is_empty());
    }

    #[test]
    fn spaces_are_inferred_between_words_set_apart() {
        let glyph = |text: &str, x: f32| {
            (
                text.to_owned(),
                Rect::from_min_size(pos2(x, 0.1), vec2(0.01, 0.02)),
            )
        };
        let text = PageText::from_glyphs(&[glyph("a", 0.10), glyph("b", 0.11), glyph("c", 0.20)]);
        assert_eq!(text.chars.iter().collect::<String>(), "ab c");
        let mut hits = Vec::new();
        text.find(0, &fold_query("ab c"), &mut hits);
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn a_ligature_glyph_matches_its_letters() {
        let glyph = |text: &str, x: f32| {
            (
                text.to_owned(),
                Rect::from_min_size(pos2(x, 0.1), vec2(0.01, 0.02)),
            )
        };
        let text =
            PageText::from_glyphs(&[glyph("o", 0.10), glyph("ffi", 0.11), glyph("ce", 0.12)]);
        let mut hits = Vec::new();
        text.find(0, &fold_query("office"), &mut hits);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].rects.len(), 1);
    }

    #[test]
    fn layout_stacks_pages_and_finds_the_visible_ones() {
        let sizes = [vec2(600.0, 800.0), vec2(600.0, 800.0), vec2(300.0, 400.0)];
        let layout = Layout::new(&sizes, 648.0, 1.0, 56.0);
        assert_eq!(layout.scale, 1.0);
        assert_eq!(layout.tops[0], 56.0 + MARGIN);
        assert_eq!(layout.tops[1], layout.tops[0] + 800.0 + GAP);
        assert_eq!(layout.page_rect(&sizes, 2).width(), 300.0);
        assert_eq!(layout.page_rect(&sizes, 2).center().x, layout.size.x / 2.0);
        assert_eq!(layout.visible(&sizes, 0.0, 100.0), 0..1);
        assert_eq!(
            layout.visible(&sizes, layout.tops[1] - 5.0, layout.tops[1] + 10.0),
            1..2
        );
        assert_eq!(layout.visible(&sizes, 0.0, 10_000.0), 0..3);
        assert_eq!(layout.page_at(layout.tops[2] + 1.0), 2);
        // Zoomed in, the content is wider than the stage.
        let zoomed = Layout::new(&sizes, 648.0, 2.0, 56.0);
        assert_eq!(zoomed.size.x, 1200.0 + 2.0 * MARGIN);
    }

    #[test]
    fn plans_step_and_cap_the_whole_page() {
        let plan = Plan::new(500.0, 1.5, 1.3, 8192);
        assert!(plan.base >= 750);
        assert_eq!(plan.base, plan.full, "a page this small is rendered whole");
        assert_eq!(
            Plan::new(510.0, 1.5, 1.3, 8192),
            plan,
            "a small zoom keeps the step"
        );
        // US Letter and A4 pages at the fit zoom of a 2x display are whole.
        for aspect in [792.0 / 612.0, 842.0 / 595.0] {
            let fit = Plan::new(FIT_MAX, 2.0, aspect, 16_384);
            assert!(!fit.detailed(), "{fit:?}");
        }
        // Deep zoom: the whole page stays within its pixels, the rest is a detail.
        let deep = Plan::new(FIT_MAX * 5.0, 2.0, 792.0 / 612.0, 16_384);
        assert!(deep.detailed());
        assert!(u64::from(deep.base).pow(2) * 792 / 612 <= BASE_PIXELS / 2);
        assert!(deep.full >= 9000);
        assert!(Plan::new(20_000.0, 2.0, 1.3, 8192).base <= 8192);
    }

    #[test]
    fn a_detail_covers_the_view_with_room_to_scroll_and_stays_put_for_small_ones() {
        // A page 1000 points wide and 1300 tall, rendered 4000 pixels wide;
        // the view a 600 by 400 window into its middle.
        let page = Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 1300.0));
        let view = Rect::from_min_size(pos2(200.0, 400.0), vec2(600.0, 400.0));
        let detail = detail_region(page, view, 4000, 1.3).expect("the page is in view");
        assert_eq!(detail.shown, [800, 1600, 3200, 3200]);
        assert!(covers(detail.render, detail.shown));
        assert!(detail.render[1] < detail.shown[1] && detail.render[3] > detail.shown[3]);
        assert!(detail.render.iter().all(|edge| edge % DETAIL_GRID == 0));
        let nudged = detail_region(page, view.translate(vec2(0.0, 20.0)), 4000, 1.3).unwrap();
        assert!(
            covers(detail.render, nudged.shown),
            "a small scroll needs no new detail"
        );
        assert_eq!(
            detail_region(page, view.translate(vec2(0.0, 2000.0)), 4000, 1.3),
            None
        );
    }

    #[test]
    fn a_detail_renders_only_its_part_of_the_page() {
        let bytes = Arc::new(Bytes::Read(sample(&["Hello world"])));
        let pdf = open(&bytes).expect("the sample opens");
        let cache = hayro::RenderCache::new();
        let settings = hayro::hayro_interpret::InterpreterSettings::default();
        let render = hayro::RenderSettings::default();
        let (whole, _) = rasterize(&pdf.pages()[0], &cache, &settings, &render, 612, None).unwrap();
        let (part, place) = rasterize(
            &pdf.pages()[0],
            &cache,
            &settings,
            &render,
            612,
            Some([50, 70, 350, 130]),
        )
        .unwrap();
        assert_eq!(part.size, [300, 60]);
        assert!((place.left() - 50.0 / 612.0).abs() < 1e-6);
        assert!((place.bottom() - 130.0 / 792.0).abs() < 1e-6);
        // The same pixels as the whole page has there.
        for (x, y) in [(0, 0), (40, 30), (299, 59), (120, 20)] {
            assert_eq!(part[(x, y)], whole[(x + 50, y + 70)], "at {x}, {y}");
        }
    }

    #[test]
    fn wanted_pages_start_in_the_middle_and_prefetch_both_ways_at_half_width() {
        let plan = |_| Plan {
            base: 100,
            full: 100,
        };
        let wanted = wanted_pages(3..6, 10, 5, plan);
        let pages: Vec<usize> = wanted.iter().map(|want| want.page).collect();
        assert_eq!(pages, vec![5, 4, 3, 6, 2, 7, 1]);
        let widths: Vec<u32> = wanted.iter().map(|want| want.width).collect();
        assert_eq!(widths, vec![100, 100, 100, 50, 50, 50, 50]);
        let start = wanted_pages(0..1, 2, 0, plan);
        assert_eq!(
            start.iter().map(|want| want.page).collect::<Vec<_>>(),
            vec![0, 1]
        );
    }

    #[test]
    fn packed_glyph_boxes_keep_their_place() {
        let rect = Rect::from_min_max(pos2(0.1, 0.2), pos2(0.35, 0.2125));
        let back = unpack(pack(rect));
        assert!((back.min - rect.min).length() < 1e-4 && (back.max - rect.max).length() < 1e-4);
        assert_eq!(
            unpack(pack(Rect::from_min_max(pos2(-0.1, 0.0), pos2(1.2, 1.0))))
                .max
                .x,
            1.0
        );
    }

    #[test]
    fn a_viewer_opens_renders_and_searches_in_the_background() {
        let ctx = egui::Context::default();
        let mut viewer = Viewer::from_bytes(
            Arc::new(Bytes::Read(sample(&["Synthetic invoice", "Total due"]))),
            PathBuf::from("sample.pdf"),
            "sample.pdf".into(),
            "chat".into(),
            "message".into(),
            &ctx,
        );
        let deadline = Instant::now() + std::time::Duration::from_secs(20);
        while viewer.texture(0).is_none() && Instant::now() < deadline {
            let wanted = if viewer.pages().is_empty() {
                vec![]
            } else {
                vec![Want {
                    page: 0,
                    width: 200,
                    detail: None,
                }]
            };
            viewer.exchange(&ctx, &wanted, 0..1, 1.0);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(viewer.pages(), &[vec2(612.0, 792.0)]);
        assert_eq!(viewer.texture(0).map(|page| page.width), Some(200));
        viewer.search.query = "total".into();
        viewer.start_text();
        while viewer.text_progress().0 < 1 && Instant::now() < deadline {
            viewer.exchange(&ctx, &[], 0..1, 1.0);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(viewer.recount());
        assert_eq!(viewer.search.hits.len(), 1);
        assert!(!viewer.recount(), "nothing changed since");
        // Closing the search lets the text go; opening it reads it again.
        viewer.close_search();
        assert_eq!(viewer.text_progress(), (0, 1));
        assert!(viewer.search.hits.is_empty());
        viewer.search.open = true;
        viewer.start_text();
        while viewer.text_progress().0 < 1 && Instant::now() < deadline {
            viewer.exchange(&ctx, &[], 0..1, 1.0);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        viewer.recount();
        assert_eq!(viewer.search.hits.len(), 1);
    }

    /// The viewer keeps only what it was last asked for, and pages on screen:
    /// a page scrolled far off goes, and a detail goes with its page.
    #[test]
    fn pages_and_details_not_wanted_are_let_go() {
        let ctx = egui::Context::default();
        let mut viewer = Viewer::from_bytes(
            Arc::new(Bytes::Read(bench_document(3))),
            PathBuf::from("bench.pdf"),
            "bench.pdf".into(),
            "chat".into(),
            "message".into(),
            &ctx,
        );
        let whole = |page| Want {
            page,
            width: 120,
            detail: None,
        };
        let detail = Want {
            page: 1,
            width: 1224,
            detail: Some(Detail {
                render: [0, 0, 256, 256],
                shown: [0, 0, 200, 200],
            }),
        };
        let wanted = [whole(1), detail, whole(0), whole(2)];
        let deadline = Instant::now() + std::time::Duration::from_secs(30);
        while (viewer.detail(1).is_none() || (0..3).any(|page| viewer.texture(page).is_none()))
            && Instant::now() < deadline
        {
            let asked: &[Want] = if viewer.pages().is_empty() {
                &[]
            } else {
                &wanted
            };
            viewer.exchange(&ctx, asked, 1..2, 1.0);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let held = viewer.detail(1).expect("the detail arrived");
        assert_eq!(held.texture.size(), [256, 256]);
        assert!(held.place.width() > 0.0 && held.place.max.x < 0.25);
        // Scrolled on: page 0 and the detail are no longer wanted.
        viewer.exchange(&ctx, &[whole(2), whole(1)], 2..3, 1.0);
        assert!(viewer.texture(0).is_none());
        assert!(viewer.detail(1).is_none());
        assert!(viewer.texture(1).is_some() && viewer.texture(2).is_some());
        assert_eq!(
            viewer.texture_bytes(),
            [1, 2]
                .map(|page| viewer.texture(page).unwrap().texture.byte_size())
                .iter()
                .sum::<usize>()
        );
    }

    #[test]
    fn a_locked_or_broken_file_fails_without_pages() {
        let ctx = egui::Context::default();
        let mut viewer = Viewer::from_bytes(
            Arc::new(Bytes::Read(b"not a pdf at all".to_vec())),
            PathBuf::from("broken.pdf"),
            "broken.pdf".into(),
            "chat".into(),
            "message".into(),
            &ctx,
        );
        let deadline = Instant::now() + std::time::Duration::from_secs(20);
        while matches!(viewer.load, Load::Opening) && Instant::now() < deadline {
            viewer.exchange(&ctx, &[], 0..0, 1.0);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(matches!(viewer.load, Load::Failed(Failure::Unreadable)));
    }
}
