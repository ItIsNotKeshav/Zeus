//! The first-run setup: the card arriving, a mode, a theme, a font and
//! doodles picked on the way, and the phone link, then a step back.

use super::{Cue, Gesture, Target};
use crate::app::App;
use egui::PointerButton;
use std::time::Duration;

/// Length of the tour, excluding its optional start delay.
pub const DURATION: Duration = Duration::from_secs(24);

/// Sets up the opening shot: a new installation's welcome, in the dark
/// theme and in English.
pub fn prepare(app: &mut App) {
    assert!(app.backend.is_offline(), "a tour requires an offline app");
    app.settings.interface_language = Some(crate::i18n::Locale::English);
    app.locale = crate::i18n::Locale::English;
    super::super::apply_flags(app, Some("setup,nothing"));
}

pub(super) fn script() -> Vec<Cue> {
    use Gesture::*;
    use Target::*;
    let mut cues = Vec::new();
    let mut click = |at: f32, label: &'static str| {
        cues.push(Cue {
            at,
            gesture: Move(Label(label)),
        });
        cues.push(Cue {
            at: at + 0.45,
            gesture: Click(PointerButton::Primary),
        });
    };
    click(1.6, "Get started");
    click(3.2, "Light");
    click(4.8, "WhatsApp");
    click(6.4, "Dark");
    click(8.0, "Zeus");
    click(9.4, "Continue");
    click(11.0, "Figtree");
    click(12.4, "Space");
    click(13.8, "Classic");
    click(15.0, "Google Sans Flex");
    click(16.4, "Continue");
    click(18.0, "Continue");
    click(19.6, "Continue");
    click(22.0, "Back");
    cues
}
