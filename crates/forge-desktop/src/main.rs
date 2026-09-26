mod actions;
mod actions_screen;
mod app_shortcut_modal;
mod async_bridge;
mod audio_router;
mod audio_routes;
mod boot;
mod builtin_sections;
mod chat;
mod chat_drawer;
mod chat_feed;
mod chat_feed_bridge;
mod chrome;
mod clip_editor;
mod clip_hotkeys;
mod clip_key_state;
mod clip_messages;
mod clip_playback;
mod cloud_credentials;
mod cloud_tts_boot;
mod combo_capture;
mod combo_conflict;
mod config_form;
mod connect_flow;
mod data_backup;
mod diagnostic_bundle;
mod discord_screen;
mod discord_webhook_modal;
mod discord_webhooks;
mod event_feed;
mod event_log;
mod event_loss;
mod event_loss_card;
mod footer;
mod globals;
mod globals_view;
mod home;
mod home_stats;
mod hotkey_action_modal;
mod hotkey_bindings;
mod hotkey_sync;
mod hotkeys_screen;
mod i18n;
mod in_flight_steps;
mod instance_lock;
mod integration_detail;
mod integration_quick_action_modal;
mod integration_quick_actions;
mod integrations;
mod log_archive;
mod log_level;
mod log_scrub;
mod log_tail;
mod midi_mapping_modal;
mod midi_screen;
mod midi_signal;
mod obs_connect;
mod obs_credentials_form;
mod obs_settings_modal;
mod overlay_frame_sink;
mod overlay_url;
mod overlays_screen;
mod picker_favorites;
mod platforms;
mod presentation;
mod queue_health;
mod queues;
mod remote_audio;
mod root;
mod routed_sink;
mod run_history_modal;
mod runtime_handles;
mod runtime_status;
mod screen;
mod script_editor;
mod server_console;
mod server_restart;
mod settings;
mod settings_audio;
mod settings_audio_routing;
mod settings_diagnostics;
mod settings_scripting;
mod settings_shortcuts;
mod settings_storage;
mod settings_voice_gate;
mod settings_websocket;
mod shell;
mod shortcut_overrides;
mod shutdown;
mod sidebar;
mod soundboard;
mod speak_boot;
mod speak_bridge;
mod speak_state;
mod stay_awake;
mod stream_apps;
#[cfg(test)]
mod test_support;
mod titlebar;
mod toasts;
mod topics;
mod triggers_screen;
mod tts;
mod tts_dashboard;
mod tts_engines;
mod tts_filters;
mod unavailable_builtin;
mod update_check;
mod voice_aliases;
mod voice_gate;
mod vtube_connect;
mod vtube_connect_form;

use forge_components::{
    FOOTER_HEIGHT, IconAssets, bind_picker_keys, bind_text_area_keys, bind_text_input_keys,
};
use forge_platform_core::paths;
use gpui::{
    App, AppContext, Bounds, Pixels, SharedString, TitlebarOptions, WindowBounds, WindowOptions,
    point, px, size,
};

use crate::actions::{bind_list_keys, register_shell_key_bindings};
use crate::log_tail::LogTail;
use crate::presentation::Presentation;
use crate::root::{RootView, run_boot};
use crate::screen::Screen;
use crate::settings::{NAV_WIDTH, PANE_MIN_WIDTH};
use crate::sidebar::SIDEBAR_MAX;
use crate::titlebar::TITLEBAR_HEIGHT;

const SHELL_MIN_CONTENT_HEIGHT: Pixels = px(480.0);

const FLAG_SCREEN: &str = "--screen";
const FLAG_SELECT: &str = "--select";
const FLAG_HELP: &str = "--help";
const EXIT_USAGE_ERROR: i32 = 2;

enum StartupRequest {
    Open(Screen),
    Help,
}

