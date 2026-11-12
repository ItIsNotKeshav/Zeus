//! Pacing for decorative animations while a screen reader is attached.
//!
//! On Windows a window that redraws every frame keeps its thread busy
//! drawing or blocked in the vsync swap, and that thread is the only one
//! that can answer the `WM_GETOBJECT` messages a screen reader sends to the
//! window. Each one waits for the next frame, a key press needs many, and
//! NVDA freezes for a second or two per key press while, say, a contact is
//! typing. So while a reader is attached, animations that only show that
//! something is happening (typing dots, spinners, the recording light) draw
//! about ten frames a second. Their phase follows the clock, so they move in
//! steps rather than slow down. Without a reader every request is exactly
//! what it was. Video and other content keep their own rate.
//!
//! While the window is minimised or another window has the focus, these
//! animations ask for no frames at all: they hold still until something
//! else draws the window, and move again as soon as it is back in front.
//!
//! [`Spring`] is the shared spring for one-shot motion, parametrised as
//! SwiftUI's `spring(duration:bounce:)`.
//!
//! Settings' Motion decides whether the interface moves at all
//! ([`reduced`]): Full always moves, Reduced lands every change at once, and
//! Auto reduces motion while the system asks for less (macOS's Reduce
//! motion, Windows' Animation effects off, GNOME's Animations off or the
//! desktop portal's `reduced-motion`) or a screen reader is attached. The
//! pacing above follows the screen reader alone, whatever Motion says.

#[cfg(test)]
use std::cell::Cell;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

/// How much the interface moves, from Settings > Appearance > Motion.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MotionPreference {
    /// As the system asks: reduced while it asks for less motion or a screen
    /// reader is attached.
    #[default]
    Auto,
    /// Every animation, whatever the system asks.
    Full,
    /// As little movement as possible: changes land at once.
    Reduced,
}

impl MotionPreference {
    pub const ALL: [MotionPreference; 3] = [Self::Auto, Self::Full, Self::Reduced];

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Full => "Full",
            Self::Reduced => "Reduced",
        }
    }

    /// The preference as [`PREFERENCE`] keeps it.
    const fn bits(self) -> u8 {
        match self {
            Self::Auto => 0,
            Self::Full => 1,
            Self::Reduced => 2,
        }
    }

    /// The preference [`bits`](Self::bits) stored, Auto for anything else.
    const fn from_bits(bits: u8) -> Self {
        match bits {
            1 => Self::Full,
            2 => Self::Reduced,
            _ => Self::Auto,
        }
    }
}

/// Settings' Motion, for [`reduced`].
#[cfg(not(test))]
static PREFERENCE: AtomicU8 = AtomicU8::new(MotionPreference::Auto.bits());
#[cfg(test)]
thread_local! {
    /// Each test runs on a thread of its own, so one test's Motion cannot
    /// change what another one draws.
    static PREFERENCE: AtomicU8 = const { AtomicU8::new(MotionPreference::Auto.bits()) };
    /// What [`system_reduces_motion`] answers on this thread: never reduced
    /// unless a test says so ([`override_system_motion`]), so no test depends
    /// on the machine it runs on. `None` reads the system.
    static SYSTEM_OVERRIDE: Cell<Option<bool>> = const { Cell::new(Some(false)) };
}

/// Records Settings' Motion for [`reduced`]: the app does so as a window
/// attaches and whenever the setting changes.
pub fn set_preference(preference: MotionPreference) {
    let bits = preference.bits();
    #[cfg(not(test))]
    PREFERENCE.store(bits, Ordering::Relaxed);
    #[cfg(test)]
    PREFERENCE.with(|stored| stored.store(bits, Ordering::Relaxed));
}

/// Settings' Motion, as last recorded by [`set_preference`].
pub fn preference() -> MotionPreference {
    #[cfg(not(test))]
    let bits = PREFERENCE.load(Ordering::Relaxed);
    #[cfg(test)]
    let bits = PREFERENCE.with(|stored| stored.load(Ordering::Relaxed));
    MotionPreference::from_bits(bits)
}

/// Whether the interface should hold still: never at Full, always at
/// Reduced, and at Auto while the system asks for less motion
/// ([`system_reduces_motion`]) or a screen reader is attached. Every
/// animation that moves something asks here, and lands at its end at once
/// when it answers yes.
pub fn reduced(ctx: &egui::Context) -> bool {
    match preference() {
        MotionPreference::Full => false,
        MotionPreference::Reduced => true,
        MotionPreference::Auto => system_reduces_motion() || screen_reader_attached(ctx),
    }
}

/// Whether the system asks apps for less motion: macOS's Reduce motion,
/// Windows' Animation effects turned off, or on Linux GNOME's Animations off
/// or the desktop portal's `reduced-motion`. Read at most every two seconds,
/// since animations ask many times a frame.
pub fn system_reduces_motion() -> bool {
    use std::sync::Mutex;
    use std::time::Instant;
    #[cfg(test)]
    if let Some(reduced) = SYSTEM_OVERRIDE.with(Cell::get) {
        return reduced;
    }
    static READ: Mutex<Option<(Instant, bool)>> = Mutex::new(None);
    let mut read = READ.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    match *read {
        Some((at, reduced)) if at.elapsed() < Duration::from_secs(2) => reduced,
        _ => {
            let reduced = platform::reduces_motion();
            *read = Some((Instant::now(), reduced));
            reduced
        }
    }
}

/// Makes [`system_reduces_motion`] answer `reduced` on this thread until the
/// guard it returns is dropped.
#[cfg(test)]
#[must_use]
pub(crate) fn override_system_motion(reduced: bool) -> SystemMotionOverride {
    SystemMotionOverride(SYSTEM_OVERRIDE.with(|cell| cell.replace(Some(reduced))))
}

