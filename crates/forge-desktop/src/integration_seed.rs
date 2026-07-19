use std::sync::Arc;
use std::time::Duration;

use forge_platform_core::{
    ActiveRow, BannerLevel, BuiltinContent, BuiltinHealth, BuiltinId, BuiltinStatus,
    CapabilityFlags, ConnectionState, ContentList, ContentListItem, DetailSection, HeaderAction,
    HealthBar, HealthLevel, HealthMetric, HealthStream, HealthValue, InfoField, KeyValueRow,
    ListFooter, PickerKind, QuickAction, QuickActions, SectionIcon, StatColumn, SubscriptionRow,
    SubscriptionStatus,
};
use forge_types::SubActionStep;

use forge_components::tr;

pub struct IntegrationSeed {
    pub icon: SectionIcon,
    pub status: Arc<dyn BuiltinStatus>,
    pub health: Arc<dyn BuiltinHealth>,
    pub content: Arc<dyn BuiltinContent>,
    pub quick: Arc<dyn QuickActions>,
}

struct SnapshotStatus {
    id: BuiltinId,
    display_name: String,
    version: Option<String>,
    endpoint: Option<String>,
    uptime: Option<Duration>,
    connection: ConnectionState,
    capability_flags: CapabilityFlags,
    header_actions: Vec<HeaderAction>,
}

impl BuiltinStatus for SnapshotStatus {
    fn id(&self) -> &BuiltinId {
        &self.id
    }
    fn display_name(&self) -> &str {
        &self.display_name
    }
    fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }
    fn connection(&self) -> ConnectionState {
        self.connection
    }
    fn uptime(&self) -> Option<Duration> {
        self.uptime
    }
    fn endpoint(&self) -> Option<&str> {
        self.endpoint.as_deref()
    }
    fn capability_flags(&self) -> CapabilityFlags {
        self.capability_flags.clone()
    }
    fn header_actions(&self) -> Vec<HeaderAction> {
        self.header_actions.clone()
    }
}

struct SnapshotHealth {
    metrics: [HealthMetric; 4],
}

impl BuiltinHealth for SnapshotHealth {
    fn metrics(&self) -> [HealthMetric; 4] {
        self.metrics.clone()
    }
    fn stream(&self) -> HealthStream {
        Box::pin(futures_util::stream::empty())
    }
}

struct SnapshotContent {
    sections: Vec<DetailSection>,
}

impl BuiltinContent for SnapshotContent {
    fn sections(&self) -> Vec<DetailSection> {
        self.sections.clone()
    }
}

struct SnapshotQuickActions {
    actions: Vec<QuickAction>,
}

impl QuickActions for SnapshotQuickActions {
    fn actions(&self) -> Vec<QuickAction> {
        self.actions.clone()
    }
}

fn text_metric(label: impl Into<String>, primary: &str, secondary: Option<&str>) -> HealthMetric {
    HealthMetric {
        label: label.into(),
        value: HealthValue::Text {
            primary: primary.to_owned(),
            secondary: secondary.map(ToOwned::to_owned),
        },
    }
}

fn status_metric(
    label: impl Into<String>,
    val: impl Into<String>,
    active: bool,
    detail: Option<&str>,
) -> HealthMetric {
    HealthMetric {
        label: label.into(),
        value: HealthValue::Status {
            label: val.into(),
            active,
            detail: detail.map(ToOwned::to_owned),
        },
    }
}

fn ratio_metric(
    label: impl Into<String>,
    used: u64,
    total: u64,
    reset_hint: Option<&str>,
) -> HealthMetric {
    HealthMetric {
        label: label.into(),
        value: HealthValue::Ratio {
            used,
            total,
            reset_hint: reset_hint.map(ToOwned::to_owned),
        },
    }
}

fn list_item(icon: &str, name: &str, active: bool, active_label: Option<&str>) -> ContentListItem {
    ContentListItem {
        icon: SectionIcon::new(icon),
        name: name.to_owned(),
        monospace_name: false,
        active,
        active_label: active_label.map(ToOwned::to_owned),
        trailing: Vec::new(),
        enabled: true,
    }
}

fn quick(
    label: impl Into<String>,
    icon: &str,
    kind_id: &str,
    picker: Option<PickerKind>,
    enabled: bool,
) -> QuickAction {
    QuickAction {
        label: label.into(),
        icon: SectionIcon::new(icon),
        enabled,
        subaction_template: SubActionStep {
            kind_id: kind_id.to_owned(),
            config: std::collections::BTreeMap::new(),
            enabled: true,
            continue_on_error: false,
            label: None,
        },
        picker,
    }
}

