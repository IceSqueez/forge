mod actions;
mod actions_screen;
mod async_bridge;
mod boot;
mod builtin_sections;
mod chat;
mod chat_drawer;
mod chat_feed;
mod chrome;
mod cloud_credentials;
mod cloud_tts_boot;
mod event_feed;
mod event_log;
mod footer;
mod globals;
mod globals_view;
mod home;
mod home_stats;
mod i18n;
mod integration_detail;
mod integration_seed;
mod integrations;
mod oauth_connect;
mod picker_favorites;
mod platforms;
mod presentation;
mod queue_health;
mod queues;
mod root;
mod runtime_handles;
mod runtime_status;
mod screen;
mod script_editor;
mod server_console;
mod settings;
mod settings_audio;
mod settings_hotkeys;
mod settings_scripting;
mod settings_shortcuts;
mod settings_storage;
mod settings_websocket;
mod shell;
mod shutdown;
mod sidebar;
mod soundboard;
mod speak_boot;
mod speak_bridge;
mod speak_state;
mod stream_apps;
mod titlebar;
mod toasts;
mod topics;
mod triggers_screen;
mod tts;
mod tts_dashboard;
mod tts_engines;
mod tts_filters;
mod twitch_panel;
mod voice_aliases;

use forge_components::{IconAssets, bind_picker_keys, bind_text_area_keys, bind_text_input_keys};
use forge_platform_core::paths;
use gpui::{
    App, AppContext, Bounds, SharedString, TitlebarOptions, WindowBounds, WindowOptions, point, px,
    size,
};

use crate::actions::{bind_list_keys, register_shell_key_bindings};
use crate::presentation::Presentation;
use crate::root::{RootView, run_boot};

fn init_tracing() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let console_layer = fmt::layer().with_target(false);

    let log_dir = paths::data_dir().join("logs");
    let (file_layer, guard) = match std::fs::create_dir_all(&log_dir) {
        Ok(()) => {
            let appender = tracing_appender::rolling::daily(&log_dir, "forge.log");
            let (writer, guard) = tracing_appender::non_blocking(appender);
            let layer = fmt::layer()
                .with_writer(writer)
                .with_ansi(false)
                .with_target(true);
            (Some(layer), Some(guard))
        }
        Err(_) => (None, None),
    };

    tracing_subscriber::registry()
        .with(env_filter)
        .with(console_layer)
        .with(file_layer)
        .init();

    if guard.is_some() {
        tracing::info!(path = %log_dir.display(), "file logging enabled");
    } else {
        tracing::warn!("file logging disabled: could not create log directory");
    }
    guard
}

fn main() {
    let _log_guard = init_tracing();

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
                ..Default::default()
            };

            let rt_handle_for_root = rt_handle.clone();
            let window = match cx.open_window(options, move |_window, cx| {
                cx.new(|cx| RootView::new(rt_handle_for_root.clone(), cx))
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

            run_boot(rt_handle.clone(), window, cx);
        });
}
