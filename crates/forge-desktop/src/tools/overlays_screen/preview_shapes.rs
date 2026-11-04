use forge_components::{
    ActivePresentation, BORDER_THIN, Density, ForgePalette, GlyphArt, Icon, Radius, Spacing,
    glyph_art, icon, radius, spacing, tr,
};
use forge_overlay::metrics::{
    ALERT, BRIGHT_INK_RGB, CHAT, FRAME, GOAL, LATEST, MESSAGE_INK_RGB, SURFACE_RGB, TICKER,
};
use forge_overlay::{
    AxisBound, ElementSizing, PreviewCard, PreviewComposition, PreviewElement, PreviewFont,
    PreviewImageSide, PreviewLabelSide, PreviewLineRole, PreviewMark, PreviewPosition,
    PreviewShape, PreviewTextOverflow, TALLY_SEPARATOR,
};
use gpui::{
    AnyElement, App, Div, FontWeight, HighlightStyle, Hsla, Length, Pixels, Rgba, SharedString,
    Size, StyledText, div, img, prelude::*, px, relative,
};
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use super::kind_visuals::accent_color;
use super::preview_stage::{HINT_FS, HINT_GAP, HINT_GLYPH};

const PLACEHOLDER: &str = "-";
const CHANNEL_MAX: f32 = 255.0;
const UNSCALED_TEXT: f32 = 1.0;

const OPAQUE: f32 = 1.0;
const WHOLE_BOX: f32 = 1.0;

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
    text_scale: f32,
    room: Option<Size<Pixels>>,
}

impl ElementPlan {
    pub(super) fn of(element: PreviewElement) -> Self {
        let sizing = element.sizing;
        let text_scale = sizing
            .zip(element.text_size)
            .map_or(UNSCALED_TEXT, |(sizing, size)| sizing.text_scale(size));
        Self {
            sizing,
            text_scale,
            room: None,
        }
    }

    pub(super) fn within(self, room: Size<Pixels>) -> Self {
        Self {
            room: Some(room),
            ..self
        }
    }

