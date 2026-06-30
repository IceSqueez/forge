use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use forge_speak_queue::SpeakEvent;
use forge_types::ClipId;

use forge_discord::DiscordClient;
use forge_events::Event;
use forge_hotkey::HotkeyClient;
use forge_midi::MidiClient;
use forge_obs::ObsClient;
use forge_platform_kick::KickIntegrationBundle;
use forge_vtube::VTubeClient;
use forge_widgets::{DeviceLabel, PickerItem, ToastKind};

use forge_platform_core::{HeaderAction, HealthDelta};
use forge_storage::settings::Density;
use forge_storage::{GlobalEntry, Language};
use forge_types::{ActionId, OAuthToken, PlatformId, UnifiedChatRow};
use forge_widgets::{ThemeId, VariantKind};
use time::OffsetDateTime;

use crate::Screen;
use crate::actions::{AddActionMsg, AddSubActionMsg, RemoveSubActionMsg};
use crate::event_feed::EventFeedMsg;
use crate::live_chat::{EventsFilter, PlatformFilter, SendId};
use crate::local_callback_flow::LocalCallbackFlowMsg;
use crate::queues_view::QueueSummary;
use crate::script_editor::ScriptEditorMsg;
use crate::server_screen::ServerScreenMsg;
use crate::settings_websocket::SettingsWebSocketMsg;

#[derive(Debug, Clone)]
pub struct TwitchBootBundle {
    pub access_token: OAuthToken,
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

pub struct KickBundleRef(pub(crate) Arc<KickIntegrationBundle>);

impl KickBundleRef {
    pub fn new(bundle: Arc<KickIntegrationBundle>) -> Self {
        Self(bundle)
    }

    pub(crate) fn into_arc(self) -> Arc<KickIntegrationBundle> {
        self.0
    }
}

impl std::fmt::Debug for KickBundleRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("KickBundleRef").finish()
    }
}

impl Clone for KickBundleRef {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

pub struct DiscordClientRef(pub(crate) Arc<DiscordClient>);

impl DiscordClientRef {
    pub fn new(client: Arc<DiscordClient>) -> Self {
        Self(client)
    }

    pub(crate) fn into_arc(self) -> Arc<DiscordClient> {
        self.0
    }
}

impl std::fmt::Debug for DiscordClientRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("DiscordClientRef").finish()
    }
}

impl Clone for DiscordClientRef {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

pub struct MidiClientRef(pub(crate) Arc<MidiClient>);

impl MidiClientRef {
    pub fn new(client: Arc<MidiClient>) -> Self {
        Self(client)
    }

    pub(crate) fn into_arc(self) -> Arc<MidiClient> {
        self.0
    }
}

impl std::fmt::Debug for MidiClientRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("MidiClientRef").finish()
    }
}

impl Clone for MidiClientRef {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

pub struct HotkeyClientRef(pub(crate) Arc<HotkeyClient>);

impl HotkeyClientRef {
    pub fn new(client: Arc<HotkeyClient>) -> Self {
        Self(client)
    }

    pub(crate) fn into_arc(self) -> Arc<HotkeyClient> {
        self.0
    }
}

impl std::fmt::Debug for HotkeyClientRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("HotkeyClientRef").finish()
    }
}

impl Clone for HotkeyClientRef {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

pub struct VTubeClientRef(pub(crate) Arc<VTubeClient>);

impl VTubeClientRef {
    pub fn new(client: Arc<VTubeClient>) -> Self {
        Self(client)
    }

    pub(crate) fn into_arc(self) -> Arc<VTubeClient> {
        self.0
    }
}

impl std::fmt::Debug for VTubeClientRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("VTubeClientRef")
            .field(&self.0.connection_state())
            .finish()
    }
}

