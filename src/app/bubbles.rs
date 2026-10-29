//! Chat bubbles, as Android has them: up to [`MAX_BUBBLES`] chats as round
//! pictures floating over every other window, one of which at a time opens
//! into a small chat with its transcript and a reply field that takes text,
//! picked files, dropped files and pasted pictures.
//!
//! A bubble is not the open chat. Like the pop-out, it keeps its own place in
//! the transcript ([`PaneView`]), what is typed in it and what is attached to
//! it, so it works whichever chat the window shows; a chat may be open in the
//! window and in a bubble at once. Bubbles belong to the account on screen:
//! switching accounts lets them go. `ui::bubbles` draws them in a window of
//! their own.

use std::path::PathBuf;

use super::{App, ClipboardPaste, PaneView, Pending};
use crate::backend::Command;
use crate::model::{AccountId, Action, ChatId};

/// The most chats kept as bubbles; a new one past it lets go of the bubble
/// used longest ago.
pub const MAX_BUBBLES: usize = 3;

/// One chat as a bubble.
pub struct Bubble {
    pub account: AccountId,
    pub chat: ChatId,
    /// What the reply field holds.
    pub text: String,
    /// What goes with the next send, in the order it was added.
    pub staged: Vec<Pending>,
    /// Where its transcript is.
    pub view: PaneView,
    /// Whether the reply field takes the keyboard the next frame.
    pub focus: bool,
    /// Whether a file picker opened from it has not answered yet.
    pub picking: bool,
    /// When it was last opened or made, for letting the oldest go.
    used: u64,
}

/// The chats kept as bubbles, top first, and the one open.
#[derive(Default)]
pub struct Bubbles {
    pub list: Vec<Bubble>,
    pub expanded: Option<ChatId>,
    uses: u64,
}

impl Bubbles {
    pub fn get(&self, chat: &str) -> Option<&Bubble> {
        self.list.iter().find(|bubble| bubble.chat == chat)
    }

    pub fn get_mut(&mut self, chat: &str) -> Option<&mut Bubble> {
        self.list.iter_mut().find(|bubble| bubble.chat == chat)
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn clear(&mut self) {
        self.list.clear();
        self.expanded = None;
    }

    fn touch(&mut self, chat: &str) {
        self.uses += 1;
        let uses = self.uses;
        if let Some(bubble) = self.get_mut(chat) {
            bubble.used = uses;
        }
    }

    /// Adds `chat` on top, letting the bubble used longest ago go when there
    /// are already [`MAX_BUBBLES`]. A chat that is already a bubble stays
    /// where it is.
    fn add(&mut self, account: AccountId, chat: ChatId) {
        if self.get(&chat).is_none() {
            if self.list.len() >= MAX_BUBBLES
                && let Some(oldest) = self
                    .list
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, bubble)| bubble.used)
                    .map(|(index, _)| index)
            {
                let gone = self.list.remove(oldest);
                if self.expanded.as_deref() == Some(gone.chat.as_str()) {
                    self.expanded = None;
                }
            }
            self.list.insert(
                0,
                Bubble {
                    account,
                    chat: chat.clone(),
                    text: String::new(),
                    staged: Vec::new(),
                    view: PaneView {
                        scroll_to_bottom: true,
                        at_bottom: true,
                        ..PaneView::default()
                    },
                    focus: false,
                    picking: false,
                    used: 0,
                },
            );
        }
        self.touch(&chat);
    }

    /// Opens `chat`'s bubble, closing the one that was open.
    fn expand(&mut self, chat: &str) {
        if self.get(chat).is_none() {
            return;
        }
        self.expanded = Some(chat.to_owned());
        self.touch(chat);
        if let Some(bubble) = self.get_mut(chat) {
            bubble.focus = true;
        }
    }

    fn remove(&mut self, chat: &str) -> Option<Bubble> {
        let index = self.list.iter().position(|bubble| bubble.chat == chat)?;
        if self.expanded.as_deref() == Some(chat) {
            self.expanded = None;
        }
        Some(self.list.remove(index))
    }
}

