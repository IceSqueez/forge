use forge_components::breadcrumb::BreadcrumbCrumb;
use forge_components::{
    BORDER_THIN, ColumnWidth, ConfirmTone, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, DataRow,
    Density, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon, OverlayPosition, PlatformKind, Radius,
    Spacing, badge, breadcrumb, card, column, confirm_modal, data_table, empty_state, fmt_bytes,
    fmt_uptime, fmt_uptime_short, icon, metric_card, overlay, platform_color, radius, spacing,
    sparkline, status_dot, tr, with_alpha,
};
use std::sync::Arc;
use std::time::Duration;

use forge_events::EventSource;
use forge_server::{ConnectedClientSnapshot, EventFilterSnapshot, ServerHandle, ServerSnapshot};
use forge_storage::{CredentialId, CredentialsRepo};
use gpui::{
    AnyElement, ClickEvent, ClipboardItem, Context, Div, FontWeight, Pixels, Rgba, SharedString,
    Window, div, prelude::*, px, relative,
};

use crate::presentation::ActivePresentation;

const BEARER_CREDENTIAL_ID: &str = "server:bearer";

const MAX_BANDWIDTH_SAMPLES: usize = 60;
const MAX_VISIBLE_CHIPS: usize = 3;

const CLIENT_DOT: Pixels = px(6.0);
const STATUS_DOT: Pixels = px(7.0);
const DOT_CELL_W: Pixels = px(24.0);
const EVS_CELL_W: Pixels = px(80.0);
const UPTIME_CELL_W: Pixels = px(70.0);
const X_CELL_W: Pixels = px(22.0);
const ICON_BOX: Pixels = px(48.0);
const SERVER_GLYPH: Pixels = px(20.0);
const X_GLYPH: Pixels = px(13.0);
const LINK_GLYPH: Pixels = px(11.0);
const CONTROL_GLYPH: Pixels = px(12.0);
const HEADER_GLYPH: Pixels = px(14.0);
const COUNT_BADGE_RADIUS: Pixels = px(8.0);
const KIND_BADGE_RADIUS: Pixels = px(4.0);
const CLIENT_GROW: f32 = 14.0;
const SUBS_GROW: f32 = 16.0;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum ServerStatus {
    Running,
    #[default]
    Stopped,
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServerControl {
    Restarting,
    Stopping,
}

#[derive(Debug, Clone)]
struct OwnedSubscriptionChip {
    label: String,
    source: EventSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClientLiveness {
    Active,
    Idle,
}

#[derive(Debug, Clone)]
struct OwnedClientRow {
    key: String,
    identification: String,
    client_type_label: String,
    liveness: ClientLiveness,
    subscriptions: Vec<OwnedSubscriptionChip>,
    events_per_second: f32,
    uptime_short: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OwnedFileMime {
    Html,
    Css,
    Js,
    Json,
    Image,
    Wasm,
    Other,
}

impl OwnedFileMime {
    fn from_path(path: &std::path::Path) -> Self {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        match ext.as_str() {
            "html" | "htm" => Self::Html,
            "css" => Self::Css,
            "js" | "mjs" => Self::Js,
            "json" => Self::Json,
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" => Self::Image,
            "wasm" => Self::Wasm,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OwnedOverlayKind {
    File { mime: OwnedFileMime },
    Dir,
}

#[derive(Debug, Clone)]
struct OwnedOverlayEntry {
    name: String,
    kind: OwnedOverlayKind,
    size_bytes: u64,
    child_count: u32,
}

#[derive(Debug, Clone, Default)]
struct ServerStats {
    events_per_second: f32,
    events_per_second_avg: f32,
    http_requests: u64,
    bandwidth_kbps: f32,
    bandwidth_peak_kbps: f32,
    total_bytes_sent: u64,
    total_events_out: u64,
}

pub struct ServerConsoleView {
    server: Option<ServerHandle>,
    rt_handle: tokio::runtime::Handle,
    credentials: Arc<dyn CredentialsRepo>,
    bind_address: String,
    bearer_token: String,
    token_revealed: bool,
    server_status: ServerStatus,
    control_in_flight: Option<ServerControl>,
    uptime_seconds: i64,
    connected_clients: Vec<OwnedClientRow>,
    bandwidth_samples: Vec<f32>,
    stats: ServerStats,
    overlay_root: String,
    overlay_entries: Vec<OwnedOverlayEntry>,
    selected_overlay_file: Option<usize>,
    /// Target client's stable `identification`, not its row index (which shifts under a live snapshot refresh).
    pending_disconnect: Option<String>,
}

impl ServerConsoleView {
    pub fn new(
        server: Option<ServerHandle>,
        rt_handle: tokio::runtime::Handle,
        credentials: Arc<dyn CredentialsRepo>,
        cx: &mut Context<Self>,
    ) -> Self {
        let server_status = if server.is_some() {
            ServerStatus::Running
        } else {
            ServerStatus::Stopped
        };
        let view = Self {
            server,
            rt_handle,
            credentials,
            bind_address: "0.0.0.0:8081".to_owned(),
            bearer_token: String::new(),
            token_revealed: false,
            server_status,
            control_in_flight: None,
            uptime_seconds: 0,
            connected_clients: Vec::new(),
            bandwidth_samples: Vec::new(),
            stats: ServerStats::default(),
            overlay_root: String::new(),
            overlay_entries: Vec::new(),
            selected_overlay_file: None,
            pending_disconnect: None,
        };
        view.fetch_token(cx);
        if view.server.is_some() {
            view.start_poll(cx);
        }
        view
    }

    fn fetch_token(&self, cx: &mut Context<Self>) {
        let credentials = Arc::clone(&self.credentials);
        let (tx, rx) = tokio::sync::oneshot::channel::<Option<String>>();
        self.rt_handle.spawn(async move {
            let token = credentials
                .load(&CredentialId::new(BEARER_CREDENTIAL_ID))
                .await
                .ok()
                .flatten();
            let _ = tx.send(token);
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Some(token)) = rx.await {
                let _ = this.update(cx, |this, cx| {
                    this.bearer_token = token;
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn start_poll(&self, cx: &mut Context<Self>) {
        let Some(handle) = self.server.clone() else {
            return;
        };
        let rt_handle = self.rt_handle.clone();
        cx.spawn(async move |this, cx| {
            let mut tick: u32 = 0;
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                tick = tick.wrapping_add(1);
                let want_overlay = tick == 1 || tick.is_multiple_of(5);
                let (tx, rx) = tokio::sync::oneshot::channel::<ServerPoll>();
                let handle = handle.clone();
                rt_handle.spawn(async move {
                    let snapshot = handle.snapshot().await;
                    let bind_address = handle.bind_addr().await.to_string();
                    let overlay = if want_overlay {
                        let root = handle.overlay_root().await;
                        Some(scan_overlay_root(root.as_ref()).await)
                    } else {
                        None
                    };
                    let _ = tx.send(ServerPoll {
                        snapshot,
                        bind_address,
                        overlay,
                    });
                });
                let Ok(poll) = rx.await else {
                    continue;
                };
                if this
                    .update(cx, |this, cx| {
                        this.apply_poll(poll);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn apply_poll(&mut self, poll: ServerPoll) {
        self.bind_address = poll.bind_address;

        if self.server_status == ServerStatus::Running && self.control_in_flight.is_none() {
            let snapshot = &poll.snapshot;
            self.uptime_seconds = snapshot.uptime_seconds;
            self.connected_clients = snapshot
                .connected_clients
                .iter()
                .map(client_row_from_snapshot)
                .collect();

            let kbps = snapshot.bandwidth.outbound_bytes_per_second as f32 / 1000.0;
            let peak_kbps = snapshot.bandwidth.peak_outbound_bytes_per_second as f32 / 1000.0;
            self.stats = ServerStats {
                events_per_second: snapshot.aggregate_events_per_second,
                events_per_second_avg: snapshot.aggregate_events_per_second,
                http_requests: 0,
                bandwidth_kbps: kbps,
                bandwidth_peak_kbps: peak_kbps,
                total_bytes_sent: snapshot.bandwidth.outbound_bytes_total,
                total_events_out: 0,
            };
            self.bandwidth_samples.push(kbps);
            if self.bandwidth_samples.len() > MAX_BANDWIDTH_SAMPLES {
                let excess = self.bandwidth_samples.len() - MAX_BANDWIDTH_SAMPLES;
                self.bandwidth_samples.drain(..excess);
            }
        }

        if let Some(overlay) = poll.overlay {
            self.overlay_root = overlay.root;
            self.overlay_entries = overlay.entries;
            if let Some(idx) = self.selected_overlay_file
                && idx >= self.overlay_entries.len()
            {
                self.selected_overlay_file = None;
            }
        }
    }

    fn toggle_token_reveal(&mut self, cx: &mut Context<Self>) {
        self.token_revealed = !self.token_revealed;
        cx.notify();
    }

    fn copy_bind_address(&mut self, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(self.bind_address.clone()));
    }

    fn copy_token(&mut self, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(self.bearer_token.clone()));
    }

    fn copy_overlay_url(&mut self, url: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(url));
    }

    fn regenerate_token(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self.server.clone() else {
            return;
        };
        let credentials = Arc::clone(&self.credentials);
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<String, String>>();
        self.rt_handle.spawn(async move {
            let auth = handle.auth_state().await;
            let _ = tx.send(
                auth.regenerate(credentials.as_ref())
                    .await
                    .map_err(err_text),
            );
        });
        cx.spawn(async move |this, cx| match rx.await {
            Ok(Ok(token)) => {
                let _ = this.update(cx, |this, cx| {
                    this.bearer_token = token;
                    cx.notify();
                });
            }
            Ok(Err(reason)) => eprintln!("forge-desktop: token regenerate failed: {reason}"),
            Err(_) => {}
        })
        .detach();
    }

    fn restart_server(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self.server.clone() else {
            return;
        };
        if self.control_in_flight.is_some() {
            return;
        }
        self.control_in_flight = Some(ServerControl::Restarting);
        cx.notify();
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<(), String>>();
        self.rt_handle.spawn(async move {
            let _ = tx.send(handle.restart().await.map_err(err_text));
        });
        cx.spawn(async move |this, cx| {
            let outcome = rx.await;
            let _ = this.update(cx, |this, cx| {
                this.control_in_flight = None;
                match outcome {
                    Ok(Ok(())) => this.server_status = ServerStatus::Running,
                    Ok(Err(reason)) => this.server_status = ServerStatus::Error(reason),
                    Err(_) => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn stop_server(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self.server.clone() else {
            return;
        };
        if self.control_in_flight.is_some() {
            return;
        }
        self.control_in_flight = Some(ServerControl::Stopping);
        cx.notify();
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<(), String>>();
        self.rt_handle.spawn(async move {
            let _ = tx.send(handle.stop().await.map_err(err_text));
        });
        cx.spawn(async move |this, cx| {
            let outcome = rx.await;
            let _ = this.update(cx, |this, cx| {
                this.control_in_flight = None;
                match outcome {
                    Ok(Ok(())) => this.server_status = ServerStatus::Stopped,
                    Ok(Err(reason)) => this.server_status = ServerStatus::Error(reason),
                    Err(_) => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn open_overlay_folder(&mut self, _cx: &mut Context<Self>) {
        let Some(handle) = self.server.clone() else {
            return;
        };
        self.rt_handle.spawn(async move {
            let root = handle.overlay_root().await;
            let path = (*root).clone();
            let _ = tokio::task::spawn_blocking(move || {
                if let Err(e) = open::that(&path) {
                    eprintln!("forge-desktop: failed to open overlay folder: {e}");
                }
            })
            .await;
        });
    }

    fn select_overlay_file(&mut self, index: usize, cx: &mut Context<Self>) {
        self.selected_overlay_file = Some(index);
        cx.notify();
    }

    fn request_disconnect(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(row) = self.connected_clients.get(index) {
            self.pending_disconnect = Some(row.identification.clone());
            cx.notify();
        }
    }

    fn cancel_disconnect(&mut self, cx: &mut Context<Self>) {
        self.pending_disconnect = None;
        cx.notify();
    }

    fn confirm_disconnect(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.pending_disconnect.take() {
            self.connected_clients.retain(|c| c.identification != id);
        }
        cx.notify();
    }

    fn header_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let brand = palette.brand;
        let info = palette.info;
        let s_color = status_color(&self.server_status, palette);

        let icon_box = div()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(ICON_BOX)
            .rounded(radius(Radius::Lg))
            .border(BORDER_THIN)
            .border_color(with_alpha(brand, 0.2))
            .bg(with_alpha(brand, 0.1))
            .child(icon(Icon::Server, SERVER_GLYPH, brand));

        let ws_badge = badge(with_alpha(info, 0.12), info, "WS + HTTP", true, FONT_XS);

        let title_row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(tr!("server_header_title")),
            )
            .child(ws_badge)
            .child(div().flex_1())
            .child(status_dot(s_color, STATUS_DOT))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(s_color)
                    .child(status_label(&self.server_status)),
            );

        let description = div()
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FONT_SM)
            .text_color(palette.text_muted)
            .child(tr!("server_header_desc"));

        let actions_row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(div().flex_1())
            .child(self.restart_button(palette, density, cx))
            .child(self.stop_button(palette, density, cx));

        let top_section = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(title_row)
            .child(description)
            .child(actions_row);

        let header_row = div()
            .w_full()
            .flex()
            .items_start()
            .gap(spacing(Spacing::Md, density))
            .child(icon_box)
            .child(top_section);

        let credentials_row = div()
            .w_full()
            .flex()
            .items_start()
            .gap(spacing(Spacing::Lg, density))
            .child(weighted(1.0, self.bind_column(palette, density, cx)))
            .child(weighted(2.0, self.token_column(palette, density, cx)));

        let content = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(header_row)
            .child(hline(palette.border_regular))
            .child(credentials_row);

        card(content, palette)
            .padding(spacing(Spacing::Lg, density))
            .radius(Radius::Lg)
            .full_width()
            .into_any_element()
    }

    fn bind_column(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let uptime_str = if self.uptime_seconds > 0 {
            tr!(
                "server_up_prefix",
                uptime = fmt_uptime(self.uptime_seconds.max(0) as u64)
            )
        } else {
            tr!("server_not_running")
        };

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(caption(tr!("server_bind_address"), palette))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(self.bind_address.clone()),
            )
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(uptime_str),
            )
            .child(self.copy_address_button(palette, density, cx))
            .into_any_element()
    }

    fn token_column(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(caption(tr!("server_bearer_token"), palette))
            .child(self.bearer_token_row(palette, density, cx))
            .child(self.regen_warning(palette, density))
            .into_any_element()
    }

    fn bearer_token_row(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let shown = if self.token_revealed {
            self.bearer_token.clone()
        } else {
            mask_token(&self.bearer_token)
        };
        let reveal_glyph = if self.token_revealed {
            Icon::EyeOff
        } else {
            Icon::Eye
        };

        let field = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.shell)
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(shown),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(
                        div()
                            .id("srv-token-reveal")
                            .flex()
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.toggle_token_reveal(cx)
                            }))
                            .child(icon(reveal_glyph, CONTROL_GLYPH, palette.text_faint)),
                    )
                    .child(
                        div()
                            .id("srv-token-copy")
                            .flex()
                            .cursor_pointer()
                            .on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| this.copy_token(cx)),
                            )
                            .child(icon(Icon::Copy, CONTROL_GLYPH, palette.text_faint)),
                    ),
            );

        let regenerate = div()
            .id("srv-token-regen")
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .cursor_pointer()
            .child(icon(Icon::Refresh, CONTROL_GLYPH, palette.warning))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.warning)
                    .child(tr!("server_btn_regenerate")),
            )
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.regenerate_token(cx)));

        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(field)
            .child(regenerate)
            .into_any_element()
    }

    fn regen_warning(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xxs, density))
                    .child(icon(Icon::AlertTriangle, LINK_GLYPH, palette.warning))
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_XXS)
                            .text_color(palette.warning)
                            .child(tr!("server_regen_warning_title")),
                    ),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(tr!("server_regen_warning_body")),
            )
            .into_any_element()
    }

    fn restart_button(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let busy = self.control_in_flight.is_some();
        let base = palette.success;
        let border = if busy { with_alpha(base, 0.4) } else { base };
        let label = if self.control_in_flight == Some(ServerControl::Restarting) {
            tr!("server_btn_restarting")
        } else {
            tr!("server_btn_restart")
        };
        let hover_bg = with_alpha(base, 0.08);

        let mut btn = div()
            .id("srv-restart")
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(border)
            .text_color(border)
            .child(icon(Icon::Refresh, CONTROL_GLYPH, border))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .child(label),
            );
        if !busy {
            btn = btn
                .cursor_pointer()
                .hover(move |s| s.bg(hover_bg))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.restart_server(cx)));
        }
        btn.into_any_element()
    }

    fn stop_button(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let busy = self.control_in_flight.is_some();
        let base = palette.random;
        let border = if busy { with_alpha(base, 0.4) } else { base };
        let label = if self.control_in_flight == Some(ServerControl::Stopping) {
            tr!("server_btn_stopping")
        } else {
            tr!("server_btn_stop")
        };
        let hover_bg = with_alpha(base, 0.08);

        let mut btn = div()
            .id("srv-stop")
            .flex()
            .items_center()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(border)
            .text_color(border)
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .child(label),
            );
        if !busy {
            btn = btn
                .cursor_pointer()
                .hover(move |s| s.bg(hover_bg))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.stop_server(cx)));
        }
        btn.into_any_element()
    }

    fn copy_address_button(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let normal = palette.text_secondary;
        let hover_bg = with_alpha(palette.text_primary, 0.06);

        div()
            .id("srv-copy-addr")
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .text_color(normal)
            .cursor_pointer()
            .hover(move |s| s.bg(hover_bg))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.copy_bind_address(cx)))
            .child(icon(Icon::Copy, CONTROL_GLYPH, normal))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .child(tr!("server_btn_copy")),
            )
            .into_any_element()
    }

    fn stats_grid(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let success = palette.success;
        let cell = |el: AnyElement| div().flex_1().min_w(px(0.0)).child(el);

        div()
            .w_full()
            .flex()
            .flex_row()
            .gap(spacing(Spacing::Xs, density))
            .child(cell(
                metric_card(
                    tr!("server_stat_clients"),
                    format!("{}", self.connected_clients.len()),
                    Some(tr!("server_stat_clients_sub")),
                    Some(success),
                    palette,
                )
                .into_any_element(),
            ))
            .child(cell(
                metric_card(
                    tr!("server_stat_events_out"),
                    format!("{:.1} ev/s", self.stats.events_per_second),
                    Some(tr!(
                        "server_stat_events_sub",
                        avg = format!("{:.1}", self.stats.events_per_second_avg)
                    )),
                    None,
                    palette,
                )
                .into_any_element(),
            ))
            .child(cell(
                metric_card(
                    tr!("server_stat_http"),
                    format!("{}", self.stats.http_requests),
                    Some(tr!("server_stat_http_sub")),
                    None,
                    palette,
                )
                .into_any_element(),
            ))
            .child(cell(
                metric_card(
                    tr!("server_stat_bandwidth"),
                    format!("{:.0} KB/s", self.stats.bandwidth_kbps),
                    Some(tr!(
                        "server_stat_bandwidth_sub",
                        peak = format!("{:.0}", self.stats.bandwidth_peak_kbps)
                    )),
                    Some(success),
                    palette,
                )
                .into_any_element(),
            ))
            .into_any_element()
    }

    fn throughput_card(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let sample_count = self.bandwidth_samples.len().min(MAX_BANDWIDTH_SAMPLES);
        let peak = self
            .bandwidth_samples
            .iter()
            .copied()
            .fold(0.0_f32, f32::max);

        let header = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(icon(Icon::ChartLine, HEADER_GLYPH, palette.brand))
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_SM)
                            .text_color(palette.text_primary)
                            .child(tr!("server_throughput_title")),
                    ),
            )
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(tr!(
                        "server_throughput_meta",
                        seconds = sample_count as i64,
                        peak = format!("{peak:.0}")
                    )),
            );

        let chart = div()
            .w_full()
            .h(px(48.0))
            .p(px(4.0))
            .rounded(radius(Radius::Sm))
            .bg(palette.shell)
            .child(sparkline(&self.bandwidth_samples, palette.brand));

        card(
            div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Sm, density))
                .child(header)
                .child(chart),
            palette,
        )
        .padding(spacing(Spacing::Md, density))
        .radius(Radius::Lg)
        .full_width()
        .into_any_element()
    }

    fn overlay_panel(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let origin = overlay_origin(&self.bind_address);

        let header = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .child(icon(Icon::Folder, HEADER_GLYPH, palette.warning))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(tr!("server_overlay_files_title")),
            );

        let root_label = if self.overlay_root.is_empty() {
            "-".to_owned()
        } else {
            self.overlay_root.clone()
        };
        let path_row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(root_label),
            )
            .child(
                div()
                    .id("srv-open-folder")
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xxs, density))
                    .py(spacing(Spacing::Xxs, density))
                    .px(spacing(Spacing::Xs, density))
                    .rounded(radius(Radius::Sm))
                    .border(BORDER_THIN)
                    .border_color(palette.border_regular)
                    .text_color(palette.text_secondary)
                    .cursor_pointer()
                    .on_click(
                        cx.listener(|this, _: &ClickEvent, _, cx| this.open_overlay_folder(cx)),
                    )
                    .child(icon(Icon::FolderOpen, CONTROL_GLYPH, palette.text_muted))
                    .child(
                        div()
                            .font_family(DEFAULT_MONO_FAMILY)
                            .text_size(FONT_XS)
                            .child(tr!("server_btn_open")),
                    ),
            );

        let files: AnyElement = if self.overlay_entries.is_empty() {
            empty_state(tr!("server_overlay_files_empty"), palette)
                .density(density)
                .into_any_element()
        } else {
            let mut col = div()
                .w_full()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xxs, density))
                .py(spacing(Spacing::Xxs, density))
                .px(spacing(Spacing::Xs, density));
            for (index, entry) in self.overlay_entries.iter().enumerate() {
                let selected = self.selected_overlay_file == Some(index);
                col = col.child(
                    self.overlay_entry_row(index, entry, selected, &origin, palette, density, cx),
                );
            }
            div()
                .id("srv-overlay-scroll")
                .w_full()
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .child(col)
                .into_any_element()
        };

        let inner = div()
            .size_full()
            .flex()
            .flex_col()
            .child(header)
            .child(path_row)
            .child(hline(palette.border_regular))
            .child(files);

        card(inner, palette)
            .padding(px(0.0))
            .radius(Radius::Lg)
            .full_width()
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn overlay_entry_row(
        &self,
        index: usize,
        entry: &OwnedOverlayEntry,
        selected: bool,
        origin: &str,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let size_label = match entry.kind {
            OwnedOverlayKind::Dir => {
                tr!("server_overlay_dir_items", count = entry.child_count as i64)
            }
            OwnedOverlayKind::File { .. } => fmt_bytes(entry.size_bytes),
        };
        let kind_color = match entry.kind {
            OwnedOverlayKind::File {
                mime: OwnedFileMime::Html,
            } => palette.brand,
            OwnedOverlayKind::Dir => palette.warning,
            _ => palette.text_muted,
        };
        let bg = if selected {
            palette.shell
        } else {
            palette.base
        };

        let kind_badge = div()
            .flex_none()
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Xxs, density))
            .rounded(KIND_BADGE_RADIUS)
            .bg(with_alpha(kind_color, 0.12))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(kind_color)
                    .child(overlay_kind_tag(entry.kind)),
            );

        let name_row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(kind_badge)
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(entry.name.clone()),
            )
            .child(div().flex_1())
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(size_label),
            );

        let mut content = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(name_row);

        if selected {
            let url = format!("{origin}/overlays/{}", entry.name);
            let url_for_copy = url.clone();
            content = content.child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .font_family(DEFAULT_MONO_FAMILY)
                            .text_size(FONT_XS)
                            .text_color(palette.text_muted)
                            .child(url),
                    )
                    .child(
                        div()
                            .id((
                                gpui::ElementId::from("srv-overlay-copy"),
                                entry.name.clone(),
                            ))
                            .flex()
                            .items_center()
                            .gap(spacing(Spacing::Xxs, density))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.copy_overlay_url(url_for_copy.clone(), cx)
                            }))
                            .child(icon(Icon::Copy, LINK_GLYPH, palette.text_secondary))
                            .child(icon(Icon::ExternalLink, LINK_GLYPH, palette.text_secondary)),
                    ),
            );
        }

        div()
            .id((
                gpui::ElementId::from("srv-overlay-entry"),
                entry.name.clone(),
            ))
            .w_full()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Sm))
            .bg(bg)
            .cursor_pointer()
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.select_overlay_file(index, cx)),
            )
            .child(content)
            .into_any_element()
    }

    fn clients_panel(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let count_badge = div()
            .flex_none()
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Xs, density))
            .rounded(COUNT_BADGE_RADIUS)
            .bg(palette.surface_overlay)
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(format!("{}", self.connected_clients.len())),
            );

        let header = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .child(icon(Icon::Users, HEADER_GLYPH, palette.text_faint))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(tr!("server_clients_header")),
            )
            .child(div().flex_1())
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(tr!("server_clients_live")),
            )
            .child(count_badge);

        let columns = vec![
            column("", ColumnWidth::Fixed(DOT_CELL_W)),
            column(tr!("server_col_client"), ColumnWidth::Flex(CLIENT_GROW)),
            column(
                tr!("server_col_subscriptions"),
                ColumnWidth::Flex(SUBS_GROW),
            ),
            column(tr!("server_col_evs"), ColumnWidth::Fixed(EVS_CELL_W)),
            column(tr!("server_col_uptime"), ColumnWidth::Fixed(UPTIME_CELL_W)),
            column("", ColumnWidth::Fixed(X_CELL_W)),
        ];

        let table: AnyElement = if self.connected_clients.is_empty() {
            let header_only = data_table(palette, columns, Vec::new())
                .density(density)
                .header_bg(palette.elevated)
                .separator(palette.elevated)
                .header_padding(
                    spacing(Spacing::Xxs, density),
                    spacing(Spacing::Sm, density),
                )
                .cell_gap(spacing(Spacing::Xxs, density));
            div()
                .w_full()
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .flex_col()
                .child(header_only)
                .child(
                    div()
                        .flex_1()
                        .min_h(px(0.0))
                        .child(empty_state(tr!("server_clients_empty"), palette).density(density)),
                )
                .into_any_element()
        } else {
            let rows: Vec<DataRow> = self
                .connected_clients
                .iter()
                .enumerate()
                .map(|(index, row)| self.client_row(index, row, palette, density, cx))
                .collect();
            data_table(palette, columns, rows)
                .density(density)
                .header_bg(palette.elevated)
                .separator(palette.elevated)
                .header_padding(
                    spacing(Spacing::Xxs, density),
                    spacing(Spacing::Sm, density),
                )
                .row_padding(spacing(Spacing::Xs, density), spacing(Spacing::Sm, density))
                .cell_gap(spacing(Spacing::Xxs, density))
                .scroll_body("srv-clients-scroll")
                .into_any_element()
        };

        let inner = div()
            .size_full()
            .flex()
            .flex_col()
            .child(header)
            .child(div().w_full().h(px(1.0)).bg(palette.border_regular))
            .child(table);

        card(inner, palette)
            .padding(px(0.0))
            .radius(Radius::Lg)
            .full_width()
            .into_any_element()
    }

    fn client_row(
        &self,
        index: usize,
        row: &OwnedClientRow,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> DataRow {
        let dot_color = match row.liveness {
            ClientLiveness::Active => palette.success,
            ClientLiveness::Idle => palette.warning,
        };

        let id_col = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(row.identification.clone()),
            )
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(row.client_type_label.clone()),
            );

        let x_button = div()
            .id((gpui::ElementId::from("srv-disconnect"), row.key.clone()))
            .flex()
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Xxs, density))
            .rounded(radius(Radius::Sm))
            .cursor_pointer()
            .hover(move |s| s.bg(with_alpha(palette.text_secondary, 0.06)))
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.request_disconnect(index, cx)),
            )
            .child(icon(Icon::X, X_GLYPH, palette.text_faint));

        let evs = div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XS)
            .text_color(palette.text_primary)
            .child(format!("{:.1}", row.events_per_second));
        let uptime = div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XS)
            .text_color(palette.text_muted)
            .child(row.uptime_short.clone());

        DataRow::new(vec![
            status_dot(dot_color, CLIENT_DOT).into_any_element(),
            id_col.into_any_element(),
            chips_row(&row.subscriptions, palette, density),
            evs.into_any_element(),
            uptime.into_any_element(),
            x_button.into_any_element(),
        ])
    }

    fn footer_bar(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let port = extract_port(&self.bind_address);
        let s_color = status_color(&self.server_status, palette);

        let left = div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XS)
            .text_color(palette.text_faint)
            .child(format!("WebSocket :{port}/ws · HTTP :{port}/"));

        let right = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(tr!(
                        "server_footer_totals",
                        sent = fmt_bytes(self.stats.total_bytes_sent),
                        events = self.stats.total_events_out as i64
                    )),
            )
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child("·"),
            )
            .child(status_dot(s_color, STATUS_DOT))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(s_color)
                    .child(status_label(&self.server_status)),
            );

        div()
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Md, density))
            .border_t(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.shell)
            .child(left)
            .child(right)
            .into_any_element()
    }

    fn disconnect_confirm(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let id = self.pending_disconnect.as_ref()?;
        let row = self
            .connected_clients
            .iter()
            .find(|c| &c.identification == id)?;

        let message = tr!(
            "server_disconnect_confirm_hint",
            info = row.client_type_label.as_str()
        );
        let confirm = confirm_modal(
            tr!("server_disconnect_confirm_title"),
            message,
            ConfirmTone::Warning,
            palette,
        )
        .item_name(row.identification.clone())
        .esc_hint(tr!("server_disconnect_esc_hint"))
        .on_cancel(
            "srv-disc-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_disconnect(cx)),
        )
        .on_confirm(
            "srv-disc-confirm",
            tr!("server_btn_disconnect"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_disconnect(cx)),
        );

        let view = cx.entity();
        Some(
            overlay(confirm, palette)
                .position(OverlayPosition::Center)
                .on_dismiss("srv-disc-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel_disconnect(cx));
                })
                .into_any_element(),
        )
    }
}

