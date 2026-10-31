use crate::error::RuntimeError;
use std::sync::Arc;
use std::time::Duration;

use forge_net::{TaskOwner, TaskShutdown};
use forge_storage::{ScheduledRunId, ScheduledRunRepo};
use tokio::sync::{mpsc, oneshot, watch};

use super::hand_off::HandOff;
use super::request::{ScheduleRequest, ScheduledPlacement, normalized_key};
use super::waiting::{WaitingForQueueWatch, WaitingRuns};
use forge_types::WallClock;

pub(super) enum Command {
    Schedule(
        Box<ScheduleRequest>,
        oneshot::Sender<Result<ScheduledPlacement, RuntimeError>>,
    ),
    RunNow(
        ScheduledRunId,
        oneshot::Sender<Result<HandOff, RuntimeError>>,
    ),
}

#[derive(Clone)]
pub struct ScheduledRunsHandle {
    commands: mpsc::Sender<Command>,
    repo: Arc<dyn ScheduledRunRepo>,
    clock: Arc<dyn WallClock>,
    waiting: watch::Receiver<WaitingRuns>,
    tasks: TaskOwner,
}

impl ScheduledRunsHandle {
    pub(super) fn new(
        commands: mpsc::Sender<Command>,
        repo: Arc<dyn ScheduledRunRepo>,
        clock: Arc<dyn WallClock>,
        waiting: watch::Receiver<WaitingRuns>,
        tasks: TaskOwner,
    ) -> Self {
        Self {
            commands,
            repo,
            clock,
            waiting,
            tasks,
        }
    }

    pub async fn stop(self, grace: Duration) -> TaskShutdown {
        self.tasks.shutdown(grace).await
    }

    pub fn watch_waiting_for_queue(&self) -> WaitingForQueueWatch {
        WaitingForQueueWatch::new(self.waiting.clone())
    }

    pub async fn schedule(
        &self,
        request: ScheduleRequest,
    ) -> Result<ScheduledPlacement, RuntimeError> {
        let (reply, placed) = oneshot::channel();
        self.send(Command::Schedule(Box::new(request), reply))
            .await?;
        placed.await.map_err(|_| RuntimeError::SchedulerStopped)?
    }

    pub async fn cancel(&self, id: ScheduledRunId) -> Result<bool, RuntimeError> {
        Ok(self.repo.cancel(id, self.clock.now()).await?)
    }

    pub async fn cancel_by_key(&self, key: &str) -> Result<bool, RuntimeError> {
        let Some(key) = normalized_key(Some(key.to_owned()))? else {
            return Ok(false);
        };
        Ok(self.repo.cancel_by_key(&key, self.clock.now()).await?)
    }
    pub async fn run_now(&self, id: ScheduledRunId) -> Result<HandOff, RuntimeError> {
        let (reply, handed) = oneshot::channel();
        self.send(Command::RunNow(id, reply)).await?;
        handed.await.map_err(|_| RuntimeError::SchedulerStopped)?
    }

    async fn send(&self, command: Command) -> Result<(), RuntimeError> {
        self.commands
            .send(command)
            .await
            .map_err(|_| RuntimeError::SchedulerStopped)
    }
}
