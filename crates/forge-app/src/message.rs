use crate::Screen;
use forge_events::Event;
use forge_widgets::ThemeId;

#[derive(Debug, Clone)]
pub enum OnboardingMsg {
    SkipSetup,
    AdvanceFromWelcome,
    PlatformSelected(String),
    AdvanceFromPicker,
    BackFromPicker,
    SkipPicker,
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
