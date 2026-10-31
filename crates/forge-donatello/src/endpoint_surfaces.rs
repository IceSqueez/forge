use forge_platform_core::EndpointSurface;

pub const DONATELLO_API: EndpointSurface = EndpointSurface::http(
    "FORGE_DONATELLO_API_BASE_URL",
    "https://donatello.to/api/v1",
);

pub const ENDPOINT_SURFACES: &[EndpointSurface] = &[DONATELLO_API];
