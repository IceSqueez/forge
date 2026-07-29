use forge_components::breadcrumb::BreadcrumbCrumb;
use forge_components::{
    BORDER_THIN, ColumnWidth, Confirm, ConfirmTone, DataRow, Density, FONT_SM, FONT_XS, FONT_XXS,
    ForgePalette, Icon, OverlayPosition, PlatformKind, Radius, Spacing, body_family, card, column,
    confirm_modal, data_table, empty_state, fmt_bytes, fmt_number, fmt_uptime_short, header_status,
    icon, metric_card, mono_family, overlay, page_frame, platform_color, radius, spacing,
    sparkline, status_dot, tooltip_builder, tr, virtual_table,
};
use std::sync::Arc;
use std::time::Duration;

use forge_events::EventSource;
use forge_server::{ConnectedClientSnapshot, EventFilterSnapshot, ServerHandle, ServerSnapshot};
use forge_storage::{CredentialId, CredentialsRepo};
use gpui::{
    AnyElement, ClickEvent, Context, Div, ElementId, FontWeight, Pixels, Rgba, SharedString,
    UniformListScrollHandle, Window, div, prelude::*, px,
};

use crate::async_bridge::{self, ErrorSink};
use crate::overlay_url::{extract_port, overlay_origin};
use crate::presentation::ActivePresentation;

const BEARER_CREDENTIAL_ID: &str = "server:bearer";

const MAX_THROUGHPUT_SAMPLES: usize = 60;
const MAX_VISIBLE_CHIPS: usize = 6;
/// Matches the rolling window `forge-server` measures per-client event rate over, so the stat hint stays honest.
const EVENT_RATE_WINDOW_SECONDS: i64 = 10;
const RECENT_CLIENT_WINDOW_SECONDS: i64 = 600;
const RECENT_CLIENT_WINDOW_MINUTES: i64 = RECENT_CLIENT_WINDOW_SECONDS / 60;

const CLIENT_DOT: Pixels = px(6.0);
const FOOTER_DOT: Pixels = px(6.0);
const DOT_CELL_W: Pixels = px(12.0);
const EVS_CELL_W: Pixels = px(60.0);
const UPTIME_CELL_W: Pixels = px(70.0);
const X_CELL_W: Pixels = px(20.0);
const SUB_CHIP: Pixels = px(8.0);
const SUB_CHIP_RADIUS: Pixels = px(2.0);
const X_GLYPH: Pixels = px(12.0);
const CONTROL_GLYPH: Pixels = px(12.0);
const LINK_GLYPH: Pixels = px(11.0);
const HEADER_GLYPH: Pixels = px(14.0);
const FILE_GLYPH: Pixels = px(12.0);
const SPARK_HEIGHT: Pixels = px(60.0);
const CLIENT_GROW: f32 = 1.4;
const SUBS_GROW: f32 = 1.6;

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

#[derive(Debug, Clone)]
struct OwnedClientRow {
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
    Dir,
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
}

#[derive(Debug, Clone, Default)]
struct ServerStats {
    events_per_second: f32,
    http_requests: u64,
    bandwidth_kbps: f32,
    total_bytes_sent: u64,
    total_events_out: u64,
    dropped_events: u64,
}

pub struct ServerConsoleView {
    server: Option<ServerHandle>,
    rt_handle: tokio::runtime::Handle,
    credentials: Arc<dyn CredentialsRepo>,
    running: bool,
    bind_address: Option<String>,
    bearer_token: String,
    token_revealed: bool,
    connected_clients: Vec<OwnedClientRow>,
    recent_clients: usize,
    clients_scroll: UniformListScrollHandle,
    throughput_samples: Vec<f32>,
    stats: ServerStats,
    overlay_root: String,
    overlay_entries: Vec<OwnedOverlayEntry>,
    selected_overlay_file: Option<usize>,
    /// Target client's stable `identification`, not its row index (which shifts under a live snapshot refresh).
    pending_disconnect: Confirm<String>,
}

impl ServerConsoleView {
    pub fn new(
        server: Option<ServerHandle>,
        rt_handle: tokio::runtime::Handle,
        credentials: Arc<dyn CredentialsRepo>,
        cx: &mut Context<Self>,
    ) -> Self {
        let running = server
            .as_ref()
            .is_some_and(|handle| *handle.run_state().borrow());
        let view = Self {
            server,
            rt_handle,
            credentials,
            running,
            bind_address: None,
            bearer_token: String::new(),
            token_revealed: false,
            connected_clients: Vec::new(),
            recent_clients: 0,
            clients_scroll: UniformListScrollHandle::new(),
            throughput_samples: Vec::new(),
            stats: ServerStats::default(),
            overlay_root: String::new(),
            overlay_entries: Vec::new(),
            selected_overlay_file: None,
            pending_disconnect: Confirm::default(),
        };
        view.fetch_token(cx);
        if view.server.is_some() {
            view.start_run_state_bridge(cx);
            view.start_poll(cx);
        }
        view
    }

