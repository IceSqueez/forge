use super::{HEADER_GLYPH, RECENT_CLIENT_WINDOW_SECONDS, ServerConsoleView};
use forge_components::{
    BORDER_THIN, Density, FONT_SM, FONT_XXS, ForgePalette, Icon, Radius, Spacing, body_family,
    card, fmt_bytes, fmt_number, header_status, icon, metric_card, mono_family, scaled, spacing,
    sparkline, status_dot, tr,
};
use forge_server::extract_port;
use gpui::{AnyElement, FontWeight, Pixels, Rgba, SharedString, div, prelude::*, px};

const EVENT_RATE_WINDOW_SECONDS: i64 = 10;
const RECENT_CLIENT_WINDOW_MINUTES: i64 = RECENT_CLIENT_WINDOW_SECONDS / 60;
const FOOTER_PAD_V: Pixels = px(7.0);
const FOOTER_PAD_H: Pixels = px(14.0);
const FOOTER_DOT: Pixels = px(6.0);
const SPARK_HEIGHT: Pixels = px(60.0);
const SPARK_FILL_TOP_ALPHA: f32 = 0.4;
const SPARK_STROKE_WIDTH: f32 = 1.0;

fn footer_cell(text: impl Into<SharedString>, color: Rgba) -> impl IntoElement {
    div()
        .font_family(mono_family())
        .text_size(FONT_XXS)
        .text_color(color)
        .child(text.into())
}

impl ServerConsoleView {
    pub(super) fn stats_grid(&self, palette: &ForgePalette, density: Density) -> AnyElement {
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
                        count = i64::try_from(self.recent_clients).unwrap_or(i64::MAX),
                        minutes = RECENT_CLIENT_WINDOW_MINUTES
                    )),
                    recent_color,
                    palette,
                    density,
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
                    density,
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
                    density,
                )
                .into_any_element(),
            ))
            .child(cell(
                metric_card(
                    tr!("server_stat_bandwidth"),
                    tr!(
                        "server_stat_bandwidth_value",
                        rate = fmt_number(self.stats.bandwidth_kilobytes_per_second as f64, 0)
                    ),
                    Some(tr!("server_stat_bandwidth_sub")),
                    None,
                    palette,
                    density,
                )
                .into_any_element(),
            ))
            .into_any_element()
    }

    pub(super) fn throughput_card(&self, palette: &ForgePalette, density: Density) -> AnyElement {
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
                        seconds = i64::try_from(sample_count).unwrap_or(i64::MAX),
                        max = fmt_number(max as f64, 0)
                    )),
            );

        let chart = div().w_full().h(SPARK_HEIGHT).child(
            sparkline(&self.throughput_samples, palette.brand)
                .zero_baseline()
                .fading_fill(SPARK_FILL_TOP_ALPHA)
                .stroke_width(SPARK_STROKE_WIDTH),
        );

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

    pub(super) fn footer_bar(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let (health_label, health_color) = if self.stats.dropped_events == 0 {
            (tr!("server_footer_health_ok"), palette.text_faint)
        } else {
            (
                tr!(
                    "server_footer_health_degraded",
                    dropped = i64::try_from(self.stats.dropped_events).unwrap_or(i64::MAX)
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
                    events = i64::try_from(self.stats.total_events_out).unwrap_or(i64::MAX)
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
            .mt(spacing(Spacing::Xxs, density))
            .py(scaled(FOOTER_PAD_V, density))
            .px(scaled(FOOTER_PAD_H, density))
            .border_t(BORDER_THIN)
            .border_color(palette.surface_overlay)
            .bg(palette.shell)
            .child(left)
            .child(right)
            .into_any_element()
    }

    pub(super) fn breadcrumb_status(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        if self.is_running() {
            header_status(
                palette.success,
                tr!(
                    "server_status_listening",
                    clients = i64::try_from(self.connected_clients.len()).unwrap_or(i64::MAX)
                ),
                density,
            )
            .into_any_element()
        } else {
            header_status(palette.text_faint, tr!("server_status_stopped"), density)
                .into_any_element()
        }
    }
}
