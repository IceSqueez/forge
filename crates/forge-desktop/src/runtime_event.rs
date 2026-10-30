use forge_discord::vocabulary::event_kinds as discord_kinds;
use forge_events::{
    ActionDone, ActionSkipped, ActionStarted, ChatSendFailed, ChatSent, CommandMatched, Event,
    EventSource, GlobalArrayAppended, GlobalArrayRemoved, GlobalDecremented, GlobalDeleted,
    GlobalIncremented, GlobalSet, GlobalToggled, LatestChanged, PlatformConnectionChanged,
    QuickActionDone, RequestFailed, RunOutcome, ScriptFailed, ScriptHttpCall, ScriptLogLevel,
    ScriptLogged, ScriptReloaded, ScriptStarted, SubActionDone, SubActionRun, TimerTick,
    TypedEvent, WhisperSent,
};
use forge_hotkey::vocabulary::{HotkeyEnableFailed, HotkeyPressed, event_kinds as hotkey_kinds};
use forge_midi::vocabulary::event_kinds as midi_kinds;
use forge_obs::vocabulary::event_kinds as obs_kinds;
use forge_platform_kick::vocabulary::event_kinds as kick_kinds;
use forge_platform_twitch::vocabulary::{
    BotStatusChanged, PlatformReauthRequired, SenderFallback, event_kinds as twitch_kinds,
};
use forge_platform_youtube::vocabulary::event_kinds as youtube_kinds;
use forge_runtime::OVERLAY_SPEECH_FAILED_KIND;
use forge_soundboard::vocabulary::{
    ClipAdoptionSettled, PlaybackFailed, PlaybackFinished, PlaybackStarted,
};
use forge_speak_queue::vocabulary::SpeakFailed;
use forge_types::{ClipId, PlatformId};
use forge_vtube::VTubeError;
use forge_vtube::vocabulary::{VTubeConnectionChanged, VTubeDisconnectReason};

const TWITCH_KIND_PREFIX: &str = "twitch.";

const CHAT_MESSAGE_KINDS: [&str; 3] = [
    twitch_kinds::CHANNEL_CHAT_MESSAGE,
    youtube_kinds::CHAT_MESSAGE,
    kick_kinds::CHAT_MESSAGE_SENT,
];

const BAN_CHANGE_KINDS: [(&str, PlatformId); 4] = [
    (twitch_kinds::CHANNEL_BAN, PlatformId::Twitch),
    (twitch_kinds::CHANNEL_UNBAN, PlatformId::Twitch),
    (youtube_kinds::CHANNEL_USER_BANNED, PlatformId::YouTube),
    (kick_kinds::MODERATION_BANNED, PlatformId::Kick),
];

const GLOBAL_CHANGE_KINDS: [&str; 7] = [
    GlobalSet::KIND,
    GlobalIncremented::KIND,
    GlobalDecremented::KIND,
    GlobalToggled::KIND,
    GlobalArrayAppended::KIND,
    GlobalArrayRemoved::KIND,
    GlobalDeleted::KIND,
];

const SCRIPT_ACTIVITY_KINDS: [&str; 2] = [ScriptHttpCall::KIND, ScriptReloaded::KIND];

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ClipPlayback {
    Started(PlaybackStarted),
    Finished(PlaybackFinished),
    Failed(PlaybackFailed),
}

impl ClipPlayback {
    pub(crate) fn clip_id(&self) -> Option<ClipId> {
        match self {
            Self::Started(started) => started.clip_id,
            Self::Finished(finished) => finished.clip_id,
            Self::Failed(failed) => failed.clip_id,
        }
    }

