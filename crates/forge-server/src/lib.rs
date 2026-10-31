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
mod overlay_url;
mod protocol;
mod run_state;
mod sandbox;
mod server;
mod server_info;
pub mod settings;
mod snapshot;
mod ws_client;

#[cfg(test)]
pub mod test_helpers;

pub use audio_clips::{ClipMediaType, ClipOffer, ClipOutcome, ClipOutcomeHandle};
pub use auth::{generate_token, masked_token};
pub use bus_adapter::{ClientId, EventFilter};
pub use config::{
    DEFAULT_PORT, LAN_BIND_ADDRESS, LOCALHOST_BIND_ADDRESS, ServerConfig, ServerSettings,
};
pub use delivery_hub::DeliveryHub;
pub use error::ServerError;
pub use handle::ServerHandle;
pub use overlay_token::masked_overlay_token;
pub use run_state::ServerRunWatch;
pub use server::{Server, start_server, stopped_server};
pub use snapshot::{
    BandwidthSnapshot, ConnectedClientSnapshot, EventFilterSnapshot, ServerSnapshot,
};

pub use audio_clips::ClipTicket;
pub use config::ServerTiming;
pub use overlay_url::{
    OVERLAY_ROUTE, extract_port, overlay_file_url, overlay_origin, overlay_page_url,
    resolve_routable_host,
};
pub use protocol::WireEventFilter;
pub use protocol::{WsEnvelope, WsRequest};
