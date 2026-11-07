//! Which conversations stay in memory. The open chat of each account keeps
//! everything it has loaded. The two chats left most recently keep their
//! newest page, so flipping between chats reads nothing again; any other chat
//! left is dropped, and opening it reads one page from the archive on the
//! worker, as a first open does. A chat nobody opened has no conversation at
//! all: its messages wait in the archive. This is settled when a chat is
//! left, never on a timer.

use std::time::Instant;

use super::{App, Conversation, PAGE};
use crate::account::Account;
use crate::model::{ChatId, Dialog};

/// Chats left whose conversations stay in memory beside the open one.
const KEPT_CONVERSATIONS: usize = 2;

/// What a dropped conversation knew of the phone's history, so opening its
/// chat again asks the phone no sooner than the backoff allows, and not at all
/// once it said nothing older is left.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PhoneHistory {
    pub(super) exhausted: bool,
    answered: Option<Instant>,
    misses: u32,
    delivered: bool,
}

impl Conversation {
    /// Whether an older page or the phone's history is on its way. It lands
    /// next to the rows it was asked from, so they stay until it is in.
    fn busy(&self) -> bool {
        self.loading_older || self.fetching_phone
    }

    fn phone_history(&self) -> PhoneHistory {
        PhoneHistory {
            exhausted: self.phone_exhausted,
            answered: self.phone_answered,
            misses: self.phone_misses,
            delivered: self.phone_delivered,
        }
    }

    /// Takes back what a dropped conversation knew of the phone's history.
    pub(super) fn resume_phone(&mut self, phone: PhoneHistory) {
        self.phone_exhausted = phone.exhausted;
        self.phone_answered = phone.answered;
        self.phone_misses = phone.misses;
        self.phone_delivered = phone.delivered;
    }

    /// Lets the rows older than the newest `keep` go, except a `held` message
    /// and everything after it, so what stays is one unbroken stretch up to
    /// the newest message. The archive pages the rest back in.
    fn trim(&mut self, keep: usize, held: &[&str]) {
        let mut from = self.messages.len().saturating_sub(keep);
        for id in held {
            if let Some(index) = self.messages.iter().position(|message| message.id == *id) {
                from = from.min(index);
            }
        }
        if from == 0 {
            return;
        }
        for message in self.messages.drain(..from) {
            self.rows.remove(&message.id);
        }
        // A long history leaves a buffer far larger than one page.
        if self.messages.capacity() > self.messages.len() * 2 {
            self.messages.shrink_to_fit();
        }
        if self.rows.capacity() > self.rows.len() * 2 {
            self.rows.shrink_to_fit();
        }
        // What went is in the archive, so scrolling up pages it again.
        self.complete = false;
        self.key_scroll = None;
    }
}

impl App {
    /// Trims and drops the conversations of chats left since this was last
    /// done, in every account. Costs a comparison per account when no chat
    /// was left.
    pub(super) fn settle_conversations(&mut self) {
        if !self.accounts.iter().any(unsettled) {
            return;
        }
        let window = self.window_holds();
        let active = self.active;
        for (index, account) in self.accounts.iter_mut().enumerate() {
            if unsettled(account) {
                let holds = if index == active {
                    &window[..]
                } else {
                    &[][..]
                };
                settle(account, holds);
            }
        }
    }

    /// Messages the window still points at, in chats that need not be open:
    /// a quote waiting in the composer and what a dialog is about.
    fn window_holds(&self) -> Vec<(ChatId, String)> {
        let mut holds = Vec::new();
        if let (Some(target), Some(open)) = (&self.reply_to, self.open_chat.as_deref()) {
            holds.push((target.source_chat(open).to_owned(), target.id.clone()));
        }
        match &self.dialog {
            Some(
                Dialog::ConfirmDeleteMessage { chat, message, .. }
                | Dialog::PollResults { chat, message }
                | Dialog::InteractiveList { chat, message, .. }
                | Dialog::MessageInfo { chat, message }
                | Dialog::Reactions { chat, message },
            ) => holds.push((chat.clone(), message.clone())),
            Some(
                Dialog::ConfirmDeleteSelection { chat, messages, .. }
                | Dialog::Forward { chat, messages },
            ) => holds.extend(
                messages
                    .iter()
                    .map(|message| (chat.clone(), message.clone())),
            ),
            _ => {}
        }
        holds
    }

