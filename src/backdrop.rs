//! The translucent window behind the Messages theme, with what each platform
//! offers: AppKit's sidebar material on macOS, Mica through DWM on Windows,
//! and on Hyprland a transparent window its compositor blurs. Elsewhere, or
//! where the platform refuses, the window stays opaque in the theme's
//! colours, which still looks right.
//!
//! The theme draws its frosted header, glass and tints itself, everywhere;
//! only whether the desktop shows through the window is decided here, once
//! per frame, and [`active`] tells the rest of the interface.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

static ACTIVE: AtomicBool = AtomicBool::new(false);
/// [`glass`]'s bits.
#[cfg(not(test))]
static GLASS: AtomicU32 = AtomicU32::new(GLASS_DEFAULT.to_bits());
#[cfg(test)]
thread_local! {
    /// Each test runs on a thread of its own, so one test's Glass cannot
    /// change what another one draws.
    static GLASS: AtomicU32 = const { AtomicU32::new(GLASS_DEFAULT.to_bits()) };
}

/// Settings' Glass where it starts: the see-through surfaces as they always
/// were. 0 is Clear, 1 Solid.
pub const GLASS_DEFAULT: f32 = 0.3;
/// How much of its default opacity a see-through fill keeps at Clear.
const CLEAREST: f32 = 0.6;

/// How tinted glass is, from 0 (Clear) to 1 (Solid), as Settings' Glass
/// says, or Solid while the system asks for less transparency.
pub fn glass() -> f32 {
    #[cfg(not(test))]
    let bits = GLASS.load(Ordering::Relaxed);
    #[cfg(test)]
    let bits = GLASS.with(|glass| glass.load(Ordering::Relaxed));
    f32::from_bits(bits)
}

/// Whether glass is frosted: anything short of Solid. At Solid the frosted
/// header and composer, glass and window material give way to solid
/// surfaces, as the Blur switch's Off did.
///
/// Never on macOS: it draws with Metal through egui-wgpu, which paints a
/// frame in one render pass that a paint callback cannot read back, and the
/// frost (`ui/glass.rs`) blurs what is already drawn. The window's material
/// still follows Settings there (`Settings::vibrant_window`).
pub fn blur() -> bool {
    !cfg!(target_os = "macos") && glass() < 1.0
}

/// Records Settings' Glass for [`glass`]; the app does so every frame.
pub fn set_glass(tint: f32) {
    let bits = tint.clamp(0.0, 1.0).to_bits();
    #[cfg(not(test))]
    GLASS.store(bits, Ordering::Relaxed);
    #[cfg(test)]
    GLASS.with(|glass| glass.store(bits, Ordering::Relaxed));
}

/// `alpha`, how opaque a see-through fill is at [`GLASS_DEFAULT`] (0 to 1),
/// at the tint Glass has now: thinner towards Clear, opaque at Solid.
pub fn tinted(alpha: f32) -> f32 {
    tinted_at(alpha, glass())
}

/// [`tinted`] for the window's own surfaces, the chat list and the chat's
/// background: Clear leaves them as they are at the default, so only the
/// floating glass thins out, and Solid still tints them opaque.
pub fn tinted_window(alpha: f32) -> f32 {
    tinted_at(alpha, glass().max(GLASS_DEFAULT))
}

fn tinted_at(alpha: f32, tint: f32) -> f32 {
    if tint <= GLASS_DEFAULT {
        alpha * (CLEAREST + (1.0 - CLEAREST) * tint / GLASS_DEFAULT)
    } else {
        alpha + (1.0 - alpha) * (tint - GLASS_DEFAULT) / (1.0 - GLASS_DEFAULT)
    }
}

/// Whether the system asks apps for less transparency: macOS's Reduce
/// transparency, or Windows' Transparency effects turned off. Read at most
/// every two seconds, since the app asks every frame.
pub fn system_reduces_transparency() -> bool {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    static READ: Mutex<Option<(Instant, bool)>> = Mutex::new(None);
    let mut read = READ.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    match *read {
        Some((at, reduced)) if at.elapsed() < Duration::from_secs(2) => reduced,
        _ => {
            let reduced = platform::reduces_transparency();
            *read = Some((Instant::now(), reduced));
            reduced
        }
    }
}

/// Whether the desktop shows through the window this frame: the Messages
/// theme is on and the platform gave the window its material.
pub fn active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

