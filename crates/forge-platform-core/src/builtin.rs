use std::collections::BTreeMap;
use std::fmt;
use std::pin::Pin;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use forge_types::{
    HEALTH_VIEWERS, IntegrationId, MessageCode, MessageRef, SubActionStep, UserText, Variant,
    VerbatimText,
};

use crate::ConnectionState;
use crate::collections::{CollectionId, CollectionRevisions};
use crate::option_source::OptionListId;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthText {
    Text(UserText),
    Count(u64),
    Span(Duration),
    Connection(ConnectionState),
}

impl HealthText {
    pub fn unlocalized(text: impl Into<String>) -> Self {
        Self::Text(UserText::unlocalized(text))
    }
}

impl HealthText {
    pub fn placeholder() -> Self {
        Self::Text(UserText::Verbatim(VerbatimText::new("-")))
    }
}

impl From<MessageRef> for HealthText {
    fn from(message: MessageRef) -> Self {
        Self::Text(UserText::Message(message))
    }
}

impl From<VerbatimText> for HealthText {
    fn from(text: VerbatimText) -> Self {
        Self::Text(UserText::Verbatim(text))
    }
}

impl From<UserText> for HealthText {
    fn from(text: UserText) -> Self {
        Self::Text(text)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HealthValue {
    Status {
        label: HealthText,
        active: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<HealthText>,
    },
    Text {
        primary: HealthText,
        secondary: Option<HealthText>,
    },
    Pair {
        left: HealthText,
        right: HealthText,
    },
    Ratio {
        used: u64,
        total: u64,
        reset_hint: Option<HealthText>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HealthMetricId(MessageCode);

impl HealthMetricId {
    pub const VIEWERS: Self = Self::new(HEALTH_VIEWERS);

    pub const fn new(label: MessageCode) -> Self {
        Self(label)
    }

    pub fn label(&self) -> MessageRef {
        MessageRef::new(self.0.clone())
    }
}

impl fmt::Display for HealthMetricId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthMetric {
    pub id: HealthMetricId,
    pub value: HealthValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthDelta {
    pub metric: HealthMetricId,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrailingToken {
    Badge(UserText, TokenColor),
    Icon(SectionIcon, TokenColor),
    Label(UserText),
    TintedLabel(UserText, TokenColor),
    ActionIcon {
        icon: SectionIcon,
        tint: TokenColor,
        step: SubActionStep,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowAction {
    Play,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ContentListItem {
    pub icon: SectionIcon,
    pub icon_tint: Option<TokenColor>,
    pub name: String,
    pub monospace_name: bool,
    pub active: bool,
    pub active_label: Option<UserText>,
    pub trailing: Vec<TrailingToken>,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_click: Option<SubActionStep>,
}

fn default_row_padding_y_px() -> u8 {
    7
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentList {
    pub title: UserText,
    pub icon: SectionIcon,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline_label: Option<UserText>,
    pub count_label: Option<UserText>,
    pub visible_rows: Option<u16>,
    #[serde(default = "default_row_padding_y_px")]
    pub row_padding_y_px: u8,
    #[serde(default)]
    pub refreshable: bool,
    pub items: Vec<ContentListItem>,
    pub footer: Option<ListFooter>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyValueRow {
    pub icon: SectionIcon,
    pub name: UserText,
    pub tag: Option<UserText>,
    pub action: Option<RowAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveRow {
    pub name: String,
    pub active: bool,
    pub mode_label: Option<UserText>,
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
    pub error_label: Option<UserText>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeroBadgeTone {
    Neutral,
    Positive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeroBadge {
    pub label: UserText,
    pub tone: HeroBadgeTone,
    pub monospace: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListFooter {
    pub cta_label: Option<UserText>,
    pub trailing_label: Option<UserText>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfoField {
    pub label: UserText,
    pub value: UserText,
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
    pub label: UserText,
    pub level: HealthLevel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatColumn {
    pub label: UserText,
    pub value: UserText,
    pub subtitle: UserText,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DetailSection {
    TwoColumnLists {
        left: Box<ContentList>,
        right: Box<ContentList>,
    },
    KeyValueList {
        title: UserText,
        icon: SectionIcon,
        items: Vec<KeyValueRow>,
    },
    ActiveItemList {
        title: UserText,
        icon: SectionIcon,
        items: Vec<ActiveRow>,
    },
    WarningBanner {
        level: BannerLevel,
        title: UserText,
        body: UserText,
        cta: Option<UserText>,
    },
    SubscriptionList {
        title: UserText,
        icon: SectionIcon,
        items: Vec<SubscriptionRow>,
        footer: Option<ListFooter>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        banner: Option<UserText>,
    },
    ScopesList {
        title: UserText,
        icon: SectionIcon,
        scopes: Vec<String>,
        footer: Option<ListFooter>,
    },
    InfoCard {
        title: UserText,
        live: bool,
        fields: Vec<InfoField>,
        health_bar: Option<HealthBar>,
    },
    StatsGrid {
        title: UserText,
        icon: SectionIcon,
        columns: Vec<StatColumn>,
    },
    TwoColumn {
        left: Box<DetailSection>,
        right: Box<DetailSection>,
    },
    ChipGrid {
        title: UserText,
        icon: SectionIcon,
        chip_icon: SectionIcon,
        items: Vec<String>,
    },
}

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
    pub label: UserText,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuickActionChoiceSource {
    Static(Vec<QuickActionChoiceOption>),
    OptionList(OptionListId),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuickActionFieldKind {
    Text,
    Multiline,
    MultilineList,
    Toggle,
    Choice(QuickActionChoiceSource),
    Int { min: i64, max: i64 },
}

impl QuickActionFieldKind {
    pub fn marshal(&self, value: &QuickActionFieldValue) -> Variant {
        match (self, value) {
            (Self::MultilineList, QuickActionFieldValue::Text(text)) => Variant::Array(
                text.lines()
                    .map(|line| Variant::String(line.to_owned()))
                    .collect(),
            ),
            (_, QuickActionFieldValue::Text(text)) => Variant::String(text.clone()),
            (_, QuickActionFieldValue::Toggle(toggle)) => Variant::Bool(*toggle),
            (_, QuickActionFieldValue::Int(n)) => Variant::Int(*n),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuickActionFieldValue {
    Text(String),
    Toggle(bool),
    Int(i64),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuickActionField {
    pub key: String,
    pub label: UserText,
    pub kind: QuickActionFieldKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<QuickActionFieldValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<UserText>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<UserText>,
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuickActionLiveness {
    #[default]
    Unknown,
    Live,
    Absent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuickActionGroup {
    pub label: UserText,
    pub icon: SectionIcon,
    #[serde(default)]
    pub accent: QuickActionAccent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuickAction {
    pub label: UserText,
    pub icon: SectionIcon,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked_reason: Option<UserText>,
    #[serde(default)]
    pub liveness: QuickActionLiveness,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<QuickActionGroup>,
    #[serde(default)]
    pub destructive: bool,
    #[serde(default)]
    pub accent: QuickActionAccent,
    pub subaction_template: SubActionStep,
    #[serde(default)]
    pub fields: Vec<QuickActionField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<CollectionId>,
}

impl QuickAction {
    pub fn is_runnable(&self) -> bool {
        self.enabled && self.liveness != QuickActionLiveness::Absent
    }

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
    pub label: Option<UserText>,
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
    fn id(&self) -> &IntegrationId;
    fn display_name(&self) -> &str;
    fn version(&self) -> Option<&str>;
    fn connection(&self) -> ConnectionState;
    fn uptime(&self) -> Option<Duration>;
    fn endpoint(&self) -> Option<&str>;
    fn capability_flags(&self) -> CapabilityFlags;
    fn header_actions(&self) -> Vec<HeaderAction>;
    fn token_expiry(&self) -> Option<SystemTime> {
        None
    }
    fn name_badges(&self) -> Vec<HeroBadge> {
        Vec::new()
    }
    fn hero_name(&self) -> Option<&str> {
        None
    }
    fn activity_count(&self) -> Option<usize> {
        None
    }
    fn connect_failure(&self) -> Option<MessageRef> {
        None
    }
}

pub const HEALTH_CHANNEL_CAPACITY: usize = 16;

pub trait BuiltinHealth: Send + Sync {
    fn metrics(&self) -> [HealthMetric; 4];
    fn stream(&self) -> HealthStream;
}

pub trait BuiltinContent: Send + Sync {
    fn sections(&self) -> Vec<DetailSection>;
}

pub trait QuickActions: Send + Sync {
    fn actions(&self) -> Vec<QuickAction>;

    fn revisions(&self) -> Option<CollectionRevisions> {
        None
    }
}

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

pub type ControlOutcome = Result<(), ControlFailure>;

#[async_trait]
pub trait BuiltinControl: Send + Sync {
    async fn reconnect(&self) -> ControlOutcome;
    async fn disconnect(&self) -> ControlOutcome;
    async fn refresh_token(&self) -> ControlOutcome;
}

#[cfg(test)]
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
        let id = IntegrationId::new("kick");
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, r#""kick""#);
        let back: IntegrationId = serde_json::from_str(&json).unwrap();
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
            id: HealthMetricId::VIEWERS,
            value: HealthValue::Status {
                label: HealthText::unlocalized("Connected"),
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
                label: HealthText::unlocalized("Connected"),
                active: true,
                detail: None,
            },
            HealthValue::Text {
                primary: HealthText::unlocalized("42 msg/s"),
                secondary: Some(HealthText::unlocalized("peak: 150")),
            },
            HealthValue::Text {
                primary: HealthText::unlocalized("idle"),
                secondary: None,
            },
            HealthValue::Pair {
                left: HealthText::unlocalized("60 fps"),
                right: HealthText::unlocalized("2.4%"),
            },
            HealthValue::Ratio {
                used: 800,
                total: 1000,
                reset_hint: Some(HealthText::unlocalized("resets hourly")),
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
    fn health_delta_metric_id_travels_as_bare_string_and_matches_declared_const() {
        let delta = HealthDelta {
            metric: HealthMetricId::VIEWERS,
            new_value: HealthValue::Ratio {
                used: 450,
                total: 800,
                reset_hint: None,
            },
        };
        let json = serde_json::to_value(&delta).unwrap();
        assert_eq!(json["metric"], serde_json::json!("common.health_viewers"));
        let back: HealthDelta = serde_json::from_value(json).unwrap();
        assert_eq!(back.metric, HealthMetricId::VIEWERS);
        assert_eq!(back.new_value, delta.new_value);
    }

    #[test]
    fn capability_flags_serde_roundtrip_with_and_without_label() {
        for flags in [
            CapabilityFlags {
                limited: true,
                label: Some(UserText::unlocalized("read-only")),
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
            title: UserText::unlocalized("Limited API"),
            body: UserText::unlocalized("Kick has no public OAuth API."),
            cta: Some(UserText::unlocalized("Learn more")),
        };
        let json = serde_json::to_string(&section).unwrap();
        let back: DetailSection = serde_json::from_str(&json).unwrap();
        assert_eq!(back, section);
    }

    fn content_list_fixture(title: &str) -> ContentList {
        ContentList {
            title: UserText::unlocalized(title),
            icon: SectionIcon::new("list"),
            inline_label: None,
            count_label: Some(UserText::unlocalized("3")),
            visible_rows: Some(8),
            row_padding_y_px: 7,
            refreshable: false,
            items: vec![ContentListItem {
                icon: SectionIcon::new("camera"),
                icon_tint: Some(TokenColor::Accent),
                name: "Main Scene".to_owned(),
                monospace_name: false,
                active: true,
                active_label: Some(UserText::unlocalized("LIVE")),
                trailing: vec![
                    TrailingToken::Badge(UserText::unlocalized("HD"), TokenColor::Green),
                    TrailingToken::TintedLabel(
                        UserText::unlocalized("-12.3 dB"),
                        TokenColor::Subtle,
                    ),
                ],
                enabled: true,
                on_click: None,
            }],
            footer: None,
        }
    }

    #[test]
    fn detail_section_two_column_lists_roundtrip_preserves_per_side_header_options() {
        let section = DetailSection::TwoColumnLists {
            left: Box::new(ContentList {
                inline_label: Some(UserText::unlocalized("in Gameplay")),
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
            "title": { "verbatim": "Scenes" },
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
            title: UserText::unlocalized("Live Broadcast"),
            live: true,
            fields: vec![InfoField {
                label: UserText::unlocalized("Viewers"),
                value: UserText::unlocalized("1 234"),
                monospace_value: false,
            }],
            health_bar: Some(HealthBar {
                fraction: 0.72,
                label: UserText::unlocalized("72%"),
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
            label: UserText::unlocalized("Switch to Main"),
            icon: SectionIcon::new("play"),
            enabled: false,
            locked_reason: Some(UserText::unlocalized(
                "Requires Twitch Affiliate or Partner",
            )),
            liveness: QuickActionLiveness::Unknown,
            group: Some(QuickActionGroup {
                label: UserText::unlocalized("Raids & ads"),
                icon: SectionIcon::new("businessplan"),
                accent: QuickActionAccent::Bits,
            }),
            destructive: true,
            accent: QuickActionAccent::Danger,
            subaction_template: SubActionStep {
                kind_id: "obs.scene.switch_current".to_owned(),
                config: BTreeMap::from([(
                    "scene".to_owned(),
                    Variant::String("Main Scene".to_owned()),
                )]),
                enabled: true,
                continue_on_error: false,
                condition: None,
                label: None,
            },
            fields: vec![QuickActionField {
                key: "scene".to_owned(),
                label: UserText::unlocalized("Scene"),
                kind: QuickActionFieldKind::Choice(QuickActionChoiceSource::OptionList(
                    crate::OptionListId::new("obs.scenes"),
                )),
                default: None,
                placeholder: None,
                hint: None,
                required: false,
            }],
            collection: Some(crate::collections::CollectionId::new("rewards")),
        };
        let json = serde_json::to_string(&action).unwrap();
        let back: QuickAction = serde_json::from_str(&json).unwrap();
        assert_eq!(back, action);
    }

    #[test]
    fn quick_action_deserializes_legacy_json_missing_new_fields_as_defaults() {
        let template = SubActionStep {
            kind_id: "obs.scene.switch_current".to_owned(),
            config: BTreeMap::new(),
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: None,
        };
        let legacy = serde_json::json!({
            "label": { "verbatim": "Switch to Main" },
            "icon": "play",
            "enabled": true,
            "subaction_template": serde_json::to_value(&template).unwrap(),
        });
        let action: QuickAction = serde_json::from_value(legacy).unwrap();
        assert!(action.locked_reason.is_none());
        assert!(action.group.is_none());
        assert!(!action.destructive);
        assert_eq!(action.accent, QuickActionAccent::Brand);
        assert!(action.collection.is_none());
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
            label: UserText::unlocalized(key),
            kind,
            default,
            placeholder: None,
            hint: None,
            required: false,
        }
    }

    fn qa_with(template: SubActionStep, fields: Vec<QuickActionField>) -> QuickAction {
        QuickAction {
            label: UserText::unlocalized("A"),
            icon: SectionIcon::new("x"),
            enabled: true,
            locked_reason: None,
            liveness: QuickActionLiveness::Unknown,
            group: None,
            destructive: false,
            accent: QuickActionAccent::default(),
            subaction_template: template,
            fields,
            collection: None,
        }
    }

    #[test]
    fn an_action_runs_unless_it_is_disabled_or_its_target_entity_is_known_absent() {
        let cases = [
            (true, QuickActionLiveness::Unknown, true),
            (true, QuickActionLiveness::Live, true),
            (true, QuickActionLiveness::Absent, false),
            (false, QuickActionLiveness::Unknown, false),
            (false, QuickActionLiveness::Live, false),
            (false, QuickActionLiveness::Absent, false),
        ];
        for (enabled, liveness, expected) in cases {
            let action = QuickAction {
                enabled,
                liveness,
                ..qa_with(qa_template(BTreeMap::new()), Vec::new())
            };

            assert_eq!(
                action.is_runnable(),
                expected,
                "enabled={enabled} liveness={liveness:?}"
            );
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
    fn marshal_scalar_kinds_produce_matching_scalar_variants() {
        let choice = QuickActionFieldKind::Choice(QuickActionChoiceSource::Static(vec![
            QuickActionChoiceOption {
                value: "primary".to_owned(),
                label: UserText::unlocalized("Primary"),
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
            (
                QuickActionFieldKind::Int { min: 15, max: 1800 },
                QuickActionFieldValue::Int(60),
                Variant::Int(60),
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

    #[test]
    fn option_list_choice_source_keeps_its_snake_case_wire_tag_through_a_round_trip() {
        let source = QuickActionChoiceSource::OptionList(OptionListId::new("obs.scenes"));
        let wire = serde_json::json!({ "option_list": "obs.scenes" });
        assert_eq!(serde_json::to_value(&source).unwrap(), wire);
        assert_eq!(
            serde_json::from_value::<QuickActionChoiceSource>(wire).unwrap(),
            source
        );
    }

    #[expect(dead_code, reason = "held only to keep the channel open")]
    fn dyn_all(
        _: &dyn BuiltinStatus,
        _: &dyn BuiltinHealth,
        _: &dyn BuiltinContent,
        _: &dyn QuickActions,
    ) {
    }

    #[test]
    fn a_code_missing_from_the_catalog_or_one_language_is_reported() {
        use crate::emitted_codes::{emitted_codes, uncatalogued};
        use forge_types::{MessageCatalog, MessageCode};

        const KNOWN: MessageCode = MessageCode::from_static("demo", "known");
        const UNTRANSLATED: MessageCode = MessageCode::from_static("demo", "untranslated");
        const STRANGER: MessageCode = MessageCode::from_static("demo", "stranger");
        const CATALOG: MessageCatalog = MessageCatalog {
            namespace: "demo",
            codes: &[KNOWN, UNTRANSLATED],
            en_ftl: "msg-demo-known = Known\nmsg-demo-untranslated = Untranslated\n",
            uk_ftl: "msg-demo-known = Vidomo\n",
        };
        let emitted = emitted_codes(&serde_json::json!(vec![
            UserText::from(KNOWN),
            UserText::from(UNTRANSLATED),
            UserText::from(STRANGER),
            UserText::unlocalized("demo.verbatim"),
        ]));

        assert_eq!(
            uncatalogued(&emitted, &[CATALOG]),
            ["demo.stranger", "demo.untranslated"]
        );
    }
}
