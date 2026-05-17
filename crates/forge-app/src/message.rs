use forge_events::Event;
use forge_platform_core::oauth::{DeviceCodeResponse, TokenResponse};
use forge_widgets::ThemeId;

use crate::Screen;

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
pub enum Message {
    Navigate(Screen),
    Onboarding(OnboardingMsg),
    OnboardingPersistResult(Result<(), String>),
    ThemeChanged(ThemeId),
    BusEvent(Event),
    Noop,
}
