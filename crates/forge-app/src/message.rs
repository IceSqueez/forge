use crate::Screen;
use forge_events::Event;
use forge_widgets::ThemeId;

#[derive(Debug, Clone)]
pub enum OnboardingMsg {
    SkipSetup,
    AdvanceFromWelcome,
}

#[derive(Debug, Clone)]
pub enum Message {
    Navigate(Screen),
    Onboarding(OnboardingMsg),
    ThemeChanged(ThemeId),
    BusEvent(Event),
    Noop,
}
