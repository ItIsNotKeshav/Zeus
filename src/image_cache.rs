//! Releases egui's per-URI image caches for images that leave the screen.
//!
//! egui caches the bytes, the decoded pixels, and the GPU texture of every URI
//! it has loaded. It only ever evicts the textures of `.svg` files, so left
//! alone a session that scrolls through photos holds every one of them for
//! good. Each draw site registers its URI here, and [`sweep`] forgets the ones
//! that fall out of the resident window, by count and by the bytes their
//! textures take.
//!
//! Chat pictures go through [`fit_uri`]: a [`FitLoader`] decodes them off the
//! interface thread at the size they are drawn, so a 12-megapixel photo in a
//! bubble takes a few megabytes of texture rather than 48.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, mpsc};

/// Image URIs kept resident. Anything the current frame drew is always kept,
/// so this only bounds what has scrolled away.
const RESIDENT: usize = 256;

/// The most texture memory the resident images may take, in bytes. On a Mac
/// the GPU shares the system's memory, so this is memory the app is seen to
/// use. Anything the current frame drew is always kept.
const BUDGET: usize = 256 * 1024 * 1024;

struct Entry {
    /// Frame the image was last drawn on.
    frame: u64,
    /// Whether its bytes are registered with egui's byte loader. egui drops
    /// them once the texture is uploaded, so this is what says a later draw
    /// has to register them again.
    registered: bool,
}

#[derive(Clone, Default)]
struct Registry(Arc<Mutex<HashMap<String, Entry>>>);

fn registry(ctx: &egui::Context) -> Registry {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Registry>(egui::Id::new("image-cache"))
            .clone()
    })
}

/// Registers `bytes` under `uri`, and marks the image as drawn this frame.
///
/// Re-registers them after [`sweep`] has forgotten the image, or after egui
/// has dropped them once the texture was uploaded.
pub fn include(ctx: &egui::Context, uri: &str, bytes: &[u8]) {
    let frame = ctx.cumulative_frame_nr();
    let registry = registry(ctx);
    let mut entries = registry.0.lock().unwrap_or_else(|p| p.into_inner());
    let fresh = match entries.get_mut(uri) {
        Some(entry) => {
            entry.frame = frame;
            !std::mem::replace(&mut entry.registered, true)
        }
        None => {
            entries.insert(
                uri.to_owned(),
                Entry {
                    frame,
                    registered: true,
                },
            );
            true
        }
    };
    drop(entries);
    if fresh {
        ctx.include_bytes(uri.to_owned(), bytes.to_vec());
    }
}

/// Marks an image URI as drawn this frame.
pub fn touch(ctx: &egui::Context, uri: &str) {
    let frame = ctx.cumulative_frame_nr();
    let registry = registry(ctx);
    let mut entries = registry.0.lock().unwrap_or_else(|p| p.into_inner());
    match entries.get_mut(uri) {
        Some(entry) => entry.frame = frame,
        None => {
            entries.insert(
                uri.to_owned(),
                Entry {
                    frame,
                    registered: false,
                },
            );
        }
    }
}

/// Forgets one image now rather than once it falls out of the resident
/// window: the info panel frees its thumbnails when it closes. A later draw
/// registers the bytes again. Forgetting a file's URI also forgets the
/// pictures [`fit_uri`] made of it, so a changed avatar is read again.
pub fn forget(ctx: &egui::Context, uri: &str) {
    let fitted: Vec<String> = {
        let registry = registry(ctx);
        let mut entries = registry.0.lock().unwrap_or_else(|p| p.into_inner());
        entries.remove(uri);
        let fitted: Vec<String> = entries
            .keys()
            .filter(|key| fitted_source(key) == Some(uri))
            .cloned()
            .collect();
        for key in &fitted {
            entries.remove(key);
        }
        fitted
    };
    ctx.forget_image(uri);
    for key in fitted {
        ctx.forget_image(&key);
    }
}