    fn is_running(&self) -> bool {
        self.running
    }

    fn start_run_state_bridge(&self, cx: &mut Context<Self>) {
        let Some(handle) = self.server.as_ref() else {
            return;
        };
        let mut run_state = handle.run_state();
        cx.spawn(async move |this, cx| {
            loop {
                let running = *run_state.borrow_and_update();
                if this
                    .update(cx, |this, cx| this.apply_run_state(running, cx))
                    .is_err()
                    || run_state.changed().await.is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn apply_run_state(&mut self, running: bool, cx: &mut Context<Self>) {
        if self.running == running {
            return;
        }
        self.running = running;
        if !running {
            self.connected_clients.clear();
            self.recent_clients = 0;
            self.pending_disconnect.cancel();
            self.throughput_samples.clear();
        }
        cx.notify();
    }

    fn fetch_token(&self, cx: &mut Context<Self>) {
        let credentials = Arc::clone(&self.credentials);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                credentials
                    .load(&CredentialId::new(BEARER_CREDENTIAL_ID))
                    .await
                    .ok()
                    .flatten()
            },
            |this, result: Option<String>, cx| {
                if let Some(token) = result {
                    this.bearer_token = token;
                    cx.notify();
                }
            },
            cx,
        );
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
        self.bind_address = Some(poll.bind_address);

        if self.running {
            let snapshot = &poll.snapshot;
            self.connected_clients = snapshot
                .connected_clients
                .iter()
                .map(client_row_from_snapshot)
                .collect();
            self.recent_clients = count_recent_clients(&snapshot.connected_clients);

            self.stats = ServerStats {
                events_per_second: snapshot.aggregate_events_per_second,
                http_requests: snapshot.http_requests_total,
                bandwidth_kbps: snapshot.bandwidth.outbound_bytes_per_second as f32 / 1000.0,
                total_bytes_sent: snapshot.bandwidth.outbound_bytes_total,
                total_events_out: snapshot.events_out_total,
                dropped_events: snapshot.dropped_events_total,
            };
            self.throughput_samples
                .push(snapshot.aggregate_events_per_second);
            if self.throughput_samples.len() > MAX_THROUGHPUT_SAMPLES {
                let excess = self.throughput_samples.len() - MAX_THROUGHPUT_SAMPLES;
                self.throughput_samples.drain(..excess);
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

    /// Falls back to the first hosted `.html` so the browser-source box is populated before the user picks a file.
    fn browser_source_entry(&self) -> Option<&OwnedOverlayEntry> {
        self.selected_overlay_file
            .and_then(|index| self.overlay_entries.get(index))
            .filter(|entry| entry.is_file())
            .or_else(|| self.overlay_entries.iter().find(|e| e.is_html()))
    }

    fn browser_source_url(&self) -> Option<String> {
        if !self.running {
            return None;
        }
        let origin = overlay_origin(self.bind_address.as_deref()?);
        let entry = self.browser_source_entry()?;
        Some(format!("{origin}/overlays/{}", entry.name))
    }

    fn toggle_token_reveal(&mut self, cx: &mut Context<Self>) {
        self.token_revealed = !self.token_revealed;
        cx.notify();
    }

    fn copy_bind_address(&mut self, cx: &mut Context<Self>) {
        if let Some(address) = self.bind_address.clone() {
            crate::toasts::copy_to_clipboard(address, cx);
        }
    }

    fn copy_token(&mut self, cx: &mut Context<Self>) {
        crate::toasts::copy_to_clipboard(self.bearer_token.clone(), cx);
    }

    fn copy_browser_source_url(&mut self, cx: &mut Context<Self>) {
        if let Some(url) = self.browser_source_url() {
            crate::toasts::copy_to_clipboard(url, cx);
        }
    }

    fn regenerate_token(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self.server.clone() else {
            return;
        };
        let credentials = Arc::clone(&self.credentials);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let auth = handle.auth_state().await;
                auth.regenerate(credentials.as_ref())
                    .await
                    .map_err(|e| e.to_string())
            },
            |this, result: Result<String, String>, cx| match result {
                Ok(token) => {
                    this.bearer_token = token;
                    cx.notify();
                }
                Err(reason) => {
                    tracing::warn!(error = %reason, "failed to regenerate the server bearer token");
                }
            },
            cx,
        );
    }

    fn open_overlay_folder(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self.server.clone() else {
            return;
        };
        async_bridge::report_failure(
            &self.rt_handle,
            async move {
                let root = handle.overlay_root().await;
                async_bridge::open_path((*root).clone()).await
            },
            ErrorSink::Toast,
            tr!("server_open_overlay_folder_failed"),
            cx,
        );
    }

    fn select_overlay_file(&mut self, index: usize, cx: &mut Context<Self>) {
        self.selected_overlay_file = Some(index);
        cx.notify();
    }

    fn request_disconnect(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(row) = self.connected_clients.get(index) {
            self.pending_disconnect.request(row.identification.clone());
            cx.notify();
        }
    }

    fn cancel_disconnect(&mut self, cx: &mut Context<Self>) {
        self.pending_disconnect.cancel();
        cx.notify();
    }

    fn confirm_disconnect(&mut self, cx: &mut Context<Self>) {
        let target = self.pending_disconnect.take();
        cx.notify();
        let (Some(identification), Some(handle)) = (target, self.server.clone()) else {
            return;
        };
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                handle.kick_client(&identification).await;
                identification
            },
            |this, identification: String, cx| {
                this.connected_clients
                    .retain(|c| c.identification != identification);
                cx.notify();
            },
            cx,
        );
    }

    fn credentials_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let row = div()
            .w_full()
            .flex()
            .items_start()
            .gap(spacing(Spacing::Md, density))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(self.bind_column(palette, density, cx)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(self.token_column(palette, density, cx)),
            );

        card(row, palette)
            .padding_xy(spacing(Spacing::Sm, density), spacing(Spacing::Md, density))
            .radius(Radius::Md)
            .full_width()
            .into_any_element()
    }

