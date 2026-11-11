//! The unlinked window: the window's outline while Zeus starts, then the first-run
//! setup and the phone link (`super::setup`). The centred card here is the
//! lock screen's.

use egui::{Align, CornerRadius, Frame, Layout, Margin, Stroke, Vec2};

use crate::app::App;
use crate::backend::LinkStatus;
use crate::theme;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    // Someone already in the setup stays in it while the link restarts; the
    // link step says what it is waiting for.
    let mid_setup = !app.settings.setup_done && app.setup_step != super::setup::Step::Welcome;
    if matches!(app.link, LinkStatus::Starting | LinkStatus::Connecting) && !mid_setup {
        let label = crate::i18n::gettext(app.locale, "Connecting…");
        super::skeleton::window(app, ui, label.as_ref());
    } else {
        super::skeleton::fade_out(app, ui.ctx());
        super::setup::show(app, ui);
    }
}

/// How far a surface has come in since it appeared, eased: 0 on the frame
/// it first shows (again), 1 once [`theme::MOTION_SCREEN`] has passed.
fn appearance(ctx: &egui::Context, id: egui::Id) -> f32 {
    let progress = (super::arrive::shown_for(ctx, id) / theme::MOTION_SCREEN).clamp(0.0, 1.0);
    if progress < 1.0 {
        ctx.request_repaint();
    }
    theme::ease_out(progress)
}

/// The centred card of the lock screen: the mark, `tagline` as its title,
/// and `body`, over the setup's soft light. It rises into place as it
/// appears.
pub(super) fn card(
    app: &mut App,
    ui: &mut egui::Ui,
    salt: &str,
    tagline: &str,
    body: impl FnOnce(&mut App, &mut egui::Ui),
) {
    let palette = app.palette;
    egui::CentralPanel::default()
        .frame(Frame::new().fill(palette.window))
        .show(ui, |ui| {
            let rect = ui.max_rect();
            super::setup::backdrop(ui, &palette, rect, false);
            let appearance_id = ui.id().with((salt, "card-appearance"));
            let shown = appearance(ui.ctx(), appearance_id);
            let opened = super::arrive::shown_for(ui.ctx(), appearance_id);
            let card_width = (440.0_f32.min(rect.width() - 24.0)).max(0.0);
            // Center the card using its previous height. Its content determines
            // the next frame's height.
            let height_id = ui.id().with(format!("{salt}-card-height"));
            let known_height = ui
                .ctx()
                .data(|data| data.get_temp::<f32>(height_id))
                .unwrap_or(420.0);
            let card_height = (known_height.min(rect.height() - 24.0)).max(0.0);
            let card = egui::Rect::from_center_size(
                rect.center() + Vec2::new(0.0, 12.0 * (1.0 - shown)),
                Vec2::new(card_width, card_height),
            );
            let mut card_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(card)
                    .layout(Layout::top_down(Align::Center)),
            );
            card_ui.multiply_opacity(shown);
            // The card's own fill is the first shape; what is on it follows
            // the card up, top to bottom.
            let first = super::arrive::painted(&card_ui);
            let shown = Frame::new()
                .fill(palette.panel)
                .stroke(Stroke::new(1.0, palette.outline))
                .corner_radius(CornerRadius::same(theme::RADIUS_DIALOG))
                .inner_margin(Margin::same(36))
                .shadow(palette.modal_shadow())
                .show(&mut card_ui, |ui| {
                    ui.set_width((card_width - 72.0).max(0.0));
                    ui.spacing_mut().item_spacing.y = 8.0;
                    let (logo, _) = ui.allocate_exact_size(Vec2::splat(60.0), egui::Sense::hover());
                    theme::mark(ui, logo.center(), 60.0);
                    ui.add_space(12.0);
                    theme::text(ui, tagline, theme::display(28.0), palette.text);
                    ui.add_space(10.0);
                    body(app, ui);
                });
            super::arrive::cascade(
                &card_ui,
                egui::layers::ShapeIdx(first.0 + 1),
                shown.response.rect,
                opened,
                super::arrive::Cascade {
                    delay: 0.06,
                    spread: 0.22,
                    duration: theme::MOTION_SCREEN,
                    offset: Vec2::new(0.0, 10.0),
                    from_opacity: 0.0,
                },
            );
            let height = shown.response.rect.height();
            if (height - known_height).abs() > 2.0 {
                ui.ctx()
                    .data_mut(|data| data.insert_temp(height_id, height.round()));
                ui.ctx().request_repaint();
            }
        });
}

pub(super) fn busy(ui: &mut egui::Ui, color: egui::Color32, label: &str) {
    ui.horizontal(|ui| {
        let width = 24.0
            + 8.0
            + ui.painter()
                .layout_no_wrap(label.to_owned(), theme::medium(14.0), color)
                .size()
                .x;
        ui.add_space((ui.available_width() - width).max(0.0) / 2.0);
        theme::spinner(ui, 18.0, color);
        theme::text(ui, label, theme::medium(14.0), ui.visuals().text_color());
    });
}

/// Whether the archive's key is gone for good, rather than the keyring being
/// locked or unavailable, which "Try again" can fix.
pub(super) fn archive_key_lost(message: &str) -> bool {
    message.contains("OS keyring key is missing")
        || message.contains("archive key in the OS keyring is invalid")
        || message.contains("could not be unlocked with its OS keyring key")
}

#[cfg(test)]
mod start_over_tests {
    #[test]
    fn only_a_lost_key_offers_to_start_over() {
        for lost in [
            "The archive is encrypted but its OS keyring key is missing. Restore the original keyring; the archive has not been changed",
            "The archive key in the OS keyring is invalid",
            "The archive could not be unlocked with its OS keyring key: file is not a database",
        ] {
            assert!(super::archive_key_lost(lost), "{lost}");
        }
        for recoverable in [
            "Unlock your OS keyring and restart Zeus",
            "The OS keyring could not open Zeus's archive key",
        ] {
            assert!(!super::archive_key_lost(recoverable), "{recoverable}");
        }
    }
}
