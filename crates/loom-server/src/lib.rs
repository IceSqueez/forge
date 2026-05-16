#![doc = "Versioned WebSocket + HTTP server for overlays and remote control. /ws/v1/..."]

pub mod config;
pub mod error;
pub mod handle;
pub mod server;

pub use config::ServerConfig;
pub use error::ServerError;
pub use handle::ServerHandle;
pub use server::Server;