    fn bind_column(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let address = if self.running {
            self.bind_address.clone()
        } else {
            None
        };

        let field = match address {
            Some(address) => mono_field(palette, density)
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .truncate()
                        .text_color(palette.text_primary)
                        .child(address),
                )
                .child(
                    div()
                        .id("srv-copy-addr")
                        .flex()
                        .flex_none()
                        .cursor_pointer()
                        .tooltip(tooltip_builder(tr!("common_copy"), palette))
                        .on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| this.copy_bind_address(cx)),
                        )
                        .child(icon(Icon::Copy, CONTROL_GLYPH, palette.text_faint)),
                ),
            None => mono_field(palette, density).child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .text_color(palette.text_faint)
                    .child(tr!("server_not_running")),
            ),
        };

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(caption(tr!("server_bind_address"), palette))
            .child(field)
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

        let field = mono_field(palette, density)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_color(palette.text_primary)
                    .child(shown),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
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
                            .tooltip(tooltip_builder(tr!("common_copy"), palette))
                            .on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| this.copy_token(cx)),
                            )
                            .child(icon(Icon::Copy, CONTROL_GLYPH, palette.text_faint)),
                    ),
            );

        let regenerate = div()
            .id("srv-token-regen")
            .flex()
            .flex_none()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .cursor_pointer()
            .hover(move |s| s.border_color(palette.warning))
            .child(icon(Icon::Refresh, CONTROL_GLYPH, palette.warning))
            .child(
                div()
                    .font_family(body_family())
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
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(icon(Icon::AlertTriangle, LINK_GLYPH, palette.warning))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.warning)
                    .child(tr!("server_regen_warning_title")),
            )
            .into_any_element()
    }

    fn stats_grid(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let cell = |el: AnyElement| div().flex_1().min_w(px(0.0)).child(el);
        let recent_color = (self.recent_clients > 0).then_some(palette.success);

        div()
            .w_full()
            .flex()
            .flex_row()
            .gap(spacing(Spacing::Sm, density))
            .child(cell(
                metric_card(
                    tr!("server_stat_clients"),
                    fmt_number(self.connected_clients.len() as f64, 0),
                    Some(tr!(
                        "server_stat_clients_sub",
                        count = self.recent_clients as i64,
                        minutes = RECENT_CLIENT_WINDOW_MINUTES
                    )),
                    recent_color,
                    palette,
                )
                .into_any_element(),
            ))
            .child(cell(
                metric_card(
                    tr!("server_stat_events_rate"),
                    fmt_number(self.stats.events_per_second as f64, 1),
                    Some(tr!(
                        "server_stat_events_sub",
                        seconds = EVENT_RATE_WINDOW_SECONDS
                    )),
                    None,
                    palette,
                )
                .into_any_element(),
            ))
            .child(cell(
                metric_card(
                    tr!("server_stat_http"),
                    fmt_number(self.stats.http_requests as f64, 0),
                    Some(tr!("server_stat_http_sub")),
                    None,
                    palette,
                )
                .into_any_element(),
            ))
            .child(cell(
                metric_card(
                    tr!("server_stat_bandwidth"),
                    tr!(
                        "server_stat_bandwidth_value",
                        rate = fmt_number(self.stats.bandwidth_kbps as f64, 0)
                    ),
                    Some(tr!("server_stat_bandwidth_sub")),
                    None,
                    palette,
                )
                .into_any_element(),
            ))
            .into_any_element()
    }

    fn throughput_card(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let sample_count = self.throughput_samples.len();
        let max = self
            .throughput_samples
            .iter()
            .copied()
            .filter(|s| s.is_finite())
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
                            .font_family(body_family())
                            .font_weight(FontWeight::MEDIUM)
                            .text_size(FONT_SM)
                            .text_color(palette.text_primary)
                            .child(tr!("server_throughput_title")),
                    ),
            )
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(tr!(
                        "server_throughput_meta",
                        seconds = sample_count as i64,
                        max = fmt_number(max as f64, 0)
                    )),
            );

        let chart = div()
            .w_full()
            .h(SPARK_HEIGHT)
            .child(sparkline(&self.throughput_samples, palette.brand));

        card(
            div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density))
                .child(header)
                .child(chart),
            palette,
        )
        .padding(spacing(Spacing::Sm, density))
        .radius(Radius::Md)
        .full_width()
        .into_any_element()
    }

    fn overlay_panel(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let header = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Sm, density))
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(icon(Icon::Folder, HEADER_GLYPH, palette.warning))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(tr!("server_overlay_host_title")),
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
            .child(
                mono_field(palette, density).child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .truncate()
                        .text_size(FONT_XXS)
                        .text_color(palette.text_primary)
                        .child(root_label),
                ),
            )
            .child(
                div()
                    .id("srv-open-folder")
                    .flex()
                    .flex_none()
                    .items_center()
                    .py(spacing(Spacing::Xs, density))
                    .px(spacing(Spacing::Xs, density))
                    .rounded(radius(Radius::Sm))
                    .border(BORDER_THIN)
                    .border_color(palette.border_regular)
                    .cursor_pointer()
                    .hover(move |s| s.border_color(palette.border_input))
                    .tooltip(tooltip_builder(tr!("server_open_overlay_folder"), palette))
                    .on_click(
                        cx.listener(|this, _: &ClickEvent, _, cx| this.open_overlay_folder(cx)),
                    )
                    .child(icon(Icon::FolderOpen, CONTROL_GLYPH, palette.text_muted)),
            );

        let files: AnyElement = if self.overlay_entries.is_empty() {
            empty_state(tr!("server_overlay_files_empty"), palette)
                .density(density)
                .into_any_element()
        } else {
            let mut col = div().w_full().flex().flex_col();
            for (index, entry) in self.overlay_entries.iter().enumerate() {
                col = col.child(self.overlay_entry_row(index, entry, palette, density, cx));
            }
            col.into_any_element()
        };

        let body = div()
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .p(spacing(Spacing::Sm, density))
            .child(path_row)
            .child(files)
            .child(self.browser_source_block(palette, density, cx));

        let inner = div()
            .size_full()
            .flex()
            .flex_col()
            .child(header)
            .child(body);

        card(inner, palette)
            .padding(px(0.0))
            .radius(Radius::Md)
            .full_width()
            .into_any_element()
    }

    fn overlay_entry_row(
        &self,
        index: usize,
        entry: &OwnedOverlayEntry,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (glyph, glyph_color, size_label) = match entry.kind {
            OwnedOverlayKind::Dir => (
                Icon::Folder,
                palette.warning,
                tr!("server_overlay_dir_files", count = entry.child_count as i64),
            ),
            OwnedOverlayKind::File { .. } => {
                (Icon::FileCode, palette.info, fmt_bytes(entry.size_bytes))
            }
        };
        let selected = self
            .browser_source_entry()
            .is_some_and(|current| current.name == entry.name);
        let name_color = if selected {
            palette.text_primary
        } else {
            palette.text_secondary
        };

        let mut row = div()
            .id((ElementId::from("srv-overlay-entry"), entry.name.clone()))
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Xxs, density))
            .child(icon(glyph, FILE_GLYPH, glyph_color))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(name_color)
                    .child(entry.name.clone()),
            )
            .child(
                div()
                    .flex_none()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(size_label),
            );

        if entry.is_file() {
            row =
                row.cursor_pointer()
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.select_overlay_file(index, cx)
                    }));
        }

        row.into_any_element()
    }

    fn browser_source_block(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let url = self.browser_source_url();

        let value = match url.clone() {
            Some(url) => div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .text_color(palette.info)
                .child(url),
            None => div()
                .flex_1()
                .min_w(px(0.0))
                .text_color(palette.text_faint)
                .child("-"),
        };

        let mut field = mono_field(palette, density)
            .text_size(FONT_XXS)
            .child(value);
        if url.is_some() {
            field = field.child(
                div()
                    .id("srv-overlay-url-copy")
                    .flex()
                    .flex_none()
                    .cursor_pointer()
                    .tooltip(tooltip_builder(tr!("common_copy"), palette))
                    .on_click(
                        cx.listener(|this, _: &ClickEvent, _, cx| this.copy_browser_source_url(cx)),
                    )
                    .child(icon(Icon::Copy, LINK_GLYPH, palette.brand)),
            );
        }

        div()
            .w_full()
            .flex_none()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .pt(spacing(Spacing::Sm, density))
            .border_t(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(caption(tr!("server_overlay_browser_source_url"), palette))
            .child(field)
            .into_any_element()
    }

    fn clients_panel(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let header = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Sm, density))
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(icon(Icon::Browser, HEADER_GLYPH, palette.info))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(tr!("server_clients_header")),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_faint)
                    .child(fmt_number(self.connected_clients.len() as f64, 0)),
            )
            .child(div().flex_1())
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(tr!("server_clients_live")),
            );

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
                .header_rule(palette.border_regular)
                .header_padding(spacing(Spacing::Xs, density), spacing(Spacing::Sm, density))
                .cell_gap(spacing(Spacing::Xs, density));
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
            let pal = *palette;
            virtual_table(
                "srv-clients-scroll",
                palette,
                columns,
                self.connected_clients.len(),
                &self.clients_scroll,
                density,
            )
            .header_bg(palette.elevated)
            .separator(palette.elevated)
            .header_rule(palette.border_regular)
            .header_padding(spacing(Spacing::Xs, density), spacing(Spacing::Sm, density))
            .row_padding(spacing(Spacing::Xs, density), spacing(Spacing::Sm, density))
            .cell_gap(spacing(Spacing::Xs, density))
            .build(
                move |this, ix, _window, cx| match this.connected_clients.get(ix) {
                    Some(row) => this.client_row(ix, row, &pal, density, cx),
                    None => DataRow::new(Vec::new()),
                },
                cx,
            )
        };

        let inner = div()
            .size_full()
            .flex()
            .flex_col()
            .child(header)
            .child(table);

        card(inner, palette)
            .padding(px(0.0))
            .radius(Radius::Md)
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

        let name = div()
            .truncate()
            .font_family(mono_family())
            .text_size(FONT_XS)
            .text_color(palette.text_primary)
            .child(row.identification.clone());

        let x_button = div()
            .id((ElementId::from("srv-disconnect"), row.key.clone()))
            .flex()
            .cursor_pointer()
            .tooltip(tooltip_builder(tr!("server_disconnect_tooltip"), palette))
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.request_disconnect(index, cx)),
            )
            .child(icon(Icon::X, X_GLYPH, palette.text_faint));

        let evs = div()
            .font_family(mono_family())
            .text_size(FONT_XS)
            .text_color(palette.text_primary)
            .child(fmt_number(row.events_per_second as f64, 1));
        let uptime = div()
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_muted)
            .child(row.uptime_short.clone());

        DataRow::new(vec![
            status_dot(dot_color, CLIENT_DOT).into_any_element(),
            name.into_any_element(),
            subscriptions_cell(row, palette, density),
            evs.into_any_element(),
            uptime.into_any_element(),
            x_button.into_any_element(),
        ])
    }

    fn footer_bar(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let (health_label, health_color) = if self.stats.dropped_events == 0 {
            (tr!("server_footer_health_ok"), palette.text_faint)
        } else {
            (
                tr!(
                    "server_footer_health_degraded",
                    dropped = self.stats.dropped_events as i64
                ),
                palette.warning,
            )
        };

        let left = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(footer_cell(
                tr!(
                    "server_footer_totals",
                    sent = fmt_bytes(self.stats.total_bytes_sent),
                    events = self.stats.total_events_out as i64
                ),
                palette.text_faint,
            ))
            .child(footer_cell("·", palette.text_faint))
            .child(footer_cell(health_label, health_color));

        let (endpoint_label, dot_color) = match self.bind_address.as_deref() {
            Some(address) if self.is_running() => (
                tr!(
                    "server_footer_endpoint_accepting",
                    port = extract_port(address)
                ),
                palette.success,
            ),
            _ => (tr!("server_footer_endpoint_stopped"), palette.text_faint),
        };

        let right = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(status_dot(dot_color, FOOTER_DOT))
            .child(footer_cell(endpoint_label, palette.text_faint));

        div()
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .border_t(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.shell)
            .child(left)
            .child(right)
            .into_any_element()
    }

    fn breadcrumb_status(&self, palette: &ForgePalette) -> AnyElement {
        if self.is_running() {
            header_status(
                palette.success,
                tr!(
                    "server_status_listening",
                    clients = self.connected_clients.len() as i64
                ),
            )
            .into_any_element()
        } else {
            header_status(palette.text_faint, tr!("server_status_stopped")).into_any_element()
        }
    }

    fn disconnect_confirm(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let id = self.pending_disconnect.get()?;
        let row = self
            .connected_clients
            .iter()
            .find(|c| &c.identification == id)?;

        let message = tr!(
            "server_disconnect_confirm_hint",
            info = row.origin_label.as_str()
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
            .child(self.credentials_card(&palette, density, cx))
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

        let modal = self.disconnect_confirm(&palette, cx);

        let frame = page_frame(
            vec![
                BreadcrumbCrumb::leaf(tr!("server_breadcrumb_builtin")),
                BreadcrumbCrumb::leaf(tr!("server_breadcrumb_server")),
            ],
            &palette,
        )
        .header_right(self.breadcrumb_status(&palette))
        .body(
            div()
                .flex_1()
                .min_h(px(0.0))
                .w_full()
                .flex()
                .flex_col()
                .child(scroll)
                .child(self.footer_bar(&palette, density)),
        );

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(frame)
            .children(modal)
    }
}

