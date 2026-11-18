//! The app icon picker's artwork: a set of tiles drawn from Zeus's own mark
//! (`assets/app-icons/*.svg`, the default being `packaging/icons/zeus.svg`).
//!
//! The choice changes the running app's icon only: the macOS Dock tile, the
//! window icon (Windows and Linux), the tray item and the Windows taskbar
//! badge. The installed bundle's own icon (Finder, the launcher, the file on
//! disk) is never modified.

use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

/// One icon the picker offers. All are free; none is locked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppIcon {
    /// Zeus's own black tile.
    #[default]
    Default,
    Lagoon,
    Meadow,
    Orchid,
    Sunset,
    Ocean,
    Mint,
    Graphite,
    Midnight,
    Paper,
    Glow,
    LongShadow,
    Starfield,
    /// A green tile in the colour of WhatsApp's, with Zeus's own mark: never
    /// WhatsApp's logo.
    Green,
}

impl AppIcon {
    /// Every icon, in the order the picker shows them.
    pub const ALL: [AppIcon; 14] = [
        AppIcon::Default,
        AppIcon::Lagoon,
        AppIcon::Meadow,
        AppIcon::Orchid,
        AppIcon::Sunset,
        AppIcon::Ocean,
        AppIcon::Mint,
        AppIcon::Graphite,
        AppIcon::Midnight,
        AppIcon::Paper,
        AppIcon::Glow,
        AppIcon::LongShadow,
        AppIcon::Starfield,
        AppIcon::Green,
    ];

    /// The name shown on hover and read out by screen readers.
    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "Zeus",
            Self::Lagoon => "Lagoon",
            Self::Meadow => "Meadow",
            Self::Orchid => "Orchid",
            Self::Sunset => "Sunset",
            Self::Ocean => "Ocean",
            Self::Mint => "Mint",
            Self::Graphite => "Graphite",
            Self::Midnight => "Midnight",
            Self::Paper => "Paper",
            Self::Glow => "Glow",
            Self::LongShadow => "Long shadow",
            Self::Starfield => "Starfield",
            Self::Green => "Green",
        }
    }

    /// The icon's artwork.
    pub fn svg(self) -> &'static [u8] {
        match self {
            Self::Default => include_bytes!("../packaging/icons/zeus.svg"),
            Self::Lagoon => include_bytes!("../assets/app-icons/lagoon.svg"),
            Self::Meadow => include_bytes!("../assets/app-icons/meadow.svg"),
            Self::Orchid => include_bytes!("../assets/app-icons/orchid.svg"),
            Self::Sunset => include_bytes!("../assets/app-icons/sunset.svg"),
            Self::Ocean => include_bytes!("../assets/app-icons/ocean.svg"),
            Self::Mint => include_bytes!("../assets/app-icons/mint.svg"),
            Self::Graphite => include_bytes!("../assets/app-icons/graphite.svg"),
            Self::Midnight => include_bytes!("../assets/app-icons/midnight.svg"),
            Self::Paper => include_bytes!("../assets/app-icons/paper.svg"),
            Self::Glow => include_bytes!("../assets/app-icons/glow.svg"),
            Self::LongShadow => include_bytes!("../assets/app-icons/longshadow.svg"),
            Self::Starfield => include_bytes!("../assets/app-icons/starfield.svg"),
            Self::Green => include_bytes!("../assets/app-icons/green.svg"),
        }
    }

    fn index(self) -> u8 {
        Self::ALL.iter().position(|icon| *icon == self).unwrap_or(0) as u8
    }
}

/// The icon the running app wears. The tray and taskbar draw through plain
/// functions (`util::app_icon_rgba`), so the choice lives here, not in them.
static CURRENT: AtomicU8 = AtomicU8::new(0);

/// The icon in use now.
pub fn current() -> AppIcon {
    AppIcon::ALL
        .get(CURRENT.load(Ordering::Relaxed) as usize)
        .copied()
        .unwrap_or_default()
}

/// Makes `icon` the one every icon surface draws from now on.
pub fn set_current(icon: AppIcon) {
    CURRENT.store(icon.index(), Ordering::Relaxed);
}

/// `icon` rasterized to straight-alpha RGBA at `size` pixels square.
pub fn rgba(icon: AppIcon, size: usize) -> Option<Vec<u8>> {
    crate::util::render_mark(icon.svg(), size)
}

