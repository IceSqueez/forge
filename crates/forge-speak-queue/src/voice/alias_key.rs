use forge_types::PlatformId;

const PLATFORM_SEPARATOR: char = ':';

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasKey {
    pub platform: Option<PlatformId>,
    pub handle: String,
}

impl AliasKey {
    pub fn new(platform: Option<PlatformId>, handle: &str) -> Self {
        Self {
            platform,
            handle: handle.to_owned(),
        }
    }

    pub fn parse(stored: &str) -> Self {
        let (platform, handle) = split_stored(stored);
        Self::new(platform, handle)
    }

    pub fn to_stored(&self) -> String {
        match self.platform {
            Some(platform) => format!("{}{PLATFORM_SEPARATOR}{}", platform.as_str(), self.handle),
            None => self.handle.clone(),
        }
    }
}

pub(crate) fn split_stored(stored: &str) -> (Option<PlatformId>, &str) {
    stored
        .split_once(PLATFORM_SEPARATOR)
        .and_then(|(token, handle)| {
            PlatformId::parse(token).map(|platform| (Some(platform), handle))
        })
        .unwrap_or((None, stored))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stored_key_parses_into_a_platform_only_for_an_exact_platform_token() {
        for (stored, platform, handle) in [
            ("twitch:77", Some(PlatformId::Twitch), "77"),
            ("youtube:Alice", Some(PlatformId::YouTube), "Alice"),
            ("kick:a:b", Some(PlatformId::Kick), "a:b"),
            ("twitch:", Some(PlatformId::Twitch), ""),
            ("Alice", None, "Alice"),
            ("discord:Alice", None, "discord:Alice"),
            ("Twitch:Alice", None, "Twitch:Alice"),
            ("Mr: X", None, "Mr: X"),
            (":Alice", None, ":Alice"),
            ("", None, ""),
        ] {
            assert_eq!(
                AliasKey::parse(stored),
                AliasKey::new(platform, handle),
                "{stored:?}"
            );
        }
    }

    #[test]
    fn every_stored_key_survives_a_parse_and_store_round_trip_byte_for_byte() {
        for stored in [
            "twitch:77",
            "Alice",
            "discord:Alice",
            "Mr: X",
            "Twitch:Alice",
            "twitch:",
            "kick:a:b",
            "youtube:Зірка",
            ":",
            "",
        ] {
            assert_eq!(AliasKey::parse(stored).to_stored(), stored);
        }
    }

    #[test]
    fn a_built_key_parses_back_to_itself_for_every_platform_scope() {
        let scopes = PlatformId::ALL.into_iter().map(Some).chain([None]);
        for platform in scopes {
            for handle in ["Alice", "Зірка", "user_42", ""] {
                let key = AliasKey::new(platform, handle);

                assert_eq!(AliasKey::parse(&key.to_stored()), key, "{key:?}");
            }
        }
    }
}