    pub(crate) fn play_id(&self) -> u64 {
        match self {
            Self::Started(started) => started.play_id,
            Self::Finished(finished) => finished.play_id,
            Self::Failed(failed) => failed.play_id,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ObsConnection {
    Connected,
    PasswordRejected,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RuntimeEvent {
    PlatformConnection(PlatformConnectionChanged),
    ReauthRequired(PlatformReauthRequired),
    TwitchBotStatus(BotStatusChanged),
    SenderFallback(SenderFallback),
    ObsConnection(ObsConnection),
    ObsSceneChanged,
    VTubeConnection(VTubeConnectionChanged),
    ChatMessage,
    CommandMatched(CommandMatched),
    ChatSent(ChatSent),
    ChatSendFailed(ChatSendFailed),
    WhisperSent(WhisperSent),
    ActionStarted(ActionStarted),
    ActionDone(ActionDone),
    ActionSkipped(ActionSkipped),
    QuickActionDone(QuickActionDone),
    SubActionRun(SubActionRun),
    SubActionDone(SubActionDone),
    TimerTick(TimerTick),
    GlobalChanged,
    ScriptStarted(ScriptStarted),
    ScriptFailed(ScriptFailed),
    ScriptLogged(ScriptLogged),
    ScriptActivity,
    RequestFailed(RequestFailed),
    LatestChanged(LatestChanged),
    HotkeyPressed(HotkeyPressed),
    HotkeyEnableFailed(HotkeyEnableFailed),
    HotkeyRegistrationsChanged,
    ClipPlayback(ClipPlayback),
    ClipAdoptionSettled(ClipAdoptionSettled),
    SpeakFailed(SpeakFailed),
    OverlaySpeechFailed,
    DiscordWebhookFailed,
}

pub(crate) fn decode(event: &Event) -> Option<RuntimeEvent> {
    let kind = event.kind.as_str();
    match (event.source, kind) {
        (_, PlatformConnectionChanged::KIND) => {
            PlatformConnectionChanged::from_event(event).map(RuntimeEvent::PlatformConnection)
        }
        (EventSource::Twitch, PlatformReauthRequired::KIND) => {
            PlatformReauthRequired::from_event(event).map(RuntimeEvent::ReauthRequired)
        }
        (EventSource::Twitch, BotStatusChanged::KIND) => {
            BotStatusChanged::from_event(event).map(RuntimeEvent::TwitchBotStatus)
        }
        (_, SenderFallback::KIND) => {
            SenderFallback::from_event(event).map(RuntimeEvent::SenderFallback)
        }
        (EventSource::Obs, obs_kinds::CONNECTION_CONNECTED) => {
            Some(RuntimeEvent::ObsConnection(ObsConnection::Connected))
        }
        (EventSource::Obs, obs_kinds::CONNECTION_AUTH_FAILED) => {
            Some(RuntimeEvent::ObsConnection(ObsConnection::PasswordRejected))
        }
        (_, obs_kinds::SCENE_CHANGED) => Some(RuntimeEvent::ObsSceneChanged),
        (EventSource::VTube, VTubeConnectionChanged::KIND) => {
            VTubeConnectionChanged::from_event(event).map(RuntimeEvent::VTubeConnection)
        }
        (_, kind) if CHAT_MESSAGE_KINDS.contains(&kind) => Some(RuntimeEvent::ChatMessage),
        (_, CommandMatched::KIND) => {
            CommandMatched::from_event(event).map(RuntimeEvent::CommandMatched)
        }
        (_, ChatSent::KIND) => ChatSent::from_event(event).map(RuntimeEvent::ChatSent),
        (_, ChatSendFailed::KIND) => {
            ChatSendFailed::from_event(event).map(RuntimeEvent::ChatSendFailed)
        }
        (_, WhisperSent::KIND) => WhisperSent::from_event(event).map(RuntimeEvent::WhisperSent),
        (_, ActionStarted::KIND) => {
            ActionStarted::from_event(event).map(RuntimeEvent::ActionStarted)
        }
        (_, ActionDone::KIND) => ActionDone::from_event(event).map(RuntimeEvent::ActionDone),
        (_, ActionSkipped::KIND) => {
            ActionSkipped::from_event(event).map(RuntimeEvent::ActionSkipped)
        }
        (_, QuickActionDone::KIND) => {
            QuickActionDone::from_event(event).map(RuntimeEvent::QuickActionDone)
        }
        (_, SubActionRun::KIND) => SubActionRun::from_event(event).map(RuntimeEvent::SubActionRun),
        (_, SubActionDone::KIND) => {
            SubActionDone::from_event(event).map(RuntimeEvent::SubActionDone)
        }
        (_, TimerTick::KIND) => TimerTick::from_event(event).map(RuntimeEvent::TimerTick),
        (_, kind) if GLOBAL_CHANGE_KINDS.contains(&kind) => Some(RuntimeEvent::GlobalChanged),
        (_, ScriptStarted::KIND) => {
            ScriptStarted::from_event(event).map(RuntimeEvent::ScriptStarted)
        }
        (_, ScriptFailed::KIND) => ScriptFailed::from_event(event).map(RuntimeEvent::ScriptFailed),
        (_, ScriptLogged::KIND) => ScriptLogged::from_event(event).map(RuntimeEvent::ScriptLogged),
        (_, kind) if SCRIPT_ACTIVITY_KINDS.contains(&kind) => Some(RuntimeEvent::ScriptActivity),
        (_, RequestFailed::KIND) => {
            RequestFailed::from_event(event).map(RuntimeEvent::RequestFailed)
        }
        (_, LatestChanged::KIND) => {
            LatestChanged::from_event(event).map(RuntimeEvent::LatestChanged)
        }
        (EventSource::Hotkey, HotkeyPressed::KIND) => {
            HotkeyPressed::from_event(event).map(RuntimeEvent::HotkeyPressed)
        }
        (EventSource::Hotkey, HotkeyEnableFailed::KIND) => {
            HotkeyEnableFailed::from_event(event).map(RuntimeEvent::HotkeyEnableFailed)
        }
        (EventSource::Hotkey, hotkey_kinds::REGISTERED | hotkey_kinds::UNREGISTERED) => {
            Some(RuntimeEvent::HotkeyRegistrationsChanged)
        }
        (EventSource::Audio, PlaybackStarted::KIND) => PlaybackStarted::from_event(event)
            .map(|started| RuntimeEvent::ClipPlayback(ClipPlayback::Started(started))),
        (EventSource::Audio, PlaybackFinished::KIND) => PlaybackFinished::from_event(event)
            .map(|finished| RuntimeEvent::ClipPlayback(ClipPlayback::Finished(finished))),
        (EventSource::Audio, PlaybackFailed::KIND) => PlaybackFailed::from_event(event)
            .map(|failed| RuntimeEvent::ClipPlayback(ClipPlayback::Failed(failed))),
        (EventSource::Audio, ClipAdoptionSettled::KIND) => {
            ClipAdoptionSettled::from_event(event).map(RuntimeEvent::ClipAdoptionSettled)
        }
        (_, SpeakFailed::KIND) => SpeakFailed::from_event(event).map(RuntimeEvent::SpeakFailed),
        (EventSource::Core, OVERLAY_SPEECH_FAILED_KIND) => Some(RuntimeEvent::OverlaySpeechFailed),
        (_, discord_kinds::WEBHOOK_FAILED) => Some(RuntimeEvent::DiscordWebhookFailed),
        _ => None,
    }
}

impl RuntimeEvent {
    pub(crate) fn is_failure(&self) -> bool {
        match self {
            Self::ActionDone(done) => done.outcome == RunOutcome::Failed,
            Self::SubActionDone(done) => done.outcome == RunOutcome::Failed,
            Self::QuickActionDone(done) => done.outcome == RunOutcome::Failed,
            Self::ScriptLogged(logged) => logged.level == ScriptLogLevel::Error,
            Self::ObsConnection(connection) => *connection == ObsConnection::PasswordRejected,
            Self::ClipPlayback(playback) => matches!(playback, ClipPlayback::Failed(_)),
            Self::ChatSendFailed(_)
            | Self::ScriptFailed(_)
            | Self::RequestFailed(_)
            | Self::HotkeyEnableFailed(_)
            | Self::SpeakFailed(_)
            | Self::OverlaySpeechFailed
            | Self::DiscordWebhookFailed => true,
            _ => false,
        }
    }

    pub(crate) fn is_warning(&self) -> bool {
        matches!(self, Self::SenderFallback(_))
    }

    pub(crate) fn is_run_bookkeeping(&self) -> bool {
        matches!(
            self,
            Self::ActionStarted(_)
                | Self::SubActionRun(_)
                | Self::SubActionDone(_)
                | Self::GlobalChanged
                | Self::ScriptStarted(_)
                | Self::ScriptFailed(_)
                | Self::ScriptLogged(_)
                | Self::ScriptActivity
        )
    }
}

pub(crate) fn viewer_ban_platform(event: &Event) -> Option<PlatformId> {
    BAN_CHANGE_KINDS
        .iter()
        .find(|(change, _)| *change == event.kind)
        .map(|(_, platform)| *platform)
}

pub(crate) fn marks_chat_run(event: &Event) -> bool {
    matches!(
        event.kind.as_str(),
        CommandMatched::KIND | ActionStarted::KIND
    )
}

pub(crate) fn twitch_topic(event: &Event) -> Option<&str> {
    event.kind.strip_prefix(TWITCH_KIND_PREFIX)
}

pub(crate) fn is_twitch_follow(event: &Event) -> bool {
    event.kind == twitch_kinds::CHANNEL_FOLLOW
}

pub(crate) fn is_hotkey_event(event: &Event) -> bool {
    event.kind.starts_with(hotkey_kinds::PREFIX)
}

pub(crate) fn is_discord_event(event: &Event) -> bool {
    event.kind.starts_with(discord_kinds::PREFIX)
}

pub(crate) fn is_midi_port_event(event: &Event) -> bool {
    event.kind.starts_with(midi_kinds::PORT_PREFIX)
}

pub(crate) fn changes_vtube_connection(event: &Event) -> bool {
    event.source == EventSource::VTube && event.kind == VTubeConnectionChanged::KIND
}

pub(crate) fn changes_obs_content(event: &Event) -> bool {
    event.source == EventSource::Obs
        && obs_kinds::CATALOG_CHANGING_KINDS.contains(&event.kind.as_str())
}

pub(crate) fn changes_obs_catalog(event: &Event) -> bool {
    event.source == EventSource::Obs
        && (event.kind.starts_with(obs_kinds::CONNECTION_PREFIX)
            || obs_kinds::NAME_LIST_CHANGING_KINDS.contains(&event.kind.as_str()))
}

pub(crate) fn rebuilds_descriptors(event: &Event) -> bool {
    match event.source {
        EventSource::Obs => event.kind.starts_with(obs_kinds::CONNECTION_PREFIX),
        EventSource::Twitch => {
            twitch_kinds::DESCRIPTOR_REBUILDING_KINDS.contains(&event.kind.as_str())
        }
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VTubeDisconnect {
    AwaitingApproval,
    Unreachable,
    TokenRejected,
    Denied,
    TimedOut,
    AuthFailed,
    SubscribeFailed,
    SocketClosed,
    Unresponsive,
    NoAnswer,
    Unknown,
}

impl From<VTubeDisconnectReason> for VTubeDisconnect {
    fn from(reason: VTubeDisconnectReason) -> Self {
        match reason {
            VTubeDisconnectReason::AwaitingApproval => Self::AwaitingApproval,
            VTubeDisconnectReason::ConnectFailed => Self::Unreachable,
            VTubeDisconnectReason::AuthRequired => Self::TokenRejected,
            VTubeDisconnectReason::AuthDenied => Self::Denied,
            VTubeDisconnectReason::AuthTimeout => Self::TimedOut,
            VTubeDisconnectReason::AuthFailed => Self::AuthFailed,
            VTubeDisconnectReason::SubscribeFailed => Self::SubscribeFailed,
            VTubeDisconnectReason::SocketClosed => Self::SocketClosed,
            VTubeDisconnectReason::Unresponsive => Self::Unresponsive,
        }
    }
}

impl From<&VTubeError> for VTubeDisconnect {
    fn from(error: &VTubeError) -> Self {
        match error {
            VTubeError::Connect(_) => Self::Unreachable,
            VTubeError::TokenRejected => Self::TokenRejected,
            VTubeError::TokenDenied => Self::Denied,
            VTubeError::TokenTimeout => Self::TimedOut,
            VTubeError::Timeout => Self::NoAnswer,
            _ => Self::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_events::{ActingSender, ConnectionState, RunOutcome, ScriptLogLevel};
    use forge_platform_twitch::BotModerator;
    use forge_platform_twitch::vocabulary::{BotStatusTag, FallbackLevel};
    use forge_soundboard::vocabulary::SettledVerdict;
    use forge_speak_queue::RequestId;
    use forge_speak_queue::vocabulary::FailureReason;
    use forge_types::{ActionId, QueueId, TriggerInstanceId};
    use serde_json::json;

    use super::*;

    const SERVER_ERROR_STATUS: u16 = 503;

    fn typed<T: TypedEvent>(value: T, source: EventSource) -> Event {
        value.into_event(source, None)
    }

    fn bare(source: EventSource, kind: &str) -> Event {
        Event::new(source, kind, json!({}))
    }

    fn action_done(outcome: RunOutcome) -> ActionDone {
        ActionDone {
            action_id: ActionId::new(),
            action_name: "Shoutout".to_owned(),
            outcome,
            total_ms: 12,
            manual_test: false,
            reason: None,
            reason_text: None,
        }
    }

    fn sub_action_done(outcome: RunOutcome) -> SubActionDone {
        SubActionDone {
            step_index: 0,
            kind: "chat.send".to_owned(),
            outcome,
            duration_ms: 3,
            message: None,
            message_text: None,
            integration: None,
        }
    }

    fn quick_action_done(outcome: RunOutcome) -> QuickActionDone {
        QuickActionDone {
            kind: "obs.scene".to_owned(),
            outcome,
            label: "Go live".to_owned(),
            builtin_id: "obs".to_owned(),
        }
    }

    fn script_logged(level: ScriptLogLevel) -> ScriptLogged {
        ScriptLogged {
            level,
            message: "tick".to_owned(),
            script_id: None,
        }
    }

    fn playback_failed() -> PlaybackFailed {
        PlaybackFailed {
            play_id: 7,
            clip_id: Some(ClipId::new()),
            clip_label: Some("Air Horn".to_owned()),
            error: "device lost".to_owned(),
        }
    }

    fn sender_fallback() -> SenderFallback {
        SenderFallback {
            level: FallbackLevel::Warning,
            operation: "chat.send".to_owned(),
            reason: "bot is not connected".to_owned(),
        }
    }

    fn bot_status() -> BotStatusChanged {
        BotStatusChanged {
            status: BotStatusTag::Connected,
            login: Some("forgebot".to_owned()),
            moderator: BotModerator::Moderator,
        }
    }

    fn vtube_changed() -> VTubeConnectionChanged {
        VTubeConnectionChanged {
            is_connected: true,
            endpoint: "ws://localhost:8001".to_owned(),
            reason: None,
            detail: None,
        }
    }

    fn hotkey_pressed() -> HotkeyPressed {
        HotkeyPressed {
            combo: "F9".to_owned(),
            id: 1,
            timestamp_us: 5,
        }
    }

    fn adoption_settled() -> ClipAdoptionSettled {
        ClipAdoptionSettled {
            clip_id: ClipId::new(),
            verdict: SettledVerdict::Refused {
                reason: "not audio".to_owned(),
            },
        }
    }

    fn speak_failed() -> SpeakFailed {
        SpeakFailed {
            request_id: RequestId::new(),
            reason: FailureReason::EngineError,
            message: forge_types::MessageRef::new(forge_types::MessageCode::from_static(
                "speak",
                "synthesis_failed",
            )),
            viewer_name: "viewer".to_owned(),
            text: "hello".to_owned(),
        }
    }

    fn typed_cases() -> Vec<(Event, RuntimeEvent)> {
        let connection = PlatformConnectionChanged {
            platform_id: "kick".to_owned(),
            state: ConnectionState::Connected,
            expected: false,
        };
        let reauth = PlatformReauthRequired {
            platform_id: PlatformId::Twitch,
        };
        let chat_sent = ChatSent {
            channel: "twitch".to_owned(),
            message: "hi".to_owned(),
            reply_to_message_id: None,
            sent_as: Some(ActingSender {
                role: forge_events::SenderRole::Bot,
                login: "forgebot".to_owned(),
            }),
        };
        let send_failed = ChatSendFailed {
            channel: "kick".to_owned(),
            error: "rate limited".to_owned(),
        };
        let matched = CommandMatched {
            command: "!so".to_owned(),
            kind_id: "twitch.chat.command".to_owned(),
        };
        let whispered = WhisperSent {
            channel: "twitch".to_owned(),
            whisper_to_login: "viewer".to_owned(),
            sent_as: Some(ActingSender {
                role: forge_events::SenderRole::Bot,
                login: "forgebot".to_owned(),
            }),
        };
        let started = ActionStarted {
            action_id: ActionId::new(),
            action_name: "Shoutout".to_owned(),
            sub_action_count: 2,
        };
        let skipped = ActionSkipped {
            action_id: ActionId::new(),
            reason: "queue busy".to_owned(),
            queue_id: QueueId::new(),
        };
        let step_run = SubActionRun {
            step_index: 1,
            kind: "chat.send".to_owned(),
        };
        let tick = TimerTick {
            instance_id: TriggerInstanceId::new(),
            interval_minutes: 5,
            message_count: 0,
        };
        let script_started = ScriptStarted {
            script_id: None,
            script_name: Some("greet".to_owned()),
            is_inline: true,
        };
        let script_failed = ScriptFailed {
            script_id: None,
            script_name: Some("greet".to_owned()),
            is_inline: false,
            error_type: "runtime".to_owned(),
            message: "boom".to_owned(),
        };
        let request_failed = RequestFailed {
            endpoint: "helix/chat".to_owned(),
            status_code: SERVER_ERROR_STATUS,
            body_snippet: String::new(),
            retry_after_secs: None,
        };
        let latest = LatestChanged {
            slot: "donation".to_owned(),
            platform: None,
            merged_changed: true,
            cleared: false,
        };
        let enable_failed = HotkeyEnableFailed {
            combos: vec!["F9".to_owned()],
        };
        let playback_started = PlaybackStarted {
            play_id: 1,
            clip_id: None,
            clip_label: None,
            device: "default".to_owned(),
            duration_secs: Some(1.5),
            looped: false,
        };
        let playback_finished = PlaybackFinished {
            play_id: 1,
            clip_id: None,
            clip_label: None,
        };
        let done = action_done(RunOutcome::Success);
        let playback_failed = playback_failed();
        let adoption = adoption_settled();
        let speak = speak_failed();
        vec![
            (
                typed(connection.clone(), EventSource::Kick),
                RuntimeEvent::PlatformConnection(connection),
            ),
            (
                typed(reauth.clone(), EventSource::Twitch),
                RuntimeEvent::ReauthRequired(reauth),
            ),
            (
                typed(bot_status(), EventSource::Twitch),
                RuntimeEvent::TwitchBotStatus(bot_status()),
            ),
            (
                typed(sender_fallback(), EventSource::Twitch),
                RuntimeEvent::SenderFallback(sender_fallback()),
            ),
            (
                typed(vtube_changed(), EventSource::VTube),
                RuntimeEvent::VTubeConnection(vtube_changed()),
            ),
            (
                typed(chat_sent.clone(), EventSource::Twitch),
                RuntimeEvent::ChatSent(chat_sent),
            ),
            (
                typed(matched.clone(), EventSource::Core),
                RuntimeEvent::CommandMatched(matched),
            ),
            (
                typed(whispered.clone(), EventSource::Twitch),
                RuntimeEvent::WhisperSent(whispered),
            ),
            (
                typed(send_failed.clone(), EventSource::Kick),
                RuntimeEvent::ChatSendFailed(send_failed),
            ),
            (
                typed(started.clone(), EventSource::Core),
                RuntimeEvent::ActionStarted(started),
            ),
            (
                typed(done.clone(), EventSource::Core),
                RuntimeEvent::ActionDone(done),
            ),
            (
                typed(skipped.clone(), EventSource::Core),
                RuntimeEvent::ActionSkipped(skipped),
            ),
            (
                typed(quick_action_done(RunOutcome::Success), EventSource::Obs),
                RuntimeEvent::QuickActionDone(quick_action_done(RunOutcome::Success)),
            ),
            (
                typed(step_run.clone(), EventSource::Core),
                RuntimeEvent::SubActionRun(step_run),
            ),
            (
                typed(sub_action_done(RunOutcome::Failed), EventSource::Core),
                RuntimeEvent::SubActionDone(sub_action_done(RunOutcome::Failed)),
            ),
            (
                typed(tick.clone(), EventSource::Timer),
                RuntimeEvent::TimerTick(tick),
            ),
            (
                typed(script_started.clone(), EventSource::Rhai),
                RuntimeEvent::ScriptStarted(script_started),
            ),
            (
                typed(script_failed.clone(), EventSource::Rhai),
                RuntimeEvent::ScriptFailed(script_failed),
            ),
            (
                typed(script_logged(ScriptLogLevel::Warn), EventSource::Rhai),
                RuntimeEvent::ScriptLogged(script_logged(ScriptLogLevel::Warn)),
            ),
            (
                typed(request_failed.clone(), EventSource::Http),
                RuntimeEvent::RequestFailed(request_failed),
            ),
            (
                typed(latest.clone(), EventSource::Core),
                RuntimeEvent::LatestChanged(latest),
            ),
            (
                typed(hotkey_pressed(), EventSource::Hotkey),
                RuntimeEvent::HotkeyPressed(hotkey_pressed()),
            ),
            (
                typed(enable_failed.clone(), EventSource::Hotkey),
                RuntimeEvent::HotkeyEnableFailed(enable_failed),
            ),
            (
                typed(playback_started.clone(), EventSource::Audio),
                RuntimeEvent::ClipPlayback(ClipPlayback::Started(playback_started)),
            ),
            (
                typed(playback_finished.clone(), EventSource::Audio),
                RuntimeEvent::ClipPlayback(ClipPlayback::Finished(playback_finished)),
            ),
            (
                typed(playback_failed.clone(), EventSource::Audio),
                RuntimeEvent::ClipPlayback(ClipPlayback::Failed(playback_failed)),
            ),
            (
                typed(adoption.clone(), EventSource::Audio),
                RuntimeEvent::ClipAdoptionSettled(adoption),
            ),
            (
                typed(speak.clone(), EventSource::Audio),
                RuntimeEvent::SpeakFailed(speak),
            ),
        ]
    }

    #[test]
    fn decode_returns_the_producers_typed_payload_for_every_typed_kind() {
        for (event, expected) in typed_cases() {
            assert_eq!(
                decode(&event),
                Some(expected),
                "{} from {:?}",
                event.kind,
                event.source
            );
        }
    }

    #[test]
    fn decode_recognises_every_payload_less_signal_by_kind() {
        let mut cases = vec![
            (
                bare(EventSource::Obs, obs_kinds::CONNECTION_CONNECTED),
                RuntimeEvent::ObsConnection(ObsConnection::Connected),
            ),
            (
                bare(EventSource::Obs, obs_kinds::CONNECTION_AUTH_FAILED),
                RuntimeEvent::ObsConnection(ObsConnection::PasswordRejected),
            ),
            (
                bare(EventSource::Obs, obs_kinds::SCENE_CHANGED),
                RuntimeEvent::ObsSceneChanged,
            ),
            (
                bare(EventSource::Twitch, twitch_kinds::CHANNEL_CHAT_MESSAGE),
                RuntimeEvent::ChatMessage,
            ),
            (
                bare(EventSource::YouTube, youtube_kinds::CHAT_MESSAGE),
                RuntimeEvent::ChatMessage,
            ),
            (
                bare(EventSource::Kick, kick_kinds::CHAT_MESSAGE_SENT),
                RuntimeEvent::ChatMessage,
            ),
            (
                bare(EventSource::Rhai, ScriptHttpCall::KIND),
                RuntimeEvent::ScriptActivity,
            ),
            (
                bare(EventSource::Rhai, ScriptReloaded::KIND),
                RuntimeEvent::ScriptActivity,
            ),
            (
                bare(EventSource::Hotkey, hotkey_kinds::REGISTERED),
                RuntimeEvent::HotkeyRegistrationsChanged,
            ),
            (
                bare(EventSource::Hotkey, hotkey_kinds::UNREGISTERED),
                RuntimeEvent::HotkeyRegistrationsChanged,
            ),
            (
                bare(EventSource::Core, OVERLAY_SPEECH_FAILED_KIND),
                RuntimeEvent::OverlaySpeechFailed,
            ),
            (
                bare(EventSource::Discord, discord_kinds::WEBHOOK_FAILED),
                RuntimeEvent::DiscordWebhookFailed,
            ),
        ];
        cases.extend(
            GLOBAL_CHANGE_KINDS
                .iter()
                .map(|kind| (bare(EventSource::Core, kind), RuntimeEvent::GlobalChanged)),
        );

        for (event, expected) in cases {
            assert_eq!(decode(&event), Some(expected), "{}", event.kind);
        }
    }

    #[test]
    fn decode_ignores_source_gated_kinds_published_by_another_source() {
        for event in [
            typed(
                PlatformReauthRequired {
                    platform_id: PlatformId::Kick,
                },
                EventSource::Kick,
            ),
            typed(bot_status(), EventSource::Kick),
            bare(EventSource::Core, obs_kinds::CONNECTION_CONNECTED),
            bare(EventSource::VTube, obs_kinds::CONNECTION_AUTH_FAILED),
            typed(vtube_changed(), EventSource::Obs),
            typed(hotkey_pressed(), EventSource::Midi),
            typed(HotkeyEnableFailed { combos: Vec::new() }, EventSource::Core),
            bare(EventSource::Midi, hotkey_kinds::REGISTERED),
            typed(playback_failed(), EventSource::Core),
            typed(adoption_settled(), EventSource::Core),
            bare(EventSource::Server, OVERLAY_SPEECH_FAILED_KIND),
        ] {
            assert_eq!(
                decode(&event),
                None,
                "{} from {:?}",
                event.kind,
                event.source
            );
        }
    }

    #[test]
    fn decode_drops_a_typed_kind_whose_payload_does_not_match_its_schema() {
        for (event, _) in typed_cases() {
            let malformed = Event::new(event.source, &event.kind, json!({ "unexpected": true }));

            assert_eq!(decode(&malformed), None, "{}", event.kind);
        }
    }

    #[test]
    fn decode_drops_a_typed_kind_with_an_unknown_enum_value() {
        let mut payload = serde_json::to_value(action_done(RunOutcome::Failed)).unwrap();
        payload["outcome"] = json!("exploded");

        let event = Event::new(EventSource::Core, ActionDone::KIND, payload);

        assert_eq!(decode(&event), None);
    }

    #[test]
    fn decode_ignores_kinds_the_desktop_does_not_read() {
        for event in [
            bare(EventSource::Twitch, twitch_kinds::CHANNEL_FOLLOW),
            bare(EventSource::Obs, "obs.scene"),
            bare(EventSource::Core, ""),
            bare(EventSource::Core, "ACTION.DONE"),
        ] {
            assert_eq!(decode(&event), None, "{:?}", event.kind);
        }
    }

    #[test]
    fn is_failure_follows_the_typed_outcome_not_the_kind() {
        for (event, failure) in [
            (
                RuntimeEvent::ActionDone(action_done(RunOutcome::Failed)),
                true,
            ),
            (
                RuntimeEvent::ActionDone(action_done(RunOutcome::Success)),
                false,
            ),
            (
                RuntimeEvent::ActionDone(action_done(RunOutcome::Skipped)),
                false,
            ),
            (
                RuntimeEvent::SubActionDone(sub_action_done(RunOutcome::Failed)),
                true,
            ),
            (
                RuntimeEvent::SubActionDone(sub_action_done(RunOutcome::Success)),
                false,
            ),
            (
                RuntimeEvent::QuickActionDone(quick_action_done(RunOutcome::Failed)),
                true,
            ),
            (
                RuntimeEvent::QuickActionDone(quick_action_done(RunOutcome::Skipped)),
                false,
            ),
            (
                RuntimeEvent::ScriptLogged(script_logged(ScriptLogLevel::Error)),
                true,
            ),
            (
                RuntimeEvent::ScriptLogged(script_logged(ScriptLogLevel::Warn)),
                false,
            ),
            (
                RuntimeEvent::ScriptLogged(script_logged(ScriptLogLevel::Info)),
                false,
            ),
            (
                RuntimeEvent::ObsConnection(ObsConnection::PasswordRejected),
                true,
            ),
            (RuntimeEvent::ObsConnection(ObsConnection::Connected), false),
            (
                RuntimeEvent::ClipPlayback(ClipPlayback::Failed(playback_failed())),
                true,
            ),
            (
                RuntimeEvent::ClipPlayback(ClipPlayback::Finished(PlaybackFinished {
                    play_id: 1,
                    clip_id: None,
                    clip_label: None,
                })),
                false,
            ),
            (
                RuntimeEvent::ChatSendFailed(ChatSendFailed {
                    channel: "kick".to_owned(),
                    error: "x".to_owned(),
                }),
                true,
            ),
            (
                RuntimeEvent::HotkeyEnableFailed(HotkeyEnableFailed { combos: Vec::new() }),
                true,
            ),
            (RuntimeEvent::SpeakFailed(speak_failed()), true),
            (typed_case(ScriptFailed::KIND), true),
            (typed_case(RequestFailed::KIND), true),
            (RuntimeEvent::OverlaySpeechFailed, true),
            (RuntimeEvent::DiscordWebhookFailed, true),
            (RuntimeEvent::SenderFallback(sender_fallback()), false),
            (RuntimeEvent::ChatMessage, false),
            (RuntimeEvent::GlobalChanged, false),
        ] {
            assert_eq!(event.is_failure(), failure, "{event:?}");
        }
    }

    #[test]
    fn only_a_sender_fallback_is_a_warning() {
        for (event, warning) in [
            (RuntimeEvent::SenderFallback(sender_fallback()), true),
            (
                RuntimeEvent::ScriptLogged(script_logged(ScriptLogLevel::Warn)),
                false,
            ),
            (
                RuntimeEvent::ActionDone(action_done(RunOutcome::Failed)),
                false,
            ),
            (RuntimeEvent::ChatMessage, false),
        ] {
            assert_eq!(event.is_warning(), warning, "{event:?}");
        }
    }

    fn typed_case(kind: &str) -> RuntimeEvent {
        typed_cases()
            .into_iter()
            .find(|(event, _)| event.kind == kind)
            .map(|(_, typed)| typed)
            .unwrap()
    }

    #[test]
    fn run_bookkeeping_covers_step_script_and_global_traffic_but_not_run_results() {
        for (event, bookkeeping) in [
            (typed_case(ActionStarted::KIND), true),
            (typed_case(SubActionRun::KIND), true),
            (typed_case(SubActionDone::KIND), true),
            (typed_case(ScriptStarted::KIND), true),
            (typed_case(ScriptFailed::KIND), true),
            (typed_case(ScriptLogged::KIND), true),
            (RuntimeEvent::GlobalChanged, true),
            (RuntimeEvent::ScriptActivity, true),
            (typed_case(ActionDone::KIND), false),
            (typed_case(ActionSkipped::KIND), false),
            (typed_case(QuickActionDone::KIND), false),
            (typed_case(TimerTick::KIND), false),
            (typed_case(ChatSent::KIND), false),
            (RuntimeEvent::ChatMessage, false),
            (RuntimeEvent::ObsSceneChanged, false),
            (RuntimeEvent::DiscordWebhookFailed, false),
        ] {
            assert_eq!(event.is_run_bookkeeping(), bookkeeping, "{event:?}");
        }
    }

    #[test]
    fn viewer_ban_platform_recognises_only_the_ban_and_unban_kinds_of_each_platform() {
        for (kind, expected) in [
            ("twitch.channel.ban", Some(PlatformId::Twitch)),
            ("twitch.channel.unban", Some(PlatformId::Twitch)),
            ("youtube.channel.user_banned", Some(PlatformId::YouTube)),
            ("kick.moderation.banned", Some(PlatformId::Kick)),
            ("twitch.channel.ban.extra", None),
            ("twitch.channel", None),
            ("TWITCH.CHANNEL.BAN", None),
            ("twitch.chat.message", None),
            ("kick.moderation.unbanned", None),
            ("", None),
        ] {
            assert_eq!(
                viewer_ban_platform(&bare(EventSource::Twitch, kind)),
                expected,
                "{kind:?}"
            );
        }
    }

    const PUBLISHED_LIFECYCLE_KINDS: [&str; 8] = [
        "twitch.channel.poll.begin",
        "twitch.channel.poll.progress",
        "twitch.channel.poll.end",
        "twitch.channel.prediction.begin",
        "twitch.channel.prediction.progress",
        "twitch.channel.prediction.lock",
        "twitch.channel.prediction.end",
        "twitch.channel.raid",
    ];

    #[test]
    fn obs_connection_and_twitch_lifecycle_events_rebuild_the_descriptors() {
        let mut cases: Vec<(EventSource, &str)> = vec![
            (EventSource::Obs, "obs.connection.opened"),
            (EventSource::Obs, "obs.connection.closed"),
        ];
        cases.extend(
            PUBLISHED_LIFECYCLE_KINDS
                .iter()
                .map(|kind| (EventSource::Twitch, *kind)),
        );

        for (source, kind) in cases {
            assert!(
                rebuilds_descriptors(&bare(source, kind)),
                "{source:?} / {kind} must trigger a descriptor rebuild"
            );
        }
    }

    #[test]
    fn unrelated_kinds_and_mismatched_sources_do_not_rebuild_the_descriptors() {
        for (source, kind) in [
            (EventSource::Twitch, "twitch.chat.message"),
            (EventSource::Twitch, "twitch.channel.follow"),
            (EventSource::Twitch, "twitch.channel.poll"),
            (EventSource::Twitch, "twitch.channel.raid_received"),
            (EventSource::Twitch, "obs.connection.opened"),
            (EventSource::Obs, "twitch.channel.poll.begin"),
            (EventSource::Obs, "obs.scene.changed"),
            (EventSource::Obs, "obs.connection"),
            (EventSource::Core, "twitch.channel.raid"),
            (EventSource::Kick, "obs.connection.opened"),
        ] {
            assert!(
                !rebuilds_descriptors(&bare(source, kind)),
                "{source:?} / {kind} must not trigger a descriptor rebuild"
            );
        }
    }

    const PUBLISHED_CATALOG_KINDS: [&str; 13] = [
        "obs.scene.changed",
        "obs.scene.preview_changed",
        "obs.scene.list_changed",
        "obs.scene.created",
        "obs.scene.removed",
        "obs.scene.renamed",
        "obs.source.visibility_changed",
        "obs.source.lock_changed",
        "obs.source.input_created",
        "obs.source.input_removed",
        "obs.source.input_renamed",
        "obs.source.scene_item_created",
        "obs.source.scene_item_removed",
    ];

    #[test]
    fn every_obs_scene_and_source_change_refreshes_the_content_rows() {
        for kind in PUBLISHED_CATALOG_KINDS {
            assert!(
                changes_obs_content(&bare(EventSource::Obs, kind)),
                "{kind} must refresh the OBS content rows"
            );
        }
    }

    #[test]
    fn audio_level_and_non_catalog_events_leave_the_content_rows_alone() {
        for (source, kind) in [
            (EventSource::Obs, "obs.audio.source_volume_changed"),
            (EventSource::Obs, "obs.audio.source_mute_changed"),
            (EventSource::Obs, "obs.audio.source_balance_changed"),
            (EventSource::Obs, "obs.audio.source_sync_offset_changed"),
            (EventSource::Obs, "obs.connection.connected"),
            (EventSource::Obs, "obs.streaming.started"),
            (EventSource::Obs, "obs.scene"),
            (EventSource::Core, "obs.scene.changed"),
            (EventSource::Server, "obs.source.visibility_changed"),
        ] {
            assert!(
                !changes_obs_content(&bare(source, kind)),
                "{source:?} / {kind} must not refresh the OBS content rows"
            );
        }
    }

    #[test]
    fn only_obs_connection_and_name_list_events_change_the_obs_catalog() {
        for (source, kind, expected) in [
            (EventSource::Obs, "obs.connection.connected", true),
            (EventSource::Obs, "obs.connection.disconnected", true),
            (EventSource::Obs, "obs.scene.created", true),
            (EventSource::Obs, "obs.scene.renamed", true),
            (EventSource::Obs, "obs.source.input_removed", true),
            (EventSource::Obs, "obs.source.scene_item_created", true),
            (EventSource::Obs, "obs.scene.changed", false),
            (EventSource::Obs, "obs.recording.started", false),
            (EventSource::Twitch, "obs.scene.created", false),
            (EventSource::VTube, "obs.connection.connected", false),
        ] {
            assert_eq!(
                changes_obs_catalog(&bare(source, kind)),
                expected,
                "{source:?} {kind}"
            );
        }
    }

    #[test]
    fn only_the_vtube_connection_event_changes_the_vtube_catalog() {
        for (source, kind, expected) in [
            (EventSource::VTube, "vtube.connection.changed", true),
            (EventSource::VTube, "vtube.model.loaded", false),
            (EventSource::VTube, "vtube.hotkey.triggered", false),
            (EventSource::Obs, "vtube.connection.changed", false),
        ] {
            assert_eq!(
                changes_vtube_connection(&bare(source, kind)),
                expected,
                "{source:?} {kind}"
            );
        }
    }

    #[test]
    fn every_vtube_disconnect_reason_maps_to_its_own_disconnect() {
        for (reason, expected) in [
            (
                VTubeDisconnectReason::AwaitingApproval,
                VTubeDisconnect::AwaitingApproval,
            ),
            (
                VTubeDisconnectReason::ConnectFailed,
                VTubeDisconnect::Unreachable,
            ),
            (
                VTubeDisconnectReason::AuthRequired,
                VTubeDisconnect::TokenRejected,
            ),
            (VTubeDisconnectReason::AuthDenied, VTubeDisconnect::Denied),
            (
                VTubeDisconnectReason::AuthTimeout,
                VTubeDisconnect::TimedOut,
            ),
            (
                VTubeDisconnectReason::AuthFailed,
                VTubeDisconnect::AuthFailed,
            ),
            (
                VTubeDisconnectReason::SubscribeFailed,
                VTubeDisconnect::SubscribeFailed,
            ),
            (
                VTubeDisconnectReason::SocketClosed,
                VTubeDisconnect::SocketClosed,
            ),
            (
                VTubeDisconnectReason::Unresponsive,
                VTubeDisconnect::Unresponsive,
            ),
        ] {
            assert_eq!(VTubeDisconnect::from(reason), expected, "{reason:?}");
        }
    }

    #[test]
    fn vtube_connection_event_with_an_unknown_reason_word_is_not_decoded() {
        for reason in ["", "AUTH_DENIED", "something_new"] {
            let event = Event::new(
                EventSource::VTube,
                VTubeConnectionChanged::KIND,
                json!({
                    "is_connected": false,
                    "endpoint": "ws://localhost:8001",
                    "reason": reason,
                    "detail": null,
                }),
            );

            assert_eq!(decode(&event), None, "{reason:?}");
        }
    }
}
