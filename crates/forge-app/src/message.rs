use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use forge_speak_queue::SpeakEvent;
use forge_types::ClipId;

use forge_events::Event;
use forge_obs::ObsClient;
use forge_widgets::{DeviceLabel, PickerItem, ToastKind};

use forge_platform_core::{HeaderAction, HealthDelta};
use forge_storage::GlobalEntry;
use forge_types::ActionId;
use forge_widgets::{ThemeId, VariantKind};
use time::OffsetDateTime;

use crate::Screen;
use crate::actions::{AddActionMsg, AddSubActionMsg, AddTriggerMsg, RemoveSubActionMsg};
use crate::event_feed::EventFeedMsg;
use crate::live_chat::PlatformFilter;
use crate::queues_view::QueueSummary;
use crate::script_editor::ScriptEditorMsg;
use crate::server_screen::ServerScreenMsg;
use crate::settings_websocket::SettingsWebSocketMsg;

#[derive(Debug, Clone)]
pub struct TwitchBootBundle {
    pub access_token: String,
    pub client_id: String,
    pub user_id: String,
    pub login: String,
    pub expires_at: Option<std::time::SystemTime>,
}

pub struct ObsClientRef(pub(crate) Arc<ObsClient>);

impl ObsClientRef {
    pub fn new(client: Arc<ObsClient>) -> Self {
        Self(client)
    }

    pub(crate) fn into_arc(self) -> Arc<ObsClient> {
        self.0
    }
}

impl std::fmt::Debug for ObsClientRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ObsClientRef")
            .field(&self.0.endpoint())
            .finish()
    }
}