impl Render for ServerConsoleView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let page_header = breadcrumb(
            vec![
                BreadcrumbCrumb::leaf(tr!("server_breadcrumb_builtin")),
                BreadcrumbCrumb::leaf(tr!("server_breadcrumb_server")),
            ],
            &palette,
        );

        let panels = div()
            .w_full()
            .flex()
            .flex_row()
            .items_start()
            .gap(spacing(Spacing::Sm, density))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(self.overlay_panel(&palette, density, cx)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(self.clients_panel(&palette, density, cx)),
            );

        let body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Md, density))
            .px(spacing(Spacing::Lg, density))
            .child(self.header_card(&palette, density, cx))
            .child(self.stats_grid(&palette, density))
            .child(self.throughput_card(&palette, density))
            .child(panels);

        let scroll = div()
            .id("srv-scroll")
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .child(body);

        let overlay = self.disconnect_confirm(&palette, cx);

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(page_header)
            .child(scroll)
            .child(self.footer_bar(&palette, density))
            .children(overlay)
    }
}

fn weighted(grow: f32, child: impl IntoElement) -> Div {
    let mut cell = div().min_w(px(0.0)).child(child);
    let style = cell.style();
    style.flex_grow = Some(grow);
    style.flex_basis = Some(relative(0.0).into());
    cell
}

