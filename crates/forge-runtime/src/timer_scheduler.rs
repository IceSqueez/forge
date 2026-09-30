use std::collections::HashMap;
use std::sync::Arc;

use forge_events::Event;
use forge_registry::{TriggerKindDescriptor, effective_config};
use forge_storage::CatalogChanges;
use forge_types::{PlatformScope, TriggerInstanceId, UserBadge};
use tokio::time::Instant;
use tracing::warn;

use crate::bus::{Delivery, EventBus, EventSubscription};
use crate::catalog::{Catalog, CatalogSnapshot};
use crate::chat_stream::{ChatRecord, ChatRecordMapper, event_source_to_chat_source};
use crate::delivery::TIMER_SCHEDULER;
use crate::own_chat_echoes::{CHAT_SENT_KIND, OwnChatEchoes};
use crate::stream_live::StreamLiveHandle;
use crate::triggers::{TIMER_TICK_KIND, TimerSchedule, TimerTickDescriptor};

struct ArmedTimer {
    schedule: TimerSchedule,
    scope: PlatformScope,
    next_fire: Instant,
    counting_since: Instant,
    chat_messages: u32,
}

impl ArmedTimer {
    fn fresh(schedule: TimerSchedule, scope: PlatformScope, now: Instant) -> Self {
        Self {
            schedule,
            scope,
            next_fire: now + schedule.interval(),
            counting_since: now,
            chat_messages: 0,
        }
    }

    fn restart(&mut self, now: Instant) {
        self.next_fire = now + self.schedule.interval();
        self.counting_since = now;
        self.chat_messages = 0;
    }

    fn gates_open(&self, live: bool) -> bool {
        (live || !self.schedule.only_while_live)
            && self.chat_messages >= self.schedule.min_chat_messages
    }
}

struct TimerScheduler {
    bus: Arc<EventBus>,
    catalog: Arc<Catalog>,
    armed: HashMap<TriggerInstanceId, ArmedTimer>,
    live: bool,
    chat: ChatRecordMapper,
    own_echoes: OwnChatEchoes,
}

pub fn spawn_timer_scheduler(
    bus: Arc<EventBus>,
    catalog: Arc<Catalog>,
    stream_live: StreamLiveHandle,
) {
    let chat = bus.subscribe_observer(TIMER_SCHEDULER);
    let changes = catalog.changes();
    let scheduler = TimerScheduler {
        live: stream_live.current().is_live(),
        bus,
        catalog,
        armed: HashMap::new(),
        chat: ChatRecordMapper::default(),
        own_echoes: OwnChatEchoes::default(),
    };
    tokio::spawn(scheduler.run(changes, stream_live, chat));
}

impl TimerScheduler {
    async fn run(
        mut self,
        mut changes: CatalogChanges,
        mut stream_live: StreamLiveHandle,
        mut chat: EventSubscription,
    ) {
        self.reconcile().await;
        let mut catalog_open = true;
        let mut live_open = true;
        loop {
            let next_fire = self.armed.values().map(|timer| timer.next_fire).min();
            tokio::select! {
                revision = changes.changed(), if catalog_open => match revision {
                    Some(_) => self.reconcile().await,
                    None => catalog_open = false,
                },
                state = stream_live.changed(), if live_open => match state {
                    Some(state) => self.follow_live(state.is_live()),
                    None => live_open = false,
                },
                delivery = chat.next() => match delivery {
                    Delivery::Event(event) => self.count_chat(&event),
                    Delivery::Skipped(_) => {}
                    Delivery::Closed => return,
                },
                () = sleep_until_due(next_fire) => self.fire_due(),
            }
        }
    }

    async fn reconcile(&mut self) {
        let snapshot = match self.catalog.current().await {
            Ok(snapshot) => snapshot,
            Err(e) => {
                warn!("timer_scheduler: catalog rebuild failed, keeping armed timers: {e}");
                return;
            }
        };
        let wanted = armed_set(&snapshot);
        let now = Instant::now();
        self.armed.retain(|id, _| wanted.contains_key(id));
        for (id, (schedule, scope)) in wanted {
            let unchanged = self
                .armed
                .get(&id)
                .is_some_and(|timer| timer.schedule == schedule && timer.scope == scope);
            if !unchanged {
                self.armed
                    .insert(id, ArmedTimer::fresh(schedule, scope, now));
            }
        }
    }

