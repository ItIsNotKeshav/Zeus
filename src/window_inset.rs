//! Windows draws a window's title bar taller while it floats than once it is
//! maximized: the floating bar sits under the sizing frame, which a maximized
//! window pushes off the screen. The app's own content then starts that much
//! higher, and the first row looks cut off against the title bar. While the
//! window is maximized the interface starts lower by the same amount, so its
//! spacing under the title bar matches the floating window's.

/// Points the interface starts below the top of the window's client area:
/// the sizing frame Windows hides while the window is maximized, zero when
/// it floats, is full screen or is on another platform.
pub fn top(input: &egui::RawInput) -> f32 {
    let Some(viewport) = input.viewports.get(&input.viewport_id) else {
        return 0.0;
    };
    let maximized = viewport.maximized.unwrap_or(false);
    let fullscreen = viewport.fullscreen.unwrap_or(false);
    match viewport.native_pixels_per_point {
        Some(scale) if maximized && !fullscreen && scale > 0.0 => frame_pixels(scale) / scale,
        _ => 0.0,
    }
}

#[cfg(windows)]
fn frame_pixels(scale: f32) -> f32 {
    use windows::Win32::UI::HiDpi::GetSystemMetricsForDpi;
    use windows::Win32::UI::WindowsAndMessaging::{SM_CXPADDEDBORDER, SM_CYSIZEFRAME};
    let dpi = (scale * 96.0).round() as u32;
    // SAFETY: plain system metrics, no pointers.
    let (frame, padding) = unsafe {
        (
            GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi),
            GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi),
        )
    };
    (frame + padding) as f32
}

#[cfg(not(windows))]
fn frame_pixels(_scale: f32) -> f32 {
    0.0
}
