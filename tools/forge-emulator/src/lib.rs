mod error;
pub mod fakes;
pub mod fixture;
pub mod harness;
mod problem;
pub mod vocabulary;

pub use error::{EXIT_INPUT_INVALID, EXIT_INPUT_SYNTAX, EXIT_INPUT_UNREADABLE, EmulatorError};
pub(crate) use problem::Problem;

pub fn endpoint_surfaces() -> Vec<forge_platform_core::EndpointSurface> {
    [
        forge_platform_twitch::endpoint_surfaces::ENDPOINT_SURFACES,
        forge_platform_kick::endpoint_surfaces::ENDPOINT_SURFACES,
        forge_platform_youtube::endpoint_surfaces::ENDPOINT_SURFACES,
        forge_donatello::endpoint_surfaces::ENDPOINT_SURFACES,
        forge_monobank::endpoint_surfaces::ENDPOINT_SURFACES,
        forge_discord::endpoint_surfaces::ENDPOINT_SURFACES,
        forge_emotes::endpoint_surfaces::ENDPOINT_SURFACES,
    ]
    .concat()
}
