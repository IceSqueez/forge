use forge_events::Event;
use forge_platform_core::oauth::{DeviceCodeResponse, TokenResponse};
use forge_types::ActionId;
use forge_widgets::ThemeId;

use crate::Screen;
use crate::actions::{AddActionMsg, AddSubActionMsg, AddTriggerMsg, RemoveSubActionMsg};
use crate::live_chat::ChatFilter;

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

#[derive(Debug, Clone)]
pub enum Message {
    Navigate(Screen),
    Onboarding(OnboardingMsg),
    OnboardingPersistResult(Result<(), String>),
    Settings(SettingsMsg),
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