impl Clone for VTubeClientRef {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

pub use forge_runtime::dashboard::DashboardStats as HomeStatsData;

#[derive(Debug, Clone)]
pub enum HomeMsg {
    LoadStats,
    StatsLoaded(Result<HomeStatsData, String>),
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
    Scripting(crate::settings_scripting::ScriptingSettingsMsg),
    Shortcuts(crate::settings_shortcuts::ShortcutsMsg),
    LanguageChanged(Language),
    LanguagePersisted(Result<(), String>),
    DensityChanged(Density),
    DensityPersisted(Result<(), String>),
    FontCatalogLoaded(Vec<forge_widgets::FontFamily>),
    FontChanged(forge_widgets::FontRole, Option<String>),
    FontPersisted(Result<(), String>),
    FontMonoShowAllToggled(bool),
}

#[derive(Debug, Clone)]
pub enum ActionsMsg {
    LoadRequested,
    SummariesLoaded(Result<Vec<crate::actions::ActionSummary>, String>),
    ActionSelected(ActionId),
    DetailLoaded(Result<crate::actions::ActionDetail, String>),
    ToggleEnabled(ActionId, bool),
    EnabledToggled(Result<(), String>),
    TestTrigger(ActionId),
    DeleteAction(ActionId),
    ActionDeleted(Result<(), String>),
    DuplicateAction(ActionId),
    ActionDuplicated(Result<ActionId, String>),
    RemoveTriggerInstance(ActionId, forge_types::TriggerInstanceId),
    TriggerInstanceRemoved(Result<ActionId, String>),
    TriggerChipClicked(forge_types::TriggerInstanceId),
    OpenAddActionModal,
    OpenTriggerPicker(ActionId),
    TriggerPickerMsg(crate::actions_trigger_picker::TriggerPickerMsg),
    TriggerInstanceAssigned(Result<ActionId, String>),
    SearchChanged(String),
    FilterChanged(crate::actions::ActionsFilter),
    ToggleGroupCollapsed(crate::actions::TriggerCategory),
    TelemetryLoaded(Result<forge_storage::ActionTelemetry, String>),
    ToggleStepMenu(usize),
    ToggleActionMenu(forge_types::ActionId),
    DismissActionMenu,
    RenameStarted(forge_types::ActionId),
    RenameBufferChanged(String),
    RenameSubmit,
    RenameCancel,
    RenameSaved(Result<(forge_types::ActionId, String), String>),
    DismissStepMenu,
    /// Descend into a composite step's nested sub-chain (pushes a nav frame).
    EnterBranch {
        step_index: usize,
        chain_key: String,
        case_index: Option<usize>,
    },
    /// Pop the nav path back to the given depth (0 = the action's own steps).
    BreadcrumbPop(usize),
    /// Reload the editor detail after a nested-chain mutation, preserving the
    /// current nav path.
    BranchReload(ActionId),
    /// Append an empty case row to a switch step's case list.
    AddSwitchCase(usize),
    /// Remove a switch case row by (step index, case index).
    RemoveSwitchCase(usize, usize),
    /// Reorder a switch case row up (`true`) or down (`false`).
    MoveSwitchCase(usize, usize, bool),
    /// Buffer an edit to a switch case's single-value match input.
    SwitchCaseMatchChanged(usize, usize, String),
    /// Persist the buffered switch case match value.
    SwitchCaseMatchCommitted(usize, usize),
    Editor(ActionEditorMsg),
}

#[derive(Debug, Clone)]
pub enum MoveSubActionMsg {
    Up(ActionId, usize),
    Down(ActionId, usize),
    ToTop(ActionId, usize),
    ToBottom(ActionId, usize),
    Moved(Result<ActionId, String>),
}

#[derive(Debug, Clone)]
pub enum ActionEditorMsg {
    AddAction(AddActionMsg),
    AddSubAction(AddSubActionMsg),
    RemoveSubAction(RemoveSubActionMsg),
    MoveSubAction(MoveSubActionMsg),
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
    VariantEditor(VariantEditorMsg),
}

#[derive(Debug, Clone)]
pub enum BuiltinDetailMsg {
    HealthDelta(HealthDelta),
    HeaderActionClicked(HeaderAction),
    ControlResult(Result<(), String>),
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
    NewQueueNameChanged(String),
    NewQueueBlockingToggled,
    NewQueueSubmit,
    NewQueueSubmitResult(Result<(), String>),
    RegisterResult(
        forge_types::QueueId,
        Result<forge_runtime::MembershipOutcome, String>,
    ),
    NewQueueCancel,
    ConfigureQueue(forge_types::QueueId, String, bool),
    EditQueueSubmit,
    EditQueueSubmitResult(Result<(), String>),
    ReconfigureResult(
        forge_types::QueueId,
        Result<forge_runtime::MembershipOutcome, String>,
    ),
    EditQueueCancel,
    PauseResult(Result<(), String>),
    ResumeResult(Result<(), String>),
}

#[derive(Debug, Clone)]
pub enum SidebarMsg {
    ToggleActionsQueues,
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
    LoadRequested,
    AliasesLoaded(Result<Vec<crate::voice_aliases::VoiceAliasRow>, String>),
    PlayPreview(usize),
    Assign,
    Edit(usize),
    FormViewerChanged(String),
    FormEngineChanged(String),
    FormVoiceChanged(String),
    FormPitchChanged(String),
    FormRateChanged(String),
    FormCancel,
    FormSubmit,
    FormSubmitResult(Result<(), String>),
    DeleteRequested(usize),
    DeleteConfirm,
    DeleteCancel,
    DeleteResult(Result<(), String>),
}

#[derive(Debug, Clone)]
pub enum TtsFiltersMsg {
    LoadRequested,
    Loaded(
        Result<
            (
                Vec<forge_storage::FilterRule>,
                forge_storage::TtsPipelineSettings,
            ),
            String,
        >,
    ),
    PreviewInputChanged(String),
    AddRuleClicked,
    EditRule(usize),
    DeleteRule(usize),
    ToggleRule(usize),
    MoveRuleUp(usize),
    MoveRuleDown(usize),
    DraftKindChanged(crate::tts_filters::DraftKind),
    DraftNameChanged(String),
    DraftPatternChanged(String),
    DraftReplacementChanged(String),
    DraftWordsChanged(String),
    DraftBlocklistModeChanged(forge_storage::BlocklistMode),
    DraftSubmit,
    DraftCancel,
    UrlModeChanged(forge_storage::UrlMode),
    MaxLengthChanged(String),
    StripTwitchEmotesToggled(bool),
    StripRewardEmotesToggled(bool),
    SettingsBlocklistModeChanged(forge_storage::BlocklistMode),
    Save,
    SaveResult(Result<(), String>),
    SpeakPreview,
    SpeakPreviewResult(Result<(), String>),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudEngineKind {
    Azure,
    ElevenLabs,
    OpenAI,
    Polly,
}

impl CloudEngineKind {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Azure => "Azure Speech",
            Self::ElevenLabs => "ElevenLabs",
            Self::OpenAI => "OpenAI TTS",
            Self::Polly => "Amazon Polly",
        }
    }
}

