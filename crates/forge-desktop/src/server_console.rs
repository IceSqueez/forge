use forge_components::breadcrumb::BreadcrumbCrumb;
use forge_components::{
    BORDER_THIN, ConfirmTone, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_SM, FONT_XS,
    FONT_XXS, ForgePalette, Icon, OverlayPosition, Radius, Spacing, badge, breadcrumb, card,
    confirm_modal, icon, metric_card, overlay, radius, spacing, sparkline, status_dot, with_alpha,
};
use std::sync::Arc;
use std::time::Duration;

use forge_events::EventSource;
use forge_server::{ConnectedClientSnapshot, EventFilterSnapshot, ServerHandle, ServerSnapshot};
use forge_storage::{CredentialId, CredentialsRepo};
use gpui::{
    AnyElement, ClickEvent, ClipboardItem, Context, Div, Pixels, Rgba, Window, div, prelude::*, px,
    relative,
};

use crate::presentation::ActivePresentation;

/// The credential id under which the server persists its bearer token; read to
/// display the live token in the hero card (the token is never in the snapshot).
const BEARER_CREDENTIAL_ID: &str = "server:bearer";

/// Newest-first cap on retained throughput samples plotted by the throughput sparkline.
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
/// `Running` when the server bound at boot, `Stopped` when disabled or after a Stop,
/// `Error` when a Restart / Stop verb was rejected.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum ServerStatus {
    Running,
    #[default]
    Stopped,
    Error(String),
}

/// Which lifecycle command is mid-flight, gating the header controls into a disabled
/// pending look and freezing the live metric fold until the verb resolves.
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

/// Per-client activity marker: the liveness dot hue in the client table. The poll
/// resolves `Active` / `Idle` from throughput; `Disconnecting` has no snapshot source
/// yet, so it is retained for the hue table but not constructed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum ClientLiveness {
    Active,
    Idle,
    Disconnecting,
}

/// A cached view-model of one connected WS client, mapped from a
/// `ConnectedClientSnapshot` on each metrics poll.
#[derive(Debug, Clone)]
struct OwnedClientRow {
    identification: String,
    client_type_label: String,
    liveness: ClientLiveness,
    subscriptions: Vec<OwnedSubscriptionChip>,
    events_per_second: f32,
    uptime_short: String,
}

/// Coarse content class of an overlay-host entry, driving its kind tag and hue,
/// resolved from a served file's extension.
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
    /// Maps a served file's extension to its coarse content class; anything
    /// unrecognized falls back to [`OwnedFileMime::Other`].
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

/// An overlay-host tree entry: either a served file (with its MIME class) or a
/// subdirectory carrying a child count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OwnedOverlayKind {
    File { mime: OwnedFileMime },
    Dir,
}

