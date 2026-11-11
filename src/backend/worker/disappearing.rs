//! A chat's disappearing-message timer: changing it, and the notice a change
//! leaves in the conversation.
//!
//! A direct change is an `EPHEMERAL_SETTING` message. The send's `Ok` only
//! means the stanza left the socket, so the timer is written from the server's
//! ack, through the worker's shared ack map (`AwaitingAck::Timer`). A group
//! change is an IQ, and the server's own group notification carries the
//! change's timestamp and actor; the accepted IQ only refreshes the metadata,
//! so a newer change is never overwritten and no notice is invented.
//!
//! Both paths file one system row, whose id is the change's own, so a
//! redelivery replaces it rather than making a second one.

use super::*;

/// How long a sent timer change waits for its server ack before the control
/// is re-enabled and the change is reported as failed.
const TIMER_ACK_PATIENCE: Duration = Duration::from_secs(30);
/// How long after that a late ack is still applied, without a second report.
const TIMER_LATE_ACK_PATIENCE: Duration = Duration::from_secs(5 * 60);
/// How long an ack that outran its `TimerSet` is held, and how many.
const EARLY_ACK_TTL: Duration = Duration::from_secs(60);
const EARLY_ACK_LIMIT: usize = 256;

/// A direct timer change whose sent message is waiting for the server's ack.
#[derive(Clone)]
pub(super) struct TimerAck {
    chat: ChatId,
    seconds: u32,
    /// When the send built the setting, the clock the archive compares against
    /// the phone's settings. The ack's own time is a different clock and
    /// ordering by it could overwrite a newer change made on the phone.
    sent_at: i64,
    /// When to report the change as unanswered.
    deadline: Instant,
    /// The deadline was already reported: the control is enabled again, and an
    /// ack that arrives later still applies the timer without a second report.
    reported: bool,
}

/// A server ack that arrived before its `TimerSet`, held until the send names
/// the id it belongs to. Exact id matching keeps an ordinary message's ack
/// from being mistaken for a timer change.
pub(super) struct EarlyAck {
    pub(super) error: Option<String>,
    at: Instant,
}

impl Worker {
    /// Changes a chat's disappearing-message timer using the direct or group
    /// WhatsApp API. A direct change is written only once the server acks it;
    /// a group's is written from the server's group notification.
    pub(super) fn set_disappearing_timer(&mut self, chat: ChatId, seconds: u32) {
        let is_group = ChatKind::from_id(&chat) == ChatKind::Group;
        // A member of a locked group gets no menu, and one that asks anyway
        // gets nothing sent: WhatsApp would refuse the change.
        if is_group
            && !self
                .archive
                .chat(&chat)
                .ok()
                .flatten()
                .is_some_and(|row| row.can_edit_info())
        {
            self.emit(Event::Error(
                "Only admins can change disappearing messages.".to_owned(),
            ));
            return;
        }
        // A send on a dropped socket would fail as "client is not connected";
        // say to reconnect instead, as the other send paths do.
        let (Some(client), Some(jid)) = (self.connected_client(), Self::jid_of(&chat)) else {
            self.emit(Event::Error(
                "Connect to WhatsApp to change disappearing messages.".to_owned(),
            ));
            return;
        };
        self.timer_saving(&chat, true);
        let session = self.account_session;
        let commands = self.commands.clone();
        let waker = self.waker.clone();
        tokio::spawn(async move {
            if is_group {
                let result = client
                    .groups()
                    .set_ephemeral(jid, seconds)
                    .await
                    .map_err(|error| error.to_string());
                let _ = commands.send(Command::GroupEdited {
                    chat,
                    edit: GroupEdit::Timer(seconds),
                    result,
                });
            } else {
                // The timer's own setting timestamp versions it; the library
                // embeds it in the message it sent, so read it from there
                // rather than from any other clock.
                let result = client
                    .set_chat_disappearing_timer(jid, seconds)
                    .await
                    .map(|sent| {
                        let setting = sent
                            .message
                            .get_base_message()
                            .protocol_message
                            .as_option()
                            .and_then(|protocol| protocol.ephemeral_setting_timestamp);
                        (
                            sent.message_id.to_string(),
                            setting.unwrap_or_else(crate::util::now),
                        )
                    })
                    .map_err(|error| error.to_string());
                let _ = commands.send(Command::TimerSet {
                    chat,
                    session,
                    seconds,
                    result,
                });
            }
            waker.wake();
        });
    }

