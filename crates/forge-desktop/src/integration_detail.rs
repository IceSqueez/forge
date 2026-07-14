use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, ConfirmTone, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density,
    FONT_LG, FONT_SM, FONT_XS, ForgePalette, Icon, OverlayPosition, Radius, Spacing, ToastKind,
    breadcrumb, confirm_modal, icon, overlay, radius, spacing, with_alpha,
};
use forge_platform_core::{
    BuiltinContent, BuiltinControl, BuiltinHealth, BuiltinStatus, CapabilityFlags, ConnectionState,
    DetailSection, HeaderAction, HealthDelta, HealthMetric, QuickAction, QuickActions, SectionIcon,
};
use futures_util::StreamExt as _;
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, FontWeight, Rgba, Subscription, Window,
    div, prelude::*, px,
};
use std::sync::Arc;
use std::time::Duration;

use crate::builtin_sections::{content_sections, format_uptime, health_grid};
use crate::platforms::PlatformConnectivity;
use crate::presentation::ActivePresentation;
use crate::screen::Screen;
use crate::sidebar::NavRequested;
use crate::toasts::PushToast;

/// The single generic integration detail screen. It consumes the four `Builtin*`
/// trait outputs — status, health metrics, content sections, quick actions — and
/// renders them uniformly, so no integration has any per-screen detail code: a
/// new integration reaches this view by supplying the four traits, nothing here
/// changes. It never switches on the integration id when rendering.
///
/// The view holds the live trait objects and a cached snapshot read from them: the
/// snapshot is read synchronously on mount and re-read whenever the observed
/// connectivity topic advances (a `platform.connection.changed` fold), so the header,
/// alt-state, health, content and quick actions track the real `ConnectionState`.
pub struct IntegrationDetail {
    // Live trait surface, held so the snapshot can be re-read on a connection change.
    status: Arc<dyn BuiltinStatus>,
    health: Arc<dyn BuiltinHealth>,
    content: Arc<dyn BuiltinContent>,
    quick: Arc<dyn QuickActions>,
    // The lifecycle-verb handle (reconnect / disconnect / refresh-token). `None`
    // when this integration exposes no control surface (seed fallback / no
    // credentials); a lifecycle action is then a silent no-op, matching the header
    // buttons that still render but do nothing.
    control: Option<Arc<dyn BuiltinControl>>,
    // The tokio runtime handle onto which a control verb is spawned: the verb does
    // real network I/O, so it must run with a tokio reactor rather than on gpui's
    // foreground executor.
    rt_handle: tokio::runtime::Handle,
    icon: SectionIcon,
    display_name: String,
    version: Option<String>,
    endpoint: Option<String>,
    uptime: Option<Duration>,
    connection: ConnectionState,
    capability_flags: CapabilityFlags,
    header_actions: Vec<HeaderAction>,
    health_metrics: [HealthMetric; 4],
    sections: Vec<DetailSection>,
    quick_actions: Vec<QuickAction>,
    /// Two-phase disconnect gate: armed by the header Disconnect action, rendered
    /// by the shared confirm modal. `false` = no confirm showing.
    pending_disconnect: bool,
    /// Transient feedback line for a dispatched lifecycle/quick action. Without a
    /// live runtime the action is stubbed and only this toast is shown.
    toast: Option<String>,
    /// Held so the connectivity observation lives for the view's lifetime.
    _conn_obs: Subscription,
}

impl EventEmitter<NavRequested> for IntegrationDetail {}

