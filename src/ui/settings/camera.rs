//! Settings' camera rows under Calls and voice: which camera video calls
//! open, with a refresh, and a live, mirrored preview that eases open under
//! the rows.

use egui::{CornerRadius, Rect, Sense, pos2, vec2};

use super::{Section, Text, translated};
use crate::app::App;
use crate::camera_check::Failure;
use crate::i18n::{Locale, gettext};
use crate::model::Action;
use crate::theme::{self, Icon, Palette};
use crate::ui::controls;

/// The widest the preview's picture is drawn.
const PICTURE_WIDTH: f32 = 420.0;
/// The room between the panel's edge and the picture: the panel's radius
/// less the picture's, so the two corners run parallel.
const INSET: f32 = (theme::RADIUS_CARD - theme::RADIUS_CONTROL) as f32;

/// Adds the Camera picker, the preview button and, while it is open or
/// closing, the preview panel.
pub(super) fn rows(app: &App, voice: &mut Section) {
    let locale = app.locale;
    voice.row(translated(locale, "Camera"), missing(app), picker);
    voice.row(
        translated(locale, "Check your camera"),
        translated(locale, "Only you see it. Nothing is recorded or sent."),
        check_button,
    );
    if app.camera_check.is_some() || app.camera_preview.shown {
        voice.block(vec![translated(locale, "Camera preview")], panel);
    }
}

/// Names a picked camera the last scan did not find, so the reader knows
/// why calls open another one. An empty scan proves nothing.
fn missing(app: &App) -> Text {
    let Some(saved) = &app.settings.camera else {
        return Text::default();
    };
    let gone = app.cameras.found.as_ref().is_some_and(|found| {
        !found.is_empty() && crate::calls::find_camera(found, saved).is_none()
    });
    if !gone {
        return Text::default();
    }
    let text = translated(
        app.locale,
        "“{name}” is not connected. Zeus uses the first camera until it is.",
    );
    Text {
        shown: text.shown.replace("{name}", &saved.name).into(),
        source: text.source.replace("{name}", &saved.name).into(),
    }
}

/// The camera list, with the first camera first, and a refresh.
fn picker(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let locale = app.locale;
    let first_label = gettext(locale, "First camera").into_owned();
    let saved = app.settings.camera.clone();
    let found = app.cameras.found.clone().unwrap_or_default();
    let selected = match &saved {
        Some(device) => device.name.clone(),
        None => first_label.clone(),
    };
    let mut picked = None;
    controls::dropdown(
        ui,
        &palette,
        "settings-camera",
        "Camera",
        &selected,
        controls::WIDTH,
        |ui| {
            if controls::option(ui, &palette, &first_label, saved.is_none()) {
                picked = Some(None);
            }
            for (id, label) in &found {
                let current = saved.as_ref().is_some_and(|device| device.name == *label);
                if controls::option(ui, &palette, label, current) {
                    picked = Some(Some(crate::settings::SavedDevice {
                        id: id.clone(),
                        name: label.clone(),
                    }));
                }
            }
        },
    );
    let scanning = app.cameras.scanning();
    let refresh = ui
        .add_enabled_ui(!scanning, |ui| {
            theme::icon_button(
                ui,
                Icon::Refresh,
                15.0,
                palette.secondary,
                palette.text,
                &gettext(locale, "Look for cameras again"),
            )
        })
        .inner;
    if refresh.clicked() {
        app.actions.push(Action::RefreshCameras);
    }
    if let Some(device) = picked
        && device != saved
    {
        app.actions.push(Action::SetCamera(device));
    }
}

/// Opens or closes the preview. A call has the camera to itself.
fn check_button(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let locale = app.locale;
    let in_call = app.call.is_some();
    let checking = app.camera_check.is_some();
    let label = if checking {
        gettext(locale, "Stop")
    } else {
        gettext(locale, "Preview")
    };
    let button = ui
        .add_enabled_ui(!in_call, |ui| {
            theme::soft_button(ui, &palette, Some(Icon::Video), &label, checking)
        })
        .inner
        .on_disabled_hover_text(gettext(locale, "The call is using the camera."));
    theme::reveal_focus(&button);
    if button.clicked() {
        app.actions.push(Action::CheckCamera(!checking));
    }
}

