#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EndpointSurface {
    TwitchApi,
    TwitchEventSubSocket,
    KickPublicApi,
    KickChannelApi,
    KickChatSocket,
    YouTubeDataApi,
    YouTubeUploadApi,
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
    pub const ALL: [EndpointSurface; 7] = [
        Self::TwitchApi,
        Self::TwitchEventSubSocket,
        Self::KickPublicApi,
        Self::KickChannelApi,
        Self::KickChatSocket,
        Self::YouTubeDataApi,
        Self::YouTubeUploadApi,
    ];

    pub const fn env_var(self) -> &'static str {
        match self {
            Self::TwitchApi => "FORGE_TWITCH_API_BASE_URL",
            Self::TwitchEventSubSocket => "FORGE_TWITCH_EVENTSUB_WS_URL",
            Self::KickPublicApi => "FORGE_KICK_API_BASE_URL",
            Self::KickChannelApi => "FORGE_KICK_CHANNEL_API_BASE_URL",
            Self::KickChatSocket => "FORGE_KICK_CHAT_WS_BASE_URL",
            Self::YouTubeDataApi => "FORGE_YOUTUBE_API_BASE_URL",
            Self::YouTubeUploadApi => "FORGE_YOUTUBE_UPLOAD_BASE_URL",
        }
    }

    pub(crate) const fn default_base_url(self) -> &'static str {
        match self {
            Self::TwitchApi => "https://api.twitch.tv",
            Self::TwitchEventSubSocket => "wss://eventsub.wss.twitch.tv/ws",
            Self::KickPublicApi => "https://api.kick.com/public/v1",
            Self::KickChannelApi => "https://kick.com/api/v2",
            Self::KickChatSocket => "wss://ws-us2.pusher.com/app",
            Self::YouTubeDataApi => "https://www.googleapis.com/youtube/v3",
            Self::YouTubeUploadApi => "https://www.googleapis.com/upload/youtube/v3",
        }
    }

    pub(crate) const fn protocol(self) -> EndpointProtocol {
        match self {
            Self::TwitchApi
            | Self::KickPublicApi
            | Self::KickChannelApi
            | Self::YouTubeDataApi
            | Self::YouTubeUploadApi => EndpointProtocol::Http,
            Self::TwitchEventSubSocket | Self::KickChatSocket => EndpointProtocol::WebSocket,
        }
    }
}
