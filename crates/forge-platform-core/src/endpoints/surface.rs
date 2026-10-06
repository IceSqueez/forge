#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EndpointSurface {
    TwitchApi,
    TwitchEventSubSocket,
    TwitchOAuth,
    KickPublicApi,
    KickChannelApi,
    KickChatSocket,
    KickOAuth,
    YouTubeDataApi,
    YouTubeUploadApi,
    YouTubeOAuth,
    DonatelloApi,
    MonobankApi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EndpointProtocol {
    Http,
    WebSocket,
}

impl EndpointProtocol {
    pub(crate) fn accepts_scheme(self, scheme: &str) -> bool {
        match self {
            Self::Http => matches!(scheme, "http" | "https"),
            Self::WebSocket => matches!(scheme, "ws" | "wss"),
        }
    }
}

impl EndpointSurface {
    pub const ALL: [EndpointSurface; 12] = [
        Self::TwitchApi,
        Self::TwitchEventSubSocket,
        Self::TwitchOAuth,
        Self::KickPublicApi,
        Self::KickChannelApi,
        Self::KickChatSocket,
        Self::KickOAuth,
        Self::YouTubeDataApi,
        Self::YouTubeUploadApi,
        Self::YouTubeOAuth,
        Self::DonatelloApi,
        Self::MonobankApi,
    ];

    pub const fn env_var(self) -> &'static str {
        match self {
            Self::TwitchApi => "FORGE_TWITCH_API_BASE_URL",
            Self::TwitchEventSubSocket => "FORGE_TWITCH_EVENTSUB_WS_URL",
            Self::TwitchOAuth => "FORGE_TWITCH_OAUTH_BASE_URL",
            Self::KickPublicApi => "FORGE_KICK_API_BASE_URL",
            Self::KickChannelApi => "FORGE_KICK_CHANNEL_API_BASE_URL",
            Self::KickChatSocket => "FORGE_KICK_CHAT_WS_BASE_URL",
            Self::KickOAuth => "FORGE_KICK_OAUTH_BASE_URL",
            Self::YouTubeDataApi => "FORGE_YOUTUBE_API_BASE_URL",
            Self::YouTubeUploadApi => "FORGE_YOUTUBE_UPLOAD_BASE_URL",
            Self::YouTubeOAuth => "FORGE_YOUTUBE_OAUTH_BASE_URL",
            Self::DonatelloApi => "FORGE_DONATELLO_API_BASE_URL",
            Self::MonobankApi => "FORGE_MONOBANK_API_BASE_URL",
        }
    }

    pub(crate) const fn default_base_url(self) -> &'static str {
        match self {
            Self::TwitchApi => "https://api.twitch.tv",
            Self::TwitchEventSubSocket => "wss://eventsub.wss.twitch.tv/ws",
            Self::TwitchOAuth => "https://id.twitch.tv/oauth2",
            Self::KickPublicApi => "https://api.kick.com/public/v1",
            Self::KickChannelApi => "https://kick.com/api/v2",
            Self::KickChatSocket => "wss://ws-us2.pusher.com/app",
            Self::KickOAuth => "https://id.kick.com/oauth",
            Self::YouTubeDataApi => "https://www.googleapis.com/youtube/v3",
            Self::YouTubeUploadApi => "https://www.googleapis.com/upload/youtube/v3",
            Self::YouTubeOAuth => "https://oauth2.googleapis.com",
            Self::DonatelloApi => "https://donatello.to/api/v1",
            Self::MonobankApi => "https://api.monobank.ua",
        }
    }

    pub(crate) const fn protocol(self) -> EndpointProtocol {
        match self {
            Self::TwitchApi
            | Self::TwitchOAuth
            | Self::KickPublicApi
            | Self::KickChannelApi
            | Self::KickOAuth
            | Self::YouTubeDataApi
            | Self::YouTubeUploadApi
            | Self::YouTubeOAuth
            | Self::DonatelloApi
            | Self::MonobankApi => EndpointProtocol::Http,
            Self::TwitchEventSubSocket | Self::KickChatSocket => EndpointProtocol::WebSocket,
        }
    }
}
