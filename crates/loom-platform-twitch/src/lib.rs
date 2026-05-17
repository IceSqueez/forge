#![doc = "Twitch platform integration: auth (alpha-2), chat ingestion (alpha-3+)."]

pub mod auth;

pub use auth::{
    TWITCH_BROADCASTER_SCOPES, TWITCH_DEVICE_ENDPOINT, TWITCH_TOKEN_ENDPOINT, new_twitch_poller,
    request_twitch_device_code, twitch_auth_flow,
};
