use std::path::PathBuf;
use std::time::Duration;

use super::game_guard::GameGuard;
use super::live_paths::LivePaths;
use super::process::ForgeProcess;
use super::spec::{ForgeCommand, KickClient, LaunchSpec};
use crate::EmulatorError;
use crate::fakes::emotes::FakeEmotes;
use crate::fixture::{Fixture, ForgeDataDir, SeedReport, seed};
use crate::harness::control::{ControlClient, EventStream};

pub struct LaunchOptions {
    pub emulator: PathBuf,
    pub forge: ForgeCommand,
    pub run_root: PathBuf,
    pub fixture: Fixture,
    pub endpoint_overrides: Vec<(&'static str, String)>,
    pub log_directives: String,
    pub guard: GameGuard,
    pub live: LivePaths,
    pub ready_timeout: Duration,
    pub max_attempts: u32,
    pub shutdown_grace: Duration,
}

pub struct LaunchedForge {
    pub process: ForgeProcess,
    pub client: ControlClient,
    pub events: EventStream,
    pub seed: SeedReport,
    pub attempts: u32,
    pub spec: LaunchSpec,
    pub emotes: FakeEmotes,
}

pub async fn launch_forge(options: &LaunchOptions) -> Result<LaunchedForge, EmulatorError> {
    if options.max_attempts == 0 {
        return Err(EmulatorError::InvalidLaunch {
            reason: "at least one launch attempt is required".to_owned(),
        });
    }
    let emotes = FakeEmotes::start().await?;
    let endpoint_overrides = with_emote_overrides(&options.endpoint_overrides, &emotes);
    for attempt in 1..=options.max_attempts {
        let attempt_dir = options.run_root.join(format!("attempt-{attempt}"));
        let data = attempt_dir.join("data");
        let home = attempt_dir.join("home");
        options.live.refuse_overlap(&data, &home)?;
        std::fs::create_dir_all(&data).map_err(|e| EmulatorError::DataDir {
            path: data.clone(),
            reason: e.to_string(),
        })?;
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
            kick_client: options.fixture.kick.as_ref().map(|kick| KickClient {
                client_id: kick.client_id.clone(),
                client_secret: kick.client_secret.clone(),
            }),
            endpoint_overrides: endpoint_overrides.clone(),
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
                    spec,
                    emotes,
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

fn with_emote_overrides(
    overrides: &[(&'static str, String)],
    emotes: &FakeEmotes,
) -> Vec<(&'static str, String)> {
    let mut merged = overrides.to_vec();
    for (variable, url) in emotes.endpoint_overrides() {
        if !merged.iter().any(|(present, _)| *present == variable) {
            merged.push((variable, url));
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use forge_emotes::endpoint_surfaces::{ENDPOINT_SURFACES, SEVEN_TV_API};
    use forge_platform_twitch::endpoint_surfaces::TWITCH_API;

    use super::*;
    use crate::harness::launch::DEFAULT_LOG_DIRECTIVES;

    const LOOPBACK_ORIGIN: &str = "http://127.0.0.1:";

    fn spec_with(data: &std::path::Path, overrides: Vec<(&'static str, String)>) -> LaunchSpec {
        LaunchSpec {
            forge: ForgeCommand::binary("/nonexistent/forge"),
            data_dir: ForgeDataDir::fresh(data).unwrap(),
            home: PathBuf::from("/scratch/home"),
            twitch_client_id: None,
            kick_client: None,
            endpoint_overrides: overrides,
            log_directives: DEFAULT_LOG_DIRECTIVES.to_owned(),
        }
    }

    #[tokio::test]
    async fn every_launch_points_the_three_emote_providers_at_the_loopback_fake() {
        let emotes = FakeEmotes::start().await.unwrap();
        let caller = vec![(TWITCH_API.env_var(), "http://127.0.0.1:1".to_owned())];
        let merged: BTreeMap<&str, String> =
            with_emote_overrides(&caller, &emotes).into_iter().collect();
        for surface in ENDPOINT_SURFACES {
            let url = merged.get(surface.env_var()).map(String::as_str);
            assert!(
                url.is_some_and(|url| url.starts_with(LOOPBACK_ORIGIN)),
                "{}: {url:?}",
                surface.env_var()
            );
        }
        let data = tempfile::tempdir().unwrap();
        let spec = spec_with(data.path(), with_emote_overrides(&caller, &emotes));
        assert!(spec.validate().is_ok(), "{:?}", spec.validate());
        emotes.shutdown().await;
    }

    #[tokio::test]
    async fn a_caller_override_of_an_emote_provider_is_kept_over_the_fake() {
        let emotes = FakeEmotes::start().await.unwrap();
        let caller = vec![(SEVEN_TV_API.env_var(), "http://127.0.0.1:9".to_owned())];
        let seven_tv: Vec<String> = with_emote_overrides(&caller, &emotes)
            .into_iter()
            .filter(|(variable, _)| *variable == SEVEN_TV_API.env_var())
            .map(|(_, url)| url)
            .collect();
        assert_eq!(seven_tv, ["http://127.0.0.1:9"]);
        emotes.shutdown().await;
    }
}