    fn follow_live(&mut self, live: bool) {
        let went_live = live && !self.live;
        self.live = live;
        if !went_live {
            return;
        }
        let now = Instant::now();
        self.armed
            .values_mut()
            .filter(|timer| timer.schedule.only_while_live)
            .for_each(|timer| timer.restart(now));
    }

    fn count_chat(&mut self, event: &Event) {
        if self.armed.is_empty() {
            return;
        }
        let now = Instant::now();
        if event.kind == CHAT_SENT_KIND {
            self.retract_own_message(event, now);
            return;
        }
        let Some(ChatRecord::Row(row)) = self.chat.map(event) else {
            return;
        };
        if row.is_event || row.badges.contains(&UserBadge::Broadcaster) || row.is_from_bot() {
            return;
        }
        if self.own_echoes.is_own_echo(&row, now) {
            return;
        }
        let platform = event.source.to_platform_id();
        self.armed
            .values_mut()
            .filter(|timer| timer.scope.matches(platform))
            .for_each(|timer| timer.chat_messages = timer.chat_messages.saturating_add(1));
    }

    fn retract_own_message(&mut self, event: &Event, now: Instant) {
        let Some(source) = event_source_to_chat_source(event.source) else {
            return;
        };
        let Some(message) = event.payload.get("message").and_then(|v| v.as_str()) else {
            return;
        };
        let Some(counted_at) = self.own_echoes.record_sent(source, message, now) else {
            return;
        };
        let platform = event.source.to_platform_id();
        self.armed
            .values_mut()
            .filter(|timer| timer.scope.matches(platform) && timer.counting_since <= counted_at)
            .for_each(|timer| timer.chat_messages = timer.chat_messages.saturating_sub(1));
    }

    fn fire_due(&mut self) {
        let now = Instant::now();
        for (id, timer) in &mut self.armed {
            if timer.next_fire > now {
                continue;
            }
            if !timer.gates_open(self.live) {
                timer.next_fire = now + timer.schedule.interval();
                continue;
            }
            self.bus
                .publish(timer.schedule.tick_event(*id, timer.chat_messages));
            timer.restart(now);
        }
    }
}

fn armed_set(
    snapshot: &CatalogSnapshot,
) -> HashMap<TriggerInstanceId, (TimerSchedule, PlatformScope)> {
    let defaults = TimerTickDescriptor.default_config();
    snapshot
        .binding_indexes_for_kinds([TIMER_TICK_KIND])
        .into_iter()
        .filter_map(|index| snapshot.binding(index))
        .map(|binding| {
            let config = effective_config(&defaults, &binding.instance.overrides);
            (
                binding.instance.id,
                (
                    TimerSchedule::from_config(&config),
                    binding.instance.platform_scope.clone(),
                ),
            )
        })
        .collect()
}

