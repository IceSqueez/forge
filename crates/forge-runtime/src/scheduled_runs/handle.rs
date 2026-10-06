use std::sync::Arc;

use forge_storage::{ScheduledRunId, ScheduledRunRepo};
use tokio::sync::{mpsc, oneshot, watch};

use super::clock::WallClock;
use super::hand_off::HandOff;
use super::request::{ScheduleError, ScheduleRequest, ScheduledPlacement, normalized_key};
use super::waiting::{WaitingForQueueWatch, WaitingRuns};
use crate::task_stop::TaskStop;

pub(super) enum Command {
    Schedule(
        ScheduleRequest,
        oneshot::Sender<Result<ScheduledPlacement, ScheduleError>>,
    ),
    RunNow(
        ScheduledRunId,
        oneshot::Sender<Result<HandOff, ScheduleError>>,
    ),
}

#[derive(Clone)]
pub struct ScheduledRunsHandle {
    commands: mpsc::Sender<Command>,
    repo: Arc<dyn ScheduledRunRepo>,
    clock: Arc<dyn WallClock>,
    waiting: watch::Receiver<WaitingRuns>,
    stop: TaskStop,
}

impl ScheduledRunsHandle {
    pub(super) fn new(
        commands: mpsc::Sender<Command>,
        repo: Arc<dyn ScheduledRunRepo>,
        clock: Arc<dyn WallClock>,
        waiting: watch::Receiver<WaitingRuns>,
        stop: TaskStop,
    ) -> Self {
        Self {
            commands,
            repo,
            clock,
            waiting,
            stop,
        }
    }

    pub async fn stop(self) {
        self.stop.stop().await;
    }

    pub fn watch_waiting_for_queue(&self) -> WaitingForQueueWatch {
        WaitingForQueueWatch::new(self.waiting.clone())
    }

    pub async fn schedule(
        &self,
        request: ScheduleRequest,
    ) -> Result<ScheduledPlacement, ScheduleError> {
        let (reply, placed) = oneshot::channel();
        self.send(Command::Schedule(request, reply)).await?;
        placed.await.map_err(|_| ScheduleError::SchedulerStopped)?
    }

    pub async fn cancel(&self, id: ScheduledRunId) -> Result<bool, ScheduleError> {
        Ok(self.repo.cancel(id, self.clock.now()).await?)
    }

    pub async fn cancel_by_key(&self, key: &str) -> Result<bool, ScheduleError> {
        let Some(key) = normalized_key(Some(key.to_owned()))? else {
            return Ok(false);
        };
        Ok(self.repo.cancel_by_key(&key, self.clock.now()).await?)
    }
    pub async fn run_now(&self, id: ScheduledRunId) -> Result<HandOff, ScheduleError> {
        let (reply, handed) = oneshot::channel();
        self.send(Command::RunNow(id, reply)).await?;
        handed.await.map_err(|_| ScheduleError::SchedulerStopped)?
    }

    async fn send(&self, command: Command) -> Result<(), ScheduleError> {
        self.commands
            .send(command)
            .await
            .map_err(|_| ScheduleError::SchedulerStopped)
    }
}
