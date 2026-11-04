use std::sync::Arc;

use async_trait::async_trait;
use forge_events::EventObserver;
use forge_overlay::{RowAvatar, RowRemoval};
use forge_runtime::{OverlayConnectListener, OverlayFrameSink, OverlayPage, OverlayReceivers};
use forge_storage::OverlayId;

use crate::delivery::bus_adapter::BusAdapter;

#[derive(Clone)]
pub struct DeliveryHub {
    adapter: Arc<BusAdapter>,
}

impl DeliveryHub {
    pub fn new(observer: Arc<dyn EventObserver>) -> Self {
        let adapter = BusAdapter::new(observer);
        adapter.spawn();
        Self { adapter }
    }

    pub fn set_overlay_connect_listener(&self, listener: Arc<dyn OverlayConnectListener>) {
        self.adapter.set_overlay_connect_listener(listener);
    }

    pub(crate) fn adapter(&self) -> Arc<BusAdapter> {
        Arc::clone(&self.adapter)
    }
}

#[async_trait]
impl OverlayFrameSink for DeliveryHub {
    async fn deliver_content(
        &self,
        identity: &OverlayId,
        content: serde_json::Value,
        duration_ms: Option<u64>,
    ) -> OverlayReceivers {
        self.adapter
            .deliver_overlay_content(identity, &content, duration_ms)
            .await
    }

    async fn deliver_content_to_page(
        &self,
        identity: &OverlayId,
        page: OverlayPage,
        content: serde_json::Value,
    ) -> OverlayReceivers {
        self.adapter
            .deliver_overlay_content_to_page(identity, page.into(), &content)
            .await
    }

    async fn receivers(&self, identity: &OverlayId) -> OverlayReceivers {
        self.adapter.overlay_receivers(identity).await
    }

    async fn deliver_reload(&self, identity: &OverlayId) {
        self.adapter.deliver_overlay_reload(Some(identity)).await;
    }

    async fn revoke(&self, identity: &OverlayId) {
        self.adapter.revoke_overlay(identity).await;
    }

    async fn deliver_removal(&self, identity: &OverlayId, removal: &RowRemoval) {
        self.adapter
            .deliver_overlay_removal(identity, removal)
            .await;
    }