    pub(super) fn text_scale(self) -> f32 {
        self.text_scale
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct CardExtent {
    pub(super) least: Pixels,
    pub(super) most: Option<Pixels>,
}

pub(super) fn card_extent(least: u32, most: Option<u32>, room: Pixels, scale: Scale) -> CardExtent {
    let least = scale.at(least as f32).min(room);
    CardExtent {
        least,
        most: most.map(|most| scale.at(most as f32).min(room).max(least)),
    }
}

fn bounded_card(card: Div, bounds: Option<PreviewCard>, plan: ElementPlan, scale: Scale) -> Div {
    let Some((bounds, room)) = bounds.zip(plan.room) else {
        return card;
    };
    let width = card_extent(bounds.min_width, bounds.max_width, room.width, scale);
    let height = card_extent(bounds.min_height, bounds.max_height, room.height, scale);

    card.min_w(width.least)
        .min_h(height.least)
        .when_some(width.most, Div::max_w)
        .when_some(height.most, Div::max_h)
}

fn sized(root: Div, plan: ElementPlan) -> Div {
    let Some(sizing) = plan.sizing else {
        return root;
    };
    let whole_box = || Length::from(relative(WHOLE_BOX));

    let root = match sizing.width {
        Some(AxisBound::Exact) => root.w(whole_box()),
        Some(AxisBound::AtMost) => root.max_w(whole_box()),
        None => root,
    };

    match sizing.height {
        Some(AxisBound::Exact) => root.h(whole_box()),
        Some(AxisBound::AtMost) => root.max_h(whole_box()),
        None => root,
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
    pub(super) image: Option<Arc<Path>>,
}

pub(super) fn render_composition(
    composition: &PreviewComposition,
    plan: ElementPlan,
    scale: Scale,
    glyphs: &StageGlyphs,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    let accent = accent_color(composition.accent);
    let family = match composition.font {
        PreviewFont::Sans => cx.body_family(),
        PreviewFont::Mono => cx.mono_family(),
    };

    match composition.shape {
        PreviewShape::Blank => blank_note(glyphs.badge, family, palette, density),
        PreviewShape::BadgeBanner => badge_banner(composition, plan, scale, accent, family, glyphs),
        PreviewShape::BorderedFrame => bordered_frame(composition, scale, accent, family),
        PreviewShape::LatestCard => latest_card(composition, plan, scale, accent, family),
        PreviewShape::MessageFeed => message_feed(composition, plan, scale, accent, family),
        PreviewShape::ProgressBar => progress_bar(composition, plan, scale, accent, family),
        PreviewShape::Strip => strip(composition, plan, scale, accent, family),
    }
}

fn blank_note(
    badge: Icon,
    family: SharedString,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Sm, density))
        .py(spacing(Spacing::Sm, density))
        .px(spacing(Spacing::Md, density))
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
    glyphs: &StageGlyphs,
) -> AnyElement {
    let art = glyphs.icon.as_ref();
    let cut = composition
        .card
        .is_some_and(|card| card.overflow == PreviewTextOverflow::Ellipsis);
    let mut lines = div().flex_1().min_w(px(0.0)).flex().flex_col();

    if let Some(text) = line_text(composition, PreviewLineRole::Headline) {
        lines = lines.child(banner_headline(text, scale, family.clone(), cut));
    }
    if let Some(text) = line_text(composition, PreviewLineRole::Subline) {
        lines = lines.child(
            div()
                .font_family(family.clone())
                .text_size(scale.text_at(ALERT.subline_size))
                .text_color(accent)
                .when(cut, Styled::truncate)
                .child(text),
        );
    }
    if composition.lines.is_empty() {
        lines = lines.child(banner_headline(PLACEHOLDER.into(), scale, family, cut));
    }

    let message = div()
        .min_w(px(0.0))
        .flex()
        .items_center()
        .gap(scale.text_at(ALERT.gap))
        .children(art.map(|art| {
            div().flex_none().child(glyph_art(
                art,
                scale.text_at(ALERT.icon_size),
                accent,
                "overlays-preview-icon",
            ))
        }))
        .child(lines);

    let picture = composition
        .image
        .zip(glyphs.image.as_ref())
        .map(|(placed, path)| (placed.side, banner_picture(path, placed.height, scale)));
    let stacked = matches!(
        picture,
        Some((PreviewImageSide::Above | PreviewImageSide::Below, _))
    );
    let (leading, trailing) = match picture {
        Some((PreviewImageSide::Below | PreviewImageSide::Right, picture)) => (None, Some(picture)),
        Some((_, picture)) => (Some(picture), None),
        None => (None, None),
    };
    let card = div()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .when(stacked, |card| card.flex_col())
        .overflow_hidden()
        .gap(scale.text_at(ALERT.gap))
        .py(scale.text_at(ALERT.padding_block))
        .px(scale.text_at(ALERT.padding_inline))
        .rounded(scale.text_at(ALERT.radius))
        .border(scale.at(ALERT.border))
        .border_color(accent)
        .bg(composition.surface.map_or_else(
            || surface(ALERT.surface_alpha),
            |chosen| page_color(chosen.rgb, chosen.alpha),
        ))
        .children(leading)
        .child(message)
        .children(trailing);

    bounded_card(sized(card, plan), composition.card, plan, scale).into_any_element()
}

fn banner_picture(path: &Arc<Path>, height: u16, scale: Scale) -> impl IntoElement {
    img(Arc::clone(path))
        .id("overlays-preview-image")
        .flex_none()
        .h(scale.at(f32::from(height)))
}

fn banner_headline(
    text: SharedString,
    scale: Scale,
    family: SharedString,
    cut: bool,
) -> impl IntoElement {
    div()
        .font_family(family)
        .font_weight(FontWeight::BOLD)
        .text_size(scale.text_at(ALERT.headline_size))
        .text_color(bright_ink())
        .when(cut, Styled::truncate)
        .child(text)
}

fn bordered_frame(
    composition: &PreviewComposition,
    scale: Scale,
    accent: Rgba,
    family: SharedString,
) -> AnyElement {
    let headline = line_text(composition, PreviewLineRole::Headline);
    let subline = line_text(composition, PreviewLineRole::Subline);
    let empty = headline.is_none() && subline.is_none();

    let corner = div()
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

    let corner = corner
        .children(headline.map(|text| {
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(scale.text_at(FRAME.headline_size))
                .text_color(bright_ink())
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
                    .text_color(bright_ink())
                    .child(PLACEHOLDER),
            )
        });
    let corner = match composition.label {
        Some(label) => corner
            .relative()
            .left(scale.at(f32::from(label.offset_x)))
            .top(scale.at(f32::from(label.offset_y))),
        None => corner,
    };

    let lane = div()
        .absolute()
        .left_0()
        .right_0()
        .px(scale.at(FRAME.label_inset_inline))
        .flex();
    let lane = match composition.label.map(|label| label.side) {
        Some(PreviewLabelSide::Center) => lane.justify_center(),
        Some(PreviewLabelSide::Right) => lane.justify_end(),
        Some(PreviewLabelSide::Left) | None => lane,
    };
    let lane = match composition.position {
        PreviewPosition::Top => lane.top(-scale.text_at(FRAME.label_overhang)),
        PreviewPosition::Center | PreviewPosition::Bottom => {
            lane.bottom(-scale.text_at(FRAME.label_overhang))
        }
    };

    let frame = div()
        .absolute()
        .size_full()
        .rounded(scale.at(FRAME.radius))
        .border(scale.at(FRAME.border))
        .border_color(accent);

    div()
        .absolute()
        .top(scale.at(FRAME.inset))
        .left(scale.at(FRAME.inset))
        .right(scale.at(FRAME.inset))
        .bottom(scale.at(FRAME.inset))
        .child(frame)
        .child(lane.child(corner))
        .into_any_element()
}

fn message_feed(
    composition: &PreviewComposition,
    plan: ElementPlan,
    scale: Scale,
    accent: Rgba,
    family: SharedString,
) -> AnyElement {
    let author = line_text(composition, PreviewLineRole::Headline);
    let message = line_text(composition, PreviewLineRole::Subline)
        .unwrap_or_else(|| SharedString::from(PLACEHOLDER));
    let badges = line_text(composition, PreviewLineRole::Label);

    let row = div()
        .w_full()
        .flex()
        .items_baseline()
        .gap(scale.text_at(CHAT.row_gap))
        .py(scale.text_at(CHAT.row_padding_block))
        .px(scale.text_at(CHAT.row_padding_inline))
        .rounded(scale.text_at(CHAT.row_radius))
        .bg(surface(CHAT.surface_alpha))
        .children(badges.map(|text| {
            div()
                .flex_none()
                .font_weight(FontWeight::BOLD)
                .text_size(scale.text_at(CHAT.badges_size))
                .text_color(accent)
                .child(text.to_uppercase())
        }))
        .children(author.map(|name| {
            div()
                .min_w(px(0.0))
                .font_weight(FontWeight::BOLD)
                .text_size(scale.text_at(CHAT.author_size))
                .text_color(author_ink(composition))
                .child(name)
        }))
        .child(
            div()
                .min_w(px(0.0))
                .text_size(scale.text_at(CHAT.message_size))
                .text_color(page_color(MESSAGE_INK_RGB, OPAQUE))
                .child(message),
        );

    let rows = div()
        .w_full()
        .flex()
        .flex_col()
        .map(|rows| match composition.position {
            PreviewPosition::Top => rows.justify_start(),
            PreviewPosition::Center | PreviewPosition::Bottom => rows.justify_end(),
        })
        .gap(scale.text_at(CHAT.row_gap))
        .overflow_hidden()
        .font_family(family)
        .child(row);

    sized(div().flex_none().flex().child(rows), plan).into_any_element()
}

fn progress_bar(
    composition: &PreviewComposition,
    plan: ElementPlan,
    scale: Scale,
    accent: Rgba,
    family: SharedString,
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
        .bg(translucent_ink(GOAL.track_wash_alpha))
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
                .text_color(bright_ink())
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
                .child(tally_text(figure)),
        );