pub fn seed(id: &BuiltinId) -> IntegrationSeed {
    match id.as_str() {
        "twitch" => twitch(),
        "obs" => obs(),
        "kick" => kick(),
        "youtube" => generic(id, "YouTube", "brand-youtube"),
        "vtube" => generic(id, "VTube Studio", "mood-smile"),
        "discord" => generic(id, "Discord", "brand-discord"),
        "midi" => generic(id, "MIDI", "piano"),
        "hotkey" => generic(id, "Hotkeys", "keyboard"),
        _ => generic(id, "Integration", "broadcast"),
    }
}

fn assemble(
    icon: &str,
    status: SnapshotStatus,
    metrics: [HealthMetric; 4],
    sections: Vec<DetailSection>,
    actions: Vec<QuickAction>,
) -> IntegrationSeed {
    IntegrationSeed {
        icon: SectionIcon::new(icon),
        status: Arc::new(status),
        health: Arc::new(SnapshotHealth { metrics }),
        content: Arc::new(SnapshotContent { sections }),
        quick: Arc::new(SnapshotQuickActions { actions }),
    }
}

fn twitch() -> IntegrationSeed {
    let status = SnapshotStatus {
        id: BuiltinId::new("twitch"),
        display_name: "Twitch".to_owned(),
        version: Some("Helix v5".to_owned()),
        endpoint: Some("eventsub.wss.twitch.tv".to_owned()),
        uptime: Some(Duration::from_secs(8040)),
        connection: ConnectionState::Connected,
        capability_flags: CapabilityFlags {
            limited: false,
            label: None,
        },
        header_actions: vec![
            HeaderAction::Reconnect,
            HeaderAction::RefreshToken,
            HeaderAction::Disconnect,
        ],
    };
    let metrics = [
        status_metric(
            tr!("iseed_metric_chat"),
            "Connected",
            true,
            Some("tmi.twitch.tv"),
        ),
        text_metric(tr!("iseed_metric_messages"), "1,204", Some("peak 42/s")),
        ratio_metric(tr!("iseed_metric_eventsub"), 12, 12, None),
        ratio_metric(tr!("iseed_metric_api_budget"), 642, 800, Some("resets 60s")),
    ];
    let subs = DetailSection::SubscriptionList {
        title: tr!("iseed_section_eventsub_subs"),
        icon: SectionIcon::new("rss"),
        items: vec![
            SubscriptionRow {
                name: "channel.follow".to_owned(),
                status: SubscriptionStatus::Active,
                version: Some("v2".to_owned()),
                event_count: Some(1240),
                error_label: None,
            },
            SubscriptionRow {
                name: "channel.subscribe".to_owned(),
                status: SubscriptionStatus::Active,
                version: Some("v1".to_owned()),
                event_count: Some(88),
                error_label: None,
            },
            SubscriptionRow {
                name: "channel.cheer".to_owned(),
                status: SubscriptionStatus::Degraded,
                version: Some("v1".to_owned()),
                event_count: None,
                error_label: Some("retrying".to_owned()),
            },
            SubscriptionRow {
                name: "channel.raid".to_owned(),
                status: SubscriptionStatus::Active,
                version: Some("v1".to_owned()),
                event_count: Some(12),
                error_label: None,
            },
        ],
        footer: Some(ListFooter {
            cta_label: Some(tr!("iseed_cta_manage_subscriptions")),
            trailing_label: Some("4 topics".to_owned()),
        }),
    };
    let scopes = DetailSection::ScopesList {
        title: tr!("iseed_section_oauth_scopes"),
        scopes: vec![
            "chat:read".to_owned(),
            "chat:edit".to_owned(),
            "channel:read:subscriptions".to_owned(),
            "bits:read".to_owned(),
            "moderator:read:followers".to_owned(),
        ],
        footer: Some(ListFooter {
            cta_label: None,
            trailing_label: Some("5 granted".to_owned()),
        }),
    };
    let live = DetailSection::InfoCard {
        title: tr!("iseed_section_live_broadcast"),
        live: true,
        fields: vec![
            InfoField {
                label: tr!("iseed_field_viewers"),
                value: "1,204".to_owned(),
                monospace_value: false,
            },
            InfoField {
                label: tr!("iseed_field_category"),
                value: "Just Chatting".to_owned(),
                monospace_value: false,
            },
            InfoField {
                label: tr!("iseed_field_uptime"),
                value: "2h 14m".to_owned(),
                monospace_value: false,
            },
            InfoField {
                label: tr!("iseed_field_latency"),
                value: "2.1s".to_owned(),
                monospace_value: true,
            },
        ],
        health_bar: Some(HealthBar {
            fraction: 0.72,
            label: "72%".to_owned(),
            level: HealthLevel::Good,
        }),
    };
    let actions = vec![
        quick(
            tr!("iseed_action_run_ad"),
            "bolt",
            "twitch.ads.run",
            None,
            true,
        ),
        quick(
            tr!("iseed_action_create_clip"),
            "video",
            "twitch.clips.create",
            None,
            true,
        ),
        quick(
            tr!("iseed_action_commercial"),
            "broadcast",
            "twitch.ads.commercial",
            None,
            true,
        ),
        quick(
            tr!("iseed_action_shoutout"),
            "speakerphone",
            "twitch.chat.shoutout",
            None,
            true,
        ),
    ];
    assemble(
        "brand-twitch",
        status,
        metrics,
        vec![subs, scopes, live],
        actions,
    )
}