fn hline(color: Rgba) -> Div {
    div().w_full().h(px(1.0)).bg(color)
}

fn caption(label: impl Into<SharedString>, palette: &ForgePalette) -> impl IntoElement {
    div()
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_XS)
        .text_color(palette.text_muted)
        .child(label.into())
}

fn chips_row(
    chips: &[OwnedSubscriptionChip],
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    if chips.is_empty() {
        return div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XS)
            .text_color(palette.text_faint)
            .child("-")
            .into_any_element();
    }

    let visible = chips.len().min(MAX_VISIBLE_CHIPS);
    let overflow = chips.len().saturating_sub(MAX_VISIBLE_CHIPS);
    let bg = palette.surface_overlay;

    let mut row = div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xxs, density));
    for chip in &chips[..visible] {
        row = row.child(chip_pill(
            chip.label.clone(),
            color_for_source(chip.source, palette),
            bg,
        ));
    }
    if overflow > 0 {
        row = row.child(chip_pill(
            format!("+{overflow} more"),
            palette.text_faint,
            bg,
        ));
    }
    row.into_any_element()
}

fn chip_pill(label: String, fg: Rgba, bg: Rgba) -> impl IntoElement {
    badge(bg, fg, label, true, FONT_XS)
        .weight(FontWeight::NORMAL)
        .padding_xy(
            spacing(Spacing::Xxs, Density::Cozy),
            spacing(Spacing::Xxs, Density::Cozy),
        )
        .radius(radius(Radius::Md))
        .flex_none()
}

