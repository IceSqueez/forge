use forge_discord::vocabulary::event_kinds as discord_kinds;
use forge_events::{
    ActingSender, ActionDone, ActionSkipped, ActionStarted, ChatSendFailed, ChatSent, Event,
    EventSource, GlobalArrayAppended, GlobalArrayRemoved, GlobalDecremented, GlobalDeleted,
    GlobalIncremented, GlobalSet, GlobalToggled, LatestChanged, PlatformConnectionChanged,
    QuickActionDone, RequestFailed, RunOutcome, SENT_AS_FIELD, ScriptFailed, ScriptHttpCall,
    ScriptLogLevel, ScriptLogged, ScriptReloaded, ScriptStarted, SubActionDone, SubActionRun,
    TimerTick, TypedEvent,
};
use forge_hotkey::vocabulary::{HotkeyEnableFailed, HotkeyPressed, event_kinds as hotkey_kinds};
use forge_midi::vocabulary::event_kinds as midi_kinds;
use forge_obs::vocabulary::event_kinds as obs_kinds;
use forge_platform_kick::vocabulary::event_kinds as kick_kinds;
use forge_platform_twitch::vocabulary::{
    BotStatusChanged, PlatformReauthRequired, SenderFallback, event_kinds as twitch_kinds,
};
use forge_platform_youtube::vocabulary::event_kinds as youtube_kinds;
use forge_runtime::{COMMAND_MATCHED_KIND, OVERLAY_SPEECH_FAILED_KIND};
use forge_soundboard::vocabulary::{
    ClipAdoptionSettled, PlaybackFailed, PlaybackFinished, PlaybackStarted,
};
use forge_speak_queue::vocabulary::SpeakFailed;
use forge_types::{ClipId, PlatformId};
use forge_vtube::VTubeError;
use forge_vtube::vocabulary::VTubeConnectionChanged;

use crate::integrations::WHISPER_SENT_KIND;

const COMMAND_FIELD: &str = "command";
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
    CommandMatched { command: Option<String> },
    ChatSent(ChatSent),
    ChatSendFailed(ChatSendFailed),
    WhisperSent { sent_as: Option<ActingSender> },
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
        (_, COMMAND_MATCHED_KIND) => Some(RuntimeEvent::CommandMatched {
            command: event
                .payload
                .get(COMMAND_FIELD)
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
        }),
        (_, ChatSent::KIND) => ChatSent::from_event(event).map(RuntimeEvent::ChatSent),
        (_, ChatSendFailed::KIND) => {
            ChatSendFailed::from_event(event).map(RuntimeEvent::ChatSendFailed)
        }
        (_, WHISPER_SENT_KIND) => Some(RuntimeEvent::WhisperSent {
            sent_as: event
                .payload
                .get(SENT_AS_FIELD)
                .and_then(|sent_as| serde_json::from_value(sent_as.clone()).ok()),
        }),
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
        COMMAND_MATCHED_KIND | ActionStarted::KIND
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
    Unknown,
}

impl VTubeDisconnect {
    fn from_reason(reason: &str) -> Self {
        match reason {
            "awaiting_approval" => Self::AwaitingApproval,
            "connect_failed" => Self::Unreachable,
            "auth_required" => Self::TokenRejected,
            "auth_denied" => Self::Denied,
            "auth_timeout" => Self::TimedOut,
            "auth_failed" => Self::AuthFailed,
            "subscribe_failed" => Self::SubscribeFailed,
            "socket_closed" => Self::SocketClosed,
            "unresponsive" => Self::Unresponsive,
            _ => Self::Unknown,
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
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VTubeConnection {
    Connected,
    Disconnected(VTubeDisconnect),
}

impl VTubeConnection {
    pub(crate) fn of(changed: &VTubeConnectionChanged) -> Option<Self> {
        if changed.is_connected {
            return Some(Self::Connected);
        }
        changed
            .reason
            .as_deref()
            .map(|reason| Self::Disconnected(VTubeDisconnect::from_reason(reason)))
    }
}
