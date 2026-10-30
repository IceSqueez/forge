use forge_types::Secret;

pub struct OAuthClientIdentity {
    pub client_id: String,
    pub client_secret: Secret,
}

impl From<(String, String)> for OAuthClientIdentity {
    fn from((client_id, client_secret): (String, String)) -> Self {
        Self {
            client_id,
            client_secret: Secret::new(client_secret),
        }
    }
}

#[derive(Default)]
pub struct PlatformIdentities {
    pub twitch_client_id: Option<String>,
    pub youtube: Option<OAuthClientIdentity>,
    pub kick: Option<OAuthClientIdentity>,
}

impl PlatformIdentities {
    pub fn from_environment() -> Self {
        Self {
            twitch_client_id: forge_platform_twitch::client_id(),
            youtube: forge_platform_youtube::client_credentials().map(Into::into),
            kick: forge_platform_kick::client_credentials().map(Into::into),
        }
    }
}