impl IntegrationDetail {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        icon: SectionIcon,
        status: Arc<dyn BuiltinStatus>,
        health: Arc<dyn BuiltinHealth>,
        content: Arc<dyn BuiltinContent>,
        quick: Arc<dyn QuickActions>,
        control: Option<Arc<dyn BuiltinControl>>,
        rt_handle: tokio::runtime::Handle,
        connectivity: Entity<PlatformConnectivity>,
        cx: &mut Context<Self>,
    ) -> Self {
        // The connectivity fold advances on a `platform.connection.changed`; re-read
        // this integration's live snapshot from its trait objects whenever it does.
        let conn_obs = cx.observe(&connectivity, |this, _, cx| this.reload(cx));

        let display_name = status.display_name().to_owned();
        let version = status.version().map(ToOwned::to_owned);
        let endpoint = status.endpoint().map(ToOwned::to_owned);
        let uptime = status.uptime();
        let connection = status.connection();
        let capability_flags = status.capability_flags();
        let header_actions = status.header_actions();
        let health_metrics = health.metrics();
        let sections = content.sections();
        let quick_actions = quick.actions();

        // View-scoped live health drain: seeded synchronously above from
        // `metrics()`, then this per-instance `stream()` folds each delta into the
        // grid. The task is tied to this view's lifetime — once the user navigates
        // away and the entity is released, `this.update` returns `Err` and the loop
        // ends. It is deliberately NOT a boot-global drain: a lagging health stream
        // must never stall the shared runtime→UI bridge topics.
        let mut health_stream = health.stream();
        cx.spawn(async move |this, cx| {
            while let Some(delta) = health_stream.next().await {
                if this
                    .update(cx, |detail, cx| detail.apply_health_delta(delta, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();

        Self {
            status,
            health,
            content,
            quick,
            control,
            rt_handle,
            icon,
            display_name,
            version,
            endpoint,
            uptime,
            connection,
            capability_flags,
            header_actions,
            health_metrics,
            sections,
            quick_actions,
            pending_disconnect: false,
            toast: None,
            _conn_obs: conn_obs,
        }
    }

    /// Re-reads the cached snapshot from the live trait objects and repaints. Called
    /// when the connectivity topic advances so the header, alt-state, health, content
    /// and quick actions reflect the integration's current `ConnectionState`.
    fn reload(&mut self, cx: &mut Context<Self>) {
        self.display_name = self.status.display_name().to_owned();
        self.version = self.status.version().map(ToOwned::to_owned);
        self.endpoint = self.status.endpoint().map(ToOwned::to_owned);
        self.uptime = self.status.uptime();
        self.connection = self.status.connection();
        self.capability_flags = self.status.capability_flags();
        self.header_actions = self.status.header_actions();
        self.health_metrics = self.health.metrics();
        self.sections = self.content.sections();
        self.quick_actions = self.quick.actions();
        cx.notify();
    }

    /// Folds a single live health delta into the cached 4-metric grid and
    /// repaints. The grid is fixed at four cells, so an out-of-range index is
    /// ignored (no repaint). Driven by the view-scoped health drain started on
    /// mount.
    fn apply_health_delta(&mut self, delta: HealthDelta, cx: &mut Context<Self>) {
        let idx = delta.index as usize;
        if idx < self.health_metrics.len() {
            self.health_metrics[idx].value = delta.new_value;
            cx.notify();
        }
    }

    fn go_back(&mut self, cx: &mut Context<Self>) {
        cx.emit(NavRequested(Screen::Platforms));
    }

    fn on_header_action(&mut self, action: HeaderAction, cx: &mut Context<Self>) {
        match action {
            // Destructive: arm the two-phase confirm gate; the verb itself fires
            // only once the modal is accepted (see `confirm_disconnect`).
            HeaderAction::Disconnect => {
                self.pending_disconnect = true;
                cx.notify();
            }
            HeaderAction::Reconnect => self.dispatch_control(ControlVerb::Reconnect),
            HeaderAction::RefreshToken => self.dispatch_control(ControlVerb::RefreshToken),
            HeaderAction::Settings => {
                self.toast = Some("Settings coming soon".to_owned());
                cx.notify();
            }
        }
    }

    /// Spawns a lifecycle verb onto the tokio runtime. With no `control` surface the
    /// dispatch is a silent no-op (the header button still renders but does nothing),
    /// matching how the integration presents an absent control. The resulting steady
    /// connection state is not returned here: it is observed through the
    /// `platform.connection.changed` bridge, which advances the connectivity topic and
    /// triggers `reload`. A rejected verb is logged with the trait's coarse,
    /// PII-safe reason and never surfaced as transport detail.
    fn dispatch_control(&self, verb: ControlVerb) {
        let Some(ctrl) = self.control.clone() else {
            return;
        };
        self.rt_handle.spawn(async move {
            let outcome = match verb {
                ControlVerb::Reconnect => ctrl.reconnect().await,
                ControlVerb::Disconnect => ctrl.disconnect().await,
                ControlVerb::RefreshToken => ctrl.refresh_token().await,
            };
            if let Err(failure) = outcome {
                eprintln!("forge-desktop: integration control action failed: {failure}");
            }
        });
    }

    fn cancel_disconnect(&mut self, cx: &mut Context<Self>) {
        self.pending_disconnect = false;
        cx.notify();
    }

    fn confirm_disconnect(&mut self, cx: &mut Context<Self>) {
        self.pending_disconnect = false;
        self.dispatch_control(ControlVerb::Disconnect);
        cx.notify();
    }

    fn on_quick_action(&mut self, idx: usize, cx: &mut Context<Self>) {
        let Some(action) = self.quick_actions.get(idx) else {
            return;
        };
        if !action.enabled {
            return;
        }
        // Real dispatch routes through the runtime's action engine as a pre-filled
        // SubAction (and, for picker actions, opens the scene/source picker first).
        // With no live runtime the dispatch is stubbed to a global toast.
        let label = action.label.clone();
        cx.push_toast(ToastKind::Info, format!("{label} — queued"));
    }

    fn dismiss_toast(&mut self, cx: &mut Context<Self>) {
        self.toast = None;
        cx.notify();
    }

    fn header_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (letter, brand) = hero_identity(self.icon.as_str(), &self.display_name, palette);

        let tile = div()
            .flex_none()
            .size(px(48.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(11.0))
            .bg(brand)
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(24.0))
                    .text_color(palette.shell)
                    .child(letter),
            );

        let mut name_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_LG)
                    .text_color(palette.text_primary)
                    .child(self.display_name.clone()),
            );
        if let Some(version) = &self.version {
            name_row = name_row.child(pill(version.clone(), palette.text_muted, palette));
        }
        if self.capability_flags.limited {
            let label = self
                .capability_flags
                .label
                .clone()
                .unwrap_or_else(|| "Limited".to_owned());
            name_row = name_row.child(pill(label.to_uppercase(), palette.warning, palette));
        }

        let sub = sub_line(self.endpoint.as_deref(), self.uptime);
        let info = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(name_row)
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_muted)
                    .child(sub),
            );

        let mut actions = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density));
        for (i, action) in self.header_actions.iter().enumerate() {
            actions = actions.child(self.action_button(i, action.clone(), palette, density, cx));
        }

        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Md, density))
            .py(spacing(Spacing::Md, density))
            .px(spacing(Spacing::Md, density))
            .rounded(radius(Radius::Lg))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.elevated)
            .child(tile)
            .child(info)
            .child(actions)
            .into_any_element()
    }

    fn action_button(
        &self,
        idx: usize,
        action: HeaderAction,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = header_action_label(&action);
        let text_color = match action {
            HeaderAction::Disconnect => palette.random,
            _ => palette.text_secondary,
        };
        let hover_bg = with_alpha(palette.border_regular, 0.06);
        div()
            .id(("header-action", idx))
            .flex()
            .items_center()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Md, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .cursor_pointer()
            .hover(move |s| s.bg(hover_bg))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.on_header_action(action.clone(), cx)
            }))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(text_color)
                    .child(label),
            )
            .into_any_element()
    }

    /// The quick-actions card: a bolt-led header over a divider and a row of up to
    /// four accent-tinted action buttons. Disabled actions dim and show an `N/A`
    /// trailing marker; enabled ones dispatch through [`Self::on_quick_action`].
    fn quick_actions_card(
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
            .px(spacing(Spacing::Md, density))
            .child(icon(Icon::Bolt, FONT_SM, palette.warning))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child("Quick actions"),
            );

        let divider = div().w_full().h(BORDER_THIN).bg(palette.border_regular);

        let mut btn_row = div()
            .w_full()
            .flex()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Md, density));
        let capped = self.quick_actions.len().min(4);
        for i in 0..capped {
            let action = &self.quick_actions[i];
            btn_row = btn_row.child(self.quick_action_button(
                i,
                action,
                quick_action_accent(i, palette),
                palette,
                density,
                cx,
            ));
        }
        for _ in capped..4 {
            btn_row = btn_row.child(div().flex_1());
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.elevated)
            .child(header)
            .child(divider)
            .child(btn_row)
            .into_any_element()
    }

    fn quick_action_button(
        &self,
        idx: usize,
        action: &QuickAction,
        accent: Rgba,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let enabled = action.enabled;
        let (icon_color, label_color, bg_color, border_color) = if enabled {
            (
                accent,
                palette.text_primary,
                palette.shell,
                palette.border_regular,
            )
        } else {
            (
                with_alpha(palette.text_faint, 0.5),
                with_alpha(palette.text_faint, 0.5),
                with_alpha(palette.shell, 0.5),
                with_alpha(palette.border_regular, 0.5),
            )
        };

        let mut content = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(icon(
                Icon::from_name(action.icon.as_str()),
                FONT_SM,
                icon_color,
            ))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(label_color)
                    .child(action.label.clone()),
            );
        if !enabled {
            content = content.child(div().flex_1()).child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(with_alpha(palette.text_faint, 0.5))
                    .child("N/A"),
            );
        }

        let mut btn = div()
            .id(("quick-action", idx))
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(border_color)
            .bg(bg_color)
            .child(content);
        if enabled {
            let hover_bg = with_alpha(bg_color, (bg_color.a + 0.06).min(1.0));
            btn = btn
                .cursor_pointer()
                .hover(move |s| s.bg(hover_bg))
                .on_click(
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.on_quick_action(idx, cx)),
                );
        }
        btn.into_any_element()
    }

    fn disconnect_overlay(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let card = confirm_modal(
            "Disconnect integration",
            "Chats and events from this integration stop until you reconnect.",
            ConfirmTone::Warning,
            palette,
        )
        .item_name(self.display_name.clone())
        .esc_hint("to cancel")
        .on_cancel(
            "integration-disconnect-cancel",
            "Cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_disconnect(cx)),
        )
        .on_confirm(
            "integration-disconnect-confirm",
            "Disconnect",
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_disconnect(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("integration-disconnect-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_disconnect(cx));
            })
    }

    /// The runtime-gated alt-state banner, selected purely from the integration's
    /// `ConnectionState` — no per-integration branch. A live `Connected` integration
    /// shows no banner; the transient and disconnected states each surface a strip
    /// above the detail (reconnecting / connecting-in-flight / not-connected), while
    /// the full detail frame stays visible beneath.
    fn state_banner(&self, palette: &ForgePalette, density: Density) -> Option<AnyElement> {
        let (accent, glyph, title, detail): (Rgba, Icon, &str, &str) = match self.connection {
            ConnectionState::Connected => return None,
            ConnectionState::Connecting => (
                palette.info,
                Icon::Refresh,
                "Connecting…",
                "Establishing a session with this integration.",
            ),
            ConnectionState::Reconnecting => (
                palette.warning,
                Icon::Refresh,
                "Reconnecting…",
                "The session dropped; forge is re-establishing it.",
            ),
            ConnectionState::Disconnected => (
                palette.text_muted,
                Icon::PlugConnected,
                "Not connected",
                "Use Reconnect above to link this integration.",
            ),
        };

        let text_col = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(title.to_owned()),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(detail.to_owned()),
            );

        Some(
            div()
                .w_full()
                .flex()
                .items_start()
                .gap(spacing(Spacing::Sm, density))
                .py(spacing(Spacing::Sm, density))
                .px(spacing(Spacing::Md, density))
                .rounded(radius(Radius::Md))
                .border(BORDER_THIN)
                .border_color(accent)
                .bg(palette.elevated)
                .child(icon(glyph, FONT_SM, accent))
                .child(text_col)
                .into_any_element(),
        )
    }

    fn toast_banner(
        &self,
        message: String,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .absolute()
            .right(spacing(Spacing::Md, density))
            .bottom(spacing(Spacing::Md, density))
            .id("integration-toast")
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Md, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.success)
            .bg(palette.elevated)
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.dismiss_toast(cx)))
            .child(icon(Icon::CircleCheck, FONT_SM, palette.success))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(message),
            )
            .into_any_element()
    }
}