/// The window icon, for platforms that take one from the application.
#[cfg(not(target_os = "macos"))]
pub fn window_icon(icon: AppIcon) -> egui::IconData {
    const SIZE: usize = 128;
    egui::IconData {
        rgba: rgba(icon, SIZE).unwrap_or_else(|| crate::util::app_icon_rgba(SIZE)),
        width: SIZE as u32,
        height: SIZE as u32,
    }
}

/// `icon` as a PNG for the Dock: the tile at 80% of a 512 square, the
/// margin macOS leaves around its own icons. 512 pixels is the Dock's
/// largest magnified tile on a 2x display; a 1024 square would only make
/// AppKit keep a four times larger decoded bitmap for as long as the app
/// runs, and take longer to rasterize on the interface thread.
#[cfg(target_os = "macos")]
fn dock_png(icon: AppIcon) -> Option<Vec<u8>> {
    const CANVAS: usize = 512;
    const TILE: usize = 412;
    let tile = rgba(icon, TILE)?;
    let mut canvas = vec![0u8; CANVAS * CANVAS * 4];
    let offset = (CANVAS - TILE) / 2;
    for row in 0..TILE {
        let from = row * TILE * 4;
        let to = ((row + offset) * CANVAS + offset) * 4;
        canvas[to..to + TILE * 4].copy_from_slice(&tile[from..from + TILE * 4]);
    }
    let image = image::RgbaImage::from_raw(CANVAS as u32, CANVAS as u32, canvas)?;
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).ok()?;
    Some(png.into_inner())
}

/// The Dock icon Cargo runs get, as `main` installs it.
#[cfg(target_os = "macos")]
const DOCK_ICON: &[u8] = include_bytes!("../packaging/macos/icon-dock.png");

/// Puts `icon` on the Dock tile, on the main thread. A packaged app goes
/// back to its bundle's own icon for the default; an unbundled one to the
/// embedded Dock image.
#[cfg(target_os = "macos")]
pub fn apply_dock(icon: AppIcon) {
    if icon == AppIcon::Default {
        match fastframe_dock::set_icon(DOCK_ICON) {
            Ok(true) => {}
            Ok(false) => {
                crate::macos::set_dock_icon(None);
            }
            Err(error) => log::warn!("could not set the Dock icon: {error}"),
        }
    } else if let Some(png) = dock_png(icon) {
        if !crate::macos::set_dock_icon(Some(&png)) {
            log::warn!("could not set the Dock icon");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_renders_something_at_64_pixels() {
        for icon in AppIcon::ALL {
            let rgba = rgba(icon, 64).unwrap_or_else(|| panic!("{icon:?} does not render"));
            assert_eq!(rgba.len(), 64 * 64 * 4, "{icon:?}");
            let opaque = rgba.chunks(4).filter(|pixel| pixel[3] > 200).count();
            assert!(opaque > 64 * 64 / 2, "{icon:?} is mostly empty");
            assert!(
                rgba.chunks(4).any(|pixel| pixel[3] == 0),
                "{icon:?} has no rounded corners"
            );
        }
    }

    #[test]
    fn icons_have_distinct_artwork_and_names() {
        for (index, icon) in AppIcon::ALL.iter().enumerate() {
            for other in &AppIcon::ALL[index + 1..] {
                assert_ne!(icon.svg(), other.svg());
                assert_ne!(icon.label(), other.label());
            }
        }
    }

    #[test]
    fn the_choice_round_trips_through_settings_and_the_current_icon() {
        for icon in AppIcon::ALL {
            let settings = crate::settings::Settings {
                app_icon: icon,
                ..Default::default()
            };
            let json = serde_json::to_string(&settings).unwrap();
            let back: crate::settings::Settings = serde_json::from_str(&json).unwrap();
            assert_eq!(back.app_icon, icon);
            // What `set_current` stores, read back as `current` reads it.
            // The global itself is left alone: tests that draw the app's
            // icon run in parallel and read it.
            assert_eq!(AppIcon::ALL[usize::from(icon.index())], icon);
        }
        // A file from before the picker has none.
        let old: crate::settings::Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(old.app_icon, AppIcon::Default);
    }
}
