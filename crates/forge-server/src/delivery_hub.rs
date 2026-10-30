use std::sync::Arc;

use async_trait::async_trait;
use forge_events::EventObserver;
use forge_runtime::{OverlayConnectListener, OverlayFrameSink, OverlayReceivers};
use forge_storage::OverlayId;

use crate::bus_adapter::BusAdapter;

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

    async fn receivers(&self, identity: &OverlayId) -> OverlayReceivers {
        self.adapter.overlay_receivers(identity).await
    }

    async fn deliver_reload(&self, identity: &OverlayId) {
        self.adapter.deliver_overlay_reload(Some(identity)).await;
    }

    async fn revoke(&self, identity: &OverlayId) {
        self.adapter.revoke_overlay(identity).await;
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Mutex;

    use forge_runtime::{EventBus, NullEventLogRepo};

    use super::*;
    use crate::bus_adapter::{ClientFilterSet, OverlayClientClass, WsFrame};

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
        async fn overlay_connected(&self, identity: &OverlayId) {
            self.0.lock().expect("mutex").push(identity.clone());
        }
    }

    fn hub() -> DeliveryHub {
        DeliveryHub::new(EventBus::new(Arc::new(NullEventLogRepo)))
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
            .overlay_connected(&OverlayId::new("goal-box"))
            .await;

        assert_eq!(first.heard(), vec![OverlayId::new("goal-box")]);
        assert!(second.heard().is_empty());
    }
}
