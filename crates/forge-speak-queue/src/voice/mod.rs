mod alias_key;
mod language;
mod resolver;

pub use alias_key::AliasKey;
pub(crate) use language::{candidate_languages, voice_speaks_language};
pub use resolver::{ResolveResult, VoiceAliasResolver, VoiceSkip};
