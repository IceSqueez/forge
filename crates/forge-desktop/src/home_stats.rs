use forge_components::ForgePalette;
use forge_events::{Event, EventSource};
use forge_platform_core::BuiltinId;
use forge_runtime::LiveViewerCount;
use gpui::{Rgba, SharedString};

use crate::event_log::EventLog;
use crate::screen::Screen;

/// Em-dash rendered wherever a stat has no value yet — the empty state a runtime
/// read collapses to before its source is wired.
const EM_DASH: &str = "\u{2014}";

/// Upper bound on retained highlight rows. The card renders only the newest five, so
/// a small ring keeps the fold cheap under the live bus firehose.
const RECENT_CAP: usize = 50;

/// The five first-class integrations surfaced on Home: three chat platforms and
/// two stream apps. Each maps to a sidebar destination, a display label and a
/// brand-dot hue resolved from the active theme. The list is the whole Home
/// connections roster; a sixth (empty) grid slot is drawn by the view to preserve
/// the design's six-column rhythm.
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

    /// Stable builtin key routed into the generic integration detail screen.
    pub fn builtin_id(self) -> BuiltinId {
        BuiltinId::new(self.id_str())
    }

    /// The stable builtin id string for this integration.
    pub fn id_str(self) -> &'static str {
        match self {
            Integration::Twitch => "twitch",
            Integration::YouTube => "youtube",
            Integration::Kick => "kick",
            Integration::Obs => "obs",
            Integration::VTube => "vtube",
        }
    }

    /// Resolves an integration from a runtime `platform_id` / builtin id string, or
    /// `None` for an id outside the five-integration connectivity roster.
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

    /// Router destination a connection cell navigates to when clicked — the generic
    /// integration detail parameterized by this integration's builtin id.
    pub fn screen(self) -> Screen {
        Screen::BuiltinDetail(self.builtin_id())
    }

    /// Brand-mark hue, resolved from the active palette so the dot re-tints with the
    /// theme. Mirrors the design's integration grid: twitch=brand, youtube=random
    /// (red), kick=info (sky), obs=success (green), vtube=warning (yellow).
    pub fn dot_color(self, palette: &ForgePalette) -> Rgba {
        match self {
            Integration::Twitch => palette.brand,
            Integration::YouTube => palette.random,
            Integration::Kick => palette.info,
            Integration::Obs => palette.success,
            Integration::VTube => palette.warning,
        }
    }
}

/// Which source hue a recent-event row inks its dot and source label with. The
/// runtime's `EventSource` taxonomy is the eventual driver; the slice seeds a small
/// representative subset resolved to `ForgePalette` fields at render time.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SourceHue {
    Twitch,
    YouTube,
    Kick,
    Core,
}

impl SourceHue {
    /// Keys a highlight row to its source hue. The three chat platforms carry their
    /// brand hue; every other runtime source collapses to the core success hue.
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
            SourceHue::Twitch => palette.brand,
            SourceHue::YouTube => palette.random,
            SourceHue::Kick => palette.info,
            SourceHue::Core => palette.success,
        }
    }
}

/// One row of the Home recent-events card: a wall-clock time, the lowercased source
/// name, the event kind, a human summary and the hue keying its dot + source label.
/// Source-neutral so the row re-tints with the theme; the real feed maps bus events
/// into this shape once the observability stream reaches Home.
#[derive(Clone, Debug)]
pub struct HomeEvent {
    pub time: SharedString,
    pub source: SharedString,
    pub name: SharedString,
    pub desc: SharedString,
    pub hue: SourceHue,
}

impl HomeEvent {
    /// Decodes a notable observability event into a highlight row, or `None` for the
    /// high-frequency low-level kinds ([`is_home_notable`]) that belong only in the
    /// full event feed. The one-line summary is shared with the feed so both surfaces
    /// phrase an event identically.
    fn from_event(event: &Event) -> Option<Self> {
        if !is_home_notable(&event.kind) {
            return None;
        }
        let ts = event.timestamp;
        let time = format!("{:02}:{:02}:{:02}", ts.hour(), ts.minute(), ts.second());
        Some(Self {
            time: time.into(),
            source: source_label(event.source).into(),
            name: event.kind.clone().into(),
            desc: EventLog::summarize(event).into(),
            hue: SourceHue::from_source(event.source),
        })
    }
}

/// OBS stream-health readout backing the Home stream-health card. Present only when
/// OBS is connected (the card renders solely on `Some`). Carries preformatted
/// display strings so the view stays free of unit math; the real values flow from
/// the OBS health bridge once it reaches Home.
#[derive(Clone, Debug)]
pub struct ObsHealth {
    pub bitrate: SharedString,
    pub dropped: SharedString,
    pub dropped_pct: Option<SharedString>,
    /// True when zero frames dropped — inks the dropped value with the success hue.
    pub dropped_ok: bool,
    pub fps: SharedString,
    pub cpu: SharedString,
    /// Recent events-per-second history, oldest→newest, plotted by the throughput
    /// sparkline on the stream-health card.
    pub throughput: Vec<f32>,
}

