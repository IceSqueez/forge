use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_LG,
    FONT_MD, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon, Radius, Spacing, breadcrumb, card,
    ghost_button_with_icon, icon, radius, spacing, sparkline, status_dot,
};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, FontWeight, Pixels, Rgba, Subscription,
    Window, div, prelude::*, px,
};

use crate::home_stats::{HomeEvent, HomeStats, Integration, ObsHealth};
use crate::presentation::ActivePresentation;
use crate::screen::Screen;
use crate::sidebar::NavRequested;

// Off-scale layout literals. These sit deliberately off the density-scaled
// `Spacing` steps: the Home hub is a fixed-rhythm dashboard whose paddings, tile
// sizes and glyph sizes are hand-tuned to the design, so — mirroring the kit
// card/sidebar precedent — they are carried as literals rather than snapped to the
// nearest token, which would alter the composition.
const BODY_PAD_V: Pixels = px(22.0);
const BODY_PAD_H: Pixels = px(28.0);

const HERO_PAD_V: Pixels = px(22.0);
const HERO_PAD_H: Pixels = px(24.0);
const HERO_BRAND_BOX: Pixels = px(54.0);
const HERO_BRAND_FONT: Pixels = px(26.0);
const HERO_TITLE_FONT: Pixels = px(22.0);

const JUMP_PAD_V: Pixels = px(16.0);
const JUMP_PAD_H: Pixels = px(18.0);
const JUMP_ICON_BOX: Pixels = px(34.0);
const JUMP_ICON_RADIUS: Pixels = px(8.0);
const JUMP_ICON_GLYPH: Pixels = FONT_LG;
const JUMP_STAT_FONT: Pixels = px(24.0);
const JUMP_HINT_ARROW: Pixels = px(12.0);
const JUMP_WARN_ICON: Pixels = px(14.0);

const CONN_HEADER_PAD_V: Pixels = px(10.0);
const CONN_HEADER_PAD_H: Pixels = px(14.0);
const CONN_HEADER_ICON: Pixels = px(14.0);
const CONN_CELL_PAD_V: Pixels = px(12.0);
const CONN_CELL_PAD_H: Pixels = px(14.0);
const CONN_BRAND_DOT: Pixels = px(10.0);
const CONN_BRAND_DOT_RADIUS: Pixels = px(3.0);
const CONN_STATUS_DOT: Pixels = px(8.0);
const CELL_GAP: Pixels = px(1.0);

const HEALTH_ICON: Pixels = px(14.0);
const HEALTH_LIVE_DOT: Pixels = px(6.0);

const EVENT_ROW_PAD_V: Pixels = px(7.0);
const EVENT_ROW_PAD_H: Pixels = px(4.0);
const EVENT_TIME_W: Pixels = px(60.0);
const EVENT_DOT: Pixels = px(6.0);
const EVENT_LIVE_DOT: Pixels = px(6.0);

const GLANCE_ROW_PAD_V: Pixels = px(5.0);
const DIVIDER_H: Pixels = px(1.0);
const GLANCE_CARD_W: Pixels = px(340.0);

/// The Home hub screen view-entity: a breadcrumb header over a scrollable dashboard
/// — a brand hero with import / new-action affordances, three IA jump cards
/// (Audience → Chat, Automation → Actions, Connections → platforms), an optional
/// OBS stream-health card, the integration connections strip, and a recent-events +
/// at-a-glance footer pair.
///
/// It owns no dashboard data: every stat is read from an injected [`HomeStats`]
/// topic (a cached runtime read, never the source of truth) and the view repaints
/// when that topic notifies. Navigation is voiced as [`NavRequested`] events the
/// root shell subscribes to — the screen never mutates the router itself.
pub struct HomeView {
    stats: Entity<HomeStats>,
    _stats_obs: Subscription,
}

impl HomeView {
    pub fn new(stats: Entity<HomeStats>, cx: &mut Context<Self>) -> Self {
        let stats_obs = cx.observe(&stats, |_, _, cx| cx.notify());
        Self {
            stats,
            _stats_obs: stats_obs,
        }
    }

    /// Voices a navigation intent to the root shell. No `self` state changes, so no
    /// `cx.notify()` — the shell owns the routing side effect.
    fn go(&mut self, screen: Screen, cx: &mut Context<Self>) {
        cx.emit(NavRequested(screen));
    }

    // --- hero -----------------------------------------------------------------

    fn render_hero(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let brand_box = div()
            .flex_none()
            .size(HERO_BRAND_BOX)
            .flex()
            .items_center()
            .justify_center()
            .rounded(radius(Radius::Lg))
            .bg(palette.brand)
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(HERO_BRAND_FONT)
                    .text_color(palette.shell)
                    .child("F"),
            );