impl App {
    /// Applies a bubble's action; anything else is not one and is left.
    pub(super) fn apply_bubble(&mut self, action: Action, ctx: &egui::Context) {
        match action {
            Action::Bubble(chat) => self.open_bubble(chat),
            Action::ToggleBubble(chat) => {
                if self.bubbles.expanded.as_deref() == Some(chat.as_str()) {
                    self.bubbles.expanded = None;
                } else {
                    self.bubbles.expand(&chat);
                }
            }
            Action::CollapseBubbles => self.bubbles.expanded = None,
            Action::DismissBubble(chat) => {
                self.bubbles.remove(&chat);
            }
            Action::DockBubble(chat) => {
                if self.bubbles.remove(&chat).is_some() {
                    self.open_chat(chat);
                    ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Focus);
                }
            }
            Action::BubbleSend(chat) => self.send_from_bubble(chat),
            Action::BubbleAttach(chat) => {
                if let Some(bubble) = self.bubbles.get_mut(&chat) {
                    bubble.picking = true;
                    self.backend.send(Command::PickFiles {
                        chat,
                        documents: false,
                    });
                }
            }
            Action::BubbleUnstage { chat, index } => {
                if let Some(bubble) = self.bubbles.get_mut(&chat)
                    && index < bubble.staged.len()
                {
                    bubble.staged.remove(index);
                }
            }
            _ => {}
        }
    }

    /// Makes `chat` a bubble and opens it.
    fn open_bubble(&mut self, chat: ChatId) {
        if !self.settings.chat_bubbles || self.chat(&chat).is_none_or(|known| known.locked) {
            return;
        }
        // A chat is never in a pop-out and a bubble at once.
        if self.pop_out.as_ref().is_some_and(|pop| pop.chat == chat) {
            self.pop_out = None;
        }
        self.ensure_loaded(&chat);
        let account = self.account().id.clone();
        self.bubbles.add(account, chat.clone());
        self.bubbles.expand(&chat);
    }

    /// Sends what `chat`'s bubble holds: its attachments with the text as
    /// their caption, or the text alone. It never touches the open chat's
    /// composer, quote or edit.
    fn send_from_bubble(&mut self, chat: ChatId) {
        if !self.chat(&chat).is_some_and(|known| known.can_send()) {
            return;
        }
        let account = self.account().id.clone();
        let Some(bubble) = self
            .bubbles
            .get_mut(&chat)
            .filter(|bubble| bubble.account == account)
        else {
            return;
        };
        let text = bubble.text.trim().to_owned();
        if text.is_empty() && bubble.staged.is_empty() {
            return;
        }
        bubble.text.clear();
        let staged = std::mem::take(&mut bubble.staged);
        // Its own transcript follows what it sent.
        bubble.view.scroll_to_bottom = true;
        bubble.focus = true;
        self.stop_composing(&chat);
        if staged.is_empty() {
            self.backend.send(Command::SendText {
                chat,
                text,
                quoting: None,
                mentions: Vec::new(),
            });
        } else {
            let caption = Some(text).filter(|text| !text.is_empty());
            self.send_staged(&chat, staged, caption, Vec::new(), None);
        }
    }

    /// Files a picker opened from a bubble answered with, when one waits for
    /// them. Returns whether the bubble took them.
    pub(super) fn bubble_picked(&mut self, chat: &str, paths: &[PathBuf]) -> bool {
        let Some(bubble) = self.bubbles.get_mut(chat).filter(|bubble| bubble.picking) else {
            return false;
        };
        bubble.picking = false;
        bubble
            .staged
            .extend(paths.iter().cloned().map(Pending::File));
        bubble.focus = true;
        true
    }

    /// Takes what the bubble window was handed this frame: files dropped on
    /// it, or a paste of files or a picture (Ctrl/Cmd+V, read as the main
    /// window reads one). A text paste goes on to the reply field. `ctx` is
    /// the bubble window's.
    pub(crate) fn take_bubble_input(&mut self, ctx: &egui::Context, chat: &str) {
        let dropped: Vec<PathBuf> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .map(|file| file.path().to_path_buf())
                .collect()
        });
        let pasting = ctx.input(|input| {
            input.events.iter().any(|event| {
                matches!(event, egui::Event::Paste(_))
                    || matches!(
                        event,
                        egui::Event::Key {
                            key: egui::Key::V,
                            pressed: true,
                            modifiers,
                            ..
                        } if modifiers.command
                    )
            })
        });
        let pasted = if pasting && !cfg!(test) {
            super::clipboard_contents(super::clipboard_files, super::clipboard_image).ok()
        } else {
            None
        };
        let Some(bubble) = self.bubbles.get_mut(chat) else {
            return;
        };
        bubble.staged.extend(dropped.into_iter().map(Pending::File));
        if let Some(contents) = pasted {
            match contents {
                ClipboardPaste::Files(paths) => {
                    bubble.staged.extend(paths.into_iter().map(Pending::File));
                }
                ClipboardPaste::Image {
                    width,
                    height,
                    rgba,
                } => bubble.staged.push(Pending::Picture {
                    width,
                    height,
                    rgba: std::sync::Arc::new(rgba),
                    texture: None,
                }),
            }
            // The clipboard's text (a file's path, a picture's address) is
            // not typed into the reply as well.
            ctx.input_mut(|input| {
                input
                    .events
                    .retain(|event| !matches!(event, egui::Event::Paste(_)))
            });
        }
    }

    /// Runs `draw` with `chat`'s bubble's place swapped in for the open
    /// chat's, and swaps it back after.
    pub(crate) fn with_bubble_view<R>(
        &mut self,
        chat: &str,
        draw: impl FnOnce(&mut App) -> R,
    ) -> R {
        let Some(mut view) = self
            .bubbles
            .get_mut(chat)
            .map(|bubble| std::mem::take(&mut bubble.view))
        else {
            return draw(self);
        };
        view.swap_with(self);
        let out = draw(self);
        view.swap_with(self);
        if let Some(bubble) = self.bubbles.get_mut(chat) {
            bubble.view = view;
        }
        out
    }

    /// Lets go of what Settings has just turned off: a split and the pop-out
    /// with Split view, the bubbles with Chat bubbles.
    pub(super) fn drop_disabled_experiments(&mut self) {
        if !self.settings.split_view {
            for account in &mut self.accounts {
                account.split = None;
            }
            self.pop_out = None;
        }
        if !self.settings.chat_bubbles {
            self.bubbles.clear();
        }
    }

    /// Whether `action` may leave a bubble's window: what a pop-out allows,
    /// and the bubbles' own.
    pub fn bubble_allows(action: &Action) -> bool {
        App::pop_out_allows(action)
            || matches!(
                action,
                Action::Bubble(_)
                    | Action::ToggleBubble(_)
                    | Action::CollapseBubbles
                    | Action::DismissBubble(_)
                    | Action::DockBubble(_)
                    | Action::BubbleSend(_)
                    | Action::BubbleAttach(_)
                    | Action::BubbleUnstage { .. }
                    | Action::MarkRead(_)
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fourth_bubble_lets_the_oldest_go() {
        let mut bubbles = Bubbles::default();
        let account = AccountId::first();
        for chat in ["a", "b", "c"] {
            bubbles.add(account.clone(), chat.into());
        }
        // "a" was opened since, so "b" is the one used longest ago.
        bubbles.expand("a");
        bubbles.add(account.clone(), "d".into());
        let chats: Vec<_> = bubbles.list.iter().map(|b| b.chat.as_str()).collect();
        assert_eq!(chats, ["d", "c", "a"]);
        assert_eq!(bubbles.expanded.as_deref(), Some("a"));
        // An existing bubble is not added twice.
        bubbles.add(account, "c".into());
        assert_eq!(bubbles.list.len(), MAX_BUBBLES);
    }

    #[test]
    fn removing_the_open_bubble_closes_it() {
        let mut bubbles = Bubbles::default();
        bubbles.add(AccountId::first(), "a".into());
        bubbles.expand("a");
        assert!(bubbles.remove("a").is_some());
        assert!(bubbles.expanded.is_none() && bubbles.is_empty());
    }
}
