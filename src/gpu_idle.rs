//! Lets the window's graphics device go while nobody is looking (macOS).
//!
//! eframe (crmne/egui) can drop its wgpu device after a pass, leaving the
//! last frame on screen at half resolution, and bring it back for the next
//! pass that asks. [`GpuIdle`] decides when: the window has been unfocused for
//! the font atlas's own wait, nothing on screen moves or plays, and nothing
//! visible changed lately. While the device is gone, an event that changes
//! what the window shows asks for it back, at most once per [`WAKE_GAP`],
//! and a ringing call or the app lock at once. eframe brings it back on its
//! own for input, focus, a resize, a change of scale, or an uncovered window.
//!
//! Nothing here touches eframe: the shell reads its state into
//! [`GpuIdle::observe`] and hands [`Change`] to `Frame::release_gpu` or
//! `Frame::request_gpu`, so the decisions are tested without a device.

use std::time::{Duration, Instant};

use crate::backend::Event;

/// The shortest time between two requests for the device: a busy chat
/// brings the window back once per this, not once per message.
pub const WAKE_GAP: Duration = Duration::from_secs(2);
/// How long nothing visible must have changed, and the device must have been
/// back, before it goes again: a restore for one message or one hover over
/// the window is not undone in the next pass.
pub const QUIET: Duration = Duration::from_secs(30);

/// Where eframe's device is, as `eframe::GpuState` reports it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum State {
    /// Drawing at full resolution.
    #[default]
    Active,
    /// Drawing the last pass at 1x before the device goes.
    LowRes,
    /// No device: passes run the app's logic and paint nothing.
    Released,
}

impl State {
    fn name(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::LowRes => "lowres",
            Self::Released => "released",
        }
    }
}

/// How urgently an event wants the device back.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Wake {
    /// It changes nothing on screen, or nothing worth a frame.
    #[default]
    No,
    /// It changes what the window shows: within [`WAKE_GAP`].
    Soon,
    /// A call rings or the lock changed: at once.
    Now,
}

/// What the shell asks eframe for this pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Release,
    Request,
}

/// The window's device, as decided across passes. One per app: it outlives
/// windows, and [`Self::new_window`] starts each one with a device.
#[derive(Debug, Default)]
pub struct GpuIdle {
    state: State,
    /// The release was asked for and eframe has not acted on it yet.
    asked: bool,
    /// The strongest wake noted while the device was away or going.
    pending: Wake,
    /// When something visible last changed.
    changed: Option<Instant>,
    /// When the device was last asked back.
    requested: Option<Instant>,
    /// When the device last went, and last came back.
    released_at: Option<Instant>,
    restored_at: Option<Instant>,
    /// Whether the app lock was on when the release was asked for.
    locked: bool,
    /// The device went since [`Self::take_gone`] last looked.
    gone: bool,
}

impl GpuIdle {
    pub fn state(&self) -> State {
        self.state
    }

    /// A new window starts with a device and nothing waiting.
    pub fn new_window(&mut self) {
        self.state = State::Active;
        self.asked = false;
        self.pending = Wake::No;
    }

    /// Takes eframe's state at the start of a pass.
    pub fn observe(&mut self, state: State, now: Instant) {
        if state == self.state {
            return;
        }
        match state {
            State::Released => {
                self.released_at = Some(now);
                self.gone = true;
            }
            State::Active => {
                self.restored_at = Some(now);
                self.pending = Wake::No;
                self.gone = false;
            }
            State::LowRes => {}
        }
        self.asked = false;
        self.state = state;
    }

    /// Notes an event's [`Wake`]. Every visible change restarts [`QUIET`];
    /// one while the device is away or going waits for [`Self::wake`].
    pub fn note(&mut self, wake: Wake, now: Instant) {
        if wake == Wake::No {
            return;
        }
        self.changed = Some(now);
        if self.state != State::Active || self.asked {
            self.pending = self.pending.max(wake);
        }
    }

