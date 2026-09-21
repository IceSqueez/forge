use forge_components::{
    BORDER_THIN, Density, ForgePalette, GlyphArt, Icon, Radius, Spacing, body_family, glyph_art,
    icon, mono_family, radius, spacing, tr, with_alpha,
};
use forge_overlay::metrics::{ALERT, CHAT, FRAME, GOAL, SURFACE_RGB, TICKER};
use forge_overlay::{
    AxisBound, AxisFallback, ElementAxis, ElementSizing, PreviewComposition, PreviewElement,
    PreviewFont, PreviewLineRole, PreviewPosition, PreviewShape, element_sizing,
};
use gpui::{
    AnyElement, Div, FontWeight, Length, Pixels, Rgba, SharedString, div, prelude::*, px, relative,
};

use super::kind_visuals::accent_color;
use super::preview_stage::{HINT_FS, HINT_GAP, HINT_GLYPH};

const PLACEHOLDER: &str = "-";
const CHANNEL_MAX: f32 = 255.0;
const UNSCALED_TEXT: f32 = 1.0;

/// The goal page washes its track with white at this alpha; the metrics carry lengths only.
const TRACK_WASH_ALPHA: f32 = 0.14;

/// Two factors the stage composes: `at` carries the metrics a stylesheet states in plain pixels,
/// `text_at` the ones it multiplies by `--text-scale`.
#[derive(Clone, Copy)]
pub(super) struct Scale {
    canvas: f32,
    text: f32,
}

impl Scale {
    pub(super) fn new(canvas: f32, text: f32) -> Self {
        Self { canvas, text }
    }

    pub(super) fn at(self, metric: f32) -> Pixels {
        px(metric * self.canvas)
    }

    fn text_at(self, metric: f32) -> Pixels {
        px(metric * self.canvas * self.text)
    }
}

#[derive(Clone, Copy)]
pub(super) struct ElementPlan {
    sizing: Option<ElementSizing>,
    element: PreviewElement,
    text_scale: f32,
}

impl ElementPlan {
    pub(super) fn of(kind_id: &str, element: PreviewElement) -> Self {
        let sizing = element_sizing(kind_id);
        let text_scale = sizing
            .zip(element.text_size)
            .map_or(UNSCALED_TEXT, |(sizing, size)| sizing.text_scale(size));
        Self {
            sizing,
            element,
            text_scale,
        }
    }

    pub(super) fn text_scale(self) -> f32 {
        self.text_scale
    }
}

fn sized(root: Div, plan: ElementPlan, scale: Scale) -> Div {
    let Some(sizing) = plan.sizing else {
        return root;
    };

    let root = match axis_extent(sizing.width, plan.element.width, scale) {
        Some((AxisBound::Exact, length)) => root.w(length),
        Some((AxisBound::AtLeast, length)) => root.min_w(length),
        Some((AxisBound::AtMost, length)) => root.max_w(length),
        None => root,
    };

    match axis_extent(sizing.height, plan.element.height, scale) {
        Some((AxisBound::Exact, length)) => root.h(length),
        Some((AxisBound::AtLeast, length)) => root.min_h(length),
        Some((AxisBound::AtMost, length)) => root.max_h(length),
        None => root,
    }
}

fn axis_extent(
    rule: Option<ElementAxis>,
    value: Option<u32>,
    scale: Scale,
) -> Option<(AxisBound, Length)> {
    let rule = rule?;
    let length = match value {
        Some(pixels) => Length::from(scale.at(pixels as f32)),
        None => match rule.fallback {
            AxisFallback::Content => return None,
            AxisFallback::Canvas => Length::from(relative(1.0)),
            AxisFallback::Pixels(metric) => Length::from(scale.at(metric)),
        },
    };
    Some((rule.bound, length))
}

