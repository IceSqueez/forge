mod action_failures;
mod connection_loss;
mod intent;
mod preferences;

use std::str::FromStr;
use std::sync::{Arc, Mutex, PoisonError};

use forge_events::Event;
use forge_platform_core::{CONNECTION_STATE_CHANGED_KIND, ConnectionState};
use forge_types::ActionId;
use serde_json::Value;
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;
use tracing::debug;

use crate::action_engine::ACTION_DONE_KIND;
use crate::bus::{Delivery, EventBus, EventSubscription};
use crate::delivery::NOTIFICATION_POLICY;

pub use action_failures::{
    ACTION_FAILURE_REARM, ACTION_NOTICE_WINDOW, MAX_ACTION_NOTICES_PER_WINDOW,
};
pub use connection_loss::{CONNECTION_LOSS_GRACE, CONNECTION_STABLE_REARM, MIN_CUE_SPACING};
pub use intent::{ActionFailureNotice, NotificationIntent, NotificationIntents};
pub use preferences::NotificationPreferences;

use action_failures::ActionFailureGate;
use connection_loss::ConnectionLossGate;

const NOTIFICATION_INTENT_CAPACITY: usize = 32;
const FAILED_OUTCOME: &str = "failed";
const SUCCESS_OUTCOME: &str = "success";

#[derive(Clone)]
pub struct NotificationPolicyHandle {
    preferences: Arc<watch::Sender<NotificationPreferences>>,
    intents: Arc<Mutex<Option<NotificationIntents>>>,
}

impl NotificationPolicyHandle {
    pub fn set_preferences(&self, preferences: NotificationPreferences) {
        self.preferences.send_replace(preferences);
    }

    pub fn preferences(&self) -> NotificationPreferences {
        *self.preferences.borrow()
    }

    pub fn take_intents(&self) -> Option<NotificationIntents> {
        self.intents
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }
}

pub fn spawn_notification_policy(
    bus: &EventBus,
    initial: NotificationPreferences,
) -> NotificationPolicyHandle {
    let (preferences_tx, preferences) = watch::channel(initial);
    let (intents_tx, intents_rx) = mpsc::channel(NOTIFICATION_INTENT_CAPACITY);
    let policy = NotificationPolicy {
        failures: ActionFailureGate::default(),
        links: ConnectionLossGate::default(),
        preferences,
        intents: intents_tx,
    };
    tokio::spawn(policy.run(bus.subscribe_observer(NOTIFICATION_POLICY)));
    NotificationPolicyHandle {
        preferences: Arc::new(preferences_tx),
        intents: Arc::new(Mutex::new(Some(NotificationIntents::new(intents_rx)))),
    }
}

enum PolicySignal {
    ActionSucceeded(ActionId),
    ActionFailed(ActionFailureNotice),
    LinkUp(String),
    LinkDown { platform_id: String, expected: bool },
}

struct NotificationPolicy {
    failures: ActionFailureGate,
    links: ConnectionLossGate,
    preferences: watch::Receiver<NotificationPreferences>,
    intents: mpsc::Sender<NotificationIntent>,
}

impl NotificationPolicy {
    async fn run(mut self, mut events: EventSubscription) {
        let mut due = Vec::new();
        loop {
            let deadline = [self.failures.deadline(), self.links.deadline()]
                .into_iter()
                .flatten()
                .min();
            tokio::select! {
                delivery = events.next() => match delivery {
                    Delivery::Event(event) => self.observe(&event, Instant::now(), &mut due),
                    Delivery::Skipped(_) => {}
                    Delivery::Closed => return,
                },
                () = sleep_until_due(deadline) => {
                    let now = Instant::now();
                    self.failures.flush_due(now, &mut due);
                    self.links.flush_due(now, &mut due);
                }
            }
            self.deliver(&mut due);
        }
    }

    fn observe(&mut self, event: &Event, now: Instant, due: &mut Vec<NotificationIntent>) {
        let Some(signal) = classify(event) else {
            return;
        };
        match signal {
            PolicySignal::ActionSucceeded(action_id) => self.failures.succeeded(&action_id),
            PolicySignal::ActionFailed(notice) => {
                if self.preferences.borrow().action_failure_notice {
                    self.failures.failed(notice, now, due);
                }
            }
            PolicySignal::LinkUp(platform_id) => self.links.connected(platform_id, now),
            PolicySignal::LinkDown {
                platform_id,
                expected,
            } => self.links.disconnected(&platform_id, expected, now),
        }
    }

    fn deliver(&self, due: &mut Vec<NotificationIntent>) {
        let preferences = *self.preferences.borrow();
        for intent in due.drain(..) {
            if !preferences.allows(&intent) {
                continue;
            }
            if let Err(e) = self.intents.try_send(intent) {
                debug!("notification policy: intent not delivered: {e}");
            }
        }
    }
}

fn classify(event: &Event) -> Option<PolicySignal> {
    if event.replay {
        return None;
    }
    let payload = &event.payload;
    match event.kind.as_str() {
        ACTION_DONE_KIND => classify_action_done(payload),
        CONNECTION_STATE_CHANGED_KIND => classify_connection_change(payload),
        _ => None,
    }
}

fn classify_action_done(payload: &Value) -> Option<PolicySignal> {
    let action_id = ActionId::from_str(payload.get("action_id")?.as_str()?).ok()?;
    match payload.get("outcome")?.as_str()? {
        SUCCESS_OUTCOME => Some(PolicySignal::ActionSucceeded(action_id)),
        FAILED_OUTCOME if !flag(payload, "manual_test") => {
            Some(PolicySignal::ActionFailed(ActionFailureNotice {
                action_id,
                action_name: payload
                    .get("action_name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                reason: payload
                    .get("reason")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            }))
        }
        _ => None,
    }
}

fn classify_connection_change(payload: &Value) -> Option<PolicySignal> {
    let platform_id = payload.get("platform_id")?.as_str()?.to_owned();
    let state: ConnectionState = serde_json::from_value(payload.get("state")?.clone()).ok()?;
    Some(match state {
        ConnectionState::Connected => PolicySignal::LinkUp(platform_id),
        ConnectionState::Disconnected
        | ConnectionState::Connecting
        | ConnectionState::Reconnecting => PolicySignal::LinkDown {
            platform_id,
            expected: flag(payload, "expected"),
        },
    })
}

fn flag(payload: &Value, key: &str) -> bool {
    payload.get(key).and_then(Value::as_bool).unwrap_or(false)
}

async fn sleep_until_due(deadline: Option<Instant>) {
    match deadline {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}