    /// Notes the app lock as it is now: a lock or an unlock since the
    /// release must be drawn at once, since the last frame shows the other.
    pub fn note_lock(&mut self, locked: bool, now: Instant) {
        if self.state != State::Active && locked != self.locked {
            self.locked = locked;
            self.note(Wake::Now, now);
        }
    }

    /// When the device may go, while it is here: `Some(ZERO)` now,
    /// `Some(wait)` to look again after `wait`, `None` while the window is
    /// in use (`away` is when it lost focus) or `busy` (something moves,
    /// plays or is being sent).
    pub fn release_after(
        &self,
        now: Instant,
        away: Option<Instant>,
        idle: Duration,
        busy: bool,
    ) -> Option<Duration> {
        if self.state != State::Active || self.asked || busy {
            return None;
        }
        let left = |since: Option<Instant>, wait: Duration| {
            since.map_or(Duration::ZERO, |since| {
                wait.saturating_sub(now.saturating_duration_since(since))
            })
        };
        // Focused, the window is in use.
        away?;
        Some(
            left(away, idle)
                .max(left(self.changed, QUIET))
                .max(left(self.restored_at, QUIET)),
        )
    }

    /// Whether the device went since the last call: once per release, in
    /// the first pass without it, when what is freed is not drawn again
    /// until the device is back.
    pub fn take_gone(&mut self) -> bool {
        std::mem::take(&mut self.gone)
    }

    /// Records that the release was asked for this pass.
    pub fn released(&mut self, locked: bool) {
        self.asked = true;
        self.locked = locked;
        self.pending = Wake::No;
    }

    /// While the device is away or going: whether to ask for it back this
    /// pass, and otherwise how long until a waiting wake may. A minimized
    /// or covered window (`shown` false) keeps a [`Wake::Soon`] waiting:
    /// nobody would see the frame, and eframe brings the device back on
    /// its own when the window is shown again.
    pub fn wake(&mut self, now: Instant, shown: bool) -> (Option<Change>, Option<Duration>) {
        if self.state == State::Active && !self.asked {
            return (None, None);
        }
        let left = match self.pending {
            Wake::No => return (None, None),
            Wake::Soon if !shown => return (None, None),
            Wake::Now => Duration::ZERO,
            Wake::Soon => self.requested.map_or(Duration::ZERO, |requested| {
                WAKE_GAP.saturating_sub(now.saturating_duration_since(requested))
            }),
        };
        if !left.is_zero() {
            return (None, Some(left));
        }
        self.pending = Wake::No;
        self.requested = Some(now);
        // A release asked for and not yet acted on is called off by this.
        self.asked = false;
        (Some(Change::Request), None)
    }

    /// The memory probe's part: the state, and how many seconds ago the
    /// device last went and came back (`-` for never).
    pub fn describe(&self, now: Instant) -> String {
        let ago = |at: Option<Instant>| {
            at.map_or_else(
                || "-".to_owned(),
                |at| now.saturating_duration_since(at).as_secs().to_string(),
            )
        };
        format!(
            "gpu={} gpu_released_s={} gpu_restored_s={}",
            self.state.name(),
            ago(self.released_at),
            ago(self.restored_at),
        )
    }
}