async fn sleep_until_due(next_fire: Option<Instant>) {
    match next_fire {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Mutex as StdMutex;
    use std::time::Duration;

    use forge_events::EventSource;
    use forge_obs::SwitchableObsSink;
    use forge_platform_core::ViewerReport;
    use forge_storage::action::MockActionRepo;
    use forge_storage::trigger_instance::MockTriggerInstanceRepo;
    use forge_storage::{ActionRepo, CatalogRevision, TriggerInstanceRepo};
    use forge_types::{
        Action, ActionId, ChatPayload, ChatSegment, ModerationMarks, PermissionRung, PlatformId,
        QueueId, TriggerConfig, TriggerInstance, Variant,
    };
    use serde_json::json;
    use tokio::sync::mpsc::UnboundedSender;

    use super::*;
    use crate::NullEventLogRepo;
    use crate::live_viewers::{LiveViewerAggregatorHandle, spawn_live_viewer_aggregator};
    use crate::stream_live::spawn_stream_live_signal;
    use crate::test_support::channel_viewer_source;

    const MINUTE: Duration = Duration::from_secs(60);
    const SECOND: Duration = Duration::from_secs(1);

    #[derive(Clone, Copy)]
    struct Timer {
        minutes: i64,
        only_while_live: bool,
        min_chat_messages: i64,
    }

    impl Timer {
        fn every(minutes: i64) -> Self {
            Self {
                minutes,
                only_while_live: false,
                min_chat_messages: 0,
            }
        }

        fn live_only(self) -> Self {
            Self {
                only_while_live: true,
                ..self
            }
        }

        fn after_messages(self, min_chat_messages: i64) -> Self {
            Self {
                min_chat_messages,
                ..self
            }
        }

        fn instance(self) -> TriggerInstance {
            let mut overrides = TriggerConfig::new();
            overrides.insert("interval_minutes".to_owned(), Variant::Int(self.minutes));
            overrides.insert(
                "only_while_live".to_owned(),
                Variant::Bool(self.only_while_live),
            );
            overrides.insert(
                "min_chat_messages".to_owned(),
                Variant::Int(self.min_chat_messages),
            );
            TriggerInstance {
                id: TriggerInstanceId::new(),
                kind_id: TIMER_TICK_KIND.to_owned(),
                name: "timer".to_owned(),
                overrides,
                enabled: true,
                user_defined: true,
                platform_scope: PlatformScope::Any,
                cooldown_secs: 0,
                cooldown_global: true,
                permission_rung: PermissionRung::Everyone,
            }
        }
    }

    fn twitch_only(instance: TriggerInstance) -> TriggerInstance {
        TriggerInstance {
            platform_scope: PlatformScope::only(BTreeSet::from([PlatformId::Twitch])).unwrap(),
            ..instance
        }
    }

    type Stored<T> = Arc<StdMutex<T>>;

    struct Rig {
        bus: Arc<EventBus>,
        ticks: EventSubscription,
        action: Stored<Action>,
        instances: Stored<Vec<TriggerInstance>>,
        revision: CatalogRevision,
        live_reports: UnboundedSender<ViewerReport>,
        next_message: u32,
        _viewers: LiveViewerAggregatorHandle,
        _obs: Arc<SwitchableObsSink>,
    }

    async fn rig(instances: Vec<TriggerInstance>) -> Rig {
        let action: Stored<Action> = Arc::new(StdMutex::new(Action {
            id: ActionId::new(),
            name: "timed".to_owned(),
            group: None,
            queue_id: QueueId::new(),
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: forge_types::ExecutionMode::Sequential,
            description: None,
            sub_actions: vec![],
        }));
        let instances: Stored<Vec<TriggerInstance>> = Arc::new(StdMutex::new(instances));

        let mut action_repo = MockActionRepo::new();
        let listed = Arc::clone(&action);
        action_repo
            .expect_list()
            .returning(move || Ok(vec![listed.lock().unwrap().clone()]));
        let mut instance_repo = MockTriggerInstanceRepo::new();
        let linked = Arc::clone(&instances);
        instance_repo
            .expect_list_for_action()
            .returning(move |_| Ok(linked.lock().unwrap().clone()));

        let revision = CatalogRevision::new();
        let catalog = Catalog::new(
            Arc::new(action_repo) as Arc<dyn ActionRepo>,
            Arc::new(instance_repo) as Arc<dyn TriggerInstanceRepo>,
            revision.clone(),
        );

        let viewers = spawn_live_viewer_aggregator();
        let obs = SwitchableObsSink::new();
        let stream_live = spawn_stream_live_signal(&viewers, obs.stream_output());
        let (source, live_reports) = channel_viewer_source();
        viewers.register(PlatformId::Twitch, source);

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let ticks = bus.subscribe();
        spawn_timer_scheduler(Arc::clone(&bus), catalog, stream_live);
        let rig = Rig {
            bus,
            ticks,
            action,
            instances,
            revision,
            live_reports,
            next_message: 0,
            _viewers: viewers,
            _obs: obs,
        };
        settle().await;
        rig
    }

    async fn settle() {
        for _ in 0..64 {
            tokio::task::yield_now().await;
        }
    }

    impl Rig {
        async fn elapse(&mut self, by: Duration) -> Vec<Event> {
            tokio::time::advance(by).await;
            settle().await;
            let mut ticks = Vec::new();
            while let Ok(Some(event)) = self.ticks.try_recv() {
                if event.kind == TIMER_TICK_KIND {
                    ticks.push(event);
                }
            }
            ticks
        }

        async fn tick_count_over(&mut self, by: Duration) -> usize {
            self.elapse(by).await.len()
        }

        async fn go_live(&self) {
            self.live_reports
                .send(ViewerReport::Live { count: 1 })
                .unwrap();
            settle().await;
        }

        async fn go_offline(&self) {
            self.live_reports.send(ViewerReport::Absent).unwrap();
            settle().await;
        }

        async fn chat(&mut self, source: EventSource, badges: Vec<UserBadge>, is_event: bool) {
            self.post(source, "viewer", badges, is_event, "hello").await;
        }

        async fn said(&mut self, source: EventSource, author: &str, text: &str) {
            self.post(source, author, vec![], false, text).await;
        }

        async fn post(
            &mut self,
            source: EventSource,
            author: &str,
            badges: Vec<UserBadge>,
            is_event: bool,
            text: &str,
        ) {
            self.next_message += 1;
            let payload = ChatPayload {
                platform_msg_id: format!("msg-{}", self.next_message),
                author: author.to_owned(),
                author_color: None,
                segments: vec![ChatSegment::Text {
                    text: text.to_owned(),
                }],
                badges,
                is_event,
                event_detail: None,
                moderation: ModerationMarks::default(),
            };
            self.bus.publish(Event::new(
                source,
                "chat.message",
                json!({ (ChatPayload::KEY): serde_json::to_value(&payload).unwrap() }),
            ));
            settle().await;
        }

        async fn forge_sent(&self, source: EventSource, message: &str) {
            self.bus.publish(Event::new(
                source,
                CHAT_SENT_KIND,
                json!({ "channel": "twitch", "message": message }),
            ));
            settle().await;
        }

        async fn viewer_messages(&mut self, count: u32) {
            for _ in 0..count {
                self.chat(EventSource::Twitch, vec![], false).await;
            }
        }

        async fn edit(&self, change: impl FnOnce(&mut Action, &mut Vec<TriggerInstance>)) {
            change(
                &mut self.action.lock().unwrap(),
                &mut self.instances.lock().unwrap(),
            );
            self.revision.advance();
            settle().await;
        }
    }

    fn addressed_instance(tick: &Event) -> serde_json::Value {
        tick.payload["instance_id"].clone()
    }

    #[tokio::test(start_paused = true)]
    async fn first_fire_lands_one_interval_after_arming_not_before() {
        let mut rig = rig(vec![Timer::every(10).instance()]).await;

        let early = rig.tick_count_over(10 * MINUTE - SECOND).await;
        let on_time = rig.tick_count_over(SECOND).await;

        assert_eq!((early, on_time), (0, 1));
    }

    #[tokio::test(start_paused = true)]
    async fn timer_keeps_firing_every_interval_after_the_first() {
        let mut rig = rig(vec![Timer::every(10).instance()]).await;
        rig.elapse(10 * MINUTE).await;

        let counts = [
            rig.tick_count_over(10 * MINUTE - SECOND).await,
            rig.tick_count_over(SECOND).await,
            rig.tick_count_over(10 * MINUTE).await,
        ];

        assert_eq!(counts, [0, 1, 1]);
    }

    #[tokio::test(start_paused = true)]
    async fn each_tick_names_only_the_timer_that_came_due() {
        let fast = Timer::every(10).instance();
        let slow = Timer::every(15).instance();
        let fast_id = serde_json::to_value(fast.id).unwrap();
        let mut rig = rig(vec![fast, slow]).await;

        let ticks = rig.elapse(10 * MINUTE).await;

        assert_eq!(
            ticks.iter().map(addressed_instance).collect::<Vec<_>>(),
            vec![fast_id]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_long_suspend_yields_one_late_fire_then_re_anchors_on_it() {
        let mut rig = rig(vec![Timer::every(10).instance()]).await;

        let after_suspend = rig.tick_count_over(10 * 10 * MINUTE + 3 * MINUTE).await;
        let before_next = rig.tick_count_over(10 * MINUTE - SECOND).await;
        let next = rig.tick_count_over(SECOND).await;

        assert_eq!((after_suspend, before_next, next), (1, 0, 1));
    }

    #[tokio::test(start_paused = true)]
    async fn live_only_timer_stays_silent_while_offline() {
        let mut rig = rig(vec![Timer::every(10).live_only().instance()]).await;

        assert_eq!(rig.tick_count_over(30 * MINUTE).await, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn going_live_re_anchors_a_live_only_timer_to_the_live_moment() {
        let mut rig = rig(vec![Timer::every(10).live_only().instance()]).await;
        rig.elapse(7 * MINUTE).await;
        rig.go_live().await;

        let at_original_deadline = rig.tick_count_over(3 * MINUTE).await;
        let before_live_deadline = rig.tick_count_over(7 * MINUTE - SECOND).await;
        let at_live_deadline = rig.tick_count_over(SECOND).await;

        assert_eq!(
            (at_original_deadline, before_live_deadline, at_live_deadline),
            (0, 0, 1)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn going_offline_silences_a_live_only_timer() {
        let mut rig = rig(vec![Timer::every(10).live_only().instance()]).await;
        rig.go_live().await;
        rig.elapse(10 * MINUTE).await;
        rig.go_offline().await;

        assert_eq!(rig.tick_count_over(30 * MINUTE).await, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn going_live_keeps_the_phase_of_a_timer_that_ignores_live_state() {
        let mut rig = rig(vec![Timer::every(10).instance()]).await;
        rig.elapse(7 * MINUTE).await;
        rig.go_live().await;

        assert_eq!(rig.tick_count_over(3 * MINUTE).await, 1);
    }

    #[tokio::test(start_paused = true)]
    async fn going_live_discards_chat_counted_while_offline() {
        let mut rig = rig(vec![
            Timer::every(10).live_only().after_messages(2).instance(),
        ])
        .await;
        rig.viewer_messages(2).await;
        rig.go_live().await;

        assert_eq!(rig.tick_count_over(10 * MINUTE).await, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn one_message_short_of_the_minimum_skips_the_fire() {
        let mut rig = rig(vec![Timer::every(10).after_messages(3).instance()]).await;
        rig.viewer_messages(2).await;

        assert_eq!(rig.tick_count_over(10 * MINUTE).await, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn reaching_the_minimum_fires_and_reports_the_count() {
        let mut rig = rig(vec![Timer::every(10).after_messages(3).instance()]).await;
        rig.viewer_messages(3).await;

        let ticks = rig.elapse(10 * MINUTE).await;

        assert_eq!(
            ticks
                .iter()
                .map(|tick| tick.payload["message_count"].clone())
                .collect::<Vec<_>>(),
            vec![json!(3)]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_fire_resets_the_chat_counter() {
        let mut rig = rig(vec![Timer::every(10).after_messages(3).instance()]).await;
        rig.viewer_messages(3).await;
        rig.elapse(10 * MINUTE).await;

        assert_eq!(rig.tick_count_over(10 * MINUTE).await, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn a_skipped_fire_keeps_counting_toward_the_next_deadline() {
        let mut rig = rig(vec![Timer::every(10).after_messages(3).instance()]).await;
        rig.viewer_messages(2).await;
        rig.elapse(10 * MINUTE).await;
        rig.viewer_messages(1).await;

        assert_eq!(rig.tick_count_over(10 * MINUTE).await, 1);
    }

    #[tokio::test(start_paused = true)]
    async fn broadcaster_bot_and_notice_messages_do_not_count() {
        for (label, author, badges, is_event) in [
            ("broadcaster", "viewer", vec![UserBadge::Broadcaster], false),
            ("bot badge", "viewer", vec![UserBadge::Bot], false),
            ("known bot account", "Nightbot", vec![], false),
            ("notice", "viewer", vec![], true),
        ] {
            let mut rig = rig(vec![Timer::every(10).after_messages(1).instance()]).await;
            rig.post(EventSource::Twitch, author, badges, is_event, "hello")
                .await;

            assert_eq!(rig.tick_count_over(10 * MINUTE).await, 0, "{label}");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn platform_scope_counts_only_chat_from_the_scoped_platform() {
        let mut rig = rig(vec![twitch_only(
            Timer::every(10).after_messages(1).instance(),
        )])
        .await;
        rig.chat(EventSource::Kick, vec![], false).await;
        let out_of_scope = rig.tick_count_over(10 * MINUTE).await;
        rig.chat(EventSource::Twitch, vec![], false).await;
        let in_scope = rig.tick_count_over(10 * MINUTE).await;

        assert_eq!((out_of_scope, in_scope), (0, 1));
    }

    #[tokio::test(start_paused = true)]
    async fn a_timer_created_at_runtime_arms_from_the_write() {
        let mut rig = rig(vec![]).await;
        rig.elapse(4 * MINUTE).await;
        rig.edit(|_, instances| instances.push(Timer::every(10).instance()))
            .await;

        let before = rig.tick_count_over(10 * MINUTE - SECOND).await;
        let due = rig.tick_count_over(SECOND).await;

        assert_eq!((before, due), (0, 1));
    }

    #[tokio::test(start_paused = true)]
    async fn disabling_or_deleting_a_timer_stops_it() {
        let disable_instance: fn(&mut Action, &mut Vec<TriggerInstance>) =
            |_, instances| instances[0].enabled = false;
        let disable_action: fn(&mut Action, &mut Vec<TriggerInstance>) =
            |action, _| action.enabled = false;
        let delete: fn(&mut Action, &mut Vec<TriggerInstance>) = |_, instances| instances.clear();
        for (label, change) in [
            ("disable instance", disable_instance),
            ("disable action", disable_action),
            ("delete", delete),
        ] {
            let mut rig = rig(vec![Timer::every(10).instance()]).await;
            rig.elapse(5 * MINUTE).await;
            rig.edit(change).await;

            assert_eq!(rig.tick_count_over(30 * MINUTE).await, 0, "{label}");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn editing_the_interval_re_arms_from_the_edit() {
        let mut rig = rig(vec![Timer::every(10).instance()]).await;
        rig.elapse(5 * MINUTE).await;
        rig.edit(|_, instances| {
            instances[0]
                .overrides
                .insert("interval_minutes".to_owned(), Variant::Int(20));
        })
        .await;

        let before = rig.tick_count_over(20 * MINUTE - SECOND).await;
        let due = rig.tick_count_over(SECOND).await;

        assert_eq!((before, due), (0, 1));
    }

    #[tokio::test(start_paused = true)]
    async fn renaming_a_timer_keeps_its_phase_and_counter() {
        let mut rig = rig(vec![Timer::every(10).after_messages(1).instance()]).await;
        rig.viewer_messages(1).await;
        rig.elapse(5 * MINUTE).await;
        rig.edit(|_, instances| instances[0].name = "renamed".to_owned())
            .await;

        assert_eq!(rig.tick_count_over(5 * MINUTE).await, 1);
    }

    #[tokio::test(start_paused = true)]
    async fn forges_own_echo_does_not_count_whichever_arrives_first() {
        for echo_first in [false, true] {
            let mut rig = rig(vec![Timer::every(10).after_messages(1).instance()]).await;
            if echo_first {
                rig.said(EventSource::Twitch, "forge_helper", "Follow the channel!")
                    .await;
                rig.forge_sent(EventSource::Twitch, "Follow the channel!")
                    .await;
            } else {
                rig.forge_sent(EventSource::Twitch, "Follow the channel!")
                    .await;
                rig.said(EventSource::Twitch, "forge_helper", "Follow the channel!")
                    .await;
            }

            assert_eq!(
                rig.tick_count_over(10 * MINUTE).await,
                0,
                "echo first: {echo_first}"
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_viewer_repeating_forges_message_still_counts() {
        let mut rig = rig(vec![Timer::every(10).after_messages(1).instance()]).await;
        rig.forge_sent(EventSource::Twitch, "Follow the channel!")
            .await;
        rig.said(EventSource::Twitch, "forge_helper", "Follow the channel!")
            .await;
        rig.said(EventSource::Twitch, "viewer", "Follow the channel!")
            .await;

        assert_eq!(rig.tick_count_over(10 * MINUTE).await, 1);
    }

    #[tokio::test(start_paused = true)]
    async fn a_late_send_leaves_a_counter_restarted_after_its_echo_alone() {
        let mut rig = rig(vec![Timer::every(10).after_messages(1).instance()]).await;
        rig.elapse(9 * MINUTE + 30 * SECOND).await;
        rig.said(EventSource::Twitch, "forge_helper", "Follow the channel!")
            .await;
        rig.elapse(30 * SECOND).await;
        rig.viewer_messages(1).await;
        rig.elapse(10 * SECOND).await;
        rig.forge_sent(EventSource::Twitch, "Follow the channel!")
            .await;

        assert_eq!(rig.tick_count_over(10 * MINUTE - 10 * SECOND).await, 1);
    }

    #[tokio::test(start_paused = true)]
    async fn retracting_an_echo_leaves_timers_scoped_to_other_platforms_alone() {
        let mut rig = rig(vec![twitch_only(
            Timer::every(10).after_messages(1).instance(),
        )])
        .await;
        rig.said(EventSource::Kick, "forge_helper", "Follow the channel!")
            .await;
        rig.viewer_messages(1).await;
        rig.forge_sent(EventSource::Kick, "Follow the channel!")
            .await;

        assert_eq!(rig.tick_count_over(10 * MINUTE).await, 1);
    }
}
