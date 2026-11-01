mod app;
mod audience;
mod audio;
mod automation;
mod data_flows;
mod integrations;
mod preferences;
mod shared;
#[cfg(test)]
mod test_support;
mod tools;

use std::cell::Cell;
use std::rc::Rc;

#[cfg(target_os = "linux")]
use forge_components::ActivePresentation;
use forge_components::{FOOTER_HEIGHT, IconAssets};
use gpui::{
    AnyWindowHandle, App, AppContext, Bounds, Pixels, Point, QuitMode, SharedString, Size,
    TitlebarOptions, WindowBounds, WindowOptions, point, px, size,
};

use crate::app::actions::register_key_bindings;
use crate::app::log_tail::LogTail;
use crate::app::navigation::Screen;
use crate::app::root::{BootInputs, RestoreWaitView, RootView, run_boot};
use crate::app::shutdown::{RUNTIME_SHUTDOWN_BUDGET, RuntimeSlot, silence_logging};
use crate::app::sidebar::SIDEBAR_MAX;
use crate::app::titlebar::TITLEBAR_HEIGHT;
use crate::preferences::settings::{NAV_WIDTH, PANE_MIN_WIDTH};
use forge_components::{AccentColor, Density, FontFamilies, Presentation, ThemeId};

const SHELL_MIN_CONTENT_HEIGHT: Pixels = px(480.0);
const TRAFFIC_LIGHT_OFFSET: Point<Pixels> = point(px(14.0), px(10.0));
const RESTORE_WAIT_WINDOW_SIZE: Size<Pixels> = size(px(420.0), px(240.0));

const FLAG_SCREEN: &str = "--screen";
const FLAG_SELECT: &str = "--select";
const FLAG_HELP: &str = "--help";
const EXIT_USAGE_ERROR: i32 = 2;
const EXIT_STARTUP_FAILURE: i32 = 1;
const EXIT_RAISED_RUNNING_INSTANCE: i32 = 0;

enum StartupRequest {
    Open(Screen),
    Help,
}

#[derive(Debug, thiserror::Error)]
enum StartupArgsError {
    #[error("{0} requires a value")]
    MissingValue(&'static str),

    #[error("unrecognized argument: {0}")]
    Unrecognized(String),

    #[error("{FLAG_SELECT} requires {FLAG_SCREEN}")]
    SelectWithoutScreen,

    #[error("{0}")]
    InvalidScreen(String),
}

fn parse_startup_request(
    mut args: impl Iterator<Item = String>,
) -> Result<StartupRequest, StartupArgsError> {
    let mut screen_arg: Option<String> = None;
    let mut select_arg: Option<String> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            FLAG_HELP => return Ok(StartupRequest::Help),
            FLAG_SCREEN => {
                screen_arg = Some(
                    args.next()
                        .ok_or(StartupArgsError::MissingValue(FLAG_SCREEN))?,
                );
            }
            FLAG_SELECT => {
                select_arg = Some(
                    args.next()
                        .ok_or(StartupArgsError::MissingValue(FLAG_SELECT))?,
                );
            }
            forge_update::CANDIDATE_FLAG => {}
            data_flows::restore_relaunch::PREVIOUS_INSTANCE_FLAG => {
                args.next().ok_or(StartupArgsError::MissingValue(
                    data_flows::restore_relaunch::PREVIOUS_INSTANCE_FLAG,
                ))?;
            }
            other => return Err(StartupArgsError::Unrecognized(other.to_owned())),
        }
    }
    let Some(screen_arg) = screen_arg else {
        return match select_arg {
            Some(_) => Err(StartupArgsError::SelectWithoutScreen),
            None => Ok(StartupRequest::Open(Screen::Home)),
        };
    };
    let invalid_screen =
        |err: &dyn std::fmt::Display| StartupArgsError::InvalidScreen(err.to_string());
    let screen = Screen::parse_cli(&screen_arg).map_err(|err| invalid_screen(&err))?;
    let screen = match select_arg {
        Some(id) => screen
            .with_selected_entity(&id)
            .map_err(|err| invalid_screen(&err))?,
        None => screen,
    };
    Ok(StartupRequest::Open(screen))
}

