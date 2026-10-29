//! Split view and the pop-out: a second chat beside the open one, and a chat
//! in a small window of its own that floats above other windows.
//!
//! The chat panel holds at most two panes. The open chat (`App::open_chat`)
//! is always one of them, and it alone owns the composer, the reply and edit
//! strips, the selection and the search: the other pane (`Split::other`)
//! shows its chat's messages and a quiet composer, and a press anywhere in it
//! makes it the open chat, so nothing that belongs to "the open chat" ever has
//! to know a second pane exists. Which side each chat sits on does not change
//! when focus moves between them.
//!
//! What the transcript keeps in `App` for the open chat while it is drawn (the
//! flags that scroll it to the end, the unread divider, a jump to a message)
//! is a [`PaneView`]. The other pane's lives in the split and is swapped in
//! around its drawing (`ui::split`), and with the open chat's on a change of
//! focus, so each pane keeps its own place.
//!
//! The pop-out is not a pane: a chat of the account on screen, drawn by
//! `ui::split::pop_out` in a viewport of its own with a reply field that keeps
//! its text beside it, so it works the same whichever chat is open.

use egui::{Pos2, Rect, pos2};

use super::{App, JumpHighlight, UnreadDivider};
use crate::model::{AccountId, Action, ChatId, Scroll};

/// The narrowest a pane may be, in points: the conversation header with its
/// avatar, the name and three buttons still fits.
pub const MIN_PANE: f32 = 360.0;
/// How much of the panel the left pane takes at first.
pub const DEFAULT_RATIO: f32 = 0.5;

/// A side of the chat panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    pub fn other(self) -> Self {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        }
    }
}

/// What the transcript of a chat on screen reads from the app while it is
/// drawn: the app holds these for the open chat, and a pane that is not the
/// open chat keeps its own and has them swapped in.
#[derive(Clone, Debug, Default)]
pub struct PaneView {
    pub scroll_to_bottom: bool,
    pub scroll_to_bottom_forced: bool,
    pub scroll_page: Option<Scroll>,
    pub at_bottom: bool,
    pub scroll_anchor: Option<String>,
    pub jump_highlight: Option<JumpHighlight>,
    pub unread_divider: Option<UnreadDivider>,
}

impl PaneView {
    /// Exchanges this view with the app's own.
    pub fn swap_with(&mut self, app: &mut App) {
        std::mem::swap(&mut self.scroll_to_bottom, &mut app.scroll_to_bottom);
        std::mem::swap(
            &mut self.scroll_to_bottom_forced,
            &mut app.scroll_to_bottom_forced,
        );
        std::mem::swap(&mut self.scroll_page, &mut app.scroll_page);
        std::mem::swap(&mut self.at_bottom, &mut app.at_bottom);
        std::mem::swap(&mut self.scroll_anchor, &mut app.scroll_anchor);
        std::mem::swap(&mut self.jump_highlight, &mut app.jump_highlight);
        std::mem::swap(&mut self.unread_divider, &mut app.unread_divider);
    }
}

/// Two chats side by side in the chat panel.
#[derive(Clone, Debug)]
pub struct Split {
    /// The chat in the pane that is not the open chat.
    pub other: ChatId,
    /// The side the open chat's pane is on.
    pub open_side: Side,
    /// How much of the panel the left pane takes, before its gap.
    pub ratio: f32,
    /// What the other chat's transcript keeps.
    pub view: PaneView,
}

impl Split {
    /// The side the other chat's pane is on.
    pub fn other_side(&self) -> Side {
        self.open_side.other()
    }
}

/// A chat shown in a window of its own.
#[derive(Clone, Debug)]
pub struct PopOut {
    pub account: AccountId,
    pub chat: ChatId,
    /// What the reply field holds.
    pub text: String,
    /// What the transcript keeps.
    pub view: PaneView,
    /// Whether the reply field takes the keyboard focus the next frame.
    pub focus: bool,
}

/// Whether a panel `width` points wide has room for two panes `gap` apart.
pub fn fits(width: f32, gap: f32) -> bool {
    width >= 2.0 * MIN_PANE + gap
}

/// `ratio` held so that both panes of a panel `width` wide, `gap` apart, keep
/// at least [`MIN_PANE`], or an even split where that is not possible.
pub fn clamp_ratio(width: f32, gap: f32, ratio: f32) -> f32 {
    let room = (width - gap).max(1.0);
    let low = (MIN_PANE / room).min(0.5);
    ratio.clamp(low, 1.0 - low)
}

/// The left and the right pane of `area`, `gap` apart.
pub fn panes(area: Rect, gap: f32, ratio: f32) -> (Rect, Rect) {
    let room = (area.width() - gap).max(0.0);
    let left = (room * ratio).round();
    (
        Rect::from_min_max(area.min, pos2(area.min.x + left, area.max.y)),
        Rect::from_min_max(pos2(area.min.x + left + gap, area.min.y), area.max),
    )
}

