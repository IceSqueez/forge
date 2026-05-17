#![doc = "Primitive value types and error scaffolding for forge."]

pub mod ids;
pub mod token;
pub mod variant;

pub use ids::{ActionId, CommandId, EventId, GlobalId, QueueId, ScriptId, TriggerId, UserId};
pub use token::{ApiKey, OAuthToken, RefreshToken};
pub use variant::{Variant, VariantError, VariantType};
