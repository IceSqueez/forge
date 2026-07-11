mod state;

use std::sync::Arc;
use std::time::Duration;

use forge_components::IconAssets;
use forge_events::{Event, EventSource, EventsError};
use forge_runtime::{EventBus, NullEventLogRepo};
use gpui::{
    App, AppContext, Application, Bounds, SharedString, TitlebarOptions, WindowBounds,
    WindowOptions, px, size,
};
use state::UiState;

/// Interval between synthetic `timer.tick` events published by the minimal runtime.
const TICK_INTERVAL: Duration = Duration::from_secs(1);

/// Runtime → UI bridge: one live runtime value flows runtime → bridge task →
/// root `Entity<UiState>` → repaint.
///
/// The tokio runtime owns the bus and the tick publisher; the gpui shell holds
/// only an `Arc<EventBus>` handle and drains it from a single boot-time bridge
/// task. No runtime state lives in the UI; no shared mutable state crosses the
/// boundary in either direction.
fn main() {
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(err) => {
            eprintln!("forge-desktop: failed to start tokio runtime: {err}");
            return;
        }
    };

    let bus = EventBus::new(Arc::new(NullEventLogRepo));

    // Minimal runtime: a single tokio task publishing `timer.tick` observability
    // events onto the bus. Stands in for the full timer subsystem — the spike
    // only needs genuine bus traffic to prove the drain-then-notify path.
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

    // `rt` stays owned by this stack frame for the whole of `run` (which blocks
    // until the app quits), keeping the publisher task and time driver alive.
    Application::new()
        .with_assets(IconAssets)
        .run(move |cx: &mut App| {
            let state = cx.new(|_| UiState::new());

            start_bridge(cx, state.clone(), Arc::clone(&bus));

            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(720.0), px(480.0)),
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("forge runtime bridge")),
                    ..Default::default()
                }),
                app_id: Some("forge-desktop".to_owned()),
                ..Default::default()
            };

            let root = state.clone();
            match cx.open_window(options, move |_, _| root) {
                Ok(_) => cx.activate(true),
                Err(err) => eprintln!("forge-desktop: failed to open window: {err}"),
            }
        });
}

/// Starts the single runtime→UI bridge task on the foreground executor. It owns
/// the bus subscription, and for each `timer.tick` applies the new value onto
/// the root entity + `cx.notify()`, from which the view repaints. Broadcast recv
/// is executor-agnostic (no tokio timer/reactor), so it runs safely off the gpui
/// foreground executor while the publisher runs on the tokio runtime.
fn start_bridge(cx: &mut App, state: gpui::Entity<UiState>, bus: Arc<EventBus>) {
    cx.spawn(async move |cx| {
        let mut subscription = bus.subscribe();
        loop {
            match subscription.recv().await {
                Ok(event) => {
                    if event.kind == "timer.tick"
                        && state
                            .update(cx, |ui, cx| {
                                ui.on_timer_tick();
                                cx.notify();
                            })
                            .is_err()
                    {
                        // Root entity released → the app is shutting down.
                        break;
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
