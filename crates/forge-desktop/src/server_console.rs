use forge_components::breadcrumb::BreadcrumbCrumb;
use forge_components::{
    BORDER_THIN, ConfirmTone, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_SM, FONT_XS,
    FONT_XXS, ForgePalette, Icon, OverlayPosition, Radius, Spacing, badge, breadcrumb, card,
    confirm_modal, icon, metric_card, overlay, radius, spacing, status_dot, with_alpha,
};
use forge_events::EventSource;
use gpui::{
    AnyElement, ClickEvent, ClipboardItem, Context, Div, Pixels, Rgba, Window, div, prelude::*, px,
    relative,
};

use crate::presentation::ActivePresentation;

/// Newest-first cap on retained throughput samples; the deferred sparkline reads the
/// tail of this history once a kit chart component lands.
const MAX_BANDWIDTH_SAMPLES: usize = 60;
/// Subscription chips shown inline before collapsing the remainder into a "+N more"
/// pill — the parity source pins this at three visible chips.
const MAX_VISIBLE_CHIPS: usize = 3;

/// Liveness dot diameter in the client table (the source's fixed 6px disc).
const CLIENT_DOT: Pixels = px(6.0);
/// Header / footer status dot diameter (the source's fixed 7px disc).
const STATUS_DOT: Pixels = px(7.0);
/// Client-table leading dot column width (the source's fixed 24px cell).
const DOT_CELL_W: Pixels = px(24.0);
/// Client-table events/sec column width (the source's fixed 80px cell).
const EVS_CELL_W: Pixels = px(80.0);
/// Client-table uptime column width (the source's fixed 70px cell).
const UPTIME_CELL_W: Pixels = px(70.0);
/// Client-table trailing disconnect column width (the source's fixed 22px cell).
const X_CELL_W: Pixels = px(22.0);
/// Hero icon tile side (the source's fixed 48px square).
const ICON_BOX: Pixels = px(48.0);
/// Hero server glyph size (the source's fixed 20px icon).
const SERVER_GLYPH: Pixels = px(20.0);
/// Disconnect glyph size in a client row (the source's fixed 13px icon).
const X_GLYPH: Pixels = px(13.0);
/// Overlay copy / external-link glyph size (the source's fixed 11px icons).
const LINK_GLYPH: Pixels = px(11.0);
/// Header control glyph size — restart / stop / copy / folder-open (source's 12px).
const CONTROL_GLYPH: Pixels = px(12.0);
/// Panel-header glyph size — users / folder / chart (the source's fixed 14px icon).
const HEADER_GLYPH: Pixels = px(14.0);
/// Client count pill corner radius (the source's fixed 8px).
const COUNT_BADGE_RADIUS: Pixels = px(8.0);
/// Overlay kind-tag pill corner radius (the source's fixed 4px).
const KIND_BADGE_RADIUS: Pixels = px(4.0);
/// Client-table column grow weights reproducing the source's `FillPortion(14)` /
/// `FillPortion(16)` split for the identity and subscriptions columns.
const CLIENT_GROW: f32 = 14.0;
const SUBS_GROW: f32 = 16.0;

/// Lifecycle state of the hosted WS+HTTP server, driving the status dot hue and label.
/// The `Error` arm mirrors the runtime taxonomy the bridge will deliver; the seed
/// pins `Running`, so it is not constructed here yet.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(dead_code)]
enum ServerStatus {
    Running,
    #[default]
    Stopped,
    Error(String),
}

/// Which lifecycle command is mid-flight, gating the header controls into a disabled
/// pending look. Seeded `None` and never set here — Restart / Stop are inert
/// placeholders (see [`ServerConsoleView::restart_server`]) — but the pending visuals
/// stay wired for when the real lifecycle capability lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServerControl {
    Restarting,
    Stopping,
}

/// One event-source subscription a client holds, rendered as a source-tinted chip.
#[derive(Debug, Clone)]
struct OwnedSubscriptionChip {
    label: String,
    source: EventSource,
}