#[expect(clippy::print_stdout, reason = "command-line help output")]
fn print_usage() {
    println!(
        "Usage: forge-desktop [{FLAG_SCREEN} <name>[:<param>]] [{FLAG_SELECT} <id>] [{FLAG_HELP}]"
    );
    println!();
    println!(
        "  {FLAG_SCREEN} <name>   open directly on a screen ({})",
        Screen::accepted_cli_names().join(", ")
    );
    println!("  {FLAG_SELECT} <id>    preselect an entity on the actions or triggers screen");
    println!("  {FLAG_HELP}          print this message");
}

fn init_tracing() -> (Option<tracing_appender::non_blocking::WorkerGuard>, LogTail) {
    use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

    let (env_filter, env_overridden) = match EnvFilter::try_from_default_env() {
        Ok(filter) => (filter, true),
        Err(_) => (crate::app::log_level::default_filter(), false),
    };
    let filter_layer = crate::app::log_level::reloadable(env_filter, env_overridden);
    let console_layer = fmt::layer()
        .with_target(false)
        .with_writer(crate::app::log_scrub::scrubbed(std::io::stdout));
    let log_tail = LogTail::new();

    const RETAINED_LOG_FILES: usize = 14;

    let log_dir = crate::app::app_paths::app_paths().logs_dir();
    let appender = std::fs::create_dir_all(&log_dir).ok().and_then(|()| {
        tracing_appender::rolling::Builder::new()
            .rotation(tracing_appender::rolling::Rotation::DAILY)
            .filename_prefix(app::log_archive::LOG_PREFIX)
            .max_log_files(RETAINED_LOG_FILES)
            .build(&log_dir)
            .ok()
    });
    let (file_layer, guard) = match appender {
        Some(appender) => {
            let (writer, guard) = tracing_appender::non_blocking(appender);
            let layer = app::log_archive::file_layer(crate::app::log_scrub::scrubbed(writer));
            (Some(layer), Some(guard))
        }
        None => (None, None),
    };

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(console_layer)
        .with(file_layer)
        .with(log_tail.layer())
        .init();

    if guard.is_some() {
        tracing::info!(path = %log_dir.display(), "file logging enabled");
    } else {
        tracing::warn!(
            "file logging disabled: could not create log directory or init rolling appender"
        );
    }
    (guard, log_tail)
}

#[cfg(target_os = "linux")]
fn prewarm_kit_fonts(cx: &mut App) {
    use gpui::{Font, FontWeight, font};

    let weights = [
        FontWeight::NORMAL,
        FontWeight::MEDIUM,
        FontWeight::SEMIBOLD,
        FontWeight::BOLD,
    ];
    let fonts: Vec<Font> = [cx.body_family(), cx.mono_family()]
        .into_iter()
        .flat_map(|family| {
            weights.map(|weight| Font {
                weight,
                ..font(family.clone())
            })
        })
        .collect();
    let text_system = cx.text_system().clone();
    cx.background_spawn(async move { text_system.prewarm_fonts(&fonts) })
        .detach();
}

fn lossy_cli_args() -> impl Iterator<Item = String> {
    std::env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
}

struct MainWindowLaunch {
    rt_handle: tokio::runtime::Handle,
    log_tail: LogTail,
    endpoints: forge_platform_core::PlatformEndpoints,
    initial_screen: Screen,
    window_failure: Rc<Cell<bool>>,
    splash: Option<AnyWindowHandle>,
}

enum StartupGate {
    Claimed(InstanceClaim),
    AwaitingPrevious(u32),
}

struct InstanceClaim {
    instance_lock: Option<app::instance_lock::InstanceLock>,
    restore_outcome: forge_transfer::RestoreOutcome,
}

