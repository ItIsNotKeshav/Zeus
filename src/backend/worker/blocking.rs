//! Use the library's blocking API; the archive keeps no copy of the list.

use super::*;

impl Worker {
    pub(super) fn publish_blocklist(&self, error: Option<crate::blocking::Failure>) {
        self.emit(Event::Blocklist {
            state: self.blocklist.clone(),
            error,
        });
    }

    pub(super) fn fetch_blocklist(&mut self) {
        let Some(client) = self.connected_client() else {
            return;
        };
        if self.blocklist.loading || self.blocklist.pending {
            return;
        }
        self.blocklist.loading = true;
        self.blocklist.failed = false;
        self.blocklist.generation = self.blocklist.generation.wrapping_add(1);
        let generation = self.blocklist.generation;
        self.publish_blocklist(None);
        let commands = self.commands.clone();
        tokio::spawn(async move {
            let result = client
                .blocking()
                .get_blocklist()
                .await
                .map(|entries| {
                    entries
                        .into_iter()
                        .map(|entry| entry.jid.to_non_ad_string())
                        .collect()
                })
                .map_err(|_| ());
            let _ = commands.send(Command::BlocklistFetched { generation, result });
        });
    }

    pub(super) async fn blocklist_fetched(
        &mut self,
        generation: u64,
        result: Result<Vec<ChatId>, ()>,
    ) {
        if generation != self.blocklist.generation {
            return;
        }
        self.blocklist.failed = result.is_err();
        if let Ok(entries) = result {
            let mut ids = HashSet::new();
            for id in entries {
                if let Some(jid) = Self::jid_of(&id) {
                    let id = self.canonical_sync_chat(&jid).await;
                    if crate::blocking::eligible(&id, self.me_pn.as_deref(), self.me_lid.as_deref())
                    {
                        ids.insert(id);
                    }
                }
            }
            self.blocklist.ids = Some(ids);
        }
        self.blocklist.loading = false;
        self.publish_blocklist(None);
    }

    pub(super) fn set_blocked(&mut self, chat: ChatId, blocked: bool) {
        let chat = self.canonical_str(&chat);
        if !crate::blocking::eligible(&chat, self.me_pn.as_deref(), self.me_lid.as_deref()) {
            return;
        }
        let (Some(client), Some(jid)) = (self.connected_client(), Self::jid_of(&chat)) else {
            self.publish_blocklist(Some(crate::blocking::Failure::Offline));
            return;
        };
        let Some(generation) = self.blocklist.begin_change() else {
            return;
        };
        self.publish_blocklist(None);
        let commands = self.commands.clone();
        tokio::spawn(async move {
            let result = if blocked {
                client.blocking().block(&jid).await
            } else {
                client.blocking().unblock(&jid).await
            }
            .map_err(|_| ());
            let _ = commands.send(Command::BlockedSet {
                generation,
                chat,
                blocked,
                result,
            });
        });
    }

    pub(super) fn blocked_set(
        &mut self,
        generation: u64,
        chat: ChatId,
        blocked: bool,
        result: Result<(), ()>,
    ) {
        if generation != self.blocklist.generation {
            return;
        }
        self.blocklist.pending = false;
        if result.is_err() {
            self.publish_blocklist(Some(crate::blocking::Failure::Refused));
            return;
        }
        let chat = self.canonical_str(&chat);
        let ids = self.blocklist.ids.get_or_insert_default();
        if blocked {
            ids.insert(chat);
        } else {
            ids.remove(&chat);
        }
        self.publish_blocklist(None);
        self.fetch_blocklist();
    }

    pub(super) fn is_blocked(&self, chat: &str) -> bool {
        self.blocklist.contains(&self.canonical_str(chat))
    }

    /// Restore composed content if the block arrived after the UI queued a send.
    pub(super) fn refuse_blocked(&self, command: Command) {
        let (chat, quoting, unsent) = match command {
            Command::SendText {
                chat,
                text,
                quoting,
                ..
            } => (chat, quoting, Unsent::Text(text)),
            Command::SendLocation {
                chat,
                latitude,
                longitude,
                quoting,
            } => (
                chat,
                quoting,
                Unsent::Location {
                    latitude,
                    longitude,
                },
            ),
            Command::SendFiles {
                chat,
                paths,
                documents,
                view_once,
                caption,
                quoting,
                ..
            } => (
                chat,
                quoting,
                Unsent::Files {
                    paths,
                    documents,
                    view_once,
                    caption,
                },
            ),
            Command::SendImage {
                chat,
                width,
                height,
                rgba,
                view_once,
                caption,
                quoting,
                ..
            } => (
                chat,
                quoting,
                Unsent::Image {
                    width,
                    height,
                    rgba,
                    view_once,
                    caption,
                },
            ),
            Command::SendVoice {
                chat,
                samples,
                quoting,
            } => (chat, quoting, Unsent::Voice(samples)),
            Command::SendSticker { chat, quoting, .. } => (chat, quoting, Unsent::Sticker),
            Command::SendGif { chat, quoting, .. } => (chat, quoting, Unsent::Gif),
            Command::CreatePoll { chat, .. } => {
                self.emit(Event::PollCreated {
                    chat,
                    error: Some("Unblock this contact to send messages.".into()),
                });
                return;
            }
            _ => {
                self.emit(Event::Error(
                    "Unblock this contact to send messages.".into(),
                ));
                return;
            }
        };
        self.refuse(chat, quoting, unsent, Refusal::Blocked);
    }
}