/// Forgets the images that have not been drawn for the longest, until no
/// more than `RESIDENT` are kept and their textures fit in `BUDGET`.
///
/// Images the current frame drew are never forgotten, so a frame that shows
/// more than the window holds does not reload them on the next one.
pub fn sweep(ctx: &egui::Context) {
    let frame = ctx.cumulative_frame_nr();
    let stale: Vec<String> = {
        let registry = registry(ctx);
        let mut entries = registry.0.lock().unwrap_or_else(|p| p.into_inner());
        if entries.is_empty() {
            return;
        }
        // egui names each texture it loads after its URI.
        let mut sizes: HashMap<&str, usize> = HashMap::new();
        let mut total = 0;
        {
            let manager = ctx.tex_manager();
            let manager = manager.read();
            for (_, meta) in manager.allocated() {
                if let Some((uri, _)) = entries.get_key_value(meta.name.as_str()) {
                    *sizes.entry(uri.as_str()).or_default() += meta.bytes_used();
                    total += meta.bytes_used();
                }
            }
        }
        let mut count = entries.len();
        if count <= RESIDENT && total <= BUDGET {
            return;
        }
        let mut idle: Vec<(u64, &str)> = entries
            .iter()
            .filter(|(_, entry)| entry.frame < frame)
            .map(|(uri, entry)| (entry.frame, uri.as_str()))
            .collect();
        idle.sort_unstable();
        let mut stale = Vec::new();
        for (_, uri) in idle {
            if count <= RESIDENT && total <= BUDGET {
                break;
            }
            count -= 1;
            total -= sizes.get(uri).copied().unwrap_or(0);
            stale.push(uri.to_owned());
        }
        drop(sizes);
        for uri in &stale {
            entries.remove(uri);
        }
        stale
    };
    for uri in stale {
        ctx.forget_image(&uri);
    }
}

/// The scheme [`FitLoader`] serves: `fit://{side}/{file URI}`.
const FIT: &str = "fit://";

/// The sides a fitted picture's shorter edge is decoded at, in pixels. A
/// picture drawn at a size between two steps uses the larger, so one drawn a
/// little bigger or smaller (a resized window) keeps its texture.
const SIDES: [u32; 8] = [128, 192, 256, 384, 512, 768, 1024, 1536];

/// The longest a fitted picture's longer edge is, so a panorama stays small.
const LONGEST: u32 = 4096;

/// A URI for the picture at `path`, decoded no larger than it is drawn at
/// `size` points on this display, and registered as drawn this frame. A file
/// [`FitLoader`] cannot decode (an animated GIF or WebP, an SVG) keeps its
/// plain file URI. Full-size viewers use [`crate::util::image_uri`] instead.
pub fn fit_uri(ctx: &egui::Context, path: &Path, size: egui::Vec2) -> String {
    let file = crate::util::image_uri(path);
    let uri = if fits(path) {
        let pixels = (size.x.max(size.y) * ctx.pixels_per_point()).ceil() as u32;
        let side = SIDES
            .iter()
            .copied()
            .find(|side| *side >= pixels)
            .unwrap_or(SIDES[SIDES.len() - 1]);
        format!("{FIT}{side}/{file}")
    } else {
        file
    };
    touch(ctx, &uri);
    uri
}

/// Whether [`FitLoader`] decodes the file: the still formats photos come in.
fn fits(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            ["jpg", "jpeg", "png"]
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
}

/// The file URI a [`fit_uri`] was made from, and the side it asks for.
fn parse_fit(uri: &str) -> Option<(u32, &str)> {
    let (side, file) = uri.strip_prefix(FIT)?.split_once('/')?;
    Some((side.parse().ok()?, file))
}

fn fitted_source(uri: &str) -> Option<&str> {
    parse_fit(uri).map(|(_, file)| file)
}

/// The path a file URI names, as [`crate::util::image_uri`] writes it.
fn uri_path(file: &str) -> Option<&str> {
    let path = file.strip_prefix("file://")?;
    if cfg!(windows) {
        Some(path.strip_prefix('/').unwrap_or(path))
    } else {
        Some(path)
    }
}

