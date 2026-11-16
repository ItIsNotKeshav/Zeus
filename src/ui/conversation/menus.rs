//! Zeus's own parts of reactions and the message menus.

use super::*;

/// How many reactors the pill's tooltip names before summarizing the rest.
pub(super) const REACTION_TOOLTIP_ROWS: usize = 12;

/// Who reacted with what, built only while the tooltip is showing: a message
/// with hundreds of reactions would otherwise format a name per reaction on
/// every frame it scrolls past.
pub(super) fn reaction_tooltip(view: &View<'_>, message: &Message) -> String {
    let mut lines: Vec<String> = message
        .reactions
        .iter()
        .take(REACTION_TOOLTIP_ROWS)
        .map(|reaction| {
            let who = if reaction.from_me {
                "You".to_owned()
            } else {
                (view.names_or)(&reaction.sender, None)
            };
            format!("{} {who}", reaction.emoji)
        })
        .collect();
    let more = message
        .reactions
        .len()
        .saturating_sub(REACTION_TOOLTIP_ROWS);
    if more > 0 {
        lines.push(format!("+{more}"));
    }
    lines.join("\n")
}

/// Records that a message was drawn with no reactions.
pub(super) fn note_no_reactions(ui: &egui::Ui, view: &View<'_>, message: &Message) {
    let tally_id = egui::Id::new(("reaction-tally", &view.chat.id, &message.id));
    let frame = ui.ctx().cumulative_frame_nr();
    ui.ctx()
        .data_mut(|data| data.insert_temp(tally_id, (frame, Vec::<(String, u32)>::new())));
}

/// How long a reaction pill's pop runs.
pub(super) const REACTION_POP_SECONDS: f64 = 0.35;

/// The value recorded for a row when it was last drawn, if that was the frame
/// before this one (or this frame). A row that was off screen since, scrolled
/// back into view, has no earlier state: a change made while it was away is
/// not a live one, so nothing pops for it.
pub(super) fn seen_last_frame<T>(stored: Option<(u64, T)>, frame: u64) -> Option<T> {
    stored.and_then(|(at, value)| (frame.saturating_sub(at) <= 1).then_some(value))
}

/// What a message's reaction tally did since it was last drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReactionChange {
    None,
    Arrived,
    Changed,
}

/// Compares the tally drawn before with the one now. No earlier tally means
/// the message was only just seen, which is not a change.
pub(super) fn reaction_change(
    previous: Option<&[(String, u32)]>,
    now: &[(String, u32)],
) -> ReactionChange {
    match previous {
        None => ReactionChange::None,
        Some(before) if before == now => ReactionChange::None,
        Some([]) if !now.is_empty() => ReactionChange::Arrived,
        Some(_) => ReactionChange::Changed,
    }
}

/// Delay between one quick reaction starting to spring in and the next.
pub(super) const QUICK_STAGGER: f32 = 0.025;
/// Length of a quick reaction's spring, in seconds.
pub(super) const QUICK_SPRING_SECONDS: f32 = 0.35;
/// Bounce of a quick reaction's spring.
pub(super) const QUICK_SPRING_BOUNCE: f64 = 0.3;
/// Size of the quick reaction under the pointer.
pub(super) const QUICK_HOVER_SCALE: f32 = 1.3;
/// Size of the quick reaction while it is pressed.
pub(super) const QUICK_PRESS_SCALE: f32 = 0.9;

/// Seconds since the quick reactions were first drawn for this menu. A gap
/// of a few frames between draws means the menu was closed, so it starts over.
pub(super) fn quick_reactions_opened(ui: &egui::Ui, calm: bool) -> f32 {
    if calm {
        return 0.0;
    }
    let now = ui.input(|input| input.time);
    let id = ui.id().with("quick-reactions-open");
    let (start, _) = ui
        .ctx()
        .data(|data| data.get_temp::<(f64, f64)>(id))
        .filter(|(_, seen)| now - seen < 0.25)
        .unwrap_or((now, now));
    ui.ctx().data_mut(|data| data.insert_temp(id, (start, now)));
    let elapsed = (now - start) as f32;
    if elapsed < QUICK_SPRING_SECONDS + QUICK_STAGGER * 8.0 {
        ui.ctx().request_repaint();
    }
    elapsed
}

