use std::collections::VecDeque;

use crate::platform_visuals::platform_color;
use forge_components::{ForgePalette, fmt_number, fmt_uptime, tr};
use forge_events::{Event, EventSource};
use forge_obs::ObsStreamStats;
use forge_runtime::{DashboardStats, LiveViewerCount};
use forge_types::{IntegrationId, PlatformId};
use gpui::{Rgba, SharedString};
use time::OffsetDateTime;

use crate::event_log::EventLog;
use crate::navigation::Screen;
use crate::runtime_event::RuntimeEvent;

const NO_DATA: &str = "-";

const RECENT_CAP: usize = 50;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Integration {
    Twitch,
    YouTube,
    Kick,
    Obs,
    VTube,
}

impl Integration {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Integration::Twitch => "Twitch",
            Integration::YouTube => "YouTube",
            Integration::Kick => "Kick",
            Integration::Obs => "OBS",
            Integration::VTube => "VTube",
        }
    }

    pub(crate) fn builtin_id(self) -> IntegrationId {
        IntegrationId::new(self.id_str())
    }

    pub(crate) fn id_str(self) -> &'static str {
        match self {
            Integration::Twitch => "twitch",
            Integration::YouTube => "youtube",
            Integration::Kick => "kick",
            Integration::Obs => "obs",
            Integration::VTube => "vtube",
        }
    }

    pub(crate) fn from_id(id: &str) -> Option<Self> {
        match id {
            "twitch" => Some(Integration::Twitch),
            "youtube" => Some(Integration::YouTube),
            "kick" => Some(Integration::Kick),
            "obs" => Some(Integration::Obs),
            "vtube" => Some(Integration::VTube),
            _ => None,
        }
    }

    pub(crate) fn screen(self) -> Screen {
        Screen::BuiltinDetail(self.builtin_id())
    }

    pub(crate) fn dot_color(self, palette: &ForgePalette) -> Rgba {
        match self {
            Integration::Twitch => platform_color(PlatformId::Twitch, palette),
            Integration::YouTube => platform_color(PlatformId::YouTube, palette),
            Integration::Kick => platform_color(PlatformId::Kick, palette),
            Integration::Obs => palette.success,
            Integration::VTube => palette.warning,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SourceHue {
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

    pub(crate) fn color(self, palette: &ForgePalette) -> Rgba {
        match self {
            SourceHue::Twitch => platform_color(PlatformId::Twitch, palette),
            SourceHue::YouTube => platform_color(PlatformId::YouTube, palette),
            SourceHue::Kick => platform_color(PlatformId::Kick, palette),
            SourceHue::Core => palette.success,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct HomeEvent {
    pub id: SharedString,
    pub at: OffsetDateTime,
    pub source: SharedString,
    pub name: SharedString,
    pub desc: SharedString,
    pub hue: SourceHue,
}

impl HomeEvent {
    fn from_event(event: &Event, typed: Option<&RuntimeEvent>) -> Option<Self> {
        if typed.is_some_and(RuntimeEvent::is_run_bookkeeping) {
            return None;
        }
        Some(Self {
            id: event.id.to_string().into(),
            at: event.timestamp,
            source: source_label(event.source).into(),
            name: event.kind.clone().into(),
            desc: EventLog::summarize(event, typed).into(),
            hue: SourceHue::from_source(event.source),
        })
    }
}

const THROUGHPUT_WINDOW_SECS: i64 = 60;
const HEALTHY_DROPPED_PERCENT: f64 = 1.0;
const PERCENT_SCALE: f64 = 100.0;
const NO_READING: &str = "-";

#[derive(Default)]
struct ThroughputWindow {
    seconds: VecDeque<(i64, u32)>,
}

impl ThroughputWindow {
    fn record(&mut self, second: i64) {
        if let Some(bucket) = self.seconds.iter_mut().rev().find(|(at, _)| *at == second) {
            bucket.1 += 1;
            return;
        }
        let position = self.seconds.partition_point(|(at, _)| *at < second);
        self.seconds.insert(position, (second, 1));
        let newest = self.seconds.back().map_or(second, |(at, _)| *at);
        while self
            .seconds
            .front()
            .is_some_and(|(at, _)| newest - *at >= THROUGHPUT_WINDOW_SECS)
        {
            self.seconds.pop_front();
        }
    }

    fn samples(&self, now_second: i64) -> Vec<f32> {
        let oldest = now_second - THROUGHPUT_WINDOW_SECS + 1;
        (oldest..=now_second)
            .map(|second| {
                self.seconds
                    .iter()
                    .find(|(at, _)| *at == second)
                    .map_or(0.0, |(_, count)| *count as f32)
            })
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ObsHealth {
    pub live_for: Option<String>,
    pub bitrate: String,
    pub bitrate_known: bool,
    pub dropped: String,
    pub dropped_pct: Option<String>,
    pub dropped_ok: bool,
    pub fps: String,
    pub cpu: String,
    pub throughput: Vec<f32>,
}

impl ObsHealth {
    fn from_stats(stats: ObsStreamStats, throughput: Vec<f32>) -> Self {
        let live = stats.stream_active;
        let dropped_share = (live && stats.total_frames > 0)
            .then(|| stats.dropped_frames as f64 / stats.total_frames as f64 * PERCENT_SCALE);
        Self {
            live_for: live.then(|| {
                let duration = fmt_uptime(stats.stream_duration.unwrap_or_default().as_secs());
                tr!("home_health_live_for", duration = duration.as_str())
            }),
            bitrate: stats.bitrate_kbps.filter(|_| live).map_or_else(
                || NO_READING.to_owned(),
                |kbps| fmt_number(f64::from(kbps), 0),
            ),
            bitrate_known: live && stats.bitrate_kbps.is_some(),
            dropped: if live {
                stats.dropped_frames.to_string()
            } else {
                NO_READING.to_owned()
            },
            dropped_pct: dropped_share.map(|share| format!("{}%", fmt_number(share, 2))),
            dropped_ok: dropped_share.is_none_or(|share| share < HEALTHY_DROPPED_PERCENT),
            fps: fmt_number(stats.fps, 1),
            cpu: fmt_number(stats.cpu_percent, 1),
            throughput,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConnectionsTally {
    pub connections: Vec<(Integration, bool)>,
    pub connected: usize,
    pub total: usize,
}

impl ConnectionsTally {
    pub(crate) fn warn(&self) -> bool {
        self.connected < self.total
    }
}

pub(crate) struct HomeStats {
    live_viewers: Option<u64>,
    actions_count: Option<usize>,
    commands_count: Option<usize>,
    triggers_fired: Option<u64>,
    globals_count: Option<usize>,
    connections: Vec<(Integration, bool)>,
    obs_health: Option<ObsHealth>,
    throughput: ThroughputWindow,
    recent_events: Vec<HomeEvent>,
}

impl HomeStats {
    pub(crate) fn new() -> Self {
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
            throughput: ThroughputWindow::default(),
            recent_events: Vec::new(),
        }
    }

    pub(crate) fn set_obs_stats(&mut self, stats: Option<ObsStreamStats>, now: OffsetDateTime) {
        let samples = self.throughput.samples(now.unix_timestamp());
        self.obs_health = stats.map(|stats| ObsHealth::from_stats(stats, samples));
    }

    pub(crate) fn record_event(&mut self, event: &Event, typed: Option<&RuntimeEvent>) -> bool {
        self.throughput.record(event.timestamp.unix_timestamp());
        let Some(home_event) = HomeEvent::from_event(event, typed) else {
            return false;
        };
        if self.recent_events.len() >= RECENT_CAP {
            self.recent_events.remove(0);
        }
        self.recent_events.push(home_event);
        true
    }

    pub(crate) fn record_action_done(&mut self) {
        self.triggers_fired = Some(self.triggers_fired.unwrap_or(0) + 1);
    }

    pub(crate) fn set_stats(&mut self, stats: DashboardStats) -> bool {
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

    pub(crate) fn set_live_viewers(&mut self, count: LiveViewerCount) -> bool {
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

    pub(crate) fn viewers_display(&self) -> String {
        self.live_viewers
            .map_or_else(|| NO_DATA.to_owned(), |n| fmt_number(n as f64, 0))
    }

    pub(crate) fn actions_display(&self) -> String {
        self.actions_count
            .map_or_else(|| NO_DATA.to_owned(), |n| n.to_string())
    }

    pub(crate) fn commands_display(&self) -> String {
        self.commands_count
            .map_or_else(|| NO_DATA.to_owned(), |n| n.to_string())
    }

    pub(crate) fn triggers_fired_display(&self) -> String {
        self.triggers_fired
            .map_or_else(|| NO_DATA.to_owned(), |n| fmt_number(n as f64, 0))
    }

    pub(crate) fn globals_display(&self) -> String {
        self.globals_count
            .map_or_else(|| NO_DATA.to_owned(), |n| n.to_string())
    }

    pub(crate) fn enabled_connections(
        &self,
        is_enabled: impl Fn(Integration) -> bool,
    ) -> ConnectionsTally {
        let connections: Vec<(Integration, bool)> = self
            .connections
            .iter()
            .copied()
            .filter(|(integ, _)| is_enabled(*integ))
            .collect();
        ConnectionsTally {
            connected: connections.iter().filter(|(_, ok)| *ok).count(),
            total: connections.len(),
            connections,
        }
    }

    pub(crate) fn is_connected(&self, integration: Integration) -> bool {
        self.connections
            .iter()
            .any(|(integ, connected)| *integ == integration && *connected)
    }

    pub(crate) fn set_connections(&mut self, connections: &[(Integration, bool)]) -> bool {
        if self.connections == connections {
            return false;
        }
        self.connections = connections.to_vec();
        true
    }

    pub(crate) fn obs_health_snapshot(&self) -> Option<ObsHealth> {
        self.obs_health.clone()
    }

    pub(crate) fn recent(&self, limit: usize) -> Vec<HomeEvent> {
        self.recent_events
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }
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
        EventSource::Donation => "donation",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats_with(connections: Vec<(Integration, bool)>) -> HomeStats {
        HomeStats {
            connections,
            ..HomeStats::new()
        }
    }

    #[test]
    fn the_tally_counts_only_enabled_integrations() {
        let stats = stats_with(vec![
            (Integration::Twitch, true),
            (Integration::Kick, false),
            (Integration::Obs, true),
            (Integration::VTube, false),
        ]);

        let tally = stats.enabled_connections(|integ| integ != Integration::Obs);

        assert_eq!(
            tally,
            ConnectionsTally {
                connections: vec![
                    (Integration::Twitch, true),
                    (Integration::Kick, false),
                    (Integration::VTube, false),
                ],
                connected: 1,
                total: 3,
            }
        );
    }

    #[test]
    fn set_connections_reports_a_change_only_when_the_slice_differs() {
        let mut stats = stats_with(vec![
            (Integration::Twitch, false),
            (Integration::Obs, false),
        ]);
        let live = [(Integration::Twitch, false), (Integration::Obs, true)];

        let unchanged =
            stats.set_connections(&[(Integration::Twitch, false), (Integration::Obs, false)]);
        let changed = stats.set_connections(&live);
        let repeated = stats.set_connections(&live);

        assert_eq!((unchanged, changed, repeated), (false, true, false));
    }

    #[test]
    fn set_connections_feeds_the_enabled_tally() {
        let mut stats = stats_with(vec![
            (Integration::Twitch, false),
            (Integration::Obs, false),
        ]);

        stats.set_connections(&[(Integration::Twitch, true), (Integration::Obs, false)]);

        let tally = stats.enabled_connections(|_| true);
        assert_eq!((tally.connected, tally.total), (1, 2));
    }

    #[test]
    fn the_tally_warns_only_while_an_enabled_integration_is_down() {
        for (connections, warn, case) in [
            (
                vec![(Integration::Twitch, true), (Integration::Obs, false)],
                true,
                "one enabled integration down",
            ),
            (
                vec![(Integration::Twitch, true), (Integration::Obs, true)],
                false,
                "every enabled integration connected",
            ),
            (Vec::new(), false, "nothing enabled"),
        ] {
            let tally = stats_with(connections).enabled_connections(|_| true);

            assert_eq!(tally.warn(), warn, "{case}");
        }
    }

    const NOW: i64 = 1_000;
    const WINDOW: usize = THROUGHPUT_WINDOW_SECS as usize;

    fn window_of(seconds: &[i64]) -> ThroughputWindow {
        let mut window = ThroughputWindow::default();
        for second in seconds {
            window.record(*second);
        }
        window
    }

    fn live(dropped_frames: u64, total_frames: u64) -> ObsStreamStats {
        ObsStreamStats {
            stream_active: true,
            dropped_frames,
            total_frames,
            ..ObsStreamStats::default()
        }
    }

    #[test]
    fn out_of_order_events_land_in_the_second_they_happened() {
        let window = window_of(&[NOW - 2, NOW, NOW - 1, NOW - 1, NOW - 2, NOW - 2]);

        assert_eq!(window.samples(NOW)[WINDOW - 3..], [3.0, 2.0, 1.0]);
    }

    #[test]
    fn the_window_keeps_fifty_nine_seconds_behind_the_newest_event_and_prunes_older() {
        let at_the_edge = window_of(&[NOW - 59, NOW]);
        let past_the_edge = window_of(&[NOW - 60, NOW]);
        let arriving_too_late = window_of(&[NOW, NOW - 60]);

        assert_eq!(at_the_edge.seconds.front().copied(), Some((NOW - 59, 1)));
        assert_eq!(past_the_edge.seconds.front().copied(), Some((NOW, 1)));
        assert_eq!(arriving_too_late.seconds.len(), 1);
    }

    #[test]
    fn samples_cover_sixty_seconds_ending_at_now() {
        let samples = window_of(&[NOW - 59, NOW]).samples(NOW);

        assert_eq!(samples.len(), WINDOW);
        assert_eq!(
            (samples[0], samples[WINDOW - 1], samples.iter().sum::<f32>()),
            (1.0, 1.0, 2.0)
        );
    }

    #[test]
    fn samples_leave_out_seconds_outside_the_sixty_ending_at_now() {
        let too_old = window_of(&[NOW - 60, NOW - 1]).samples(NOW);
        let still_ahead = window_of(&[NOW, NOW + 1]).samples(NOW);

        assert_eq!(too_old.iter().sum::<f32>(), 1.0);
        assert_eq!(still_ahead.iter().sum::<f32>(), 1.0);
    }

    #[test]
    fn an_empty_window_still_yields_sixty_zero_samples() {
        assert_eq!(ThroughputWindow::default().samples(NOW), vec![0.0; WINDOW]);
    }

    #[test]
    fn a_live_stream_reports_every_reading_with_its_units() {
        crate::i18n::install_language(forge_storage::Language::En);
        let stats = ObsStreamStats {
            stream_active: true,
            stream_duration: Some(std::time::Duration::from_secs(3_725)),
            bitrate_kbps: Some(600),
            dropped_frames: 5,
            total_frames: 1_000,
            fps: 59.94,
            cpu_percent: 12.5,
        };

        let mut health = ObsHealth::from_stats(stats, vec![1.0]);
        health.live_for = health
            .live_for
            .map(|live_for| live_for.replace(['\u{2068}', '\u{2069}'], ""));

        assert_eq!(
            health,
            ObsHealth {
                live_for: Some("LIVE 1h 2m".to_owned()),
                bitrate: "600".to_owned(),
                bitrate_known: true,
                dropped: "5".to_owned(),
                dropped_pct: Some("0.50%".to_owned()),
                dropped_ok: true,
                fps: "59.9".to_owned(),
                cpu: "12.5".to_owned(),
                throughput: vec![1.0],
            }
        );
    }

    #[test]
    fn a_stream_that_is_not_live_hides_its_stream_readings_and_keeps_the_app_ones() {
        crate::i18n::install_language(forge_storage::Language::En);
        let stats = ObsStreamStats {
            dropped_frames: 90,
            total_frames: 100,
            fps: 30.0,
            cpu_percent: 2.0,
            ..ObsStreamStats::default()
        };

        assert_eq!(
            ObsHealth::from_stats(stats, Vec::new()),
            ObsHealth {
                live_for: None,
                bitrate: "-".to_owned(),
                bitrate_known: false,
                dropped: "-".to_owned(),
                dropped_pct: None,
                dropped_ok: true,
                fps: "30.0".to_owned(),
                cpu: "2.0".to_owned(),
                throughput: Vec::new(),
            }
        );
    }

    #[test]
    fn a_dropped_share_stops_being_healthy_at_one_percent() {
        crate::i18n::install_language(forge_storage::Language::En);
        for (dropped, healthy) in [(99, true), (100, false), (101, false)] {
            let health = ObsHealth::from_stats(live(dropped, 10_000), Vec::new());

            assert_eq!(health.dropped_ok, healthy, "{dropped} of 10000");
        }
    }

    #[test]
    fn a_live_stream_with_no_frames_yet_has_no_dropped_share() {
        crate::i18n::install_language(forge_storage::Language::En);

        let health = ObsHealth::from_stats(live(0, 0), Vec::new());

        assert_eq!((health.dropped_pct, health.dropped_ok), (None, true));
    }

    #[test]
    fn losing_the_stats_clears_the_health_card() {
        crate::i18n::install_language(forge_storage::Language::En);
        let mut stats = HomeStats::new();
        let now = OffsetDateTime::UNIX_EPOCH;
        stats.set_obs_stats(Some(live(0, 0)), now);
        assert!(stats.obs_health.is_some());

        stats.set_obs_stats(None, now);

        assert_eq!(stats.obs_health, None);
    }

    #[test]
    fn the_health_card_carries_the_recent_throughput() {
        crate::i18n::install_language(forge_storage::Language::En);
        let mut stats = HomeStats::new();
        let now = OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(NOW);
        stats.throughput.record(NOW);

        stats.set_obs_stats(Some(live(0, 0)), now);

        let throughput = stats.obs_health.map(|health| health.throughput);
        assert_eq!(
            throughput.and_then(|samples| samples.last().copied()),
            Some(1.0)
        );
    }
}
