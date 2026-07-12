use forge_events::{Event, EventSource};
use gpui::SharedString;

/// Upper bound on retained feed rows. The screen renders every filtered row into a
/// flat column (no virtualization yet), and the boot tick publisher appends one row
/// per second, so the ring is bounded to keep the repaint cheap. The real 10,000
/// capacity + a virtualized list land with the production event pipeline.
const RING_CAP: usize = 500;

/// Which slice of the stream a filter tab keeps. Ported 1:1 from the shipping feed:
/// each arm is a pure predicate over an event's source + kind, so the tab counts and
/// the visible list stay side-effect-free and directly testable.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum EventFilter {
    #[default]
    All,
    Chat,
    Subs,
    Bits,
    Timers,
    Obs,
    Errors,
}

/// True for the chat-message kinds every chat platform publishes under its own
/// namespace. Mirrors the shipping feed's kind set so the Chat tab and the chat ink
/// agree across Twitch / YouTube / Kick.
fn is_chat_message_kind(kind: &str) -> bool {
    matches!(
        kind,
        "chat.message"
            | "youtube.chat.message"
            | "kick.chat.message"
            | "youtube.chat.command"
            | "kick.chat.command"
    )
}

/// One decoded row held by the [`EventLog`] topic. Carries the source-neutral facts
/// a feed row and the inspector need (identity, wall-clock stamp, source, kind,
/// summary, optional result tag, error flag, and an optional acting user for the
/// inspector payload) but NO resolved color: the source / type / result inks are
/// derived from the active theme at render time so a row re-tints on theme switch.
#[derive(Clone, Debug)]
pub struct EventItem {
    /// Stable identity used for selection and the inspector's `#last6` tag. A real
    /// event carries its `EventId` string; seed rows carry a synthetic id.
    pub id: SharedString,
    pub timestamp: SharedString,
    pub source: EventSource,
    pub kind: SharedString,
    pub summary: SharedString,
    pub result_tag: Option<SharedString>,
    pub is_error: bool,
    /// Acting user login for the inspector payload block, empty when the event has
    /// no user (timer ticks, scene changes, …).
    pub user_login: SharedString,
    /// Acting user's platform label for the inspector payload, empty when absent.
    pub user_platform: SharedString,
}

impl EventItem {
    /// Whether this row survives `filter`. A pure predicate over source + kind —
    /// the same taxonomy the shipping feed uses.
    pub fn matches(&self, filter: EventFilter) -> bool {
        let k = self.kind.as_ref();
        match filter {
            EventFilter::All => true,
            EventFilter::Chat => k.contains("chat") || k.contains("command"),
            EventFilter::Subs => {
                k.contains("sub") || k.contains("subscription") || k.contains("follow")
            }
            EventFilter::Bits => k.contains("cheer") || k.contains("bits") || k.contains("raid"),
            EventFilter::Timers => self.source == EventSource::Timer || k.contains("timer"),
            EventFilter::Obs => {
                self.source == EventSource::Obs || k.contains("scene") || k.contains("obs")
            }
            EventFilter::Errors => k.contains("error") || k.contains("fail"),
        }
    }

    fn seed(
        id: &'static str,
        timestamp: &'static str,
        source: EventSource,
        kind: &'static str,
        summary: &'static str,
        result_tag: Option<&'static str>,
        user: (&'static str, &'static str),
    ) -> Self {
        let is_error = kind.contains("error") || kind.contains("fail");
        Self {
            id: id.into(),
            timestamp: timestamp.into(),
            source,
            kind: kind.into(),
            summary: summary.into(),
            result_tag: result_tag.map(Into::into),
            is_error,
            user_login: user.0.into(),
            user_platform: user.1.into(),
        }
    }
}

/// Topic-scoped observable entity fed by the runtime→UI bridge: the sole owner of
/// the runtime-events→UI edge for the feed. The bridge drains the event bus, decodes
/// each observability event through [`EventLog::item_from_event`], appends it, then
/// `cx.notify()`s so the observing feed screen repaints. Holds no runtime state of
/// its own — only the rows it has been handed, a capacity ring, and a paused flag.
///
/// Seeded at boot with a representative sample so the screen renders visibly before
/// any real runtime traffic; live events (the boot tick publisher, and real
/// integrations once they publish) append through the same [`EventLog::push`] path.
pub struct EventLog {
    items: Vec<EventItem>,
    /// When set, [`EventLog::push`] drops incoming rows — the screen's Pause toggle
    /// flips it, matching the shipping feed where pausing stops collection.
    paused: bool,
}