#[derive(Debug, Clone)]
pub enum CloudTtsEnginesMsg {
    ApiKeyChanged(CloudEngineKind, String),
    RegionChanged(CloudEngineKind, String),
    PollySecretKeyChanged(String),
    SavePressed(CloudEngineKind),
    Saved(CloudEngineKind, Result<(), String>),
    TestPressed(CloudEngineKind),
    Tested(CloudEngineKind, Result<(), String>),
}

#[derive(Debug, Clone)]
pub enum TtsMsg {
    Dashboard(TtsDashMsg),
    Engines(TtsEnginesMsg),
    Aliases(VoiceAliasesMsg),
    Filters(TtsFiltersMsg),
    Triggers(TtsTriggersMsg),
    CloudEngines(CloudTtsEnginesMsg),
}

#[derive(Debug, Clone)]
pub enum LiveChatMsg {
    RowReceived(UnifiedChatRow),
    PlatformFilterChanged(PlatformFilter),
    EventsFilterToggled(EventsFilter),
    HideBotsToggled,
    SearchChanged(String),
    AutoScrollToggled,
    InputChanged(String),
    CrossPostToggled,
    PrimarySendTargetChanged(PlatformId),
    SecondarySendTargetToggled(PlatformId),
    SendPressed,
    SendCompleted(SendId, Result<(), String>),
    ConnectedPlatformsUpdated(Vec<PlatformId>),
    ToggleDrawer,
    DrawerSearchChanged(String),
    DrawerSelectViewer(String),
    DrawerMenuToggle,
    DrawerMenuDismiss,
    Scrolled(iced::widget::scrollable::Viewport),
    ScrollToBottom,
    ToggleEmoji,
    LoadDrawerWidth,
    DrawerWidthLoaded(Option<f32>),
    SheetResized(f32),
    ShoutoutViewer,
    WhisperOpen,
    WhisperMessageChanged(String),
    WhisperSend,
    WhisperCancel,
}

#[derive(Debug, Clone)]
pub enum BootMsg {
    Obs(Result<ObsClientRef, String>),
    Vtube(Result<VTubeClientRef, String>),
    Discord(Result<DiscordClientRef, String>),
    Midi(Result<MidiClientRef, String>),
    Hotkey(Result<HotkeyClientRef, String>),
    Twitch(Result<Option<TwitchBootBundle>, String>),
    Kick(Result<KickBundleRef, String>),
    Server(Result<crate::server_subsystem::ServerBootSnapshot, String>),
}

#[derive(Debug, Clone)]
pub enum ServerSubsystemMsg {
    RestartResult(Result<(), String>),
    StopResult(Result<(), String>),
    TokenRotated(Result<String, String>),
}

#[derive(Debug, Clone)]
pub enum Message {
    Navigate(Screen),
    Toast(ToastMsg),
    Sidebar(SidebarMsg),
    Settings(SettingsMsg),
    Home(HomeMsg),
    Globals(GlobalsMsg),
    Actions(ActionsMsg),
    Queues(QueuesMsg),
    Viewers(crate::viewers::ViewersMsg),
    TriggersRegistry(crate::triggers_registry::TriggersRegistryMsg),
    BuiltinDetail(BuiltinDetailMsg),
    Boot(BootMsg),
    ServerSubsystem(ServerSubsystemMsg),
    ThemeChanged(ThemeId),
    EventArrived(Arc<Event>),
    LiveChat(LiveChatMsg),
    ScriptEditor(ScriptEditorMsg),
    EventFeed(EventFeedMsg),
    Server(ServerScreenMsg),
    SettingsWebSocket(SettingsWebSocketMsg),
    TwitchPanel(crate::twitch_panel::TwitchPanelMsg),
    ObsPanel(crate::obs_panel::ObsPanelMsg),
    Soundboard(SoundboardMsg),
    SettingsAudio(SettingsAudioMsg),
    Tts(TtsMsg),
    LocalCallbackFlow(LocalCallbackFlowMsg),
    SettingsHotkeys(crate::settings_hotkeys::SettingsHotkeysMsg),
    OutsideClick,
    Noop,
    Lifecycle(LifecycleMsg),
}

#[derive(Debug, Clone)]
pub enum LifecycleMsg {
    CloseRequested,
    ShutdownComplete,
}