        let title_col = div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(HERO_TITLE_FONT)
                    .text_color(palette.text_primary)
                    .child("Forge"),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_muted)
                    .child("Open-source stream automation, forged for streamers"),
            );

        // Import opens the action-import flow once the storage capability reaches
        // this screen — a no-op placeholder in this slice. New action routes to the
        // Automation screen, standing in for the dedicated action editor until it
        // lands.
        let import_btn = ghost_button_with_icon(Icon::Download, "Import", palette)
            .density(density)
            .on_click("home-import", |_, _, _| {});
        let new_action_btn = ghost_button_with_icon(Icon::Plus, "New action", palette)
            .density(density)
            .on_click(
                "home-new-action",
                cx.listener(|this, _: &ClickEvent, _, cx| this.go(Screen::Actions, cx)),
            );

        let buttons = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(import_btn)
            .child(new_action_btn);

        let inner = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Md, density))
            .child(brand_box)
            .child(title_col)
            .child(buttons);

        card(inner, palette)
            .full_width()
            .padding_xy(HERO_PAD_V, HERO_PAD_H)
            .radius(Radius::Lg)
    }

    // --- jump cards -----------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    fn jump_card(
        &self,
        id: &'static str,
        glyph: Icon,
        glyph_color: Rgba,
        section: &'static str,
        title: &'static str,
        stat: String,
        stat_label: String,
        hint: &'static str,
        warn: bool,
        target: Screen,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let icon_box = div()
            .flex_none()
            .size(JUMP_ICON_BOX)
            .flex()
            .items_center()
            .justify_center()
            .rounded(JUMP_ICON_RADIUS)
            .bg(palette.surface_overlay)
            .child(icon(glyph, JUMP_ICON_GLYPH, glyph_color));

        let label_col = div()
            .flex_1()
            .flex()
            .flex_col()
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(section),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(title),
            );

        let mut head = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(icon_box)
            .child(label_col);
        if warn {
            head = head.child(icon(Icon::AlertTriangle, JUMP_WARN_ICON, palette.warning));
        }

        let stat_row = div()
            .flex()
            .items_end()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(JUMP_STAT_FONT)
                    .text_color(glyph_color)
                    .child(stat),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(stat_label),
            );

        let hint_row = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(hint),
            )
            .child(icon(Icon::ArrowRight, JUMP_HINT_ARROW, palette.text_faint));

        let border = palette.border_regular;
        let hover_border = palette.border_input;
        div()
            .id(id)
            .flex()
            .flex_1()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .py(JUMP_PAD_V)
            .px(JUMP_PAD_H)
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(border)
            .bg(palette.elevated)
            .cursor_pointer()
            .hover(move |s| s.border_color(hover_border))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.go(target.clone(), cx)))
            .child(head)
            .child(stat_row)
            .child(hint_row)
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_jump_cards(
        &self,
        viewers: String,
        actions: String,
        fired: String,
        connected: usize,
        total: usize,
        warn: bool,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let chat = self.jump_card(
            "home-jump-chat",
            Icon::MessageCircle,
            palette.brand,
            "AUDIENCE",
            "Chat",
            viewers,
            "viewers now".to_owned(),
            "Talk to your audience and see who's watching",
            false,
            Screen::Chat,
            palette,
            density,
            cx,
        );
        let automation = self.jump_card(
            "home-jump-actions",
            Icon::Bolt,
            palette.warning,
            "AUTOMATION",
            "Actions",
            actions,
            format!("actions · {fired} fired today"),
            "Set up triggers, commands and timers",
            false,
            Screen::Actions,
            palette,
            density,
            cx,
        );
        let connections = self.jump_card(
            "home-jump-connections",
            Icon::Plug,
            palette.success,
            "CONNECTIONS",
            "Connections",
            format!("{connected}/{total}"),
            "connected".to_owned(),
            "Manage platforms, apps and modules",
            warn,
            Screen::Platforms,
            palette,
            density,
            cx,
        );

        div()
            .w_full()
            .flex()
            .gap(spacing(Spacing::Xs, density))
            .child(chat)
            .child(automation)
            .child(connections)
    }

    // --- stream health --------------------------------------------------------

    fn health_stat(
        label: &'static str,
        value: String,
        unit: Option<&'static str>,
        value_color: Rgba,
        palette: &ForgePalette,
    ) -> AnyElement {
        let mut value_row = div().flex().items_end().gap(px(4.0)).child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_MD)
                .text_color(value_color)
                .child(value),
        );
        if let Some(unit) = unit {
            value_row = value_row.child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(unit),
            );
        }

        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(3.0))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(label),
            )
            .child(value_row)
            .into_any_element()
    }

    fn render_stream_health(
        &self,
        health: ObsHealth,
        palette: &ForgePalette,
        density: Density,
    ) -> impl IntoElement + use<> {
        let live_badge = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(status_dot(palette.success, HEALTH_LIVE_DOT))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.success)
                    .child("LIVE"),
            );

        let header_left = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(icon(Icon::ChartLine, HEALTH_ICON, palette.success))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child("Stream health"),
            )
            .child(live_badge);

        let header = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(header_left)
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child("last 60s · auto-refresh"),
            );

        let throughput = div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child("THROUGHPUT · ev/s"),
            )
            .child(
                div()
                    .w_full()
                    .h(px(40.0))
                    .child(sparkline(&health.throughput, palette.brand)),
            );

        let dropped_color = if health.dropped_ok {
            palette.success
        } else {
            palette.warning
        };
        let dropped_value = match health.dropped_pct {
            Some(pct) => format!("{} {}", health.dropped, pct),
            None => health.dropped.to_string(),
        };

        let stats_row = div()
            .w_full()
            .flex()
            .items_end()
            .gap(spacing(Spacing::Sm, density))
            .child(throughput)
            .child(Self::health_stat(
                "BITRATE · OBS",
                health.bitrate.to_string(),
                Some("kbps"),
                palette.text_primary,
                palette,
            ))
            .child(Self::health_stat(
                "DROPPED · OBS",
                dropped_value,
                None,
                dropped_color,
                palette,
            ))
            .child(Self::health_stat(
                "FPS",
                health.fps.to_string(),
                None,
                palette.text_primary,
                palette,
            ))
            .child(Self::health_stat(
                "CPU",
                health.cpu.to_string(),
                Some("%"),
                palette.text_primary,
                palette,
            ));

        let content = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(header)
            .child(stats_row);

        card(content, palette)
            .full_width()
            .padding(spacing(Spacing::Sm, density))
    }

    // --- connections strip ----------------------------------------------------

    fn connection_cell(
        &self,
        integ: Integration,
        connected: bool,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (status_text, status_color) = if connected {
            ("connected", palette.success)
        } else {
            ("offline", palette.text_faint)
        };
        let dot_color = if connected {
            palette.success
        } else {
            palette.text_extreme_faint
        };

        let brand_dot = div()
            .flex_none()
            .size(CONN_BRAND_DOT)
            .rounded(CONN_BRAND_DOT_RADIUS)
            .bg(integ.dot_color(palette));

        let label_col = div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(integ.label()),
            )
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(status_color)
                    .child(status_text),
            );

        let hover_bg = palette.shell;
        let target = integ.screen();
        div()
            .id(("home-conn-cell", integ as usize))
            .flex_1()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(CONN_CELL_PAD_V)
            .px(CONN_CELL_PAD_H)
            .bg(palette.elevated)
            .cursor_pointer()
            .hover(move |s| s.bg(hover_bg))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.go(target.clone(), cx)))
            .child(brand_dot)
            .child(label_col)
            .child(status_dot(dot_color, CONN_STATUS_DOT))
            .into_any_element()
    }

    fn render_connections(
        &self,
        connections: Vec<(Integration, bool)>,
        connected: usize,
        total: usize,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let disconnected = total.saturating_sub(connected);
        let summary = format!("{connected} active · {disconnected} disconnected");

        let header = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(icon(Icon::PlugConnected, CONN_HEADER_ICON, palette.success))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child("Integrations"),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(summary),
            );

        let header_bar = card(header, palette)
            .full_width()
            .padding_xy(CONN_HEADER_PAD_V, CONN_HEADER_PAD_H)
            .split_radius(radius(Radius::Md), px(0.0));

        // Six-column rhythm: the five integration cells plus one empty trailing slot,
        // 1px surface-overlay hairlines showing through the gaps.
        let mut cells = div().w_full().flex().gap(CELL_GAP);
        for (integ, ok) in connections {
            cells = cells.child(self.connection_cell(integ, ok, palette, density, cx));
        }
        cells = cells.child(div().flex_1().bg(palette.elevated));

        let cells_card = card(cells, palette)
            .full_width()
            .padding(px(0.0))
            .background(palette.surface_overlay)
            .split_radius(px(0.0), radius(Radius::Md));

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(header_bar)
            .child(cells_card)
    }

    // --- recent events + at a glance -----------------------------------------

    fn event_row(
        &self,
        idx: usize,
        ev: HomeEvent,
        has_border: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hue = ev.hue.color(palette);

        let spans = div()
            .flex_1()
            .flex()
            .items_center()
            .overflow_hidden()
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(hue)
                    .child(ev.source.clone()),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(": "),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.brand)
                    .child(ev.name.clone()),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(" \u{2014} "),
            )
            .child(
                div()
                    .flex_1()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(ev.desc.clone()),
            );

        let inner = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .py(EVENT_ROW_PAD_V)
            .px(EVENT_ROW_PAD_H)
            .child(status_dot(hue, EVENT_DOT))
            .child(
                div()
                    .flex_none()
                    .w(EVENT_TIME_W)
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(ev.time.clone()),
            )
            .child(spans);

        let hover_bg = palette.shell;
        let mut row = div()
            .id(("home-event", idx))
            .w_full()
            .rounded(radius(Radius::Sm))
            .cursor_pointer()
            .hover(move |s| s.bg(hover_bg))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.go(Screen::EventFeed, cx)))
            .child(inner);
        if has_border {
            row = row
                .border_b(BORDER_THIN)
                .border_color(palette.border_regular);
        }
        row.into_any_element()
    }

    fn render_recent_events(
        &self,
        recent: Vec<HomeEvent>,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let live_label = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(status_dot(palette.success, EVENT_LIVE_DOT))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child("LIVE"),
            );

        let header = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child("Recent events"),
            )
            .child(live_label);

        let body: AnyElement = if recent.is_empty() {
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child("No events yet")
                .into_any_element()
        } else {
            let count = recent.len();
            let mut list = div().flex().flex_col();
            for (i, ev) in recent.into_iter().enumerate() {
                list = list.child(self.event_row(i, ev, i + 1 < count, palette, cx));
            }
            list.into_any_element()
        };

        let content = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(header)
            .child(body);

        card(content, palette)
            .full_width()
            .padding(spacing(Spacing::Sm, density))
    }

    fn glance_row(
        label: &'static str,
        value: String,
        value_color: Rgba,
        last: bool,
        palette: &ForgePalette,
    ) -> AnyElement {
        let row = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .py(GLANCE_ROW_PAD_V)
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(label),
            )
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(value_color)
                    .child(value),
            );
        if last {
            row.into_any_element()
        } else {
            div()
                .w_full()
                .flex()
                .flex_col()
                .child(row)
                .child(div().w_full().h(DIVIDER_H).bg(palette.border_regular))
                .into_any_element()
        }
    }

    fn render_glance(
        &self,
        actions: String,
        fired: String,
        globals: String,
        palette: &ForgePalette,
        density: Density,
    ) -> impl IntoElement + use<> {
        let rows = div()
            .flex()
            .flex_col()
            .child(Self::glance_row(
                "Actions",
                actions,
                palette.brand,
                false,
                palette,
            ))
            .child(Self::glance_row(
                "Fired this session",
                fired,
                palette.success,
                false,
                palette,
            ))
            .child(Self::glance_row(
                "Globals",
                globals,
                palette.warning,
                true,
                palette,
            ));

        let content = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child("At a glance"),
            )
            .child(rows);

        card(content, palette)
            .full_width()
            .padding(spacing(Spacing::Sm, density))
    }
}

