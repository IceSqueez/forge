mod actions;
mod actions_screen;
mod boot;
mod builtin_sections;
mod chat;
mod chat_feed;
mod chrome;
mod cloud_tts_boot;
mod cloud_tts_engines;
mod event_feed;
mod event_log;
mod footer;
mod globals;
mod globals_view;
mod home;
mod home_stats;
mod integration_detail;
mod integration_seed;
mod integrations;
mod platforms;
mod presentation;
mod queues;
mod root;
mod runtime_handles;
mod runtime_status;
mod screen;
mod script_editor;
mod server_console;
mod settings;
mod shell;
mod sidebar;
mod soundboard;
mod speak_boot;
mod speak_state;
mod stream_apps;
mod titlebar;
mod toasts;
mod topics;
mod triggers_registry;
mod tts;
mod tts_dashboard;
mod tts_engines;
mod tts_filters;
mod tts_triggers;
mod voice_aliases;

use forge_components::{Density, IconAssets, ThemeId, bind_text_area_keys, bind_text_input_keys};
use gpui::{
    App, AppContext, Application, Bounds, SharedString, TitlebarOptions, WindowBounds,
    WindowOptions, point, px, size,
};

use crate::actions::register_shell_key_bindings;
use crate::presentation::Presentation;
use crate::root::{RootView, run_boot};

/// Boots the gpui shell under the two-phase model: main owns the tokio runtime; the
/// window opens immediately on [`RootView`] in a `Booting` state; a boot task builds
/// the real runtime off the foreground executor (opening the data provider behind the
/// schema-version gate) and transitions the root into `Ready` or a differentiated
/// `Failed` screen. The runtime stays fully behind handles — no runtime state lives in
/// the UI, and no gpui type crosses back into any backend crate.
fn main() {
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(err) => {
            eprintln!("forge-desktop: failed to start tokio runtime: {err}");
            return;
        }
    };
    let rt_handle = rt.handle().clone();

    // `rt` stays owned by this frame for the whole of `run` (which blocks until the
    // app quits), keeping the runtime tasks and time driver alive.
    Application::new()
        .with_assets(IconAssets)
        .run(move |cx: &mut App| {
            // Register embedded typefaces BEFORE the window opens, or real text
            // falls back to gpui's built-in face.
            if let Err(err) = cx
                .text_system()
                .add_fonts(forge_components::embedded_fonts())
            {
                eprintln!("forge-desktop: failed to register embedded fonts: {err}");
            }

            cx.set_global(Presentation::new(ThemeId::default(), Density::default()));
            cx.set_global(crate::toasts::Toasts::new());

            bind_text_input_keys(cx);
            bind_text_area_keys(cx);
            register_shell_key_bindings(cx);

            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1080.0), px(720.0)),
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("forge")),
                    // Draw our own branded chrome. On
                    // macOS/Windows this hides the OS bar; the macOS traffic
                    // lights inset into the custom bar's left padding, vertically
                    // centered in its 32px height.
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

            // Record the window handle so a failed boot's Retry can re-transition it.
            window
                .update(cx, |root, _window, _cx| root.set_window(window))
                .ok();

            // Two-phase boot: window is already visible in `Booting`; construct the
            // runtime off the foreground executor and flip the root on completion.
            run_boot(rt_handle.clone(), window, cx);
        });
}