/// Puts back what [`override_system_motion`] replaced.
#[cfg(test)]
pub(crate) struct SystemMotionOverride(Option<bool>);

#[cfg(test)]
impl Drop for SystemMotionOverride {
    fn drop(&mut self) {
        SYSTEM_OVERRIDE.with(|cell| cell.set(self.0));
    }
}

#[cfg(target_os = "macos")]
mod platform {
    /// Accessibility's Reduce motion.
    pub fn reduces_motion() -> bool {
        use objc2::runtime::AnyObject;
        use objc2::{class, msg_send};
        // SAFETY: the shared workspace and this getter (macOS 10.12 and
        // later) may be used from any thread; a missing workspace is null.
        unsafe {
            let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
            match workspace.as_ref() {
                Some(workspace) => msg_send![workspace, accessibilityDisplayShouldReduceMotion],
                None => false,
            }
        }
    }
}

#[cfg(windows)]
mod platform {
    /// Accessibility's Animation effects (Visual effects), off. Windows
    /// answers through `SPI_GETCLIENTAREAANIMATION`; an unanswered question
    /// counts as animations on, as Windows has them by default.
    pub fn reduces_motion() -> bool {
        use windows::Win32::UI::WindowsAndMessaging::{
            SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
        };
        // A Win32 BOOL.
        let mut animate: i32 = 1;
        // SAFETY: this action writes one BOOL through the pointer, which
        // outlives the call, and changes no setting.
        let read = unsafe {
            SystemParametersInfoW(
                SPI_GETCLIENTAREAANIMATION,
                0,
                Some((&raw mut animate).cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
        };
        read.is_ok() && animate == 0
    }
}

/// The desktop's motion setting, from the desktop portal: its own
/// `org.freedesktop.appearance reduced-motion` where the portal has it, and
/// GNOME's `org.gnome.desktop.interface enable-animations`, which GNOME's
/// and the GTK portal serve. A thread of its own reads both once and follows
/// their change signal, so a slow or absent session bus never holds up the
/// window and nothing polls. A change shows from the next frame drawn.
#[cfg(target_os = "linux")]
mod platform {
    use std::sync::Once;
    use std::sync::atomic::{AtomicBool, Ordering};

    use zbus::zvariant::{OwnedValue, Value};

    const APPEARANCE: &str = "org.freedesktop.appearance";
    const REDUCED_MOTION: &str = "reduced-motion";
    const GNOME: &str = "org.gnome.desktop.interface";
    const ANIMATIONS: &str = "enable-animations";

    /// The portal's own key asks for reduced motion.
    static PORTAL_REDUCED: AtomicBool = AtomicBool::new(false);
    /// GNOME's Animations switch is off.
    static GNOME_STILL: AtomicBool = AtomicBool::new(false);
    static WATCH: Once = Once::new();

    pub fn reduces_motion() -> bool {
        WATCH.call_once(spawn);
        PORTAL_REDUCED.load(Ordering::Relaxed) || GNOME_STILL.load(Ordering::Relaxed)
    }

    fn spawn() {
        let spawned = std::thread::Builder::new()
            .name("zapfast-motion".to_owned())
            .spawn(|| {
                if let Err(error) = watch() {
                    log::debug!("the desktop's motion setting is unavailable: {error}");
                }
            });
        if let Err(error) = spawned {
            log::warn!("unable to watch the desktop's motion setting: {error}");
        }
    }

    fn watch() -> zbus::Result<()> {
        let connection = zbus::blocking::Connection::session()?;
        let proxy = zbus::blocking::Proxy::new(
            &connection,
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Settings",
        )?;
        // Subscribe before reading, so a change in between is not missed.
        let changes = proxy.receive_signal("SettingChanged")?;
        for (namespace, key) in [(APPEARANCE, REDUCED_MOTION), (GNOME, ANIMATIONS)] {
            // `ReadOne` arrived in version 2 of the interface; `Read` wraps
            // the same answer in one more variant.
            let current: zbus::Result<OwnedValue> = proxy
                .call("ReadOne", &(namespace, key))
                .or_else(|_| proxy.call("Read", &(namespace, key)));
            if let Ok(value) = current {
                record(namespace, key, &value);
            }
        }
        for message in changes {
            let Ok((namespace, key, value)) =
                message.body().deserialize::<(String, String, OwnedValue)>()
            else {
                continue;
            };
            record(&namespace, &key, &value);
        }
        Ok(())
    }

    fn record(namespace: &str, key: &str, value: &Value<'_>) {
        match (namespace, key) {
            (APPEARANCE, REDUCED_MOTION) => {
                if let Some(reduced) = portal_reduced(value) {
                    PORTAL_REDUCED.store(reduced, Ordering::Relaxed);
                }
            }
            (GNOME, ANIMATIONS) => {
                if let Some(on) = switch(value) {
                    GNOME_STILL.store(!on, Ordering::Relaxed);
                }
            }
            _ => {}
        }
    }

    /// The portal's `reduced-motion`: 1 asks for reduced motion, 0 states no
    /// preference.
    pub(super) fn portal_reduced(value: &Value<'_>) -> Option<bool> {
        match value {
            Value::U32(value) => Some(*value == 1),
            Value::Value(inner) => portal_reduced(inner),
            _ => None,
        }
    }

    /// A boolean setting, through the one or two variants it arrives in.
    pub(super) fn switch(value: &Value<'_>) -> Option<bool> {
        match value {
            Value::Bool(on) => Some(*on),
            Value::Value(inner) => switch(inner),
            _ => None,
        }
    }
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
mod platform {
    pub fn reduces_motion() -> bool {
        false
    }
}

/// The wait between frames of a stepped animation: about ten frames a
/// second, which leaves the window's thread about as free as when idle.
pub const STEP: Duration = Duration::from_millis(100);

/// Leave an idle text cursor visible instead of repainting it indefinitely.
const CURSOR_BLINK_IDLE: f64 = 5.0;

#[derive(Clone, Default)]
struct CursorActivity {
    focus: Option<egui::Id>,
    last_input: Option<f64>,
}

impl CursorActivity {
    fn blinking(&mut self, now: f64, focus: Option<egui::Id>, input: bool) -> bool {
        if focus.is_none() {
            self.focus = None;
            self.last_input = None;
            return false;
        }
        if input || self.focus != focus || self.last_input.is_none() {
            self.focus = focus;
            self.last_input = Some(now);
        }
        now - self.last_input.unwrap_or(now) < CURSOR_BLINK_IDLE
    }
}

/// Restores the context style after the frame, including on early returns.
pub struct CursorStyleGuard(egui::Context);

impl Drop for CursorStyleGuard {
    fn drop(&mut self) {
        self.0
            .global_style_mut(|style| style.visuals.text_cursor.blink = true);
    }
}

/// Stop blinking after inactivity, and restart on input or a focus change.
/// The state belongs to the egui context so a replacement window starts fresh.
/// Keep the returned guard alive while drawing, including top-level modals.
pub fn idle_text_cursor(ui: &mut egui::Ui, user_input: bool) -> Option<CursorStyleGuard> {
    if !ui.visuals().text_cursor.blink {
        return None;
    }
    let ctx = ui.ctx().clone();
    let (now, window_focused) = ctx.input(|input| (input.time, input.focused));
    let focus = if window_focused {
        ctx.memory(|memory| memory.focused())
    } else {
        None
    };
    let id = egui::Id::new("idle-text-cursor");
    let mut activity = ctx.data_mut(|data| data.get_temp::<CursorActivity>(id).unwrap_or_default());
    let blink = activity.blinking(now, focus, user_input);
    ctx.data_mut(|data| data.insert_temp(id, activity));
    ui.style_mut().visuals.text_cursor.blink = blink;
    if !blink && ctx.global_style().visuals.text_cursor.blink {
        ctx.global_style_mut(|style| style.visuals.text_cursor.blink = false);
        Some(CursorStyleGuard(ctx))
    } else {
        None
    }
}

/// Whether a screen reader, or another assistive client, reads the window.
/// egui has no public flag, but it builds accessibility nodes only while
/// AccessKit is active, and the root's node always exists then, so probing
/// it adds nothing to the tree.
pub fn screen_reader_attached(ctx: &egui::Context) -> bool {
    ctx.accesskit_node_builder(egui::accesskit_root_id(), |_| ())
        .is_some()
}

/// Whether the window is minimised or in the background, when looping
/// decoration rests.
pub fn window_resting(ctx: &egui::Context) -> bool {
    ctx.input(|input| {
        let viewport = input.viewport();
        viewport.minimized == Some(true) || viewport.focused == Some(false)
    })
}

/// Asks for the next frame of a decorative animation: at the next refresh,
/// or a [`STEP`] later while a screen reader is attached; none while the
/// window rests.
#[track_caller]
pub fn request_frame(ctx: &egui::Context) {
    if window_resting(ctx) {
        return;
    }
    if screen_reader_attached(ctx) {
        ctx.request_repaint_after(STEP);
    } else {
        ctx.request_repaint();
    }
}

/// Like [`request_frame`], for an animation that already waits `delay`
/// between frames: at least a [`STEP`] while a screen reader is attached,
/// none while the window rests.
#[track_caller]
pub fn request_frame_after(ctx: &egui::Context, delay: Duration) {
    if window_resting(ctx) {
        return;
    }
    ctx.request_repaint_after(frame_delay(ctx, delay));
}

/// `delay`, or at least a [`STEP`] while a screen reader is attached.
pub fn frame_delay(ctx: &egui::Context, delay: Duration) -> Duration {
    if screen_reader_attached(ctx) {
        delay.max(STEP)
    } else {
        delay
    }
}

/// egui's own spinner, which asks for every frame, stepped while a screen
/// reader is attached. Without one this is exactly `ui.spinner()`.
pub fn egui_spinner(ui: &mut egui::Ui) -> egui::Response {
    if !screen_reader_attached(ui.ctx()) && !window_resting(ui.ctx()) {
        return ui.spinner();
    }
    // The same size, node and stroke as `egui::Spinner`, but stepped.
    let size = ui.style().spacing.interact_size.y;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    response.widget_info(|| egui::WidgetInfo::new(egui::WidgetType::ProgressIndicator));
    if ui.is_rect_visible(rect) {
        request_frame(ui.ctx());
        let color = ui.visuals().strong_text_color();
        let radius = (rect.height().min(rect.width()) / 2.0) - 2.0;
        let n_points = (radius.round() as u32).clamp(8, 128);
        let time = ui.input(|input| input.time);
        let start_angle = time * std::f64::consts::TAU;
        let end_angle = start_angle + 240_f64.to_radians() * time.sin();
        let points = (0..n_points)
            .map(|index| {
                let angle = egui::emath::lerp(
                    start_angle..=end_angle,
                    f64::from(index) / f64::from(n_points),
                );
                let (sin, cos) = angle.sin_cos();
                rect.center() + radius * egui::vec2(cos as f32, sin as f32)
            })
            .collect();
        ui.painter()
            .add(egui::Shape::line(points, egui::Stroke::new(3.0, color)));
    }
    response
}

thread_local! {
    /// The share of its tuned time every spring and timed animation takes on
    /// this thread, from Settings' Motion: 1 as tuned, less to run faster.
    /// Per thread, as the interface draws on one and each test on its own.
    static SPEED: std::cell::Cell<f64> = const { std::cell::Cell::new(1.0) };
}

/// Sets the share of its tuned time every spring and timed animation takes.
pub fn set_speed(factor: f64) {
    SPEED.with(|speed| speed.set(factor.clamp(0.1, 4.0)));
}

/// The share of its tuned time every spring and timed animation takes.
pub fn speed() -> f64 {
    SPEED.with(std::cell::Cell::get)
}

/// How near its target a [`Spring`] counts as at rest, as a share of the
/// distance it travels: a third of a point over 300 points.
const SETTLED: f64 = 1e-3;

/// A spring as SwiftUI's `spring(duration:bounce:)` describes one: a damped
/// harmonic oscillator of unit mass whose undamped period, its response, is
/// `duration` seconds, damped at `1 - bounce` of critical. Bounce 0 is
/// critically damped and never passes its target; 0.2 passes it by about
/// 1.5% before it settles. Bounces are taken from 0 to 0.9.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    duration: f64,
    bounce: f64,
}

impl Spring {
    pub const fn new(duration: f64, bounce: f64) -> Self {
        Self { duration, bounce }
    }

    /// The undamped angular frequency, `2π / duration`, with the duration
    /// scaled by Settings' Motion ([`speed`]).
    fn omega(self) -> f64 {
        std::f64::consts::TAU / (self.duration * speed()).max(1e-3)
    }

    /// The damping ratio, 1 for no bounce.
    fn zeta(self) -> f64 {
        1.0 - self.bounce.clamp(0.0, 0.9)
    }

    /// Where a step from 0 to 1, begun at rest, is `t` seconds in: 0 before
    /// it starts, and exactly 1 once it has settled, so the last frame lands
    /// on the target.
    pub fn value(self, t: f64) -> f32 {
        if t <= 0.0 {
            return 0.0;
        }
        if self.settled(t) {
            return 1.0;
        }
        let omega = self.omega();
        let zeta = self.zeta();
        let decay = (-zeta * omega * t).exp();
        let left = if zeta >= 1.0 {
            decay * (1.0 + omega * t)
        } else {
            let damped = omega * (1.0 - zeta * zeta).sqrt();
            decay * ((damped * t).cos() + zeta * omega / damped * (damped * t).sin())
        };
        (1.0 - left) as f32
    }

    /// The furthest the spring can still be from its target `t` seconds in:
    /// the envelope its swing decays inside. It only ever falls.
    fn envelope(self, t: f64) -> f64 {
        let omega = self.omega();
        let zeta = self.zeta();
        if zeta >= 1.0 {
            (1.0 + omega * t) * (-omega * t).exp()
        } else {
            (-zeta * omega * t).exp() / (1.0 - zeta * zeta).sqrt()
        }
    }

    /// Whether the spring has come to rest `t` seconds in.
    pub fn settled(self, t: f64) -> bool {
        t > 0.0 && self.envelope(t) <= SETTLED
    }

    /// Where a spring pulling toward 0, `position` away from it and moving
    /// at `velocity` (per second), is `dt` seconds later, and how fast it
    /// moves then. Exact for any step, so a late frame lands where an early
    /// one would have.
    pub fn advance(self, position: f32, velocity: f32, dt: f64) -> (f32, f32) {
        if dt <= 0.0 {
            return (position, velocity);
        }
        let (x0, v0) = (f64::from(position), f64::from(velocity));
        let omega = self.omega();
        let zeta = self.zeta();
        let decay_rate = zeta * omega;
        let decay = (-decay_rate * dt).exp();
        let (x, v) = if zeta >= 1.0 {
            let slope = v0 + omega * x0;
            (decay * (x0 + slope * dt), decay * (v0 - omega * slope * dt))
        } else {
            let damped = omega * (1.0 - zeta * zeta).sqrt();
            let (sin, cos) = (damped * dt).sin_cos();
            (
                decay * (x0 * cos + (v0 + decay_rate * x0) / damped * sin),
                decay * (v0 * cos - (decay_rate * v0 + omega * omega * x0) / damped * sin),
            )
        };
        (x as f32, v as f32)
    }

    /// How long the spring takes to come to rest, in seconds.
    pub fn settle_time(self) -> f64 {
        let (mut low, mut high) = (0.0, (self.duration * speed()).max(1e-3));
        while self.envelope(high) > SETTLED {
            high *= 2.0;
        }
        for _ in 0..40 {
            let middle = (low + high) / 2.0;
            if self.envelope(middle) > SETTLED {
                low = middle;
            } else {
                high = middle;
            }
        }
        high
    }
}

impl Spring {
    /// Controls that answer a touch: hovers, presses, toggles, menus. A hair
    /// of bounce, like iOS's `.snappy`.
    pub const SNAPPY: Spring = Spring::new(0.3, 0.12);
    /// What most things move on: panels, cards, list changes. No bounce,
    /// like iOS's `.smooth`.
    pub const DEFAULT: Spring = Spring::new(0.45, 0.0);
    /// Large, slow changes: whole-window moves, sheets.
    pub const CALM: Spring = Spring::new(0.7, 0.0);

    /// A critically damped spring that has come to rest after about
    /// `seconds`, for moves that keep a fixed time but follow their
    /// target's changes smoothly, with the velocity they have.
    pub const fn settling_in(seconds: f64) -> Spring {
        // A critically damped spring is within SETTLED of its target after
        // about 1.46 of its durations.
        Spring::new(seconds / 1.46, 0.0)
    }
}

/// Where a [`spring_to`] value is, and how fast it moves.
#[derive(Clone, Copy)]
struct Follow {
    value: f32,
    velocity: f32,
    time: f64,
}

/// Moves a value toward `target` on `spring` and returns where it is this
/// frame. `id` names the value between frames. A new `target` mid-move turns
/// the value around with the velocity it has, so a hover let go of halfway
/// reverses smoothly instead of restarting. The first frame an id is seen it
/// is already at `target`. While motion is [`reduced`] (a screen reader
/// attached, at Auto) it is always there. Asks for frames until it has
/// settled.
pub fn spring_to(ctx: &egui::Context, id: egui::Id, target: f32, spring: Spring) -> f32 {
    spring_from(ctx, id, target, target, spring)
}

/// [`spring_to`] for a value that starts at `start` the first frame its id is
/// seen, so something that appears moves in from there. [`forget`] makes the
/// next frame a first one again.
pub fn spring_from(
    ctx: &egui::Context,
    id: egui::Id,
    target: f32,
    start: f32,
    spring: Spring,
) -> f32 {
    if reduced(ctx) {
        ctx.data_mut(|data| data.remove::<Follow>(id));
        return target;
    }
    let now = ctx.input(|input| input.time);
    let mut follow = ctx
        .data_mut(|data| data.get_temp::<Follow>(id))
        .unwrap_or(Follow {
            value: start,
            velocity: 0.0,
            time: now,
        });
    let (offset, velocity) = spring.advance(
        follow.value - target,
        follow.velocity,
        (now - follow.time).max(0.0),
    );
    // Close and slow enough counts as there, so the last frame lands exactly.
    let rest = offset.abs() < 5e-4 && velocity.abs() < 5e-3;
    follow = if rest {
        Follow {
            value: target,
            velocity: 0.0,
            time: now,
        }
    } else {
        Follow {
            value: target + offset,
            velocity,
            time: now,
        }
    };
    // A value at rest stays remembered, at its target: forgetting it would
    // make the next frame a first one, and something that appears would
    // start over from `start`, again and again.
    ctx.data_mut(|data| data.insert_temp(id, follow));
    if !rest {
        ctx.request_repaint();
    }
    follow.value
}

/// Drops what [`spring_to`] holds for `id`, so its next frame starts anew.
pub fn forget(ctx: &egui::Context, id: egui::Id) {
    ctx.data_mut(|data| data.remove::<Follow>(id));
}

/// A cubic Bézier easing curve, as CSS and Core Animation write them: the
/// control points `(x1, y1)` and `(x2, y2)` between `(0, 0)` and `(1, 1)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Curve {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
}

impl Curve {
    pub const fn new(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        Self { x1, y1, x2, y2 }
    }