    sized(card, plan).into_any_element()
}

fn strip(
    composition: &PreviewComposition,
    plan: ElementPlan,
    scale: Scale,
    accent: Rgba,
    family: SharedString,
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
                .text_color(bright_ink())
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

    sized(bar, plan).into_any_element()
}

fn latest_card(
    composition: &PreviewComposition,
    plan: ElementPlan,
    scale: Scale,
    accent: Rgba,
    family: SharedString,
) -> AnyElement {
    let headline = line_text(composition, PreviewLineRole::Headline)
        .unwrap_or_else(|| SharedString::from(PLACEHOLDER));
    let subline = line_text(composition, PreviewLineRole::Subline);

    let value = div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .items_baseline()
        .overflow_hidden()
        .child(
            div()
                .flex_none()
                .truncate()
                .font_weight(FontWeight::BOLD)
                .text_size(scale.text_at(LATEST.headline_size))
                .text_color(bright_ink())
                .child(headline),
        )
        .children(subline.map(|text| {
            div()
                .flex_1()
                .min_w(px(0.0))
                .ml(scale.text_at(LATEST.subline_margin))
                .truncate()
                .text_size(scale.text_at(LATEST.subline_size))
                .text_color(translucent_ink(LATEST.subline_alpha))
                .child(text)
        }));

    let body = div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(scale.text_at(LATEST.line_gap))
        .children(line_text(composition, PreviewLineRole::Label).map(|text| {
            div()
                .w_full()
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(scale.text_at(LATEST.label_size))
                .text_color(accent)
                .child(text)
        }))
        .child(value);

    let mark = composition.mark.map(|mark| {
        let glyph = match mark {
            PreviewMark::Donation => Icon::Coin,
            PreviewMark::NowPlaying => Icon::Music,
        };
        div()
            .flex_none()
            .child(icon(glyph, scale.text_at(LATEST.icon_size), accent))
    });

    let card = div()
        .flex_none()
        .w_full()
        .flex()
        .items_center()
        .gap(scale.text_at(LATEST.gap))
        .overflow_hidden()
        .py(scale.text_at(LATEST.padding_block))
        .px(scale.text_at(LATEST.padding_inline))
        .rounded(scale.text_at(LATEST.radius))
        .border(scale.at(LATEST.border))
        .border_color(accent)
        .bg(surface(LATEST.surface_alpha))
        .font_family(family)
        .children(mark)
        .child(body);

    sized(card, plan).into_any_element()
}