fn status_color(status: &ServerStatus, palette: &ForgePalette) -> Rgba {
    match status {
        ServerStatus::Running => palette.success,
        ServerStatus::Stopped => palette.text_faint,
        ServerStatus::Error(_) => palette.random,
    }
}

fn status_label(status: &ServerStatus) -> String {
    match status {
        ServerStatus::Running => tr!("server_status_running"),
        ServerStatus::Stopped => tr!("server_status_stopped"),
        ServerStatus::Error(_) => tr!("server_status_error"),
    }
}

fn extract_port(bind_address: &str) -> &str {
    bind_address.split(':').next_back().unwrap_or("8081")
}

fn overlay_origin(bind_address: &str) -> String {
    let port = extract_port(bind_address);
    let host = bind_address
        .rsplit_once(':')
        .map(|(host, _)| host)
        .unwrap_or(bind_address);
    let host = match host {
        "0.0.0.0" | "::" | "[::]" => "127.0.0.1",
        other => other,
    };
    format!("http://{host}:{port}")
}

fn overlay_kind_tag(kind: OwnedOverlayKind) -> &'static str {
    match kind {
        OwnedOverlayKind::Dir => "dir",
        OwnedOverlayKind::File { mime } => match mime {
            OwnedFileMime::Html => "html",
            OwnedFileMime::Css => "css",
            OwnedFileMime::Js => "js",
            OwnedFileMime::Json => "json",
            OwnedFileMime::Image => "img",
            OwnedFileMime::Wasm => "wasm",
            OwnedFileMime::Other => "file",
        },
    }
}

