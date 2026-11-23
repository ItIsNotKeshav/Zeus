//! Settings' camera check: a live, mirrored look through the camera video
//! calls open, so the reader can see it works before a call needs it.
//!
//! The camera is opened, read and dropped on a thread of its own, as a
//! call's capture thread does ([`crate::camera`]'s capture objects are not
//! all `Send`). Each frame is scaled down and handed to the window as an
//! RGBA picture; nothing is recorded, encoded or sent. Dropping the check
//! stops the thread, which releases the camera, and the check stops by
//! itself after [`CHECK_LENGTH`] so a forgotten one does not leave the
//! camera's light on.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::ColorImage;

use crate::backend::Waker;
use crate::settings::SavedDevice;

/// How long a camera check runs before it stops by itself.
pub const CHECK_LENGTH: Duration = Duration::from_secs(120);

/// The largest picture the preview is drawn from, either way round: enough
/// for the panel at 2x, little enough to convert at the camera's rate.
const PREVIEW_BOUNDS: (usize, usize) = (640, 360);

/// Frames a second the camera is asked for.
const PREVIEW_FPS: u32 = 30;

/// Why a check shows no picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// The machine lists no camera.
    NoCamera,
    /// The system refused Zeus the camera (macOS's privacy setting).
    Refused,
    /// The camera would not open: another app has it, or the system's
    /// privacy settings keep apps from it.
    Unavailable,
    /// The camera stopped delivering: unplugged, or taken by another app.
    Stopped,
}

/// What the camera thread hands the window.
#[derive(Default)]
struct Shared {
    stop: AtomicBool,
    frame: Mutex<Option<Arc<ColorImage>>>,
    failure: Mutex<Option<Failure>>,
}

/// One running look through the camera.
pub struct CameraCheck {
    shared: Arc<Shared>,
    started: Instant,
}

impl CameraCheck {
    /// Opens the camera `saved` names (found by its name, as calls find it),
    /// or the first one when it is `None` or not connected.
    pub fn start(saved: Option<SavedDevice>, waker: Waker) -> Self {
        let shared = Arc::new(Shared::default());
        let thread = Arc::clone(&shared);
        let spawned = std::thread::Builder::new()
            .name("camera-check".to_owned())
            .spawn(move || run(saved, &thread, &waker));
        if spawned.is_err() {
            fail(&shared, Failure::Unavailable);
        }
        Self {
            shared,
            started: Instant::now(),
        }
    }

    /// A check that shows a made-up picture, for offline demos and tests,
    /// which never open the machine's camera.
    #[cfg(any(test, feature = "demo"))]
    pub fn simulated() -> Self {
        let shared = Arc::new(Shared::default());
        *lock(&shared.frame) = Some(Arc::new(demo_frame()));
        Self {
            shared,
            started: Instant::now(),
        }
    }

    /// The newest picture, once the camera has delivered one.
    pub fn frame(&self) -> Option<Arc<ColorImage>> {
        lock(&self.shared.frame).clone()
    }

    /// Why the check shows no picture, if it failed.
    pub fn failure(&self) -> Option<Failure> {
        *lock(&self.shared.failure)
    }

    /// Whether the check has run its course.
    pub fn expired(&self) -> bool {
        self.started.elapsed() >= CHECK_LENGTH
    }
}

impl Drop for CameraCheck {
    fn drop(&mut self) {
        // The thread sees this after the frame it is waiting for, then drops
        // the camera.
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn fail(shared: &Shared, failure: Failure) {
    *lock(&shared.failure) = Some(failure);
}

/// The camera thread: finds the camera, opens it, and publishes scaled
/// frames until the check is dropped or the camera stops.
fn run(saved: Option<SavedDevice>, shared: &Shared, waker: &Waker) {
    let found = crate::camera::cameras();
    if found.is_empty() {
        fail(shared, Failure::NoCamera);
        waker.wake();
        return;
    }
    let id = saved
        .as_ref()
        .and_then(|saved| crate::calls::find_camera(&found, saved))
        .unwrap_or(&found[0].0)
        .to_owned();
    #[cfg(target_os = "macos")]
    if !crate::camera::camera_allowed() {
        fail(shared, Failure::Refused);
        waker.wake();
        return;
    }
    if shared.stop.load(Ordering::Relaxed) {
        return;
    }
    let mut source = match crate::camera::Source::open(Some(&id), PREVIEW_BOUNDS, PREVIEW_FPS) {
        Ok(source) => source,
        Err(error) => {
            log::warn!("[SETTINGS] the camera check could not open the camera: {error}");
            fail(shared, Failure::Unavailable);
            waker.wake();
            return;
        }
    };
    let size = crate::camera::fit_even(source.size(), PREVIEW_BOUNDS);
    let (width, height) = size;
    let luma = width * height;
    let chroma = (width / 2) * (height / 2);
    let mut buffer = vec![0; luma + 2 * chroma];
    while !shared.stop.load(Ordering::Relaxed) {
        match source.read(&mut buffer, size) {
            crate::camera::Read::Frame(_) => {}
            crate::camera::Read::Idle => continue,
            crate::camera::Read::Ended => {
                fail(shared, Failure::Stopped);
                waker.wake();
                break;
            }
        }
        let (y, rest) = buffer.split_at(luma);
        let (u, v) = rest.split_at(chroma);
        let image = crate::video::rgb_image(y, u, v, (width, width / 2, width / 2), size, size, 0);
        *lock(&shared.frame) = Some(Arc::new(image));
        waker.wake();
    }
    source.stop();
}

/// The offline demo's picture: a soft diagonal wash with a lighter disc
/// where a face would be, so the panel reads as a camera without one.
#[cfg(any(test, feature = "demo"))]
fn demo_frame() -> ColorImage {
    let (width, height) = (320_usize, 180_usize);
    let mut pixels = Vec::with_capacity(width * height);
    for row in 0..height {
        for column in 0..width {
            let t = (column + row) as f32 / (width + height) as f32;
            let dx = column as f32 - width as f32 * 0.5;
            let dy = row as f32 - height as f32 * 0.55;
            let face = (1.0 - (dx * dx + dy * dy).sqrt() / 70.0).clamp(0.0, 1.0);
            let shade = |from: f32, to: f32| (from + (to - from) * t + 60.0 * face) as u8;
            pixels.push(egui::Color32::from_rgb(
                shade(58.0, 30.0),
                shade(72.0, 44.0),
                shade(96.0, 70.0),
            ));
        }
    }
    ColorImage::new([width, height], pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_simulated_check_shows_a_picture_and_no_failure() {
        let check = CameraCheck::simulated();
        let frame = check.frame().expect("a picture");
        assert_eq!(frame.size, [320, 180]);
        assert_eq!(check.failure(), None);
        assert!(!check.expired());
    }
}