    /// Entering: leaves fast and glides in, as iOS sheets and menus do.
    pub const ENTER: Curve = Curve::new(0.32, 0.72, 0.0, 1.0);
    /// Leaving: starts gently and picks up speed, so a surface that goes
    /// away is out of the way quickly.
    pub const EXIT: Curve = Curve::new(0.4, 0.0, 0.8, 0.4);
    /// Pressing: nearly immediate, with a short settle.
    pub const PRESS: Curve = Curve::new(0.2, 0.0, 0.0, 1.0);
    /// Moving between two resting places: eases at both ends.
    pub const STANDARD: Curve = Curve::new(0.4, 0.0, 0.2, 1.0);

    fn axis(a: f32, b: f32, s: f32) -> f32 {
        let u = 1.0 - s;
        3.0 * u * u * s * a + 3.0 * u * s * s * b + s * s * s
    }

    /// The eased progress at linear progress `t` (clamped to 0 to 1).
    pub fn at(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        if t == 0.0 || t == 1.0 {
            return t;
        }
        // Find the curve parameter whose x is `t`: Newton, then bisection.
        let mut s = t;
        for _ in 0..8 {
            let error = Self::axis(self.x1, self.x2, s) - t;
            if error.abs() < 1e-5 {
                return Self::axis(self.y1, self.y2, s);
            }
            let u = 1.0 - s;
            let slope = 3.0 * u * u * self.x1
                + 6.0 * u * s * (self.x2 - self.x1)
                + 3.0 * s * s * (1.0 - self.x2);
            if slope.abs() < 1e-6 {
                break;
            }
            s -= error / slope;
        }
        let (mut low, mut high) = (0.0_f32, 1.0_f32);
        s = t;
        for _ in 0..24 {
            let x = Self::axis(self.x1, self.x2, s);
            if (x - t).abs() < 1e-5 {
                break;
            }
            if x < t {
                low = s;
            } else {
                high = s;
            }
            s = (low + high) / 2.0;
        }
        Self::axis(self.y1, self.y2, s)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Settings' Motion at Fast settles every spring in 70% of its tuned
    /// time, and Default puts it back.
    #[test]
    fn fast_motion_settles_springs_sooner() {
        let spring = Spring::new(0.35, 0.0);
        let tuned = spring.settle_time();
        set_speed(crate::settings::MotionSpeed::Fast.factor());
        let fast = spring.settle_time();
        let halfway = spring.value(tuned * 0.35);
        set_speed(crate::settings::MotionSpeed::Default.factor());
        assert!((fast / tuned - 0.7).abs() < 1e-3, "{fast} of {tuned}");
        assert!(
            halfway > spring.value(tuned * 0.35),
            "further along when fast"
        );
        assert_eq!(spring.settle_time(), tuned);
    }

    #[test]
    fn idle_cursor_stays_visible_and_resumes_on_input_or_focus_change() {
        let first = egui::Id::new("first");
        let second = egui::Id::new("second");
        let mut activity = CursorActivity::default();
        assert!(!activity.blinking(0.0, None, false));
        assert!(activity.blinking(1.0, Some(first), false));
        assert!(activity.blinking(5.9, Some(first), false));
        assert!(!activity.blinking(6.0, Some(first), false));
        assert!(activity.blinking(7.0, Some(first), true));
        assert!(!activity.blinking(12.0, Some(first), false));
        assert!(activity.blinking(12.0, Some(second), false));
        assert!(!activity.blinking(13.0, None, false));
        assert!(activity.blinking(30.0, Some(second), false));
    }

    #[test]
    fn idle_text_field_does_not_keep_requesting_cursor_repaints() {
        let ctx = reading(false);
        let field = egui::Id::new("idle-field");
        ctx.memory_mut(|memory| memory.request_focus(field));
        let mut text = String::new();
        let mut draw = |ui: &mut egui::Ui| {
            let _cursor_style = idle_text_cursor(ui, false);
            ui.add(egui::TextEdit::singleline(&mut text).id(field));
        };
        let _ = delay_after(&ctx, 1.0, &mut draw);
        let (idle, _) = delay_after(&ctx, 7.0, &mut draw);
        assert!(idle > Duration::from_secs(1), "idle redraw in {idle:?}");

        let (active, _) = delay_after(&ctx, 8.0, |ui| {
            let _cursor_style = idle_text_cursor(ui, true);
            ui.add(egui::TextEdit::singleline(&mut text).id(field));
        });
        assert!(active < Duration::from_secs(1), "cursor did not resume");
    }

    #[test]
    fn idle_modal_field_stops_blinking_and_restores_the_context_style() {
        let ctx = reading(false);
        let field = egui::Id::new("modal-field");
        let mut text = String::new();
        let mut draw = |ui: &mut egui::Ui| {
            let _cursor_style = idle_text_cursor(ui, false);
            egui::Modal::new(egui::Id::new("modal")).show(ui.ctx(), |ui| {
                ui.add(egui::TextEdit::singleline(&mut text).id(field));
            });
        };
        ctx.memory_mut(|memory| memory.request_focus(field));
        let _ = delay_after(&ctx, 1.0, &mut draw);
        let _ = delay_after(&ctx, 2.0, &mut draw);
        let (idle, _) = delay_after(&ctx, 7.0, &mut draw);
        assert!(idle > Duration::from_secs(1), "modal redraw in {idle:?}");
        assert!(ctx.global_style().visuals.text_cursor.blink);
    }

    /// Runs one pass at `time` and returns how long egui asks the window to
    /// wait before the next one.
    fn delay_after(
        ctx: &egui::Context,
        time: f64,
        draw: impl FnMut(&mut egui::Ui),
    ) -> (Duration, Vec<egui::epaint::ClippedShape>) {
        let mut output = ctx.run_ui(
            egui::RawInput {
                time: Some(time),
                ..Default::default()
            },
            draw,
        );
        output.textures_delta.clear();
        (
            output.viewport_output[&egui::ViewportId::ROOT].repaint_delay,
            output.shapes,
        )
    }

    /// A context with AccessKit on or off, past the passes egui asks for
    /// by itself after starting.
    pub(crate) fn reading(reader: bool) -> egui::Context {
        let ctx = egui::Context::default();
        if reader {
            ctx.enable_accesskit();
        }
        settle(&ctx);
        ctx
    }

    /// Runs empty passes until egui asks for no further frame.
    pub(crate) fn settle(ctx: &egui::Context) {
        for _ in 0..10 {
            let mut output = ctx.run_ui(egui::RawInput::default(), |_| ());
            output.textures_delta.clear();
            if output.viewport_output[&egui::ViewportId::ROOT].repaint_delay == Duration::MAX {
                return;
            }
        }
        panic!("egui kept asking for frames");
    }

    /// egui subtracts its predicted frame time, 1/60 s by default, from
    /// every delay: a step is asked for as about 83 ms.
    fn is_a_step(delay: Duration) -> bool {
        delay > Duration::from_millis(75) && delay <= STEP
    }

    #[test]
    fn the_probe_follows_accesskit_and_adds_no_node() {
        let nodes = |reader: bool, probe: bool| {
            let ctx = reading(reader);
            let mut attached = None;
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                ui.label("text");
                if probe {
                    attached = Some(screen_reader_attached(ui.ctx()));
                }
            });
            output.textures_delta.clear();
            let count = output
                .platform_output
                .accesskit_update
                .map(|update| update.nodes.len());
            (attached, count)
        };
        assert_eq!(nodes(false, true), (Some(false), None));
        let (attached, probed) = nodes(true, true);
        assert_eq!(attached, Some(true));
        assert_eq!(probed, nodes(true, false).1);
        assert!(probed.is_some());
    }

