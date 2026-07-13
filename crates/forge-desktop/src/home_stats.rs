use forge_components::ForgePalette;
use forge_platform_core::BuiltinId;
use gpui::{Rgba, SharedString};

use crate::screen::Screen;

/// Em-dash rendered wherever a stat has no value yet — the empty state a runtime
/// read collapses to before its source is wired.
const EM_DASH: &str = "\u{2014}";

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
        BuiltinId::new(match self {
            Integration::Twitch => "twitch",
            Integration::YouTube => "youtube",
            Integration::Kick => "kick",
            Integration::Obs => "obs",
            Integration::VTube => "vtube",
        })
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
    fn new(
        time: &'static str,
        source: &'static str,
        name: &'static str,
        desc: &'static str,
        hue: SourceHue,
    ) -> Self {
        Self {
            time: time.into(),
            source: source.into(),
            name: name.into(),
            desc: desc.into(),
            hue,
        }
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
/// Seeded at boot with a representative sample so the screen renders visibly before
/// any runtime dashboard source exists; each field is replaced as its real source
/// (the dashboard-stats query, the live-viewer aggregate, the connection-health and
/// OBS-health bridges) lands and drains through this same entity.
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
    /// Representative starter readout — clearly a slice stub. Two of five
    /// integrations connected (Twitch + OBS), so the connections card warns and the
    /// stream-health card renders; five recent events; dashboard counts mirroring
    /// the design mockup. Real bridges replace each field as they land.
    pub fn seeded() -> Self {
        let connections = vec![
            (Integration::Twitch, true),
            (Integration::YouTube, false),
            (Integration::Kick, false),
            (Integration::Obs, true),
            (Integration::VTube, false),
        ];

        let obs_health = Some(ObsHealth {
            bitrate: "6,200".into(),
            dropped: "0".into(),
            dropped_pct: Some("0.00%".into()),
            dropped_ok: true,
            fps: "60.0".into(),
            cpu: "8.2".into(),
            throughput: vec![
                12.0, 14.0, 11.0, 16.0, 13.0, 18.0, 15.0, 20.0, 17.0, 14.0, 19.0, 16.0, 21.0, 18.0,
                15.0, 13.0, 17.0, 20.0, 16.0, 12.0, 14.0, 18.0, 22.0, 19.0, 15.0, 11.0, 13.0, 17.0,
                16.0, 14.0,
            ],
        });

        // Oldest first; the card shows the newest five reversed.
        let recent_events = vec![
            HomeEvent::new(
                "14:02:19",
                "twitch",
                "command.matched",
                "!discord by nightbot42",
                SourceHue::Twitch,
            ),
            HomeEvent::new(
                "14:02:41",
                "youtube",
                "subscription",
                "maksym_dn subscribed at tier 1",
                SourceHue::YouTube,
            ),
            HomeEvent::new(
                "14:03:04",
                "twitch",
                "cheer",
                "haash_ cheered 500 bits",
                SourceHue::Twitch,
            ),
            HomeEvent::new(
                "14:03:18",
                "core",
                "action.done",
                "raid-welcome fired",
                SourceHue::Core,
            ),
            HomeEvent::new(
                "14:03:29",
                "kick",
                "chat.message",
                "stream_fan_kyiv checking in",
                SourceHue::Kick,
            ),
        ];

        Self {
            live_viewers: Some(1284),
            actions_count: Some(47),
            triggers_fired: Some(1284),
            globals_count: Some(31),
            connections,
            obs_health,
            recent_events,
        }
    }

    /// Increments the fired-today counter on an `action.done` observability event.
    /// The bridge pairs this with `cx.notify()`; keeping the mutation free of `cx`
    /// leaves it directly exercisable. Dormant until the runtime publishes the event.
    pub fn record_action_done(&mut self) {
        self.triggers_fired = Some(self.triggers_fired.unwrap_or(0) + 1);
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