/// Decodes `path` so its shorter edge is at least `side` pixels (when the
/// picture is that big) and its longer edge at most [`LONGEST`]. The image
/// keeps the picture's own size as its source size, so layouts that read it
/// see the photo's real shape and dimensions.
fn decode_fitted(path: &Path, side: u32) -> Result<egui::ColorImage, String> {
    use image::ImageDecoder as _;
    let reader = image::ImageReader::open(path)
        .and_then(image::ImageReader::with_guessed_format)
        .map_err(|error| error.to_string())?;
    let mut decoder = reader.into_decoder().map_err(|error| error.to_string())?;
    let orientation = decoder.orientation().map_err(|error| error.to_string())?;
    let mut picture =
        image::DynamicImage::from_decoder(decoder).map_err(|error| error.to_string())?;
    picture.apply_orientation(orientation);
    let (width, height) = (picture.width().max(1), picture.height().max(1));
    let (short, long) = (width.min(height), width.max(height));
    let scale = (f64::from(side) / f64::from(short))
        .min(f64::from(LONGEST) / f64::from(long))
        .min(1.0);
    let picture = if scale < 1.0 {
        let fitted = |edge: u32| ((f64::from(edge) * scale).round() as u32).max(1);
        picture.thumbnail_exact(fitted(width), fitted(height))
    } else {
        picture
    };
    let rgba = picture.to_rgba8();
    let mut image = egui::ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_flat_samples().as_slice(),
    );
    image.source_size = egui::vec2(width as f32, height as f32);
    Ok(image)
}

/// Where a fitted picture is in [`FitLoader`].
#[derive(Clone)]
enum Fitted {
    Pending,
    Ready(Arc<egui::ColorImage>),
    Failed(String),
}

type Fits = Arc<Mutex<HashMap<String, Fitted>>>;

/// Serves [`fit_uri`]s: reads and decodes each picture on a worker thread,
/// scaled to the size it is drawn at.
///
/// One thread decodes, newest request first, so pictures that scrolled past
/// before their turn and were forgotten are skipped rather than decoded.
pub struct FitLoader {
    fits: Fits,
    jobs: Mutex<Option<mpsc::Sender<(String, egui::Context)>>>,
}

impl Default for FitLoader {
    fn default() -> Self {
        Self {
            fits: Fits::default(),
            jobs: Mutex::new(None),
        }
    }
}

impl FitLoader {
    fn queue(&self, uri: &str, ctx: &egui::Context) {
        let mut jobs = self.jobs.lock().unwrap_or_else(|p| p.into_inner());
        if jobs.is_none() {
            let (sender, receiver) = mpsc::channel::<(String, egui::Context)>();
            let fits = Arc::clone(&self.fits);
            let started = std::thread::Builder::new()
                .name("fit-pictures".into())
                .spawn(move || decode_jobs(&receiver, &fits));
            if let Err(error) = started {
                log::warn!("cannot start the picture decoder: {error}");
                self.fits
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .insert(uri.to_owned(), Fitted::Failed(error.to_string()));
                return;
            }
            *jobs = Some(sender);
        }
        if let Some(sender) = jobs.as_ref() {
            let _ = sender.send((uri.to_owned(), ctx.clone()));
        }
    }
}

/// The URIs of message thumbnails, which [`FitLoader`] decodes at once.
const THUMBNAIL: &str = "bytes://thumb-";

/// The most a thumbnail may weigh to be decoded on the interface thread. A
/// message's preview is a few kilobytes and decodes in well under a
/// millisecond; anything larger goes to egui's own loader.
const SMALL_THUMBNAIL: usize = 32 * 1024;