fn mono_field(palette: &ForgePalette, density: Density) -> Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap(spacing(Spacing::Xs, density))
        .py(spacing(Spacing::Xs, density))
        .px(spacing(Spacing::Sm, density))
        .rounded(radius(Radius::Sm))
        .border(BORDER_THIN)
        .border_color(palette.border_input)
        .bg(palette.shell)
        .font_family(mono_family())
        .text_size(FONT_SM)
}

fn footer_cell(text: impl Into<SharedString>, color: Rgba) -> impl IntoElement {
    div()
        .font_family(mono_family())
        .text_size(FONT_XXS)
        .text_color(color)
        .child(text.into())
}

fn caption(label: impl Into<SharedString>, palette: &ForgePalette) -> impl IntoElement {
    div()
        .font_family(mono_family())
        .text_size(FONT_XXS)
        .text_color(palette.text_muted)
        .child(label.into())
}

fn subscriptions_cell(
    row: &OwnedClientRow,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let mut cell = div()
        .flex()
        .items_center()
        .min_w(px(0.0))
        .gap(spacing(Spacing::Xxs, density))
        .child(
            div()
                .min_w(px(0.0))
                .truncate()
                .mr(spacing(Spacing::Xxs, density))
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_muted)
                .child(row.origin_label.clone()),
        );

    if row.subscriptions.is_empty() {
        return cell
            .child(
                div()
                    .flex_none()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child("-"),
            )
            .into_any_element();
    }

    let visible = row.subscriptions.len().min(MAX_VISIBLE_CHIPS);
    for chip in &row.subscriptions[..visible] {
        cell = cell.child(subscription_chip_element(&row.key, chip, palette));
    }

    let overflow = row.subscriptions.len().saturating_sub(MAX_VISIBLE_CHIPS);
    if overflow > 0 {
        cell = cell.child(
            div()
                .flex_none()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(format!("+{overflow}")),
        );
    }

    cell.into_any_element()
}

