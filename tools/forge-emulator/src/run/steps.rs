use std::collections::HashMap;
use std::time::Duration;

use axum::http::StatusCode;
use forge_donatello::DONATELLO_INTEGRATION;
use forge_monobank::MONOBANK_INTEGRATION;
use forge_types::ActionId;
use time::OffsetDateTime;

use super::forge_host::ForgeHost;
use super::ledger_checks::{
    holds_live_subscription, live_subscription_types, subscription_excerpt,
};
use super::outcome::{ActionDetail, LedgerExcerpt};
use crate::EmulatorError;
use crate::control::ControlClient;
use crate::donatello::{DONATES_PATH, FakeDonatello};
use crate::fixture::SeedReport;
use crate::kick::{CHANNELS_PATH, FakeKick};
use crate::monobank::{FakeMonobank, MonobankEndpoint};
use crate::obs::{
    CURRENT_PROGRAM_SCENE_CHANGED, FakeObs, INPUT_MUTE_STATE_CHANGED, STREAM_STATE_CHANGED,
};
use crate::overlay::OverlayPages;
use crate::scenario::{Crowd, DonatelloGift, MonobankGift, OfflineGift, StepAction};
use crate::twitch::FakeTwitch;
use crate::vtube::{
    FakeVTube, HOTKEY_TRIGGERED_EVENT, ITEM_EVENT, MODEL_CONFIG_CHANGED_EVENT, MODEL_LOADED_EVENT,
    TRACKING_STATUS_CHANGED_EVENT,
};
use crate::youtube::{FakeYouTube, LIVE_CHAT_MESSAGES_PATH};

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
    pub(crate) kick: Option<&'a FakeKick>,
    pub(crate) youtube: Option<&'a FakeYouTube>,
    pub(crate) obs: Option<&'a FakeObs>,
    pub(crate) vtube: Option<&'a FakeVTube>,
    pub(crate) donations: DonationFakes<'a>,
    pub(crate) actions: &'a ActionIndex,
    pub(crate) pages: &'a OverlayPages,
}

#[derive(Clone, Copy, Default)]
pub struct DonationFakes<'a> {
    pub donatello: Option<&'a FakeDonatello>,
    pub monobank: Option<(&'a FakeMonobank, &'a str)>,
}

impl DonationFakes<'_> {
    pub(crate) fn donate_on_donatello(
        &self,
        gift: &DonatelloGift,
    ) -> Result<ActionDetail, ActionFailure> {
        let donatello = self
            .donatello
            .ok_or_else(|| plain("the run has no fake Donatello".to_owned()))?;
        donatello.donate(gift.to_fake(OffsetDateTime::now_utc()).map_err(refused)?);
        Ok(ActionDetail::DonationListed {
            provider: DONATELLO_INTEGRATION.id.as_str().to_owned(),
            donation_id: gift.id.clone(),
        })
    }

    pub(crate) fn top_up_on_monobank(
        &self,
        gift: &MonobankGift,
    ) -> Result<ActionDetail, ActionFailure> {
        let (monobank, jar) = self
            .monobank
            .ok_or_else(|| plain("the run has no fake monobank".to_owned()))?;
        monobank.top_up(jar, gift.to_fake(OffsetDateTime::now_utc()));
        Ok(ActionDetail::DonationListed {
            provider: MONOBANK_INTEGRATION.id.as_str().to_owned(),
            donation_id: gift.id.clone(),
        })
    }

    pub(crate) fn deliver(&self, gift: &OfflineGift) -> Result<ActionDetail, ActionFailure> {
        match gift {
            OfflineGift::Donatello(gift) => self.donate_on_donatello(gift),
            OfflineGift::Monobank(gift) => self.top_up_on_monobank(gift),
        }
    }

    pub(crate) async fn await_lists(
        &self,
        within: Duration,
    ) -> Result<ActionDetail, ActionFailure> {
        let mut services = Vec::new();
        if let Some(donatello) = self.donatello {
            let from = donatello.requests().len();
            donatello
                .wait_for("a Donatello donation list request", within, |requests| {
                    requests[from..]
                        .iter()
                        .any(|request| {
                            request.path == DONATES_PATH
                                && request.status == StatusCode::OK.as_u16()
                        })
                        .then_some(())
                })
                .await
                .map_err(refused)?;
            services.push(DONATELLO_INTEGRATION.id.as_str().to_owned());
        }
        if let Some((monobank, _)) = self.monobank {
            let from = monobank.requests().len();
            monobank
                .wait_for("a monobank statement request", within, |requests| {
                    requests[from..]
                        .iter()
                        .any(|request| {
                            request.endpoint == Some(MonobankEndpoint::Statement)
                                && request.status == StatusCode::OK.as_u16()
                        })
                        .then_some(())
                })
                .await
                .map_err(refused)?;
            services.push(MONOBANK_INTEGRATION.id.as_str().to_owned());
        }
        Ok(ActionDetail::DonationsPolled { services })
    }
}

