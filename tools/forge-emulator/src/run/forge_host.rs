use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use time::OffsetDateTime;
use tokio::sync::{Mutex, MutexGuard};
use tokio::task::JoinHandle;

use super::journal::Journal;
use crate::EmulatorError;
use crate::control::{ControlClient, EventFilter};
use crate::launch::{ForgeExit, ForgeProcess, GameGuard, LaunchSpec, LivePaths};

const BOOT_HISTORY_LIMIT: u32 = 500;

pub struct LiveForge {
    pub process: Option<ForgeProcess>,
    pub client: ControlClient,
    feeder: Option<JoinHandle<()>>,
}

pub struct Relaunch {
    pub spec: LaunchSpec,
    pub guard: GameGuard,
    pub live: LivePaths,
    pub server_port: u16,
    pub bearer_token: String,
    pub filters: Vec<EventFilter>,
    pub shutdown_grace: Duration,
}

pub struct ForgeHost {
    live: Mutex<Option<LiveForge>>,
    last_pid: AtomicU32,
    journal: Journal,
    relaunch: Option<Relaunch>,
}

impl ForgeHost {
    pub fn new(
        process: ForgeProcess,
        client: ControlClient,
        feeder: JoinHandle<()>,
        journal: Journal,
        relaunch: Relaunch,
    ) -> Self {
        Self {
            last_pid: AtomicU32::new(process.pid()),
            live: Mutex::new(Some(LiveForge {
                process: Some(process),
                client,
                feeder: Some(feeder),
            })),
            journal,
            relaunch: Some(relaunch),
        }
    }

    pub fn attached(client: ControlClient, journal: Journal) -> Self {
        Self {
            last_pid: AtomicU32::new(0),
            live: Mutex::new(Some(LiveForge {
                process: None,
                client,
                feeder: None,
            })),
            journal,
            relaunch: None,
        }
    }

    pub async fn live(&self) -> MutexGuard<'_, Option<LiveForge>> {
        self.live.lock().await
    }

    pub async fn stop(&self) -> Result<ForgeExit, EmulatorError> {
        let relaunch = self.relaunch()?;
        let running = self.live.lock().await.take();
        let Some((Some(process), client, feeder)) = running.map(LiveForge::into_parts) else {
            return Err(refused("forge is not running, so there is nothing to stop"));
        };
        if let Some(feeder) = feeder {
            feeder.abort();
        }
        drop(client);
        process.shutdown(relaunch.shutdown_grace).await
    }

    pub async fn start(&self, ready_timeout: Duration) -> Result<u32, EmulatorError> {
        let relaunch = self.relaunch()?;
        let launched_at = OffsetDateTime::now_utc();
        let mut process =
            ForgeProcess::spawn(&relaunch.spec, &relaunch.guard, &relaunch.live).await?;
        let (client, events) = match process
            .wait_ready(relaunch.server_port, &relaunch.bearer_token, ready_timeout)
            .await
        {
            Ok(connected) => connected,
            Err(e) => {
                let _ = process.shutdown(relaunch.shutdown_grace).await;
                return Err(e);
            }
        };
        if !relaunch.filters.is_empty()
            && let Err(e) = client.subscribe(&relaunch.filters).await
        {
            drop(client);
            let _ = process.shutdown(relaunch.shutdown_grace).await;
            return Err(e);
        }
        match client.recent_events(BOOT_HISTORY_LIMIT).await {
            Ok(history) => self.journal.seed(
                history
                    .into_iter()
                    .filter(|event| event.timestamp >= launched_at)
                    .filter(|event| {
                        relaunch.filters.is_empty()
                            || relaunch.filters.iter().any(|filter| filter.admits(event))
                    })
                    .collect(),
            ),
            Err(e) => {
                drop(client);
                let _ = process.shutdown(relaunch.shutdown_grace).await;
                return Err(e);
            }
        }
        let pid = process.pid();
        self.last_pid.store(pid, Ordering::Relaxed);
        let feeder = self.journal.attach(events);
        *self.live.lock().await = Some(LiveForge {
            process: Some(process),
            client,
            feeder: Some(feeder),
        });
        Ok(pid)
    }

    pub fn last_pid(&self) -> u32 {
        self.last_pid.load(Ordering::Relaxed)
    }

    pub fn into_live(self) -> Option<LiveForge> {
        self.live.into_inner()
    }
}

impl ForgeHost {
    fn relaunch(&self) -> Result<&Relaunch, EmulatorError> {
        self.relaunch.as_ref().ok_or_else(|| {
            refused("this forge was not launched by the emulator, so it cannot be restarted")
        })
    }
}

impl LiveForge {
    pub fn into_parts(self) -> (Option<ForgeProcess>, ControlClient, Option<JoinHandle<()>>) {
        (self.process, self.client, self.feeder)
    }
}

fn refused(reason: &str) -> EmulatorError {
    EmulatorError::InvalidLaunch {
        reason: reason.to_owned(),
    }
}