/// A cached view-model of one overlay-host entry, mapped from a throttled scan of the
/// server's overlay root.
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
/// throughput stat grid, a throughput chart, and a two-panel overlay-host +
/// connected-clients row, over a per-screen status footer.
///
/// Live data comes from a Category-3 poll of `ServerHandle::snapshot()` — a
/// per-instance `cx.spawn` loop that, once a second while this screen is mounted,
/// hops the snapshot (plus the live bind address and, throttled, the overlay listing)
/// off the tokio runtime through a oneshot and folds it into these cached fields.
/// `server == None` (disabled at boot, or the bind failed) renders the Stopped state
/// and runs no poll. Restart / Stop dispatch the real `ServerHandle` lifecycle verbs
/// on `rt_handle`; Regenerate rotates the bearer token through the handle's auth
/// state; Open-folder opens the overlay root through the OS shell; Copy writes the
/// clipboard; force-disconnect removes the row locally after a two-phase confirm (the
/// server exposes no force-disconnect capability, matching the retiring UI).
pub struct ServerConsoleView {
    /// The hosted server handle when it bound at boot, else `None`. Lifecycle verbs
    /// and the metrics poll early-return when absent.
    server: Option<ServerHandle>,
    /// The tokio runtime handle: snapshot polling and every lifecycle verb do real
    /// network / lock / fs I/O and must run with a reactor, not on gpui's foreground
    /// executor, so they are spawned here and hopped back through a oneshot.
    rt_handle: tokio::runtime::Handle,
    /// The credentials repo, read to surface the live bearer token (never carried in
    /// the snapshot) and passed to the auth state on a token regenerate.
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
    /// Two-phase disconnect gate — armed by a row's `X`, resolved by the confirm
    /// overlay. Holds the target client's stable `identification` (not its row index,
    /// which can shift under a live snapshot refresh). `None` = no confirm showing.
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
        // Surface the real bearer token regardless of run state, then start the live
        // metrics poll only when a server is actually bound.
        view.fetch_token(cx);
        if view.server.is_some() {
            view.start_poll(cx);
        }
        view
    }

    // --- live poll --------------------------------------------------------

    /// Reads the persisted bearer token off the credentials repo and applies it into
    /// the hero card. The read touches SQLite, so it hops the tokio runtime and comes
    /// back through a oneshot; a released view makes the apply a no-op.
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

    /// View-scoped Category-3 metrics poll. Once a second it hops the tokio runtime
    /// for a fresh `snapshot()` plus the live bind address, and (on the first tick and
    /// every fifth thereafter, mirroring the retiring cadence) a fresh overlay-root
    /// scan, then folds the result via [`Self::apply_poll`]. The task is tied to this
    /// view's lifetime: once the user navigates away and the entity is released,
    /// `this.update` returns `Err` and the loop ends. Deliberately per-instance, not a
    /// boot-global drain — a slow snapshot must never stall the shared runtime→UI bus
    /// bridge.
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

    /// Folds one poll result into the cached view-model. The bind address always
    /// tracks the live bind; the throughput / client / uptime metrics only fold while
    /// the server is believed up and no lifecycle verb is mid-flight, so a Stopped or
    /// restarting console freezes its last readout instead of flickering; the overlay
    /// listing replaces the cached tree whenever a fresh scan is present.
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
                // Not carried by `ServerSnapshot`; the console leaves these at zero.
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

    /// Rotates the bearer token through the running server's auth state and folds the
    /// new token back into the hero card. A no-op when the server is not running.
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

    /// Dispatches the real server restart on the tokio runtime, gating the header
    /// controls into their pending look until the verb resolves. A no-op when the
    /// server is not running or another verb is already mid-flight.
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

    /// Dispatches the real server stop on the tokio runtime, gating the header controls
    /// into their pending look until the verb resolves. A no-op when the server is not
    /// running or another verb is already mid-flight.
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

    /// Opens the overlay-host root in the OS file manager. The path lookup and the
    /// blocking shell-open both run off the foreground executor on the tokio runtime.
    /// A no-op when the server is not running.
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
    /// reveal toggle is fully local; Copy writes the clipboard; Regenerate rotates the
    /// token through the running server's auth state.
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

    // --- stat grid + throughput ------------------------------------------

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

// ── live poll plumbing ────────────────────────────────────────────────────

/// One resolved metrics poll, assembled on the tokio runtime and hopped back to the
/// view: a fresh server snapshot, the live bind address, and — only on the throttled
/// scan ticks — a fresh overlay-root listing.
struct ServerPoll {
    snapshot: ServerSnapshot,
    bind_address: String,
    overlay: Option<OverlayListing>,
}

/// A resolved overlay-root scan: the resolved root path plus its sorted entries.
struct OverlayListing {
    root: String,
    entries: Vec<OwnedOverlayEntry>,
}

/// Enumerates the overlay-host root one level deep, resolving each entry's kind, size
/// and (for directories) immediate child count, then sorts directories first and each
/// group case-insensitively by name. An unreadable root yields an empty listing.
/// Runs on the tokio runtime — `tokio::fs` needs a reactor.
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

/// Maps one connected-client snapshot row to its cached view-model: an
/// `address · type` identity subtitle, an Active/Idle liveness marker keyed off
/// throughput, and its resolved subscription chips.
fn client_row_from_snapshot(client: &ConnectedClientSnapshot) -> OwnedClientRow {
    let liveness = if client.events_per_second > 0.0 {
        ClientLiveness::Active
    } else {
        ClientLiveness::Idle
    };
    OwnedClientRow {
        identification: client.identification.clone(),
        client_type_label: format!("{} · {}", client.remote_addr, client.client_type),
        liveness,
        subscriptions: client.subscriptions.iter().map(subscription_chip).collect(),
        events_per_second: client.events_per_second,
        uptime_short: format_short_duration_secs(client.uptime_seconds),
    }
}

/// Resolves one event-filter snapshot into a source-tinted chip: a wildcard source
/// (`"*"`) tints neutrally as `Core`, otherwise the source string is parsed back to an
/// [`EventSource`] for its hue, and the label reproduces the `source.kind` / `source.*`
/// / bare-kind / `*` forms.
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

/// The lowercase source token used in subscription-chip labels.
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

/// Compact per-client uptime: seconds, then whole minutes, then whole hours.
fn format_short_duration_secs(seconds: i64) -> String {
    let seconds = seconds.max(0);
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3_600 {
        format!("{}m", seconds / 60)
    } else {
        format!("{}h", seconds / 3_600)
    }
}

/// PII-safe rendering of a rejected server verb for the console's error state — the
/// coarse `ServerError` Display, no bearer token or URL.
fn err_text(err: forge_server::ServerError) -> String {
    err.to_string()
}