impl EventEmitter<NavRequested> for HomeView {}

impl Render for HomeView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        // Snapshot every readout the tree needs up front, ending the immutable borrow
        // on the topic before the per-element `cx.listener` closures are built.
        let stats = self.stats.read(cx);
        let viewers = stats.viewers_display();
        let actions = stats.actions_display();
        let fired = stats.triggers_fired_display();
        let globals = stats.globals_display();
        let connected = stats.connected_count();
        let total = stats.total_count();
        let warn = stats.connections_warn();
        let connections = stats.connections_snapshot();
        let recent = stats.recent(5);
        let obs_health = stats.obs_health_snapshot();

        let header = breadcrumb(vec![BreadcrumbCrumb::leaf("Home")], &palette);

        let hero = self.render_hero(&palette, density, cx);
        let jump_cards = self.render_jump_cards(
            viewers, actions, fired, connected, total, warn, &palette, density, cx,
        );
        let connections_strip =
            self.render_connections(connections, connected, total, &palette, density, cx);
        let recent_card = self.render_recent_events(recent, &palette, density, cx);
        let glance_card = self.render_glance(
            self.stats.read(cx).actions_display(),
            self.stats.read(cx).triggers_fired_display(),
            globals,
            &palette,
            density,
        );

        let bottom = div()
            .w_full()
            .flex()
            .items_start()
            .gap(spacing(Spacing::Sm, density))
            .child(div().flex_1().child(recent_card))
            .child(div().flex_none().w(GLANCE_CARD_W).child(glance_card));

        let mut content = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(hero)
            .child(jump_cards);

        if let Some(health) = obs_health {
            content = content.child(self.render_stream_health(health, &palette, density));
        }

        content = content.child(connections_strip).child(bottom);

        let body = div()
            .id("home-scroll")
            .flex_1()
            .overflow_y_scroll()
            .bg(palette.base)
            .child(
                div()
                    .w_full()
                    .pt(BODY_PAD_V)
                    .pb(BODY_PAD_V)
                    .px(BODY_PAD_H)
                    .child(content),
            );

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(header)
            .child(body)
    }
}
