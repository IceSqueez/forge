use forge_platform_core::EndpointSurface;

pub const SEVEN_TV_API: EndpointSurface =
    EndpointSurface::http("FORGE_SEVEN_TV_API_BASE_URL", "https://7tv.io/v3");
pub const BETTER_TTV_API: EndpointSurface = EndpointSurface::http(
    "FORGE_BETTER_TTV_API_BASE_URL",
    "https://api.betterttv.net/3",
);
pub const FRANKER_FACE_Z_API: EndpointSurface = EndpointSurface::http(
    "FORGE_FRANKER_FACE_Z_API_BASE_URL",
    "https://api.frankerfacez.com/v1",
);

pub const ENDPOINT_SURFACES: &[EndpointSurface] =
    &[SEVEN_TV_API, BETTER_TTV_API, FRANKER_FACE_Z_API];