impl App {
    /// The chat of the pane beside the open chat, when there is one to show:
    /// it still exists and is not a locked chat outside its folder.
    pub fn split_other(&self) -> Option<&crate::model::Chat> {
        let split = self.split.as_ref()?;
        self.chat(&split.other)
            .filter(|chat| !chat.locked || self.locked_folder_open())
    }

    /// Whether the reader sees this chat: it is the open chat, or the other
    /// pane of a split drawn on the chat list's page.
    pub fn on_screen(&self, chat: &str) -> bool {
        self.open_chat.as_deref() == Some(chat)
            || (self.page == crate::model::Page::Chats
                && self.split_other().is_some_and(|other| other.id == chat))
    }

    /// The chat a pane at `side` shows, when the panel is split.
    pub fn pane_chat(&self, side: Side) -> Option<&ChatId> {
        let split = self.split.as_ref()?;
        if split.open_side == side {
            self.open_chat.as_ref()
        } else {
            Some(&split.other)
        }
    }

    /// Opens `chat` in a pane on `side` of the chat panel and makes it the
    /// open chat. Beside nothing, it is a split of the open chat and this one;
    /// in a split, it takes the place of the chat in that pane.
    pub(super) fn open_split(&mut self, chat: ChatId, side: Side) {
        if self.chat(&chat).is_none()
            || (self.chat(&chat).is_some_and(|chat| chat.locked) && !self.locked_folder_open())
        {
            return;
        }
        let Some(open) = self.open_chat.clone() else {
            self.open_chat(chat);
            return;
        };
        if open == chat {
            return;
        }
        match self.split.take() {
            None => {
                // The chat on screen steps aside, keeping its place.
                let mut view = PaneView::default();
                view.swap_with(self);
                self.split = Some(Split {
                    other: open,
                    open_side: side,
                    ratio: DEFAULT_RATIO,
                    view,
                });
            }
            Some(mut split) => {
                if split.other != chat && split.open_side != side {
                    // The dropped chat takes the other pane; the open chat
                    // keeps its own until focus moves to the new one.
                    split.other = chat.clone();
                    split.view = PaneView::default();
                }
                self.split = Some(split);
            }
        }
        self.open_chat(chat);
    }

    /// Closes the pane showing `chat`; the chat in the other pane fills the
    /// panel. `ctx` is for the animation that plays the pane out.
    pub(super) fn close_split_pane(&mut self, chat: &str, ctx: &egui::Context) {
        let Some(split) = self.split.take() else {
            return;
        };
        let title = self.chat(chat).map(|chat| self.chat_title(chat));
        let (side, ratio) = if self.open_chat.as_deref() == Some(chat) {
            (split.open_side, split.ratio)
        } else if split.other == chat {
            (split.other_side(), split.ratio)
        } else {
            // Not one of the panes: nothing to close.
            self.split = Some(split);
            return;
        };
        crate::ui::split::leave(ctx, chat, title.unwrap_or_default(), side, ratio);
        if split.other == chat {
            return;
        }
        // The open chat's pane closed: the other chat takes its place, with
        // the place it had.
        let mut view = split.view;
        self.open_chat(split.other);
        view.swap_with(self);
    }

    /// Puts the open chat on the other side of the panel.
    pub(super) fn swap_split_sides(&mut self) {
        if let Some(split) = &mut self.split {
            split.open_side = split.open_side.other();
            split.ratio = 1.0 - split.ratio;
        }
    }

    /// Moves focus to the other pane when `id` is its chat, ahead of
    /// `open_chat` opening it: the open chat steps aside into the pane the
    /// other leaves, and the two panes' views change hands. Returns whether
    /// it did.
    pub(super) fn focus_other_pane(&mut self, id: &str) -> bool {
        let Some(open) = self.open_chat.clone() else {
            return false;
        };
        let Some(mut split) = self.split.take() else {
            return false;
        };
        if split.other != id || open == id {
            self.split = Some(split);
            return false;
        }
        split.other = open;
        split.open_side = split.open_side.other();
        split.view.swap_with(self);
        self.split = Some(split);
        true
    }

    /// Forgets a split that no longer has two chats: one left, was deleted or
    /// locked.
    pub(super) fn forget_split_of(&mut self, id: &str) {
        if self.split.as_ref().is_some_and(|split| split.other == id) {
            self.split = None;
        }
        if self.pop_out.as_ref().is_some_and(|pop| pop.chat == id) {
            self.pop_out = None;
        }
    }

