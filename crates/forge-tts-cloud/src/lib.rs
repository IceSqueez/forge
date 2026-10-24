pub mod credentials;
pub mod network;
pub mod policy;

#[cfg(any(feature = "azure", feature = "elevenlabs", feature = "openai"))]
mod gateway_failure;

#[cfg(feature = "azure")]
pub mod azure;

#[cfg(feature = "elevenlabs")]
pub mod elevenlabs;

#[cfg(feature = "openai")]
pub mod openai;

#[cfg(feature = "polly")]
pub mod polly;

pub use network::{CloudGateways, ProviderGateway, ProviderLink};