/// How urgently `event` wants the device back. `live` is whether its account
/// is on screen, `open_chat` that account's open chat, and `syncing` whether
/// history is arriving, whose chats and pages are no reason to draw. A call
/// takes the window whichever account it belongs to.
pub fn wake_for(event: &Event, live: bool, open_chat: Option<&str>, syncing: bool) -> Wake {
    let open = |chat: &String| open_chat == Some(chat.as_str());
    match event {
        Event::Call(_) | Event::PhoneOnlyCall(_) => Wake::Now,
        _ if !live => Wake::No,
        Event::Incoming { .. } | Event::Link(_) => Wake::Soon,
        Event::ChatUpdated(_) if !syncing => Wake::Soon,
        Event::Messages {
            chat, older: false, ..
        } if !syncing && open(chat) => Wake::Soon,
        Event::Typing { chat, .. } if open(chat) => Wake::Soon,
        Event::Presence { id, .. } if open(id) => Wake::Soon,
        _ => Wake::No,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::LinkStatus;

    const IDLE: Duration = Duration::from_secs(60);

    fn secs(seconds: u64) -> Duration {
        Duration::from_secs(seconds)
    }

    #[test]
    fn releases_only_after_the_idle_wait_with_nothing_busy() {
        let start = Instant::now();
        let idle = GpuIdle::default();
        // Focused: never.
        assert_eq!(
            idle.release_after(start + secs(600), None, IDLE, false),
            None
        );
        // Unfocused, not long enough: look again when the wait is over.
        assert_eq!(
            idle.release_after(start + secs(20), Some(start), IDLE, false),
            Some(secs(40))
        );
        assert_eq!(
            idle.release_after(start + secs(60), Some(start), IDLE, false),
            Some(Duration::ZERO)
        );
        // Something plays or is sent: not while it lasts.
        assert_eq!(
            idle.release_after(start + secs(600), Some(start), IDLE, true),
            None
        );
    }

    #[test]
    fn a_visible_change_or_a_restore_holds_the_device_a_while() {
        let start = Instant::now();
        let mut idle = GpuIdle::default();
        idle.note(Wake::Soon, start + secs(50));
        assert_eq!(
            idle.release_after(start + secs(60), Some(start), IDLE, false),
            Some(secs(20))
        );
        assert_eq!(
            idle.release_after(start + secs(80), Some(start), IDLE, false),
            Some(Duration::ZERO)
        );
        // Back for a hover, it stays a while rather than going at once.
        idle.observe(State::Released, start + secs(90));
        idle.observe(State::Active, start + secs(100));
        assert_eq!(
            idle.release_after(start + secs(110), Some(start), IDLE, false),
            Some(secs(20))
        );
    }

    #[test]
    fn asks_once_and_never_while_going_or_gone() {
        let start = Instant::now();
        let mut idle = GpuIdle::default();
        let later = start + secs(61);
        assert_eq!(
            idle.release_after(later, Some(start), IDLE, false),
            Some(Duration::ZERO)
        );
        idle.released(false);
        assert_eq!(idle.release_after(later, Some(start), IDLE, false), None);
        idle.observe(State::LowRes, later);
        assert_eq!(idle.release_after(later, Some(start), IDLE, false), None);
        idle.observe(State::Released, later);
        assert_eq!(idle.release_after(later, Some(start), IDLE, false), None);
    }

    #[test]
    fn wakes_at_most_once_per_gap_and_a_call_at_once() {
        let start = Instant::now();
        let mut idle = GpuIdle::default();
        // While the device is here, nothing is asked back.
        idle.note(Wake::Soon, start);
        assert_eq!(idle.wake(start, true), (None, None));
        idle.released(false);
        idle.observe(State::Released, start);
        assert_eq!(idle.wake(start, true), (None, None));
        idle.note(Wake::Soon, start);
        assert_eq!(idle.wake(start, true), (Some(Change::Request), None));
        // eframe has not brought it back yet; another message waits the gap.
        idle.note(Wake::Soon, start + secs(1));
        assert_eq!(idle.wake(start + secs(1), true), (None, Some(secs(1))));
        assert_eq!(
            idle.wake(start + secs(2), true),
            (Some(Change::Request), None)
        );
        // A ringing call does not wait.
        idle.note(Wake::Now, start + secs(3));
        assert_eq!(
            idle.wake(start + secs(3), true),
            (Some(Change::Request), None)
        );
        assert_eq!(idle.wake(start + secs(3), true), (None, None));
    }

    #[test]
    fn a_hidden_window_waits_to_be_shown_except_for_a_call() {
        let start = Instant::now();
        let mut idle = GpuIdle::default();
        idle.released(false);
        idle.observe(State::Released, start);
        idle.note(Wake::Soon, start);
        assert_eq!(idle.wake(start, false), (None, None));
        idle.note(Wake::Now, start);
        assert_eq!(idle.wake(start, false), (Some(Change::Request), None));
    }

    #[test]
    fn a_change_while_the_release_is_under_way_is_kept() {
        let start = Instant::now();
        let mut idle = GpuIdle::default();
        idle.released(false);
        idle.note(Wake::Soon, start);
        idle.observe(State::LowRes, start);
        assert_eq!(idle.wake(start, true), (Some(Change::Request), None));
        // Back with the device, nothing is left waiting.
        idle.note(Wake::Soon, start + secs(1));
        idle.observe(State::Active, start + secs(1));
        assert_eq!(idle.wake(start + secs(5), true), (None, None));
    }

    #[test]
    fn tells_once_that_the_device_went() {
        let start = Instant::now();
        let mut idle = GpuIdle::default();
        assert!(!idle.take_gone());
        idle.released(false);
        idle.observe(State::LowRes, start);
        assert!(!idle.take_gone());
        idle.observe(State::Released, start);
        assert!(idle.take_gone());
        assert!(!idle.take_gone());
        // Gone and back before anyone looked: nothing to tell.
        idle.observe(State::Active, start + secs(1));
        idle.observe(State::Released, start + secs(2));
        idle.observe(State::Active, start + secs(3));
        assert!(!idle.take_gone());
    }

    #[test]
    fn the_lock_changing_while_released_wakes_at_once() {
        let start = Instant::now();
        let mut idle = GpuIdle::default();
        idle.released(false);
        idle.observe(State::Released, start);
        idle.note_lock(false, start);
        assert_eq!(idle.wake(start, true), (None, None));
        idle.note_lock(true, start);
        assert_eq!(idle.wake(start, true), (Some(Change::Request), None));
    }

    #[test]
    fn only_visible_events_of_the_account_on_screen_wake() {
        let typing = |chat: &str| Event::Typing {
            chat: chat.into(),
            sender: "someone".into(),
            composing: true,
        };
        let page = |chat: &str, older| Event::Messages {
            chat: chat.into(),
            messages: Vec::new(),
            older,
            complete: false,
        };
        let open = Some("open");
        assert_eq!(wake_for(&typing("open"), true, open, false), Wake::Soon);
        assert_eq!(wake_for(&typing("other"), true, open, false), Wake::No);
        assert_eq!(wake_for(&typing("open"), false, open, false), Wake::No);
        assert_eq!(
            wake_for(&page("open", false), true, open, false),
            Wake::Soon
        );
        assert_eq!(wake_for(&page("open", true), true, open, false), Wake::No);
        assert_eq!(wake_for(&page("other", false), true, open, false), Wake::No);
        // History sync's pages and chats draw nothing new.
        assert_eq!(wake_for(&page("open", false), true, open, true), Wake::No);
        assert_eq!(
            wake_for(&Event::SyncProgress(40), true, open, false),
            Wake::No
        );
        // The connection banner follows every change of link.
        for status in [LinkStatus::Connecting, LinkStatus::Connected] {
            assert_eq!(
                wake_for(&Event::Link(status), true, open, false),
                Wake::Soon
            );
        }
        assert_eq!(
            wake_for(&Event::Link(LinkStatus::Connecting), false, open, false),
            Wake::No
        );
    }

    #[test]
    fn describes_the_state_for_the_memory_probe() {
        let start = Instant::now();
        let mut idle = GpuIdle::default();
        assert_eq!(
            idle.describe(start),
            "gpu=active gpu_released_s=- gpu_restored_s=-"
        );
        idle.observe(State::Released, start);
        assert_eq!(
            idle.describe(start + secs(90)),
            "gpu=released gpu_released_s=90 gpu_restored_s=-"
        );
    }
}