impl Render for IntegrationDetail {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let header_card = self.header_card(&palette, density, cx);
        let reconnecting = matches!(
            self.connection,
            ConnectionState::Connecting | ConnectionState::Reconnecting
        );
        let state_banner = self.state_banner(&palette, density);
        let health = health_grid(&self.health_metrics, reconnecting, &palette, density);
        let content = content_sections(&self.sections, &palette, density);
        let quick = self.quick_actions_card(&palette, density, cx);

        let body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .children(state_banner)
            .child(header_card)
            .child(health)
            .child(content)
            .child(quick);

        let crumbs = breadcrumb(
            vec![
                BreadcrumbCrumb::link(
                    "Platforms",
                    "integration-crumb-platforms",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.go_back(cx)),
                ),
                BreadcrumbCrumb::leaf(self.display_name.clone()),
            ],
            &palette,
        );

        let scroll = div()
            .id("integration-detail-scroll")
            .flex_1()
            .overflow_y_scroll()
            .bg(palette.base)
            .child(
                div()
                    .w_full()
                    .py(spacing(Spacing::Md, density))
                    .px(spacing(Spacing::Lg, density))
                    .child(body),
            );

        let disconnect_overlay = self
            .pending_disconnect
            .then(|| self.disconnect_overlay(&palette, cx));
        let toast = self
            .toast
            .clone()
            .map(|m| self.toast_banner(m, &palette, density, cx));

        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(crumbs)
            .child(scroll)
            .children(disconnect_overlay)
            .children(toast)
    }
}

