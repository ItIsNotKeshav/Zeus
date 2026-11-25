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
//! couple per frame and keeps under `BUDGET`, dropping the pages farthest
//! from the one being read. Everything is let go when the viewer closes.

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
/// Texture bytes one viewer keeps. Pages on screen stay whatever this says.
const BUDGET: usize = 96 << 20;
/// How long the render thread waits with nothing to do before it hands back
/// what it freed: by then the window has uploaded and dropped its pages.
const IDLE: std::time::Duration = std::time::Duration::from_millis(300);
/// The most pixels one page is rendered with: past that a deep zoom
/// stretches the page instead of holding a texture of hundreds of megabytes.
const MAX_PIXELS: u64 = 24 << 20;
/// Pages rendered ahead of the view, each way.
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
    boxes: Vec<Rect>,
}

const NONE: u32 = u32::MAX;

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
            text.boxes.push(*rect);
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
            let rect = self.boxes[owner as usize];
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

/// A page to render, at a width in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Job {
    page: usize,
    width: u32,
}

enum Reply {
    Opened(Vec<Vec2>),
    Failed(Failure),
    Page {
        page: usize,
        width: u32,
        image: egui::ColorImage,
    },
    PageFailed(usize),
    Text(usize, PageText),
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

/// The threads behind one open document.
struct Worker {
    bytes: Arc<Vec<u8>>,
    shared: Arc<Shared>,
    replies: Receiver<Reply>,
    sender: Sender<Reply>,
    ctx: egui::Context,
    text_started: bool,
}

impl Worker {
    fn start(bytes: Arc<Vec<u8>>, ctx: egui::Context) -> Self {
        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue::default()),
            wake: Condvar::new(),
            closed: AtomicBool::new(false),
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
            text_started: false,
        }
    }

    fn start_text(&mut self, pages: usize) {
        if self.text_started {
            return;
        }
        self.text_started = true;
        let bytes = self.bytes.clone();
        let shared = self.shared.clone();
        let sender = self.sender.clone();
        let ctx = self.ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("pdf-text".into())
            .stack_size(STACK)
            .spawn(move || text_thread(&bytes, pages, &shared, &sender, &ctx));
        if let Err(error) = spawned {
            log::warn!("pdf: could not start the text thread: {error}");
        }
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

fn open(bytes: &Arc<Vec<u8>>) -> Result<hayro::hayro_syntax::Pdf, Failure> {
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
/// counted against the app for as long as the viewer is open. Does nothing
/// where mimalloc is not the allocator, as in tests.
fn release_freed() {
    // SAFETY: mi_collect only walks this thread's own heap.
    unsafe { libmimalloc_sys::mi_collect(true) };
}

fn contained<T>(work: impl FnOnce() -> T) -> Option<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).ok()
}