    #[test]
    fn decorative_frames_are_immediate_without_a_reader_and_stepped_with_one() {
        let (plain, _) = delay_after(&reading(false), 0.0, |ui| request_frame(ui.ctx()));
        assert_eq!(plain, Duration::ZERO);
        let (stepped, _) = delay_after(&reading(true), 0.0, |ui| request_frame(ui.ctx()));
        assert!(is_a_step(stepped), "{stepped:?}");
    }

    #[test]
    fn decoration_rests_while_the_window_is_in_the_background() {
        for (focused, minimized) in [(false, false), (true, true)] {
            let ctx = reading(false);
            let mut input = egui::RawInput {
                time: Some(0.0),
                ..Default::default()
            };
            let viewport = input.viewports.entry(egui::ViewportId::ROOT).or_default();
            viewport.focused = Some(focused);
            viewport.minimized = Some(minimized);
            let output = ctx.run_ui(input, |ui| {
                request_frame(ui.ctx());
                request_frame_after(ui.ctx(), Duration::from_millis(33));
            });
            assert_eq!(
                output.viewport_output[&egui::ViewportId::ROOT].repaint_delay,
                Duration::MAX,
                "focused {focused}, minimised {minimized}"
            );
        }
    }

    #[test]
    fn timed_animations_wait_at_least_a_step_with_a_reader() {
        let timed = |reader: bool, delay: u64| {
            delay_after(&reading(reader), 0.0, |ui| {
                request_frame_after(ui.ctx(), Duration::from_millis(delay));
            })
            .0
        };
        let predicted = Duration::from_secs_f32(1.0 / 60.0);
        // Without a reader every delay is what was asked for.
        assert_eq!(timed(false, 33), Duration::from_millis(33) - predicted);
        assert_eq!(timed(false, 150), Duration::from_millis(150) - predicted);
        assert!(is_a_step(timed(true, 33)), "{:?}", timed(true, 33));
        // A slower animation keeps its own pace.
        assert_eq!(timed(true, 150), Duration::from_millis(150) - predicted);
    }

