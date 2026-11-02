use std::collections::HashMap;

use crate::provider::{EmoteSource, PRECEDENCE, ProviderEmote};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EmoteCatalog {
    images: HashMap<String, String>,
}

impl EmoteCatalog {
    pub fn from_sources<'a>(layer: impl Fn(EmoteSource) -> Option<&'a [ProviderEmote]>) -> Self {
        let mut images = HashMap::new();
        for source in PRECEDENCE {
            for emote in layer(source).unwrap_or_default() {
                images
                    .entry(emote.code.clone())
                    .or_insert_with(|| emote.url.clone());
            }
        }
        Self { images }
    }

    pub fn image(&self, code: &str) -> Option<&str> {
        self.images.get(code).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.images.len()
    }

    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }
}
