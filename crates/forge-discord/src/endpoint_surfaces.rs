use forge_platform_core::EndpointSurface;

pub const DISCORD_API: EndpointSurface =
    EndpointSurface::http("FORGE_DISCORD_API_BASE_URL", "https://discord.com/api");

pub const ENDPOINT_SURFACES: &[EndpointSurface] = &[DISCORD_API];