/// The preview: a glass panel that grows open under the rows and shrinks
/// away when the check stops, with the camera's picture mirrored inside it,
/// as a mirror shows a face.
fn panel(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let locale = app.locale;
    let ctx = ui.ctx().clone();
    let open = app.camera_check.is_some();
    let motion = egui::Id::new("settings-camera-preview");
    // Grows in from nothing the first time it is drawn, and closes in the
    // share of that time surfaces take to leave.
    let t = if open {
        theme::appear(&ctx, motion, theme::motion_time(&ctx, theme::MOTION_PANEL))
    } else {
        let seconds = theme::motion_time(&ctx, theme::MOTION_PANEL * theme::CLOSE);
        theme::animate(&ctx, motion, false, seconds)
    };
    if let Some(check) = &app.camera_check {
        app.camera_preview.failure = check.failure();
        if let Some(frame) = check.frame() {
            crate::ui::call::call_texture(
                ui,
                &mut app.camera_preview.texture,
                "zeus-settings-camera",
                &frame,
            );
        }
    }
    if t <= 0.0 && !open {
        // Closed: the picture goes, the panel takes no room, and the next
        // preview grows in anew.
        theme::forget_motion(&ctx, motion);
        app.camera_preview = crate::app::CameraPreview::default();
        return;
    }
    app.camera_preview.shown = true;

    let width = ui.available_width();
    let shape = app
        .camera_preview
        .texture
        .as_ref()
        .map(|kept| vec2(kept.frame.size[0] as f32, kept.frame.size[1] as f32))
        .filter(|size| size.x > 0.0 && size.y > 0.0)
        .unwrap_or(vec2(16.0, 9.0));
    // The camera's own shape, as wide as there is room for, and no taller
    // than a landscape picture would be, so a portrait camera stands narrow.
    let most = vec2(
        (width - 2.0 * INSET).min(PICTURE_WIDTH),
        PICTURE_WIDTH * 9.0 / 16.0,
    );
    let fit = (most.x / shape.x).min(most.y / shape.y);
    let (picture_width, picture_height) = (shape.x * fit, shape.y * fit);
    let full = vec2(picture_width + 2.0 * INSET, picture_height + 2.0 * INSET);
    let (slot, response) = ui.allocate_exact_size(vec2(width, full.y * t), Sense::hover());
    let label = gettext(locale, "Camera preview");
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Image, true, &label));
    // Drawn in a layer of its own: faded with the motion, and clipped to the
    // room it has taken so far, so it is uncovered as it grows.
    let mut layer = ui.new_child(egui::UiBuilder::new().max_rect(slot));
    layer.set_clip_rect(slot.intersect(ui.clip_rect()));
    layer.multiply_opacity(t);
    // The card hugs the picture, centred under the rows, and settles in from
    // slightly smaller, as panels do, about its top.
    let scale = theme::CLOSE_SCALE + (1.0 - theme::CLOSE_SCALE) * t;
    let card = Rect::from_center_size(
        pos2(slot.center().x, slot.top() + full.y * scale / 2.0),
        full * scale,
    );
    layer.painter().rect(
        card,
        CornerRadius::same(theme::RADIUS_CARD),
        palette.menu_glass(),
        palette.glass_edge(),
        egui::StrokeKind::Inside,
    );
    let picture = Rect::from_min_size(
        pos2(
            card.center().x - picture_width * scale / 2.0,
            card.top() + INSET * scale,
        ),
        vec2(picture_width, picture_height) * scale,
    );
    let inner = CornerRadius::same(theme::RADIUS_CONTROL);
    layer
        .painter()
        .rect_filled(picture, inner, tile_fill(&palette));
    let failure = app.camera_preview.failure;
    match (&app.camera_preview.texture, failure) {
        (Some(kept), None) => {
            // Mirrored: left and right swap, as a mirror shows a face.
            let mirror = Rect::from_min_max(pos2(1.0, 0.0), pos2(0.0, 1.0));
            egui::Image::new((kept.texture.id(), picture.size()))
                .uv(mirror)
                .corner_radius(inner)
                .paint_at(&layer, picture);
        }
        _ => status(&layer, &palette, locale, failure, picture),
    }
}

/// What the picture's place says while there is no picture: the camera
/// opening, or why it did not.
fn status(
    ui: &egui::Ui,
    palette: &Palette,
    locale: Locale,
    failure: Option<Failure>,
    picture: Rect,
) {
    let (icon, text, color) = match failure {
        None => (
            Icon::Video,
            gettext(locale, "Opening the camera…"),
            palette.secondary,
        ),
        Some(failure) => (Icon::VideoOff, failure_text(locale, failure), palette.text),
    };
    let icon_rect = Rect::from_center_size(picture.center() - vec2(0.0, 18.0), vec2(28.0, 28.0));
    theme::paint_icon(ui, icon, icon_rect, 24.0, palette.secondary);
    let galley = ui.painter().layout(
        text.into_owned(),
        theme::medium(theme::TEXT_CAPTION),
        color,
        (picture.width() - 48.0).max(80.0),
    );
    let at = pos2(
        picture.center().x - galley.size().x / 2.0,
        icon_rect.bottom() + theme::SPACE_S,
    );
    ui.painter().galley(at, galley, color);
}

/// Why the preview shows no picture, in words the reader can act on.
fn failure_text(locale: Locale, failure: Failure) -> std::borrow::Cow<'static, str> {
    match failure {
        Failure::NoCamera => gettext(locale, "No camera found. Connect one, then look again."),
        Failure::Refused => gettext(
            locale,
            "Zeus may not use the camera. Allow it in System Settings, Privacy & Security, Camera.",
        ),
        Failure::Unavailable => gettext(
            locale,
            "Could not open this camera. Another app may be using it, or camera access may be off in your system's privacy settings.",
        ),
        Failure::Stopped => gettext(
            locale,
            "The camera stopped. It may have been unplugged or taken by another app.",
        ),
    }
}

/// The picture's ground before it arrives: a shade off the panel in either
/// theme, as a call's tiles have.
fn tile_fill(palette: &Palette) -> egui::Color32 {
    theme::blend(
        palette.window,
        palette.text,
        if palette.dark { 0.08 } else { 0.06 },
    )
}