fn claim_instance() -> Result<InstanceClaim, i32> {
    let instance_lock =
        match app::instance_lock::acquire(&crate::app::app_paths::app_paths().data_dir()) {
            app::instance_lock::LockOutcome::Acquired(lock) => Some(lock),
            app::instance_lock::LockOutcome::AlreadyRunning => {
                app::boot_signal::already_running();
                return Err(
                    match app::instance_raise::ask_running_instance(
                        &crate::app::app_paths::app_paths().data_dir(),
                    ) {
                        Ok(()) => EXIT_RAISED_RUNNING_INSTANCE,
                        Err(err) => {
                            tracing::warn!(error = %err, "running instance could not be raised");
                            EXIT_STARTUP_FAILURE
                        }
                    },
                );
            }
            app::instance_lock::LockOutcome::Unavailable(err) => {
                tracing::warn!(error = %err, "single-instance lock unavailable; starting anyway");
                None
            }
        };
    let restore_outcome = app::boot_restore::settle_pending_restore(
        &crate::app::app_paths::app_paths().data_dir(),
        instance_lock.is_some(),
    )
    .map_err(|err| {
        tracing::error!(error = %err, "pending restore could not be settled; exiting");
        EXIT_STARTUP_FAILURE
    })?;
    if instance_lock.is_some()
        && let Err(err) =
            forge_transfer::sweep_import_staging(&crate::app::app_paths::app_paths().data_dir())
    {
        tracing::warn!(error = %err, "leftover import staging could not be removed");
    }
    Ok(InstanceClaim {
        instance_lock,
        restore_outcome,
    })
}

fn await_previous_then_claim(previous: u32) -> Result<InstanceClaim, i32> {
    if data_flows::restore_relaunch::await_previous_exit(previous) {
        tracing::info!(previous, "previous forge instance exited");
    } else {
        tracing::warn!(
            previous,
            "previous forge instance still running after the wait"
        );
    }
    claim_instance()
}

fn finish_restore_first(previous: u32, mut launch: MainWindowLaunch, cx: &mut App) {
    cx.set_global(Presentation::accented(
        ThemeId::default(),
        AccentColor::default(),
        Density::default(),
    ));
    launch.splash = open_restore_wait_window(cx);
    let claim = launch
        .rt_handle
        .spawn_blocking(move || await_previous_then_claim(previous));
    cx.spawn(async move |cx| match claim.await {
        Ok(Ok(claim)) => cx.update(|cx| begin_boot(claim, launch, cx)),
        Ok(Err(exit_code)) => cx.update(|cx| {
            launch
                .window_failure
                .set(exit_code != EXIT_RAISED_RUNNING_INSTANCE);
            cx.quit();
        }),
        Err(error) => {
            tracing::error!(error = %error, "the startup wait stopped; exiting");
            cx.update(|cx| {
                launch.window_failure.set(true);
                cx.quit();
            });
        }
    })
    .detach();
}

fn open_restore_wait_window(cx: &mut App) -> Option<AnyWindowHandle> {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            RESTORE_WAIT_WINDOW_SIZE,
            cx,
        ))),
        titlebar: Some(TitlebarOptions {
            title: Some(SharedString::from("forge")),
            ..Default::default()
        }),
        app_id: Some("forge-desktop".to_owned()),
        ..Default::default()
    };
    cx.open_window(options, |_, cx| cx.new(|_| RestoreWaitView))
        .inspect_err(|err| tracing::warn!(error = %err, "restore wait window could not open"))
        .ok()
        .map(Into::into)
}

fn begin_boot(claim: InstanceClaim, launch: MainWindowLaunch, cx: &mut App) {
    let InstanceClaim {
        instance_lock,
        restore_outcome,
    } = claim;
    let holds_lock = instance_lock.is_some();
    cx.global::<data_flows::update_exit::ExitHandoff>()
        .adopt_lock(instance_lock);
    cx.set_global(app::boot_restore::BootRestore(restore_outcome));
    if holds_lock {
        match app::instance_raise::listen(&crate::app::app_paths::app_paths().data_dir()) {
            Ok(requests) => app::instance_raise::follow_raise_requests(requests, cx),
            Err(err) => tracing::warn!(error = %err, "raise endpoint unavailable"),
        }
    }
    let crate::app::boot::BootStorage {
        presentation,
        opening,
    } = crate::app::boot::open_boot_storage(&launch.rt_handle);
    cx.spawn(async move |cx| {
        let presentation = presentation.arrived().await;
        cx.update(|cx| open_main_window(presentation, opening, launch, cx));
    })
    .detach();
}

