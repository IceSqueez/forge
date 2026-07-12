mod actions;
mod builtin_sections;
mod chat;
mod chat_feed;
mod chrome;
mod event_feed;
mod event_log;
mod footer;
mod globals;
mod globals_view;
mod home;
mod home_stats;
mod integration_detail;
mod integration_seed;
mod platforms;
mod presentation;
mod runtime_status;
mod screen;
mod screen_stub;
mod settings;
mod shell;
mod sidebar;
mod stream_apps;
mod titlebar;
mod topics;

use std::sync::Arc;
use std::time::Duration;

use forge_components::{Density, IconAssets, ThemeId, bind_text_area_keys, bind_text_input_keys};
use forge_events::{Event, EventSource, EventsError};
use forge_runtime::{EventBus, NullEventLogRepo};
use gpui::{
    App, AppContext, Application, Bounds, SharedString, TitlebarOptions, WindowBounds,
    WindowOptions, point, px, size,
};

use crate::actions::register_shell_key_bindings;
use crate::chat_feed::ChatFeed;
use crate::event_log::EventLog;
use crate::globals::Globals;
use crate::home_stats::HomeStats;
use crate::platforms::PlatformConnectivity;
use crate::presentation::Presentation;
use crate::runtime_status::RuntimeStatus;
use crate::shell::AppShell;
use crate::topics::Topics;

/// Interval between synthetic `timer.tick` events published by the minimal runtime.
const TICK_INTERVAL: Duration = Duration::from_secs(1);

/// Boots the gpui app shell: the tokio runtime owns the bus and a tick publisher;
/// the shell registers fonts, installs the presentation global, binds keys, starts
/// the single runtime→UI bridge task, and opens the window on the root
/// [`AppShell`]. The runtime remains fully behind handles — no runtime state lives
/// in the UI, and no gpui type crosses back into any backend crate.
fn main() {
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(err) => {
            eprintln!("forge-desktop: failed to start tokio runtime: {err}");
            return;
        }
    };

    let bus = EventBus::new(Arc::new(NullEventLogRepo));

    // Minimal runtime: one tokio task publishing `timer.tick` observability events.
    // Stands in for the full timer subsystem — the shell only needs genuine bus
    // traffic to prove the drain-then-notify path into a topic entity.
    {
        let bus = Arc::clone(&bus);
        rt.spawn(async move {
            let mut ticker = tokio::time::interval(TICK_INTERVAL);
            loop {
                ticker.tick().await;
                bus.publish(Event::new(
                    EventSource::Timer,
                    "timer.tick",
                    serde_json::Value::Null,
                ));
            }
        });
    }

    // `rt` stays owned by this frame for the whole of `run` (which blocks until the
    // app quits), keeping the publisher task and time driver alive.
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

            bind_text_input_keys(cx);
            bind_text_area_keys(cx);
            register_shell_key_bindings(cx);

            let status = cx.new(|_| RuntimeStatus::new());
            // Seeded so the Chat screen renders visibly before any platform
            // connection exists; the bridge appends real events into this same
            // topic once platforms publish them.
            let chat_feed = cx.new(|_| ChatFeed::seeded());
            // Seeded so the Home dashboard renders visibly before any runtime
            // dashboard source exists; the bridge advances it (e.g. on `action.done`)
            // and real sources replace each field as they land.
            let home_stats = cx.new(|_| HomeStats::seeded());
            // Seeded so the Event Feed renders a representative sample before real
            // traffic; the bridge streams live observability events into it, so the
            // boot tick publisher's `timer.tick` rows accumulate on top of the seed.
            let event_log = cx.new(|_| EventLog::seeded());
            // Seeded so the Globals manager renders a representative sample across
            // all seven Variant kinds before a storage provider is wired; there is
            // no runtime source yet, so the bridge does not drain into it — edits
            // mutate this in-memory cache directly.
            let globals = cx.new(|_| Globals::seeded());
            // Seeded so the Platforms overview renders visibly before a connectivity
            // bridge exists; the bridge replaces each entry as the platform-connection
            // stream lands.
            let platforms = cx.new(|_| PlatformConnectivity::seeded());
            start_bridge(
                cx,
                status.clone(),
                chat_feed.clone(),
                home_stats.clone(),
                event_log.clone(),
                Arc::clone(&bus),
            );

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

            let status_for_window = status.clone();
            let chat_feed_for_window = chat_feed.clone();
            let home_stats_for_window = home_stats.clone();
            let event_log_for_window = event_log.clone();
            let globals_for_window = globals.clone();
            let platforms_for_window = platforms.clone();
            match cx.open_window(options, move |window, cx| {
                cx.new(|cx| {
                    AppShell::new(
                        status_for_window.clone(),
                        Topics::new(
                            chat_feed_for_window.clone(),
                            home_stats_for_window.clone(),
                            event_log_for_window.clone(),
                            globals_for_window.clone(),
                            platforms_for_window.clone(),
                        ),
                        window,
                        cx,
                    )
                })
            }) {
                Ok(_) => cx.activate(true),
                Err(err) => eprintln!("forge-desktop: failed to open window: {err}"),
            }
        });
}

/// Starts the single runtime→UI bridge task on the foreground executor. It owns
/// the bus subscription and, for each `timer.tick`, advances the topic entity +
/// `cx.notify()`, from which observing views repaint. Broadcast recv is
/// executor-agnostic (no tokio timer/reactor), so it runs safely off the gpui
/// foreground executor while the publisher runs on the tokio runtime.
fn start_bridge(
    cx: &mut App,
    status: gpui::Entity<RuntimeStatus>,
    chat_feed: gpui::Entity<ChatFeed>,
    home_stats: gpui::Entity<HomeStats>,
    event_log: gpui::Entity<EventLog>,
    bus: Arc<EventBus>,
) {
    cx.spawn(async move |cx| {
        let mut subscription = bus.subscribe();
        loop {
            match subscription.recv().await {
                Ok(event) => {
                    // Every observability event streams into the feed topic first
                    // (the feed's own Pause flag gates collection). This is the live
                    // runtime→UI edge the Event Feed screen observes; `timer.tick`
                    // rows accumulate here on top of the boot seed.
                    if let Some(item) = EventLog::item_from_event(&event)
                        && event_log
                            .update(cx, |log, cx| {
                                log.push(item);
                                cx.notify();
                            })
                            .is_err()
                    {
                        break;
                    }
                    if event.kind == "timer.tick" {
                        if status
                            .update(cx, |status, cx| {
                                status.tick();
                                cx.notify();
                            })
                            .is_err()
                        {
                            // Topic entity released → the app is shutting down.
                            break;
                        }
                    } else if event.kind == "action.done" {
                        // No runner publishes `action.done` in the minimal runtime yet,
                        // so this arm is dormant; it is the live path the Home
                        // fired-today counter advances through once the action engine
                        // runs.
                        if home_stats
                            .update(cx, |stats, cx| {
                                stats.record_action_done();
                                cx.notify();
                            })
                            .is_err()
                        {
                            break;
                        }
                    } else if let Some(message) = ChatFeed::message_from_event(&event) {
                        // No platform publishes `chat.message` yet, so this arm is
                        // dormant at runtime; it is the live path a real chat
                        // connection appends through once it lands.
                        if chat_feed
                            .update(cx, |feed, cx| {
                                feed.push(message);
                                cx.notify();
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                }
                // A lagging subscriber dropped some ticks; keep draining.
                Err(EventsError::LaggingReceiver) => {}
                // Bus closed (or any other terminal error) → stop the bridge.
                Err(_) => break,
            }
        }
    })
    .detach();
}