fn line_text(composition: &PreviewComposition, role: PreviewLineRole) -> Option<SharedString> {
    composition
        .lines
        .iter()
        .find(|line| line.role == role)
        .map(|line| SharedString::from(line.text.clone()))
}

fn tally_separator_range(tally: &str) -> Option<Range<usize>> {
    tally
        .find(TALLY_SEPARATOR)
        .map(|start| start..start + TALLY_SEPARATOR.len())
}

fn tally_text(tally: SharedString) -> StyledText {
    let highlights = tally_separator_range(&tally).map(|range| {
        let ink = Hsla::from(translucent_ink(GOAL.separator_alpha));
        (range, HighlightStyle::from(ink))
    });
    StyledText::new(tally).with_highlights(highlights)
}

fn author_ink(composition: &PreviewComposition) -> Rgba {
    composition
        .author_ink
        .map_or_else(bright_ink, |chosen| page_color(chosen, OPAQUE))
}

fn bright_ink() -> Rgba {
    page_color(BRIGHT_INK_RGB, OPAQUE)
}

fn translucent_ink(alpha: f32) -> Rgba {
    page_color(BRIGHT_INK_RGB, alpha)
}

fn surface(alpha: f32) -> Rgba {
    page_color(SURFACE_RGB, alpha)
}

