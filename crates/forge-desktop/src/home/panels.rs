use super::*;

const BAR_H: gpui::Pixels = px(40.0);
const ROW_GAP: gpui::Pixels = px(3.0);

impl HomeView {
    pub(super) fn health_stat(
        label: String,
        value: String,
        unit: Option<String>,
        value_color: Rgba,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        let mut value_row = div()
            .flex()
            .items_end()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_MD)
                    .text_color(value_color)
                    .child(value),
            );
        if let Some(unit) = unit {
            value_row = value_row.child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(unit),
            );
        }

        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(scaled(ROW_GAP, density))
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(label),
            )
            .child(value_row)
            .into_any_element()
    }

    pub(super) fn render_stream_health(
        &self,
        health: ObsHealth,
        palette: &ForgePalette,
        density: Density,
    ) -> impl IntoElement + use<> {
        let (badge_ink, badge_label) = match &health.live_for {
            Some(live_for) => (palette.success, live_for.clone()),
            None => (palette.text_faint, tr!("home_health_offline")),
        };
        let status_badge = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(if health.live_for.is_some() {
                pulse_dot("home-health-live-pulse", badge_ink, HEALTH_LIVE_DOT).into_any_element()
            } else {
                status_dot(badge_ink, HEALTH_LIVE_DOT).into_any_element()
            })
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(badge_ink)
                    .child(badge_label),
            );

        let header_left = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(icon(Icon::ChartLine, HEALTH_ICON, palette.success))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(tr!("home_health_title")),
            )
            .child(status_badge);

        let header = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(header_left)
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(tr!("home_health_refresh_hint")),
            );

        let throughput = div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(tr!("home_health_throughput_label")),
            )
            .child(
                div()
                    .w_full()
                    .h(BAR_H)
                    .child(sparkline(&health.throughput, palette.brand)),
            );

        let dropped_color = if health.dropped_ok {
            palette.success
        } else {
            palette.warning
        };
        let bitrate_unit = health.bitrate_known.then(|| "kbps".to_owned());

        let stats_row = div()
            .w_full()
            .flex()
            .items_end()
            .gap(spacing(Spacing::Sm, density))
            .child(throughput)
            .child(Self::health_stat(
                tr!("home_health_bitrate_label"),
                health.bitrate,
                bitrate_unit,
                palette.text_primary,
                palette,
                density,
            ))
            .child(Self::health_stat(
                tr!("home_health_dropped_label"),
                health.dropped,
                health.dropped_pct,
                dropped_color,
                palette,
                density,
            ))
            .child(Self::health_stat(
                tr!("home_health_fps_label"),
                health.fps,
                None,
                palette.text_primary,
                palette,
                density,
            ))
            .child(Self::health_stat(
                tr!("home_health_cpu_label"),
                health.cpu,
                Some("%".to_owned()),
                palette.text_primary,
                palette,
                density,
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

    pub(super) fn connection_cell(
        &self,
        integ: Integration,
        connected: bool,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (status_text, status_color): (String, Rgba) = if connected {
            (tr!("home_conn_connected"), palette.success)
        } else {
            (tr!("home_conn_offline"), palette.text_faint)
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
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(integ.label()),
            )
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(status_color)
                    .child(status_text),
            );

        let hover_bg = palette.shell;
        let target = integ.screen();
        let key = HomeRow::Connection(integ);
        let cell_bg = if self.row_keys.is_at(&key) {
            hover_bg
        } else {
            palette.elevated
        };
        let cell = div()
            .id(("home-conn-cell", integ as usize))
            .relative()
            .flex_1()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(scaled(CONN_CELL_PAD_V, density))
            .px(scaled(CONN_CELL_PAD_H, density))
            .bg(cell_bg)
            .cursor_pointer()
            .hover(move |s| s.bg(hover_bg))
            .key_activated(palette.text_primary)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.go(target.clone(), cx)))
            .child(brand_dot)
            .child(label_col)
            .child(status_dot(dot_color, CONN_STATUS_DOT));
        self.with_row_anchor(&key, cell).into_any_element()
    }

    pub(super) fn render_connections(
        &self,
        connections: Vec<(Integration, bool)>,
        connected: usize,
        total: usize,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let disconnected = total.saturating_sub(connected);
        let summary = tr!(
            "home_connections_summary",
            active = connected,
            disconnected = disconnected
        );

        let header = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(icon(Icon::PlugConnected, CONN_HEADER_ICON, palette.success))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(tr!("home_connections_title")),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(summary),
            );

        let header_bar = card(header, palette)
            .full_width()
            .padding_xy(CONN_HEADER_PAD_V, scaled(CONN_HEADER_PAD_H, density))
            .split_radius(radius(Radius::Md), px(0.0));

        let mut cells = div().w_full().flex().gap(CELL_GAP);
        for (integ, ok) in connections {
            cells = cells.child(self.connection_cell(integ, ok, palette, density, cx));
        }

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

    pub(super) fn event_row(
        &self,
        ev: HomeEvent,
        has_border: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let hue = ev.hue.color(palette);

        let spans = div()
            .flex_1()
            .flex()
            .items_center()
            .overflow_hidden()
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(hue)
                    .child(ev.source.clone()),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(": "),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.brand)
                    .child(ev.name.clone()),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(" - "),
            )
            .child(
                div()
                    .flex_1()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(ev.desc.clone()),
            );

        let inner = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(scaled(EVENT_ROW_PAD_V, density))
            .px(spacing(Spacing::Xxs, density))
            .child(status_dot(hue, EVENT_DOT))
            .child(
                div()
                    .flex_none()
                    .w(event_time_width())
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(fmt_display(DateShape::TimeWithSeconds, ev.at)),
            )
            .child(spans);

        let hover_bg = palette.shell;
        let mut row = div()
            .id((gpui::ElementId::from("home-event"), ev.id.clone()))
            .relative()
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
        let key = HomeRow::Event(ev.id.clone());
        if self.row_keys.is_at(&key) {
            row = row.bg(hover_bg);
        }
        self.with_row_anchor(&key, row).into_any_element()
    }

    pub(super) fn render_recent_events(
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
            .child(pulse_dot(
                "home-events-live-pulse",
                palette.success,
                EVENT_LIVE_DOT,
            ))
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(tr!("home_health_live")),
            );

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
                    .child(icon(Icon::Activity, EVENT_HEADER_ICON, palette.info))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_SM)
                            .text_color(palette.text_primary)
                            .child(tr!("home_events_title")),
                    ),
            )
            .child(live_label);

        let body: AnyElement = if recent.is_empty() {
            empty_state(tr!("home_events_empty"), palette)
                .density(density)
                .into_any_element()
        } else {
            let count = recent.len();
            let mut list = div().flex().flex_col();
            for (i, ev) in recent.into_iter().enumerate() {
                list = list.child(self.event_row(ev, i + 1 < count, palette, cx));
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
            .full_height()
            .padding(spacing(Spacing::Sm, density))
    }

    pub(super) fn glance_row(
        label: String,
        value: String,
        value_color: Rgba,
        last: bool,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        let row = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .py(scaled(GLANCE_ROW_PAD_V, density))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(label),
            )
            .child(
                div()
                    .font_family(mono_family())
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

    pub(super) fn render_glance(
        &self,
        actions: String,
        commands: String,
        fired: String,
        globals: String,
        palette: &ForgePalette,
        density: Density,
    ) -> impl IntoElement + use<> {
        let rows = div()
            .flex()
            .flex_col()
            .child(Self::glance_row(
                tr!("home_glance_actions"),
                actions,
                palette.brand,
                false,
                palette,
                density,
            ))
            .child(Self::glance_row(
                tr!("home_glance_commands"),
                commands,
                palette.info,
                false,
                palette,
                density,
            ))
            .child(Self::glance_row(
                tr!("home_glance_fired"),
                fired,
                palette.success,
                false,
                palette,
                density,
            ))
            .child(Self::glance_row(
                tr!("home_glance_globals"),
                globals,
                palette.warning,
                true,
                palette,
                density,
            ));

        let content = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(icon(Icon::LayoutGrid, GLANCE_HEADER_ICON, palette.brand))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_SM)
                            .text_color(palette.text_primary)
                            .child(tr!("home_glance_title")),
                    ),
            )
            .child(rows);

        card(content, palette)
            .full_width()
            .full_height()
            .padding(spacing(Spacing::Sm, density))
    }
}
