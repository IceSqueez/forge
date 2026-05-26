#![doc = "Versioned WebSocket + HTTP server for overlays and remote control. /ws/v1/..."]

mod routes;

pub mod auth;
pub mod bandwidth;
pub mod bus_adapter;
pub mod config;
pub mod error;
pub mod handle;
pub mod protocol;
pub mod server;
pub mod server_info;
pub mod ws_client;

#[cfg(test)]
pub mod test_helpers;

pub use auth::AuthState;
pub use bandwidth::BandwidthTracker;
pub use bus_adapter::{BusAdapter, ClientId, EventFilter};
pub use config::{ServerConfig, ServerSettings};
pub use error::ServerError;
pub use handle::ServerHandle;
pub use server::{AppState, Server, start_server};
pub use server_info::ServerInfo;
pub use ws_client::{ClientType, WsClient, detect_from_user_agent};
