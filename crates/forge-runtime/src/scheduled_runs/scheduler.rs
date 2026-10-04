use std::sync::Arc;
use std::time::Duration;

use forge_storage::{
    ActionRepo, CatalogChanges, CatalogRevision, ScheduledRunRepo, ScheduledRunSpec,
};
use time::OffsetDateTime;
use tokio::sync::mpsc;
use tracing::{info, warn};

use super::catch_up::CatchUpSettle;
use super::clock::WallClock;
use super::hand_off::{HandOffPath, UNREADABLE_REASON};
use super::handle::{Command, ScheduledRunsHandle};
use super::limits::{
    CATCH_UP_SETTLE_LIMIT, MAX_PENDING_SCHEDULED_RUNS, MAX_PENDING_SCHEDULED_RUNS_PER_ACTION,
    WALL_CLOCK_RECHECK,
};
use super::request::{ScheduleError, ScheduleRequest, ScheduledPlacement};
use crate::catalog::Catalog;
use crate::{EventBus, QueueSchedulerHandle};

const COMMAND_CAPACITY: usize = 64;

pub struct ScheduledRunsParts {
    pub repo: Arc<dyn ScheduledRunRepo>,
    pub revision: CatalogRevision,
    pub catalog: Arc<Catalog>,
    pub actions: Arc<dyn ActionRepo>,
    pub queues: QueueSchedulerHandle,
    pub bus: Arc<EventBus>,
    pub clock: Arc<dyn WallClock>,
    pub catch_up: CatchUpSettle,
}

struct ScheduledRunsTask {
    repo: Arc<dyn ScheduledRunRepo>,
    actions: Arc<dyn ActionRepo>,
    clock: Arc<dyn WallClock>,
    path: HandOffPath,
    next_due: Option<OffsetDateTime>,
    retry_at: Option<OffsetDateTime>,
}

pub fn spawn_scheduled_runs(parts: ScheduledRunsParts) -> ScheduledRunsHandle {
    let ScheduledRunsParts {
        repo,
        revision,
        catalog,
        actions,
        queues,
        bus,
        clock,
        catch_up,
    } = parts;
    let (commands_tx, commands) = mpsc::channel(COMMAND_CAPACITY);
    let changes = revision.subscribe();
    let task = ScheduledRunsTask {
        repo: Arc::clone(&repo),
        actions: Arc::clone(&actions),
        clock: Arc::clone(&clock),
        path: HandOffPath {
            repo: Arc::clone(&repo),
            catalog,
            actions,
            queues,
            bus,
            clock: Arc::clone(&clock),
        },
        next_due: None,
        retry_at: None,
    };
    tokio::spawn(task.run(commands, changes, catch_up));
    ScheduledRunsHandle::new(commands_tx, repo, clock)
}

impl ScheduledRunsTask {
    async fn run(
        mut self,
        mut commands: mpsc::Receiver<Command>,
        mut changes: CatalogChanges,
        catch_up: CatchUpSettle,
    ) {
        self.refresh_next_due().await;
        let mut settling = Box::pin(catch_up.wait(CATCH_UP_SETTLE_LIMIT));
        let mut settled = false;
        let mut commands_open = true;
        let mut revisions_open = true;
        loop {
            let pause = self.pause_before_next_check();
            tokio::select! {
                command = commands.recv(), if commands_open => match command {
                    Some(command) => self.serve(command).await,
                    None => commands_open = false,
                },
                revision = changes.changed(), if revisions_open => match revision {
                    Some(_) => self.refresh_next_due().await,
                    None => revisions_open = false,
                },
                in_time = &mut settling, if !settled => {
                    settled = true;
                    if !in_time {
                        info!("scheduled runs: integrations still connecting after the settle limit, catching up anyway");
                    }
                    self.dispatch_due().await;
                },
                () = sleep_for(pause), if settled => self.dispatch_due().await,
            }
        }
    }

    async fn serve(&mut self, command: Command) {
        match command {
            Command::Schedule(request, reply) => {
                let _ = reply.send(self.schedule(request).await);
            }
            Command::RunNow(id, reply) => {
                let handed = self.path.now(id).await.map_err(ScheduleError::from);
                let _ = reply.send(handed);
            }
        }
    }