pub(super) fn body_padding(shape: PreviewShape) -> Option<f32> {
    match shape {
        PreviewShape::AudioPlayer => None,
        PreviewShape::BadgeBanner => Some(ALERT.body_padding),
        PreviewShape::MessageFeed => Some(CHAT.body_padding),
        PreviewShape::ProgressBar => Some(GOAL.body_padding),
        PreviewShape::BorderedFrame | PreviewShape::Strip => None,
    }
}

pub(super) fn centers_horizontally(shape: PreviewShape) -> bool {
    shape != PreviewShape::MessageFeed
}

pub(super) fn fills_canvas(shape: PreviewShape) -> bool {
    shape == PreviewShape::BorderedFrame
}

pub(super) struct StageGlyphs {
    pub(super) badge: Icon,
    pub(super) icon: Option<GlyphArt>,
}

pub(super) fn render_composition(
    composition: &PreviewComposition,
    plan: ElementPlan,
    scale: Scale,
    glyphs: &StageGlyphs,
    palette: &ForgePalette,
) -> AnyElement {
    let accent = accent_color(composition.accent, palette);
    let family = match composition.font {
        PreviewFont::Sans => body_family(),
        PreviewFont::Mono => mono_family(),
    };

    match composition.shape {
        PreviewShape::AudioPlayer => audio_player(glyphs.badge, family, palette),
        PreviewShape::BadgeBanner => badge_banner(
            composition,
            plan,
            scale,
            accent,
            family,
            glyphs.icon.as_ref(),
            palette,
        ),
        PreviewShape::BorderedFrame => bordered_frame(composition, scale, accent, family, palette),
        PreviewShape::MessageFeed => {
            message_feed(composition, plan, scale, accent, family, palette)
        }
        PreviewShape::ProgressBar => {
            progress_bar(composition, plan, scale, accent, family, palette)
        }
        PreviewShape::Strip => strip(composition, plan, scale, accent, family, palette),
    }
}

fn audio_player(badge: Icon, family: SharedString, palette: &ForgePalette) -> AnyElement {
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Sm, Density::Cozy))
        .py(spacing(Spacing::Sm, Density::Cozy))
        .px(spacing(Spacing::Md, Density::Cozy))
        .rounded(radius(Radius::Md))
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .child(
            div()
                .flex_none()
                .child(icon(badge, HINT_GLYPH, palette.text_faint)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(HINT_GAP)
                .font_family(family)
                .text_size(HINT_FS)
                .text_color(palette.text_faint)
                .child(tr!("overlays_preview_audio_silent"))
                .child(tr!("overlays_preview_audio_needs_source")),
        )
        .into_any_element()
}

fn badge_banner(
    composition: &PreviewComposition,
    plan: ElementPlan,
    scale: Scale,
    accent: Rgba,
    family: SharedString,
    art: Option<&GlyphArt>,
    palette: &ForgePalette,
) -> AnyElement {
    let mut lines = div().flex_1().min_w(px(0.0)).flex().flex_col();

    if let Some(text) = line_text(composition, PreviewLineRole::Headline) {
        lines = lines.child(banner_headline(text, scale, family.clone(), palette));
    }
    if let Some(text) = line_text(composition, PreviewLineRole::Subline) {
        lines = lines.child(
            div()
                .font_family(family.clone())
                .text_size(scale.text_at(ALERT.subline_size))
                .text_color(accent)
                .child(text),
        );
    }
    if composition.lines.is_empty() {
        lines = lines.child(banner_headline(PLACEHOLDER.into(), scale, family, palette));
    }

    let card = div()
        .flex_none()
        .flex()
        .items_center()
        .gap(scale.text_at(ALERT.gap))
        .py(scale.text_at(ALERT.padding_block))
        .px(scale.text_at(ALERT.padding_inline))
        .rounded(scale.text_at(ALERT.radius))
        .border(scale.at(ALERT.border))
        .border_color(accent)
        .bg(surface(ALERT.surface_alpha))
        .children(art.map(|art| {
            div().flex_none().child(glyph_art(
                art,
                scale.text_at(ALERT.icon_size),
                accent,
                "overlays-preview-icon",
            ))
        }))
        .child(lines);

    sized(card, plan, scale).into_any_element()
}

