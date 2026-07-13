use std::sync::Arc;

use forge_components::{
    DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_LG, FONT_SM, ForgePalette, Icon,
    Radius, Spacing, card, icon, primary_button, radius, spacing,
};
use forge_events::EventsError;
use forge_runtime::EventBus;
use gpui::{
    AnyElement, App, AppContext, AsyncApp, Context, Entity, Window, WindowHandle, div, prelude::*,
};

use crate::boot::{BootFailure, build_runtime};
use crate::chat_feed::ChatFeed;
use crate::event_log::EventLog;
use crate::globals::Globals;
use crate::home_stats::HomeStats;
use crate::platforms::PlatformConnectivity;
use crate::presentation::{ActivePresentation, Presentation};
use crate::runtime_handles::RuntimeHandles;
use crate::runtime_status::RuntimeStatus;
use crate::shell::AppShell;
use crate::topics::Topics;

/// The two-phase boot state. `Ready` wraps today's shell verbatim; the two `Failed`
/// variants are differentiated by cause so the shell routes to the matching screen.
enum BootState {
    Booting,
    Ready {
        shell: Entity<AppShell>,
        // Owned here to keep the runtime's engine/scheduler/evaluator tasks alive;
        // screens read these handles once their phase wires them.
        #[allow(dead_code)]
        handles: RuntimeHandles,
    },
    Failed(BootFailure),
}

/// Window-root boot state machine. Opens immediately in `Booting`; a boot task builds
/// the runtime off the foreground executor and transitions this into `Ready` (handing
/// in the shell + handle bundle) or a differentiated `Failed` screen. Holds the tokio
/// handle + its own window handle so a failed boot's Retry can re-run construction.
pub struct RootView {
    state: BootState,
    rt_handle: tokio::runtime::Handle,
    window: Option<WindowHandle<RootView>>,
}

impl RootView {
    pub fn new(rt_handle: tokio::runtime::Handle, cx: &mut Context<Self>) -> Self {
        cx.observe_global::<Presentation>(|_, cx| cx.notify())
            .detach();
        Self {
            state: BootState::Booting,
            rt_handle,
            window: None,
        }
    }

    pub fn set_window(&mut self, window: WindowHandle<RootView>) {
        self.window = Some(window);
    }

    fn mark_ready(&mut self, shell: Entity<AppShell>, handles: RuntimeHandles) {
        self.state = BootState::Ready { shell, handles };
    }

    fn mark_failed(&mut self, failure: BootFailure) {
        self.state = BootState::Failed(failure);
    }

    fn retry(&mut self, cx: &mut Context<Self>) {
        let Some(window) = self.window else {
            return;
        };
        let rt_handle = self.rt_handle.clone();
        self.state = BootState::Booting;
        cx.notify();
        run_boot(rt_handle, window, cx);
    }
}

/// Kicks off the two-phase boot: constructs the seeded topic caches, spawns the
/// runtime construction on the tokio runtime, then awaits its result on the foreground
/// executor and transitions the window root. Re-usable for the initial boot and Retry.
pub fn run_boot(rt_handle: tokio::runtime::Handle, window: WindowHandle<RootView>, cx: &mut App) {
    let status = cx.new(|_| RuntimeStatus::new());
    let chat_feed = cx.new(|_| ChatFeed::seeded());
    let home_stats = cx.new(|_| HomeStats::seeded());
    let event_log = cx.new(|_| EventLog::seeded());
    let globals = cx.new(|_| Globals::seeded());
    let platforms = cx.new(|_| PlatformConnectivity::seeded());

    let (result_tx, result_rx) =
        tokio::sync::oneshot::channel::<Result<RuntimeHandles, BootFailure>>();
    rt_handle.spawn(async move {
        let _ = result_tx.send(build_runtime().await);
    });

    cx.spawn(async move |cx| {
        let outcome = match result_rx.await {
            Ok(outcome) => outcome,
            Err(_) => Err(BootFailure::Retry {
                reason: "runtime construction task ended before reporting a result".to_owned(),
            }),
        };
        match outcome {
            Ok(handles) => {
                let bus = Arc::clone(&handles.bus);
                let status_for_bridge = status.clone();
                let chat_feed_for_bridge = chat_feed.clone();
                let home_stats_for_bridge = home_stats.clone();
                let event_log_for_bridge = event_log.clone();
                let applied = window.update(cx, |root, window, cx| {
                    let topics = Topics::new(chat_feed, home_stats, event_log, globals, platforms);
                    let shell = cx.new(|cx| AppShell::new(status, topics, window, cx));
                    root.mark_ready(shell, handles);
                    cx.notify();
                });
                if applied.is_ok() {
                    start_bridge(
                        cx,
                        status_for_bridge,
                        chat_feed_for_bridge,
                        home_stats_for_bridge,
                        event_log_for_bridge,
                        bus,
                    );
                }
            }
            Err(failure) => {
                let _ = window.update(cx, |root, _window, cx| {
                    root.mark_failed(failure);
                    cx.notify();
                });
            }
        }
    })
    .detach();
}

