mod error;
pub mod fakes;
pub mod fixture;
pub mod harness;
mod problem;
pub mod vocabulary;

pub use error::{
    EXIT_ALREADY_RUNNING, EXIT_BOOT_FAILED, EXIT_FAILURE, EXIT_FORGE_EXITED, EXIT_GAME_GUARD,
    EXIT_INPUT_INVALID, EXIT_INPUT_SYNTAX, EXIT_INPUT_UNREADABLE, EXIT_LIVE_PATHS, EXIT_NOT_READY,
    EXIT_OVERRIDE_REFUSED, EXIT_PORT_TAKEN, EXIT_SERVER_NOT_STARTED, EmulatorError,
};
pub use problem::Problem;

pub fn endpoint_surfaces() -> Vec<forge_platform_core::EndpointSurface> {
    [
        forge_platform_twitch::endpoint_surfaces::ENDPOINT_SURFACES,
        forge_platform_kick::endpoint_surfaces::ENDPOINT_SURFACES,
        forge_platform_youtube::endpoint_surfaces::ENDPOINT_SURFACES,
        forge_donatello::endpoint_surfaces::ENDPOINT_SURFACES,
        forge_monobank::endpoint_surfaces::ENDPOINT_SURFACES,
        forge_discord::endpoint_surfaces::ENDPOINT_SURFACES,
    ]
    .concat()
}
