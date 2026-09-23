use std::collections::HashMap;
use std::time::Duration;

use forge_types::ActionId;

use super::ledger_checks::{
    holds_live_subscription, live_subscription_types, subscription_excerpt,
};
use super::outcome::{ActionDetail, LedgerExcerpt};
use crate::EmulatorError;
use crate::control::ControlClient;
use crate::fixture::SeedReport;
use crate::overlay::OverlayPages;
use crate::scenario::{Crowd, StepAction};
use crate::twitch::FakeTwitch;

/// Resolves a scenario's action names to the ids the seeder stored.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActionIndex {
    by_name: HashMap<String, ActionId>,
}

impl ActionIndex {
    pub fn from_seed(seed: &SeedReport) -> Self {
        let commands = seed
            .chat_commands
            .iter()
            .map(|command| (command.action_name.clone(), command.action_id));
        let triggers = seed
            .event_triggers
            .iter()
            .map(|trigger| (trigger.action_name.clone(), trigger.action_id));
        Self {
            by_name: commands.chain(triggers).collect(),
        }
    }

    pub fn resolve(&self, name: &str) -> Option<ActionId> {
        self.by_name.get(name).copied()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ActionFailure {
    pub(crate) reason: String,
    pub(crate) ledger: Option<LedgerExcerpt>,
}

pub(crate) struct Stimuli<'a> {
    pub(crate) client: &'a ControlClient,
    pub(crate) twitch: Option<&'a FakeTwitch>,
    pub(crate) actions: &'a ActionIndex,
    pub(crate) pages: &'a OverlayPages,
}

impl Stimuli<'_> {
    /// `ForgeReady` is satisfied before any step runs, so it is never performed here.
    pub(crate) async fn perform(&self, action: &StepAction) -> Result<ActionDetail, ActionFailure> {
        match action {
            StepAction::ForgeReady { .. } => Err(plain(
                "forge_ready is only valid as the first step".to_owned(),
            )),
            StepAction::TwitchSubscribed { types, within_ms } => {
                self.await_subscriptions(types, *within_ms).await
            }
            StepAction::Chat(message) => {
                let twitch = self.twitch()?;
                twitch
                    .inject_chat_message(&message.viewer.to_viewer(), &message.text)
                    .await
                    .map(|message_id| ActionDetail::ChatSent { message_id })
                    .map_err(|e| with_ledger(e.to_string(), twitch))
            }
            StepAction::Crowd(crowd) => self.send_crowd(crowd).await,
            StepAction::TwitchEvent {
                subscription_type,
                event,
            } => self.inject_event(subscription_type, event).await,
            StepAction::SessionReconnect { within_ms } => self.reconnect(*within_ms).await,
            StepAction::OverlayPage { overlay, within_ms } => self
                .pages
                .open_page(overlay, Duration::from_millis(*within_ms))
                .await
                .map(|identity| ActionDetail::OverlayPageOpened {
                    overlay: overlay.clone(),
                    identity,
                })
                .map_err(refused),
            StepAction::Pause { ms, .. } => {
                tokio::time::sleep(Duration::from_millis(*ms)).await;
                Ok(ActionDetail::Paused)
            }
            StepAction::RunAction { action, args } => {
                let action_id = self.actions.resolve(action).ok_or_else(|| {
                    plain(format!("the fixture seeded no action named `{action}`"))
                })?;
                self.client
                    .do_action(action_id, args)
                    .await
                    .map(|execution_id| ActionDetail::ActionRun {
                        action_id,
                        execution_id,
                    })
                    .map_err(refused)
            }
            StepAction::SetGlobal {
                name,
                value,
                persisted,
            } => self
                .client
                .set_global(name, value, *persisted)
                .await
                .map(|()| ActionDetail::GlobalSet)
                .map_err(refused),
        }
    }

