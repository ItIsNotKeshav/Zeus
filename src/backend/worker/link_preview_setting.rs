//! The phone's "Disable link previews", read before anything is fetched.
//!
//! The setting lives in the phone's `regular` app-state collection, and an
//! incremental sync brings only what changed since the last one. A choice made
//! before ZapFast followed it was consumed by earlier syncs and never comes
//! again, and a phone that never touched the setting says nothing at all, so
//! silence alone cannot mean previews are allowed. Until the phone has said,
//! they stay off, and `regular` is rebuilt once from a snapshot, which replays
//! the setting as a `DisableLinkPreviewsUpdate`. A complete snapshot that
//! replayed none means the phone never set it, the one case that turns them on.

use super::*;

/// What a stored setting means for this session: whether previews are off, and
/// whether the phone has yet to be asked. An unreadable archive keeps them off
/// without asking, since no answer could be kept.
pub(super) fn link_previews_at_start(stored: crate::archive::Result<Option<bool>>) -> (bool, bool) {
    match stored {
        Ok(Some(disabled)) => (disabled, false),
        Ok(None) => (true, true),
        Err(error) => {
            log::warn!("could not read the phone's link preview setting: {error}");
            // A lost preview is recoverable; contacting a site is not.
            (true, false)
        }
    }
}