fn color_for_source(source: EventSource, palette: &ForgePalette) -> Rgba {
    match source {
        EventSource::Twitch => platform_color(PlatformKind::Twitch, palette),
        EventSource::YouTube => platform_color(PlatformKind::YouTube, palette),
        EventSource::Kick => platform_color(PlatformKind::Kick, palette),
        EventSource::Core => palette.warning,
        EventSource::Rhai => palette.warning,
        EventSource::Http => palette.random,
        EventSource::Obs => palette.success,
        EventSource::VTube => palette.accent_teal,
        EventSource::Discord => palette.brand,
        EventSource::Midi => palette.bits,
        EventSource::Hotkey => palette.bits,
        EventSource::Timer => palette.warning,
        EventSource::Server => palette.info,
        EventSource::Audio => palette.bits,
    }
}

fn mask_token(token: &str) -> String {
    let tail: String = token
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("fg_•••••{tail}")
}

struct ServerPoll {
    snapshot: ServerSnapshot,
    bind_address: String,
    overlay: Option<OverlayListing>,
}

struct OverlayListing {
    root: String,
    entries: Vec<OwnedOverlayEntry>,
}

async fn scan_overlay_root(root: &std::path::Path) -> OverlayListing {
    let root_str = root.to_string_lossy().into_owned();
    let mut read_dir = match tokio::fs::read_dir(root).await {
        Ok(read_dir) => read_dir,
        Err(_) => {
            return OverlayListing {
                root: root_str,
                entries: Vec::new(),
            };
        }
    };

    let mut entries: Vec<OwnedOverlayEntry> = Vec::new();
    while let Ok(Some(entry)) = read_dir.next_entry().await {
        let name = entry.file_name().to_string_lossy().into_owned();
        let meta = match entry.metadata().await {
            Ok(meta) => meta,
            Err(_) => continue,
        };
        if meta.is_dir() {
            let mut child_count: u32 = 0;
            if let Ok(mut child) = tokio::fs::read_dir(entry.path()).await {
                while let Ok(Some(_)) = child.next_entry().await {
                    child_count = child_count.saturating_add(1);
                }
            }
            entries.push(OwnedOverlayEntry {
                name,
                kind: OwnedOverlayKind::Dir,
                size_bytes: 0,
                child_count,
            });
        } else {
            let mime = OwnedFileMime::from_path(&entry.path());
            entries.push(OwnedOverlayEntry {
                name,
                kind: OwnedOverlayKind::File { mime },
                size_bytes: meta.len(),
                child_count: 0,
            });
        }
    }

    entries.sort_by(|a, b| {
        let dir_a = matches!(a.kind, OwnedOverlayKind::Dir);
        let dir_b = matches!(b.kind, OwnedOverlayKind::Dir);
        match (dir_a, dir_b) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a
                .name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase()),
        }
    });

    OverlayListing {
        root: root_str,
        entries,
    }
}