    /// Trims the open chat of every account to its newest page once the
    /// window is gone. A new window starts every chat at its end, whatever
    /// the old one had scrolled to, so the older rows would only sit in
    /// memory until the window comes back. What the window still points at
    /// stays, and so does the chat while a jump, a voice run or a sweep needs
    /// its rows; nothing is trimmed while the window shows.
    pub(super) fn trim_open_conversations(&mut self) {
        let window = self.window_holds();
        let extra = self.hidden_window_holds();
        let active = self.active;
        for (index, account) in self.accounts.iter_mut().enumerate() {
            if index == active {
                if let Some(extra) = &extra {
                    trim_open(account, &window, extra);
                }
            } else {
                trim_open(account, &[], &[]);
            }
        }
    }

    /// The messages of the open chat that the window's own state points at,
    /// beyond what `window_holds` lists, or `None` while something under way
    /// needs the whole conversation: a jump to a message still to load, a
    /// quote waiting for its message, a sweep, the voice message player or
    /// the unread divider before the transcript has reached it.
    fn hidden_window_holds(&self) -> Option<Vec<String>> {
        let open = self.open_chat.as_deref()?;
        let divider = self
            .unread_divider
            .as_ref()
            .filter(|divider| divider.chat == open);
        if self.scroll_anchor.is_some()
            || self.reply_when_loaded.is_some()
            || self.sweep.is_some()
            || self.player.is_playing()
            || self.voice_wanted.is_some()
            || divider.is_some_and(|divider| !divider.placed)
        {
            return None;
        }
        let mut holds: Vec<String> = Vec::new();
        holds.extend(self.editing.clone());
        if let Some((chat, messages)) = &self.selection
            && chat == open
        {
            holds.extend(messages.iter().cloned());
        }
        if let Some((chat, message)) = &self.reaction_target
            && chat == open
        {
            holds.push(message.clone());
        }
        if let Some(jump) = &self.jump_highlight
            && jump.chat == open
        {
            holds.push(jump.message.clone());
        }
        holds.extend(divider.and_then(|divider| divider.first.clone()));
        Some(holds)
    }

    /// The conversation of `chat` in memory: the open chat's, made if need
    /// be, or one the reader left recently. A chat dropped from memory, or
    /// never opened, has none: it reads its messages from the archive when
    /// it opens, so an event about it has nothing here to change.
    pub(super) fn resident_conversation(&mut self, chat: &str) -> Option<&mut Conversation> {
        let account = &mut self.accounts[self.active];
        if account.open_chat.as_deref() == Some(chat) {
            return Some(account.conversations.entry(chat.to_owned()).or_default());
        }
        account.conversations.get_mut(chat)
    }
}

/// Whether a chat was left since the account's conversations were settled,
/// by any route. Nothing is dropped without a backend to read it back: an
/// offline demo or test has no archive behind it.
fn unsettled(account: &Account) -> bool {
    !account.backend.is_offline()
        && (account.left_pending || account.settled_open != account.open_chat)
}

/// Keeps the open chat's conversation whole, trims the two chats left last
/// to their newest page and drops the rest. One the window or a parked quote
/// points at is trimmed down to that message and kept, and one waiting for
/// an older page is left alone. `window` holds the messages the window
/// points at, for the account on screen.
fn settle(account: &mut Account, window: &[(ChatId, String)]) {
    account.left_pending = false;
    if account.settled_open != account.open_chat {
        // Left by a route that did not say so, such as a lock or a hidden
        // account's event.
        if let Some(left) = account.settled_open.take()
            && !account.left_chats.contains(&left)
        {
            account.left_chats.insert(0, left);
        }
        account.settled_open.clone_from(&account.open_chat);
    }
    let open = account.open_chat.clone();
    let conversations = &account.conversations;
    account
        .left_chats
        .retain(|chat| open.as_ref() != Some(chat) && conversations.contains_key(chat));
    // A quote parked with a draft keeps its message, wherever it came from.
    let mut holds: Vec<(&str, &str)> = window
        .iter()
        .map(|(chat, message)| (chat.as_str(), message.as_str()))
        .collect();
    holds.extend(
        account
            .attachment_drafts
            .iter()
            .filter_map(|(destination, draft)| {
                let target = draft.quoting.as_ref()?;
                Some((target.source_chat(destination), target.id.as_str()))
            }),
    );
    let mut kept = 0;
    let mut dropped = Vec::new();
    for chat in &account.left_chats {
        let Some(conversation) = account.conversations.get_mut(chat) else {
            continue;
        };
        if conversation.busy() {
            continue;
        }
        let held: Vec<&str> = holds
            .iter()
            .filter(|(source, _)| *source == chat.as_str())
            .map(|(_, id)| *id)
            .collect();
        if !held.is_empty() {
            // Kept as long as what holds it, at no cost to the others.
            conversation.trim(PAGE, &held);
        } else if kept < KEPT_CONVERSATIONS {
            conversation.trim(PAGE, &[]);
            kept += 1;
        } else {
            dropped.push(chat.clone());
        }
    }
    for chat in dropped {
        if let Some(conversation) = account.conversations.remove(&chat) {
            let phone = conversation.phone_history();
            if phone != PhoneHistory::default() {
                account.phone_history.insert(chat.clone(), phone);
            }
        }
        account.left_chats.retain(|left| *left != chat);
    }
}