fn render_thread(
    bytes: &Arc<Vec<u8>>,
    shared: &Shared,
    sender: &Sender<Reply>,
    ctx: &egui::Context,
) {
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
        let image =
            contained(|| rasterize(page, &cache, &interpreter, &settings, job.width)).flatten();
        let outcome = match image {
            Some(image) => {
                log::debug!(
                    "pdf: page {} at {} px in {} ms",
                    job.page + 1,
                    job.width,
                    started.elapsed().as_millis()
                );
                Reply::Page {
                    page: job.page,
                    width: job.width,
                    image,
                }
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

fn rasterize<'a>(
    page: &'a hayro::hayro_syntax::page::Page<'a>,
    cache: &hayro::RenderCache<'a>,
    interpreter: &hayro::hayro_interpret::InterpreterSettings,
    settings: &hayro::RenderSettings,
    width: u32,
) -> Option<egui::ColorImage> {
    let (page_width, _) = page.render_dimensions();
    if page_width <= 0.0 || width == 0 {
        return None;
    }
    let scale = width as f32 / page_width;
    let pixmap = hayro::render(
        page,
        cache,
        interpreter,
        settings,
        &hayro::PixmapSettings {
            x_scale: scale,
            y_scale: scale,
            bg_color: hayro::vello_cpu::color::palette::css::WHITE,
        },
    );
    let size = [usize::from(pixmap.width()), usize::from(pixmap.height())];
    if size[0] == 0 || size[1] == 0 {
        return None;
    }
    // Over an opaque white page, premultiplied and straight alpha agree.
    Some(egui::ColorImage::from_rgba_premultiplied(
        size,
        pixmap.data_as_u8_slice(),
    ))
}

fn text_thread(
    bytes: &Arc<Vec<u8>>,
    pages: usize,
    shared: &Shared,
    sender: &Sender<Reply>,
    ctx: &egui::Context,
) {
    let Some(Ok(pdf)) = contained(|| open(bytes)) else {
        return;
    };
    let interpreter = hayro::hayro_interpret::InterpreterSettings::default();
    let started = Instant::now();
    for index in 0..pages.min(pdf.pages().len()) {
        if shared.closed.load(Ordering::Acquire) {
            return;
        }
        let page = &pdf.pages()[index];
        let text = contained(|| page_text(page, &interpreter)).unwrap_or_default();
        let _ = sender.send(Reply::Text(index, text));
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

/// The width in pixels to render a page drawn `points` wide: a step on a
/// scale of eighth octaves (at most 9% wider than drawn, 19% more pixels),
/// so a zoom renders again only when it moves a
/// step, and within the texture and pixel limits.
pub fn render_width(points: f32, pixels_per_point: f32, aspect: f32, max_side: usize) -> u32 {
    let wanted = (points * pixels_per_point).max(32.0);
    let step = (wanted.log2() * 8.0).ceil() / 8.0;
    let mut width = 2f32.powf(step).round();
    let max_side = max_side.min(usize::from(u16::MAX)) as f32;
    width = width.min(max_side).min(max_side / aspect.max(f32::EPSILON));
    let pixels = f64::from(width) * f64::from(width) * f64::from(aspect);
    if pixels > MAX_PIXELS as f64 {
        width = ((MAX_PIXELS as f64 / f64::from(aspect)).sqrt()) as f32;
    }
    width.max(1.0) as u32
}

/// The document's state, as far as the window knows.
pub enum Load {
    Opening,
    Ready(Vec<Vec2>),
    Failed(Failure),
}

/// A rendered page on the GPU.
pub struct PageTexture {
    pub texture: egui::TextureHandle,
    /// The width it was rendered at, in pixels.
    pub width: u32,
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
    textures: HashMap<usize, PageTexture>,
    bytes: usize,
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
    /// Reads `path` and starts rendering it. The read is the only work on
    /// this thread: a chat's PDF is already on disk, and even a large one
    /// reads in milliseconds.
    pub fn open(
        path: PathBuf,
        title: String,
        chat: ChatId,
        message: String,
        ctx: &egui::Context,
    ) -> std::io::Result<Self> {
        let bytes = Arc::new(std::fs::read(&path)?);
        Ok(Self::from_bytes(bytes, path, title, chat, message, ctx))
    }

    fn from_bytes(
        bytes: Arc<Vec<u8>>,
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
            bytes: 0,
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
    /// `wanted` (pages and widths, most wanted first) less what is held or
    /// on its way, and uploads a couple of finished pages. `visible` pages
    /// are never let go to stay under budget; `center` is the page read.
    pub fn exchange(
        &mut self,
        ctx: &egui::Context,
        wanted: &[(usize, u32)],
        visible: std::ops::Range<usize>,
        center: usize,
        now: f64,
    ) {
        let zooming = self.zooming(now);
        let textures = &self.textures;
        let failed = &self.failed;
        let mut arrived = std::mem::take(&mut self.arrived);
        self.worker.exchange(
            |current, arrived| {
                let coming: Vec<(usize, u32)> = arrived
                    .iter()
                    .filter_map(|reply| match reply {
                        Reply::Page { page, width, .. } => Some((*page, *width)),
                        _ => None,
                    })
                    .chain(current.map(|job| (job.page, job.width)))
                    .collect();
                wanted
                    .iter()
                    .filter(|(page, width)| {
                        !failed.contains(page)
                            && !coming.iter().any(|(p, w)| p == page && w == width)
                            && match textures.get(page) {
                                // Sharp enough and not far larger than needed;
                                // while zooming, anything held will do.
                                Some(held) => {
                                    !(zooming || (held.width >= *width && held.width <= width * 2))
                                }
                                None => true,
                            }
                    })
                    .map(|&(page, width)| Job { page, width })
                    .collect()
            },
            &mut arrived,
        );
        self.arrived = arrived;
        self.apply(ctx);
        self.evict(visible, center);
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
                Reply::Page { page, width, image } => {
                    uploads += 1;
                    // Drawn from the next frame on.
                    ctx.request_repaint();
                    let bytes = image.width() * image.height() * 4;
                    let texture = ctx.load_texture(
                        format!("pdf-page-{page}"),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    if let Some(old) = self.textures.insert(
                        page,
                        PageTexture {
                            texture,
                            width,
                            bytes,
                        },
                    ) {
                        self.bytes -= old.bytes;
                    }
                    self.bytes += bytes;
                }
                Reply::PageFailed(page) => {
                    self.failed.insert(page);
                }
                Reply::Text(page, text) => {
                    if let Some(slot) = self.text.get_mut(page) {
                        *slot = Some(text);
                    }
                }
            }
        }
    }

    /// Lets go of the pages farthest from `center` until the textures fit
    /// the budget, never one on screen.
    fn evict(&mut self, visible: std::ops::Range<usize>, center: usize) {
        while self.bytes > BUDGET {
            let Some(&far) = self
                .textures
                .keys()
                .filter(|page| !visible.contains(page))
                .max_by_key(|page| page.abs_diff(center))
            else {
                break;
            };
            if let Some(old) = self.textures.remove(&far) {
                self.bytes -= old.bytes;
            }
        }
    }

    /// Starts reading the text for search, once.
    pub fn start_text(&mut self) {
        let pages = self.pages().len();
        if pages > 0 {
            self.worker.start_text(pages);
        }
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
/// Files larger than this are not read whole for a picture.
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

/// The first page of the PDF at `path`, `width` pixels wide, for its card
/// in the chat: rendered once off the interface thread, kept while cards
/// show it, and let go once none has for a while.
pub fn preview(ctx: &egui::Context, path: &Path, width: u32) -> Preview {
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
                let image = contained(|| first_page(&owned, width)).flatten();
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

fn first_page(path: &Path, width: u32) -> Option<egui::ColorImage> {
    if std::fs::metadata(path).ok()?.len() > PREVIEW_MAX_BYTES {
        return None;
    }
    let bytes = Arc::new(std::fs::read(path).ok()?);
    let pdf = open(&bytes).ok()?;
    let cache = hayro::RenderCache::new();
    rasterize(
        &pdf.pages()[0],
        &cache,
        &hayro::hayro_interpret::InterpreterSettings::default(),
        &hayro::RenderSettings::default(),
        width,
    )
}

/// The pages to ask for: those on screen nearest the middle first, then
/// `PREFETCH` each way, each at the width it is drawn at.
pub fn wanted_pages(
    visible: std::ops::Range<usize>,
    count: usize,
    middle: usize,
    width_of: impl Fn(usize) -> u32,
) -> Vec<(usize, u32)> {
    let mut pages: Vec<usize> = visible.clone().collect();
    pages.sort_by_key(|page| page.abs_diff(middle));
    for step in 1..=PREFETCH {
        if let Some(after) = visible
            .end
            .checked_add(step - 1)
            .filter(|page| *page < count)
        {
            pages.push(after);
        }
        if let Some(before) = visible.start.checked_sub(step) {
            pages.push(before);
        }
    }
    pages.dedup();
    pages
        .into_iter()
        .map(|page| (page, width_of(page)))
        .collect()
}

/// A PDF of `pages`, each a content stream drawn on a US Letter page with
/// Helvetica as `/F1`, Helvetica-Bold as `/F2` and Times as `/F3`, its
/// cross-reference table at the right offsets. For tests and the demo.
#[cfg(any(test, feature = "demo"))]
fn write_pdf(pages: &[String]) -> Vec<u8> {
    let fonts = 3 + pages.len() * 2;
    let kids: Vec<String> = (0..pages.len())
        .map(|index| format!("{} 0 R", 3 + index * 2))
        .collect();
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids.join(" "),
            pages.len()
        ),
    ];
    for (index, content) in pages.iter().enumerate() {
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents {} 0 R              /Resources << /Font << /F1 {fonts} 0 R /F2 {} 0 R /F3 {} 0 R >> >> >>",
            4 + index * 2,
            fonts + 1,
            fonts + 2,
        ));
        objects.push(format!(
            "<< /Length {} >>
stream
{content}
endstream",
            content.len()
        ));
    }
    for font in ["Helvetica", "Helvetica-Bold", "Times-Roman"] {
        objects.push(format!(
            "<< /Type /Font /Subtype /Type1 /BaseFont /{font} >>"
        ));
    }
    let mut pdf = b"%PDF-1.4
"
    .to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(
            format!(
                "{} 0 obj
{object}
endobj
",
                index + 1
            )
            .as_bytes(),
        );
    }
    let xref = pdf.len();
    pdf.extend_from_slice(
        format!(
            "xref
0 {}
0000000000 65535 f 
",
            objects.len() + 1
        )
        .as_bytes(),
    );
    for offset in offsets {
        pdf.extend_from_slice(
            format!(
                "{offset:010} 00000 n 
"
            )
            .as_bytes(),
        );
    }
    pdf.extend_from_slice(
        format!(
            "trailer
<< /Size {} /Root 1 0 R >>
startxref
{xref}
%%EOF
",
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
        let bytes = Arc::new(bytes);
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
        let bytes = Arc::new(sample(&["Hello world"]));
        let pdf = open(&bytes).expect("the sample opens");
        let page = &pdf.pages()[0];
        assert_eq!(page.render_dimensions(), (612.0, 792.0));
        let cache = hayro::RenderCache::new();
        let image = rasterize(
            page,
            &cache,
            &hayro::hayro_interpret::InterpreterSettings::default(),
            &hayro::RenderSettings::default(),
            306,
        )
        .expect("it renders");
        assert_eq!(image.size, [306, 396]);
        assert_eq!(image.pixels[0], egui::Color32::WHITE);
        assert!(
            image.pixels.iter().any(|pixel| pixel.r() < 128),
            "the text leaves dark pixels"
        );
    }

    #[test]
    fn something_that_is_not_a_pdf_is_unreadable() {
        let bytes = Arc::new(b"%PDF-1.4 and nothing else".to_vec());
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
    fn render_widths_step_and_respect_limits() {
        let width = render_width(500.0, 1.5, 1.3, 8192);
        assert!(width >= 750);
        assert_eq!(
            render_width(510.0, 1.5, 1.3, 8192),
            width,
            "a small zoom keeps the step"
        );
        assert!(render_width(20_000.0, 2.0, 1.3, 8192) <= 8192);
        let tall = render_width(4000.0, 2.0, 1.3, 16_384);
        assert!(u64::from(tall) * u64::from(tall) * 13 / 10 <= MAX_PIXELS);
    }

    #[test]
    fn wanted_pages_start_in_the_middle_and_prefetch_both_ways() {
        let wanted = wanted_pages(3..6, 10, 5, |_| 100);
        let pages: Vec<usize> = wanted.iter().map(|(page, _)| *page).collect();
        assert_eq!(pages, vec![5, 4, 3, 6, 2, 7, 1]);
        let start = wanted_pages(0..1, 2, 0, |_| 100);
        assert_eq!(
            start.iter().map(|(page, _)| *page).collect::<Vec<_>>(),
            vec![0, 1]
        );
    }

    #[test]
    fn a_viewer_opens_renders_and_searches_in_the_background() {
        let ctx = egui::Context::default();
        let mut viewer = Viewer::from_bytes(
            Arc::new(sample(&["Synthetic invoice", "Total due"])),
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
                vec![(0, 200)]
            };
            viewer.exchange(&ctx, &wanted, 0..1, 0, 1.0);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(viewer.pages(), &[vec2(612.0, 792.0)]);
        assert_eq!(viewer.texture(0).map(|page| page.width), Some(200));
        viewer.search.query = "total".into();
        viewer.start_text();
        while viewer.text_progress().0 < 1 && Instant::now() < deadline {
            viewer.exchange(&ctx, &[], 0..1, 0, 1.0);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(viewer.recount());
        assert_eq!(viewer.search.hits.len(), 1);
        assert!(!viewer.recount(), "nothing changed since");
    }

    #[test]
    fn a_locked_or_broken_file_fails_without_pages() {
        let ctx = egui::Context::default();
        let mut viewer = Viewer::from_bytes(
            Arc::new(b"not a pdf at all".to_vec()),
            PathBuf::from("broken.pdf"),
            "broken.pdf".into(),
            "chat".into(),
            "message".into(),
            &ctx,
        );
        let deadline = Instant::now() + std::time::Duration::from_secs(20);
        while matches!(viewer.load, Load::Opening) && Instant::now() < deadline {
            viewer.exchange(&ctx, &[], 0..0, 0, 1.0);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(matches!(viewer.load, Load::Failed(Failure::Unreadable)));
    }
}
