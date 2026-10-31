use gpui::Entity;

use crate::awake_state::AwakeState;
use crate::chat_feed::ChatFeed;
use crate::event_log::EventLog;
use crate::event_loss::EventLoss;
use crate::globals::Globals;
use crate::home_stats::HomeStats;
use crate::integration_lifecycle::IntegrationLifecycle;
use crate::platforms::PlatformConnectivity;
use crate::problem_report::ProblemReport;
use crate::queue_health::QueueHealth;
use crate::speak_state::SpeakState;
use crate::twitch_bot_presence::TwitchBotPresence;
use crate::update_flow::UpdateFlow;

pub(crate) struct Topics {
    pub chat_feed: Entity<ChatFeed>,
    pub home_stats: Entity<HomeStats>,
    pub event_log: Entity<EventLog>,
    pub globals: Entity<Globals>,
    pub platforms: Entity<PlatformConnectivity>,
    pub speak: Entity<SpeakState>,
    pub queue_health: Entity<QueueHealth>,
    pub event_loss: Entity<EventLoss>,
    pub awake: Entity<AwakeState>,
    pub integration_lifecycle: Entity<IntegrationLifecycle>,
    pub update_flow: Entity<UpdateFlow>,
    pub problem_report: Entity<ProblemReport>,
    pub twitch_bot: Entity<TwitchBotPresence>,
}

impl Topics {
    #[expect(
        clippy::too_many_arguments,
        reason = "constructor receives each shared handle it wires"
    )]
    pub(crate) fn new(
        chat_feed: Entity<ChatFeed>,
        home_stats: Entity<HomeStats>,
        event_log: Entity<EventLog>,
        globals: Entity<Globals>,
        platforms: Entity<PlatformConnectivity>,
        speak: Entity<SpeakState>,
        queue_health: Entity<QueueHealth>,
        event_loss: Entity<EventLoss>,
        awake: Entity<AwakeState>,
        integration_lifecycle: Entity<IntegrationLifecycle>,
        update_flow: Entity<UpdateFlow>,
        problem_report: Entity<ProblemReport>,
        twitch_bot: Entity<TwitchBotPresence>,
    ) -> Self {
        Self {
            chat_feed,
            home_stats,
            event_log,
            globals,
            platforms,
            speak,
            queue_health,
            event_loss,
            awake,
            integration_lifecycle,
            update_flow,
            problem_report,
            twitch_bot,
        }
    }
}