/// Per-client activity marker: the liveness dot hue in the client table. The
/// `Disconnecting` arm mirrors the runtime taxonomy the bridge will deliver; the seed
/// exercises `Active` / `Idle`, so it is not constructed here yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum ClientLiveness {
    Active,
    Idle,
    Disconnecting,
}

/// A cached view-model of one connected WS client. The live set arrives over the
/// runtime→UI bridge from `forge-server` once wired; here it is seeded representative.
#[derive(Debug, Clone)]
struct OwnedClientRow {
    identification: String,
    client_type_label: String,
    liveness: ClientLiveness,
    subscriptions: Vec<OwnedSubscriptionChip>,
    events_per_second: f32,
    uptime_short: String,
}

/// Coarse content class of an overlay-host entry, driving its kind tag and hue. The
/// non-HTML arms mirror the runtime's MIME taxonomy the bridge will deliver; the seed
/// exercises `Html`, so they are not constructed here yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum OwnedFileMime {
    Html,
    Css,
    Js,
    Json,
    Image,
    Wasm,
    Other,
}

/// An overlay-host tree entry: either a served file (with its MIME class) or a
/// subdirectory carrying a child count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OwnedOverlayKind {
    File { mime: OwnedFileMime },
    Dir,
}

/// A cached view-model of one overlay-host entry, seeded until the runtime→UI bridge
/// delivers the real sandbox listing.
#[derive(Debug, Clone)]
struct OwnedOverlayEntry {
    name: String,
    kind: OwnedOverlayKind,
    size_bytes: u64,
    child_count: u32,
}

/// Aggregate server throughput counters shown in the stat grid and footer.
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

/// The WebSocket-server console screen view-entity: a hero credentials card, a
/// throughput stat grid, a (deferred) throughput chart, and a two-panel overlay-host
/// + connected-clients row, over a per-screen status footer.
///
/// Owns its server snapshot as seeded stub state — `forge-desktop` wires no server
/// crate yet, so the status, credentials, clients, stats and overlay listing are
/// seeded representative and the handlers mutate this cached state. The real screen
/// loads them from `forge-server` over the runtime→UI bridge (`ServerInfoArrived` /
/// `BandwidthTick` / `OverlayListingArrived`); Restart / Stop / Regenerate drive the
/// server lifecycle through its handle, and force-disconnect removes the client
/// through the same handle. Here Restart / Stop / Regenerate and Open-folder are inert
/// placeholders, Copy writes the clipboard, and disconnect removes the row locally
/// after a two-phase confirm.
pub struct ServerConsoleView {
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
    /// Two-phase disconnect gate — armed by a row's `X`, resolved by the confirm
    /// overlay. Holds the target client's stable `identification` (not its row index,
    /// which can shift under a live snapshot refresh). `None` = no confirm showing.
    pending_disconnect: Option<String>,
}