/// Whether to make the window transparent when it is created. macOS always
/// does, since every theme there paints over it; elsewhere when the Messages
/// theme is on and the platform can show the desktop through it, and on
/// macOS or Linux when the theme's window colour itself has alpha
/// (`see_through`): such a theme has chosen to show the desktop, which a
/// compositor may blur. On an opaque surface its translucent pixels are never
/// composited and leave trails (#593). Other themes keep the opaque window they
/// had. Transparency cannot be added to a window later, so macOS replaces the
/// window when a theme changes this choice.
pub fn transparent_window(messages: bool, see_through: bool) -> bool {
    cfg!(target_os = "macos")
        || (messages && platform::may_show_through())
        || (see_through && cfg!(any(target_os = "macos", target_os = "linux")))
}

/// Whether the window on screen was made transparent, so a frame may clear
/// to a colour with alpha. Set by `main` as each window is made.
static WINDOW_TRANSPARENT: AtomicBool = AtomicBool::new(false);

/// Records whether the window just made is transparent.
pub fn set_window_transparent(transparent: bool) {
    WINDOW_TRANSPARENT.store(transparent, Ordering::Relaxed);
}

/// Whether the main window was made transparent, which also means its GL
/// config has an alpha channel: other windows of the app may be see-through.
pub fn window_is_transparent() -> bool {
    WINDOW_TRANSPARENT.load(Ordering::Relaxed)
}

/// Takes away the hairline border and rounded corners Windows 11 draws
/// around a borderless window of this app titled `title`. A see-through
/// window, such as the chat bubbles', otherwise shows them as a thin rectangle
/// floating round what it draws. Does nothing on other systems, and nothing
/// until the window exists, so call it on each frame the window is shown.
pub fn strip_window_frame(title: &str) {
    #[cfg(windows)]
    platform::strip_window_frame(title);
    #[cfg(not(windows))]
    let _ = title;
}
/// The colour a frame clears to: `window`, made opaque on an opaque window,
/// where alpha would leave the last frame's pixels behind (a theme with
/// alpha chosen while the window is open, until the next one is made).
pub fn clear_color(window: egui::Color32) -> egui::Color32 {
    if WINDOW_TRANSPARENT.load(Ordering::Relaxed) {
        window
    } else {
        egui::Color32::from_rgb(window.r(), window.g(), window.b())
    }
}

/// Gives the window its material while `palette` wants one, takes it away
/// otherwise, and records which for [`active`]. Cheap to call every frame.
pub fn apply(frame: &eframe::Frame, palette: &crate::theme::Palette, transparent: bool) {
    let wanted = palette.shows_desktop() && transparent && blur();
    ACTIVE.store(platform::apply(frame, palette, wanted), Ordering::Relaxed);
}

#[cfg(target_os = "macos")]
mod platform {
    pub fn may_show_through() -> bool {
        true
    }

    /// Accessibility's Reduce transparency.
    pub fn reduces_transparency() -> bool {
        use objc2::runtime::AnyObject;
        use objc2::{class, msg_send};
        // SAFETY: the shared workspace and this getter (macOS 10.10 and
        // later) may be used from any thread; a missing workspace is null.
        unsafe {
            let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
            match workspace.as_ref() {
                Some(workspace) => {
                    msg_send![workspace, accessibilityDisplayShouldReduceTransparency]
                }
                None => false,
            }
        }
    }

    pub fn apply(frame: &eframe::Frame, palette: &crate::theme::Palette, wanted: bool) -> bool {
        crate::macos::vibrancy(frame, palette, wanted);
        wanted
    }
}

#[cfg(windows)]
mod platform {
    use std::sync::Mutex;

    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::{COLORREF, HWND};
    use windows::Win32::Graphics::Dwm::{
        DWMSBT_MAINWINDOW, DWMSBT_NONE, DWMWA_CAPTION_COLOR, DWMWA_SYSTEMBACKDROP_TYPE,
        DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE, DwmExtendFrameIntoClientArea,
        DwmSetWindowAttribute,
    };
    use windows::Win32::UI::Controls::MARGINS;
    use windows::core::BOOL;

    /// Windows 11 22H2 and later; earlier versions refuse Mica in `apply`,
    /// and the window stays opaque.
    pub fn may_show_through() -> bool {
        true
    }

    /// See [`super::strip_window_frame`]. The window is found by its title
    /// among this process's own, and each one is stripped once.
    pub fn strip_window_frame(title: &str) {
        use windows::Win32::Graphics::Dwm::{
            DWMWA_BORDER_COLOR, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND,
        };
        use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowThreadProcessId};
        use windows::core::PCWSTR;

        /// `DWMWA_COLOR_NONE`: no border at all, rather than one in a colour.
        const NO_BORDER: u32 = 0xFFFF_FFFE;
        /// The window last stripped, so the attributes are set once for it.
        static STRIPPED: Mutex<Option<isize>> = Mutex::new(None);

