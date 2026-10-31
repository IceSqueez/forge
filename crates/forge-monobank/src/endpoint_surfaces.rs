use forge_platform_core::EndpointSurface;

pub const MONOBANK_API: EndpointSurface =
    EndpointSurface::http("FORGE_MONOBANK_API_BASE_URL", "https://api.monobank.ua");

pub const ENDPOINT_SURFACES: &[EndpointSurface] = &[MONOBANK_API];
