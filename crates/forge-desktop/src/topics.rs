use gpui::Entity;

use crate::chat_feed::ChatFeed;
use crate::event_log::EventLog;
use crate::globals::Globals;
use crate::home_stats::HomeStats;

/// Plain grouping of the runtime→UI bridge topic entities — the observable caches
/// the boot bridge drains the runtime bus into. It is a state-less handle bundle,
/// not a view: the root [`crate::shell::AppShell`] holds one `Topics` field and
/// hands the relevant topic to each routed screen (Chat gets `chat_feed`, Home gets
/// `home_stats`), so those caches persist across navigation while the root stays
/// within its ≤5 top-level field budget. New screens add their topic here, exactly
/// as [`crate::chrome::Chrome`] bundles the persistent chrome children.
pub struct Topics {
    pub chat_feed: Entity<ChatFeed>,
    pub home_stats: Entity<HomeStats>,
    pub event_log: Entity<EventLog>,
    pub globals: Entity<Globals>,
}

impl Topics {
    pub fn new(
        chat_feed: Entity<ChatFeed>,
        home_stats: Entity<HomeStats>,
        event_log: Entity<EventLog>,
        globals: Entity<Globals>,
    ) -> Self {
        Self {
            chat_feed,
            home_stats,
            event_log,
            globals,
        }
    }
}