fn subscription_chip_element(
    row_key: &str,
    chip: &OwnedSubscriptionChip,
    palette: &ForgePalette,
) -> AnyElement {
    if chip.kind == SubscriptionChipKind::All {
        return div()
            .flex_none()
            .italic()
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(SUBSCRIBE_ALL_LABEL)
            .into_any_element();
    }

    let color = match chip.kind {
        SubscriptionChipKind::Source(source) => color_for_source(source, palette),
        _ => palette.text_faint,
    };

    div()
        .id((
            ElementId::from("srv-sub-chip"),
            SharedString::from(format!("{row_key}:{}", chip.label)),
        ))
        .flex_none()
        .size(SUB_CHIP)
        .rounded(SUB_CHIP_RADIUS)
        .bg(color)
        .tooltip(tooltip_builder(chip.label.clone(), palette))
        .into_any_element()
}

const SUBSCRIBE_ALL_LABEL: &str = "*all";

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
    if token.is_empty() {
        return "-".to_owned();
    }
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

fn is_html_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("html") || ext.eq_ignore_ascii_case("htm"))
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
            entries.push(OwnedOverlayEntry {
                name,
                kind: OwnedOverlayKind::File {
                    html: is_html_path(&entry.path()),
                },
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

fn count_recent_clients(clients: &[ConnectedClientSnapshot]) -> usize {
    clients
        .iter()
        .filter(|c| c.uptime_seconds >= 0 && c.uptime_seconds <= RECENT_CLIENT_WINDOW_SECONDS)
        .count()
}

fn client_row_from_snapshot(client: &ConnectedClientSnapshot) -> OwnedClientRow {
    let liveness = if client.events_per_second > 0.0 {
        ClientLiveness::Active
    } else {
        ClientLiveness::Idle
    };
    OwnedClientRow {
        key: client.identification.clone(),
        identification: client.identification.clone(),
        origin_label: format!("{} · {}", client.remote_addr, client.client_type),
        liveness,
        subscriptions: client.subscriptions.iter().map(subscription_chip).collect(),
        events_per_second: client.events_per_second,
        uptime_short: fmt_uptime_short(client.uptime_seconds.max(0) as u64),
    }
}

fn subscription_chip(filter: &EventFilterSnapshot) -> OwnedSubscriptionChip {
    let source_wildcard = filter.source == "*";
    let kind_wildcard = filter.kind == "*";

    if source_wildcard && kind_wildcard {
        return OwnedSubscriptionChip {
            label: SUBSCRIBE_ALL_LABEL.to_owned(),
            kind: SubscriptionChipKind::All,
        };
    }

    if source_wildcard {
        return OwnedSubscriptionChip {
            label: filter.kind.clone(),
            kind: SubscriptionChipKind::Unknown,
        };
    }

    let source =
        serde_json::from_value::<EventSource>(serde_json::Value::String(filter.source.clone()))
            .ok();
    let source_label = source.map(event_source_label).unwrap_or(&filter.source);
    let label = if kind_wildcard {
        format!("{source_label}.*")
    } else {
        format!("{source_label}.{}", filter.kind)
    };
    OwnedSubscriptionChip {
        label,
        kind: match source {
            Some(source) => SubscriptionChipKind::Source(source),
            None => SubscriptionChipKind::Unknown,
        },
    }
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn snapshot_client(
        identification: &str,
        events_per_second: f32,
        uptime_seconds: i64,
    ) -> ConnectedClientSnapshot {
        ConnectedClientSnapshot {
            identification: identification.to_owned(),
            remote_addr: "203.0.113.10".to_owned(),
            client_type: "obs_browser".to_owned(),
            subscriptions: Vec::new(),
            events_per_second,
            uptime_seconds,
        }
    }

    fn filter(source: &str, kind: &str) -> EventFilterSnapshot {
        EventFilterSnapshot {
            source: source.to_owned(),
            kind: kind.to_owned(),
        }
    }

    #[test]
    fn overlay_origin_rewrites_wildcard_binds_to_loopback() {
        for (bind, expected) in [
            ("127.0.0.1:9515", "http://127.0.0.1:9515"),
            ("192.168.1.5:9515", "http://192.168.1.5:9515"),
            ("0.0.0.0:9515", "http://127.0.0.1:9515"),
            ("[::]:9515", "http://127.0.0.1:9515"),
            ("[::1]:9515", "http://[::1]:9515"),
        ] {
            assert_eq!(overlay_origin(bind), expected, "bind {bind}");
        }
    }

    #[test]
    fn mask_token_keeps_only_the_last_four_characters() {
        let token = "fg_supersecretvalue9c4a";
        let masked = mask_token(token);
        assert!(masked.ends_with("9c4a"), "masked = {masked}");
        assert!(
            !masked.contains("supersecret"),
            "masked token leaks its body: {masked}"
        );
    }

    #[test]
    fn mask_token_handles_tokens_shorter_than_the_revealed_tail() {
        for (token, expected_tail) in [("a", "a"), ("abc", "abc"), ("abcd", "abcd")] {
            let masked = mask_token(token);
            assert!(
                masked.ends_with(expected_tail),
                "token {token} masked to {masked}"
            );
        }
    }

    #[test]
    fn mask_token_splits_multibyte_tokens_on_character_boundaries() {
        let masked = mask_token("префікс-тікт");
        assert!(masked.ends_with("тікт"), "masked = {masked}");
        assert!(!masked.contains("префікс"), "masked = {masked}");
    }

    #[test]
    fn mask_token_renders_a_placeholder_when_no_token_is_stored() {
        assert_eq!(mask_token(""), "-");
    }

    #[test]
    fn is_html_path_accepts_html_and_htm_in_any_case() {
        for name in ["a.html", "a.htm", "a.HTML", "a.Htm"] {
            assert!(is_html_path(std::path::Path::new(name)), "{name}");
        }
        for name in ["a.htmlx", "a.txt", "a.js", "html", "a."] {
            assert!(!is_html_path(std::path::Path::new(name)), "{name}");
        }
    }

    #[test]
    fn count_recent_clients_counts_only_the_last_ten_minutes() {
        let clients = [
            snapshot_client("stale", 0.0, RECENT_CLIENT_WINDOW_SECONDS + 1),
            snapshot_client("edge", 0.0, RECENT_CLIENT_WINDOW_SECONDS),
            snapshot_client("fresh", 0.0, 0),
            snapshot_client("skewed", 0.0, -1),
        ];
        assert_eq!(count_recent_clients(&clients), 2);
    }

    #[test]
    fn client_row_marks_a_silent_client_idle_and_an_emitting_one_active() {
        for (eps, expected) in [
            (0.0, ClientLiveness::Idle),
            (0.05, ClientLiveness::Active),
            (12.5, ClientLiveness::Active),
        ] {
            let row = client_row_from_snapshot(&snapshot_client("dash", eps, 30));
            assert_eq!(row.liveness, expected, "eps {eps}");
        }
    }

    #[test]
    fn client_rows_sharing_a_remote_ip_still_get_distinct_element_keys() {
        let first = client_row_from_snapshot(&snapshot_client("127.0.0.1:51001", 0.0, 30));
        let second = client_row_from_snapshot(&snapshot_client("127.0.0.1:51002", 0.0, 30));
        assert_ne!(
            first.key, second.key,
            "two overlays on one host must not collide on the disconnect button id"
        );
    }

    #[test]
    fn client_row_clamps_a_negative_uptime_to_zero() {
        let row = client_row_from_snapshot(&snapshot_client("dash", 0.0, -42));
        assert_eq!(row.uptime_short, "0s");
    }

    #[test]
    fn subscription_chip_collapses_a_full_wildcard_to_a_single_label() {
        let chip = subscription_chip(&filter("*", "*"));
        assert_eq!(chip.label, SUBSCRIBE_ALL_LABEL);
        assert_eq!(chip.kind, SubscriptionChipKind::All);
    }

    #[test]
    fn subscription_chip_labels_each_source_and_kind_combination() {
        for (source, kind, expected_label, expected_kind) in [
            (
                "twitch",
                "chat.message",
                "twitch.chat.message",
                SubscriptionChipKind::Source(EventSource::Twitch),
            ),
            (
                "twitch",
                "*",
                "twitch.*",
                SubscriptionChipKind::Source(EventSource::Twitch),
            ),
            (
                "you_tube",
                "*",
                "youtube.*",
                SubscriptionChipKind::Source(EventSource::YouTube),
            ),
            (
                "v_tube",
                "model.loaded",
                "vtube.model.loaded",
                SubscriptionChipKind::Source(EventSource::VTube),
            ),
            (
                "*",
                "chat.message",
                "chat.message",
                SubscriptionChipKind::Unknown,
            ),
            (
                "peacock",
                "chat.message",
                "peacock.chat.message",
                SubscriptionChipKind::Unknown,
            ),
        ] {
            let chip = subscription_chip(&filter(source, kind));
            assert_eq!(chip.label, expected_label, "source {source} kind {kind}");
            assert_eq!(chip.kind, expected_kind, "source {source} kind {kind}");
        }
    }
}