    /// Schedules passes the way eframe does, the next one `repaint_delay`
    /// after the previous ended, and counts them over two seconds.
    fn frames_in_two_seconds(reader: bool) -> usize {
        let ctx = reading(reader);
        // A frame of a maximised 4K window costs about this much.
        let cost = 0.010;
        let mut time = 0.0;
        let mut frames = 0;
        while time < 2.0 {
            let (delay, _) = delay_after(&ctx, time, |ui| request_frame(ui.ctx()));
            frames += 1;
            // An immediate repaint waits for the next refresh at most.
            time += cost + delay.as_secs_f64().max(1.0 / 60.0 - cost);
        }
        frames
    }

    #[test]
    fn stepped_animations_draw_about_ten_frames_a_second() {
        let stepped = frames_in_two_seconds(true);
        assert!((18..=24).contains(&stepped), "{stepped} frames in 2 s");
        let plain = frames_in_two_seconds(false);
        assert!(plain >= 118, "{plain} frames in 2 s");
    }

    #[test]
    fn the_stepped_egui_spinner_draws_egui_s_own_at_the_same_time() {
        let draw = |reader: bool, time: f64| {
            let ctx = reading(reader);
            let mut node = None;
            let (delay, shapes) = delay_after(&ctx, time, |ui| {
                let response = egui_spinner(ui);
                node = Some(response.rect);
            });
            let shapes: Vec<_> = shapes.into_iter().map(|clipped| clipped.shape).collect();
            (delay, shapes, node)
        };
        for time in [0.25, 1.7, 3.3] {
            let (plain_delay, plain, plain_rect) = draw(false, time);
            let (stepped_delay, stepped, stepped_rect) = draw(true, time);
            assert_eq!(plain_delay, Duration::ZERO);
            assert!(is_a_step(stepped_delay), "{stepped_delay:?}");
            assert_eq!(plain_rect, stepped_rect);
            assert_eq!(format!("{plain:?}"), format!("{stepped:?}"));
        }
        // And it still turns between steps.
        assert_ne!(
            format!("{:?}", draw(true, 1.0).1),
            format!("{:?}", draw(true, 1.1).1)
        );
    }