    /// Tells the interface whether a timer change for `chat` is in flight. A
    /// privacy-id mapping learned mid-flight can change the chat's id, so both
    /// the stored id and the canonical one hear about it.
    pub(super) fn timer_saving(&self, chat: &ChatId, saving: bool) {
        self.emit(Event::TimerSaving {
            chat: chat.clone(),
            saving,
        });
        let canonical = self.canonical_str(chat);
        if canonical != *chat {
            self.emit(Event::TimerSaving {
                chat: canonical,
                saving,
            });
        }
    }

    /// Takes a direct timer change off the wire. `Ok` carries the sent
    /// message's id and setting time: the change is written only when the
    /// server acks it, so the control stays disabled until then. A send that
    /// never left the socket reports and re-enables it.
    pub(super) fn timer_set(
        &mut self,
        chat: ChatId,
        session: u64,
        seconds: u32,
        result: Result<(String, i64), String>,
    ) {
        if session != self.account_session {
            return;
        }
        match result {
            Ok((id, sent_at)) => {
                self.awaiting_ack.insert(
                    id.clone(),
                    AwaitingAck::Timer(TimerAck {
                        chat,
                        seconds,
                        sent_at,
                        deadline: Instant::now() + TIMER_ACK_PATIENCE,
                        reported: false,
                    }),
                );
                // An ack that arrived before this completion waited by id.
                if let Some(early) = self.take_early_ack(&id) {
                    self.resolve_timer_ack(&id, early.error.as_deref());
                }
            }
            Err(error) => {
                self.timer_saving(&chat, false);
                log::warn!("could not change disappearing messages ({error})");
                self.emit(Event::Error(
                    "Could not change disappearing messages.".to_owned(),
                ));
            }
        }
    }

    /// Applies the server's ack of a direct timer change, or reports its nack.
    /// Only an entry this map holds is settled, so an ordinary message's ack
    /// can never be taken for a timer change.
    pub(super) fn settle_timer_ack(&mut self, ack: &wa_events::ServerAck) {
        self.resolve_timer_ack(&ack.id, ack.error.as_deref());
    }

    /// Writes the change an ack accepted. A privacy-id mapping learned since
    /// the send may have merged the chat, so the destination is resolved
    /// again here, and the notice stays out of a range the reader cleared.
    fn resolve_timer_ack(&mut self, id: &str, error: Option<&str>) {
        let Some(AwaitingAck::Timer(pending)) = self.awaiting_ack.remove(id) else {
            return;
        };
        // A retry started after this change timed out may still be waiting for
        // its own answer, and its answer or deadline re-enables the control.
        // Clearing it here would let a third change start over the retry.
        let chat = self.canonical_str(&pending.chat);
        let newer_waiting = self.awaiting_ack.values().any(|awaiting| match awaiting {
            AwaitingAck::Timer(other) => !other.reported && self.canonical_str(&other.chat) == chat,
            AwaitingAck::Message(_) => false,
        });
        // A timed-out attempt already cleared saving at its deadline. A retry
        // may now be sending but not yet registered in awaiting_ack.
        if !pending.reported && !newer_waiting {
            self.timer_saving(&pending.chat, false);
        }
        if let Some(error) = error {
            if !pending.reported {
                log::warn!("WhatsApp refused a disappearing-messages change ({error})");
                self.emit(Event::Error(
                    "Could not change disappearing messages.".to_owned(),
                ));
            }
            return;
        }
        if self
            .archive
            .set_ephemeral(&chat, pending.seconds, pending.sent_at)
            .unwrap_or(false)
        {
            self.file_timer_notice(
                &chat,
                &format!("{}{}", crate::model::TIMER_NOTICE_PREFIX, id),
                self.me(),
                true,
                pending.sent_at,
                pending.seconds,
                None,
            );
            self.emit_chat(&chat);
        }
    }