/// Boot-global bus drain, owned by the shell for the app's lifetime. It subscribes to
/// the real bus and folds each observability row into the matching topic cache, from
/// which observing views repaint. It is the single insertion point later phases extend
/// (connection dots, dashboard counters, uptime); at idle boot the bus is quiet, so the
/// seeded screens are unchanged.
fn start_bridge(
    cx: &mut AsyncApp,
    status: Entity<RuntimeStatus>,
    chat_feed: Entity<ChatFeed>,
    home_stats: Entity<HomeStats>,
    event_log: Entity<EventLog>,
    bus: Arc<EventBus>,
) {
    cx.spawn(async move |cx| {
        let mut subscription = bus.subscribe();
        loop {
            match subscription.recv().await {
                Ok(event) => {
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
                            break;
                        }
                    } else if event.kind == "action.done" {
                        if home_stats
                            .update(cx, |stats, cx| {
                                stats.record_action_done();
                                cx.notify();
                            })
                            .is_err()
                        {
                            break;
                        }
                    } else if let Some(message) = ChatFeed::message_from_event(&event)
                        && chat_feed
                            .update(cx, |feed, cx| {
                                feed.push(message);
                                cx.notify();
                            })
                            .is_err()
                    {
                        break;
                    }
                }
                Err(EventsError::LaggingReceiver) => {}
                Err(_) => break,
            }
        }
    })
    .detach();
}

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        match &self.state {
            BootState::Booting => splash(&palette, density),
            BootState::Ready { shell, .. } => {
                div().size_full().child(shell.clone()).into_any_element()
            }
            BootState::Failed(BootFailure::UpgradeRequired { expected, found }) => {
                upgrade_screen(*expected, *found, &palette, density)
            }
            BootState::Failed(BootFailure::Retry { reason }) => {
                retry_screen(reason, cx.entity(), &palette, density)
            }
        }
    }
}

fn centered(child: impl IntoElement, palette: &ForgePalette, density: Density) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(palette.base)
        .p(spacing(Spacing::Lg, density))
        .child(child)
        .into_any_element()
}

fn splash(palette: &ForgePalette, density: Density) -> AnyElement {
    let column = div()
        .flex()
        .flex_col()
        .items_center()
        .gap(spacing(Spacing::Sm, density))
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_LG)
                .text_color(palette.brand)
                .child("forge"),
        )
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child("Starting…"),
        );
    centered(column, palette, density)
}

fn upgrade_screen(
    expected: u32,
    found: u32,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let body = div()
        .flex()
        .flex_col()
        .items_center()
        .gap(spacing(Spacing::Md, density))
        .child(icon(Icon::AlertTriangle, FONT_LG, palette.warning))
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_LG)
                .text_color(palette.text_primary)
                .child("Update required"),
        )
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_secondary)
                .child(format!(
                    "Your forge data uses schema version {found}, newer than this build's version {expected}. Update forge to the latest release to open it."
                )),
        )
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child("Your data is safe and untouched."),
        );
    centered(card(body, palette), palette, density)
}

fn retry_screen(
    reason: &str,
    root: Entity<RootView>,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let detail = div()
        .w_full()
        .p(spacing(Spacing::Sm, density))
        .rounded(radius(Radius::Sm))
        .bg(palette.surface_overlay)
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_SM)
        .text_color(palette.text_secondary)
        .child(reason.to_owned());

    let retry =
        primary_button("Retry", palette).on_click("boot-retry", move |_, _window, cx: &mut App| {
            root.update(cx, |root, cx| root.retry(cx));
        });

    let body = div()
        .flex()
        .flex_col()
        .items_center()
        .gap(spacing(Spacing::Md, density))
        .child(icon(Icon::AlertTriangle, FONT_LG, palette.warning))
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_LG)
                .text_color(palette.text_primary)
                .child("Couldn't open your data"),
        )
        .child(detail)
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child("Your data is safe. If this keeps happening, please report it."),
        )
        .child(retry);
    centered(card(body, palette), palette, density)
}
