//! Holds frames to the display's refresh on macOS.
//!
//! macOS 13 and later ignore the swap interval of an OpenGL window: eframe
//! asks for `SwapInterval::Wait(1)`, glutin reports no failure, and the swap
//! returns at once all the same. Left alone, a scroll drew about 120 frames
//! a second on a 60 Hz panel. Each refresh then showed whichever of two
//! unevenly timed frames had finished, so the picture juddered without a
//! single frame being dropped, and the doubled work made the glass blur miss
//! refreshes it would otherwise have made (crmne/zapfast#55).
//!
//! While frames follow each other closely (a scroll, an animation) each one
//! waits for the next tick of a display link before it starts, so frames
//! begin once per refresh, in step with it. A lone frame, the first after
//! the window has been still, never waits, so a key press or a click is
//! drawn at once. The link runs only while frames are coming and stops by
//! itself a moment after they end, so an idle window wakes for nothing.
//!
//! The waiting is here and the counting of refreshes is behind [`Ticks`], so
//! the policy is tested on every platform.

use std::time::{Duration, Instant};

/// Frames closer together than this are an animation to pace. A scroll at
/// 60 frames a second is 17 ms apart, a 30 Hz display's 33 ms; a lone redraw
/// or a spinner's slow frames are further apart and are left alone.
const ANIMATING: Duration = Duration::from_millis(40);

/// The longest a frame waits for a refresh: more than a 30 Hz display's
/// period, so a tick that never comes holds the window up only briefly.
const LIMIT: Duration = Duration::from_millis(80);

/// Waits for a tick that missed this many times in a row turn pacing off for
/// the run, rather than hold every frame up for [`LIMIT`].
const MISSES: u32 = 3;

/// Where display refreshes are counted.
pub trait Ticks {
    /// Refreshes counted so far.
    fn count(&self) -> u64;

    /// Notes that a frame is beginning, so a link that nobody waits on any
    /// more can rest.
    fn began(&self);

    /// Makes sure refreshes are being counted.
    fn start(&self);

    /// Waits until more than `seen` refreshes have been counted, for at most
    /// `limit`, and gives the count then.
    fn wait_past(&self, seen: u64, limit: Duration) -> u64;
}

/// Starts each frame in step with the display's refresh.
pub struct Pacer<T> {
    ticks: T,
    /// When the last frame began.
    last: Option<Instant>,
    /// The refresh count the last frame began on.
    seen: u64,
    /// Waits in a row that ended without a refresh.
    misses: u32,
    off: bool,
}

impl<T: Ticks> Pacer<T> {
    pub fn new(ticks: T) -> Self {
        Self {
            ticks,
            last: None,
            seen: 0,
            misses: 0,
            off: false,
        }
    }

    /// Called as a frame begins: waits for the next refresh if frames are
    /// following each other, and gives back at once if this one is alone.
    pub fn hold(&mut self) {
        if self.off {
            return;
        }
        self.hold_at(Instant::now());
        self.last = Some(Instant::now());
    }

    fn hold_at(&mut self, now: Instant) {
        let animating = self
            .last
            .is_some_and(|last| now.saturating_duration_since(last) < ANIMATING);
        self.ticks.began();
        if !animating {
            self.seen = self.ticks.count();
            self.misses = 0;
            return;
        }
        self.ticks.start();
        let counted = self.ticks.wait_past(self.seen, LIMIT);
        if counted > self.seen {
            self.misses = 0;
        } else {
            self.misses += 1;
            if self.misses >= MISSES {
                log::warn!("frame pacing off: the display link stopped counting refreshes");
                self.off = true;
            }
        }
        self.seen = counted;
    }
}

/// Whether the user turned pacing off with `ZEUS_FRAME_PACING=0`, for
/// comparing a build with and without it.
#[cfg(target_os = "macos")]
fn disabled() -> bool {
    std::env::var("ZEUS_FRAME_PACING")
        .is_ok_and(|value| matches!(value.as_str(), "0" | "off" | "false"))
}

/// The pacer for this platform's display, or nothing where the swap already
/// waits for it or the display cannot be followed.
#[cfg(target_os = "macos")]
pub fn for_display() -> Option<Pacer<DisplayLink>> {
    if disabled() {
        log::info!("frame pacing off (ZEUS_FRAME_PACING)");
        return None;
    }
    DisplayLink::new().map(Pacer::new)
}

#[cfg(target_os = "macos")]
pub use link::DisplayLink;