impl ServerConsoleView {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            bind_address: "0.0.0.0:8081".to_owned(),
            bearer_token: "fg_placeholder00000000000000005L9k".to_owned(),
            token_revealed: false,
            server_status: ServerStatus::Running,
            control_in_flight: None,
            uptime_seconds: 8040,
            connected_clients: seed_clients(),
            bandwidth_samples: seed_bandwidth(),
            stats: seed_stats(),
            overlay_root: "~/.local/share/forge/overlays".to_owned(),
            overlay_entries: seed_overlays(),
            selected_overlay_file: Some(1),
            pending_disconnect: None,
        }
    }

    // --- handlers ---------------------------------------------------------

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

    /// Placeholder — regenerating the bearer token needs a server capability that does
    /// not exist yet, so the affordance renders and clicks inertly.
    fn regenerate_token(&mut self, _cx: &mut Context<Self>) {}

    /// Placeholder — the lifecycle restart capability is not wired yet, so the control
    /// renders in its enabled look and clicks inertly.
    fn restart_server(&mut self, _cx: &mut Context<Self>) {}

    /// Placeholder — the lifecycle stop capability is not wired yet, so the control
    /// renders in its enabled look and clicks inertly.
    fn stop_server(&mut self, _cx: &mut Context<Self>) {}

    /// Placeholder — opening the overlay folder is an OS-shell action routed through
    /// the runtime once wired; here it clicks inertly.
    fn open_overlay_folder(&mut self, _cx: &mut Context<Self>) {}

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

    /// Removes the armed client from the cached roster. The real force-disconnect goes
    /// through the server handle; here it only drops the local row.
    fn confirm_disconnect(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.pending_disconnect.take() {
            self.connected_clients.retain(|c| c.identification != id);
        }
        cx.notify();
    }

    // --- hero credentials card -------------------------------------------

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
                    .child("Built-in Server"),
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
            .child("Internal HTTP + WebSocket server for overlays and remote control");

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
            format!("Up {}", format_server_uptime(self.uptime_seconds))
        } else {
            "Not running".to_owned()
        };

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(caption("BIND ADDRESS", palette))
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
            .child(caption("BEARER TOKEN", palette))
            .child(self.bearer_token_row(palette, density, cx))
            .child(self.regen_warning(palette, density))
            .into_any_element()
    }

    /// Masked-token box (eye + copy affordances) beside the Regenerate control. The
    /// reveal toggle is fully local; Copy writes the clipboard; Regenerate is inert.
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
                    .child("Regenerate"),
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
                            .child("Regenerating disconnects all clients"),
                    ),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child("Connected WebSocket clients must reconnect with the new token."),
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
            "Restarting…"
        } else {
            "Restart"
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
            "Stopping…"
        } else {
            "Stop"
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
                    .child("COPY"),
            )
            .into_any_element()
    }

    // --- stat grid + deferred throughput ---------------------------------

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
                    "CLIENTS",
                    format!("{}", self.connected_clients.len()),
                    Some("connected"),
                    Some(success),
                    palette,
                )
                .into_any_element(),
            ))
            .child(cell(
                metric_card(
                    "EVENTS OUT",
                    format!("{:.1} ev/s", self.stats.events_per_second),
                    Some(format!("avg {:.1} ev/s", self.stats.events_per_second_avg)),
                    None,
                    palette,
                )
                .into_any_element(),
            ))
            .child(cell(
                metric_card(
                    "HTTP REQUESTS",
                    format!("{}", self.stats.http_requests),
                    Some("overlays served"),
                    None,
                    palette,
                )
                .into_any_element(),
            ))
            .child(cell(
                metric_card(
                    "BANDWIDTH",
                    format!("{:.0} KB/s", self.stats.bandwidth_kbps),
                    Some(format!("peak {:.0} KB/s", self.stats.bandwidth_peak_kbps)),
                    Some(success),
                    palette,
                )
                .into_any_element(),
            ))
            .into_any_element()
    }

    /// The throughput card. The drawn sparkline is deferred until a kit chart
    /// component exists; the card frame and header render with a numeric band as
    /// placeholder content so the section stays present.
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
                            .child("Throughput"),
                    ),
            )
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(format!("last {sample_count}s · peak {peak:.0} KB/s")),
            );

        let placeholder = div()
            .w_full()
            .h(px(48.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(radius(Radius::Sm))
            .bg(palette.shell)
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_faint)
                    .child("—"),
            );

        card(
            div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Sm, density))
                .child(header)
                .child(placeholder),
            palette,
        )
        .padding(spacing(Spacing::Md, density))
        .radius(Radius::Lg)
        .full_width()
        .into_any_element()
    }

    // --- overlay-host panel ----------------------------------------------

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
                    .child("Overlay Files"),
            );

        let root_label = if self.overlay_root.is_empty() {
            "—".to_owned()
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
                            .child("OPEN"),
                    ),
            );

        let files: AnyElement = if self.overlay_entries.is_empty() {
            div()
                .w_full()
                .py(spacing(Spacing::Lg, density))
                .px(spacing(Spacing::Sm, density))
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_faint)
                .child("No overlay files found")
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
            OwnedOverlayKind::Dir => format!("{} items", entry.child_count),
            OwnedOverlayKind::File { .. } => format_bytes(entry.size_bytes),
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
                            .id(("srv-overlay-copy", index))
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
            .id(("srv-overlay-entry", index))
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

    // --- connected-clients panel -----------------------------------------

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
                    .child("Connected Clients"),
            )
            .child(div().flex_1())
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child("live"),
            )
            .child(count_badge);

        let col_header = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Sm, density))
            .child(div().w(DOT_CELL_W).flex_none())
            .child(weighted(CLIENT_GROW, col_caption("CLIENT", palette)))
            .child(weighted(SUBS_GROW, col_caption("SUBSCRIPTIONS", palette)))
            .child(
                div()
                    .w(EVS_CELL_W)
                    .flex_none()
                    .child(col_caption("EV/S", palette)),
            )
            .child(
                div()
                    .w(UPTIME_CELL_W)
                    .flex_none()
                    .child(col_caption("UPTIME", palette)),
            )
            .child(div().w(X_CELL_W).flex_none());

        let rows: AnyElement = if self.connected_clients.is_empty() {
            div()
                .w_full()
                .py(spacing(Spacing::Lg, density))
                .px(spacing(Spacing::Sm, density))
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_faint)
                .child("No clients connected")
                .into_any_element()
        } else {
            let mut col = div().w_full().flex().flex_col();
            for (index, row) in self.connected_clients.iter().enumerate() {
                col = col.child(self.client_row(index, row, palette, density, cx));
            }
            div()
                .id("srv-clients-scroll")
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
            .child(hline(palette.border_regular))
            .child(col_header)
            .child(rows);

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
    ) -> AnyElement {
        let dot_color = match row.liveness {
            ClientLiveness::Active => palette.success,
            ClientLiveness::Idle => palette.warning,
            ClientLiveness::Disconnecting => palette.random,
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
            .id(("srv-disconnect", index))
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

        let content = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .hover(move |s| s.bg(palette.surface_overlay))
            .child(
                div()
                    .w(DOT_CELL_W)
                    .flex_none()
                    .child(status_dot(dot_color, CLIENT_DOT)),
            )
            .child(weighted(CLIENT_GROW, id_col))
            .child(weighted(
                SUBS_GROW,
                chips_row(&row.subscriptions, palette, density),
            ))
            .child(
                div()
                    .w(EVS_CELL_W)
                    .flex_none()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(format!("{:.1}", row.events_per_second)),
            )
            .child(
                div()
                    .w(UPTIME_CELL_W)
                    .flex_none()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(row.uptime_short.clone()),
            )
            .child(div().w(X_CELL_W).flex_none().child(x_button));

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(content)
            .child(hline(palette.elevated))
            .into_any_element()
    }

    // --- footer + overlay ------------------------------------------------

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
                    .child(format!(
                        "Total sent: {} · Total events: {}",
                        format_bytes(self.stats.total_bytes_sent),
                        self.stats.total_events_out
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

    /// The armed disconnect confirm, or `None` if the pending client has vanished from
    /// the roster since its `X` was clicked (defensive, matching the other confirm
    /// consumers).
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

        let message = format!(
            "Client at {} will be disconnected from the WebSocket server. Other clients are not affected.",
            row.client_type_label
        );
        let confirm = confirm_modal("Disconnect client?", message, ConfirmTone::Warning, palette)
            .item_name(row.identification.clone())
            .esc_hint("to cancel")
            .on_cancel(
                "srv-disc-cancel",
                "Cancel",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_disconnect(cx)),
            )
            .on_confirm(
                "srv-disc-confirm",
                "Disconnect",
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
                BreadcrumbCrumb::leaf("Builtin"),
                BreadcrumbCrumb::leaf("Server"),
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

// ── view-specific fragments ───────────────────────────────────────────────

/// A flex table cell that grows proportionally to `grow`, reproducing the source's
/// `FillPortion` split. `flex_basis: 0` makes the grow weight the sole size driver.
fn weighted(grow: f32, child: impl IntoElement) -> Div {
    let mut cell = div().min_w(px(0.0)).child(child);
    let style = cell.style();
    style.flex_grow = Some(grow);
    style.flex_basis = Some(relative(0.0).into());
    cell
}

/// A full-width 1px rule inking `color` — the source's separator geometry.
fn hline(color: Rgba) -> Div {
    div().w_full().h(px(1.0)).bg(color)
}

/// An uppercase mono caption over a credentials field, inking `text_muted`.
fn caption(label: &'static str, palette: &ForgePalette) -> impl IntoElement {
    div()
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_XS)
        .text_color(palette.text_muted)
        .child(label)
}

/// A client-table column caption, inking `text_faint`.
fn col_caption(label: &'static str, palette: &ForgePalette) -> impl IntoElement {
    div()
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_XS)
        .text_color(palette.text_faint)
        .child(label)
}

/// The subscription-chip cluster: up to [`MAX_VISIBLE_CHIPS`] source-tinted label
/// chips, collapsing any remainder into a faint "+N more" pill; an empty set renders
/// an em dash.
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
            .child("—")
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

/// One subscription chip: a mono label inking `fg` on a `bg` pill.
fn chip_pill(label: String, fg: Rgba, bg: Rgba) -> impl IntoElement {
    div()
        .flex_none()
        .py(spacing(Spacing::Xxs, Density::Cozy))
        .px(spacing(Spacing::Xxs, Density::Cozy))
        .rounded(radius(Radius::Md))
        .bg(bg)
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XS)
                .text_color(fg)
                .child(label),
        )
}

