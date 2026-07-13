use forge_events::{Event, EventSource};
use gpui::SharedString;

/// Upper bound on retained feed rows. The screen renders every filtered row into a
/// flat column (no virtualization yet), so the ring is bounded to keep the repaint
/// cheap under the live bus firehose. The real 10,000 capacity + a virtualized list
/// land with the production event pipeline.
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
}

/// Topic-scoped observable entity fed by the runtime→UI bridge: the sole owner of
/// the runtime-events→UI edge for the feed. The bridge drains the event bus, decodes
/// each observability event through [`EventLog::item_from_event`], appends it, then
/// `cx.notify()`s so the observing feed screen repaints. Holds no runtime state of
/// its own — only the rows it has been handed, a capacity ring, and a paused flag.
///
/// Starts empty and live: the boot-global bridge drains the real event bus and
/// appends every observability row through [`EventLog::push`], so the feed reflects
/// actual runtime traffic rather than a static sample. Renders empty-but-live until
/// the runtime publishes.
pub struct EventLog {
    items: Vec<EventItem>,
    /// When set, [`EventLog::push`] drops incoming rows — the screen's Pause toggle
    /// flips it, matching the shipping feed where pausing stops collection.
    paused: bool,
}

impl EventLog {
    /// An empty, unpaused feed. Rows arrive live over the bridge via
    /// [`EventLog::push`].
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
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
    /// so the whole runtime firehose streams into the feed live. The summary/result-tag
    /// mapping is provisional (the shipping feed resolves richer per-kind payloads);
    /// once the full event pipeline lands this decode is replaced by the shared
    /// summary formatter.
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
    /// the slice — the production feed owns the exhaustive per-kind formatter. Shared
    /// with the Home highlight reel so both surfaces phrase an event identically.
    pub(crate) fn summarize(event: &Event) -> String {
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