    #[test]
    fn a_spring_starts_at_rest_and_settles_on_its_target() {
        for spring in [
            Spring::new(0.5, 0.2),
            Spring::new(0.35, 0.0),
            Spring::new(0.45, 0.25),
        ] {
            assert_eq!(spring.value(-1.0), 0.0);
            assert_eq!(spring.value(0.0), 0.0);
            let first = spring.value(0.01);
            assert!(first > 0.0 && first < 0.1, "{spring:?} at 10 ms: {first}");
            let rest = spring.settle_time();
            assert!(
                rest > spring.duration && rest < spring.duration * 2.5,
                "{spring:?} settles at {rest}"
            );
            assert!(!spring.settled(rest * 0.5));
            assert!(spring.settled(rest + 1e-6));
            assert_eq!(spring.value(rest + 0.01), 1.0);
            assert_eq!(spring.value(10.0), 1.0);
            // Settling snaps by no more than the tolerance.
            assert!((spring.value(rest - 0.005) - 1.0).abs() < 2e-3);
        }
    }

    #[test]
    fn a_spring_without_bounce_never_overshoots() {
        let spring = Spring::new(0.35, 0.0);
        let mut last = 0.0;
        for step in 0..=400 {
            let value = spring.value(f64::from(step) / 400.0);
            assert!(value <= 1.0, "{value} at step {step}");
            assert!(value >= last, "{value} after {last}");
            last = value;
        }
    }