fn open_main_window(
    presentation: crate::app::boot::BootPresentation,
    opening: crate::app::boot::BackendOpening,
    launch: MainWindowLaunch,
    cx: &mut App,
) {
    let MainWindowLaunch {
        rt_handle,
        log_tail,
        endpoints,
        initial_screen,
        window_failure,
        splash,
    } = launch;
    let appearance = &presentation.appearance;
    cx.set_global(
        Presentation::accented(appearance.theme, appearance.accent, appearance.density).with_fonts(
            FontFamilies::with_overrides(
                appearance.body_font.clone().map(Into::into),
                appearance.mono_font.clone().map(Into::into),
            ),
        ),
    );
    #[cfg(target_os = "linux")]
    prewarm_kit_fonts(cx);

    let (window_bounds, display_id) = crate::app::window_placement::initial_window_bounds(
        presentation.window_placement.as_ref(),
        cx,
    );
    let options = WindowOptions {
        window_bounds: Some(window_bounds),
        display_id,
        titlebar: Some(TitlebarOptions {
            title: Some(SharedString::from("forge")),
            appears_transparent: true,
            traffic_light_position: Some(TRAFFIC_LIGHT_OFFSET),
        }),
        app_id: Some("forge-desktop".to_owned()),
        window_min_size: Some(size(
            SIDEBAR_MAX + NAV_WIDTH + PANE_MIN_WIDTH,
            TITLEBAR_HEIGHT + FOOTER_HEIGHT + SHELL_MIN_CONTENT_HEIGHT,
        )),
        ..Default::default()
    };

    let boot_inputs = BootInputs {
        rt_handle: rt_handle.clone(),
        log_tail: log_tail.clone(),
        endpoints: endpoints.clone(),
        initial_screen: initial_screen.clone(),
    };
    let boot_inputs_for_root = boot_inputs.clone();
    let placement_storage = presentation.backend.clone().map(|backend| {
        (
            backend,
            rt_handle.clone(),
            presentation.window_placement.clone(),
        )
    });
    let window = match cx.open_window(options, move |window, cx| {
        crate::app::window_presence::track_window_presence(window, cx);
        if let Some((backend, rt_handle, persisted)) = placement_storage {
            crate::app::window_placement::track_window_placement(
                window,
                backend.settings_repo(),
                rt_handle,
                persisted,
                cx,
            );
        }
        cx.new(|cx| RootView::new(boot_inputs_for_root.clone(), cx))
    }) {
        Ok(window) => window,
        Err(err) => {
            tracing::error!(error = %err, "failed to open window");
            window_failure.set(true);
            cx.quit();
            return;
        }
    };
    cx.activate(true);
    if let Some(splash) = splash
        && let Err(err) = splash.update(cx, |_, window, _| window.remove_window())
    {
        tracing::debug!(error = %err, "restore wait window was already gone");
    }

    window
        .update(cx, |root, _window, _cx| root.set_window(window))
        .ok();

    run_boot(boot_inputs, Some(opening), window, cx);
}

fn requested_screen_or_exit() -> Option<Screen> {
    match parse_startup_request(lossy_cli_args()) {
        Ok(StartupRequest::Open(screen)) => Some(screen),
        Ok(StartupRequest::Help) => {
            print_usage();
            None
        }
        #[expect(
            clippy::print_stderr,
            reason = "usage errors are reported before logging starts"
        )]
        Err(message) => {
            eprintln!("forge-desktop: {message}");
            std::process::exit(EXIT_USAGE_ERROR);
        }
    }
}

fn prepare_application(cx: &mut App) {
    if let Err(err) = cx
        .text_system()
        .add_fonts(forge_components::embedded_fonts())
    {
        tracing::error!(error = %err, "failed to register embedded fonts");
    }

    crate::app::presentation::seed_presentation(cx);
    cx.set_global(forge_components::Toasts::new());

    crate::app::i18n::install_os_default();

    register_key_bindings(cx);
    cx.on_action(|_: &crate::app::actions::Quit, cx| cx.quit());
    if cfg!(target_os = "macos") {
        cx.set_menus([gpui::Menu::new("forge").items([gpui::MenuItem::action(
            forge_components::tr!("app_menu_quit"),
            crate::app::actions::Quit,
        )])]);
    }
}