impl Worker {
    /// Reads the phone's setting once, for an account that has not stored it.
    pub(super) fn recover_link_previews(&mut self) {
        if !self.link_previews_unknown
            || self.link_previews_reading
            || !matches!(self.status, LinkStatus::Connected)
        {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        self.link_previews_reading = true;
        log::info!("reading the link preview setting from the phone");
        let sender = self.wa_sender.clone();
        let generation = self.privacy_generation;
        tokio::spawn(async move {
            use whatsapp_rust::WAPatchName;
            let complete = match client
                .resync_app_state(
                    [WAPatchName::Regular],
                    whatsapp_rust::AppStateResyncMode::Snapshot,
                )
                .await
            {
                Ok(report) => report.synced.contains(&WAPatchName::Regular),
                Err(error) => {
                    log::warn!("could not read the link preview setting: {error}");
                    false
                }
            };
            // The same queue as the replayed mutations, so a setting the
            // snapshot carried is applied before this says there was none.
            let _ = sender.send(RuntimeEvent::LinkPreviewsRead {
                generation,
                complete,
            });
        });
    }

    /// The snapshot finished. A setting it replayed is stored by now; if there
    /// was none the phone never set it, and the default holds. An unfinished
    /// one leaves previews off until the next connection asks again.
    pub(super) fn link_previews_read(&mut self, generation: u64, complete: bool) {
        // A task from an unlinked device says nothing about the next account.
        if generation != self.privacy_generation {
            return;
        }
        self.link_previews_reading = false;
        if !complete {
            log::warn!(
                "link preview setting not read yet; previews stay off until the next connection"
            );
            return;
        }
        if !self.link_previews_unknown {
            return;
        }
        // Dated at the epoch, so any change the phone really made wins.
        match self.archive.set_link_previews_disabled_at(false, 0) {
            Ok(disabled) => {
                self.link_previews_unknown = false;
                self.link_previews_disabled = disabled;
                self.emit(Event::LinkPreviewsDisabled(disabled));
            }
            Err(error) => log::warn!("could not store the link preview setting: {error}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::receipt_tests::worker;
    use super::*;

    fn unknown(worker: &mut Worker) {
        worker.link_previews_disabled = true;
        worker.link_previews_unknown = true;
        worker.link_previews_reading = true;
    }

    async fn phone_says(worker: &mut Worker, disabled: bool, at: i64) {
        let mut action = wa::sync_action_value::PrivacySettingDisableLinkPreviewsAction::default();
        action.is_previews_disabled = Some(disabled);
        worker
            .handle_wa_event(Arc::new(wa_events::Event::DisableLinkPreviewsUpdate(
                wa_events::DisableLinkPreviewsUpdate::builder()
                    .previews_disabled(disabled)
                    .timestamp(whatsapp_rust::wacore::time::from_secs(at).unwrap())
                    .action(Box::new(action))
                    .from_full_sync(true)
                    .build(),
            )))
            .await;
    }

    #[test]
    fn only_a_stored_answer_lets_previews_run_unasked() {
        assert_eq!(link_previews_at_start(Ok(Some(false))), (false, false));
        assert_eq!(link_previews_at_start(Ok(Some(true))), (true, false));
        // Never told: off, and the phone is asked.
        assert_eq!(link_previews_at_start(Ok(None)), (true, true));
        let unreadable = rusqlite::Error::InvalidQuery;
        assert_eq!(link_previews_at_start(Err(unreadable)), (true, false));
    }

    #[tokio::test]
    async fn nothing_is_fetched_until_the_snapshot_finishes() {
        let (mut worker, events, _inbox, _wa) = worker();
        unknown(&mut worker);
        worker
            .handle_command(Command::FetchLinkPreview {
                url: "https://example.com/a".into(),
            })
            .await;
        assert!(worker.preview_fetch.is_none());
        assert!(matches!(
            events.try_recv().unwrap(),
            Event::LinkPreview { preview: None, .. }
        ));

        // An unfinished snapshot proves nothing and is asked again.
        worker.link_previews_read(0, false);
        assert!(worker.link_previews_disabled && worker.link_previews_unknown);
        assert!(!worker.link_previews_reading);
        assert_eq!(worker.archive.link_previews_disabled().unwrap(), None);
    }

    #[test]
    fn a_complete_snapshot_without_the_setting_stores_the_default() {
        let (mut worker, events, _inbox, _wa) = worker();
        unknown(&mut worker);
        worker.link_previews_read(0, true);
        assert!(!worker.link_previews_disabled && !worker.link_previews_unknown);
        assert_eq!(
            worker.archive.link_previews_disabled().unwrap(),
            Some(false)
        );
        assert!(
            events
                .try_iter()
                .any(|event| matches!(event, Event::LinkPreviewsDisabled(false)))
        );
    }

    #[tokio::test]
    async fn the_setting_a_snapshot_replays_is_not_overwritten_when_it_ends() {
        let (mut worker, _events, _inbox, _wa) = worker();
        unknown(&mut worker);
        phone_says(&mut worker, true, 200).await;
        assert!(!worker.link_previews_unknown);
        worker.link_previews_read(0, true);
        assert!(worker.link_previews_disabled);
        assert_eq!(worker.archive.link_previews_disabled().unwrap(), Some(true));
    }

    #[tokio::test]
    async fn a_phone_change_after_the_default_still_wins() {
        let (mut worker, _events, _inbox, _wa) = worker();
        unknown(&mut worker);
        worker.link_previews_read(0, true);
        phone_says(&mut worker, true, 200).await;
        assert!(worker.link_previews_disabled);
        assert_eq!(worker.archive.link_previews_disabled().unwrap(), Some(true));
    }

    #[tokio::test]
    async fn unlinking_forgets_the_answer_for_the_next_account() {
        let (mut worker, events, _inbox, _wa) = worker();
        worker
            .archive
            .set_link_previews_disabled_at(false, 100)
            .unwrap();
        worker.on_logged_out().await;
        assert!(worker.link_previews_disabled && worker.link_previews_unknown);
        assert_eq!(worker.archive.link_previews_disabled().unwrap(), None);
        assert!(
            events
                .try_iter()
                .any(|event| matches!(event, Event::LinkPreviewsDisabled(true)))
        );
    }

    #[test]
    fn a_snapshot_from_an_unlinked_device_decides_nothing() {
        let (mut worker, _events, _inbox, _wa) = worker();
        unknown(&mut worker);
        worker.privacy_generation = 1;
        worker.link_previews_read(0, true);
        assert!(worker.link_previews_disabled && worker.link_previews_unknown);
        // The next account's own read is still running.
        assert!(worker.link_previews_reading);
        assert_eq!(worker.archive.link_previews_disabled().unwrap(), None);
    }
}