fn parse_startup_request(mut args: impl Iterator<Item = String>) -> Result<StartupRequest, String> {
    let mut screen_arg: Option<String> = None;
    let mut select_arg: Option<String> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            FLAG_HELP => return Ok(StartupRequest::Help),
            FLAG_SCREEN => {
                screen_arg = Some(
                    args.next()
                        .ok_or_else(|| format!("{FLAG_SCREEN} requires a value"))?,
                );
            }
            FLAG_SELECT => {
                select_arg = Some(
                    args.next()
                        .ok_or_else(|| format!("{FLAG_SELECT} requires a value"))?,
                );
            }
            other => return Err(format!("unrecognized argument: {other}")),
        }
    }
    let Some(screen_arg) = screen_arg else {
        return match select_arg {
            Some(_) => Err(format!("{FLAG_SELECT} requires {FLAG_SCREEN}")),
            None => Ok(StartupRequest::Open(Screen::Home)),
        };
    };
    let screen = Screen::parse_cli(&screen_arg).map_err(|err| err.to_string())?;
    let screen = match select_arg {
        Some(id) => screen
            .with_selected_entity(&id)
            .map_err(|err| err.to_string())?,
        None => screen,
    };
    Ok(StartupRequest::Open(screen))
}

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

    // A rejected `RUST_LOG` counts as absent: the level the process actually runs at came from
    // the default, so the persisted one still gets to drive.
    let (env_filter, env_overridden) = match EnvFilter::try_from_default_env() {
        Ok(filter) => (filter, true),
        Err(_) => (crate::log_level::default_filter(), false),
    };
    let filter_layer = crate::log_level::reloadable(env_filter, env_overridden);
    let console_layer = fmt::layer()
        .with_target(false)
        .with_writer(crate::log_scrub::scrubbed(std::io::stdout));
    let log_tail = LogTail::new();

    const RETAINED_LOG_FILES: usize = 14;

    let log_dir = paths::data_dir().join("logs");
    let appender = std::fs::create_dir_all(&log_dir).ok().and_then(|()| {
        tracing_appender::rolling::Builder::new()
            .rotation(tracing_appender::rolling::Rotation::DAILY)
            .filename_prefix("forge.log")
            .max_log_files(RETAINED_LOG_FILES)
            .build(&log_dir)
            .ok()
    });
    let (file_layer, guard) = match appender {
        Some(appender) => {
            let (writer, guard) = tracing_appender::non_blocking(appender);
            let layer = fmt::layer()
                .with_writer(crate::log_scrub::scrubbed(writer))
                .with_ansi(false)
                .with_target(true);
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

fn main() {
    let initial_screen = match parse_startup_request(std::env::args().skip(1)) {
        Ok(StartupRequest::Open(screen)) => screen,
        Ok(StartupRequest::Help) => {
            print_usage();
            return;
        }
        Err(message) => {
            eprintln!("forge-desktop: {message}");
            std::process::exit(EXIT_USAGE_ERROR);
        }
    };

    let (log_guard, log_tail) = init_tracing();

    let endpoints = match forge_platform_core::PlatformEndpoints::from_env() {
        Ok(endpoints) => endpoints,
        Err(err) => {
            tracing::error!(error = %err, "platform endpoint override refused; exiting");
            drop(log_guard);
            std::process::exit(1);
        }
    };

    let _instance_lock = match instance_lock::acquire(&paths::data_dir()) {
        instance_lock::LockOutcome::Acquired(lock) => Some(lock),
        instance_lock::LockOutcome::AlreadyRunning => {
            tracing::error!(
                "another forge instance is already running for this data directory; exiting"
            );
            drop(log_guard);
            std::process::exit(1);
        }
        instance_lock::LockOutcome::Unavailable(err) => {
            tracing::warn!(error = %err, "single-instance lock unavailable; starting anyway");
            None
        }
    };

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(err) => {
            eprintln!("forge-desktop: failed to start tokio runtime: {err}");
            return;
        }
    };
    let rt_handle = rt.handle().clone();

    // Keeps the main thread inside the runtime context, so the sqlx pool dropped during window-close teardown finds a Tokio context instead of panicking.
    let _rt_guard = rt.enter();

    // Owned for the whole of run() to keep the runtime's tasks and time driver alive.
    gpui_platform::application()
        .with_assets(IconAssets)
        .run(move |cx: &mut App| {
            // Must precede the window open, or real text falls back to gpui's built-in face.
            if let Err(err) = cx
                .text_system()
                .add_fonts(forge_components::embedded_fonts())
            {
                eprintln!("forge-desktop: failed to register embedded fonts: {err}");
            }

            let (theme, density) = crate::boot::read_persisted_presentation(&rt_handle);
            cx.set_global(Presentation::new(theme, density));

            let (body_font, mono_font) = crate::boot::read_persisted_fonts(&rt_handle);
            forge_components::set_body_family(body_font.map(Into::into));
            forge_components::set_mono_family(mono_font.map(Into::into));
            cx.set_global(crate::presentation::ActiveLanguage(
                forge_storage::Language::default(),
            ));
            cx.set_global(crate::toasts::Toasts::new());

            // Boot and failure screens render before storage resolves the saved language.
            crate::i18n::install_os_default();

            bind_text_input_keys(cx);
            bind_text_area_keys(cx);
            bind_picker_keys(cx);
            bind_list_keys(cx);
            register_shell_key_bindings(cx);

            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1080.0), px(720.0)),
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("forge")),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(14.0), px(10.0))),
                }),
                app_id: Some("forge-desktop".to_owned()),
                window_min_size: Some(size(
                    SIDEBAR_MAX + NAV_WIDTH + PANE_MIN_WIDTH,
                    TITLEBAR_HEIGHT + FOOTER_HEIGHT + SHELL_MIN_CONTENT_HEIGHT,
                )),
                ..Default::default()
            };

            let rt_handle_for_root = rt_handle.clone();
            let log_tail_for_root = log_tail.clone();
            let endpoints_for_root = endpoints.clone();
            let initial_screen_for_root = initial_screen.clone();
            let window = match cx.open_window(options, move |_window, cx| {
                cx.new(|cx| {
                    RootView::new(
                        rt_handle_for_root.clone(),
                        log_tail_for_root.clone(),
                        endpoints_for_root.clone(),
                        initial_screen_for_root.clone(),
                        cx,
                    )
                })
            }) {
                Ok(window) => window,
                Err(err) => {
                    eprintln!("forge-desktop: failed to open window: {err}");
                    return;
                }
            };
            cx.activate(true);

            window
                .update(cx, |root, _window, _cx| root.set_window(window))
                .ok();

            run_boot(
                rt_handle.clone(),
                log_tail.clone(),
                endpoints.clone(),
                initial_screen.clone(),
                window,
                cx,
            );
        });
}
