//! Files archived chats from history sync after the main chats.
//!
//! The phone decides what each history chunk holds, and big archived groups
//! come mixed in with the chats the reader actually uses. Filing a chunk in
//! one go makes the main chat list wait for every archived group in it, and
//! anything the reader asks for meanwhile waits too.
//!
//! So a chunk is split: the chats that are not archived are filed at once,
//! and the archived ones go to a queue the worker files one chat at a time
//! when it has nothing else to do. Opening an archived chat that is still
//! queued files it right away. Older messages of an archived chat are not
//! fetched in the background ([`super::prefetch`] leaves archived chats out);
//! scrolling to the top asks the phone for them, as for any chat.

use super::*;

impl Worker {
    /// Takes the archived chats out of a history chunk and queues them, so
    /// the rest of the chunk can be filed first. Returns how many were queued.
    ///
    /// A chat that already has a piece waiting is queued behind it, whatever
    /// this chunk says, so the pieces of one chat are filed in the order the
    /// phone sent them: an older chunk filed last would roll the chat's
    /// metadata back.
    pub(super) fn defer_archived_history(&mut self, parsed: &mut ParsedHistory) -> usize {
        // Privacy-id pairs are learned before the split: they name chats in
        // both halves, and the canonical id decides what the archive knows.
        for chat in &parsed.chats {
            if let (Some(lid), Some(pn)) = (&chat.lid_jid, &chat.pn_jid)
                && let (Some(lid), Some(pn)) = (Self::jid_of(lid), Self::jid_of(pn))
            {
                self.learn_pair(&lid, &pn);
            }
        }
        let mut now = Vec::with_capacity(parsed.chats.len());
        let mut deferred = 0;
        for chat in std::mem::take(&mut parsed.chats) {
            match self.defer_archived_chat(chat) {
                Some(chat) => now.push(chat),
                None => deferred += 1,
            }
        }
        parsed.chats = now;
        deferred
    }

    /// Queues one history chat when it is archived, or when a piece of it
    /// already waits; otherwise hands it back to be filed now. A chunk
    /// filed one chat at a time comes through here chat by chat, its ids
    /// already learned from the chunk's first pass.
    pub(super) fn defer_archived_chat(&mut self, chat: ParsedChat) -> Option<ParsedChat> {
        let id = self.canonical_str(&chat.id);
        let queued = self
            .deferred_history
            .iter()
            .any(|(queued, _)| *queued == id);
        if queued || self.history_chat_archived(&id, &chat) {
            self.deferred_history.push_back((id, chat));
            None
        } else {
            Some(chat)
        }
    }

    /// Whether a history chat belongs to the archived list: what the chunk
    /// says, else what the archive already holds.
    fn history_chat_archived(&self, id: &str, chat: &ParsedChat) -> bool {
        chat.archived.unwrap_or_else(|| {
            self.archive
                .chat(id)
                .ok()
                .flatten()
                .is_some_and(|row| row.archived)
        })
    }

    /// Files the next queued archived chat. The worker calls this only when
    /// no command or WhatsApp event is waiting.
    pub(super) fn file_deferred_history(&mut self) {
        let Some((_, chat)) = self.deferred_history.pop_front() else {
            return;
        };
        self.file_history_chats(vec![chat]);
        if self.deferred_history.is_empty() {
            log::info!("history: archived chats filed");
            self.emit_chats();
        }
    }

    /// Files a queued chat now, because the reader opened it.
    pub(super) fn file_deferred_chat(&mut self, id: &str) {
        let (chats, rest): (VecDeque<_>, VecDeque<_>) = std::mem::take(&mut self.deferred_history)
            .into_iter()
            .partition(|(queued, _)| queued == id);
        self.deferred_history = rest;
        if chats.is_empty() {
            return;
        }
        self.file_history_chats(chats.into_iter().map(|(_, chat)| chat).collect());
    }

    fn file_history_chats(&mut self, chats: Vec<ParsedChat>) {
        self.apply_history(
            ParsedHistory {
                chats,
                push_names: Vec::new(),
                lids: Vec::new(),
                stickers: Vec::new(),
            },
            true,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::super::chat_removal_tests::history;
    use super::super::receipt_tests::worker;
    use super::*;

    const MAIN: &str = "111@s.whatsapp.net";
    const ARCHIVED: &str = "222-333@g.us";

    fn chunk() -> ParsedHistory {
        let mut parsed = history(MAIN, &[10, 20]);
        let mut archived = history(ARCHIVED, &[5, 6, 7]);
        archived.chats[0].archived = Some(true);
        parsed.chats.append(&mut archived.chats);
        parsed
    }

    #[test]
    fn archived_chats_wait_for_the_main_ones() {
        let (mut worker, _events, _inbox, _wa) = worker();
        let mut parsed = chunk();
        assert_eq!(worker.defer_archived_history(&mut parsed), 1);
        worker.apply_history(parsed, true);
        assert!(worker.archive.chat(MAIN).unwrap().is_some());
        assert!(worker.archive.chat(ARCHIVED).unwrap().is_none());

        worker.file_deferred_history();
        let row = worker.archive.chat(ARCHIVED).unwrap().unwrap();
        assert!(row.archived);
        assert!(worker.deferred_history.is_empty());
    }

    #[test]
    fn a_later_piece_of_a_queued_chat_waits_behind_it() {
        let (mut worker, _events, _inbox, _wa) = worker();
        let mut parsed = chunk();
        worker.defer_archived_history(&mut parsed);
        // The next chunk omits the archive flag: still queued, in order.
        let mut later = history(ARCHIVED, &[8]);
        assert_eq!(worker.defer_archived_history(&mut later), 1);
        assert!(later.chats.is_empty());
        assert_eq!(worker.deferred_history.len(), 2);
    }

    #[test]
    fn opening_a_queued_chat_files_it_at_once() {
        let (mut worker, _events, _inbox, _wa) = worker();
        let mut parsed = chunk();
        worker.defer_archived_history(&mut parsed);
        worker.file_deferred_chat(ARCHIVED);
        assert!(worker.archive.chat(ARCHIVED).unwrap().is_some());
        assert!(worker.deferred_history.is_empty());
    }
}