        let wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: `wide` ends in NUL and outlives the call.
        let Ok(hwnd) = (unsafe { FindWindowW(PCWSTR::null(), PCWSTR(wide.as_ptr())) }) else {
            return;
        };
        if hwnd.is_invalid() {
            return;
        }
        let mut process = 0_u32;
        // SAFETY: `hwnd` was just found, and `process` holds the id asked for.
        unsafe { GetWindowThreadProcessId(hwnd, Some(&raw mut process)) };
        if process != std::process::id() {
            return;
        }
        let key = hwnd.0 as isize;
        let mut stripped = STRIPPED.lock().unwrap_or_else(|p| p.into_inner());
        if *stripped == Some(key) {
            return;
        }
        let border = COLORREF(NO_BORDER);
        let corners = DWMWCP_DONOTROUND;
        // SAFETY: a live window of this process; the attributes are plain
        // values of the sizes DWM expects. Windows 10 refuses both, where
        // there is no such border to take away.
        unsafe {
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_BORDER_COLOR,
                (&raw const border).cast(),
                size_of::<COLORREF>() as u32,
            );
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                (&raw const corners).cast(),
                size_of_val(&corners) as u32,
            );
        }
        *stripped = Some(key);
    }

    /// Personalisation's Transparency effects, off. A missing value is on,
    /// as Windows has it.
    pub fn reduces_transparency() -> bool {
        use windows_sys::Win32::System::Registry::{
            HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW,
        };
        let key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0"
            .encode_utf16()
            .collect();
        let name: Vec<u16> = "EnableTransparency\0".encode_utf16().collect();
        let mut value: u32 = 1;
        let mut size = std::mem::size_of::<u32>() as u32;
        // SAFETY: both names end in NUL, and `value` holds the DWORD asked for.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                (&raw mut value).cast(),
                &raw mut size,
            )
        };
        status == 0 && value == 0
    }

    /// The window last given a backdrop, whether it was wanted and dark, the
    /// title bar's colour (a COLORREF) and its text's, and whether Windows
    /// accepted the backdrop, so DWM is asked only when something changes.
    type Applied = (isize, bool, bool, u32, u32);
    static APPLIED: Mutex<Option<(Applied, bool)>> = Mutex::new(None);

    /// `color` as a COLORREF, which is 0x00BBGGRR.
    fn colorref(color: egui::Color32) -> u32 {
        u32::from(color.r()) | u32::from(color.g()) << 8 | u32::from(color.b()) << 16
    }

    pub fn apply(frame: &eframe::Frame, palette: &crate::theme::Palette, wanted: bool) -> bool {
        let Ok(handle) = frame.window_handle() else {
            return false;
        };
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return false;
        };
        let hwnd = HWND(handle.hwnd.get() as *mut _);
        // The title bar is the system's, and stays solid: Glass moves the
        // window's material and nothing here, so the bar is the same colour
        // at every setting instead of turning the grey of Mica.
        let caption = colorref(palette.titlebar());
        let caption_text = colorref(palette.text);
        let key = (
            handle.hwnd.get(),
            wanted,
            palette.dark,
            caption,
            caption_text,
        );
        let mut applied = APPLIED.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((was, accepted)) = *applied
            && was == key
        {
            return accepted;
        }
        // SAFETY: a live window of this process, on its own thread; the
        // attributes are plain values of the sizes DWM expects.
        let accepted = unsafe {
            let dark = BOOL::from(palette.dark);
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                (&raw const dark).cast(),
                size_of::<BOOL>() as u32,
            );
            // Windows 11 22000 and later; earlier versions refuse, which
            // leaves the system's own title bar.
            let (bar, bar_text) = (COLORREF(caption), COLORREF(caption_text));
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_CAPTION_COLOR,
                (&raw const bar).cast(),
                size_of::<COLORREF>() as u32,
            );
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_TEXT_COLOR,
                (&raw const bar_text).cast(),
                size_of::<COLORREF>() as u32,
            );
            let backdrop = if wanted {
                DWMSBT_MAINWINDOW
            } else {
                DWMSBT_NONE
            };
            // Mica needs Windows 11 22H2; earlier versions refuse, and the
            // window stays opaque.
            let set = DwmSetWindowAttribute(
                hwnd,
                DWMWA_SYSTEMBACKDROP_TYPE,
                (&raw const backdrop).cast(),
                size_of_val(&backdrop) as u32,
            );
            // The backdrop shows only where the frame extends over the
            // client area; all of it, while wanted. The custom title bar
            // leaves the frame alone: without decorations the window has no
            // caption for DWM to draw, and the buttons ZapFast draws sit in
            // the client area over the same Mica.
            let inset = if wanted { -1 } else { 0 };
            let margins = MARGINS {
                cxLeftWidth: inset,
                cxRightWidth: inset,
                cyTopHeight: inset,
                cyBottomHeight: inset,
            };
            let extended = DwmExtendFrameIntoClientArea(hwnd, &margins);
            wanted && set.is_ok() && extended.is_ok()
        };
        if wanted && !accepted {
            log::info!("Mica is unavailable here; the Messages theme stays opaque");
        }
        *applied = Some((key, accepted));
        accepted
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    /// Translucent only where the compositor is known to blur what shows
    /// through: Hyprland blurs translucent windows unless told not to. Other
    /// Wayland compositors blur only windows that ask (KDE's blur protocol,
    /// which ZapFast does not speak yet) or not at all (GNOME), and an
    /// unblurred desktop behind the sidebar would cost its text contrast; X11
    /// may lack a compositor entirely. All of them keep the opaque window.
    pub fn apply(frame: &eframe::Frame, _palette: &crate::theme::Palette, wanted: bool) -> bool {
        wanted
            && may_show_through()
            && frame
                .window_handle()
                .is_ok_and(|handle| matches!(handle.as_raw(), RawWindowHandle::Wayland(_)))
    }

    /// No desktop here has a setting for less transparency.
    pub fn reduces_transparency() -> bool {
        false
    }

    pub fn may_show_through() -> bool {
        blurs(
            std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some(),
            std::env::var("XDG_CURRENT_DESKTOP").ok().as_deref(),
        )
    }

    /// Whether the session's compositor blurs translucent windows by itself:
    /// Hyprland, when both its instance signature and the desktop name say
    /// so, since either alone can be left over from another session.
    pub fn blurs(signature: bool, desktop: Option<&str>) -> bool {
        signature
            && desktop.is_some_and(|desktop| {
                desktop
                    .split(':')
                    .any(|name| name.eq_ignore_ascii_case("Hyprland"))
            })
    }

    #[cfg(test)]
    mod tests {
        /// Only a compositor known to blur gets the translucent window.
        #[test]
        fn only_a_blurring_compositor_gets_translucency() {
            use super::blurs;
            assert!(blurs(true, Some("Hyprland")));
            assert!(blurs(true, Some("Hyprland:wlroots")));
            // A signature left over, or a desktop name alone, is not enough.
            assert!(!blurs(true, Some("GNOME")));
            assert!(!blurs(true, None));
            assert!(!blurs(false, Some("Hyprland")));
            assert!(!blurs(false, Some("KDE")));
        }
    }
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
mod platform {
    pub fn may_show_through() -> bool {
        false
    }