    async fn deliver_avatar(&self, identity: &OverlayId, avatar: &RowAvatar) {
        self.adapter.deliver_overlay_avatar(identity, avatar).await;
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Mutex;

    use forge_runtime::{EventBus, NullEventLogRepo};

    use tokio::sync::broadcast;

    use super::*;
    use crate::delivery::bus_adapter::{ClientFilterSet, OverlayClientClass, WsFrame};

    struct RecordingListener(Mutex<Vec<OverlayId>>);

    impl RecordingListener {
        fn new() -> Arc<Self> {
            Arc::new(Self(Mutex::new(Vec::new())))
        }

        fn heard(&self) -> Vec<OverlayId> {
            self.0.lock().expect("mutex").clone()
        }
    }

    #[async_trait]
    impl OverlayConnectListener for RecordingListener {
        async fn overlay_connected(&self, identity: &OverlayId, _: OverlayPage) {
            self.0.lock().expect("mutex").push(identity.clone());
        }
    }

    fn hub() -> DeliveryHub {
        DeliveryHub::new(EventBus::new(Arc::new(NullEventLogRepo)))
    }

    async fn overlay_page(
        hub: &DeliveryHub,
        identity: &str,
    ) -> (OverlayPage, broadcast::Receiver<WsFrame>) {
        let (client, _initial) = hub
            .adapter()
            .register_client(ClientFilterSet::new(HashSet::new()))
            .await;
        let frames = hub
            .adapter()
            .promote_to_overlay(
                client.id,
                OverlayId::new(identity),
                OverlayClientClass::BrowserSource,
            )
            .await
            .expect("a just-registered client is still on the registry");
        (client.id.into(), frames)
    }

    fn content_of(frame: Result<WsFrame, broadcast::error::TryRecvError>) -> serde_json::Value {
        match frame {
            Ok(WsFrame::Text(text)) => serde_json::from_str(&text).expect("a json frame"),
            other => panic!("the page was not delivered a text frame: {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_hub_with_no_server_reports_zero_receivers_and_delivers_to_none() {
        let hub = hub();
        let identity = OverlayId::new("goal-box");

        let reached = hub
            .deliver_content(&identity, serde_json::json!({ "value": 1 }), None)
            .await;

        assert_eq!(reached, OverlayReceivers::default());
        assert_eq!(hub.receivers(&identity).await, OverlayReceivers::default());
    }

    #[tokio::test]
    async fn content_delivered_through_the_hub_reaches_the_overlay_page_it_addresses() {
        let hub = hub();
        let identity = OverlayId::new("goal-box");
        let (client, _initial) = hub
            .adapter()
            .register_client(ClientFilterSet::new(HashSet::new()))
            .await;
        let mut page = hub
            .adapter()
            .promote_to_overlay(
                client.id,
                identity.clone(),
                OverlayClientClass::BrowserSource,
            )
            .await
            .expect("a just-registered client is still on the registry");

        let reached = hub
            .deliver_content(&identity, serde_json::json!({ "value": 7 }), None)
            .await;

        assert_eq!(
            reached,
            OverlayReceivers {
                sources: 1,
                preview_tabs: 0
            }
        );
        match page.try_recv() {
            Ok(WsFrame::Text(text)) => assert!(text.contains("\"value\":7")),
            other => panic!("the page was not delivered to: {other:?}"),
        }
    }

    #[tokio::test]
    async fn the_hub_counts_the_pages_of_the_overlay_asked_and_no_other() {
        let hub = hub();
        let target = OverlayId::new("stage-audio");
        let (client, _initial) = hub
            .adapter()
            .register_client(ClientFilterSet::new(HashSet::new()))
            .await;
        let _page = hub
            .adapter()
            .promote_to_overlay(client.id, target.clone(), OverlayClientClass::BrowserSource)
            .await
            .expect("a just-registered client is still on the registry");

        assert_eq!(
            (
                hub.receivers(&target).await,
                hub.receivers(&OverlayId::new("alert-box")).await,
            ),
            (
                OverlayReceivers {
                    sources: 1,
                    preview_tabs: 0
                },
                OverlayReceivers::default(),
            ),
        );
    }

    #[tokio::test]
    async fn the_first_connect_listener_attached_to_a_hub_wins() {
        let hub = hub();
        let first = RecordingListener::new();
        let second = RecordingListener::new();
        hub.set_overlay_connect_listener(first.clone());
        hub.set_overlay_connect_listener(second.clone());

        let attached = hub
            .adapter()
            .overlay_connect_listener()
            .expect("a listener was attached");
        attached
            .overlay_connected(&OverlayId::new("goal-box"), OverlayPage::new(1))
            .await;

        assert_eq!(first.heard(), vec![OverlayId::new("goal-box")]);
        assert!(second.heard().is_empty());
    }

    #[tokio::test]
    async fn content_to_one_page_reaches_that_page_and_not_a_sibling_page_of_the_same_overlay() {
        let hub = hub();
        let (target, mut target_frames) = overlay_page(&hub, "goal-box").await;
        let (_sibling, mut sibling_frames) = overlay_page(&hub, "goal-box").await;

        let reached = hub
            .deliver_content_to_page(
                &OverlayId::new("goal-box"),
                target,
                serde_json::json!({ "value": 7 }),
            )
            .await;

        assert_eq!(
            (
                reached,
                target_frames.try_recv().is_ok(),
                sibling_frames.try_recv().err()
            ),
            (
                OverlayReceivers {
                    sources: 1,
                    preview_tabs: 0
                },
                true,
                Some(broadcast::error::TryRecvError::Empty),
            ),
            "a replay meant for one page re-animated another page of the same overlay"
        );
    }

    #[tokio::test]
    async fn content_to_a_page_is_a_plain_content_frame_without_a_duration() {
        let hub = hub();
        let (page, mut frames) = overlay_page(&hub, "goal-box").await;

        hub.deliver_content_to_page(
            &OverlayId::new("goal-box"),
            page,
            serde_json::json!({ "label": "Sub goal", "value": 42 }),
        )
        .await;

        assert_eq!(
            content_of(frames.try_recv()),
            serde_json::json!({
                "frame": "content",
                "content": { "label": "Sub goal", "value": 42 },
            })
        );
    }

    #[tokio::test]
    async fn content_to_a_page_that_is_not_a_connected_page_of_that_overlay_reaches_nobody() {
        let hub = hub();
        let (foreign, mut foreign_frames) = overlay_page(&hub, "alert-box").await;
        let (plain, mut plain_frames) = hub
            .adapter()
            .register_client(ClientFilterSet::new(HashSet::new()))
            .await;

        for (label, page) in [
            ("a page of another overlay", foreign),
            (
                "a socket that never became an overlay page",
                plain.id.into(),
            ),
            ("a page id no socket holds", OverlayPage::new(u64::MAX)),
        ] {
            let reached = hub
                .deliver_content_to_page(
                    &OverlayId::new("goal-box"),
                    page,
                    serde_json::json!({ "value": 7 }),
                )
                .await;

            assert_eq!(reached, OverlayReceivers::default(), "{label}");
        }
        assert_eq!(
            (
                foreign_frames.try_recv().err(),
                plain_frames.try_recv().err()
            ),
            (
                Some(broadcast::error::TryRecvError::Empty),
                Some(broadcast::error::TryRecvError::Empty),
            )
        );
    }
}
