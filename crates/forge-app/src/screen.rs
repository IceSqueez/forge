#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnboardingStep {
    Welcome,
    ConnectPlatform,
    DeviceCodeFlow(String),
    ConnectObs,
    StarterPack,
    Ready,
}

impl OnboardingStep {
    pub fn as_key(&self) -> &'static str {
        match self {
            Self::Welcome => "welcome",
            Self::ConnectPlatform => "connect_platform",
            Self::DeviceCodeFlow(_) => "device_code_flow",
            Self::ConnectObs => "connect_obs",
            Self::StarterPack => "starter_pack",
            Self::Ready => "ready",
        }
    }

    pub fn from_key(s: &str) -> Option<Self> {
        match s {
            "welcome" => Some(Self::Welcome),
            "connect_platform" => Some(Self::ConnectPlatform),
            // device code sessions are volatile; resume at picker so user gets a fresh code
            "device_code_flow" => Some(Self::ConnectPlatform),
            "connect_obs" => Some(Self::ConnectObs),
            "starter_pack" => Some(Self::StarterPack),
            "ready" => Some(Self::Ready),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::OnboardingStep;

    #[test]
    fn as_key_roundtrips_welcome() {
        assert_eq!(
            OnboardingStep::from_key(OnboardingStep::Welcome.as_key()),
            Some(OnboardingStep::Welcome)
        );
    }

    #[test]
    fn as_key_roundtrips_connect_platform() {
        assert_eq!(
            OnboardingStep::from_key(OnboardingStep::ConnectPlatform.as_key()),
            Some(OnboardingStep::ConnectPlatform)
        );
    }

    #[test]
    fn device_code_flow_as_key_returns_device_code_flow_string() {
        assert_eq!(
            OnboardingStep::DeviceCodeFlow("twitch".into()).as_key(),
            "device_code_flow"
        );
    }

    #[test]
    fn device_code_flow_from_key_downgrades_to_connect_platform() {
        assert_eq!(
            OnboardingStep::from_key("device_code_flow"),
            Some(OnboardingStep::ConnectPlatform)
        );
    }

    #[test]
    fn as_key_roundtrips_connect_obs() {
        assert_eq!(
            OnboardingStep::from_key(OnboardingStep::ConnectObs.as_key()),
            Some(OnboardingStep::ConnectObs)
        );
    }

    #[test]
    fn as_key_roundtrips_starter_pack() {
        assert_eq!(
            OnboardingStep::from_key(OnboardingStep::StarterPack.as_key()),
            Some(OnboardingStep::StarterPack)
        );
    }

    #[test]
    fn as_key_roundtrips_ready() {
        assert_eq!(
            OnboardingStep::from_key(OnboardingStep::Ready.as_key()),
            Some(OnboardingStep::Ready)
        );
    }

    #[test]
    fn from_key_unknown_returns_none() {
        assert_eq!(OnboardingStep::from_key("unknown_step"), None);
    }

    #[test]
    fn from_key_empty_returns_none() {
        assert_eq!(OnboardingStep::from_key(""), None);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsSection {
    Appearance,
    Language,
    Shortcuts,
    Notifications,
    Platforms,
    Scripting,
    Queues,
    Storage,
    WebSocket,
    Version,
    Diagnostics,
}

use forge_platform_core::IntegrationId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    Onboarding(OnboardingStep),
    Home,
    LiveChat,
    EventFeed,
    Viewers,
    Globals,
    Actions,
    Commands,
    Platforms,
    StreamApps,
    Integrations,
    IntegrationDetail(IntegrationId),
    Tts,
    Soundboard,
    ScriptEditor,
    Server,
    Logs,
    Settings(SettingsSection),
}
