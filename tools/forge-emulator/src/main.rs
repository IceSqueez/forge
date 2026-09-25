use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand};
use forge_emulator::EmulatorError;
use forge_emulator::control::{
    ClientTimeouts, ControlClient, ControlEndpoint, EventFilter, Observation,
};
use forge_emulator::fixture::{Fixture, seed_forge_environment};
use forge_emulator::launch::{
    DEFAULT_LOG_DIRECTIVES, ForgeCommand, ForgeProcess, GameGuard, LaunchOptions, LaunchedForge,
    LivePaths, OutputStream, launch_forge,
};
use forge_emulator::report::{RunContext, RunReport, write_report};
use forge_emulator::run::{RunOptions, ScenarioVerdict, run_scenario};
use forge_emulator::scenario::load_scenario;
use forge_emulator::stress::{StressOptions, load_profile, run_stress, write_stress_report};
use forge_emulator::twitch::{FakeTwitch, FakeTwitchConfig};
use forge_events::Event;
use time::OffsetDateTime;
use tokio::io::AsyncReadExt;

const TOKEN_VARIABLE: &str = "FORGE_EMULATOR_TOKEN";
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);
const EXIT_TAIL_LINES: usize = 20;
const SCENARIO_FAILED: u8 = 12;
const INTERRUPTED: u8 = 130;

#[derive(Parser)]
#[command(
    name = "forge-emulator",
    about = "Drive and observe a running forge over its control socket",
    after_help = "The server bearer token is read from FORGE_EMULATOR_TOKEN."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print every event forge publishes, one JSON object per line, until interrupted.
    Watch {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long)]
        port: u16,
        /// Print this many already-published events first; they may repeat as live events.
        #[arg(long, default_value_t = 0)]
        history: u32,
    },
    /// Seed the empty directory named by FORGE_DATA_DIR from a JSON fixture on stdin, then print
    /// the seed report (including the server bearer token) as one JSON line.
    Seed,
    /// Seed a fresh fixture, start the fake Twitch, launch forge against both, and print its
    /// events until interrupted. Opens a forge window; refuses while a game may be running.
    Launch {
        /// The built forge binary.
        #[arg(long)]
        forge: PathBuf,
        /// Fixture JSON file; the chat-command fixture when omitted.
        #[arg(long)]
        fixture: Option<PathBuf>,
        /// Parent of the per-attempt fixture directories; a new temp directory when omitted.
        #[arg(long)]
        run_root: Option<PathBuf>,
        /// forge's RUST_LOG filter.
        #[arg(long, default_value = DEFAULT_LOG_DIRECTIVES)]
        log: String,
        #[arg(long, default_value_t = 60)]
        ready_timeout_secs: u64,
        /// Relaunches allowed when forge loses the race for its seeded server port.
        #[arg(long, default_value_t = 3)]
        attempts: u32,
        /// Launch even while a game is on screen: the compositor is never asked.
        #[arg(long)]
        allow_over_game: bool,
    },
    /// Work with scenario files.
    Scenario {
        #[command(subcommand)]
        command: ScenarioCommand,
    },
    /// Throughput runs: step the inbound event rate until forge degrades.
    Stress {
        #[command(subcommand)]
        command: StressCommand,
    },
}

#[derive(Subcommand)]
enum StressCommand {
    /// Validate a stress profile without launching anything.
    Check { file: PathBuf },
    /// Run a stress profile against a freshly seeded forge and write `stress-report.md` and
    /// `samples.jsonl` into the run root. Opens a forge window; refuses while a game may be
    /// running.
    Run {
        file: PathBuf,
        /// The built forge binary; measure memory only on a release build.
        #[arg(long)]
        forge: PathBuf,
        #[arg(long)]
        run_root: Option<PathBuf>,
        /// forge's RUST_LOG filter; a debug filter distorts throughput.
        #[arg(long, default_value = "info")]
        log: String,
        #[arg(long, default_value_t = 3)]
        attempts: u32,
        #[arg(long)]
        allow_over_game: bool,
    },
}

