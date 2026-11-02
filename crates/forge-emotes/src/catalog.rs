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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    const EXPECTED_ORDER: [EmoteSource; 6] = [
        EmoteSource::SevenTvChannel,
        EmoteSource::BetterTtvChannel,
        EmoteSource::FrankerFaceZChannel,
        EmoteSource::SevenTvGlobal,
        EmoteSource::BetterTtvGlobal,
        EmoteSource::FrankerFaceZGlobal,
    ];

    fn url_of(source: EmoteSource) -> String {
        format!("https://cdn.example/{source:?}")
    }

    fn layer(code: &str, source: EmoteSource) -> Vec<ProviderEmote> {
        vec![ProviderEmote {
            code: code.to_owned(),
            url: url_of(source),
        }]
    }

    fn catalog_of(layers: &HashMap<EmoteSource, Vec<ProviderEmote>>) -> EmoteCatalog {
        EmoteCatalog::from_sources(|source| layers.get(&source).map(Vec::as_slice))
    }

    #[test]
    fn a_shared_code_takes_the_image_of_the_strongest_source_present() {
        for (strongest, source) in EXPECTED_ORDER.iter().enumerate() {
            let layers: HashMap<_, _> = EXPECTED_ORDER[strongest..]
                .iter()
                .map(|weaker| (*weaker, layer("KEKW", *weaker)))
                .collect();

            assert_eq!(
                catalog_of(&layers).image("KEKW"),
                Some(url_of(*source).as_str()),
                "with {source:?} and every weaker source present"
            );
        }
    }

    #[test]
    fn codes_differing_only_in_case_stay_separate_emotes() {
        let layers = HashMap::from([
            (
                EmoteSource::SevenTvGlobal,
                layer("KEKW", EmoteSource::SevenTvGlobal),
            ),
            (
                EmoteSource::BetterTtvChannel,
                layer("kekw", EmoteSource::BetterTtvChannel),
            ),
        ]);
        let catalog = catalog_of(&layers);

        assert_eq!(
            (
                catalog.image("KEKW"),
                catalog.image("kekw"),
                catalog.image("Kekw")
            ),
            (
                Some(url_of(EmoteSource::SevenTvGlobal).as_str()),
                Some(url_of(EmoteSource::BetterTtvChannel).as_str()),
                None
            )
        );
    }

    #[test]
    fn a_duplicate_code_inside_one_source_keeps_its_first_listing() {
        let layers = HashMap::from([(
            EmoteSource::SevenTvChannel,
            vec![
                ProviderEmote {
                    code: "KEKW".to_owned(),
                    url: "https://cdn.example/first".to_owned(),
                },
                ProviderEmote {
                    code: "KEKW".to_owned(),
                    url: "https://cdn.example/second".to_owned(),
                },
            ],
        )]);

        assert_eq!(
            catalog_of(&layers).image("KEKW"),
            Some("https://cdn.example/first")
        );
    }
}
