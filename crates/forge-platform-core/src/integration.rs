use forge_types::IntegrationId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IntegrationCategory {
    key: &'static str,
    label_key: &'static str,
}

impl IntegrationCategory {
    pub const STREAMING_PLATFORMS: Self = Self {
        key: "streaming_platforms",
        label_key: "integration_category_streaming_platforms",
    };
    pub const STREAM_APPS: Self = Self {
        key: "stream_apps",
        label_key: "integration_category_stream_apps",
    };
    pub const COMMUNITY: Self = Self {
        key: "community",
        label_key: "integration_category_community",
    };
    pub const DEVICES_AND_INPUT: Self = Self {
        key: "devices_and_input",
        label_key: "integration_category_devices_and_input",
    };
    pub const MUSIC: Self = Self {
        key: "music",
        label_key: "integration_category_music",
    };

    pub const DISPLAY_ORDER: &'static [Self] = &[
        Self::STREAMING_PLATFORMS,
        Self::STREAM_APPS,
        Self::COMMUNITY,
        Self::DEVICES_AND_INPUT,
        Self::MUSIC,
    ];

    pub const fn key(self) -> &'static str {
        self.key
    }

    pub const fn label_key(self) -> &'static str {
        self.label_key
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