#[derive(Subcommand)]
enum ScenarioCommand {
    /// Validate a scenario file without launching anything.
    Check { file: PathBuf },
    /// Run a scenario against a freshly seeded forge. Opens a forge window; refuses while a game
    /// may be running. Exits 0 when every step passed, 12 when the scenario failed, 130 when
    /// interrupted, and with a launch or harness code otherwise.
    Run {
        file: PathBuf,
        /// The built forge binary.
        #[arg(long)]
        forge: PathBuf,
        /// Parent of the per-attempt fixture directories; a new temp directory when omitted.
        #[arg(long)]
        run_root: Option<PathBuf>,
        /// forge's RUST_LOG filter; log_line expectations only see the targets it enables.
        #[arg(long, default_value = DEFAULT_LOG_DIRECTIVES)]
        log: String,
        /// Relaunches allowed when forge loses the race for its seeded server port.
        #[arg(long, default_value_t = 3)]
        attempts: u32,
        /// Run even while a game is on screen: the compositor is never asked.
        #[arg(long)]
        allow_over_game: bool,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    let Cli { command } = Cli::parse();
    let outcome = match command {
        Command::Watch {
            host,
            port,
            history,
        } => watch(&host, port, history)
            .await
            .map(|()| ExitCode::SUCCESS),
        Command::Seed => seed().await.map(|()| ExitCode::SUCCESS),
        Command::Launch {
            forge,
            fixture,
            run_root,
            log,
            ready_timeout_secs,
            attempts,
            allow_over_game,
        } => launch(LaunchArgs {
            forge,
            fixture,
            run_root,
            log,
            ready_timeout: Duration::from_secs(ready_timeout_secs),
            attempts,
            allow_over_game,
        })
        .await
        .map(|()| ExitCode::SUCCESS),
        Command::Scenario {
            command: ScenarioCommand::Check { file },
        } => check_scenario(&file).map(|()| ExitCode::SUCCESS),
        Command::Scenario {
            command:
                ScenarioCommand::Run {
                    file,
                    forge,
                    run_root,
                    log,
                    attempts,
                    allow_over_game,
                },
        } => {
            run(RunArgs {
                file,
                forge,
                run_root,
                log,
                attempts,
                allow_over_game,
            })
            .await
        }
        Command::Stress {
            command: StressCommand::Check { file },
        } => load_profile(&file).map(|profile| {
            println!("{}: ok, `{}`", file.display(), profile.name);
            ExitCode::SUCCESS
        }),
        Command::Stress {
            command:
                StressCommand::Run {
                    file,
                    forge,
                    run_root,
                    log,
                    attempts,
                    allow_over_game,
                },
        } => {
            stress(RunArgs {
                file,
                forge,
                run_root,
                log,
                attempts,
                allow_over_game,
            })
            .await
        }
    };
    match outcome {
        Ok(code) => code,
        Err(e) => {
            eprintln!("forge-emulator: {e}");
            ExitCode::from(exit_code(&e))
        }
    }
}

fn exit_code(error: &EmulatorError) -> u8 {
    match error {
        EmulatorError::GameInProgress { .. } | EmulatorError::GameStateUnreadable { .. } => 3,
        EmulatorError::LiveDataDir { .. } | EmulatorError::LiveHome { .. } => 4,
        EmulatorError::ForgeExited { .. } => 5,
        EmulatorError::EndpointOverrideRefused { .. } => 6,
        EmulatorError::ServerPortTaken { .. } => 7,
        EmulatorError::ReadinessTimeout { .. } => 8,
        EmulatorError::ScenarioUnreadable { .. } => 9,
        EmulatorError::ScenarioSyntax { .. } => 10,
        EmulatorError::ScenarioInvalid { .. } => 11,
        EmulatorError::StressProfileUnreadable { .. } => 10,
        EmulatorError::StressProfileInvalid { .. } => 11,
        _ => 1,
    }
}

struct LaunchArgs {
    forge: PathBuf,
    fixture: Option<PathBuf>,
    run_root: Option<PathBuf>,
    log: String,
    ready_timeout: Duration,
    attempts: u32,
    allow_over_game: bool,
}

fn game_guard(allow_over_game: bool) -> GameGuard {
    if allow_over_game {
        eprintln!("forge-emulator: game guard bypassed by --allow-over-game");
        return GameGuard::system().allow_over_game();
    }
    GameGuard::system()
}

async fn launch(args: LaunchArgs) -> Result<(), EmulatorError> {
    let fixture = match &args.fixture {
        Some(path) => read_fixture(path)?,
        None => Fixture::chat_command_mvp(),
    };
    fixture.validate()?;
    let run_root = prepare_run_root(args.run_root)?;
    let emulator = own_executable()?;
    let fake = match &fixture.twitch {
        Some(account) => Some(FakeTwitch::start(FakeTwitchConfig::for_account(account)).await?),
        None => None,
    };
    let options = LaunchOptions {
        emulator,
        forge: ForgeCommand::binary(args.forge),
        run_root: run_root.clone(),
        endpoint_overrides: fake
            .as_ref()
            .map(|fake| fake.endpoint_overrides().to_vec())
            .unwrap_or_default(),
        fixture,
        log_directives: args.log,
        guard: game_guard(args.allow_over_game),
        live: LivePaths::discover()?,
        ready_timeout: args.ready_timeout,
        max_attempts: args.attempts,
        shutdown_grace: SHUTDOWN_GRACE,
    };
    eprintln!("forge-emulator: run root {}", run_root.display());

    let stop = stop_requested();
    tokio::pin!(stop);
    // Why: dropping an unfinished launch drops its ForgeProcess, which kills forge's process group.
    let launched = tokio::select! {
        launched = launch_forge(&options) => launched?,
        () = &mut stop => return Ok(()),
    };
    let LaunchedForge {
        mut process,
        client,
        mut events,
        seed,
        attempts,
    } = launched;
    eprintln!(
        "forge-emulator: forge ready (pid {}, attempt {attempts}, server port {}, logs {})",
        process.pid(),
        seed.server.port,
        process.log_dir().display()
    );

    let outcome = print_until_stopped(&mut process, &client, &mut events, &mut stop).await;
    drop(client);
    let exit = process.shutdown(SHUTDOWN_GRACE).await?;
    eprintln!(
        "forge-emulator: forge stopped ({}{})",
        exit.status,
        if exit.forced {
            ", killed after grace"
        } else {
            ""
        }
    );
    if let Some(fake) = fake {
        fake.shutdown().await;
    }
    outcome
}

fn prepare_run_root(requested: Option<PathBuf>) -> Result<PathBuf, EmulatorError> {
    let run_root = match requested {
        Some(root) => root,
        None => std::env::temp_dir().join(format!("forge-emulator-{}", unique_suffix())),
    };
    std::fs::create_dir_all(&run_root).map_err(|e| EmulatorError::DataDir {
        path: run_root.clone(),
        reason: e.to_string(),
    })?;
    Ok(run_root)
}

fn own_executable() -> Result<PathBuf, EmulatorError> {
    std::env::current_exe().map_err(|e| EmulatorError::InvalidLaunch {
        reason: format!("own executable path: {e}"),
    })
}

struct RunArgs {
    file: PathBuf,
    forge: PathBuf,
    run_root: Option<PathBuf>,
    log: String,
    attempts: u32,
    allow_over_game: bool,
}

async fn run(args: RunArgs) -> Result<ExitCode, EmulatorError> {
    let scenario = load_scenario(&args.file)?;
    let run_root = prepare_run_root(args.run_root)?;
    eprintln!("forge-emulator: run root {}", run_root.display());
    let options = RunOptions {
        emulator: own_executable()?,
        forge: ForgeCommand::binary(args.forge.clone()),
        run_root: run_root.clone(),
        log_directives: args.log.clone(),
        guard: game_guard(args.allow_over_game),
        live: LivePaths::discover()?,
        max_attempts: args.attempts,
        shutdown_grace: SHUTDOWN_GRACE,
    };
    let started_at = OffsetDateTime::now_utc();
    let started = Instant::now();
    let outcome = run_scenario(&scenario, options, stop_requested()).await?;
    let context = RunContext {
        scenario_file: absolute(&args.file),
        forge_binary: absolute(&args.forge),
        log_directives: args.log,
        run_root: run_root.clone(),
        started_at,
        duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    };
    let report = RunReport::new(context, &scenario, &outcome)?;
    let output_error = |e: std::io::Error| EmulatorError::Output {
        reason: e.to_string(),
    };
    let mut out = std::io::stdout();
    writeln!(out, "{}", report.verdict_line())
        .and_then(|()| out.flush())
        .map_err(output_error)?;
    let files = write_report(&report, &run_root)?;
    writeln!(out, "report: {}", files.markdown.display())
        .and_then(|()| out.flush())
        .map_err(output_error)?;
    Ok(match outcome.verdict {
        ScenarioVerdict::Passed => ExitCode::SUCCESS,
        ScenarioVerdict::Failed => ExitCode::from(SCENARIO_FAILED),
        ScenarioVerdict::Interrupted => ExitCode::from(INTERRUPTED),
    })
}

async fn stress(args: RunArgs) -> Result<ExitCode, EmulatorError> {
    let profile = load_profile(&args.file)?;
    let run_root = prepare_run_root(args.run_root)?;
    eprintln!("forge-emulator: run root {}", run_root.display());
    let options = StressOptions {
        emulator: own_executable()?,
        forge: ForgeCommand::binary(args.forge.clone()),
        run_root: run_root.clone(),
        log_directives: args.log,
        guard: game_guard(args.allow_over_game),
        live: LivePaths::discover()?,
        max_attempts: args.attempts,
        shutdown_grace: SHUTDOWN_GRACE,
    };
    let outcome = tokio::select! {
        outcome = run_stress(&profile, options) => outcome?,
        () = stop_requested() => return Ok(ExitCode::from(INTERRUPTED)),
    };
    let files = write_stress_report(&profile, &outcome, &run_root, &absolute(&args.forge))?;
    println!("report: {}", files.markdown.display());
    println!("samples: {}", files.samples.display());
    Ok(ExitCode::SUCCESS)
}

fn absolute(path: &std::path::Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_owned())
}