/// Trims `account`'s open conversation to its newest page, keeping the
/// messages `window` and `extra` point at and a quote parked with a draft.
/// One waiting for an older page is left alone, and so is any conversation
/// without a backend to read it back.
fn trim_open(account: &mut Account, window: &[(ChatId, String)], extra: &[String]) {
    if account.backend.is_offline() {
        return;
    }
    let Some(open) = account.open_chat.as_deref() else {
        return;
    };
    let Some(conversation) = account.conversations.get_mut(open) else {
        return;
    };
    if conversation.busy() {
        return;
    }
    let mut held: Vec<&str> = window
        .iter()
        .filter(|(chat, _)| chat.as_str() == open)
        .map(|(_, message)| message.as_str())
        .collect();
    held.extend(extra.iter().map(String::as_str));
    held.extend(
        account
            .attachment_drafts
            .iter()
            .filter_map(|(destination, draft)| {
                let target = draft.quoting.as_ref()?;
                (target.source_chat(destination) == open).then_some(target.id.as_str())
            }),
    );
    conversation.trim(PAGE, &held);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{Backend, Command, Event};
    use crate::model::{AccountId, Action, Chat, Content, Delivery, Message, ReplyTarget};
    use crate::paths::AppDirs;
    use crate::settings::Settings;

    type Commands = tokio::sync::mpsc::UnboundedReceiver<Command>;

    /// An app whose backend answers, so its conversations may be dropped.
    fn online(
        directory: &std::path::Path,
        chats: &[&str],
    ) -> (App, Commands, std::sync::mpsc::Sender<Event>) {
        let mut app = App::headless(AppDirs::under(directory), Settings::default()).0;
        let (backend, commands, events) = Backend::recording_with_events();
        app.backend = backend;
        for chat in chats {
            app.chats.push(Chat::new((*chat).into(), (*chat).into()));
        }
        (app, commands, events)
    }

    fn message(chat: &str, id: &str, timestamp: i64) -> Message {
        Message {
            id: id.into(),
            chat: chat.into(),
            sender: chat.into(),
            sender_name: None,
            from_me: false,
            timestamp,
            history_order: None,
            content: Content::text(id),
            status: Delivery::None,
            delivered_at: None,
            read_at: None,
            quoted: None,
            reactions: Vec::new(),
            edited: false,
            mentions: Vec::new(),
            forwarded: false,
            thumbnail: None,
        }
    }

    /// `count` messages of `chat`, `m0` the oldest.
    fn rows(chat: &str, count: i64) -> Vec<Message> {
        (0..count)
            .map(|index| message(chat, &format!("m{index}"), index))
            .collect()
    }

    /// Opens `chat` as a click does, then gives it `count` messages, as the
    /// archive's answer would.
    fn open_with(app: &mut App, chat: &str, count: i64) {
        app.actions.push(Action::OpenChat(chat.into()));
        app.apply_actions(&egui::Context::default());
        let conversation = app.conversations.get_mut(chat).expect("opened");
        conversation.merge(rows(chat, count), false);
        conversation.complete = true;
    }

    fn left(app: &App) -> Vec<&str> {
        app.left_chats.iter().map(String::as_str).collect()
    }

    const CHATS: [&str; 4] = [
        "0@s.whatsapp.net",
        "1@s.whatsapp.net",
        "2@s.whatsapp.net",
        "3@s.whatsapp.net",
    ];

    #[test]
    fn a_live_message_for_a_chat_not_in_memory_makes_no_conversation() {
        let directory = tempfile::tempdir().unwrap();
        let (open, other) = (CHATS[0], CHATS[1]);
        let (mut app, _commands, events) = online(directory.path(), &CHATS);
        app.open_chat(open.into());
        for event in [
            Event::Messages {
                chat: other.into(),
                messages: vec![message(other, "live", 10)],
                older: false,
                complete: false,
            },
            Event::OlderFetched {
                chat: other.into(),
                more: true,
                silent: true,
            },
            Event::Messages {
                chat: open.into(),
                messages: vec![message(open, "here", 10)],
                older: false,
                complete: false,
            },
        ] {
            events.send(event).unwrap();
        }
        app.handle_events();
        assert!(
            !app.conversations.contains_key(other),
            "the message waits in the archive until its chat opens"
        );
        assert!(app.conversations[open].message("here").is_some());
    }

    #[test]
    fn the_two_chats_left_last_keep_their_newest_page_and_the_rest_go() {
        let directory = tempfile::tempdir().unwrap();
        let (mut app, mut commands, _events) = online(directory.path(), &CHATS);
        for chat in CHATS {
            open_with(&mut app, chat, 100);
        }
        assert_eq!(
            app.conversations[CHATS[3]].messages.len(),
            100,
            "the open chat keeps everything it loaded"
        );
        assert_eq!(left(&app), [CHATS[2], CHATS[1]]);
        for chat in &CHATS[1..3] {
            let kept = &app.conversations[*chat];
            assert_eq!(kept.messages.len(), PAGE);
            assert_eq!(kept.messages[0].id, format!("m{}", 100 - PAGE));
            assert_eq!(kept.messages[PAGE - 1].id, "m99");
            assert!(!kept.complete, "scrolling up pages the archive again");
        }
        assert!(!app.conversations.contains_key(CHATS[0]));
        // Flipping back to a kept chat reads nothing again.
        while commands.try_recv().is_ok() {}
        app.actions.push(Action::OpenChat(CHATS[2].into()));
        app.apply_actions(&egui::Context::default());
        assert!(
            !std::iter::from_fn(|| commands.try_recv().ok())
                .any(|command| matches!(command, Command::LoadChat { .. }))
        );
        assert_eq!(app.conversations[CHATS[2]].messages.len(), PAGE);
        assert_eq!(left(&app), [CHATS[3], CHATS[1]]);
        assert_eq!(app.conversations[CHATS[3]].messages.len(), PAGE);
    }

    #[test]
    fn a_dropped_chat_opens_again_with_one_page_like_a_first_open() {
        let directory = tempfile::tempdir().unwrap();
        let (mut app, mut commands, events) = online(directory.path(), &CHATS);
        open_with(&mut app, CHATS[0], 100);
        // The phone said it has nothing older for this chat.
        app.conversations.get_mut(CHATS[0]).unwrap().phone_exhausted = true;
        for chat in &CHATS[1..] {
            open_with(&mut app, chat, 10);
        }
        assert!(!app.conversations.contains_key(CHATS[0]));
        while commands.try_recv().is_ok() {}

        app.actions.push(Action::OpenChat(CHATS[0].into()));
        app.apply_actions(&egui::Context::default());
        let asked = std::iter::from_fn(|| commands.try_recv().ok()).any(|command| {
            matches!(command, Command::LoadChat { chat, before: None } if chat == CHATS[0])
        });
        assert!(
            asked,
            "the newest page is read from the archive, on the worker"
        );
        let conversation = &app.conversations[CHATS[0]];
        assert!(conversation.requested && conversation.messages.is_empty());
        assert!(
            conversation.phone_exhausted,
            "the phone is not asked again for what it said is gone"
        );
        let page = rows(CHATS[0], 100).split_off(40);
        events
            .send(Event::Messages {
                chat: CHATS[0].into(),
                messages: page,
                older: false,
                complete: false,
            })
            .unwrap();
        app.handle_events();
        assert_eq!(app.conversations[CHATS[0]].messages.len(), 60);
        assert!(
            app.scroll_to_bottom,
            "the reader lands on the newest messages"
        );
    }

    #[test]
    fn a_cleared_chat_not_in_memory_still_asks_the_phone_for_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let (open, other) = (CHATS[0], CHATS[1]);
        let (mut app, _commands, _events) = online(directory.path(), &CHATS);
        app.open_chat(open.into());
        app.handle_chat_cleared(other, 100, true);
        assert!(!app.conversations.contains_key(other));
        app.open_chat(other.into());
        assert!(app.conversations[other].phone_exhausted);
    }

    #[test]
    fn a_left_chat_waiting_for_an_older_page_keeps_its_rows_until_it_lands() {
        let directory = tempfile::tempdir().unwrap();
        let (mut app, _commands, events) = online(directory.path(), &CHATS);
        open_with(&mut app, CHATS[0], 100);
        app.conversations.get_mut(CHATS[0]).unwrap().loading_older = true;
        open_with(&mut app, CHATS[1], 10);
        open_with(&mut app, CHATS[2], 10);
        open_with(&mut app, CHATS[3], 10);
        assert_eq!(
            app.conversations[CHATS[0]].messages.len(),
            100,
            "the page lands next to the oldest row it was asked from"
        );
        events
            .send(Event::Messages {
                chat: CHATS[0].into(),
                messages: vec![message(CHATS[0], "older", -1)],
                older: true,
                complete: false,
            })
            .unwrap();
        app.handle_events();
        // The next chat left settles it with the others.
        open_with(&mut app, CHATS[1], 10);
        assert!(!app.conversations.contains_key(CHATS[0]));
    }

    #[test]
    fn a_quote_from_a_left_chat_keeps_its_message_and_what_follows() {
        let directory = tempfile::tempdir().unwrap();
        let group = "group@g.us";
        let (mut app, _commands, _events) = online(directory.path(), &[group]);
        for chat in CHATS {
            app.chats.push(Chat::new(chat.into(), chat.into()));
        }
        open_with(&mut app, group, 100);
        // Reply privately to an old group message: the member's chat opens
        // and the quote starts in the same go.
        app.open_chat(CHATS[0].into());
        app.reply_to = Some(ReplyTarget {
            id: "m10".into(),
            chat: Some(group.into()),
        });
        app.settle_conversations();
        // The quote waits with the reply's draft while other chats open.
        for chat in &CHATS[1..] {
            open_with(&mut app, chat, 10);
        }
        let kept = &app.conversations[group];
        assert_eq!(kept.messages.len(), 90);
        assert_eq!(kept.messages[0].id, "m10");
        assert_eq!(kept.messages[89].id, "m99");
        app.actions.push(Action::OpenChat(CHATS[0].into()));
        app.apply_actions(&egui::Context::default());
        let quote = app
            .reply_to
            .clone()
            .expect("the quote came back with its chat");
        assert!(app.reply_preview(&quote, CHATS[0]).is_some());
    }

    #[test]
    fn a_hidden_account_settles_the_chats_it_leaves_too() {
        let directory = tempfile::tempdir().unwrap();
        let (mut app, _commands, _events) = online(directory.path(), &[]);
        let (mut second, _events) = Account::detached(
            &AppDirs::under(directory.path()),
            AccountId::parse("2").unwrap(),
            crate::settings::AccountSettings::default(),
        )
        .unwrap();
        second.backend = Backend::recording().0;
        for chat in CHATS {
            second
                .conversations
                .entry(chat.into())
                .or_default()
                .merge(rows(chat, 100), false);
        }
        app.accounts.push(second);
        // Its open chat changes only through its own events.
        for chat in CHATS {
            app.accounts[1].open_chat = Some(chat.into());
            app.settle_conversations();
        }
        let second = &app.accounts[1];
        assert_eq!(second.conversations[CHATS[3]].messages.len(), 100);
        assert_eq!(second.conversations[CHATS[2]].messages.len(), PAGE);
        assert_eq!(second.conversations[CHATS[1]].messages.len(), PAGE);
        assert!(!second.conversations.contains_key(CHATS[0]));
    }

    #[test]
    fn a_gone_window_trims_the_open_chat_but_keeps_what_it_points_at() {
        let directory = tempfile::tempdir().unwrap();
        let (mut app, _commands, _events) = online(directory.path(), &CHATS);
        open_with(&mut app, CHATS[0], 100);
        app.window_gone();
        let kept = &app.conversations[CHATS[0]];
        assert_eq!(kept.messages.len(), PAGE);
        assert_eq!(kept.messages[0].id, format!("m{}", 100 - PAGE));
        assert!(!kept.complete, "scrolling up pages the archive again");

        // A quote waiting in the composer keeps its message and what follows.
        open_with(&mut app, CHATS[1], 100);
        app.reply_to = Some(ReplyTarget {
            id: "m10".into(),
            chat: None,
        });
        app.window_gone();
        let kept = &app.conversations[CHATS[1]];
        assert_eq!(kept.messages.len(), 90);
        assert_eq!(kept.messages[0].id, "m10");

        // A jump still waiting for its message needs the rows it has.
        app.reply_to = None;
        open_with(&mut app, CHATS[2], 100);
        app.scroll_anchor = Some("m3".into());
        app.window_gone();
        assert_eq!(app.conversations[CHATS[2]].messages.len(), 100);
    }

    #[test]
    fn nothing_is_dropped_without_a_backend_to_read_it_back() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = App::headless(AppDirs::under(directory.path()), Settings::default()).0;
        for chat in CHATS {
            app.chats.push(Chat::new(chat.into(), chat.into()));
        }
        for chat in CHATS {
            open_with(&mut app, chat, 100);
        }
        for chat in CHATS {
            assert_eq!(app.conversations[chat].messages.len(), 100);
        }
    }
}