fn client_row_from_snapshot(client: &ConnectedClientSnapshot) -> OwnedClientRow {
    let liveness = if client.events_per_second > 0.0 {
        ClientLiveness::Active
    } else {
        ClientLiveness::Idle
    };
    OwnedClientRow {
        key: client.remote_addr.clone(),
        identification: client.identification.clone(),
        client_type_label: format!("{} · {}", client.remote_addr, client.client_type),
        liveness,
        subscriptions: client.subscriptions.iter().map(subscription_chip).collect(),
        events_per_second: client.events_per_second,
        uptime_short: fmt_uptime_short(client.uptime_seconds.max(0) as u64),
    }
}

fn subscription_chip(filter: &EventFilterSnapshot) -> OwnedSubscriptionChip {
    let source_wildcard = filter.source == "*";
    let kind_wildcard = filter.kind == "*";
    let source = if source_wildcard {
        EventSource::Core
    } else {
        serde_json::from_value::<EventSource>(serde_json::Value::String(filter.source.clone()))
            .unwrap_or(EventSource::Core)
    };
    let label = match (source_wildcard, kind_wildcard) {
        (true, true) => "*".to_owned(),
        (true, false) => filter.kind.clone(),
        (false, true) => format!("{}.*", event_source_label(source)),
        (false, false) => format!("{}.{}", event_source_label(source), filter.kind),
    };
    OwnedSubscriptionChip { label, source }
}

fn event_source_label(source: EventSource) -> &'static str {
    match source {
        EventSource::Twitch => "twitch",
        EventSource::YouTube => "youtube",
        EventSource::Kick => "kick",
        EventSource::Core => "core",
        EventSource::Rhai => "rhai",
        EventSource::Http => "http",
        EventSource::Obs => "obs",
        EventSource::VTube => "vtube",
        EventSource::Discord => "discord",
        EventSource::Midi => "midi",
        EventSource::Hotkey => "hotkey",
        EventSource::Timer => "timer",
        EventSource::Server => "server",
        EventSource::Audio => "audio",
    }
}

fn err_text(err: forge_server::ServerError) -> String {
    err.to_string()
}
