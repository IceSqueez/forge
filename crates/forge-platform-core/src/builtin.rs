use std::collections::BTreeMap;
use std::fmt;
use std::pin::Pin;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use forge_types::{SubActionStep, Variant};

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

/// Opaque icon token resolved to a tabler icon string by `forge-components::icon`.
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
    Accent,
    Subtle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrailingToken {
    Badge(String, TokenColor),
    Icon(SectionIcon, TokenColor),
    Label(String),
    TintedLabel(String, TokenColor),
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
    /// `None` tints the leading glyph by `active` state; `Some` overrides it.
    pub icon_tint: Option<TokenColor>,
    pub name: String,
    pub monospace_name: bool,
    pub active: bool,
    pub active_label: Option<String>,
    pub trailing: Vec<TrailingToken>,
    pub enabled: bool,
}

fn default_row_padding_y_px() -> u8 {
    7
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentList {
    pub title: String,
    pub icon: SectionIcon,
    /// Sits directly after the title; `count_label` sits on the header's opposite edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline_label: Option<String>,
    pub count_label: Option<String>,
    /// `Some` pins the row area to that many rows and scrolls inside it; `None` grows to fit.
    pub visible_rows: Option<u16>,
    #[serde(default = "default_row_padding_y_px")]
    pub row_padding_y_px: u8,
    /// Offers a header control that forces the owning integration to re-sync this roster now.
    #[serde(default)]
    pub refreshable: bool,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeroBadgeTone {
    Neutral,
    Positive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeroBadge {
    pub label: String,
    pub tone: HeroBadgeTone,
    pub monospace: bool,
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
        left: Box<ContentList>,
        right: Box<ContentList>,
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        banner: Option<String>,
    },
    ScopesList {
        title: String,
        icon: SectionIcon,
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
    TwoColumn {
        left: Box<DetailSection>,
        right: Box<DetailSection>,
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
    Transition,
    Profile,
    SceneCollection,
}

/// Maps to a fixed `ForgePalette` field so a quick action's icon renders in the same
/// semantic hue across every theme.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuickActionAccent {
    #[default]
    Brand,
    Success,
    Warning,
    Info,
    Bits,
    AccentPinkLight,
    Danger,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuickActionChoiceOption {
    pub value: String,
    pub label: String,
}

/// Static options are self-contained in the descriptor; dynamic options name a
/// `PickerKind` resolved asynchronously by the runtime->UI bridge when the modal opens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuickActionChoiceSource {
    Static(Vec<QuickActionChoiceOption>),
    Dynamic(PickerKind),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuickActionFieldKind {
    Text,
    Multiline,
    /// One option per line, collected as `Variant::Array` rather than a single string.
    MultilineList,
    Toggle,
    Choice(QuickActionChoiceSource),
}

impl QuickActionFieldKind {
    /// Splits into one array element per line only for `MultilineList`; every other kind
    /// marshals a collected `Text` value straight to `Variant::String`.
    pub fn marshal(&self, value: &QuickActionFieldValue) -> Variant {
        match (self, value) {
            (Self::MultilineList, QuickActionFieldValue::Text(text)) => Variant::Array(
                text.lines()
                    .map(|line| Variant::String(line.to_owned()))
                    .collect(),
            ),
            (_, QuickActionFieldValue::Text(text)) => Variant::String(text.clone()),
            (_, QuickActionFieldValue::Toggle(toggle)) => Variant::Bool(*toggle),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuickActionFieldValue {
    Text(String),
    Toggle(bool),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuickActionField {
    pub key: String,
    pub label: String,
    pub kind: QuickActionFieldKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<QuickActionFieldValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// Mirrors the target runner rejecting a blank value: the collection form blocks the run
    /// instead of letting it fail.
    #[serde(default)]
    pub required: bool,
}

impl QuickActionField {
    pub fn required(self) -> Self {
        Self {
            required: true,
            ..self
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuickAction {
    pub label: String,
    pub icon: SectionIcon,
    pub enabled: bool,
    /// Set when `enabled` is false because the broadcaster's tier/plan doesn't unlock this
    /// action (e.g. "Requires Twitch Affiliate or Partner"), distinct from being merely
    /// disconnected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked_reason: Option<String>,
    /// Category header the generic renderer groups this action under; `None` falls into a
    /// single untitled section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_icon: Option<SectionIcon>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_accent: Option<QuickActionAccent>,
    #[serde(default)]
    pub destructive: bool,
    #[serde(default)]
    pub accent: QuickActionAccent,
    pub subaction_template: SubActionStep,
    pub picker: Option<PickerKind>,
    /// Empty means the generic renderer degrades the collection modal to a plain run
    /// confirmation instead of a field form.
    #[serde(default)]
    pub fields: Vec<QuickActionField>,
}

impl QuickAction {
    pub fn merge_config(&self, values: &BTreeMap<String, QuickActionFieldValue>) -> SubActionStep {
        let mut step = self.subaction_template.clone();
        for field in &self.fields {
            if let Some(value) = values.get(&field.key).or(field.default.as_ref()) {
                step.config
                    .insert(field.key.clone(), field.kind.marshal(value));
            }
        }
        step
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityFlags {
    pub limited: bool,
    pub label: Option<String>,
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
    /// Absolute expiry of the active access token, when the integration authenticates with an
    /// expiring OAuth token; `None` for integrations without one (the UI omits the countdown).
    fn token_expiry(&self) -> Option<SystemTime> {
        None
    }
    /// Small pills rendered inline after the hero name (e.g. account id, broadcaster tier).
    fn name_badges(&self) -> Vec<HeroBadge> {
        Vec::new()
    }
    /// Account login shown as the hero title for account-scoped integrations; `None` falls back
    /// to `display_name`, which continues to label the breadcrumb either way.
    fn hero_name(&self) -> Option<&str> {
        None
    }
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

/// Coarse on purpose: a bearer, refresh token, or full request URL must never reach the
/// UI or any log sink, so the transport error is collapsed here instead of propagated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlFailure {
    NotConnected,
    Unauthorized,
    Unsupported,
    Transport,
}

impl fmt::Display for ControlFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = match self {
            Self::NotConnected => "not connected",
            Self::Unauthorized => "authorization expired or revoked",
            Self::Unsupported => "operation not supported by this integration",
            Self::Transport => "connection transport error",
        };
        f.write_str(reason)
    }
}

impl std::error::Error for ControlFailure {}

/// The steady connection state that follows is observed through `BuiltinStatus::connection()`
/// and the health stream, not returned here.
pub type ControlOutcome = Result<(), ControlFailure>;

#[async_trait]
pub trait BuiltinControl: Send + Sync {
    async fn reconnect(&self) -> ControlOutcome;
    async fn disconnect(&self) -> ControlOutcome;
    /// The renewed token stays inside the implementation; only accept/reject crosses this boundary.
    async fn refresh_token(&self) -> ControlOutcome;
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;

    use forge_types::Variant;

    use super::*;

    #[test]
    fn control_failure_display_is_coarse_and_carries_no_transport_detail() {
        const SECRET: &str = "DEADBEEF_BEARER";
        const SECRET_URL: &str = "https://id.twitch.tv/oauth2/token?access_token=DEADBEEF_BEARER";
        for failure in [
            ControlFailure::NotConnected,
            ControlFailure::Unauthorized,
            ControlFailure::Unsupported,
            ControlFailure::Transport,
        ] {
            let shown = failure.to_string();
            assert!(!shown.is_empty(), "{failure:?} display is empty");
            assert!(
                !shown.contains(SECRET) && !shown.contains(SECRET_URL),
                "{failure:?} display leaked a token-shaped string: {shown}"
            );
            for shape in ["http://", "https://", "://", "Bearer ", "token=", "?", "@"] {
                assert!(
                    !shown.contains(shape),
                    "{failure:?} display contains URL/token shape {shape:?}: {shown}"
                );
            }
        }
    }

    #[test]
    fn control_failure_debug_carries_no_transport_detail() {
        for (failure, expected) in [
            (ControlFailure::NotConnected, "NotConnected"),
            (ControlFailure::Unauthorized, "Unauthorized"),
            (ControlFailure::Unsupported, "Unsupported"),
            (ControlFailure::Transport, "Transport"),
        ] {
            assert_eq!(format!("{failure:?}"), expected);
        }
    }

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

    fn content_list_fixture(title: &str) -> ContentList {
        ContentList {
            title: title.to_owned(),
            icon: SectionIcon::new("list"),
            inline_label: None,
            count_label: Some("3".to_owned()),
            visible_rows: Some(8),
            row_padding_y_px: 7,
            refreshable: false,
            items: vec![ContentListItem {
                icon: SectionIcon::new("camera"),
                icon_tint: Some(TokenColor::Accent),
                name: "Main Scene".to_owned(),
                monospace_name: false,
                active: true,
                active_label: Some("LIVE".to_owned()),
                trailing: vec![
                    TrailingToken::Badge("HD".to_owned(), TokenColor::Green),
                    TrailingToken::TintedLabel("-12.3 dB".to_owned(), TokenColor::Subtle),
                ],
                enabled: true,
            }],
            footer: None,
        }
    }

    #[test]
    fn detail_section_two_column_lists_roundtrip_preserves_per_side_header_options() {
        let section = DetailSection::TwoColumnLists {
            left: Box::new(ContentList {
                inline_label: Some("in Gameplay".to_owned()),
                row_padding_y_px: 8,
                refreshable: true,
                ..content_list_fixture("Sources")
            }),
            right: Box::new(content_list_fixture("Scenes")),
        };
        let json = serde_json::to_string(&section).unwrap();
        let back: DetailSection = serde_json::from_str(&json).unwrap();
        assert_eq!(back, section);
    }

    #[test]
    fn content_list_json_omits_absent_inline_label() {
        let json = serde_json::to_string(&content_list_fixture("Scenes")).unwrap();
        assert!(!json.contains("inline_label"), "unexpected key in {json}");
    }

    #[test]
    fn content_list_without_header_options_deserializes_to_compact_row_defaults() {
        let json = r#"{
            "title": "Scenes",
            "icon": "list",
            "count_label": null,
            "visible_rows": null,
            "items": [],
            "footer": null
        }"#;
        let list: ContentList = serde_json::from_str(json).unwrap();
        assert_eq!(list.inline_label, None);
        assert_eq!(list.row_padding_y_px, 7);
        assert!(!list.refreshable);
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
            enabled: false,
            locked_reason: Some("Requires Twitch Affiliate or Partner".to_owned()),
            group: Some("Raids & ads".to_owned()),
            group_icon: None,
            group_accent: None,
            destructive: true,
            accent: QuickActionAccent::Danger,
            subaction_template: SubActionStep {
                kind_id: "obs.scenes.switch_current".to_owned(),
                config: BTreeMap::from([(
                    "scene".to_owned(),
                    Variant::String("Main Scene".to_owned()),
                )]),
                enabled: true,
                continue_on_error: false,
                condition: None,
                label: None,
            },
            picker: Some(PickerKind::Scene),
            fields: vec![QuickActionField {
                key: "scene".to_owned(),
                label: "Scene".to_owned(),
                kind: QuickActionFieldKind::Choice(QuickActionChoiceSource::Dynamic(
                    PickerKind::Scene,
                )),
                default: None,
                placeholder: None,
                hint: None,
                required: false,
            }],
        };
        let json = serde_json::to_string(&action).unwrap();
        let back: QuickAction = serde_json::from_str(&json).unwrap();
        assert_eq!(back, action);
    }

    #[test]
    fn quick_action_deserializes_legacy_json_missing_new_fields_as_defaults() {
        let template = SubActionStep {
            kind_id: "obs.scenes.switch_current".to_owned(),
            config: BTreeMap::new(),
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: None,
        };
        let legacy = serde_json::json!({
            "label": "Switch to Main",
            "icon": "play",
            "enabled": true,
            "subaction_template": serde_json::to_value(&template).unwrap(),
            "picker": serde_json::Value::Null,
        });
        let action: QuickAction = serde_json::from_value(legacy).unwrap();
        assert!(action.locked_reason.is_none());
        assert!(action.group.is_none());
        assert!(!action.destructive);
        assert_eq!(action.accent, QuickActionAccent::Brand);
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

    fn qa_template(config: BTreeMap<String, Variant>) -> SubActionStep {
        SubActionStep {
            kind_id: "test.kind".to_owned(),
            config,
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: None,
        }
    }

    fn qa_field(
        key: &str,
        kind: QuickActionFieldKind,
        default: Option<QuickActionFieldValue>,
    ) -> QuickActionField {
        QuickActionField {
            key: key.to_owned(),
            label: key.to_owned(),
            kind,
            default,
            placeholder: None,
            hint: None,
            required: false,
        }
    }

    fn qa_with(template: SubActionStep, fields: Vec<QuickActionField>) -> QuickAction {
        QuickAction {
            label: "A".to_owned(),
            icon: SectionIcon::new("x"),
            enabled: true,
            locked_reason: None,
            group: None,
            group_icon: None,
            group_accent: None,
            destructive: false,
            accent: QuickActionAccent::default(),
            subaction_template: template,
            picker: None,
            fields,
        }
    }

    #[test]
    fn merge_config_prefers_supplied_value_over_field_default() {
        let action = qa_with(
            qa_template(BTreeMap::new()),
            vec![qa_field(
                "title",
                QuickActionFieldKind::Text,
                Some(QuickActionFieldValue::Text("default title".to_owned())),
            )],
        );
        let mut values = BTreeMap::new();
        values.insert(
            "title".to_owned(),
            QuickActionFieldValue::Text("user title".to_owned()),
        );

        let merged = action.merge_config(&values);

        assert_eq!(
            merged.config.get("title"),
            Some(&Variant::String("user title".to_owned()))
        );
    }

    #[test]
    fn merge_config_falls_back_to_field_default_when_value_absent() {
        let action = qa_with(
            qa_template(BTreeMap::new()),
            vec![qa_field(
                "title",
                QuickActionFieldKind::Text,
                Some(QuickActionFieldValue::Text("default title".to_owned())),
            )],
        );

        let merged = action.merge_config(&BTreeMap::new());

        assert_eq!(
            merged.config.get("title"),
            Some(&Variant::String("default title".to_owned()))
        );
    }

    #[test]
    fn merge_config_leaves_key_untouched_when_no_value_and_no_default() {
        let template = qa_template(BTreeMap::from([(
            "reason".to_owned(),
            Variant::String("template value".to_owned()),
        )]));
        let action = qa_with(
            template,
            vec![
                qa_field("reason", QuickActionFieldKind::Text, None),
                qa_field("ghost", QuickActionFieldKind::Text, None),
            ],
        );

        let merged = action.merge_config(&BTreeMap::new());

        assert_eq!(
            merged.config.get("reason"),
            Some(&Variant::String("template value".to_owned())),
            "a field with neither supplied value nor default must not overwrite the template"
        );
        assert_eq!(
            merged.config.get("ghost"),
            None,
            "a valueless, defaultless field must not introduce a key"
        );
    }

    #[test]
    fn marshal_scalar_kinds_produce_string_or_bool_variants() {
        let choice = QuickActionFieldKind::Choice(QuickActionChoiceSource::Static(vec![
            QuickActionChoiceOption {
                value: "primary".to_owned(),
                label: "Primary".to_owned(),
            },
        ]));
        let cases = [
            (
                QuickActionFieldKind::Text,
                QuickActionFieldValue::Text("hi".to_owned()),
                Variant::String("hi".to_owned()),
            ),
            (
                QuickActionFieldKind::Multiline,
                QuickActionFieldValue::Text("a\nb".to_owned()),
                Variant::String("a\nb".to_owned()),
            ),
            (
                QuickActionFieldKind::Toggle,
                QuickActionFieldValue::Toggle(true),
                Variant::Bool(true),
            ),
            (
                QuickActionFieldKind::Toggle,
                QuickActionFieldValue::Toggle(false),
                Variant::Bool(false),
            ),
            (
                choice,
                QuickActionFieldValue::Text("primary".to_owned()),
                Variant::String("primary".to_owned()),
            ),
        ];
        for (kind, value, expected) in cases {
            assert_eq!(kind.marshal(&value), expected, "kind {kind:?}");
        }
    }

    #[test]
    fn marshal_multiline_list_splits_text_into_array_per_line() {
        let cases: [(&str, Vec<&str>); 5] = [
            ("one", vec!["one"]),
            ("a\nb\nc", vec!["a", "b", "c"]),
            ("a\n\nb", vec!["a", "", "b"]),
            ("a\nb\n", vec!["a", "b"]),
            ("", vec![]),
        ];
        for (input, lines) in cases {
            let got = QuickActionFieldKind::MultilineList
                .marshal(&QuickActionFieldValue::Text(input.to_owned()));
            let expected = Variant::Array(
                lines
                    .into_iter()
                    .map(|s| Variant::String(s.to_owned()))
                    .collect(),
            );
            assert_eq!(got, expected, "input {input:?}");
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