    /// Runs `draw` with the other pane's place swapped in for the open chat's,
    /// and swaps it back after: a pane's transcript reads where it is
    /// scrolled, its unread divider and its pending jumps from the app.
    pub(crate) fn with_other_view<R>(&mut self, draw: impl FnOnce(&mut App) -> R) -> R {
        let Some(mut view) = self
            .split
            .as_mut()
            .map(|split| std::mem::take(&mut split.view))
        else {
            return draw(self);
        };
        view.swap_with(self);
        let out = draw(self);
        view.swap_with(self);
        if let Some(split) = self.split.as_mut() {
            split.view = view;
        }
        out
    }

    /// The same for the pop-out's transcript.
    pub(crate) fn with_pop_out_view<R>(&mut self, draw: impl FnOnce(&mut App) -> R) -> R {
        let Some(mut view) = self
            .pop_out
            .as_mut()
            .map(|pop| std::mem::take(&mut pop.view))
        else {
            return draw(self);
        };
        view.swap_with(self);
        let out = draw(self);
        view.swap_with(self);
        if let Some(pop) = self.pop_out.as_mut() {
            pop.view = view;
        }
        out
    }

    /// Shows `chat` in a window of its own.
    pub(super) fn open_pop_out(&mut self, chat: ChatId) {
        if self.chat(&chat).is_none()
            || (self.chat(&chat).is_some_and(|chat| chat.locked) && !self.locked_folder_open())
        {
            return;
        }
        self.ensure_loaded(&chat);
        // What it had typed there, and the place it was left at, stay while
        // the same chat is popped out again.
        let keep = self.pop_out.take().filter(|pop| pop.chat == chat);
        self.pop_out = Some(keep.unwrap_or_else(|| PopOut {
            account: self.account().id.clone(),
            chat,
            text: String::new(),
            view: PaneView {
                scroll_to_bottom: true,
                at_bottom: true,
                ..PaneView::default()
            },
            focus: true,
        }));
    }

    /// Sends what the pop-out's reply field holds to its chat. It never
    /// touches the open chat's composer, quote or edit.
    pub(super) fn send_from_pop_out(&mut self, chat: ChatId, text: String) {
        let text = text.trim().to_owned();
        if text.is_empty() || !self.chat(&chat).is_some_and(|chat| chat.can_send()) {
            return;
        }
        self.stop_composing(&chat);
        self.backend.send(crate::backend::Command::SendText {
            chat,
            text,
            quoting: None,
            mentions: Vec::new(),
        });
        self.follow_outgoing(None);
    }

    /// The action `Dock` of a pop-out is: the chat opens in the window and
    /// the pop-out closes.
    pub(super) fn dock_pop_out(&mut self) {
        if let Some(pop) = self.pop_out.take()
            && self.account().id == pop.account
        {
            let chat = pop.chat;
            self.open_chat(chat);
        }
    }

    /// Whether `action` may leave a pop-out's window: it names its own chat
    /// or message and does not lean on "the open chat", whose composer,
    /// selection and menus the pop-out has none of.
    pub fn pop_out_allows(action: &Action) -> bool {
        matches!(
            action,
            Action::LoadOlder { .. }
                | Action::PopOutSend { .. }
                | Action::DockPopOut
                | Action::ClosePopOut
                | Action::Download { .. }
                | Action::DownloadMotion { .. }
                | Action::OpenUrl(_)
                | Action::CopyText(_)
                | Action::OpenFile(_)
                | Action::OpenFolder(_)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_panes_need_room_for_both_and_the_gap() {
        assert!(fits(2.0 * MIN_PANE + 8.0, 8.0));
        assert!(!fits(2.0 * MIN_PANE + 7.0, 8.0));
    }

    #[test]
    fn a_ratio_keeps_both_panes_at_their_minimum() {
        let width = 1000.0;
        let low = clamp_ratio(width, 8.0, 0.1);
        let high = clamp_ratio(width, 8.0, 0.95);
        assert!((low * (width - 8.0) - MIN_PANE).abs() < 0.5);
        assert!(((1.0 - high) * (width - 8.0) - MIN_PANE).abs() < 0.5);
        assert_eq!(clamp_ratio(width, 8.0, 0.5), 0.5);
        // Too narrow for two: an even split rather than a panic.
        assert_eq!(clamp_ratio(500.0, 8.0, 0.9), 0.5);
    }

    #[test]
    fn panes_fill_the_area_around_the_gap() {
        let area = Rect::from_min_max(pos2(10.0, 20.0), pos2(1010.0, 520.0));
        let (left, right) = panes(area, 8.0, 0.5);
        assert_eq!(left.min, area.min);
        assert_eq!(right.max, area.max);
        assert_eq!(right.left() - left.right(), 8.0);
        assert_eq!(left.width() + right.width() + 8.0, area.width());
    }

    #[test]
    fn sides_are_each_others_other() {
        assert_eq!(Side::Left.other(), Side::Right);
        assert_eq!(Side::Right.other(), Side::Left);
    }
}