fn obs() -> IntegrationSeed {
    let status = SnapshotStatus {
        id: BuiltinId::new("obs"),
        display_name: "OBS Studio".to_owned(),
        version: Some("obs-websocket v5".to_owned()),
        endpoint: Some("ws://localhost:4455".to_owned()),
        uptime: Some(Duration::from_secs(2880)),
        connection: ConnectionState::Connected,
        capability_flags: CapabilityFlags {
            limited: false,
            label: None,
        },
        header_actions: vec![HeaderAction::Reconnect, HeaderAction::Disconnect],
    };
    let metrics = [
        status_metric(
            tr!("iseed_metric_websocket"),
            "Connected",
            true,
            Some("v5.5.4"),
        ),
        text_metric(tr!("iseed_scenes"), "6", None),
        status_metric(
            tr!("iseed_metric_streaming"),
            "Live",
            true,
            Some("6000 kb/s"),
        ),
        text_metric(tr!("iseed_dropped"), "0.2%", Some("stable")),
    ];
    let scenes = ContentList {
        title: tr!("iseed_scenes"),
        icon: SectionIcon::new("layout"),
        count_label: Some("4".to_owned()),
        items: vec![
            list_item("layout", "Main", true, Some("Live")),
            list_item("layout", "BRB", false, None),
            list_item("layout", "Gameplay", false, None),
            list_item("layout", "Ending", false, None),
        ],
        footer: None,
    };
    let sources = ContentList {
        title: tr!("iseed_sources"),
        icon: SectionIcon::new("device-desktop"),
        count_label: Some("4".to_owned()),
        items: vec![
            list_item("device-desktop", "Webcam", true, Some("On")),
            list_item("device-desktop", "Game Capture", true, Some("On")),
            list_item("device-desktop", "Chat Overlay", false, None),
            list_item("volume", "Mic/Aux", false, None),
        ],
        footer: None,
    };
    let stats = DetailSection::StatsGrid {
        title: tr!("iseed_section_stream_stats"),
        icon: SectionIcon::new("activity"),
        columns: vec![
            StatColumn {
                label: tr!("iseed_stat_bitrate"),
                value: "6000".to_owned(),
                subtitle: "kb/s".to_owned(),
            },
            StatColumn {
                label: tr!("iseed_stat_fps"),
                value: "60".to_owned(),
                subtitle: "target 60".to_owned(),
            },
            StatColumn {
                label: tr!("iseed_dropped"),
                value: "0.2%".to_owned(),
                subtitle: "stable".to_owned(),
            },
        ],
    };
    let actions = vec![
        quick(
            tr!("iseed_action_switch_scene"),
            "arrows-shuffle",
            "obs.scenes.switch_current",
            Some(PickerKind::Scene),
            true,
        ),
        quick(
            tr!("iseed_action_toggle_source"),
            "eye",
            "obs.sources.toggle",
            Some(PickerKind::Source),
            true,
        ),
        quick(
            tr!("iseed_action_record"),
            "record",
            "obs.record.start",
            None,
            true,
        ),
        quick(
            tr!("iseed_action_toggle_mute"),
            "volume",
            "obs.audio.toggle_mute",
            Some(PickerKind::Input),
            true,
        ),
    ];
    assemble(
        "broadcast",
        status,
        metrics,
        vec![
            DetailSection::TwoColumnLists {
                left: scenes,
                right: sources,
            },
            stats,
        ],
        actions,
    )
}