/// Resolves the hero identity (initial letter + brand hue) from the seed icon
/// token, falling back to the display name's first letter on an unknown token.
fn hero_identity(icon_str: &str, display_name: &str, palette: &ForgePalette) -> (String, Rgba) {
    match icon_str {
        "brand-twitch" => ("T".to_owned(), palette.brand),
        "brand-youtube" => ("Y".to_owned(), palette.random),
        "brand-kick" => ("K".to_owned(), palette.info),
        "broadcast" => ("O".to_owned(), palette.success),
        "mood-smile" => ("V".to_owned(), palette.warning),
        "brand-discord" => ("D".to_owned(), palette.brand),
        "piano" => ("M".to_owned(), palette.accent_teal),
        "keyboard" => ("H".to_owned(), palette.info),
        _ => {
            let initial = display_name
                .chars()
                .next()
                .map(|c| c.to_ascii_uppercase().to_string())
                .unwrap_or_else(|| "?".to_owned());
            (initial, palette.brand)
        }
    }
}

/// The lifecycle verb a control dispatch runs, routing the three `BuiltinControl`
/// methods through the single spawn site in [`IntegrationDetail::dispatch_control`].
enum ControlVerb {
    Reconnect,
    Disconnect,
    RefreshToken,
}

fn header_action_label(action: &HeaderAction) -> &'static str {
    match action {
        HeaderAction::Reconnect => "Reconnect",
        HeaderAction::RefreshToken => "Refresh token",
        HeaderAction::Disconnect => "Disconnect",
        HeaderAction::Settings => "Settings",
    }
}

fn quick_action_accent(index: usize, palette: &ForgePalette) -> Rgba {
    match index % 4 {
        0 => palette.brand,
        1 => palette.random,
        2 => palette.warning,
        _ => palette.info,
    }
}

/// A rounded `surface_overlay` pill inking a monospace caption — the header's
/// version tag and the limited-capability badge share this shape.
fn pill(label: String, text_color: Rgba, palette: &ForgePalette) -> impl IntoElement {
    div()
        .flex_none()
        .px(px(6.0))
        .rounded(radius(Radius::Md))
        .bg(palette.surface_overlay)
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XS)
                .text_color(text_color)
                .child(label),
        )
}

fn sub_line(endpoint: Option<&str>, uptime: Option<Duration>) -> String {
    match (endpoint, uptime) {
        (Some(ep), Some(d)) => format!("{ep} \u{00b7} up {}", format_uptime(d)),
        (Some(ep), None) => ep.to_owned(),
        (None, Some(d)) => format!("up {}", format_uptime(d)),
        (None, None) => String::new(),
    }
}