    /// A connection stop drops its pending acks, so no deadline remains to
    /// release their controls. Notify both aliases before discarding them.
    pub(super) fn clear_timer_saving(&self) {
        for awaiting in self.awaiting_ack.values() {
            if let AwaitingAck::Timer(timer) = awaiting {
                self.timer_saving(&timer.chat, false);
            }
        }
    }

    /// Reports timer changes the server never acked, re-enabling the control.
    /// The entry is kept a while longer, so an ack that arrives late still
    /// applies the timer it accepted without a second report; only then is the
    /// entry dropped. Held acks age out here too, so their bound holds without
    /// traffic.
    pub(super) fn expire_timer_acks(&mut self) {
        let now = Instant::now();
        self.early_acks
            .retain(|_, held| now.duration_since(held.at) < EARLY_ACK_TTL);
        let due: Vec<(String, ChatId, bool)> = self
            .awaiting_ack
            .iter()
            .filter_map(|(id, awaiting)| match awaiting {
                AwaitingAck::Timer(pending) if pending.deadline <= now => {
                    Some((id.clone(), pending.chat.clone(), pending.reported))
                }
                _ => None,
            })
            .collect();
        for (id, chat, reported) in due {
            if reported {
                self.awaiting_ack.remove(&id);
                continue;
            }
            if let Some(AwaitingAck::Timer(pending)) = self.awaiting_ack.get_mut(&id) {
                pending.reported = true;
                pending.deadline = now + TIMER_LATE_ACK_PATIENCE;
            }
            self.timer_saving(&chat, false);
            self.emit(Event::Error(
                "Could not change disappearing messages.".to_owned(),
            ));
        }
    }

    /// Holds a message ack that matches nothing yet, bounded in age and count,
    /// so an ack that outran its `TimerSet` still lands.
    pub(super) fn hold_early_ack(&mut self, ack: &wa_events::ServerAck) {
        let now = Instant::now();
        self.early_acks
            .retain(|_, held| now.duration_since(held.at) < EARLY_ACK_TTL);
        if self.early_acks.len() >= EARLY_ACK_LIMIT
            && let Some(oldest) = self
                .early_acks
                .iter()
                .min_by_key(|(_, held)| held.at)
                .map(|(id, _)| id.clone())
        {
            self.early_acks.remove(&oldest);
        }
        self.early_acks.insert(
            ack.id.clone(),
            EarlyAck {
                error: ack.error.clone(),
                at: now,
            },
        );
    }

    /// Takes a held ack for `id`, unless it sat past its age bound.
    fn take_early_ack(&mut self, id: &str) -> Option<EarlyAck> {
        let held = self.early_acks.remove(id)?;
        (Instant::now().duration_since(held.at) < EARLY_ACK_TTL).then_some(held)
    }

    /// Files the system row for a timer change the archive accepted. The row
    /// shares the change's stable id, so a redelivery replaces it rather than
    /// making a second notice.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn file_timer_notice(
        &mut self,
        chat: &str,
        id: &str,
        sender: String,
        from_me: bool,
        timestamp: i64,
        seconds: u32,
        push_name: Option<&str>,
    ) {
        // A notice removed by clearing or deleting the chat must stay gone,
        // even if the same change is delivered again.
        if self.predates_removal(chat, timestamp) {
            return;
        }
        let notice = Message {
            id: id.to_owned(),
            chat: chat.to_owned(),
            sender,
            sender_name: if from_me {
                None
            } else {
                push_name.map(str::to_owned)
            },
            from_me,
            timestamp,
            content: Content::DisappearingTimer { seconds },
            status: Delivery::None,
            delivered_at: None,
            read_at: None,
            quoted: None,
            reactions: Vec::new(),
            edited: false,
            mentions: Vec::new(),
            forwarded: false,
            thumbnail: None,
            history_order: None,
        };
        if let Err(error) = self.archive.insert_message(&notice, None) {
            log::warn!("could not file a disappearing-messages notice: {error}");
            return;
        }
        self.emit(Event::Messages {
            chat: chat.to_owned(),
            messages: vec![notice],
            older: false,
            complete: false,
        });
    }
}
