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
use crate::chat_stream::{ChatRecord, ChatRecordMapper};
use crate::delivery::TIMER_SCHEDULER;
use crate::stream_live::StreamLiveHandle;
use crate::triggers::{TIMER_TICK_KIND, TimerSchedule, TimerTickDescriptor};

struct ArmedTimer {
    schedule: TimerSchedule,
    scope: PlatformScope,
    next_fire: Instant,
    chat_messages: u32,
}

impl ArmedTimer {
    fn fresh(schedule: TimerSchedule, scope: PlatformScope, now: Instant) -> Self {
        Self {
            schedule,
            scope,
            next_fire: now + schedule.interval(),
            chat_messages: 0,
        }
    }

    fn restart(&mut self, now: Instant) {
        self.next_fire = now + self.schedule.interval();
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
        let Some(ChatRecord::Row(row)) = self.chat.map(event) else {
            return;
        };
        if row.is_event || row.badges.contains(&UserBadge::Broadcaster) {
            return;
        }
        let platform = event.source.to_platform_id();
        self.armed
            .values_mut()
            .filter(|timer| timer.scope.matches(platform))
            .for_each(|timer| timer.chat_messages = timer.chat_messages.saturating_add(1));
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

