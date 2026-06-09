use std::fmt;
use std::pin::Pin;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use forge_types::SubActionStep;

use crate::ConnectionState;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BuiltinId(pub String);

impl BuiltinId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BuiltinId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Opaque icon token resolved to a tabler icon string by `forge-widgets::icon`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SectionIcon(pub(crate) String);

impl SectionIcon {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HealthValue {
    Status {
        label: String,
        active: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    Text {
        primary: String,
        secondary: Option<String>,
    },
    Pair {
        left: String,
        right: String,
    },
    Ratio {
        used: u64,
        total: u64,
        reset_hint: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthMetric {
    pub label: String,
    pub value: HealthValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthDelta {
    pub index: u8,
    pub new_value: HealthValue,
}

pub type HealthStream = Pin<Box<dyn futures_core::Stream<Item = HealthDelta> + Send + 'static>>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenColor {
    Green,
    Yellow,
    Red,
    Muted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrailingToken {
    Badge(String, TokenColor),
    Icon(SectionIcon),
    Label(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowAction {
    Play,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ContentListItem {
    pub icon: SectionIcon,
    pub name: String,
    pub monospace_name: bool,
    pub active: bool,
    pub active_label: Option<String>,
    pub trailing: Vec<TrailingToken>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentList {
    pub title: String,
    pub icon: SectionIcon,
    pub count_label: Option<String>,
    pub items: Vec<ContentListItem>,
    pub footer: Option<ListFooter>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyValueRow {
    pub icon: SectionIcon,
    pub name: String,
    pub tag: Option<String>,
    pub action: Option<RowAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveRow {
    pub name: String,
    pub active: bool,
    pub mode_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BannerLevel {
    Warning,
    Info,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionStatus {
    Active,
    Degraded,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionRow {
    pub name: String,
    pub status: SubscriptionStatus,
    pub version: Option<String>,
    pub event_count: Option<u64>,
    pub error_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListFooter {
    pub cta_label: Option<String>,
    pub trailing_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfoField {
    pub label: String,
    pub value: String,
    pub monospace_value: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthLevel {
    Good,
    Ok,
    Bad,
    NoData,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthBar {
    pub fraction: f32,
    pub label: String,
    pub level: HealthLevel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatColumn {
    pub label: String,
    pub value: String,
    pub subtitle: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DetailSection {
    TwoColumnLists {
        left: ContentList,
        right: ContentList,
    },
    KeyValueList {
        title: String,
        icon: SectionIcon,
        items: Vec<KeyValueRow>,
    },
    ActiveItemList {
        title: String,
        icon: SectionIcon,
        items: Vec<ActiveRow>,
    },
    WarningBanner {
        level: BannerLevel,
        title: String,
        body: String,
        cta: Option<String>,
    },
    SubscriptionList {
        title: String,
        icon: SectionIcon,
        items: Vec<SubscriptionRow>,
        footer: Option<ListFooter>,
    },
    ScopesList {
        title: String,
        scopes: Vec<String>,
        footer: Option<ListFooter>,
    },
    InfoCard {
        title: String,
        live: bool,
        fields: Vec<InfoField>,
        health_bar: Option<HealthBar>,
    },
    StatsGrid {
        title: String,
        icon: SectionIcon,
        columns: Vec<StatColumn>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PickerKind {
    Scene,
    Source,
    Input,
    Hotkey,
    Expression,
    MidiPort,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuickAction {
    pub label: String,
    pub icon: SectionIcon,
    pub enabled: bool,
    pub subaction_template: SubActionStep,
    pub picker: Option<PickerKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityFlags {
    pub limited: bool,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BadgeTone {
    Neutral,
    Positive,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeaderAction {
    Reconnect,
    RefreshToken,
    Disconnect,
    Settings,
}

pub trait BuiltinStatus: Send + Sync {
    fn id(&self) -> &BuiltinId;
    fn display_name(&self) -> &str;
    fn version(&self) -> Option<&str>;
    fn connection(&self) -> ConnectionState;
    fn uptime(&self) -> Option<Duration>;
    fn endpoint(&self) -> Option<&str>;
    fn capability_flags(&self) -> CapabilityFlags;
    fn header_actions(&self) -> Vec<HeaderAction>;
}

pub trait BuiltinHealth: Send + Sync {
    fn metrics(&self) -> [HealthMetric; 4];
    fn stream(&self) -> HealthStream;
}

pub trait BuiltinContent: Send + Sync {
    fn sections(&self) -> Vec<DetailSection>;
}

pub trait QuickActions: Send + Sync {
    fn actions(&self) -> Vec<QuickAction>;
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;

    use forge_types::Variant;

    use super::*;

    #[test]
    fn builtin_id_serde_transparent() {
        let id = BuiltinId::new("kick");
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, r#""kick""#);
        let back: BuiltinId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }

    #[test]
    fn section_icon_roundtrip() {
        let icon = SectionIcon::new("eye");
        assert_eq!(icon.as_str(), "eye");
        let json = serde_json::to_string(&icon).unwrap();
        assert_eq!(json, r#""eye""#);
        let back: SectionIcon = serde_json::from_str(&json).unwrap();
        assert_eq!(back, icon);
    }

    #[test]
    fn health_metric_serde_roundtrip() {
        let metric = HealthMetric {
            label: "Chat".to_owned(),
            value: HealthValue::Status {
                label: "Connected".to_owned(),
                active: true,
                detail: None,
            },
        };
        let json = serde_json::to_string(&metric).unwrap();
        let back: HealthMetric = serde_json::from_str(&json).unwrap();
        assert_eq!(back, metric);
    }

    #[test]
    fn health_value_serde_roundtrip_each_variant() {
        for v in [
            HealthValue::Status {
                label: "Connected".to_owned(),
                active: true,
                detail: None,
            },
            HealthValue::Text {
                primary: "42 msg/s".to_owned(),
                secondary: Some("peak: 150".to_owned()),
            },
            HealthValue::Text {
                primary: "idle".to_owned(),
                secondary: None,
            },
            HealthValue::Pair {
                left: "60 fps".to_owned(),
                right: "2.4%".to_owned(),
            },
            HealthValue::Ratio {
                used: 800,
                total: 1000,
                reset_hint: Some("resets hourly".to_owned()),
            },
            HealthValue::Ratio {
                used: 0,
                total: 100,
                reset_hint: None,
            },
        ] {
            let json = serde_json::to_string(&v).unwrap();
            let back: HealthValue = serde_json::from_str(&json).unwrap();
            assert_eq!(back, v);
        }
    }

    #[test]
    fn health_delta_serde_roundtrip() {
        let delta = HealthDelta {
            index: 2,
            new_value: HealthValue::Ratio {
                used: 450,
                total: 800,
                reset_hint: None,
            },
        };
        let json = serde_json::to_string(&delta).unwrap();
        let back: HealthDelta = serde_json::from_str(&json).unwrap();
        assert_eq!(back.index, delta.index);
        assert_eq!(back.new_value, delta.new_value);
    }

    #[test]
    fn capability_flags_serde_roundtrip_with_and_without_label() {
        for flags in [
            CapabilityFlags {
                limited: true,
                label: Some("read-only".to_owned()),
            },
            CapabilityFlags {
                limited: false,
                label: None,
            },
        ] {
            let json = serde_json::to_string(&flags).unwrap();
            let back: CapabilityFlags = serde_json::from_str(&json).unwrap();
            assert_eq!(back, flags);
        }
    }

    #[test]
    fn header_action_serde_variants() {
        let cases = [
            (HeaderAction::Reconnect, r#""reconnect""#),
            (HeaderAction::RefreshToken, r#""refresh_token""#),
            (HeaderAction::Disconnect, r#""disconnect""#),
            (HeaderAction::Settings, r#""settings""#),
        ];
        for (action, expected_json) in &cases {
            let json = serde_json::to_string(action).unwrap();
            assert_eq!(json, *expected_json);
            let back: HeaderAction = serde_json::from_str(&json).unwrap();
            assert_eq!(back, *action);
        }
    }

    #[test]
    fn detail_section_warning_banner_roundtrip() {
        let section = DetailSection::WarningBanner {
            level: BannerLevel::Warning,
            title: "Limited API".to_owned(),
            body: "Kick has no public OAuth API.".to_owned(),
            cta: Some("Learn more".to_owned()),
        };
        let json = serde_json::to_string(&section).unwrap();
        let back: DetailSection = serde_json::from_str(&json).unwrap();
        assert_eq!(back, section);
    }

    #[test]
    fn detail_section_two_column_lists_roundtrip() {
        let make_list = |title: &str| ContentList {
            title: title.to_owned(),
            icon: SectionIcon::new("list"),
            count_label: Some("3".to_owned()),
            items: vec![ContentListItem {
                icon: SectionIcon::new("camera"),
                name: "Main Scene".to_owned(),
                monospace_name: false,
                active: true,
                active_label: Some("LIVE".to_owned()),
                trailing: vec![TrailingToken::Badge("HD".to_owned(), TokenColor::Green)],
                enabled: true,
            }],
            footer: None,
        };
        let section = DetailSection::TwoColumnLists {
            left: make_list("Scenes"),
            right: make_list("Sources"),
        };
        let json = serde_json::to_string(&section).unwrap();
        let back: DetailSection = serde_json::from_str(&json).unwrap();
        assert_eq!(back, section);
    }

    #[test]
    fn detail_section_info_card_roundtrip() {
        let section = DetailSection::InfoCard {
            title: "Live Broadcast".to_owned(),
            live: true,
            fields: vec![InfoField {
                label: "Viewers".to_owned(),
                value: "1 234".to_owned(),
                monospace_value: false,
            }],
            health_bar: Some(HealthBar {
                fraction: 0.72,
                label: "72%".to_owned(),
                level: HealthLevel::Good,
            }),
        };
        let json = serde_json::to_string(&section).unwrap();
        let back: DetailSection = serde_json::from_str(&json).unwrap();
        assert_eq!(back, section);
    }

    #[test]
    fn quick_action_serde_roundtrip() {
        let action = QuickAction {
            label: "Switch to Main".to_owned(),
            icon: SectionIcon::new("play"),
            enabled: true,
            subaction_template: SubActionStep {
                kind_id: "obs.scenes.switch_current".to_owned(),
                config: BTreeMap::from([(
                    "scene".to_owned(),
                    Variant::String("Main Scene".to_owned()),
                )]),
                enabled: true,
                label: None,
            },
            picker: Some(PickerKind::Scene),
        };
        let json = serde_json::to_string(&action).unwrap();
        let back: QuickAction = serde_json::from_str(&json).unwrap();
        assert_eq!(back, action);
    }

    #[test]
    fn picker_kind_serde_variants() {
        let cases = [
            (PickerKind::Scene, r#""scene""#),
            (PickerKind::Source, r#""source""#),
            (PickerKind::Input, r#""input""#),
            (PickerKind::Hotkey, r#""hotkey""#),
            (PickerKind::Expression, r#""expression""#),
            (PickerKind::MidiPort, r#""midi_port""#),
        ];
        for (kind, expected) in &cases {
            let json = serde_json::to_string(kind).unwrap();
            assert_eq!(json, *expected);
            let back: PickerKind = serde_json::from_str(&json).unwrap();
            assert_eq!(back, *kind);
        }
    }

    #[allow(dead_code)]
    fn dyn_all(
        _: &dyn BuiltinStatus,
        _: &dyn BuiltinHealth,
        _: &dyn BuiltinContent,
        _: &dyn QuickActions,
    ) {
    }
}
