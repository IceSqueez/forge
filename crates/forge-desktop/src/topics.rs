use gpui::Entity;

use crate::chat_feed::ChatFeed;
use crate::event_log::EventLog;
use crate::globals::Globals;
use crate::home_stats::HomeStats;
use crate::platforms::PlatformConnectivity;
use crate::queue_health::QueueHealth;
use crate::speak_state::SpeakState;

pub struct Topics {
    pub chat_feed: Entity<ChatFeed>,
    pub home_stats: Entity<HomeStats>,
    pub event_log: Entity<EventLog>,
    pub globals: Entity<Globals>,
    pub platforms: Entity<PlatformConnectivity>,
    pub speak: Entity<SpeakState>,
    pub queue_health: Entity<QueueHealth>,
}

impl Topics {
    pub fn new(
        chat_feed: Entity<ChatFeed>,
        home_stats: Entity<HomeStats>,
        event_log: Entity<EventLog>,
        globals: Entity<Globals>,
        platforms: Entity<PlatformConnectivity>,
        speak: Entity<SpeakState>,
        queue_health: Entity<QueueHealth>,
    ) -> Self {
        Self {
            chat_feed,
            home_stats,
            event_log,
            globals,
            platforms,
            speak,
            queue_health,
        }
    }
}