    pub fn reduces_transparency() -> bool {
        false
    }

    pub fn apply(_frame: &eframe::Frame, _palette: &crate::theme::Palette, _wanted: bool) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Other themes keep an opaque window wherever they always had one, and
    /// Messages asks for a transparent one only where it can show through.
    #[test]
    fn opaque_themes_use_an_opaque_window() {
        assert_eq!(transparent_window(false, false), cfg!(target_os = "macos"));
        assert_eq!(
            transparent_window(true, false),
            cfg!(target_os = "macos") || platform::may_show_through()
        );
    }

    /// The default tint keeps every fill as it was; Solid makes it opaque,
    /// and Clear thins it without making it vanish.
    #[test]
    fn glass_tints_from_clear_to_solid() {
        assert_eq!(tinted_at(0.7, GLASS_DEFAULT), 0.7);
        assert_eq!(tinted_at(0.7, 1.0), 1.0);
        assert!((tinted_at(0.7, 0.0) - 0.7 * CLEAREST).abs() < 1e-6);
        let steps: Vec<f32> = (0..=10)
            .map(|step| tinted_at(0.5, step as f32 / 10.0))
            .collect();
        assert!(steps.windows(2).all(|pair| pair[0] <= pair[1]), "{steps:?}");
    }
    /// A Linux theme whose window colour has alpha gets a transparent window
    /// (#593), and an opaque window never clears to alpha.
    #[test]
    fn a_see_through_theme_gets_a_transparent_window_on_linux() {
        assert_eq!(
            transparent_window(false, true),
            cfg!(any(target_os = "macos", target_os = "linux"))
        );
        let glass = egui::Color32::from_rgba_premultiplied(0, 0, 0, 0);
        set_window_transparent(false);
        assert_eq!(clear_color(glass).a(), 255);
        set_window_transparent(true);
        assert_eq!(clear_color(glass), glass);
        set_window_transparent(false);
    }
}