async fn print_until_stopped(
    process: &mut ForgeProcess,
    client: &ControlClient,
    events: &mut forge_emulator::control::EventStream,
    stop: &mut std::pin::Pin<&mut impl std::future::Future<Output = ()>>,
) -> Result<(), EmulatorError> {
    client.subscribe(&[EventFilter::default()]).await?;
    let mut out = std::io::stdout();
    loop {
        tokio::select! {
            () = stop.as_mut() => return Ok(()),
            exit = process.exited() => {
                let exit = exit?;
                let output = process.output();
                return Err(EmulatorError::ForgeExited {
                    status: exit.status,
                    code: exit.code,
                    stderr_tail: output.tail(OutputStream::Stderr, EXIT_TAIL_LINES).join("\n"),
                    stdout_tail: output.tail(OutputStream::Stdout, EXIT_TAIL_LINES).join("\n"),
                });
            }
            observation = events.next() => match observation {
                Some(Observation::Event(event)) => print_event(&mut out, &event)?,
                Some(Observation::Dropped(count)) => {
                    eprintln!("forge-emulator: server dropped {count} events");
                }
                Some(Observation::Undecodable { frame, reason }) => {
                    eprintln!("forge-emulator: undecodable frame ({reason}): {frame}");
                }
                None => return Err(EmulatorError::ConnectionClosed),
            },
        }
    }
}

