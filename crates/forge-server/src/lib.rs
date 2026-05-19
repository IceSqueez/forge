#![doc = "Versioned WebSocket + HTTP server for overlays and remote control. /ws/v1/..."]

mod routes;

pub mod auth;
pub mod bus_adapter;
pub mod config;
pub mod error;
pub mod handle;
pub mod server;
pub mod ws_client;

pub use auth::AuthState;
pub use config::ServerConfig;
pub use error::ServerError;
pub use handle::ServerHandle;
pub use server::{Server, start_server};
pub use ws_client::{ClientType, WsClient, detect_from_user_agent};