fn main() {
    let deferred_update_error =
        match data_flows::update_launch::decide_start(std::env::args_os().skip(1)) {
            data_flows::update_launch::StartDecision::Boot { deferred_error } => deferred_error,
            data_flows::update_launch::StartDecision::Exit(code) => std::process::exit(code),
        };
    app::crash_hook::install(crate::app::app_paths::app_paths().crashes_dir());

    let Some(initial_screen) = requested_screen_or_exit() else {
        return;
    };

    let (log_guard, log_tail) = init_tracing();
    if let Some(error) = deferred_update_error {
        tracing::warn!(error = %error, "pending update could not be resumed at start");
    }

    let endpoints = match forge_platform_core::PlatformEndpoints::from_env(
        &forge_host::integration_catalog::endpoint_surfaces(),
    ) {
        Ok(endpoints) => endpoints,
        Err(err) => {
            app::boot_signal::endpoint_override_refused(&err);
            drop(log_guard);
            std::process::exit(EXIT_STARTUP_FAILURE);
        }
    };

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(err) => {
            tracing::error!(error = %err, "failed to start tokio runtime");
            drop(log_guard);
            std::process::exit(EXIT_STARTUP_FAILURE);
        }
    };
    let rt_handle = rt.handle().clone();

    let guard_handle = rt_handle.clone();
    let rt_guard = guard_handle.enter();
    let startup_gate = match data_flows::restore_relaunch::previous_instance(lossy_cli_args()) {
        Some(previous) => StartupGate::AwaitingPrevious(previous),
        None => match claim_instance() {
            Ok(claim) => StartupGate::Claimed(claim),
            Err(exit_code) => {
                drop(log_guard);
                std::process::exit(exit_code);
            }
        },
    };
    let runtime_slot = RuntimeSlot::new(rt);
    let slot_for_app = runtime_slot.clone();
    let exit_handoff = data_flows::update_exit::ExitHandoff::new(None);
    let exit_for_app = exit_handoff.clone();
    let window_failed = Rc::new(Cell::new(false));
    let window_failure = Rc::clone(&window_failed);

    let application = gpui_platform::application()
        .with_assets(IconAssets)
        .with_quit_mode(QuitMode::LastWindowClosed);
    let _app_nap_guard = application
        .background_executor()
        .prevent_app_nap("forge drives a live stream from the background");

    application.run(move |cx: &mut App| {
        cx.set_global(slot_for_app);
        cx.set_global(exit_for_app);
        prepare_application(cx);

        let launch = MainWindowLaunch {
            rt_handle,
            log_tail,
            endpoints,
            initial_screen,
            window_failure,
            splash: None,
        };
        match startup_gate {
            StartupGate::Claimed(claim) => begin_boot(claim, launch, cx),
            StartupGate::AwaitingPrevious(previous) => finish_restore_first(previous, launch, cx),
        }
    });

    drop(rt_guard);
    if let Some(rt) = runtime_slot.take() {
        rt.shutdown_timeout(RUNTIME_SHUTDOWN_BUDGET);
    }
    exit_handoff.finish();
    silence_logging();
    if window_failed.get() {
        std::process::exit(EXIT_STARTUP_FAILURE);
    }
}

#[cfg(test)]
mod tests {
    use forge_update::CANDIDATE_FLAG;

    use super::{
        EXIT_RAISED_RUNNING_INSTANCE, EXIT_STARTUP_FAILURE, FLAG_SCREEN, FLAG_SELECT,
        StartupRequest, await_previous_then_claim, claim_instance, parse_startup_request,
    };
    use crate::app::instance_lock::{LockOutcome, acquire};
    use crate::app::instance_raise;
    use crate::app::navigation::Screen;
    use crate::data_flows::restore_relaunch::PREVIOUS_INSTANCE_FLAG;

    const CLAIM_CHILD_MARKER: &str = "FORGE_CLAIM_CHILD";

    fn is_claim_child() -> bool {
        std::env::var_os(CLAIM_CHILD_MARKER).is_some()
    }

