use crate::fakes::support::Observed;
use std::collections::HashMap;
use std::ops::Deref;

use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::bans::BanStore;
use super::chat::Viewer;
use super::config::FakeTwitchConfig;
use super::frames;
use super::ids;
use super::ledger::{
    Ledger, RecordedRequest, RecordedSession, RecordedSubscription, TappedRequest,
};
use super::rewards::RewardStore;

pub(crate) type Outbox = mpsc::Sender<String>;

pub(crate) struct SubscriptionRequest {
    pub(crate) subscription_type: String,
    pub(crate) version: String,
    pub(crate) condition: Value,
    pub(crate) session_id: String,
}

pub(crate) enum SubscriptionOutcome {
    Created(RecordedSubscription),
    UnknownSession,
    Duplicate,
}

#[derive(Default)]
pub(crate) struct Inner {
    pub(crate) ledger: Ledger,
    outboxes: HashMap<String, Outbox>,
    pub(crate) viewers: HashMap<String, Viewer>,
    pub(crate) rewards: RewardStore,
    pub(crate) followers: Vec<Value>,
    pub(crate) bans: BanStore,
    request_tap: Option<mpsc::UnboundedSender<TappedRequest>>,
}

impl Inner {
    pub(crate) fn open_session(
        &mut self,
        reconnected_from: Option<String>,
        outbox: Outbox,
    ) -> RecordedSession {
        let session = RecordedSession {
            id: ids::session_id(),
            reconnected_from,
            connected_at: ids::timestamp_now(),
            live: true,
        };
        if let Some(previous) = &session.reconnected_from {
            let inherited: Vec<_> = self
                .ledger
                .subscriptions
                .iter()
                .filter(|subscription| &subscription.session_id == previous)
                .map(|subscription| RecordedSubscription {
                    session_id: session.id.clone(),
                    ..subscription.clone()
                })
                .collect();
            self.ledger.subscriptions.extend(inherited);
            self.outboxes.remove(previous);
        }
        self.outboxes.insert(session.id.clone(), outbox);
        self.ledger.sessions.push(session.clone());
        session
    }

    pub(crate) fn close_session(&mut self, session_id: &str) {
        self.outboxes.remove(session_id);
        for session in &mut self.ledger.sessions {
            if session.id == session_id {
                session.live = false;
            }
        }
    }

    pub(crate) fn create_subscription(
        &mut self,
        request: SubscriptionRequest,
    ) -> SubscriptionOutcome {
        if !self.outboxes.contains_key(&request.session_id) {
            return SubscriptionOutcome::UnknownSession;
        }
        let duplicate = self.ledger.subscriptions.iter().any(|existing| {
            existing.session_id == request.session_id
                && existing.subscription_type == request.subscription_type
                && existing.version == request.version
                && existing.condition == request.condition
        });
        if duplicate {
            return SubscriptionOutcome::Duplicate;
        }
        let subscription = RecordedSubscription {
            id: ids::uuid_like(),
            session_id: request.session_id,
            subscription_type: request.subscription_type,
            version: request.version,
            condition: request.condition,
            created_at: ids::timestamp_now(),
        };
        self.ledger.subscriptions.push(subscription.clone());
        SubscriptionOutcome::Created(subscription)
    }

    pub(crate) fn notification_deliveries(
        &self,
        subscription_type: &str,
        event: &Value,
    ) -> Vec<(Outbox, String)> {
        let mut reached: Vec<&str> = Vec::new();
        let mut deliveries = Vec::new();
        for subscription in &self.ledger.subscriptions {
            if subscription.subscription_type != subscription_type
                || reached.contains(&subscription.session_id.as_str())
            {
                continue;
            }
            if let Some(outbox) = self.outboxes.get(&subscription.session_id) {
                reached.push(&subscription.session_id);
                deliveries.push((outbox.clone(), frames::notification(subscription, event)));
            }
        }
        deliveries
    }

    pub(crate) fn reconnect_deliveries(&self, socket_url: &str) -> Vec<(Outbox, String)> {
        self.ledger
            .live_sessions()
            .filter_map(|session| {
                let outbox = self.outboxes.get(&session.id)?;
                let url = format!("{socket_url}?reconnect_from={}", session.id);
                Some((outbox.clone(), frames::reconnect(session, &url)))
            })
            .collect()
    }

    pub(crate) fn remember_viewer(&mut self, viewer: &Viewer) {
        if self.viewers.get(&viewer.user_id) != Some(viewer) {
            self.viewers.insert(viewer.user_id.clone(), viewer.clone());
        }
    }

    pub(crate) fn remember_follow(&mut self, event: &Value) {
        let Some(user_id) = event["user_id"].as_str() else {
            return;
        };
        self.followers.retain(|row| row["user_id"] != user_id);
        self.followers.push(json!({
            "user_id": user_id,
            "user_login": event["user_login"],
            "user_name": event["user_name"],
            "followed_at": event["followed_at"],
        }));
    }

    pub(crate) fn record_request(&mut self, request: RecordedRequest) {
        match &self.request_tap {
            Some(tap) => {
                let tapped = TappedRequest {
                    arrived: tokio::time::Instant::now(),
                    request,
                };
                if let Err(unsent) = tap.send(tapped) {
                    self.request_tap = None;
                    self.ledger.requests.push(unsent.0.request);
                }
            }
            None => self.ledger.requests.push(request),
        }
    }

    pub(crate) fn tap_requests(&mut self) -> mpsc::UnboundedReceiver<TappedRequest> {
        let (tap, requests) = mpsc::unbounded_channel();
        self.request_tap = Some(tap);
        self.ledger.requests.push(RecordedRequest::tap_sentinel());
        requests
    }
}

pub(crate) struct Shared {
    config: FakeTwitchConfig,
    socket_url: String,
    state: Observed<Inner>,
}

impl Shared {
    pub(crate) fn new(config: FakeTwitchConfig, socket_url: String) -> Self {
        Self {
            config,
            socket_url,
            state: Observed::new(Inner::default()),
        }
    }

    pub(crate) fn config(&self) -> &FakeTwitchConfig {
        &self.config
    }

    pub(crate) fn socket_url(&self) -> &str {
        &self.socket_url
    }
}

impl Deref for Shared {
    type Target = Observed<Inner>;

    fn deref(&self) -> &Observed<Inner> {
        &self.state
    }
}