    #[test]
    fn a_bouncy_spring_overshoots_a_little() {
        let peak = |spring: Spring| {
            (0..=400)
                .map(|step| spring.value(f64::from(step) / 400.0))
                .fold(0.0_f32, f32::max)
        };
        let send = peak(Spring::new(0.5, 0.2));
        assert!(send > 1.005 && send < 1.03, "bounce 0.2 peaks at {send}");
        // More bounce, more overshoot.
        assert!(peak(Spring::new(0.45, 0.25)) > send);
    }

    #[test]
    fn advancing_a_spring_follows_its_step_and_does_not_depend_on_the_frames() {
        for spring in [Spring::new(1.0, 0.2), Spring::new(0.35, 0.0)] {
            // Released from 1 at rest, it is where the step from 0 to 1 has
            // yet to go.
            for t in [0.05, 0.2, 0.5, 0.9] {
                let (x, _) = spring.advance(1.0, 0.0, t);
                assert!(
                    (x - (1.0 - spring.value(t))).abs() < 1e-4,
                    "{spring:?} at {t}"
                );
            }
            // One long step lands where many short ones do.
            let (mut x, mut v) = (12.0_f32, -30.0_f32);
            for _ in 0..60 {
                (x, v) = spring.advance(x, v, 1.0 / 120.0);
            }
            let (once, speed) = spring.advance(12.0, -30.0, 0.5);
            assert!((x - once).abs() < 1e-3 && (v - speed).abs() < 1e-2);
            let (rest, still) = spring.advance(12.0, -30.0, 10.0);
            assert!(rest.abs() < 1e-3 && still.abs() < 1e-3);
            assert_eq!(spring.advance(3.0, 4.0, 0.0), (3.0, 4.0));
        }
    }