fn page_color([red, green, blue]: [u8; 3], alpha: f32) -> Rgba {
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

    const ALERT_KIND: &str = forge_overlay::kinds::alert::KIND_ID;
    const CHAT_KIND: &str = forge_overlay::kinds::chat::KIND_ID;
    const FRAME_KIND: &str = forge_overlay::kinds::frame::KIND_ID;
    const GOAL_KIND: &str = forge_overlay::kinds::goal::KIND_ID;
    const TICKER_KIND: &str = forge_overlay::kinds::ticker::KIND_ID;
    const AUDIO_KIND: &str = "overlay.audio";

    fn element(kind: &str, text_size: Option<u32>) -> PreviewElement {
        let mut registry = forge_overlay::OverlayKindRegistry::new();
        forge_overlay::register_builtin_kinds(&mut registry).unwrap();
        PreviewElement {
            text_size,
            sizing: registry
                .get(kind)
                .and_then(|descriptor| descriptor.sizing()),
        }
    }

    fn scale() -> Scale {
        Scale::new(CANVAS_FACTOR, TEXT_FACTOR)
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
            let plan = ElementPlan::of(element(kind, text_size));

            assert_eq!(
                plan.text_scale(),
                expected,
                "{kind} asked for {text_size:?}"
            );
        }
    }

    fn extents(mut root: Div) -> [Option<Length>; 4] {
        let style = root.style();
        [
            style.size.width,
            style.max_size.width,
            style.size.height,
            style.max_size.height,
        ]
    }

    #[test]
    fn each_element_is_bound_to_the_box_the_way_its_stylesheet_binds_it() {
        let whole = Some(Length::from(relative(1.0)));

        for (kind, expected) in [
            (ALERT_KIND, [None, whole, None, whole]),
            (CHAT_KIND, [whole, None, whole, None]),
            (GOAL_KIND, [whole, None, None, whole]),
            (TICKER_KIND, [whole, None, None, whole]),
            (FRAME_KIND, [None, None, None, None]),
            (AUDIO_KIND, [None, None, None, None]),
        ] {
            let plan = ElementPlan::of(element(kind, None));

            assert_eq!(extents(sized(div(), plan)), expected, "{kind}");
        }
    }

    #[test]
    fn the_tally_separator_is_found_only_when_the_tally_carries_one() {
        for (tally, expected) in [
            ("42 / 100", Some(2..5)),
            ("a / b / c", Some(1..4)),
            ("42", None),
            ("42/100", None),
            ("", None),
        ] {
            assert_eq!(tally_separator_range(tally), expected, "{tally:?}");
        }
    }

    #[test]
    fn a_message_author_takes_the_chosen_name_color_and_otherwise_the_page_white() {
        let mut registry = forge_overlay::OverlayKindRegistry::new();
        forge_overlay::register_builtin_kinds(&mut registry).unwrap();
        let descriptor = registry.get(CHAT_KIND).unwrap();
        let mut composition =
            descriptor.preview(&Default::default(), forge_types::NamesLanguage::default());

        composition.author_ink = None;
        assert_eq!(author_ink(&composition), page_color(BRIGHT_INK_RGB, OPAQUE));

        composition.author_ink = Some([0x10, 0x20, 0x30]);
        assert_eq!(
            author_ink(&composition),
            page_color([0x10, 0x20, 0x30], OPAQUE)
        );
    }

    #[test]
    fn a_card_extent_stays_inside_the_room_and_never_caps_below_its_own_minimum() {
        let room = px(400.0);

        for (least, most, expected) in [
            (100, None, (px(50.0), None)),
            (100, Some(600), (px(50.0), Some(px(300.0)))),
            (800, None, (room, None)),
            (1000, Some(1200), (room, Some(room))),
            (0, Some(2000), (px(0.0), Some(room))),
            (300, Some(100), (px(150.0), Some(px(150.0)))),
        ] {
            let extent = card_extent(least, most, room, scale());

            assert_eq!(
                (extent.least, extent.most),
                expected,
                "least {least}, most {most:?}"
            );
        }
    }

    fn card_bounds() -> PreviewCard {
        PreviewCard {
            min_width: 100,
            min_height: 600,
            max_width: Some(300),
            max_height: None,
            overflow: PreviewTextOverflow::Wrap,
        }
    }

    fn card_extents(mut card: Div) -> [Option<Length>; 4] {
        let style = card.style();
        [
            style.min_size.width,
            style.max_size.width,
            style.min_size.height,
            style.max_size.height,
        ]
    }

    #[test]
    fn a_bounded_card_takes_each_axis_from_its_own_bounds_and_its_own_room() {
        let room = Size {
            width: px(1000.0),
            height: px(200.0),
        };
        let plan = ElementPlan::of(element(FRAME_KIND, None)).within(room);

        assert_eq!(
            card_extents(bounded_card(div(), Some(card_bounds()), plan, scale())),
            [
                Some(px(50.0).into()),
                Some(px(150.0).into()),
                Some(px(200.0).into()),
                None,
            ]
        );
    }

    #[test]
    fn a_card_is_left_unbounded_without_card_bounds_or_a_known_room() {
        let room = Size {
            width: px(1000.0),
            height: px(200.0),
        };
        let unplaced = ElementPlan::of(element(FRAME_KIND, None));

        for (label, bounds, plan) in [
            ("no card bounds", None, unplaced.within(room)),
            ("no room yet", Some(card_bounds()), unplaced),
        ] {
            assert_eq!(
                card_extents(bounded_card(div(), bounds, plan, scale())),
                [None; 4],
                "{label}"
            );
        }
    }
}
