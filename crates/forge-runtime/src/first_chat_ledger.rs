use std::collections::HashSet;
use std::sync::Mutex;

use forge_events::Event;
use forge_storage::{StorageError, ViewerPlatform, ViewerRepo};
use forge_types::ChatViewer;

use crate::viewer_tracker::viewer_platform;

pub struct FirstChatLedger {
    seen: Mutex<HashSet<(ViewerPlatform, String)>>,
}

impl FirstChatLedger {
    pub async fn load(repo: &dyn ViewerRepo) -> Result<Self, StorageError> {
        let seen = repo
            .list()
            .await?
            .into_iter()
            .map(|viewer| (viewer.platform, viewer.viewer_id))
            .collect();
        Ok(Self {
            seen: Mutex::new(seen),
        })
    }

    pub(crate) fn stamp(&self, event: &mut Event) {
        if event.replay {
            return;
        }
        let Some(platform) = viewer_platform(event.source) else {
            return;
        };
        let Some(mut viewer) = ChatViewer::read(&event.payload) else {
            return;
        };
        let first = self
            .seen
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert((platform, viewer.id.clone()));
        if viewer.first_message != first {
            viewer.first_message = first;
            viewer.attach(&mut event.payload);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_events::EventSource;
    use forge_storage::Viewer;
    use forge_storage::viewer::MockViewerRepo;
    use time::OffsetDateTime;

    use super::*;

    fn known(platform: ViewerPlatform, viewer_id: &str) -> Viewer {
        Viewer {
            viewer_id: viewer_id.to_owned(),
            platform,
            username: viewer_id.to_owned(),
            first_seen_at: OffsetDateTime::UNIX_EPOCH,
            last_seen_at: OffsetDateTime::UNIX_EPOCH,
            custom_greeting: false,
        }
    }

    async fn ledger_knowing(viewers: Vec<Viewer>) -> FirstChatLedger {
        let mut repo = MockViewerRepo::new();
        repo.expect_list().return_once(move || Ok(viewers));
        FirstChatLedger::load(&repo).await.unwrap()
    }

    fn line(source: EventSource, viewer_id: &str, claimed_first: bool) -> Event {
        let mut payload = serde_json::json!({ "message": "hi" });
        ChatViewer {
            first_message: claimed_first,
            ..ChatViewer::new(viewer_id, "viewer")
        }
        .attach(&mut payload);
        Event::new(source, "chat.line", payload)
    }

    fn stamped(ledger: &FirstChatLedger, mut event: Event) -> Option<bool> {
        ledger.stamp(&mut event);
        ChatViewer::read(&event.payload).map(|viewer| viewer.first_message)
    }

    #[tokio::test]
    async fn only_the_first_message_of_an_unseen_viewer_is_stamped_first() {
        let ledger = ledger_knowing(vec![]).await;
        let flags: Vec<Option<bool>> = (0..3)
            .map(|_| stamped(&ledger, line(EventSource::Twitch, "42", false)))
            .collect();
        assert_eq!(flags, [Some(true), Some(false), Some(false)]);
    }

    #[tokio::test]
    async fn a_viewer_from_the_persisted_history_is_never_first_even_when_the_line_claims_it() {
        let ledger = ledger_knowing(vec![known(ViewerPlatform::Kick, "42")]).await;
        assert_eq!(
            stamped(&ledger, line(EventSource::Kick, "42", true)),
            Some(false)
        );
    }

    #[tokio::test]
    async fn the_same_viewer_id_on_another_platform_is_a_different_viewer() {
        let ledger = ledger_knowing(vec![known(ViewerPlatform::Twitch, "42")]).await;
        let flags: Vec<Option<bool>> = [EventSource::YouTube, EventSource::Kick]
            .into_iter()
            .map(|source| stamped(&ledger, line(source, "42", false)))
            .collect();
        assert_eq!(flags, [Some(true), Some(true)]);
    }

    #[tokio::test]
    async fn a_replayed_line_keeps_its_recorded_flag_and_does_not_use_up_the_first_message() {
        let ledger = ledger_knowing(vec![]).await;
        let mut replay = line(EventSource::Twitch, "42", false);
        replay.replay = true;
        assert_eq!(stamped(&ledger, replay), Some(false));
        assert_eq!(
            stamped(&ledger, line(EventSource::Twitch, "42", false)),
            Some(true)
        );
    }

    #[tokio::test]
    async fn a_line_from_a_source_without_viewers_is_left_alone_and_does_not_use_up_the_first_message()
     {
        let ledger = ledger_knowing(vec![]).await;
        assert_eq!(
            stamped(&ledger, line(EventSource::Core, "42", false)),
            Some(false)
        );
        assert_eq!(
            stamped(&ledger, line(EventSource::Twitch, "42", false)),
            Some(true)
        );
    }

    #[tokio::test]
    async fn an_event_without_a_viewer_identity_is_left_byte_for_byte_unchanged() {
        let ledger = ledger_knowing(vec![]).await;
        for payload in [
            serde_json::json!({ "user": { "id": "42", "login": "viewer" } }),
            serde_json::json!({ ChatViewer::KEY: { "id": "", "name": "viewer" } }),
        ] {
            let mut event = Event::new(EventSource::Twitch, "chat.line", payload.clone());
            ledger.stamp(&mut event);
            assert_eq!(event.payload, payload);
        }
    }

    #[tokio::test]
    async fn an_unreadable_viewer_history_fails_the_load() {
        let mut repo = MockViewerRepo::new();
        repo.expect_list().return_once(|| {
            Err(StorageError::Connection {
                reason: "closed".to_owned(),
            })
        });
        assert!(matches!(
            FirstChatLedger::load(&repo).await,
            Err(StorageError::Connection { .. })
        ));
    }
}
