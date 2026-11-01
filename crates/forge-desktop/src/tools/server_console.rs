mod clients_section;
mod credentials;
mod metrics_section;
mod overlay_section;
mod polling;
mod server_control;
#[cfg(test)]
mod tests;

use crate::app::navigation::{NavRequested, Screen};
use crate::integrations::hub_crumb::{core_category, hub_crumb};
use crate::tools::server_bearer;
use forge_components::OverflowRevealingFocus;
use forge_components::{
    ActivePresentation, BreadcrumbCrumb, Confirm, FONT_XXS, ForgePalette, Spacing, grow_cell,
    page_frame, spacing, tr,
};
use forge_events::EventSource;
use forge_server::{ClientId, ServerHandle};
use forge_storage::{CredentialsRepo, SettingsRepo};
use forge_types::Secret;
use gpui::{
    App, Context, EventEmitter, Pixels, SharedString, Subscription, UniformListScrollHandle,
    Window, div, prelude::*, px,
};
use std::net::SocketAddr;
use std::sync::Arc;

const RECENT_CLIENT_WINDOW_SECONDS: i64 = 600;

const CONTROL_GLYPH: Pixels = px(12.0);
const LINK_GLYPH: Pixels = px(11.0);
const HEADER_GLYPH: Pixels = px(14.0);
const OVERLAY_PANEL_GROW: f32 = 1.0;
const CLIENTS_PANEL_GROW: f32 = 1.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SubscriptionChipKind {
    All,
    Source(EventSource),
    Unknown,
}