fn banner_headline(
    text: SharedString,
    scale: Scale,
    family: SharedString,
    palette: &ForgePalette,
) -> impl IntoElement {
    div()
        .font_family(family)
        .font_weight(FontWeight::BOLD)
        .text_size(scale.text_at(ALERT.headline_size))
        .text_color(palette.text_primary)
        .child(text)
}

fn bordered_frame(
    composition: &PreviewComposition,
    scale: Scale,
    accent: Rgba,
    family: SharedString,
    palette: &ForgePalette,
) -> AnyElement {
    let headline = line_text(composition, PreviewLineRole::Headline);
    let subline = line_text(composition, PreviewLineRole::Subline);
    let empty = headline.is_none() && subline.is_none();

    let mut corner = div()
        .absolute()
        .left(scale.at(FRAME.label_inset_inline))
        .flex()
        .items_center()
        .gap(scale.text_at(FRAME.label_gap))
        .py(scale.text_at(FRAME.label_padding_block))
        .px(scale.text_at(FRAME.label_padding_inline))
        .rounded(scale.at(FRAME.label_radius))
        .border(scale.at(FRAME.label_border))
        .border_color(accent)
        .bg(surface(FRAME.surface_alpha))
        .font_family(family);

    corner = match composition.position {
        PreviewPosition::Top => corner.top(-scale.text_at(FRAME.label_overhang)),
        PreviewPosition::Center | PreviewPosition::Bottom => {
            corner.bottom(-scale.text_at(FRAME.label_overhang))
        }
    };

    let corner = corner
        .children(headline.map(|text| {
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(scale.text_at(FRAME.headline_size))
                .text_color(palette.text_primary)
                .child(text)
        }))
        .children(subline.map(|text| {
            div()
                .font_weight(FontWeight::BOLD)
                .text_size(scale.text_at(FRAME.subline_size))
                .text_color(accent)
                .child(text)
        }))
        .when(empty, |label| {
            label.child(
                div()
                    .text_size(scale.text_at(FRAME.headline_size))
                    .text_color(palette.text_primary)
                    .child(PLACEHOLDER),
            )
        });

    div()
        .absolute()
        .top(scale.at(FRAME.inset))
        .left(scale.at(FRAME.inset))
        .right(scale.at(FRAME.inset))
        .bottom(scale.at(FRAME.inset))
        .rounded(scale.at(FRAME.radius))
        .border(scale.at(FRAME.border))
        .border_color(accent)
        .child(corner)
        .into_any_element()
}

fn message_feed(
    composition: &PreviewComposition,
    plan: ElementPlan,
    scale: Scale,
    accent: Rgba,
    family: SharedString,
    palette: &ForgePalette,
) -> AnyElement {
    let author = line_text(composition, PreviewLineRole::Headline);
    let message = line_text(composition, PreviewLineRole::Subline)
        .unwrap_or_else(|| SharedString::from(PLACEHOLDER));

    let row = div()
        .w_full()
        .flex()
        .items_baseline()
        .gap(scale.text_at(CHAT.row_gap))
        .py(scale.text_at(CHAT.row_padding_block))
        .px(scale.text_at(CHAT.row_padding_inline))
        .rounded(scale.text_at(CHAT.row_radius))
        .bg(surface(CHAT.surface_alpha))
        .children(author.map(|name| {
            div()
                .min_w(px(0.0))
                .font_weight(FontWeight::BOLD)
                .text_size(scale.text_at(CHAT.author_size))
                .text_color(accent)
                .child(name)
        }))
        .child(
            div()
                .min_w(px(0.0))
                .text_size(scale.text_at(CHAT.message_size))
                .text_color(palette.text_primary)
                .child(message),
        );

    let rows = div()
        .w_full()
        .flex()
        .flex_col()
        .gap(scale.text_at(CHAT.row_gap))
        .overflow_hidden()
        .font_family(family)
        .child(row);

    sized(div().flex_none().flex().child(rows), plan, scale).into_any_element()
}