async fn stop_requested() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        if let (Ok(mut terminate), Ok(mut hangup)) = (
            signal(SignalKind::terminate()),
            signal(SignalKind::hangup()),
        ) {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = terminate.recv() => {}
                _ = hangup.recv() => {}
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

fn check_scenario(file: &std::path::Path) -> Result<(), EmulatorError> {
    let scenario = load_scenario(file)?;
    let output_error = |e: std::io::Error| EmulatorError::Output {
        reason: e.to_string(),
    };
    let mut out = std::io::stdout();
    writeln!(
        out,
        "{}: ok, `{}` with {} step(s)",
        file.display(),
        scenario.name,
        scenario.steps.len()
    )
    .and_then(|()| out.flush())
    .map_err(output_error)
}

fn read_fixture(path: &std::path::Path) -> Result<Fixture, EmulatorError> {
    let invalid = |reason: String| EmulatorError::InvalidFixture { reason };
    let bytes = std::fs::read(path).map_err(|e| invalid(format!("{}: {e}", path.display())))?;
    serde_json::from_slice(&bytes).map_err(|e| invalid(format!("{}: {e}", path.display())))
}

fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    format!("{}-{nanos}", std::process::id())
}

async fn watch(host: &str, port: u16, history: u32) -> Result<(), EmulatorError> {
    let token = std::env::var(TOKEN_VARIABLE)
        .ok()
        .filter(|token| !token.is_empty())
        .ok_or(EmulatorError::MissingToken {
            variable: TOKEN_VARIABLE,
        })?;
    let endpoint = ControlEndpoint::loopback(host, port)?;
    let (client, mut observations) =
        ControlClient::connect(&endpoint, ClientTimeouts::default()).await?;
    client.authenticate(&token).await?;
    client.subscribe(&[EventFilter::default()]).await?;

    let interrupted = tokio::signal::ctrl_c();
    tokio::pin!(interrupted);
    let mut out = std::io::stdout();
    if history > 0 {
        for event in client.recent_events(history).await? {
            print_event(&mut out, &event)?;
        }
    }
    loop {
        tokio::select! {
            _ = &mut interrupted => return Ok(()),
            observation = observations.next() => match observation {
                Some(Observation::Event(event)) => print_event(&mut out, &event)?,
                Some(Observation::Dropped(count)) => {
                    eprintln!("forge-emulator: server dropped {count} events");
                }
                Some(Observation::Undecodable { frame, reason }) => {
                    eprintln!("forge-emulator: undecodable frame ({reason}): {frame}");
                }
                None => return Err(EmulatorError::ConnectionClosed),
            },
        }
    }
}

async fn seed() -> Result<(), EmulatorError> {
    let mut input = Vec::new();
    tokio::io::stdin()
        .read_to_end(&mut input)
        .await
        .map_err(|e| EmulatorError::InvalidFixture {
            reason: e.to_string(),
        })?;
    let fixture: Fixture =
        serde_json::from_slice(&input).map_err(|e| EmulatorError::InvalidFixture {
            reason: e.to_string(),
        })?;
    let report = seed_forge_environment(&fixture).await?;
    let output_error = |reason: String| EmulatorError::Output { reason };
    let line = serde_json::to_string(&report).map_err(|e| output_error(e.to_string()))?;
    let mut out = std::io::stdout();
    writeln!(out, "{line}")
        .and_then(|()| out.flush())
        .map_err(|e| output_error(e.to_string()))
}

fn print_event(out: &mut impl Write, event: &Event) -> Result<(), EmulatorError> {
    let output_error = |reason: String| EmulatorError::Output { reason };
    let line = serde_json::to_string(event).map_err(|e| output_error(e.to_string()))?;
    writeln!(out, "{line}")
        .and_then(|()| out.flush())
        .map_err(|e| output_error(e.to_string()))
}
