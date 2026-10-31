use forge_platform_core::EndpointSurface;

pub const KICK_PUBLIC_API: EndpointSurface =
    EndpointSurface::http("FORGE_KICK_API_BASE_URL", "https://api.kick.com/public/v1");
pub const KICK_CHANNEL_API: EndpointSurface =
    EndpointSurface::http("FORGE_KICK_CHANNEL_API_BASE_URL", "https://kick.com/api/v2");
pub const KICK_CHAT_SOCKET: EndpointSurface =
    EndpointSurface::web_socket("FORGE_KICK_CHAT_WS_BASE_URL", "wss://ws-us2.pusher.com/app");
pub const KICK_OAUTH: EndpointSurface =
    EndpointSurface::http("FORGE_KICK_OAUTH_BASE_URL", "https://id.kick.com/oauth");

pub const ENDPOINT_SURFACES: &[EndpointSurface] = &[
    KICK_PUBLIC_API,
    KICK_CHANNEL_API,
    KICK_CHAT_SOCKET,
    KICK_OAUTH,
];