fn progress_bar(
    composition: &PreviewComposition,
    plan: ElementPlan,
    scale: Scale,
    accent: Rgba,
    family: SharedString,
    palette: &ForgePalette,
) -> AnyElement {
    let label = line_text(composition, PreviewLineRole::Headline)
        .unwrap_or_else(|| SharedString::from(PLACEHOLDER));
    let figure = line_text(composition, PreviewLineRole::Subline)
        .unwrap_or_else(|| SharedString::from(PLACEHOLDER));

    let track = div()
        .w_full()
        .flex_none()
        .h(scale.text_at(GOAL.track_height))
        .overflow_hidden()
        .rounded(scale.at(GOAL.track_radius))
        .bg(with_alpha(palette.text_primary, TRACK_WASH_ALPHA))
        .children(
            composition
                .fill
                .map(|share| div().h_full().w(relative(share)).bg(accent)),
        );

    let card = div()
        .flex_none()
        .flex()
        .flex_col()
        .justify_center()
        .gap(scale.text_at(GOAL.gap))
        .py(scale.text_at(GOAL.padding_block))
        .px(scale.text_at(GOAL.padding_inline))
        .rounded(scale.text_at(GOAL.radius))
        .border(scale.at(GOAL.border))
        .border_color(accent)
        .bg(surface(GOAL.surface_alpha))
        .font_family(family)
        .child(
            div()
                .w_full()
                .min_w(px(0.0))
                .font_weight(FontWeight::BOLD)
                .text_size(scale.text_at(GOAL.label_size))
                .text_color(palette.text_primary)
                .child(label),
        )
        .child(track)
        .child(
            div()
                .w_full()
                .min_w(px(0.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(scale.text_at(GOAL.figure_size))
                .text_color(accent)
                .child(figure),
        );

    sized(card, plan, scale).into_any_element()
}

fn strip(
    composition: &PreviewComposition,
    plan: ElementPlan,
    scale: Scale,
    accent: Rgba,
    family: SharedString,
    palette: &ForgePalette,
) -> AnyElement {
    let headline = line_text(composition, PreviewLineRole::Headline)
        .unwrap_or_else(|| SharedString::from(PLACEHOLDER));
    let subline = line_text(composition, PreviewLineRole::Subline);

    let body = div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .items_baseline()
        .gap(scale.text_at(TICKER.gap))
        .overflow_hidden()
        .child(
            div()
                .flex_none()
                .truncate()
                .font_weight(FontWeight::BOLD)
                .text_size(scale.text_at(TICKER.headline_size))
                .text_color(palette.text_primary)
                .child(headline),
        )
        .children(subline.map(|text| {
            div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .text_size(scale.text_at(TICKER.subline_size))
                .text_color(accent)
                .child(text)
        }));

    let bar = div()
        .flex_none()
        .w_full()
        .flex()
        .items_center()
        .gap(scale.text_at(TICKER.gap))
        .py(scale.text_at(TICKER.padding_block))
        .px(scale.text_at(TICKER.padding_inline))
        .border_t(scale.at(TICKER.edge_border))
        .border_b(scale.at(TICKER.edge_border))
        .border_color(accent)
        .bg(surface(TICKER.surface_alpha))
        .font_family(family)
        .child(
            div()
                .flex_none()
                .w(scale.text_at(TICKER.mark_width))
                .h(scale.text_at(TICKER.mark_height))
                .rounded(scale.text_at(TICKER.mark_radius))
                .bg(accent),
        )
        .child(body);

    sized(bar, plan, scale).into_any_element()
}

fn line_text(composition: &PreviewComposition, role: PreviewLineRole) -> Option<SharedString> {
    composition
        .lines
        .iter()
        .find(|line| line.role == role)
        .map(|line| SharedString::from(line.text.clone()))
}

/// The pages paint their own surface regardless of the app theme, so the stage does too.
fn surface(alpha: f32) -> Rgba {
    let [red, green, blue] = SURFACE_RGB;
    Rgba {
        r: f32::from(red) / CHANNEL_MAX,
        g: f32::from(green) / CHANNEL_MAX,
        b: f32::from(blue) / CHANNEL_MAX,
        a: alpha,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CANVAS_FACTOR: f32 = 0.5;
    const TEXT_FACTOR: f32 = 2.0;

    const ALERT_KIND: &str = "overlay.alert";
    const CHAT_KIND: &str = "overlay.chat";
    const FRAME_KIND: &str = "overlay.frame";
    const GOAL_KIND: &str = "overlay.goal";
    const TICKER_KIND: &str = "overlay.ticker";
    const AUDIO_KIND: &str = "overlay.audio";

    const UNCONSTRAINED: [Option<Length>; 6] = [None, None, None, None, None, None];

    fn element(width: Option<u32>, height: Option<u32>, text_size: Option<u32>) -> PreviewElement {
        PreviewElement {
            width,
            height,
            text_size,
        }
    }

    fn scale() -> Scale {
        Scale::new(CANVAS_FACTOR, TEXT_FACTOR)
    }

    fn pixels(value: f32) -> Option<Length> {
        Some(Length::from(px(value)))
    }

    fn canvas_span() -> Option<Length> {
        Some(Length::from(relative(1.0)))
    }

    fn extents(mut root: Div) -> [Option<Length>; 6] {
        let style = root.style();
        [
            style.size.width,
            style.min_size.width,
            style.max_size.width,
            style.size.height,
            style.min_size.height,
            style.max_size.height,
        ]
    }

    #[test]
    fn a_canvas_metric_ignores_the_text_factor_that_a_text_metric_multiplies_in() {
        let scale = scale();

        assert_eq!(scale.at(10.0), px(5.0));
        assert_eq!(scale.text_at(10.0), px(10.0));
    }

    #[test]
    fn the_text_factor_is_the_chosen_size_over_the_kinds_own_base() {
        for (kind, text_size, expected) in [
            (ALERT_KIND, Some(40), 2.0),
            (ALERT_KIND, Some(10), 0.5),
            (CHAT_KIND, Some(26), 2.0),
            (FRAME_KIND, Some(13), 1.0),
            (GOAL_KIND, Some(7), 0.5),
            (TICKER_KIND, Some(38), 2.0),
            (AUDIO_KIND, Some(40), 1.0),
            (ALERT_KIND, None, 1.0),
        ] {
            let plan = ElementPlan::of(kind, element(None, None, text_size));

            assert_eq!(
                plan.text_scale(),
                expected,
                "{kind} asked for {text_size:?}"
            );
        }
    }

    #[test]
    fn an_axis_takes_the_bound_and_the_fallback_its_kind_declares() {
        for (kind, element, expected) in [
            (
                ALERT_KIND,
                element(Some(600), Some(200), None),
                [pixels(300.0), None, None, None, pixels(100.0), None],
            ),
            (ALERT_KIND, element(None, None, None), UNCONSTRAINED),
            (
                CHAT_KIND,
                element(Some(600), Some(200), None),
                [pixels(300.0), None, None, None, None, pixels(100.0)],
            ),
            (
                CHAT_KIND,
                element(None, None, None),
                [pixels(180.0), None, None, None, None, canvas_span()],
            ),
            (
                FRAME_KIND,
                element(Some(600), Some(200), None),
                UNCONSTRAINED,
            ),
            (
                GOAL_KIND,
                element(None, None, None),
                [pixels(160.0), None, None, None, None, None],
            ),
            (
                TICKER_KIND,
                element(Some(600), Some(200), None),
                [None, None, None, None, pixels(100.0), None],
            ),
            (
                AUDIO_KIND,
                element(Some(600), Some(200), Some(40)),
                UNCONSTRAINED,
            ),
        ] {
            let plan = ElementPlan::of(kind, element);

            assert_eq!(extents(sized(div(), plan, scale())), expected, "{kind}");
        }
    }
}