    async fn inject_event(
        &self,
        subscription_type: &str,
        event: &serde_json::Value,
    ) -> Result<ActionDetail, ActionFailure> {
        let twitch = self.twitch()?;
        twitch
            .inject_notification(subscription_type, event.clone())
            .await
            .map(|sessions| ActionDetail::TwitchEventDelivered {
                subscription_type: subscription_type.to_owned(),
                sessions,
            })
            .map_err(|e| {
                let reason = match e {
                    EmulatorError::NotSubscribed { .. } => {
                        format!("{e}; live subscriptions: {}", live_types(&twitch.ledger()))
                    }
                    other => other.to_string(),
                };
                with_ledger(reason, twitch)
            })
    }

    fn twitch(&self) -> Result<&FakeTwitch, ActionFailure> {
        self.twitch
            .ok_or_else(|| plain("the run has no fake Twitch".to_owned()))
    }

    async fn await_subscriptions(
        &self,
        types: &[String],
        within_ms: u64,
    ) -> Result<ActionDetail, ActionFailure> {
        let twitch = self.twitch()?;
        let what = format!("live subscriptions of type {}", types.join(", "));
        twitch
            .wait_for(&what, Duration::from_millis(within_ms), |ledger| {
                let all_held = types
                    .iter()
                    .all(|kind| holds_live_subscription(ledger, kind, None));
                all_held.then(|| {
                    ledger
                        .live_sessions()
                        .filter(|session| {
                            ledger.subscriptions.iter().any(|subscription| {
                                subscription.session_id == session.id
                                    && types.contains(&subscription.subscription_type)
                            })
                        })
                        .map(|session| session.id.clone())
                        .collect()
                })
            })
            .await
            .map(|session_ids| ActionDetail::TwitchSubscribed { session_ids })
            .map_err(|e| with_ledger(e.to_string(), twitch))
    }

    async fn send_crowd(&self, crowd: &Crowd) -> Result<ActionDetail, ActionFailure> {
        let twitch = self.twitch()?;
        let plan = crowd.plan();
        let total = plan.len();
        for (index, message) in plan.iter().enumerate() {
            if index > 0 && crowd.spacing_ms > 0 {
                tokio::time::sleep(Duration::from_millis(crowd.spacing_ms)).await;
            }
            if let Err(e) = twitch
                .inject_chat_message(&message.viewer, &message.text)
                .await
            {
                let reason = format!("crowd message {} of {total}: {e}", index + 1);
                return Err(with_ledger(reason, twitch));
            }
        }
        Ok(ActionDetail::CrowdSent { messages: total })
    }

    async fn reconnect(&self, within_ms: u64) -> Result<ActionDetail, ActionFailure> {
        let twitch = self.twitch()?;
        let predecessors: Vec<String> = twitch
            .ledger()
            .live_sessions()
            .map(|session| session.id.clone())
            .collect();
        twitch
            .inject_session_reconnect()
            .await
            .map_err(|e| with_ledger(e.to_string(), twitch))?;
        twitch
            .wait_for(
                "a successor EventSub session",
                Duration::from_millis(within_ms),
                |ledger| {
                    ledger.live_sessions().find_map(|session| {
                        let from = session.reconnected_from.as_ref()?;
                        predecessors
                            .contains(from)
                            .then(|| (from.clone(), session.id.clone()))
                    })
                },
            )
            .await
            .map(
                |(from_session, to_session)| ActionDetail::SessionReconnected {
                    from_session,
                    to_session,
                },
            )
            .map_err(|e| with_ledger(e.to_string(), twitch))
    }
}

fn plain(reason: String) -> ActionFailure {
    ActionFailure {
        reason,
        ledger: None,
    }
}

fn refused(error: EmulatorError) -> ActionFailure {
    plain(error.to_string())
}

fn live_types(ledger: &crate::twitch::Ledger) -> String {
    let types = live_subscription_types(ledger);
    if types.is_empty() {
        "none".to_owned()
    } else {
        types.join(", ")
    }
}

fn with_ledger(reason: String, twitch: &FakeTwitch) -> ActionFailure {
    ActionFailure {
        reason,
        ledger: Some(subscription_excerpt(&twitch.ledger())),
    }
}