// ── formatting + resolution helpers ───────────────────────────────────────

/// The status-dot / label hue for the server lifecycle state.
fn status_color(status: &ServerStatus, palette: &ForgePalette) -> Rgba {
    match status {
        ServerStatus::Running => palette.success,
        ServerStatus::Stopped => palette.text_faint,
        ServerStatus::Error(_) => palette.random,
    }
}

fn status_label(status: &ServerStatus) -> &'static str {
    match status {
        ServerStatus::Running => "Running",
        ServerStatus::Stopped => "Stopped",
        ServerStatus::Error(_) => "Error",
    }
}

fn format_server_uptime(seconds: i64) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3_600 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h {}m", seconds / 3_600, (seconds % 3_600) / 60)
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1_024 {
        format!("{bytes} B")
    } else if bytes < 1_048_576 {
        format!("{:.1} KB", bytes as f64 / 1_024.0)
    } else if bytes < 1_073_741_824 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else {
        format!("{:.2} GB", bytes as f64 / 1_073_741_824.0)
    }
}

fn extract_port(bind_address: &str) -> &str {
    bind_address.split(':').next_back().unwrap_or("8081")
}

/// Builds the browser-reachable origin for overlay URLs from the configured
/// `host:port` bind address. Unspecified binds (`0.0.0.0` / `::`) are rendered as
/// loopback, since those are placeholders a browser cannot open.
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