    #[test]
    fn curves_run_from_zero_to_one_and_enter_leads_exit() {
        for curve in [Curve::ENTER, Curve::EXIT, Curve::PRESS, Curve::STANDARD] {
            assert_eq!(curve.at(0.0), 0.0);
            assert_eq!(curve.at(1.0), 1.0);
            let mut last = 0.0;
            for step in 1..=20 {
                let y = curve.at(step as f32 / 20.0);
                assert!(y >= last - 1e-4, "{curve:?} fell at {step}");
                last = y;
            }
        }
        assert!(Curve::ENTER.at(0.3) > 0.3);
        assert!(Curve::EXIT.at(0.3) < 0.3);
    }

    #[test]
    fn a_followed_value_keeps_its_velocity_when_turned_around() {
        let ctx = egui::Context::default();
        let id = egui::Id::new("follow");
        let at = |time: f64, target: f32| {
            let mut value = 0.0;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    time: Some(time),
                    ..Default::default()
                },
                |ui| value = spring_from(ui.ctx(), id, target, 0.0, Spring::SNAPPY),
            );
            output.textures_delta.clear();
            value
        };
        assert_eq!(at(0.0, 1.0), 0.0);
        let early = at(0.05, 1.0);
        let mid = at(0.1, 1.0);
        assert!(early > 0.0 && mid > early && mid < 1.0);
        // Turned back, it carries on a little before it reverses.
        let carried = at(0.12, 0.0);
        assert!(carried > mid);
        assert_eq!(at(3.0, 0.0), 0.0);
        assert_eq!(at(6.0, 0.0), 0.0);
    }

    #[test]
    fn something_that_appeared_stays_shown_and_stops_asking_for_frames() {
        let ctx = egui::Context::default();
        let id = egui::Id::new("appear");
        let mut shown = 0.0;
        for step in 0..40 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    time: Some(f64::from(step) * 0.05),
                    ..Default::default()
                },
                |ui| shown = spring_from(ui.ctx(), id, 1.0, 0.0, Spring::SNAPPY),
            );
            output.textures_delta.clear();
        }
        assert_eq!(shown, 1.0);
        let mut output = ctx.run_ui(
            egui::RawInput {
                time: Some(5.0),
                ..Default::default()
            },
            |ui| shown = spring_from(ui.ctx(), id, 1.0, 0.0, Spring::SNAPPY),
        );
        output.textures_delta.clear();
        assert_eq!(shown, 1.0);
        assert_eq!(
            output.viewport_output[&egui::ViewportId::ROOT].repaint_delay,
            Duration::MAX
        );
    }

    /// Whether motion is reduced, asked inside a pass as the views ask.
    fn reduced_in_a_pass(ctx: &egui::Context) -> bool {
        let mut answer = None;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            answer = Some(reduced(ui.ctx()));
        });
        output.textures_delta.clear();
        answer.expect("the pass ran")
    }

    #[test]
    fn the_motion_preference_round_trips() {
        assert_eq!(preference(), MotionPreference::Auto);
        for choice in MotionPreference::ALL {
            set_preference(choice);
            assert_eq!(preference(), choice);
            assert_eq!(MotionPreference::from_bits(choice.bits()), choice);
        }
        assert_eq!(MotionPreference::from_bits(200), MotionPreference::Auto);
        set_preference(MotionPreference::Auto);
    }

    #[test]
    fn full_and_reduced_ignore_the_system_and_auto_follows_it() {
        for (system, reader) in [(false, false), (true, false), (false, true), (true, true)] {
            let _system = override_system_motion(system);
            assert_eq!(system_reduces_motion(), system);
            let ctx = reading(reader);
            set_preference(MotionPreference::Full);
            assert!(!reduced_in_a_pass(&ctx), "{system} {reader}");
            set_preference(MotionPreference::Reduced);
            assert!(reduced_in_a_pass(&ctx), "{system} {reader}");
            set_preference(MotionPreference::Auto);
            assert_eq!(
                reduced_in_a_pass(&ctx),
                system || reader,
                "{system} {reader}"
            );
        }
        // The guard puts the test default back: the system never reduces.
        assert!(!system_reduces_motion());
    }

    #[test]
    fn a_spring_lands_at_once_while_reduced_and_moves_at_full_with_a_reader() {
        let first_frame = |choice: MotionPreference, reader: bool| {
            set_preference(choice);
            let ctx = reading(reader);
            let id = egui::Id::new("motion");
            let mut shown = f32::NAN;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    time: Some(0.0),
                    ..Default::default()
                },
                |ui| shown = spring_from(ui.ctx(), id, 1.0, 0.0, Spring::SNAPPY),
            );
            output.textures_delta.clear();
            shown
        };
        assert_eq!(first_frame(MotionPreference::Reduced, false), 1.0);
        assert_eq!(first_frame(MotionPreference::Auto, true), 1.0);
        // It starts from where it appears, so it has somewhere to move to.
        assert_eq!(first_frame(MotionPreference::Auto, false), 0.0);
        assert_eq!(first_frame(MotionPreference::Full, true), 0.0);
        set_preference(MotionPreference::Auto);
    }

    #[test]
    fn a_reader_keeps_decoration_stepped_whatever_motion_says() {
        set_preference(MotionPreference::Full);
        let (stepped, _) = delay_after(&reading(true), 0.0, |ui| request_frame(ui.ctx()));
        assert!(is_a_step(stepped), "{stepped:?}");
        set_preference(MotionPreference::Auto);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_desktops_motion_settings_are_read_through_either_wrapping() {
        use zbus::zvariant::Value;
        assert_eq!(platform::portal_reduced(&Value::U32(1)), Some(true));
        assert_eq!(
            platform::portal_reduced(&Value::Value(Box::new(Value::U32(0)))),
            Some(false)
        );
        assert_eq!(platform::portal_reduced(&Value::Bool(true)), None);
        assert_eq!(platform::switch(&Value::Bool(false)), Some(false));
        assert_eq!(
            platform::switch(&Value::Value(Box::new(Value::Bool(true)))),
            Some(true)
        );
        assert_eq!(platform::switch(&Value::U32(1)), None);
    }
}