impl FitLoader {
    /// Decodes a small thumbnail in the frame that first draws it. egui's
    /// loader decodes on a thread of its own, so a chat would open with an
    /// empty frame, or egui's spinner, where each preview goes, and fill
    /// them in a frame later.
    fn load_thumbnail(&self, ctx: &egui::Context, uri: &str) -> egui::load::ImageLoadResult {
        let known = self
            .fits
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(uri)
            .cloned();
        if let Some(Fitted::Ready(image)) = known {
            return Ok(egui::load::ImagePoll::Ready { image });
        }
        let Ok(egui::load::BytesPoll::Ready { bytes, .. }) = ctx.try_load_bytes(uri) else {
            return Err(egui::load::LoadError::NotSupported);
        };
        if bytes.len() > SMALL_THUMBNAIL {
            return Err(egui::load::LoadError::NotSupported);
        }
        // A crafted one fails here, as a picture does on the decoder thread.
        let decoded = std::panic::catch_unwind(|| decode_thumbnail(&bytes));
        let Ok(Some(image)) = decoded else {
            return Err(egui::load::LoadError::NotSupported);
        };
        let image = Arc::new(image);
        self.fits
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(uri.to_owned(), Fitted::Ready(Arc::clone(&image)));
        Ok(egui::load::ImagePoll::Ready { image })
    }
}

/// Decodes a thumbnail's bytes, if they hold a picture of a thumbnail's
/// size: a few bytes that claim a huge picture are not given its memory.
fn decode_thumbnail(bytes: &[u8]) -> Option<egui::ColorImage> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(1024);
    limits.max_image_height = Some(1024);
    limits.max_alloc = Some(16 * 1024 * 1024);
    reader.limits(limits);
    let rgba = reader.decode().ok()?.to_rgba8();
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_flat_samples().as_slice(),
    ))
}

/// The decoder thread: takes every waiting request, decodes the newest still
/// wanted, and puts the rest back.
fn decode_jobs(receiver: &mpsc::Receiver<(String, egui::Context)>, fits: &Fits) {
    let mut waiting: Vec<(String, egui::Context)> = Vec::new();
    loop {
        if waiting.is_empty() {
            match receiver.recv() {
                Ok(job) => waiting.push(job),
                Err(_) => return,
            }
        }
        waiting.extend(receiver.try_iter());
        let Some((uri, ctx)) = waiting.pop() else {
            continue;
        };
        let wanted = matches!(
            fits.lock().unwrap_or_else(|p| p.into_inner()).get(&uri),
            Some(Fitted::Pending)
        );
        if !wanted {
            continue;
        }
        let result = parse_fit(&uri)
            .and_then(|(side, file)| Some((side, uri_path(file)?)))
            .ok_or_else(|| "not a fitted picture".to_owned())
            .and_then(|(side, path)| decode_fitted(Path::new(path), side));
        let mut fits = fits.lock().unwrap_or_else(|p| p.into_inner());
        // Forgotten while it was being decoded: nobody draws it any more.
        if let Some(entry @ Fitted::Pending) = fits.get_mut(&uri) {
            *entry = match result {
                Ok(image) => Fitted::Ready(Arc::new(image)),
                Err(error) => Fitted::Failed(error),
            };
            drop(fits);
            ctx.request_repaint();
        }
    }
}

impl egui::load::ImageLoader for FitLoader {
    fn id(&self) -> &str {
        "zeus::image_cache::FitLoader"
    }

    fn load(
        &self,
        ctx: &egui::Context,
        uri: &str,
        _size_hint: egui::load::SizeHint,
    ) -> egui::load::ImageLoadResult {
        if uri.starts_with(THUMBNAIL) {
            return self.load_thumbnail(ctx, uri);
        }
        if parse_fit(uri).is_none() {
            return Err(egui::load::LoadError::NotSupported);
        }
        let known = self
            .fits
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(uri)
            .cloned();
        match known {
            Some(Fitted::Ready(image)) => Ok(egui::load::ImagePoll::Ready { image }),
            Some(Fitted::Failed(error)) => Err(egui::load::LoadError::Loading(error)),
            Some(Fitted::Pending) => Ok(egui::load::ImagePoll::Pending { size: None }),
            None => {
                self.fits
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .insert(uri.to_owned(), Fitted::Pending);
                self.queue(uri, ctx);
                Ok(egui::load::ImagePoll::Pending { size: None })
            }
        }
    }