/// Topic-scoped observable entity backing the Home screen, fed by the runtime→UI
/// bridge (the sole owner of the bus→UI edge). It holds Home's dashboard readout —
/// live viewers, action/global counts, the connection roster, recent events and the
/// optional OBS health — never runtime state of its own. The bridge advances it and
/// `cx.notify()`s so the observing [`crate::home::HomeView`] repaints.
///
/// Starts empty and live: the highlight reel and the fired-today counter are fed by
/// the boot-global bridge off the real bus, while the remaining fields (live viewers,
/// action/global counts, connection status, OBS health) collapse to their empty
/// state ("—", all-disconnected, no health card) until their own source bridges land
/// in later phases and drain through this same entity.
pub struct HomeStats {
    live_viewers: Option<u64>,
    actions_count: Option<usize>,
    triggers_fired: Option<u64>,
    globals_count: Option<usize>,
    connections: Vec<(Integration, bool)>,
    obs_health: Option<ObsHealth>,
    recent_events: Vec<HomeEvent>,
}

impl HomeStats {
    /// An empty, live-fed readout: every stat at its empty state and the full
    /// five-integration roster listed as disconnected (the roster is static UI
    /// knowledge; per-integration connection status arrives with the connectivity
    /// bridge in a later phase). Highlights and the fired-today counter fill in live
    /// over the bridge.
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
            triggers_fired: None,
            globals_count: None,
            connections,
            obs_health: None,
            recent_events: Vec::new(),
        }
    }

    /// Folds a bus event into the highlight reel, dropping the low-level kinds that
    /// belong only in the full feed and evicting the oldest row once the ring is full.
    /// Reports whether a row was actually appended so the bridge only repaints Home on
    /// a real change. Kept free of `cx` so it stays directly exercisable.
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

    /// Increments the fired-today counter on an `action.done` observability event.
    /// The bridge pairs this with `cx.notify()`; keeping the mutation free of `cx`
    /// leaves it directly exercisable. Dormant until the runtime publishes the event.
    pub fn record_action_done(&mut self) {
        self.triggers_fired = Some(self.triggers_fired.unwrap_or(0) + 1);
    }

    /// Applies the latest aggregate concurrent-viewer figure from the live-viewer
    /// bridge, mapping `Empty` (no connected platform reports) to the "—" empty state
    /// and `Reporting(n)` to a concrete count. Reports whether the value actually moved
    /// so the bridge only repaints Home on a real change. Kept free of `cx` so it stays
    /// directly exercisable.
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

    // --- display accessors (pure, kept off render for testability) -----------

    pub fn viewers_display(&self) -> String {
        self.live_viewers
            .map_or_else(|| EM_DASH.to_owned(), fmt_thousands)
    }

    pub fn actions_display(&self) -> String {
        self.actions_count
            .map_or_else(|| EM_DASH.to_owned(), |n| n.to_string())
    }

    pub fn triggers_fired_display(&self) -> String {
        self.triggers_fired
            .map_or_else(|| EM_DASH.to_owned(), fmt_thousands)
    }

    pub fn globals_display(&self) -> String {
        self.globals_count
            .map_or_else(|| EM_DASH.to_owned(), |n| n.to_string())
    }

    pub fn connected_count(&self) -> usize {
        self.connections.iter().filter(|(_, ok)| *ok).count()
    }

    pub fn total_count(&self) -> usize {
        self.connections.len()
    }

    /// True when not every integration is connected — drives the connections card's
    /// warning glyph.
    pub fn connections_warn(&self) -> bool {
        self.connected_count() < self.total_count()
    }

    /// Owned snapshot of the connection roster so the caller can build per-cell
    /// listeners without holding a borrow on the entity.
    pub fn connections_snapshot(&self) -> Vec<(Integration, bool)> {
        self.connections.clone()
    }

    pub fn obs_health_snapshot(&self) -> Option<ObsHealth> {
        self.obs_health.clone()
    }

    /// The newest `limit` events, newest first — the recent-events card's rows.
    pub fn recent(&self, limit: usize) -> Vec<HomeEvent> {
        self.recent_events
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }
}

/// Whether an event kind is a notable Home highlight rather than low-level firehose
/// noise. The high-frequency internals — per-sub-action runs, raw global writes,
/// inline script execs, and action starts — stay in the full event feed only;
/// everything else (action.done, command.matched, chat, platform social events,
/// scene changes, request failures) is a "something happened" row. Provisional slice
/// mapping until the production dashboard owns the curation.
fn is_home_notable(kind: &str) -> bool {
    !(kind.starts_with("subaction.")
        || kind == "action.start"
        || kind.starts_with("global.")
        || kind.starts_with("script."))
}

/// Lowercased source name for a highlight row's source label.
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

/// Formats a count with `,` thousands separators (`1284` → `"1,284"`). Best-effort
/// grouping for the dashboard readout; a locale-aware formatter lands with the real
/// dashboard pipeline.
fn fmt_thousands(n: u64) -> String {
    let digits = n.to_string();
    let len = digits.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}
