pub mod credentials;
mod network;
mod policy;

#[cfg(feature = "providers")]
mod error;
#[cfg(feature = "providers")]
mod factory;
#[cfg(feature = "providers")]
mod gateway_failure;

#[cfg(feature = "azure")]
pub mod azure;

#[cfg(feature = "elevenlabs")]
pub mod elevenlabs;

#[cfg(feature = "openai")]
pub mod openai;

#[cfg(feature = "polly")]
pub mod polly;

#[cfg(feature = "providers")]
pub use factory::{CloudEngine, CloudEngineFactory};
pub use network::CloudGateways;
pub use network::ProviderGateway;
pub use network::ProviderLink;
pub use policy::rate_limit::SynthesisRateLimiter;
pub use policy::retry::RetryConfig;