impl EventLog {
    /// A representative starter set spanning the source + type taxonomy the filter
    /// tabs slice on (chat, command, subscription, cheer, timer, scene, action
    /// lifecycle, a global write, and a failed request). Clearly a slice stub — real
    /// traffic streams in through [`EventLog::push`] over the bridge.
    pub fn seeded() -> Self {
        let items = vec![
            EventItem::seed(
                "seed-00000001",
                "14:02:07.184",
                EventSource::Twitch,
                "chat.message",
                "koval_dev: Hello everyone! Stream looks great",
                Some("\u{2192} 1 action"),
                ("koval_dev", "twitch"),
            ),
            EventItem::seed(
                "seed-00000002",
                "14:02:11.902",
                EventSource::Core,
                "command.matched",
                "!quote by danylo_ua",
                Some("\u{2192} trigger fired"),
                ("danylo_ua", "twitch"),
            ),
            EventItem::seed(
                "seed-00000003",
                "14:02:11.921",
                EventSource::Core,
                "action.start",
                "Quote of the day",
                None,
                ("", ""),
            ),
            EventItem::seed(
                "seed-00000004",
                "14:02:11.939",
                EventSource::Core,
                "action.done",
                "success",
                Some("18ms total"),
                ("", ""),
            ),
            EventItem::seed(
                "seed-00000005",
                "14:02:24.507",
                EventSource::Twitch,
                "twitch.subscription",
                "maksym_dn subscribed at tier 1",
                Some("\u{2192} 1 action"),
                ("maksym_dn", "twitch"),
            ),
            EventItem::seed(
                "seed-00000006",
                "14:02:38.113",
                EventSource::Twitch,
                "twitch.cheer",
                "haash_ cheered 500 bits",
                Some("\u{2192} 1 action"),
                ("haash_", "twitch"),
            ),
            EventItem::seed(
                "seed-00000007",
                "14:02:45.660",
                EventSource::YouTube,
                "youtube.chat.message",
                "olena_lv: first time catching this live",
                Some("no match"),
                ("olena_lv", "youtube"),
            ),
            EventItem::seed(
                "seed-00000008",
                "14:02:52.030",
                EventSource::Timer,
                "timer.tick",
                "hydration-reminder",
                None,
                ("", ""),
            ),
            EventItem::seed(
                "seed-00000009",
                "14:03:03.418",
                EventSource::Obs,
                "scene.changed",
                "\"Starting soon\" \u{2192} \"Live\"",
                None,
                ("", ""),
            ),
            EventItem::seed(
                "seed-00000010",
                "14:03:09.774",
                EventSource::Rhai,
                "global.set",
                "hydration = 3",
                None,
                ("", ""),
            ),
            EventItem::seed(
                "seed-00000011",
                "14:03:15.201",
                EventSource::Http,
                "request.fail",
                "helix/users \u{2192} 429",
                Some("retry in 30s"),
                ("", ""),
            ),
        ];
        Self {
            items,
            paused: false,
        }
    }

    /// The rows in arrival order (oldest first); the screen renders newest at the
    /// bottom and auto-scrolls there.
    pub fn items(&self) -> &[EventItem] {
        &self.items
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Flips the paused flag and reports the new state. While paused, [`push`] drops
    /// incoming rows. Kept free of `cx` so the screen pairs it with `cx.notify()` and
    /// it stays directly exercisable.
    ///
    /// [`push`]: EventLog::push
    pub fn toggle_paused(&mut self) -> bool {
        self.paused = !self.paused;
        self.paused
    }

    /// Drops every retained row (the toolbar Clear). Leaves the paused flag as-is.
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Appends one decoded row, honoring the paused flag and evicting the oldest row
    /// once the ring is full. The bridge calls this inside `log.update(cx, …)` and
    /// pairs it with `cx.notify()`.
    pub fn push(&mut self, item: EventItem) {
        if self.paused {
            return;
        }
        if self.items.len() >= RING_CAP {
            self.items.remove(0);
        }
        self.items.push(item);
    }

    /// Decodes a bus event into a feed row. Every observability event maps to a row,
    /// so at runtime the boot tick publisher's `timer.tick` events stream in live on
    /// top of the seed. The summary/result-tag mapping is provisional (the shipping
    /// feed resolves richer per-kind payloads); once the full event pipeline lands
    /// this decode is replaced by the shared summary formatter.
    pub fn item_from_event(event: &Event) -> Option<EventItem> {
        let ts = event.timestamp;
        let timestamp = format!(
            "{:02}:{:02}:{:02}.{:03}",
            ts.hour(),
            ts.minute(),
            ts.second(),
            ts.millisecond()
        );
        let kind = event.kind.clone();
        let is_error = kind.contains("error") || kind.contains("fail");
        let summary = Self::summarize(event);
        let (user_login, user_platform) = Self::acting_user(event);
        Some(EventItem {
            id: event.id.to_string().into(),
            timestamp: timestamp.into(),
            source: event.source,
            kind: kind.into(),
            summary: summary.into(),
            result_tag: None,
            is_error,
            user_login: user_login.into(),
            user_platform: user_platform.into(),
        })
    }

    /// Best-effort one-line summary for a live event. Reads a small set of known
    /// payload fields; unknown kinds fall back to the kind string. Provisional for
    /// the slice — the production feed owns the exhaustive per-kind formatter.
    fn summarize(event: &Event) -> String {
        let p = &event.payload;
        match event.kind.as_str() {
            k if is_chat_message_kind(k) => {
                let author = p.get("author").and_then(|v| v.as_str()).unwrap_or("?");
                let text = p.get("text").and_then(|v| v.as_str()).unwrap_or("");
                format!("{author}: {text}")
            }
            "timer.tick" => p
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("tick")
                .to_owned(),
            "action.start" => p
                .get("action_name")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_owned(),
            "action.done" => p
                .get("outcome")
                .and_then(|v| v.as_str())
                .unwrap_or("done")
                .to_owned(),
            _ => event.kind.clone(),
        }
    }

    /// Extracts the acting user (login, platform) from a chat event's payload for
    /// the inspector, or a pair of empties when the event has no user.
    fn acting_user(event: &Event) -> (String, String) {
        if !is_chat_message_kind(&event.kind) {
            return (String::new(), String::new());
        }
        let login = event
            .payload
            .get("author")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let platform = event
            .payload
            .get("platform")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        (login, platform)
    }
}