pub(crate) async fn restart_forge(
    forge: &ForgeHost,
    donations: DonationFakes<'_>,
    within_ms: u64,
    offline: &[OfflineGift],
) -> Result<ActionDetail, ActionFailure> {
    forge.stop().await.map_err(refused)?;
    for gift in offline {
        donations.deliver(gift)?;
    }
    let pid = forge
        .start(Duration::from_millis(within_ms))
        .await
        .map_err(refused)?;
    Ok(ActionDetail::ForgeRestarted {
        offline: offline.iter().map(|gift| gift.id().to_owned()).collect(),
        pid,
    })
}

impl Stimuli<'_> {
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
            StepAction::KickChatJoined { within_ms } => {
                let kick = self.kick()?;
                let channel = kick.chat_channel();
                kick.wait_for(
                    "a Kick chat connection joined to the chatroom",
                    Duration::from_millis(*within_ms),
                    |ledger| ledger.joined(&channel),
                )
                .await
                .map(|session| ActionDetail::KickChatJoined { session })
                .map_err(refused)
            }
            StepAction::KickChat(message) => self
                .kick()?
                .inject_chat_message(&message.sender, &message.text)
                .await
                .map(|message_id| ActionDetail::KickChatSent { message_id })
                .map_err(refused),
            StepAction::KickPusherEvent { event, data } => self
                .kick()?
                .push_event(event, data)
                .await
                .map(|delivered| ActionDetail::KickEventPushed {
                    event: event.clone(),
                    delivered,
                })
                .map_err(refused),
            StepAction::KickStream { live } => {
                self.kick()?.set_live(*live);
                Ok(ActionDetail::KickStreamSet { live: *live })
            }
            StepAction::KickChannelPolled { within_ms } => {
                let kick = self.kick()?;
                let from = kick.ledger().requests.len();
                kick.wait_for(
                    "a Kick channel poll",
                    Duration::from_millis(*within_ms),
                    |ledger| {
                        ledger.requests[from..]
                            .iter()
                            .any(|request| {
                                request.path == CHANNELS_PATH
                                    && request.status == StatusCode::OK.as_u16()
                            })
                            .then_some(())
                    },
                )
                .await
                .map(|()| ActionDetail::KickChannelPolled)
                .map_err(refused)
            }
            StepAction::YoutubeChatPolled { within_ms } => {
                let youtube = self.youtube()?;
                let from = youtube.ledger().requests.len();
                youtube
                    .wait_for(
                        "a successful YouTube chat poll (forge needs non-empty build-time FORGE_YOUTUBE_CLIENT_ID and FORGE_YOUTUBE_CLIENT_SECRET to wire YouTube at all)",
                        Duration::from_millis(*within_ms),
                        |ledger| {
                            ledger.requests[from..]
                                .iter()
                                .any(|request| {
                                    request.method == "GET"
                                        && request.path == LIVE_CHAT_MESSAGES_PATH
                                        && request.status == StatusCode::OK.as_u16()
                                })
                                .then_some(())
                        },
                    )
                    .await
                    .map(|()| ActionDetail::YoutubeChatPolled)
                    .map_err(refused)
            }
            StepAction::YoutubeChat(message) => self
                .youtube()?
                .inject_chat_message(&message.author, &message.text)
                .map(|message_id| ActionDetail::YoutubeChatSent { message_id })
                .map_err(refused),
            StepAction::YoutubeChatEvent { author, snippet } => {
                let snippet = snippet.as_object().cloned().ok_or_else(|| {
                    plain("the YouTube chat event snippet must be an object".to_owned())
                })?;
                self.youtube()?
                    .inject_chat_event(author, snippet)
                    .map(|message_id| ActionDetail::YoutubeChatSent { message_id })
                    .map_err(refused)
            }
            StepAction::YoutubeBroadcast { live } => {
                self.youtube()?.set_live(*live);
                Ok(ActionDetail::YoutubeBroadcastSet { live: *live })
            }
            StepAction::ObsOnline {} => self
                .obs()?
                .go_online()
                .await
                .map(|()| ActionDetail::ObsOnline)
                .map_err(refused),
            StepAction::ObsIdentified { within_ms } => self
                .obs()?
                .wait_for(
                    "an identified OBS WebSocket session",
                    Duration::from_millis(*within_ms),
                    |ledger| ledger.live_sessions().last().map(|session| session.id),
                )
                .await
                .map(|session| ActionDetail::ObsIdentified { session })
                .map_err(refused),
            StepAction::ObsRestart { down_ms } => self
                .obs()?
                .restart(Duration::from_millis(*down_ms))
                .await
                .map(|closed_sessions| ActionDetail::ObsRestarted { closed_sessions })
                .map_err(refused),
            StepAction::ObsSceneSwitch { scene } => {
                let delivered = self.obs()?.switch_scene(scene).map_err(refused)?;
                pushed(CURRENT_PROGRAM_SCENE_CHANGED, delivered)
            }
            StepAction::ObsStream { active } => {
                let delivered = self.obs()?.set_streaming(*active).map_err(refused)?;
                pushed(STREAM_STATE_CHANGED, delivered)
            }
            StepAction::ObsInputMute { input, muted } => {
                let delivered = self.obs()?.set_input_mute(input, *muted).map_err(refused)?;
                pushed(INPUT_MUTE_STATE_CHANGED, delivered)
            }
            StepAction::VtubeOnline {} => self
                .vtube()?
                .go_online()
                .await
                .map(|()| ActionDetail::VTubeOnline)
                .map_err(refused),
            StepAction::VtubeAuthenticated { within_ms } => self
                .vtube()?
                .wait_for(
                    "an authenticated VTube Studio session",
                    Duration::from_millis(*within_ms),
                    |ledger| ledger.live_sessions().last().map(|session| session.id),
                )
                .await
                .map(|session| ActionDetail::VTubeAuthenticated { session })
                .map_err(refused),
            StepAction::VtubeHotkey { hotkey } => {
                let delivered = self.vtube()?.trigger_hotkey(hotkey).map_err(refused)?;
                vtube_pushed(HOTKEY_TRIGGERED_EVENT, delivered)
            }
            StepAction::VtubeModelLoad { model } => {
                let delivered = self.vtube()?.load_model(model).map_err(refused)?;
                vtube_pushed(MODEL_LOADED_EVENT, delivered)
            }
            StepAction::VtubeModelUnload {} => {
                let delivered = self.vtube()?.unload_model().map_err(refused)?;
                vtube_pushed(MODEL_LOADED_EVENT, delivered)
            }
            StepAction::VtubeModelConfigChanged {} => {
                let delivered = self.vtube()?.change_model_config().map_err(refused)?;
                vtube_pushed(MODEL_CONFIG_CHANGED_EVENT, delivered)
            }
            StepAction::VtubeTracking { face_found } => {
                let delivered = self.vtube()?.set_face_found(*face_found).map_err(refused)?;
                vtube_pushed(TRACKING_STATUS_CHANGED_EVENT, delivered)
            }
            StepAction::VtubeItemAdded { file } => {
                let delivered = self.vtube()?.add_item(file).map_err(refused)?;
                vtube_pushed(ITEM_EVENT, delivered)
            }
            StepAction::VtubeItemRemoved { file } => {
                let delivered = self.vtube()?.remove_item(file).map_err(refused)?;
                vtube_pushed(ITEM_EVENT, delivered)
            }
            StepAction::VtubeExpression { file, active } => self
                .vtube()?
                .set_expression(file, *active)
                .map(|_| ActionDetail::VTubeStateChanged)
                .map_err(refused),
            StepAction::DonatelloDonation(gift) => self.donations.donate_on_donatello(gift),
            StepAction::MonobankTopUp(gift) => self.donations.top_up_on_monobank(gift),
            StepAction::DonationsPolled { within_ms } => {
                self.donations
                    .await_lists(Duration::from_millis(*within_ms))
                    .await
            }
            StepAction::ForgeRestart { .. } => Err(plain(
                "forge_restart replaces the running forge, so the session performs it".to_owned(),
            )),
        }
    }

    fn kick(&self) -> Result<&FakeKick, ActionFailure> {
        self.kick
            .ok_or_else(|| plain("the run has no fake Kick".to_owned()))
    }

    fn youtube(&self) -> Result<&FakeYouTube, ActionFailure> {
        self.youtube
            .ok_or_else(|| plain("the run has no fake YouTube".to_owned()))
    }

    fn vtube(&self) -> Result<&FakeVTube, ActionFailure> {
        self.vtube
            .ok_or_else(|| plain("the run has no fake VTube Studio".to_owned()))
    }

    fn obs(&self) -> Result<&FakeObs, ActionFailure> {
        self.obs
            .ok_or_else(|| plain("the run has no fake OBS".to_owned()))
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

fn pushed(event_type: &str, delivered: usize) -> Result<ActionDetail, ActionFailure> {
    if delivered == 0 {
        return Err(plain(format!(
            "no identified OBS session subscribed to {event_type} received it"
        )));
    }
    Ok(ActionDetail::ObsEventPushed {
        event_type: event_type.to_owned(),
        delivered,
    })
}

fn vtube_pushed(event_name: &str, delivered: usize) -> Result<ActionDetail, ActionFailure> {
    if delivered == 0 {
        return Err(plain(format!(
            "no authenticated VTube Studio session subscribed to {event_name} received it"
        )));
    }
    Ok(ActionDetail::VTubeEventPushed {
        event_name: event_name.to_owned(),
        delivered,
    })
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
