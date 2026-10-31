use forge_platform_core::EndpointSurface;

pub const TWITCH_API: EndpointSurface =
    EndpointSurface::http("FORGE_TWITCH_API_BASE_URL", "https://api.twitch.tv");
pub const TWITCH_EVENTSUB_SOCKET: EndpointSurface = EndpointSurface::web_socket(
    "FORGE_TWITCH_EVENTSUB_WS_URL",
    "wss://eventsub.wss.twitch.tv/ws",
);
pub const TWITCH_OAUTH: EndpointSurface =
    EndpointSurface::http("FORGE_TWITCH_OAUTH_BASE_URL", "https://id.twitch.tv/oauth2");

pub const ENDPOINT_SURFACES: &[EndpointSurface] =
    &[TWITCH_API, TWITCH_EVENTSUB_SOCKET, TWITCH_OAUTH];