impl Clone for ObsClientRef {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

#[derive(Debug, Clone)]
pub struct HomeStatsData {
    pub actions_count: usize,
    pub commands_count: usize,
    pub triggers_fired: u64,
    pub globals_count: usize,
}

#[derive(Debug, Clone)]
pub enum HomeMsg {
    LoadStats,
    StatsLoaded(Result<HomeStatsData, String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformId {
    Twitch,
    YouTube,
    Kick,
    Trovo,
}

#[derive(Debug, Clone)]
pub enum SettingsMsg {
    ReconnectPlatform(PlatformId),
    PlatformReconnectResult(Result<(), String>),
    DbVacuumRequested,
    DbVacuumDone(Result<u64, String>),
    DbBackupRequested,
    DbBackupDone(Result<String, String>),
    OpenLogDirectoryRequested,
    OpenLogDirectoryResult(Result<(), String>),
}

#[derive(Debug, Clone)]
pub enum ActionsMsg {
    LoadRequested,
    TreeLoaded(Result<Vec<crate::actions::ActionsGroup>, String>),
    ActionSelected(ActionId),
    DetailLoaded(Result<crate::actions::ActionDetail, String>),
    ToggleEnabled(ActionId, bool),
    EnabledToggled(Result<(), String>),
    TestTrigger(ActionId),
    DeleteAction(ActionId),
    ActionDeleted(Result<(), String>),
    DuplicateAction(ActionId),
    ActionDuplicated(Result<ActionId, String>),
    DeleteTrigger(forge_types::TriggerId, ActionId),
    TriggerDeleted(Result<ActionId, String>),
    OpenAddActionModal,
    OpenAddTriggerModal(ActionId),
    SearchChanged(String),
    FilterChanged(crate::actions::ActionsFilter),
    ToggleGroupCollapsed(crate::actions::TriggerCategory),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorMode {
    Create,
    Edit(String),
}

#[derive(Debug, Clone)]
pub enum VariantEditorMsg {
    OpenCreate,
    OpenEdit(String, GlobalEntry),
    NameChanged(String),
    KindSelected(VariantKind),
    PersistenceToggled(bool),
    IntInputChanged(String),
    FloatInputChanged(String),
    BoolValueChanged(bool),
    StringInputChanged(String),
    DatetimeInputChanged(String),
    ArrayJsonChanged(String),
    ObjectJsonChanged(String),
    Cancel,
    Submit,
    Saved(Result<(), String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalsFilter {
    All,
    Persisted,
    Session,
}

#[derive(Debug, Clone)]
pub struct GlobalsLoadData {
    pub entries: Vec<GlobalEntry>,
    pub storage_bytes: u64,
    pub last_save: Option<OffsetDateTime>,
}

#[derive(Debug, Clone)]
pub enum GlobalsMsg {
    LoadRequested,
    EntriesLoaded(Result<GlobalsLoadData, String>),
    FilterSelected(GlobalsFilter),
    SearchChanged(String),
    TogglePersistence(String, bool),
    PersistenceToggled(Result<(), String>),
    OpenCreateModal,
    OpenEditModal(String),
    DeleteRequested(String),
    Deleted(Result<(), String>),
    ExportRequested,
    Exported(Result<PathBuf, String>),
}

#[derive(Debug, Clone)]
pub enum IntegrationDetailMsg {
    HealthDelta(HealthDelta),
    HeaderActionClicked(HeaderAction),
    QuickActionClicked(usize),
    PickerSearchChanged(String),
    PickerItemsLoaded(Result<(Vec<PickerItem>, Option<String>), String>),
    PickerItemSelected(usize),
    PickerCancelled,
    DismissToast,
}

#[derive(Debug, Clone)]
pub enum QueuesMsg {
    LoadRequested,
    QueuesLoaded(Result<Vec<QueueSummary>, String>),
    PauseQueue(forge_types::QueueId),
    ResumeQueue(forge_types::QueueId),
    DrainQueue(forge_types::QueueId),
    PauseAll,
    NewQueue,
    PauseResult(Result<(), String>),
    ResumeResult(Result<(), String>),
}

#[derive(Debug, Clone)]
pub enum SidebarMsg {
    ToggleActionsQueues,
    TogglePlatforms,
    ToggleStreamApps,
}

#[derive(Debug, Clone)]
pub enum SettingsAudioMsg {
    LoadRequested,
    DevicesLoaded(Result<Vec<DeviceLabel>, String>),
    RefreshDevices,
    DeviceSelected(usize),
    TestToneRequested,
    TestToneResult(Result<(), String>),
}

#[derive(Debug, Clone)]
pub enum SoundboardMsg {
    LoadRequested,
    ClipsLoaded(Result<Vec<forge_storage::StoredClip>, String>),
    OpenAddModal,
    OpenEditModal(ClipId),
    ModalDevicesLoaded(Result<Vec<DeviceLabel>, String>),
    ModalFilePickRequested,
    ModalFilePicked(Option<std::path::PathBuf>),
    ModalNameChanged(String),
    ModalHotkeyChanged(String),
    ModalDeviceSelected(usize),
    ModalVolumeChanged(f32),
    ModalSave,
    ModalSaved(Result<(), String>),
    ModalCancel,
    PlayClip(ClipId),
    PlayResult(Result<(), String>),
    DeleteClip(ClipId),
    ClipDeleted(Result<(), String>),
    HotkeyPressed(String),
}

#[derive(Debug, Clone)]
pub enum TtsDashMsg {
    SpeakEventReceived(SpeakEvent),
    PauseQueue,
    SkipCurrent,
    StopAll,
    VolumeChanged(f32),
    TestInputChanged(String),
    SpeakTest,
    CommandResult(Result<(), String>),
}

#[derive(Debug, Clone)]
pub enum TtsEnginesMsg {
    SelectEngine(String),
}

#[derive(Debug, Clone)]
pub enum VoiceAliasesMsg {
    SearchChanged(String),
    StrategyChanged(crate::voice_aliases::AssignmentStrategyChoice),
}

#[derive(Debug, Clone)]
pub enum TtsFiltersMsg {
    PreviewInputChanged(String),
    BlocklistModeChanged(crate::tts_filters::BlocklistModeChoice),
    AddRuleClicked,
}

#[derive(Debug, Clone)]
pub enum ToastMsg {
    Fired {
        kind: ToastKind,
        message: String,
        duration_ms: u64,
    },
    Dismissed(u64),
    Tick(Instant),
}

#[derive(Debug, Clone)]
pub enum TtsTriggersMsg {
    CommandEnabledToggled(bool),
    ChannelPointsEnabledToggled(bool),
    BitsEnabledToggled(bool),
    SubMessagesEnabledToggled(bool),
    ReadUsernameToggled(bool),
    SpeakEmotesToggled(bool),
    BitsSkipLineToggled(bool),
}

#[derive(Debug, Clone)]
pub enum TtsMsg {
    Dashboard(TtsDashMsg),
    Engines(TtsEnginesMsg),
    Aliases(VoiceAliasesMsg),
    Filters(TtsFiltersMsg),
    Triggers(TtsTriggersMsg),
}

#[derive(Debug, Clone)]
pub enum Message {
    Navigate(Screen),
    Toast(ToastMsg),
    Sidebar(SidebarMsg),
    Settings(SettingsMsg),
    Home(HomeMsg),
    Globals(GlobalsMsg),
    VariantEditor(VariantEditorMsg),
    Actions(ActionsMsg),
    Queues(QueuesMsg),
    Viewers(crate::viewers::ViewersMsg),
    AddAction(AddActionMsg),
    AddTrigger(AddTriggerMsg),
    AddSubAction(AddSubActionMsg),
    RemoveSubAction(RemoveSubActionMsg),
    IntegrationDetail(IntegrationDetailMsg),
    ObsBootResult(Result<ObsClientRef, String>),
    TwitchBootResult(Result<Option<TwitchBootBundle>, String>),
    ServerBootResult(Result<crate::server_subsystem::ServerBootSnapshot, String>),
    ServerRestartResult(Result<(), String>),
    ServerStopResult(Result<(), String>),
    ServerTokenRotated(Result<String, String>),
    ThemeChanged(ThemeId),
    EventArrived(Event),
    ChatInputChanged(String),
    ChatSubmit,
    ChatSent(Result<(), String>),
    ChatPlatformFilter(PlatformFilter),
    ChatToggleEventsOnly,
    ChatToggleHideBots,
    ChatToggleDrawer,
    ScriptEditor(ScriptEditorMsg),
    EventFeed(EventFeedMsg),
    Server(ServerScreenMsg),
    SettingsWebSocket(SettingsWebSocketMsg),
    TwitchPanel(crate::twitch_panel::TwitchPanelMsg),
    TwitchReauthRequested,
    ObsPanel(crate::obs_panel::ObsPanelMsg),
    Soundboard(SoundboardMsg),
    SettingsAudio(SettingsAudioMsg),
    Tts(TtsMsg),
    Noop,
}