#[derive(Debug, Clone)]
struct OwnedSubscriptionChip {
    label: String,
    kind: SubscriptionChipKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClientLiveness {
    Active,
    Idle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum BindState {
    Bound(String),
    AwaitingAddress,
    Stopped,
}

#[derive(Debug, Clone)]
struct OwnedClientRow {
    id: ClientId,
    key: String,
    identification: String,
    origin_label: String,
    liveness: ClientLiveness,
    subscriptions: Vec<OwnedSubscriptionChip>,
    events_per_second: f32,
    uptime_short: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OwnedOverlayKind {
    File { html: bool },
    Dir { overlay: bool },
}

#[derive(Debug, Clone)]
struct OwnedOverlayEntry {
    name: String,
    kind: OwnedOverlayKind,
    size_bytes: u64,
    child_count: u32,
}

impl OwnedOverlayEntry {
    fn is_html(&self) -> bool {
        matches!(self.kind, OwnedOverlayKind::File { html: true })
    }

    fn is_file(&self) -> bool {
        matches!(self.kind, OwnedOverlayKind::File { .. })
    }

    fn hosts_overlay(&self) -> bool {
        matches!(self.kind, OwnedOverlayKind::Dir { overlay: true })
    }

    fn is_selectable(&self) -> bool {
        self.hosts_overlay() || self.is_file()
    }
}

#[derive(Debug, Clone, Default)]
struct ServerStats {
    events_per_second: f32,
    http_requests: u64,
    bandwidth_kilobytes_per_second: f32,
    total_bytes_sent: u64,
    total_events_out: u64,
    dropped_events: u64,
}

struct ServerConsoleServices {
    server: Option<ServerHandle>,
    rt_handle: tokio::runtime::Handle,
    credentials: Arc<dyn CredentialsRepo>,
    settings: Arc<dyn SettingsRepo>,
}

struct ServerLifecycleState {
    running: bool,
    enabled: bool,
    restarting: bool,
    bind_address: Option<SocketAddr>,
    routable_host: Option<String>,
    routable_host_resolved_for: Option<SocketAddr>,
}

struct ServerSecretsState {
    bearer_token: Secret,
    token_revealed: bool,
    overlay_token: Option<Secret>,
    regenerate_pending: bool,
}

struct ClientsState {
    connected: Vec<OwnedClientRow>,
    recent: usize,
    scroll: UniformListScrollHandle,
    pending_disconnect: Confirm<ClientId>,
}

#[derive(Default)]
struct TrafficState {
    throughput_samples: Vec<f32>,
    stats: ServerStats,
}

#[derive(Default)]
struct OverlayState {
    root: String,
    entries: Vec<OwnedOverlayEntry>,
    selected: Option<String>,
}

pub(crate) struct ServerConsoleView {
    services: ServerConsoleServices,
    lifecycle: ServerLifecycleState,
    secrets: ServerSecretsState,
    clients: ClientsState,
    traffic: TrafficState,
    overlay: OverlayState,
    _overlay_token_watch: Subscription,
}

impl ServerConsoleView {
    pub(crate) fn new(
        server: Option<ServerHandle>,
        rt_handle: tokio::runtime::Handle,
        credentials: Arc<dyn CredentialsRepo>,
        settings: Arc<dyn SettingsRepo>,
        cx: &mut Context<Self>,
    ) -> Self {
        let running = server
            .as_ref()
            .is_some_and(|handle| handle.run_state().is_running());
        let overlay_token = crate::tools::overlay_link_token::current(cx);
        let overlay_token_watch = crate::tools::overlay_link_token::follow(
            server.as_ref(),
            &rt_handle,
            |this: &mut Self, token| this.secrets.overlay_token = token,
            cx,
        );
        let view = Self {
            services: ServerConsoleServices {
                server,
                rt_handle,
                credentials,
                settings,
            },
            lifecycle: ServerLifecycleState {
                running,
                enabled: true,
                restarting: false,
                bind_address: None,
                routable_host: None,
                routable_host_resolved_for: None,
            },
            secrets: ServerSecretsState {
                bearer_token: Secret::new(""),
                token_revealed: false,
                overlay_token,
                regenerate_pending: false,
            },
            clients: ClientsState {
                connected: Vec::new(),
                recent: 0,
                scroll: UniformListScrollHandle::new(),
                pending_disconnect: Confirm::default(),
            },
            traffic: TrafficState::default(),
            overlay: OverlayState::default(),
            _overlay_token_watch: overlay_token_watch,
        };
        view.fetch_token(cx);
        view.fetch_enabled(cx);
        if view.services.server.is_some() {
            view.start_run_state_bridge(cx);
            view.start_poll(cx);
        }
        view
    }
}

impl EventEmitter<NavRequested> for ServerConsoleView {}

impl Render for ServerConsoleView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let panels = div()
            .w_full()
            .flex()
            .flex_row()
            .items_start()
            .gap(spacing(Spacing::Sm, density))
            .child(grow_cell(
                self.overlay_panel(&palette, density, cx),
                OVERLAY_PANEL_GROW,
            ))
            .child(grow_cell(
                self.clients_panel(&palette, density, cx),
                CLIENTS_PANEL_GROW,
            ));

        let body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Md, density))
            .px(spacing(Spacing::Lg, density))
            .children(crate::tools::server_unavailable::unavailable_banner(
                cx, &palette,
            ))
            .child(self.credentials_card(&palette, density, cx))
            .child(self.stats_grid(&palette, density, cx))
            .child(self.throughput_card(&palette, density, cx))
            .child(panels)
            .child(self.footer_bar(&palette, density, cx));

        let scroll = div()
            .id("srv-scroll")
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_revealing_focus()
            .child(body);

        let modal = self.disconnect_confirm(&palette, cx);
        let regenerate_modal = self.secrets.regenerate_pending.then(|| {
            server_bearer::regenerate_confirm(
                &palette,
                Self::cancel_regenerate,
                Self::confirm_regenerate,
                cx,
            )
        });

        let frame = page_frame(
            vec![
                hub_crumb(core_category(&Screen::Server), cx),
                BreadcrumbCrumb::leaf(tr!("server_breadcrumb_server")),
            ],
            &palette,
        )
        .header_right(
            div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xs, density))
                .children(self.disabled_hint(&palette, density, cx))
                .child(self.restart_control(&palette, cx))
                .child(self.breadcrumb_status(&palette, density)),
        )
        .body(
            div()
                .flex_1()
                .min_h(px(0.0))
                .w_full()
                .flex()
                .flex_col()
                .child(scroll),
        );

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(frame)
            .children(modal)
            .children(regenerate_modal)
    }
}

fn caption(label: impl Into<SharedString>, palette: &ForgePalette, cx: &App) -> impl IntoElement {
    div()
        .font_family(cx.mono_family())
        .text_size(FONT_XXS)
        .text_color(palette.text_muted)
        .child(label.into())
}