    fn run_as_claim_child(test_name: &str, data_dir: &std::path::Path) {
        let (_binary, module) = module_path!().split_once("::").unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                format!("{module}::{test_name}").as_str(),
                "--exact",
                "--test-threads=1",
                "--nocapture",
            ])
            .env(CLAIM_CHILD_MARKER, "1")
            .env("FORGE_DATA_DIR", data_dir)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && stdout.contains("1 passed"),
            "{test_name} failed in its own process:\n{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn holding_the_lock(data_dir: &std::path::Path) -> crate::app::instance_lock::InstanceLock {
        match acquire(data_dir) {
            LockOutcome::Acquired(lock) => lock,
            LockOutcome::AlreadyRunning | LockOutcome::Unavailable(_) => {
                panic!("the parent process could not take the lock")
            }
        }
    }

    fn an_exited_process() -> u32 {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--list")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        child.wait().unwrap();
        child.id()
    }

    #[test]
    fn a_claim_on_a_free_data_directory_holds_the_lock() {
        if is_claim_child() {
            assert_eq!(
                claim_instance().map(|claim| claim.instance_lock.is_some()),
                Ok(true)
            );
            return;
        }
        let data = tempfile::tempdir().unwrap();

        run_as_claim_child(
            "a_claim_on_a_free_data_directory_holds_the_lock",
            data.path(),
        );
    }

    #[test]
    fn a_claim_with_no_usable_lock_file_starts_without_the_lock() {
        if is_claim_child() {
            assert_eq!(
                claim_instance().map(|claim| claim.instance_lock.is_some()),
                Ok(false)
            );
            return;
        }
        let data = tempfile::tempdir().unwrap();
        std::fs::create_dir(data.path().join("forge.lock")).unwrap();

        run_as_claim_child(
            "a_claim_with_no_usable_lock_file_starts_without_the_lock",
            data.path(),
        );
    }

    #[test]
    fn a_data_directory_that_cannot_settle_a_restore_is_a_startup_failure() {
        if is_claim_child() {
            assert_eq!(claim_instance().err(), Some(EXIT_STARTUP_FAILURE));
            return;
        }
        let data = tempfile::tempdir().unwrap();
        std::fs::create_dir(data.path().join(forge_transfer::RESTORE_MARKER_FILE)).unwrap();

        run_as_claim_child(
            "a_data_directory_that_cannot_settle_a_restore_is_a_startup_failure",
            data.path(),
        );
    }

    #[test]
    fn a_held_lock_with_no_instance_answering_is_a_startup_failure() {
        if is_claim_child() {
            assert_eq!(claim_instance().err(), Some(EXIT_STARTUP_FAILURE));
            return;
        }
        let data = tempfile::tempdir().unwrap();
        let _lock = holding_the_lock(data.path());

        run_as_claim_child(
            "a_held_lock_with_no_instance_answering_is_a_startup_failure",
            data.path(),
        );
    }

    #[test]
    fn a_held_lock_with_a_running_instance_raises_it_and_exits_cleanly() {
        if is_claim_child() {
            assert_eq!(claim_instance().err(), Some(EXIT_RAISED_RUNNING_INSTANCE));
            return;
        }
        let data = tempfile::tempdir().unwrap();
        let _lock = holding_the_lock(data.path());
        let rt = crate::test_support::worker_runtime();
        let _serving = rt.enter();
        let mut requests = instance_raise::listen(data.path()).unwrap();

        run_as_claim_child(
            "a_held_lock_with_a_running_instance_raises_it_and_exits_cleanly",
            data.path(),
        );

        assert_eq!(rt.block_on(requests.recv()), Some(()));
    }

    #[test]
    fn a_relaunch_that_finds_another_instance_running_refuses_to_start() {
        if is_claim_child() {
            let previous = an_exited_process();
            assert_eq!(
                await_previous_then_claim(previous).err(),
                Some(EXIT_STARTUP_FAILURE)
            );
            return;
        }
        let data = tempfile::tempdir().unwrap();
        let _lock = holding_the_lock(data.path());

        run_as_claim_child(
            "a_relaunch_that_finds_another_instance_running_refuses_to_start",
            data.path(),
        );
    }

    fn parse(args: &[&str]) -> Result<StartupRequest, String> {
        parse_startup_request(args.iter().map(|arg| (*arg).to_owned()))
            .map_err(|error| error.to_string())
    }

    fn opened(args: &[&str]) -> Option<Screen> {
        match parse(args) {
            Ok(StartupRequest::Open(screen)) => Some(screen),
            _ => None,
        }
    }

    #[test]
    fn the_update_candidate_flag_starts_exactly_as_if_it_were_absent() {
        let name = Screen::accepted_cli_names()[0];
        for (with_flag, without_flag) in [
            (vec![CANDIDATE_FLAG], vec![]),
            (
                vec![CANDIDATE_FLAG, FLAG_SCREEN, name],
                vec![FLAG_SCREEN, name],
            ),
            (
                vec![FLAG_SCREEN, name, CANDIDATE_FLAG],
                vec![FLAG_SCREEN, name],
            ),
        ] {
            let expected = opened(&without_flag);
            assert!(expected.is_some(), "baseline {without_flag:?} did not open");
            assert_eq!(opened(&with_flag), expected, "for {with_flag:?}");
        }
    }

    #[test]
    fn arguments_other_than_the_candidate_flag_still_fail_as_unrecognized() {
        for (args, offending) in [
            (
                vec!["--forge-update-candidates"],
                "--forge-update-candidates",
            ),
            (
                vec!["--forge-update-candidate=1"],
                "--forge-update-candidate=1",
            ),
            (vec!["--FORGE-UPDATE-CANDIDATE"], "--FORGE-UPDATE-CANDIDATE"),
            (
                vec!["--forge-update-supervisor"],
                "--forge-update-supervisor",
            ),
            (vec![CANDIDATE_FLAG, "--bogus"], "--bogus"),
        ] {
            let message = parse(&args).err();
            assert!(
                message
                    .as_deref()
                    .is_some_and(|m| m.contains("unrecognized") && m.contains(offending)),
                "{args:?} gave {message:?}"
            );
        }
    }

    #[test]
    fn the_restore_relaunch_flag_and_its_value_start_as_if_absent() {
        let name = Screen::accepted_cli_names()[0];
        for (with_flag, without_flag) in [
            (vec![PREVIOUS_INSTANCE_FLAG, "4242"], vec![]),
            (vec![PREVIOUS_INSTANCE_FLAG, "not-a-pid"], vec![]),
            (
                vec![PREVIOUS_INSTANCE_FLAG, "4242", FLAG_SCREEN, name],
                vec![FLAG_SCREEN, name],
            ),
            (
                vec![FLAG_SCREEN, name, PREVIOUS_INSTANCE_FLAG, "4242"],
                vec![FLAG_SCREEN, name],
            ),
        ] {
            let expected = opened(&without_flag);
            assert!(expected.is_some(), "baseline {without_flag:?} did not open");
            assert_eq!(opened(&with_flag), expected, "for {with_flag:?}");
        }
    }

    #[test]
    fn the_restore_relaunch_flag_without_a_value_is_a_usage_error() {
        let message = parse(&[
            FLAG_SCREEN,
            Screen::accepted_cli_names()[0],
            PREVIOUS_INSTANCE_FLAG,
        ])
        .err();

        assert!(
            message.as_deref().is_some_and(
                |m| m.contains(PREVIOUS_INSTANCE_FLAG) && m.contains("requires a value")
            ),
            "{message:?}"
        );
    }

    #[test]
    fn startup_usage_errors_print_the_exact_line_the_user_sees() {
        for (args, expected) in [
            (vec![FLAG_SCREEN], "--screen requires a value".to_owned()),
            (vec![FLAG_SELECT], "--select requires a value".to_owned()),
            (
                vec![PREVIOUS_INSTANCE_FLAG],
                format!("{PREVIOUS_INSTANCE_FLAG} requires a value"),
            ),
            (
                vec![FLAG_SELECT, "42"],
                "--select requires --screen".to_owned(),
            ),
            (vec!["--nope"], "unrecognized argument: --nope".to_owned()),
        ] {
            assert_eq!(parse(&args).err().as_deref(), Some(expected.as_str()));
        }
    }

    #[test]
    fn an_invalid_screen_name_prints_the_router_reason_unchanged() {
        let reason = Screen::parse_cli("no-such-screen")
            .err()
            .unwrap()
            .to_string();

        assert_eq!(
            parse(&[FLAG_SCREEN, "no-such-screen"]).err().as_deref(),
            Some(reason.as_str())
        );
    }
}
