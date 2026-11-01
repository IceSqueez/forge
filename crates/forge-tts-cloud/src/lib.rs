pub mod credentials;
mod network;
mod policy;

#[cfg(any(
    feature = "azure",
    feature = "elevenlabs",
    feature = "openai",
    feature = "polly"
))]
mod error;
#[cfg(any(
    feature = "azure",
    feature = "elevenlabs",
    feature = "openai",
    feature = "polly"
))]
mod factory;
#[cfg(any(
    feature = "azure",
    feature = "elevenlabs",
    feature = "openai",
    feature = "polly"
))]
mod gateway_failure;

#[cfg(feature = "azure")]
pub mod azure;

#[cfg(feature = "elevenlabs")]
pub mod elevenlabs;

#[cfg(feature = "openai")]
pub mod openai;

#[cfg(feature = "polly")]
pub mod polly;

#[cfg(any(
    feature = "azure",
    feature = "elevenlabs",
    feature = "openai",
    feature = "polly"
))]
pub use factory::{CloudEngine, CloudEngineFactory};
pub use network::CloudGateways;
pub use network::ProviderGateway;
pub use network::ProviderLink;
pub use policy::rate_limit::SynthesisRateLimiter;
pub use policy::retry::RetryConfig;
