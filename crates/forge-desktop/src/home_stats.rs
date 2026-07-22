use forge_components::{ForgePalette, PlatformKind, fmt_number, platform_color};
use forge_events::{Event, EventSource};
use forge_platform_core::BuiltinId;
use forge_runtime::{LiveViewerCount, dashboard::DashboardStats};
use gpui::{Rgba, SharedString};

use crate::event_log::EventLog;
use crate::screen::Screen;

const NO_DATA: &str = "-";

const RECENT_CAP: usize = 50;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Integration {
    Twitch,
    YouTube,
    Kick,
    Obs,
    VTube,
}

impl Integration {
    pub fn label(self) -> &'static str {
        match self {
            Integration::Twitch => "Twitch",
            Integration::YouTube => "YouTube",
            Integration::Kick => "Kick",
            Integration::Obs => "OBS",
            Integration::VTube => "VTube",
        }
    }

    pub fn builtin_id(self) -> BuiltinId {
        BuiltinId::new(self.id_str())
    }

    pub fn id_str(self) -> &'static str {
        match self {
            Integration::Twitch => "twitch",
            Integration::YouTube => "youtube",
            Integration::Kick => "kick",
            Integration::Obs => "obs",
            Integration::VTube => "vtube",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "twitch" => Some(Integration::Twitch),
            "youtube" => Some(Integration::YouTube),
            "kick" => Some(Integration::Kick),
            "obs" => Some(Integration::Obs),
            "vtube" => Some(Integration::VTube),
            _ => None,
        }
    }

    pub fn screen(self) -> Screen {
        Screen::BuiltinDetail(self.builtin_id())
    }

    pub fn dot_color(self, palette: &ForgePalette) -> Rgba {
        match self {
            Integration::Twitch => platform_color(PlatformKind::Twitch, palette),
            Integration::YouTube => platform_color(PlatformKind::YouTube, palette),
            Integration::Kick => platform_color(PlatformKind::Kick, palette),
            Integration::Obs => palette.success,
            Integration::VTube => palette.warning,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SourceHue {
    Twitch,
    YouTube,
    Kick,
    Core,
}

impl SourceHue {
    fn from_source(source: EventSource) -> Self {
        match source {
            EventSource::Twitch => SourceHue::Twitch,
            EventSource::YouTube => SourceHue::YouTube,
            EventSource::Kick => SourceHue::Kick,
            _ => SourceHue::Core,
        }
    }

    pub fn color(self, palette: &ForgePalette) -> Rgba {
        match self {
            SourceHue::Twitch => platform_color(PlatformKind::Twitch, palette),
            SourceHue::YouTube => platform_color(PlatformKind::YouTube, palette),
            SourceHue::Kick => platform_color(PlatformKind::Kick, palette),
            SourceHue::Core => palette.success,
        }
    }
}

#[derive(Clone, Debug)]
pub struct HomeEvent {
    pub id: SharedString,
    pub time: SharedString,
    pub source: SharedString,
    pub name: SharedString,
    pub desc: SharedString,
    pub hue: SourceHue,
}

impl HomeEvent {
    fn from_event(event: &Event) -> Option<Self> {
        if !is_home_notable(&event.kind) {
            return None;
        }
        let ts = event.timestamp;
        let time = format!("{:02}:{:02}:{:02}", ts.hour(), ts.minute(), ts.second());
        Some(Self {
            id: event.id.to_string().into(),
            time: time.into(),
            source: source_label(event.source).into(),
            name: event.kind.clone().into(),
            desc: EventLog::summarize(event).into(),
            hue: SourceHue::from_source(event.source),
        })
    }
}

#[derive(Clone, Debug)]
pub struct ObsHealth {
    pub bitrate: SharedString,
    pub dropped: SharedString,
    pub dropped_pct: Option<SharedString>,
    pub dropped_ok: bool,
    pub fps: SharedString,
    pub cpu: SharedString,
    /// Events-per-second history, oldest first.
    pub throughput: Vec<f32>,
}

pub struct HomeStats {
    live_viewers: Option<u64>,
    actions_count: Option<usize>,
    commands_count: Option<usize>,
    triggers_fired: Option<u64>,
    globals_count: Option<usize>,
    connections: Vec<(Integration, bool)>,
    obs_health: Option<ObsHealth>,
    recent_events: Vec<HomeEvent>,
}

impl HomeStats {
    pub fn new() -> Self {
        let connections = vec![
            (Integration::Twitch, false),
            (Integration::YouTube, false),
            (Integration::Kick, false),
            (Integration::Obs, false),
            (Integration::VTube, false),
        ];

        Self {
            live_viewers: None,
            actions_count: None,
            commands_count: None,
            triggers_fired: None,
            globals_count: None,
            connections,
            obs_health: None,
            recent_events: Vec::new(),
        }
    }

    pub fn record_event(&mut self, event: &Event) -> bool {
        let Some(home_event) = HomeEvent::from_event(event) else {
            return false;
        };
        if self.recent_events.len() >= RECENT_CAP {
            self.recent_events.remove(0);
        }
        self.recent_events.push(home_event);
        true
    }

    pub fn record_action_done(&mut self) {
        self.triggers_fired = Some(self.triggers_fired.unwrap_or(0) + 1);
    }

    pub fn set_stats(&mut self, stats: DashboardStats) -> bool {
        let next_actions = Some(stats.actions_count);
        let next_commands = Some(stats.commands_count);
        let next_fired = Some(stats.triggers_fired);
        let next_globals = Some(stats.globals_count);
        if self.actions_count == next_actions
            && self.commands_count == next_commands
            && self.triggers_fired == next_fired
            && self.globals_count == next_globals
        {
            return false;
        }
        self.actions_count = next_actions;
        self.commands_count = next_commands;
        self.triggers_fired = next_fired;
        self.globals_count = next_globals;
        true
    }

    pub fn set_live_viewers(&mut self, count: LiveViewerCount) -> bool {
        let next = match count {
            LiveViewerCount::Reporting(n) => Some(n),
            LiveViewerCount::Empty => None,
        };
        if self.live_viewers == next {
            return false;
        }
        self.live_viewers = next;
        true
    }

    pub fn viewers_display(&self) -> String {
        self.live_viewers
            .map_or_else(|| NO_DATA.to_owned(), |n| fmt_number(n as f64, 0))
    }

    pub fn actions_display(&self) -> String {
        self.actions_count
            .map_or_else(|| NO_DATA.to_owned(), |n| n.to_string())
    }

    pub fn commands_display(&self) -> String {
        self.commands_count
            .map_or_else(|| NO_DATA.to_owned(), |n| n.to_string())
    }

    pub fn triggers_fired_display(&self) -> String {
        self.triggers_fired
            .map_or_else(|| NO_DATA.to_owned(), |n| fmt_number(n as f64, 0))
    }

    pub fn globals_display(&self) -> String {
        self.globals_count
            .map_or_else(|| NO_DATA.to_owned(), |n| n.to_string())
    }

    pub fn connected_count(&self) -> usize {
        self.connections.iter().filter(|(_, ok)| *ok).count()
    }

    pub fn total_count(&self) -> usize {
        self.connections.len()
    }

    pub fn connections_warn(&self) -> bool {
        self.connected_count() < self.total_count()
    }

    pub fn connections_snapshot(&self) -> Vec<(Integration, bool)> {
        self.connections.clone()
    }

    pub fn obs_health_snapshot(&self) -> Option<ObsHealth> {
        self.obs_health.clone()
    }

    pub fn recent(&self, limit: usize) -> Vec<HomeEvent> {
        self.recent_events
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }
}

fn is_home_notable(kind: &str) -> bool {
    !(kind.starts_with("subaction.")
        || kind == "action.start"
        || kind.starts_with("global.")
        || kind.starts_with("script."))
}

fn source_label(source: EventSource) -> &'static str {
    match source {
        EventSource::Twitch => "twitch",
        EventSource::YouTube => "youtube",
        EventSource::Kick => "kick",
        EventSource::Core => "core",
        EventSource::Rhai => "rhai",
        EventSource::Http => "http",
        EventSource::Obs => "obs",
        EventSource::VTube => "vtube",
        EventSource::Discord => "discord",
        EventSource::Midi => "midi",
        EventSource::Hotkey => "hotkey",
        EventSource::Timer => "timer",
        EventSource::Server => "server",
        EventSource::Audio => "audio",
    }
}
