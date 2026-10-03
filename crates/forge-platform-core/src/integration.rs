use forge_types::IntegrationId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IntegrationCategory {
    key: &'static str,
    label_key: &'static str,
    blurb_key: &'static str,
}

impl IntegrationCategory {
    pub const STREAMING: Self = Self {
        key: "streaming",
        label_key: "integration_category_streaming",
        blurb_key: "integration_category_streaming_blurb",
    };
    pub const DONATIONS: Self = Self {
        key: "donations",
        label_key: "integration_category_donations",
        blurb_key: "integration_category_donations_blurb",
    };
    pub const APPS: Self = Self {
        key: "apps",
        label_key: "integration_category_apps",
        blurb_key: "integration_category_apps_blurb",
    };
    pub const COMMUNITY: Self = Self {
        key: "community",
        label_key: "integration_category_community",
        blurb_key: "integration_category_community_blurb",
    };
    pub const AUDIO: Self = Self {
        key: "audio",
        label_key: "integration_category_audio",
        blurb_key: "integration_category_audio_blurb",
    };
    pub const CONTROLS: Self = Self {
        key: "controls",
        label_key: "integration_category_controls",
        blurb_key: "integration_category_controls_blurb",
    };
    pub const TOOLS: Self = Self {
        key: "tools",
        label_key: "integration_category_tools",
        blurb_key: "integration_category_tools_blurb",
    };

    pub const DISPLAY_ORDER: &'static [Self] = &[
        Self::STREAMING,
        Self::DONATIONS,
        Self::APPS,
        Self::COMMUNITY,
        Self::AUDIO,
        Self::CONTROLS,
        Self::TOOLS,
    ];

    pub fn from_key(key: &str) -> Option<Self> {
        Self::DISPLAY_ORDER
            .iter()
            .copied()
            .find(|category| category.key == key)
    }

    pub const fn key(self) -> &'static str {
        self.key
    }

    pub const fn label_key(self) -> &'static str {
        self.label_key
    }

    pub const fn blurb_key(self) -> &'static str {
        self.blurb_key
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionAffordance {
    Connectable,
    Connectionless,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrationDeclaration {
    pub id: IntegrationId,
    pub brand_name: &'static str,
    pub category: IntegrationCategory,
    pub description_key: &'static str,
    pub connection: ConnectionAffordance,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_displayed_category_resolves_back_from_its_own_key() {
        for category in IntegrationCategory::DISPLAY_ORDER {
            assert_eq!(
                IntegrationCategory::from_key(category.key()),
                Some(*category),
                "{}",
                category.key()
            );
        }
    }

    #[test]
    fn keys_outside_the_category_set_resolve_to_no_category() {
        for key in [
            "",
            "Streaming",
            " streaming",
            "streaming ",
            "streaming_platforms",
            "stream_apps",
            "devices_and_input",
            "music",
        ] {
            assert_eq!(IntegrationCategory::from_key(key), None, "{key:?}");
        }
    }
}
