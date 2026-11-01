mod access;
mod delivery;
mod files;
mod lifecycle;
mod routes;

mod config;
mod error;
mod protocol;
pub mod settings;

#[cfg(test)]
pub mod test_helpers;
#[cfg(feature = "test-support")]
pub mod test_support;

pub use access::auth::{generate_token, masked_token};
pub use access::overlay_token::masked_overlay_token;
pub use config::{
    DEFAULT_PORT, LAN_BIND_ADDRESS, LOCALHOST_BIND_ADDRESS, ServerConfig, ServerSettings,
};
pub use delivery::bus_adapter::{ClientId, EventFilter};
pub use delivery::delivery_hub::DeliveryHub;
pub use delivery::snapshot::{
    BandwidthSnapshot, ConnectedClientSnapshot, EventFilterSnapshot, ServerSnapshot,
};
pub use error::ServerError;
pub use files::audio_clips::{ClipMediaType, ClipOffer, ClipOutcome, ClipOutcomeHandle};
pub use lifecycle::handle::ServerHandle;
pub use lifecycle::run_state::ServerRunWatch;
pub use lifecycle::server::{start_server, stopped_server};

pub use config::ServerTiming;
pub use files::audio_clips::ClipTicket;
pub use lifecycle::overlay_url::{
    OVERLAY_ROUTE, overlay_file_url, overlay_origin, overlay_page_url, resolve_routable_host,
};
pub use protocol::WireEventFilter;
pub use protocol::{WsEnvelope, WsRequest};
