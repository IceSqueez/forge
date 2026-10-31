use forge_platform_core::EndpointSurface;

pub const YOUTUBE_DATA_API: EndpointSurface = EndpointSurface::http(
    "FORGE_YOUTUBE_API_BASE_URL",
    "https://www.googleapis.com/youtube/v3",
);
pub const YOUTUBE_UPLOAD_API: EndpointSurface = EndpointSurface::http(
    "FORGE_YOUTUBE_UPLOAD_BASE_URL",
    "https://www.googleapis.com/upload/youtube/v3",
);
pub const YOUTUBE_OAUTH: EndpointSurface = EndpointSurface::http(
    "FORGE_YOUTUBE_OAUTH_BASE_URL",
    "https://oauth2.googleapis.com",
);

pub const ENDPOINT_SURFACES: &[EndpointSurface] =
    &[YOUTUBE_DATA_API, YOUTUBE_UPLOAD_API, YOUTUBE_OAUTH];
