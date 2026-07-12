use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, ConfirmTone, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density,
    FONT_LG, FONT_SM, FONT_XS, ForgePalette, Icon, OverlayPosition, Radius, Spacing, breadcrumb,
    confirm_modal, icon, overlay, radius, spacing, with_alpha,
};
use forge_platform_core::{
    BuiltinId, CapabilityFlags, ConnectionState, DetailSection, HeaderAction, HealthMetric,
    QuickAction, SectionIcon,
};
use gpui::{
    AnyElement, ClickEvent, Context, EventEmitter, FontWeight, Rgba, Window, div, prelude::*, px,
};
use std::time::Duration;

use crate::builtin_sections::{content_sections, format_uptime, health_grid};
use crate::integration_seed::seed;
use crate::presentation::ActivePresentation;
use crate::screen::Screen;
use crate::sidebar::NavRequested;

/// The single generic integration detail screen. It consumes the four `Builtin*`
/// trait outputs — status, health metrics, content sections, quick actions — and
/// renders them uniformly, so no integration has any per-screen detail code: a
/// new integration reaches this view by supplying the four traits, nothing here
/// changes. The view holds a cached snapshot of those outputs (a runtime bridge
/// will refresh them); it never switches on the integration id when rendering.
pub struct IntegrationDetail {
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
}

impl EventEmitter<NavRequested> for IntegrationDetail {}

impl IntegrationDetail {
    pub fn new(id: BuiltinId, _cx: &mut Context<Self>) -> Self {
        let s = seed(&id);
        // Consume the four traits into cached snapshot fields, exactly as a live
        // integration's traits would be read once at attach.
        let status = &s.status;
        Self {
            icon: s.icon.clone(),
            display_name: status.display_name().to_owned(),
            version: status.version().map(ToOwned::to_owned),
            endpoint: status.endpoint().map(ToOwned::to_owned),
            uptime: status.uptime(),
            connection: status.connection(),
            capability_flags: status.capability_flags(),
            header_actions: status.header_actions(),
            health_metrics: s.health.metrics(),
            sections: s.content.sections(),
            quick_actions: s.quick.actions(),
            pending_disconnect: false,
            toast: None,
        }
    }

    fn go_back(&mut self, cx: &mut Context<Self>) {
        cx.emit(NavRequested(Screen::Platforms));
    }

    fn on_header_action(&mut self, action: HeaderAction, cx: &mut Context<Self>) {
        match action {
            HeaderAction::Disconnect => {
                self.pending_disconnect = true;
            }
            HeaderAction::Reconnect => {
                self.toast = Some("Reconnect requested".to_owned());
            }
            HeaderAction::RefreshToken => {
                self.toast = Some("Token refresh requested".to_owned());
            }
            HeaderAction::Settings => {
                self.toast = Some("Settings coming soon".to_owned());
            }
        }
        cx.notify();
    }

    fn cancel_disconnect(&mut self, cx: &mut Context<Self>) {
        self.pending_disconnect = false;
        cx.notify();
    }

    fn confirm_disconnect(&mut self, cx: &mut Context<Self>) {
        self.pending_disconnect = false;
        self.toast = Some(format!("Disconnecting {}", self.display_name));
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
        // With no live runtime the dispatch is stubbed to a feedback toast.
        self.toast = Some(format!("{} — queued", action.label));
        cx.notify();
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
        let health = health_grid(&self.health_metrics, reconnecting, &palette, density);
        let content = content_sections(&self.sections, &palette, density);
        let quick = self.quick_actions_card(&palette, density, cx);

        let body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
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
