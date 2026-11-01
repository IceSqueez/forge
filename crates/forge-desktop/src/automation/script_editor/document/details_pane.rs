use super::ScriptDocument;
use forge_components::OverflowRevealingFocus;
use forge_components::{
    ActivePresentation, BORDER_THIN, Density, FONT_BADGE, FONT_XS, FONT_XXS, ForgePalette,
    ResizeEdge, ResizeRange, Spacing, fmt_display_relative, install_resize, scaled, spacing, tr,
};
use gpui::prelude::*;
use gpui::{AnyElement, App, Context, FontWeight, Pixels, Rgba, SharedString, div, px};

const DETAILS_PANE_PAD: Pixels = px(14.0);
const DETAILS_MAX_W: Pixels = px(420.0);
const DETAILS_MIN_W: Pixels = px(200.0);
const MINI_STAT_GAP: Pixels = px(8.0);
const MINI_STAT_RADIUS: Pixels = px(7.0);
const MINI_STAT_PAD_Y: Pixels = px(7.0);
const MINI_STAT_PAD_X: Pixels = px(9.0);
const MINI_STAT_LABEL_GAP: Pixels = px(3.0);

struct ScriptDetailsResizeDrag;

impl ScriptDocument {
    fn set_details_width(&mut self, width: Pixels, cx: &mut Context<Self>) {
        let clamped = width.clamp(DETAILS_MIN_W, DETAILS_MAX_W);
        if self.details_width != clamped {
            self.details_width = clamped;
            cx.notify();
        }
    }

    pub(super) fn details_pane(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let Some(open) = self.open.as_ref() else {
            return div().into_any_element();
        };
        let record = &open.record;
        let line_count = self.code_input.read(cx).content().lines().count();
        let edited = fmt_display_relative(Some(record.last_modified));

        let linked = self.linked.as_ref();
        let (type_label, type_color): (SharedString, Rgba) = match linked {
            Some(_) => (tr!("script_editor_type_action").into(), palette.theme_brand),
            None => (
                tr!("script_editor_type_standalone").into(),
                palette.accent_peach,
            ),
        };

        let mut inner = div()
            .id("script-details-pane")
            .flex_1()
            .min_h_0()
            .overflow_y_revealing_focus()
            .p(scaled(DETAILS_PANE_PAD, density))
            .child(details_heading(
                tr!("script_editor_details_heading"),
                false,
                palette,
                density,
                cx,
            ))
            .child(detail_row(
                tr!("script_editor_details_type"),
                div().text_color(type_color).child(type_label),
                palette,
                density,
                cx,
            ));

        if let Some(linked) = linked {
            inner = inner.child(detail_row(
                tr!("script_editor_details_linked"),
                div()
                    .font_family(cx.mono_family())
                    .text_color(palette.text_primary)
                    .child(SharedString::from(linked.name.clone())),
                palette,
                density,
                cx,
            ));
        }

        inner = inner
            .child(detail_row(
                tr!("script_editor_details_lines"),
                div()
                    .font_family(cx.mono_family())
                    .child(line_count.to_string()),
                palette,
                density,
                cx,
            ))
            .child(detail_row(
                tr!("script_editor_details_edited"),
                SharedString::from(edited),
                palette,
                density,
                cx,
            ));

        inner = inner.child(self.signature_section(palette, cx));

        let (runs_value, avg_value) = self.run_stat_values();
        inner = inner
            .child(details_heading(
                tr!("script_editor_run_stats_heading"),
                true,
                palette,
                density,
                cx,
            ))
            .child(run_stats_row(runs_value, avg_value, palette, density, cx));

        self.resizable_details_frame(inner, palette, cx)
    }

    fn resizable_details_frame(
        &self,
        inner: impl IntoElement,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pane = div()
            .flex_none()
            .w(self.details_width)
            .h_full()
            .flex()
            .flex_col()
            .bg(palette.shell)
            .border_l(BORDER_THIN)
            .border_color(palette.surface_overlay)
            .child(inner);

        install_resize(
            pane,
            ScriptDetailsResizeDrag,
            "scripts-details-resize",
            ResizeEdge::Left,
            ResizeRange {
                min: DETAILS_MIN_W,
                max: DETAILS_MAX_W,
            },
            palette,
            cx.listener(|this, width: &Pixels, _, cx| this.set_details_width(*width, cx)),
        )
        .into_any_element()
    }

    fn run_stat_values(&self) -> (SharedString, SharedString) {
        match &self.telemetry {
            Some(t) => (
                tr!(
                    "script_editor_stat_runs_value",
                    n = i64::try_from(t.runs_today).unwrap_or(i64::MAX)
                )
                .into(),
                t.avg_duration_ms.map_or_else(
                    || "-".into(),
                    |ms| {
                        tr!(
                            "script_editor_stat_avg_value",
                            n = i64::try_from(ms).unwrap_or(i64::MAX)
                        )
                        .into()
                    },
                ),
            ),
            None => ("-".into(), "-".into()),
        }
    }
}

fn run_stats_row(
    runs_value: SharedString,
    avg_value: SharedString,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> impl IntoElement {
    div()
        .flex()
        .gap(scaled(MINI_STAT_GAP, density))
        .child(mini_stat(
            tr!("script_editor_stat_runs"),
            runs_value,
            palette.text_primary,
            palette,
            density,
            cx,
        ))
        .child(mini_stat(
            tr!("script_editor_stat_avg"),
            avg_value,
            palette.success,
            palette,
            density,
            cx,
        ))
}

pub(super) fn details_heading(
    label: impl Into<SharedString>,
    signature: bool,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> impl IntoElement {
    let heading = div()
        .font_family(cx.mono_family())
        .text_size(FONT_XXS)
        .text_color(palette.text_muted)
        .child(label.into());
    if signature {
        heading
            .pt(spacing(Spacing::Md, density))
            .pb(spacing(Spacing::Xs, density))
    } else {
        heading.pb(spacing(Spacing::Sm, density))
    }
}

fn mini_stat(
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
    value_color: Rgba,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> impl IntoElement {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .bg(palette.base)
        .border(BORDER_THIN)
        .border_color(palette.surface_overlay)
        .rounded(MINI_STAT_RADIUS)
        .py(scaled(MINI_STAT_PAD_Y, density))
        .px(scaled(MINI_STAT_PAD_X, density))
        .child(
            div()
                .font_family(cx.mono_family())
                .text_size(FONT_BADGE)
                .text_color(palette.text_faint)
                .pb(scaled(MINI_STAT_LABEL_GAP, density))
                .child(label.into()),
        )
        .child(
            div()
                .font_family(cx.mono_family())
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(FONT_XS)
                .text_color(value_color)
                .child(value.into()),
        )
}

fn detail_row(
    label: impl Into<SharedString>,
    value: impl IntoElement,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .py(spacing(Spacing::Xxs, density))
        .child(
            div()
                .font_family(cx.body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_faint)
                .child(label.into()),
        )
        .child(
            div()
                .font_family(cx.body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_secondary)
                .child(value),
        )
}