    fn forget(&self, uri: &str) {
        self.fits
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(uri);
    }

    fn forget_all(&self) {
        self.fits.lock().unwrap_or_else(|p| p.into_inner()).clear();
    }

    fn byte_size(&self) -> usize {
        self.fits
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .values()
            .map(|fitted| match fitted {
                Fitted::Ready(image) => image.pixels.len() * 4,
                Fitted::Pending | Fitted::Failed(_) => 0,
            })
            .sum()
    }

    fn has_pending(&self) -> bool {
        self.fits
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .values()
            .any(|fitted| matches!(fitted, Fitted::Pending))
    }
}

/// Installs [`FitLoader`] ahead of egui's own loaders.
pub fn install(ctx: &egui::Context) {
    ctx.add_image_loader(Arc::new(FitLoader::default()));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> egui::Context {
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        ctx
    }

    /// Advances the frame counter the way a repaint would.
    fn frame(ctx: &egui::Context) {
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
        output.textures_delta.clear();
    }

    fn registered(ctx: &egui::Context, uri: &str) -> bool {
        ctx.try_load_bytes(uri).is_ok()
    }

    fn uri(index: usize) -> String {
        format!("bytes://test-{index:04}")
    }

    /// Answers in the same frame, so a test can watch a texture being
    /// allocated without waiting on a decoding thread.
    struct ReadyLoader;

    impl egui::load::ImageLoader for ReadyLoader {
        fn id(&self) -> &str {
            "zeus::image_cache::tests::ReadyLoader"
        }

        fn load(
            &self,
            _ctx: &egui::Context,
            _uri: &str,
            _size_hint: egui::load::SizeHint,
        ) -> egui::load::ImageLoadResult {
            Ok(egui::load::ImagePoll::Ready {
                image: std::sync::Arc::new(egui::ColorImage::example()),
            })
        }

        fn forget(&self, _uri: &str) {}

        fn forget_all(&self) {}

        fn byte_size(&self) -> usize {
            0
        }
    }

    /// Answers every URI with a 1024-pixel square: 4 MB of texture.
    struct LargeLoader;

    impl egui::load::ImageLoader for LargeLoader {
        fn id(&self) -> &str {
            "zeus::image_cache::tests::LargeLoader"
        }

        fn load(
            &self,
            _ctx: &egui::Context,
            _uri: &str,
            _size_hint: egui::load::SizeHint,
        ) -> egui::load::ImageLoadResult {
            Ok(egui::load::ImagePoll::Ready {
                image: std::sync::Arc::new(egui::ColorImage::filled(
                    [1024, 1024],
                    egui::Color32::WHITE,
                )),
            })
        }

        fn forget(&self, _uri: &str) {}

        fn forget_all(&self) {}

        fn byte_size(&self) -> usize {
            0
        }
    }

    #[test]
    fn big_textures_that_scroll_away_are_released_past_the_budget() {
        let ctx = context();
        ctx.add_image_loader(std::sync::Arc::new(LargeLoader));
        let uris: Vec<String> = (0..80).map(uri).collect();
        for uri in &uris {
            include(&ctx, uri, b"image bytes");
        }
        let mut output = draw(&ctx, &uris);
        output.textures_delta.clear();
        let used = |ctx: &egui::Context| -> usize {
            let manager = ctx.tex_manager();
            let manager = manager.read();
            manager
                .allocated()
                .filter(|(_, meta)| meta.name.starts_with("bytes://test-"))
                .map(|(_, meta)| meta.bytes_used())
                .sum()
        };
        assert!(used(&ctx) > BUDGET, "what is on screen stays");
        let mut output = draw(&ctx, &[]);
        output.textures_delta.clear();
        assert!(used(&ctx) <= BUDGET, "kept {} bytes", used(&ctx));
        assert!(used(&ctx) > BUDGET - 8 * 1024 * 1024, "kept too little");
    }

    /// Draws `shown` and then sweeps, the way a frame of the app does.
    fn draw(ctx: &egui::Context, shown: &[String]) -> egui::FullOutput {
        ctx.run_ui(egui::RawInput::default(), |ui| {
            for uri in shown {
                ui.add(egui::Image::from_uri(uri.clone()));
            }
            sweep(ui.ctx());
        })
    }

    fn allocated(ctx: &egui::Context) -> usize {
        let manager = ctx.tex_manager();
        manager.read().num_allocated()
    }

    fn picture(dir: &Path, name: &str, width: u32, height: u32) -> std::path::PathBuf {
        let path = dir.join(name);
        image::RgbaImage::from_pixel(width, height, image::Rgba([200, 30, 30, 255]))
            .save(&path)
            .expect("saved");
        path
    }

    #[test]
    fn a_fitted_picture_is_decoded_no_larger_than_drawn() {
        let dir = tempfile::tempdir().expect("temp");
        let path = picture(dir.path(), "wide.png", 2000, 1000);
        let image = decode_fitted(&path, 256).expect("decoded");
        assert_eq!(image.size, [512, 256], "the shorter edge covers the side");
        assert_eq!(
            image.source_size,
            egui::vec2(2000.0, 1000.0),
            "layouts still see the photo's own size"
        );
        let small = picture(dir.path(), "small.png", 100, 50);
        let image = decode_fitted(&small, 256).expect("decoded");
        assert_eq!(image.size, [100, 50], "a small picture is never enlarged");
    }

    /// A thumbnail is ready in the frame that first draws it, rather than a
    /// frame later from egui's loader thread.
    #[test]
    fn a_thumbnail_is_ready_at_once() {
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        install(&ctx);
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(24, 16, image::Rgba([200, 40, 40, 255]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .expect("encoded");
        let uri = "bytes://thumb-chat-message";
        include(&ctx, uri, &png);
        match ctx.try_load_image(uri, egui::load::SizeHint::default()) {
            Ok(egui::load::ImagePoll::Ready { image }) => assert_eq!(image.size, [24, 16]),
            Ok(egui::load::ImagePoll::Pending { .. }) => panic!("still pending"),
            Err(error) => panic!("{error}"),
        }
        assert!(decode_thumbnail(b"not a picture").is_none());
    }

    #[test]
    fn a_picture_is_fitted_to_the_next_side_up_and_an_animation_is_not() {
        let ctx = egui::Context::default();
        let path = Path::new("/tmp/photo.JPG");
        let file = crate::util::image_uri(path);
        assert_eq!(
            fit_uri(&ctx, path, egui::vec2(200.0, 120.0)),
            format!("fit://256/{file}")
        );
        assert_eq!(
            fit_uri(&ctx, path, egui::vec2(5000.0, 120.0)),
            format!("fit://1536/{file}"),
            "huge sizes stop at the largest side"
        );
        let gif = Path::new("/tmp/moving.gif");
        assert_eq!(
            fit_uri(&ctx, gif, egui::vec2(200.0, 120.0)),
            crate::util::image_uri(gif)
        );
        assert_eq!(
            parse_fit(&format!("fit://256/{file}")),
            Some((256, file.as_str()))
        );
    }

    #[test]
    fn forgetting_a_file_forgets_the_pictures_fitted_from_it() {
        let ctx = egui::Context::default();
        let path = Path::new("/tmp/avatar.jpg");
        let small = fit_uri(&ctx, path, egui::vec2(40.0, 40.0));
        let large = fit_uri(&ctx, path, egui::vec2(400.0, 400.0));
        let other = fit_uri(&ctx, Path::new("/tmp/other.jpg"), egui::vec2(40.0, 40.0));
        forget(&ctx, &crate::util::image_uri(path));
        let registry = registry(&ctx);
        let entries = registry.0.lock().unwrap();
        assert!(!entries.contains_key(&small) && !entries.contains_key(&large));
        assert!(entries.contains_key(&other));
    }

    #[test]
    fn a_fitted_picture_loads_off_the_interface_thread() {
        let dir = tempfile::tempdir().expect("temp");
        let path = picture(dir.path(), "photo.png", 1200, 900);
        let ctx = context();
        install(&ctx);
        let uri = fit_uri(&ctx, &path, egui::vec2(100.0, 100.0));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let texture = loop {
            match ctx.try_load_texture(
                &uri,
                egui::TextureOptions::LINEAR,
                egui::load::SizeHint::default(),
            ) {
                Ok(egui::load::TexturePoll::Ready { texture }) => break texture,
                Ok(egui::load::TexturePoll::Pending { .. }) => {
                    assert!(std::time::Instant::now() < deadline, "never decoded");
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(error) => panic!("{error}"),
            }
        };
        assert_eq!(texture.size, egui::vec2(1200.0, 900.0));
        let manager = ctx.tex_manager();
        let manager = manager.read();
        let meta = manager.meta(texture.id).expect("allocated");
        assert_eq!(meta.size, [171, 128], "decoded at the drawn size");
    }

    #[test]
    fn a_forgotten_image_is_released_and_registers_again_when_drawn() {
        let ctx = context();
        include(&ctx, &uri(1), b"image bytes");
        assert!(registered(&ctx, &uri(1)));
        forget(&ctx, &uri(1));
        assert!(
            !registered(&ctx, &uri(1)),
            "forgotten at once, not swept later"
        );
        include(&ctx, &uri(1), b"image bytes");
        assert!(registered(&ctx, &uri(1)), "a later draw registers it again");
    }

    #[test]
    fn images_the_current_frame_drew_are_never_released() {
        let ctx = context();
        let total = RESIDENT + 40;
        for index in 0..total {
            include(&ctx, &uri(index), b"image bytes");
        }
        // A frame that shows more images than the window holds must not drop
        // the ones it is showing right now.
        sweep(&ctx);
        for index in 0..total {
            assert!(registered(&ctx, &uri(index)), "{} was released", uri(index));
        }
    }

    #[test]
    fn images_that_scroll_away_are_released() {
        let ctx = context();
        let total = RESIDENT + 40;
        for index in 0..total {
            include(&ctx, &uri(index), b"image bytes");
        }
        frame(&ctx);
        sweep(&ctx);
        let kept = (0..total)
            .filter(|index| registered(&ctx, &uri(*index)))
            .count();
        assert_eq!(kept, RESIDENT, "the window should hold exactly its budget");
    }

    #[test]
    fn a_released_image_is_registered_again_when_it_comes_back() {
        let ctx = context();
        for index in 0..RESIDENT + 40 {
            include(&ctx, &uri(index), b"image bytes");
        }
        frame(&ctx);
        sweep(&ctx);
        // The oldest URI lost its bytes. Drawing it again must put them back,
        // because egui has no other way to resolve a `bytes://` image.
        include(&ctx, &uri(0), b"image bytes");
        assert!(registered(&ctx, &uri(0)));
    }

    #[test]
    fn images_that_scroll_away_free_their_texture() {
        let ctx = context();
        ctx.add_image_loader(std::sync::Arc::new(ReadyLoader));
        let uris: Vec<String> = (0..RESIDENT + 8).map(uri).collect();
        for uri in &uris {
            include(&ctx, uri, b"image bytes");
        }
        let mut showing = draw(&ctx, &uris);
        let shown = allocated(&ctx);
        assert!(
            shown >= uris.len(),
            "a drawn image should own a texture, saw {shown} for {} images",
            uris.len()
        );
        showing.textures_delta.clear();

        // Nothing on screen: the images past the window go, and the frame that
        // released them is the one that tells the painter to delete them.
        let mut empty = draw(&ctx, &[]);
        assert_eq!(
            empty.textures_delta.free.len(),
            8,
            "the swept textures should be freed by the painter"
        );
        assert_eq!(allocated(&ctx), shown - 8);
        empty.textures_delta.clear();
    }
}
