use std::path::PathBuf;
use std::time::Duration;

use super::game_guard::GameGuard;
use super::live_paths::LivePaths;
use super::process::ForgeProcess;
use super::spec::{ForgeCommand, LaunchSpec};
use crate::EmulatorError;
use crate::control::{ControlClient, EventStream};
use crate::fixture::{Fixture, ForgeDataDir, SeedReport, seed};

pub struct LaunchOptions {
    /// The forge-emulator executable, run as the seeder child.
    pub emulator: PathBuf,
    pub forge: ForgeCommand,
    /// Each attempt seeds and runs under `<run_root>/attempt-<n>/{data,home}`, kept for inspection.
    pub run_root: PathBuf,
    pub fixture: Fixture,
    pub endpoint_overrides: Vec<(&'static str, String)>,
    pub log_directives: String,
    pub guard: GameGuard,
    pub live: LivePaths,
    pub ready_timeout: Duration,
    /// Bounds relaunches after forge loses the race for its seeded server port.
    pub max_attempts: u32,
    pub shutdown_grace: Duration,
}

/// Holds the authenticated control connection that proved readiness.
pub struct LaunchedForge {
    pub process: ForgeProcess,
    pub client: ControlClient,
    pub events: EventStream,
    pub seed: SeedReport,
    pub attempts: u32,
}

pub async fn launch_forge(options: &LaunchOptions) -> Result<LaunchedForge, EmulatorError> {
    if options.max_attempts == 0 {
        return Err(EmulatorError::InvalidLaunch {
            reason: "at least one launch attempt is required".to_owned(),
        });
    }
    for attempt in 1..=options.max_attempts {
        let attempt_dir = options.run_root.join(format!("attempt-{attempt}"));
        let data = attempt_dir.join("data");
        let home = attempt_dir.join("home");
        std::fs::create_dir_all(&data).map_err(|e| EmulatorError::DataDir {
            path: data.clone(),
            reason: e.to_string(),
        })?;
        options.live.refuse_overlap(&data, &home)?;
        let data_dir = ForgeDataDir::fresh(&data)?;
        let seed_report = seed(&options.emulator, &data, &options.fixture).await?;

        let spec = LaunchSpec {
            forge: options.forge.clone(),
            data_dir,
            home,
            twitch_client_id: seed_report
                .twitch
                .as_ref()
                .map(|twitch| twitch.client_id.clone()),
            endpoint_overrides: options.endpoint_overrides.clone(),
            log_directives: options.log_directives.clone(),
        };
        let mut process = ForgeProcess::spawn(&spec, &options.guard, &options.live).await?;
        let readiness = process
            .wait_ready(
                seed_report.server.port,
                &seed_report.server.bearer_token,
                options.ready_timeout,
            )
            .await;
        match readiness {
            Ok((client, events)) => {
                return Ok(LaunchedForge {
                    process,
                    client,
                    events,
                    seed: seed_report,
                    attempts: attempt,
                });
            }
            Err(EmulatorError::ServerPortTaken { .. }) => {
                process.shutdown(options.shutdown_grace).await?;
            }
            Err(failure) => {
                process.shutdown(options.shutdown_grace).await?;
                return Err(failure);
            }
        }
    }
    Err(EmulatorError::ServerPortTaken {
        attempts: options.max_attempts,
    })
}