/// How far along the spring the `index`th quick reaction is, 0 to 1 with
/// a little overshoot, `elapsed` seconds after the menu opened.
pub(super) fn quick_reaction_progress(elapsed: f32, index: usize) -> f32 {
    let local = elapsed - QUICK_STAGGER * index as f32;
    if local <= 0.0 {
        return 0.0;
    }
    if local >= QUICK_SPRING_SECONDS * 1.5 {
        return 1.0;
    }
    let (stiffness, damping) = spring_from_duration(
        f64::from(QUICK_SPRING_SECONDS) * 1000.0,
        QUICK_SPRING_BOUNCE,
    );
    spring_unit(f64::from(local) * 1000.0, stiffness, damping) as f32
}

/// The spring-driven size of a quick reaction chasing `target`, kept per
/// emoji so a new target starts from where it is now.
pub(super) fn quick_reaction_lift(
    ctx: &egui::Context,
    id: egui::Id,
    target: f32,
    calm: bool,
) -> f32 {
    if calm {
        return target;
    }
    let id = id.with("lift");
    let (mut value, mut velocity) = ctx
        .data(|data| data.get_temp::<(f32, f32)>(id))
        .unwrap_or((1.0, 0.0));
    let dt = ctx.input(|input| input.stable_dt).min(0.05);
    let (stiffness, damping) = spring_from_duration(
        f64::from(QUICK_SPRING_SECONDS) * 1000.0,
        QUICK_SPRING_BOUNCE,
    );
    // The constants already read in seconds, as `spring_unit` scales its ms.
    let (stiffness, damping) = (stiffness as f32, damping as f32);
    let steps = 4;
    let h = dt / steps as f32;
    for _ in 0..steps {
        let pull = stiffness * (target - value) - damping * velocity;
        velocity += pull * h;
        value += velocity * h;
    }
    let settled = (value - target).abs() < 0.001 && velocity.abs() < 0.01;
    if settled {
        value = target;
        velocity = 0.0;
    } else {
        ctx.request_repaint();
    }
    ctx.data_mut(|data| data.insert_temp(id, (value, velocity)));
    value
}

/// The menu's Delete row, which opens the ways to delete `message`: for me,
/// then for everyone while it can still be revoked. Each asks to confirm.
pub(super) fn delete_submenu(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    can_revoke: bool,
    actions: &mut Vec<Action>,
) {
    let palette = view.palette;
    let for_me = crate::i18n::gettext(view.locale, "Delete for me");
    let for_everyone = crate::i18n::gettext(view.locale, "Delete for everyone");
    let mut items: Vec<&str> = vec![&*for_me];
    if can_revoke {
        items.push(&*for_everyone);
    }
    widgets::submenu(
        ui,
        &palette,
        Icon::Trash,
        &crate::i18n::gettext(view.locale, "Delete"),
        &items,
        |ui| {
            if widgets::menu_item(ui, &palette, Some(Icon::EyeOff), &for_me) {
                actions.push(Action::ShowDialog(Dialog::ConfirmDeleteMessage {
                    chat: view.chat.id.clone(),
                    message: message.id.clone(),
                    for_everyone: false,
                }));
            }
            if can_revoke && widgets::menu_item(ui, &palette, Some(Icon::Trash), &for_everyone) {
                actions.push(Action::ShowDialog(Dialog::ConfirmDeleteMessage {
                    chat: view.chat.id.clone(),
                    message: message.id.clone(),
                    for_everyone: true,
                }));
            }
        },
    );
}
