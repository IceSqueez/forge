#![doc = "Versioned WebSocket + HTTP server for overlays and remote control. /ws/v1/..."]

mod routes;

mod audio_clips;
mod auth;
mod bandwidth;
mod bus_adapter;
mod config;
mod delivery_hub;
mod error;
mod handle;
mod listener;
mod origin;
mod overlay_token;
pub mod overlay_url;
pub mod protocol;
mod run_state;
mod sandbox;
mod server;
mod server_info;
pub mod settings;
mod snapshot;
mod ws_client;

#[cfg(test)]
pub mod test_helpers;

pub use audio_clips::{
    AudioClipStore, ClipCapability, ClipMediaType, ClipOffer, ClipOutcome, ClipOutcomeHandle,
    ClipTicket, MAX_CLIP_BYTES, MAX_TOTAL_CLIP_BYTES,
};
pub use auth::{generate_token, masked_token};
pub use bandwidth::BandwidthTracker;
pub use bus_adapter::{BusAdapter, ClientId, EventFilter};
pub use config::{
    DEFAULT_PORT, LAN_BIND_ADDRESS, LOCALHOST_BIND_ADDRESS, ServerConfig, ServerSettings,
};
pub use delivery_hub::DeliveryHub;
pub use error::{SecretToken, ServerError};
pub use handle::ServerHandle;
pub use overlay_token::{OverlayTokenState, masked_overlay_token};
pub use server::{AppState, Server, start_server, stopped_server};
pub use server_info::ServerInfo;
pub use snapshot::{
    BandwidthSnapshot, ConnectedClientSnapshot, EventFilterSnapshot, ServerSnapshot,
};
pub use ws_client::{ClientType, WsClient, detect_from_user_agent};