    async fn schedule(
        &mut self,
        request: ScheduleRequest,
    ) -> Result<ScheduledPlacement, ScheduleError> {
        let Some(target) = self.actions.get(request.target_action_id).await? else {
            return Err(ScheduleError::UnknownAction);
        };
        let spec = request.into_spec(self.clock.now(), target.name)?;
        self.check_caps(&spec).await?;
        let placement = self.repo.schedule(&spec).await?;
        Ok(ScheduledPlacement {
            id: placement.id,
            due_at: spec.due_at,
            superseded: placement.superseded,
        })
    }

    async fn check_caps(&self, spec: &ScheduledRunSpec) -> Result<(), ScheduleError> {
        let pending = self.repo.count_pending().await?;
        let for_action = self
            .repo
            .count_pending_for_action(spec.target_action_id)
            .await?;
        if pending < MAX_PENDING_SCHEDULED_RUNS
            && for_action < MAX_PENDING_SCHEDULED_RUNS_PER_ACTION
        {
            return Ok(());
        }
        let replaced = match spec.key.as_deref() {
            Some(key) => self
                .repo
                .list_pending()
                .await?
                .into_iter()
                .find(|run| run.spec.key.as_deref() == Some(key)),
            None => None,
        };
        let freed = u64::from(replaced.is_some());
        let freed_for_action = u64::from(
            replaced.is_some_and(|run| run.spec.target_action_id == spec.target_action_id),
        );
        if pending.saturating_sub(freed) >= MAX_PENDING_SCHEDULED_RUNS {
            return Err(ScheduleError::PendingCapReached);
        }
        if for_action.saturating_sub(freed_for_action) >= MAX_PENDING_SCHEDULED_RUNS_PER_ACTION {
            return Err(ScheduleError::ActionPendingCapReached);
        }
        Ok(())
    }

    async fn fail_unreadable(&self, now: OffsetDateTime) {
        match self.repo.fail_unreadable_due(now, UNREADABLE_REASON).await {
            Ok(failed) => {
                for id in failed {
                    warn!(
                        scheduled_run = id.get(),
                        "scheduled run failed: its stored row cannot be read"
                    );
                }
            }
            Err(e) => warn!(error = %e, "scheduled runs: could not fail unreadable items"),
        }
    }

    async fn refresh_next_due(&mut self) {
        match self.repo.next_due().await {
            Ok(next_due) => {
                let now = self.clock.now();
                if next_due.is_none_or(|due| due > now) {
                    self.retry_at = None;
                }
                self.next_due = next_due;
            }
            Err(e) => {
                warn!(error = %e, "scheduled runs: could not read the next due time");
                let now = self.clock.now();
                self.next_due = Some(now);
                self.retry_at = Some(now + WALL_CLOCK_RECHECK);
            }
        }
    }

    fn pause_before_next_check(&self) -> Option<Duration> {
        let due = self.next_due?;
        let now = self.clock.now();
        let until_due = until(now, due);
        let until_retry = self
            .retry_at
            .map_or(Duration::ZERO, |retry| until(now, retry));
        Some(until_due.max(until_retry).min(WALL_CLOCK_RECHECK))
    }

    async fn dispatch_due(&mut self) {
        let now = self.clock.now();
        if self.next_due.is_none_or(|due| due > now) {
            return;
        }
        self.fail_unreadable(now).await;
        match self.repo.list_due(now).await {
            Ok(due) => {
                for run in due {
                    let id = run.id;
                    if let Err(e) = self.path.due(run).await {
                        warn!(scheduled_run = id.get(), error = %e, "scheduled run could not be handed off");
                    }
                }
            }
            Err(e) => warn!(error = %e, "scheduled runs: could not list due items"),
        }
        self.refresh_next_due().await;
        self.retry_at = self
            .next_due
            .filter(|due| *due <= now)
            .map(|_| now + WALL_CLOCK_RECHECK);
    }
}

fn until(now: OffsetDateTime, instant: OffsetDateTime) -> Duration {
    Duration::try_from(instant - now).unwrap_or(Duration::ZERO)
}

async fn sleep_for(pause: Option<Duration>) {
    match pause {
        Some(pause) => tokio::time::sleep(pause).await,
        None => std::future::pending().await,
    }
}