/// A CoreVideo display link, counting the refreshes of the main display.
#[cfg(target_os = "macos")]
mod link {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Condvar, Mutex, MutexGuard, OnceLock};
    use std::time::{Duration, Instant};

    use super::Ticks;

    type CVReturn = i32;
    type CVOptionFlags = u64;
    type CVDisplayLinkRef = *mut c_void;
    /// `CVDisplayLinkOutputCallback`: the link, the time now, the time the
    /// frame will show, two flags, and what was handed to
    /// `CVDisplayLinkSetOutputCallback`.
    type Callback = extern "C" fn(
        CVDisplayLinkRef,
        *const c_void,
        *const c_void,
        CVOptionFlags,
        *mut CVOptionFlags,
        *mut c_void,
    ) -> CVReturn;

    #[link(name = "CoreVideo", kind = "framework")]
    unsafe extern "C" {
        fn CVDisplayLinkCreateWithActiveCGDisplays(out: *mut CVDisplayLinkRef) -> CVReturn;
        fn CVDisplayLinkSetOutputCallback(
            link: CVDisplayLinkRef,
            callback: Callback,
            user: *mut c_void,
        ) -> CVReturn;
        fn CVDisplayLinkStart(link: CVDisplayLinkRef) -> CVReturn;
        fn CVDisplayLinkStop(link: CVDisplayLinkRef) -> CVReturn;
    }

    /// How long the link keeps counting once frames have stopped.
    const IDLE: Duration = Duration::from_secs(1);

    struct Link(CVDisplayLinkRef);

    // SAFETY: CoreVideo's display link calls may be made from any thread;
    // `Shared::link` serialises the starts and stops anyway.
    unsafe impl Send for Link {}

    struct Shared {
        /// Refreshes counted, and who waits for the next.
        ticks: Mutex<u64>,
        tick: Condvar,
        /// The link, and whether it is counting now.
        link: Mutex<Option<(Link, bool)>>,
        /// When the last frame began, in nanoseconds after [`epoch`].
        began: AtomicU64,
        /// Whether a thread is already on its way to stop an idle link.
        stopping: AtomicBool,
    }

    static SHARED: Shared = Shared {
        ticks: Mutex::new(0),
        tick: Condvar::new(),
        link: Mutex::new(None),
        began: AtomicU64::new(0),
        stopping: AtomicBool::new(false),
    };

    fn epoch() -> Instant {
        static EPOCH: OnceLock<Instant> = OnceLock::new();
        *EPOCH.get_or_init(Instant::now)
    }

    fn nanos() -> u64 {
        u64::try_from(epoch().elapsed().as_nanos()).unwrap_or(u64::MAX)
    }

    fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        mutex
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Runs on CoreVideo's thread at every refresh: counts it, wakes the
    /// frame waiting for it, and sends the link to rest once frames have
    /// stopped. Nothing here may panic, since a panic cannot unwind out of
    /// an `extern "C"` function.
    extern "C" fn on_refresh(
        _link: CVDisplayLinkRef,
        _now: *const c_void,
        _shows: *const c_void,
        _flags_in: CVOptionFlags,
        _flags_out: *mut CVOptionFlags,
        _user: *mut c_void,
    ) -> CVReturn {
        *locked(&SHARED.ticks) += 1;
        SHARED.tick.notify_all();
        let idle =
            nanos().saturating_sub(SHARED.began.load(Ordering::Relaxed)) > IDLE.as_nanos() as u64;
        // Stopping a link from inside its own callback can deadlock, so a
        // short-lived thread does it.
        if idle && !SHARED.stopping.swap(true, Ordering::AcqRel) {
            let spawned = std::thread::Builder::new()
                .name("frame-pacing-stop".into())
                .spawn(stop_if_idle);
            if spawned.is_err() {
                SHARED.stopping.store(false, Ordering::Release);
            }
        }
        0
    }

    fn stop_if_idle() {
        {
            let mut link = locked(&SHARED.link);
            let idle = nanos().saturating_sub(SHARED.began.load(Ordering::Relaxed))
                > IDLE.as_nanos() as u64;
            if let Some((Link(link), running)) = link.as_mut()
                && *running
                && idle
            {
                // SAFETY: the link was made by `DisplayLink::new` and is
                // never released.
                unsafe { CVDisplayLinkStop(*link) };
                *running = false;
                log::debug!("frame pacing: display link at rest");
            }
        }
        SHARED.stopping.store(false, Ordering::Release);
    }

    /// The main display's refreshes, counted from a CoreVideo display link
    /// that runs only while frames are coming.
    pub struct DisplayLink(());

    impl DisplayLink {
        pub fn new() -> Option<Self> {
            let mut link = locked(&SHARED.link);
            if link.is_none() {
                let mut made: CVDisplayLinkRef = std::ptr::null_mut();
                // SAFETY: `made` is a valid place for the new link, and the
                // callback is an `extern "C"` function that outlives it.
                let ok = unsafe {
                    CVDisplayLinkCreateWithActiveCGDisplays(&mut made) == 0
                        && !made.is_null()
                        && CVDisplayLinkSetOutputCallback(made, on_refresh, std::ptr::null_mut())
                            == 0
                };
                if !ok {
                    log::warn!("frame pacing off: no display link");
                    return None;
                }
                *link = Some((Link(made), false));
                log::info!("frame pacing: frames follow the display's refresh");
            }
            epoch();
            Some(Self(()))
        }
    }

    impl Ticks for DisplayLink {
        fn count(&self) -> u64 {
            *locked(&SHARED.ticks)
        }

        fn began(&self) {
            SHARED.began.store(nanos(), Ordering::Relaxed);
        }

        fn start(&self) {
            let mut link = locked(&SHARED.link);
            if let Some((Link(link), running)) = link.as_mut()
                && !*running
            {
                // SAFETY: as in `stop_if_idle`.
                *running = unsafe { CVDisplayLinkStart(*link) } == 0;
                if *running {
                    log::debug!("frame pacing: display link counting");
                }
            }
        }

        fn wait_past(&self, seen: u64, limit: Duration) -> u64 {
            let counted = locked(&SHARED.ticks);
            let (counted, _) = SHARED
                .tick
                .wait_timeout_while(counted, limit, |count| *count <= seen)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *counted
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use super::*;

    /// Refreshes that arrive on demand: each wait is answered by the next
    /// one, or by none when the display has stopped.
    struct Fake {
        count: Cell<u64>,
        started: Cell<u32>,
        waits: RefCell<Vec<u64>>,
        stuck: Cell<bool>,
    }

    impl Fake {
        fn new() -> Self {
            Self {
                count: Cell::new(0),
                started: Cell::new(0),
                waits: RefCell::new(Vec::new()),
                stuck: Cell::new(false),
            }
        }
    }

    impl Ticks for &Fake {
        fn count(&self) -> u64 {
            self.count.get()
        }

        fn began(&self) {}

        fn start(&self) {
            self.started.set(self.started.get() + 1);
        }

        fn wait_past(&self, seen: u64, _limit: Duration) -> u64 {
            self.waits.borrow_mut().push(seen);
            if !self.stuck.get() && self.count.get() <= seen {
                self.count.set(seen + 1);
            }
            self.count.get()
        }
    }

    fn at(base: Instant, millis: u64) -> Instant {
        base + Duration::from_millis(millis)
    }

    /// The first frame, and the first after the window has been still, are
    /// drawn at once, so a key press is never held back.
    #[test]
    fn a_lone_frame_does_not_wait() {
        let display = Fake::new();
        let mut pacer = Pacer::new(&display);
        let base = Instant::now();
        pacer.hold_at(base);
        pacer.last = Some(base);
        pacer.hold_at(at(base, 2_000));
        assert!(display.waits.borrow().is_empty());
        assert_eq!(display.started.get(), 0, "no link for a lone frame");
    }

    /// Frames that follow one another each wait for a refresh, so they
    /// begin once per refresh however quickly they could be made.
    #[test]
    fn frames_in_a_run_wait_for_a_refresh_each() {
        let display = Fake::new();
        let mut pacer = Pacer::new(&display);
        let base = Instant::now();
        pacer.hold_at(base);
        for step in 1..=5 {
            pacer.last = Some(at(base, (step - 1) * 7));
            pacer.hold_at(at(base, step * 7));
        }
        assert_eq!(display.waits.borrow().as_slice(), [0, 1, 2, 3, 4]);
        assert_eq!(display.count.get(), 5);
        assert_eq!(display.started.get(), 5, "asked to count before each wait");
    }

    /// A frame that took longer than a refresh finds the next one already
    /// counted and starts without waiting.
    #[test]
    fn a_slow_frame_does_not_wait_again() {
        let display = Fake::new();
        let mut pacer = Pacer::new(&display);
        let base = Instant::now();
        pacer.hold_at(base);
        pacer.last = Some(base);
        pacer.hold_at(at(base, 10));
        assert_eq!(pacer.seen, 1);
        // Two refreshes went by while the frame was drawn.
        display.count.set(3);
        pacer.last = Some(at(base, 10));
        pacer.hold_at(at(base, 30));
        assert_eq!(pacer.seen, 3);
        assert!(!pacer.off);
    }

    /// A display link that stops counting turns pacing off for the run, so
    /// frames are never held up for good.
    #[test]
    fn a_stalled_link_turns_pacing_off() {
        let display = Fake::new();
        display.stuck.set(true);
        let mut pacer = Pacer::new(&display);
        let base = Instant::now();
        pacer.hold_at(base);
        for step in 1..=MISSES as u64 {
            pacer.last = Some(at(base, (step - 1) * 20));
            pacer.hold_at(at(base, step * 20));
        }
        assert!(pacer.off);
        let waits = display.waits.borrow().len();
        pacer.hold();
        assert_eq!(display.waits.borrow().len(), waits, "off means no waiting");
    }
}