/// Maps an event source to its semantic palette hue, so a subscription chip re-tints
/// with the active theme.
fn color_for_source(source: EventSource, palette: &ForgePalette) -> Rgba {
    match source {
        EventSource::Twitch => palette.brand,
        EventSource::YouTube => palette.random,
        EventSource::Kick => palette.info,
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

/// Masks the bearer token for its default hidden state: a `fg_` prefix, a fixed bullet
/// run and the trailing four characters — matching the design's reveal-off form.
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

// ── seeded stub state ─────────────────────────────────────────────────────

/// The representative connected-client roster seeded before a server crate is wired,
/// mirroring the design's sample so both liveness hues, the overflow "+N more" pill
/// and every source-tinted chip render populated.
fn seed_clients() -> Vec<OwnedClientRow> {
    vec![
        OwnedClientRow {
            identification: "chat.html".to_owned(),
            client_type_label: "192.168.1.42 · OBS Browser".to_owned(),
            liveness: ClientLiveness::Active,
            subscriptions: vec![
                sub("twitch.chat", EventSource::Twitch),
                sub("youtube.chat", EventSource::YouTube),
                sub("kick.chat", EventSource::Kick),
                sub("discord.msg", EventSource::Discord),
            ],
            events_per_second: 11.2,
            uptime_short: "2h 14m".to_owned(),
        },
        OwnedClientRow {
            identification: "forge-mobile".to_owned(),
            client_type_label: "192.168.1.5 · iOS app".to_owned(),
            liveness: ClientLiveness::Active,
            subscriptions: vec![
                sub("twitch.*", EventSource::Twitch),
                sub("obs.*", EventSource::Obs),
                sub("server.*", EventSource::Server),
            ],
            events_per_second: 4.8,
            uptime_short: "47m".to_owned(),
        },
        OwnedClientRow {
            identification: "goals.html".to_owned(),
            client_type_label: "127.0.0.1 · OBS Browser".to_owned(),
            liveness: ClientLiveness::Idle,
            subscriptions: vec![
                sub("twitch.cheer", EventSource::Twitch),
                sub("timer.tick", EventSource::Timer),
            ],
            events_per_second: 0.1,
            uptime_short: "2h 14m".to_owned(),
        },
        OwnedClientRow {
            identification: "stream-deck".to_owned(),
            client_type_label: "192.168.1.5 · Stream Deck".to_owned(),
            liveness: ClientLiveness::Active,
            subscriptions: vec![
                sub("twitch.reward", EventSource::Twitch),
                sub("obs.scene", EventSource::Obs),
            ],
            events_per_second: 2.1,
            uptime_short: "1h 12m".to_owned(),
        },
    ]
}

fn sub(label: &str, source: EventSource) -> OwnedSubscriptionChip {
    OwnedSubscriptionChip {
        label: label.to_owned(),
        source,
    }
}

/// The representative overlay-host listing seeded before the sandbox listing arrives,
/// mirroring the design's sample so both file and directory rows render.
fn seed_overlays() -> Vec<OwnedOverlayEntry> {
    vec![
        OwnedOverlayEntry {
            name: "alerts.html".to_owned(),
            kind: OwnedOverlayKind::File {
                mime: OwnedFileMime::Html,
            },
            size_bytes: 14_541,
            child_count: 0,
        },
        OwnedOverlayEntry {
            name: "chat.html".to_owned(),
            kind: OwnedOverlayKind::File {
                mime: OwnedFileMime::Html,
            },
            size_bytes: 8_294,
            child_count: 0,
        },
        OwnedOverlayEntry {
            name: "goals.html".to_owned(),
            kind: OwnedOverlayKind::File {
                mime: OwnedFileMime::Html,
            },
            size_bytes: 5_837,
            child_count: 0,
        },
        OwnedOverlayEntry {
            name: "assets/".to_owned(),
            kind: OwnedOverlayKind::Dir,
            size_bytes: 0,
            child_count: 4,
        },
    ]
}

/// The seeded throughput counters shown in the stat grid and footer.
fn seed_stats() -> ServerStats {
    ServerStats {
        events_per_second: 14.2,
        events_per_second_avg: 12.0,
        http_requests: 2_138,
        bandwidth_kbps: 184.0,
        bandwidth_peak_kbps: 220.0,
        total_bytes_sent: 2_576_980_378,
        total_events_out: 28_471,
    }
}

/// A representative bandwidth history so the deferred throughput card reports a
/// non-empty sample window and peak.
fn seed_bandwidth() -> Vec<f32> {
    vec![
        120.0, 138.0, 96.0, 172.0, 150.0, 184.0, 205.0, 168.0, 142.0, 190.0, 176.0, 158.0, 210.0,
        188.0, 164.0, 180.0, 196.0, 172.0, 148.0, 220.0,
    ]
}
