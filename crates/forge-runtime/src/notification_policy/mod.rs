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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::time::Duration;

    use forge_events::EventSource;
    use forge_platform_core::{ConnectionTransition, connection_state_changed_event};
    use serde_json::json;

    use super::*;
    use crate::NullEventLogRepo;

    const TWITCH: &str = "twitch";
    const SECOND: Duration = Duration::from_secs(1);
    const ALL_ON: NotificationPreferences = NotificationPreferences {
        action_failure_notice: true,
        connection_loss_cue: true,
    };
    const ALL_OFF: NotificationPreferences = NotificationPreferences {
        action_failure_notice: false,
        connection_loss_cue: false,
    };

    struct Rig {
        bus: Arc<EventBus>,
        policy: NotificationPolicyHandle,
        intents: NotificationIntents,
    }

    async fn settle() {
        for _ in 0..64 {
            tokio::task::yield_now().await;
        }
    }

    async fn rig(preferences: NotificationPreferences) -> Rig {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let policy = spawn_notification_policy(&bus, preferences);
        let intents = policy.take_intents().unwrap();
        settle().await;
        Rig {
            bus,
            policy,
            intents,
        }
    }

    fn action_done(action_id: ActionId, outcome: &str, manual_test: bool) -> Event {
        Event::new(
            EventSource::Core,
            ACTION_DONE_KIND,
            json!({
                "action_id": action_id.to_string(),
                "action_name": "Shoutout",
                "outcome": outcome,
                "manual_test": manual_test,
                "reason": "step 2 failed",
            }),
        )
    }

    fn failed(action_id: ActionId) -> Event {
        action_done(action_id, FAILED_OUTCOME, false)
    }

    fn link(state: ConnectionState, transition: ConnectionTransition) -> Event {
        connection_state_changed_event(TWITCH, state, transition)
    }

    impl Rig {
        async fn publish(&mut self, event: Event) -> Vec<NotificationIntent> {
            self.bus.publish(event);
            self.drain().await
        }

        async fn elapse(&mut self, by: Duration) -> Vec<NotificationIntent> {
            tokio::time::advance(by).await;
            self.drain().await
        }

        async fn drain(&mut self) -> Vec<NotificationIntent> {
            settle().await;
            std::iter::from_fn(|| self.intents.try_next()).collect()
        }

        async fn lose_twitch(&mut self) {
            self.publish(link(
                ConnectionState::Connected,
                ConnectionTransition::Unexpected,
            ))
            .await;
            self.publish(link(
                ConnectionState::Disconnected,
                ConnectionTransition::Unexpected,
            ))
            .await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn failed_run_delivers_a_notice_with_name_and_reason() {
        let mut rig = rig(ALL_ON).await;
        let action_id = ActionId::new();
        let intents = rig.publish(failed(action_id)).await;
        assert_eq!(
            intents,
            vec![NotificationIntent::ActionFailed(ActionFailureNotice {
                action_id,
                action_name: "Shoutout".to_owned(),
                reason: Some("step 2 failed".to_owned()),
            })]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn non_notifying_action_done_events_deliver_nothing() {
        let action_id = ActionId::new();
        let mut replayed = failed(action_id);
        replayed.replay = true;
        let mut bad_id = failed(action_id);
        bad_id.payload["action_id"] = json!("not-an-id");
        for (label, event) in [
            ("manual test", action_done(action_id, FAILED_OUTCOME, true)),
            ("replayed", replayed),
            ("cancelled", action_done(action_id, "cancelled", false)),
            ("malformed action id", bad_id),
        ] {
            let mut rig = rig(ALL_ON).await;
            assert!(rig.publish(event).await.is_empty(), "{label}");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn successful_run_on_the_bus_rearms_the_failure_notice() {
        let mut rig = rig(ALL_ON).await;
        let action_id = ActionId::new();
        rig.publish(failed(action_id)).await;
        rig.publish(action_done(action_id, SUCCESS_OUTCOME, false))
            .await;
        assert_eq!(rig.publish(failed(action_id)).await.len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn folded_summary_arrives_when_the_window_closes_without_further_events() {
        let mut rig = rig(ALL_ON).await;
        for _ in 0..4 {
            rig.publish(failed(ActionId::new())).await;
        }
        assert!(rig.elapse(ACTION_NOTICE_WINDOW - SECOND).await.is_empty());
        assert_eq!(
            rig.elapse(SECOND).await,
            vec![NotificationIntent::ActionFailuresFolded { count: 1 }]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn connection_loss_cue_fires_once_the_grace_period_elapses() {
        let mut rig = rig(ALL_ON).await;
        rig.lose_twitch().await;
        assert!(rig.elapse(CONNECTION_LOSS_GRACE - SECOND).await.is_empty());
        assert_eq!(
            rig.elapse(SECOND).await,
            vec![NotificationIntent::ConnectionLossCue {
                platform_ids: vec![TWITCH.to_owned()],
            }]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn expected_reconnect_on_the_bus_delivers_no_cue() {
        let mut rig = rig(ALL_ON).await;
        rig.publish(link(
            ConnectionState::Connected,
            ConnectionTransition::Unexpected,
        ))
        .await;
        rig.publish(link(
            ConnectionState::Reconnecting,
            ConnectionTransition::Expected,
        ))
        .await;
        assert!(rig.elapse(2 * CONNECTION_LOSS_GRACE).await.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn replayed_connection_loss_delivers_no_cue() {
        let mut rig = rig(ALL_ON).await;
        rig.publish(link(
            ConnectionState::Connected,
            ConnectionTransition::Unexpected,
        ))
        .await;
        let mut replayed = link(
            ConnectionState::Disconnected,
            ConnectionTransition::Unexpected,
        );
        replayed.replay = true;
        rig.publish(replayed).await;
        assert!(rig.elapse(2 * CONNECTION_LOSS_GRACE).await.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn disabled_preferences_deliver_no_intents() {
        let mut rig = rig(ALL_OFF).await;
        rig.publish(failed(ActionId::new())).await;
        rig.lose_twitch().await;
        assert!(rig.elapse(2 * CONNECTION_LOSS_GRACE).await.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn enabling_failure_notices_applies_to_the_next_failure() {
        let mut rig = rig(ALL_OFF).await;
        let action_id = ActionId::new();
        assert!(rig.publish(failed(action_id)).await.is_empty());
        rig.policy.set_preferences(ALL_ON);
        assert_eq!(rig.publish(failed(action_id)).await.len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn disabling_the_cue_during_grace_suppresses_the_pending_cue() {
        let mut rig = rig(ALL_ON).await;
        rig.lose_twitch().await;
        rig.policy.set_preferences(NotificationPreferences {
            connection_loss_cue: false,
            ..ALL_ON
        });
        assert!(rig.elapse(CONNECTION_LOSS_GRACE).await.is_empty());
    }

    const KICK: &str = "kick";
    const YOUTUBE: &str = "youtube";
    const MILLI: Duration = Duration::from_millis(1);
    const HELD_DROP_AFTER_CUE: Duration = Duration::from_secs(15);

    fn link_of(
        platform_id: &str,
        state: ConnectionState,
        transition: ConnectionTransition,
    ) -> Event {
        connection_state_changed_event(platform_id, state, transition)
    }

    fn loss_cue(platform_ids: &[&str]) -> Vec<NotificationIntent> {
        vec![NotificationIntent::ConnectionLossCue {
            platform_ids: platform_ids.iter().map(|id| (*id).to_owned()).collect(),
        }]
    }

    async fn rig_after_twitch_cue(others: &[&str]) -> Rig {
        let mut rig = rig(ALL_ON).await;
        for platform in others {
            rig.publish(link_of(
                platform,
                ConnectionState::Connected,
                ConnectionTransition::Unexpected,
            ))
            .await;
        }
        rig.lose_twitch().await;
        assert_eq!(rig.elapse(CONNECTION_LOSS_GRACE).await, loss_cue(&[TWITCH]));
        rig
    }

    async fn drop_link(rig: &mut Rig, platform_id: &str, transition: ConnectionTransition) {
        rig.publish(link_of(
            platform_id,
            ConnectionState::Disconnected,
            transition,
        ))
        .await;
    }

    const UNTIL_SPACING_ENDS: Duration = MIN_CUE_SPACING.saturating_sub(HELD_DROP_AFTER_CUE);

    #[tokio::test(start_paused = true)]
    async fn loss_held_by_cue_spacing_cues_exactly_when_spacing_ends() {
        let mut rig = rig_after_twitch_cue(&[KICK]).await;
        rig.elapse(HELD_DROP_AFTER_CUE).await;
        drop_link(&mut rig, KICK, ConnectionTransition::Unexpected).await;
        assert!(rig.elapse(UNTIL_SPACING_ENDS - MILLI).await.is_empty());
        assert_eq!(rig.elapse(MILLI).await, loss_cue(&[KICK]));
    }

    #[tokio::test(start_paused = true)]
    async fn held_loss_that_recovers_before_spacing_ends_never_cues() {
        let mut rig = rig_after_twitch_cue(&[KICK]).await;
        rig.elapse(HELD_DROP_AFTER_CUE).await;
        drop_link(&mut rig, KICK, ConnectionTransition::Unexpected).await;
        rig.elapse(UNTIL_SPACING_ENDS - MILLI).await;
        rig.publish(link_of(
            KICK,
            ConnectionState::Connected,
            ConnectionTransition::Unexpected,
        ))
        .await;
        assert!(rig.elapse(MIN_CUE_SPACING).await.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn held_losses_fold_into_one_sorted_cue_when_spacing_ends() {
        let mut rig = rig_after_twitch_cue(&[YOUTUBE, KICK]).await;
        rig.elapse(HELD_DROP_AFTER_CUE).await;
        drop_link(&mut rig, YOUTUBE, ConnectionTransition::Unexpected).await;
        rig.elapse(SECOND).await;
        drop_link(&mut rig, KICK, ConnectionTransition::Unexpected).await;
        assert_eq!(
            rig.elapse(UNTIL_SPACING_ENDS - SECOND).await,
            loss_cue(&[KICK, YOUTUBE])
        );
    }

    #[tokio::test(start_paused = true)]
    async fn expected_drop_while_held_is_excluded_from_the_cue() {
        let mut rig = rig_after_twitch_cue(&[KICK]).await;
        rig.elapse(HELD_DROP_AFTER_CUE).await;
        drop_link(&mut rig, KICK, ConnectionTransition::Unexpected).await;
        rig.elapse(UNTIL_SPACING_ENDS - SECOND).await;
        drop_link(&mut rig, KICK, ConnectionTransition::Expected).await;
        assert!(rig.elapse(MIN_CUE_SPACING).await.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn intents_have_a_single_consumer() {
        let rig = rig(ALL_ON).await;
        assert!(rig.policy.take_intents().is_none());
    }
}