fn kick() -> IntegrationSeed {
    let status = SnapshotStatus {
        id: BuiltinId::new("kick"),
        display_name: "Kick".to_owned(),
        version: None,
        endpoint: Some("pusher.kick.com".to_owned()),
        uptime: Some(Duration::from_secs(3720)),
        connection: ConnectionState::Connected,
        capability_flags: CapabilityFlags {
            limited: true,
            label: Some(tr!("iseed_kick_capability")),
        },
        header_actions: vec![HeaderAction::Reconnect, HeaderAction::Disconnect],
    };
    let metrics = [
        status_metric(
            tr!("iseed_metric_chat"),
            "Connected",
            true,
            Some("pusher ws"),
        ),
        text_metric(tr!("iseed_metric_messages"), "312", None),
        text_metric(tr!("iseed_channel"), "streamer", None),
        text_metric(tr!("iseed_metric_mode"), "read via ws", None),
    ];
    let banner = DetailSection::WarningBanner {
        level: BannerLevel::Info,
        title: tr!("iseed_kick_banner_title"),
        body: tr!("iseed_kick_banner_body"),
        cta: None,
    };
    let channel = DetailSection::KeyValueList {
        title: tr!("iseed_channel"),
        icon: SectionIcon::new("user"),
        items: vec![
            KeyValueRow {
                icon: SectionIcon::new("user"),
                name: "streamer".to_owned(),
                tag: Some("id 4421".to_owned()),
                action: None,
            },
            KeyValueRow {
                icon: SectionIcon::new("users"),
                name: "12,004 followers".to_owned(),
                tag: None,
                action: None,
            },
            KeyValueRow {
                icon: SectionIcon::new("star"),
                name: "318 subscribers".to_owned(),
                tag: None,
                action: None,
            },
        ],
    };
    let actions = vec![
        quick(
            tr!("iseed_action_send_message"),
            "message",
            "kick.chat.send",
            None,
            true,
        ),
        quick(
            tr!("iseed_action_clear_chat"),
            "trash",
            "kick.chat.clear",
            None,
            true,
        ),
        quick(
            tr!("iseed_action_slow_mode"),
            "clock",
            "kick.chat.slow_mode",
            None,
            true,
        ),
        quick(
            tr!("iseed_action_ban_user"),
            "ban",
            "kick.mod.ban",
            None,
            false,
        ),
    ];
    assemble(
        "brand-kick",
        status,
        metrics,
        vec![banner, channel],
        actions,
    )
}

fn generic(id: &BuiltinId, display_name: &str, icon: &str) -> IntegrationSeed {
    let status = SnapshotStatus {
        id: id.clone(),
        display_name: display_name.to_owned(),
        version: None,
        endpoint: None,
        uptime: None,
        connection: ConnectionState::Disconnected,
        capability_flags: CapabilityFlags {
            limited: false,
            label: None,
        },
        header_actions: vec![HeaderAction::Reconnect],
    };
    let metrics = [
        status_metric(
            tr!("iseed_status"),
            tr!("common_status_not_connected"),
            false,
            None,
        ),
        text_metric(tr!("iseed_metric_activity"), "-", None),
        text_metric(tr!("iseed_metric_session"), "-", None),
        text_metric(tr!("iseed_metric_detail"), "-", None),
    ];
    let overview = DetailSection::ActiveItemList {
        title: tr!("iseed_section_overview"),
        icon: SectionIcon::new("info-circle"),
        items: vec![ActiveRow {
            name: tr!("iseed_generic_connect_hint"),
            active: false,
            mode_label: Some("idle".to_owned()),
        }],
    };
    let details = DetailSection::InfoCard {
        title: tr!("iseed_section_details"),
        live: false,
        fields: vec![
            InfoField {
                label: tr!("iseed_status"),
                value: tr!("common_status_not_connected"),
                monospace_value: false,
            },
            InfoField {
                label: tr!("iseed_field_since"),
                value: "-".to_owned(),
                monospace_value: true,
            },
        ],
        health_bar: None,
    };
    assemble(icon, status, metrics, vec![overview, details], Vec::new())
}
