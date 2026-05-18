use forge_events::Event;
use forge_platform_core::oauth::{DeviceCodeResponse, TokenResponse};
use forge_storage::GlobalEntry;
use forge_types::ActionId;
use forge_widgets::{ThemeId, VariantKind};
use time::OffsetDateTime;

use crate::Screen;
use crate::actions::{AddActionMsg, AddSubActionMsg, AddTriggerMsg, RemoveSubActionMsg};
use crate::live_chat::ChatFilter;

#[derive(Debug, Clone)]
pub struct HubStatsData {
    pub actions_count: usize,
    pub commands_count: usize,
    pub triggers_fired: u64,
    pub globals_count: usize,
}

#[derive(Debug, Clone)]
pub enum HubMsg {
    LoadStats,
    StatsLoaded(Result<HubStatsData, String>),
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
}

#[derive(Debug, Clone)]
pub enum OnboardingMsg {
    SkipSetup,
    AdvanceFromWelcome,
    PlatformSelected(String),
    AdvanceFromPicker,
    BackFromPicker,
    SkipPicker,
    EnterDeviceCodeFlow(String),
    DeviceCodeReceived(Result<DeviceCodeResponse, String>),
    TokenReceived(Result<TokenResponse, String>),
    CredentialsStored(Result<(), String>),
    BackFromDeviceCode,
    RetryDeviceCode,
    AdvanceFromObs,
    BackFromObs,
    SkipObs,
    AdvanceFromStarterPack,
    BackFromStarterPack,
    SkipStarterPack,
    FinishOnboarding,
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
    OpenAddActionModal,
    OpenAddTriggerModal(ActionId),
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
}

#[derive(Debug, Clone)]
pub enum SidebarMsg {
    ToggleActionsQueues,
    TogglePlatforms,
    ToggleStreamApps,
}

#[derive(Debug, Clone)]
pub enum Message {
    Navigate(Screen),
    Sidebar(SidebarMsg),
    Onboarding(OnboardingMsg),
    OnboardingPersistResult(Result<(), String>),
    Settings(SettingsMsg),
    Hub(HubMsg),
    Globals(GlobalsMsg),
    VariantEditor(VariantEditorMsg),
    Actions(ActionsMsg),
    AddAction(AddActionMsg),
    AddTrigger(AddTriggerMsg),
    AddSubAction(AddSubActionMsg),
    RemoveSubAction(RemoveSubActionMsg),
    ThemeChanged(ThemeId),
    EventArrived(Event),
    ChatInputChanged(String),
    ChatSubmit,
    ChatSent(Result<(), String>),
    ChatFilterChanged(ChatFilter),
    Noop,
}
