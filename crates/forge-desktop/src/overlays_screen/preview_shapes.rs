use forge_components::{
    BORDER_THIN, Density, ForgePalette, Icon, Radius, Spacing, body_family, icon, mono_family,
    radius, spacing, tr, with_alpha,
};
use forge_overlay::metrics::{ALERT, CHAT, FRAME, GOAL, SURFACE_RGB, TICKER};
use forge_overlay::{
    PreviewComposition, PreviewFont, PreviewLineRole, PreviewPosition, PreviewShape,
};
use gpui::{AnyElement, FontWeight, Pixels, Rgba, SharedString, div, prelude::*, px, relative};

use super::kind_visuals::accent_color;
use super::preview_stage::{HINT_FS, HINT_GAP, HINT_GLYPH};

const PLACEHOLDER: &str = "-";
const CHANNEL_MAX: f32 = 255.0;

/// The goal page washes its track with white at this alpha; the metrics carry lengths only.
const TRACK_WASH_ALPHA: f32 = 0.14;

#[derive(Clone, Copy)]
pub(super) struct Scale(f32);

impl Scale {
    pub(super) fn new(factor: f32) -> Self {
        Self(factor)
    }

    pub(super) fn at(self, metric: f32) -> Pixels {
        px(metric * self.0)
    }
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

pub(super) fn render_composition(
    composition: &PreviewComposition,
    scale: Scale,
    badge: Icon,
    palette: &ForgePalette,
) -> AnyElement {
    let accent = accent_color(composition.accent, palette);
    let family = match composition.font {
        PreviewFont::Sans => body_family(),
        PreviewFont::Mono => mono_family(),
    };

    match composition.shape {
        PreviewShape::AudioPlayer => audio_player(badge, family, palette),
        PreviewShape::BadgeBanner => {
            badge_banner(composition, scale, accent, family, badge, palette)
        }
        PreviewShape::BorderedFrame => bordered_frame(composition, scale, accent, family, palette),
        PreviewShape::MessageFeed => message_feed(composition, scale, accent, family, palette),
        PreviewShape::ProgressBar => progress_bar(composition, scale, accent, family, palette),
        PreviewShape::Strip => strip(composition, scale, accent, family, palette),
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
    scale: Scale,
    accent: Rgba,
    family: SharedString,
    badge: Icon,
    palette: &ForgePalette,
) -> AnyElement {
    let mut lines = div().flex().flex_col().min_w(px(0.0));

    if let Some(text) = line_text(composition, PreviewLineRole::Headline) {
        lines = lines.child(banner_headline(text, scale, family.clone(), palette));
    }
    if let Some(text) = line_text(composition, PreviewLineRole::Subline) {
        lines = lines.child(
            div()
                .font_family(family.clone())
                .text_size(scale.at(ALERT.subline_size))
                .text_color(accent)
                .child(text),
        );
    }
    if composition.lines.is_empty() {
        lines = lines.child(banner_headline(PLACEHOLDER.into(), scale, family, palette));
    }

    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(scale.at(ALERT.gap))
        .py(scale.at(ALERT.padding_block))
        .px(scale.at(ALERT.padding_inline))
        .rounded(scale.at(ALERT.radius))
        .border(scale.at(ALERT.border))
        .border_color(accent)
        .bg(surface(ALERT.surface_alpha))
        .child(
            div()
                .flex_none()
                .child(icon(badge, scale.at(ALERT.icon_size), accent)),
        )
        .child(lines)
        .into_any_element()
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
        .text_size(scale.at(ALERT.headline_size))
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
        .gap(scale.at(FRAME.label_gap))
        .py(scale.at(FRAME.label_padding_block))
        .px(scale.at(FRAME.label_padding_inline))
        .rounded(scale.at(FRAME.label_radius))
        .border(scale.at(FRAME.label_border))
        .border_color(accent)
        .bg(surface(FRAME.surface_alpha))
        .font_family(family);

    corner = match composition.position {
        PreviewPosition::Top => corner.top(-scale.at(FRAME.label_overhang)),
        PreviewPosition::Center | PreviewPosition::Bottom => {
            corner.bottom(-scale.at(FRAME.label_overhang))
        }
    };

    let corner = corner
        .children(headline.map(|text| {
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(scale.at(FRAME.headline_size))
                .text_color(palette.text_primary)
                .child(text)
        }))
        .children(subline.map(|text| {
            div()
                .font_weight(FontWeight::BOLD)
                .text_size(scale.at(FRAME.subline_size))
                .text_color(accent)
                .child(text)
        }))
        .when(empty, |label| {
            label.child(
                div()
                    .text_size(scale.at(FRAME.headline_size))
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
        .gap(scale.at(CHAT.row_gap))
        .py(scale.at(CHAT.row_padding_block))
        .px(scale.at(CHAT.row_padding_inline))
        .rounded(scale.at(CHAT.row_radius))
        .bg(surface(CHAT.surface_alpha))
        .font_family(family)
        .children(author.map(|name| {
            div()
                .flex_none()
                .font_weight(FontWeight::BOLD)
                .text_size(scale.at(CHAT.author_size))
                .text_color(accent)
                .child(name)
        }))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .text_size(scale.at(CHAT.message_size))
                .text_color(palette.text_primary)
                .child(message),
        );

    div()
        .flex_none()
        .w(scale.at(CHAT.column_width))
        .flex()
        .flex_col()
        .gap(scale.at(CHAT.row_gap))
        .child(row)
        .into_any_element()
}

fn progress_bar(
    composition: &PreviewComposition,
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
        .h(scale.at(GOAL.track_height))
        .overflow_hidden()
        .rounded(scale.at(GOAL.track_radius))
        .bg(with_alpha(palette.text_primary, TRACK_WASH_ALPHA))
        .children(
            composition
                .fill
                .map(|share| div().h_full().w(relative(share)).bg(accent)),
        );

    div()
        .flex_none()
        .w(scale.at(GOAL.width))
        .flex()
        .flex_col()
        .gap(scale.at(GOAL.gap))
        .py(scale.at(GOAL.padding_block))
        .px(scale.at(GOAL.padding_inline))
        .rounded(scale.at(GOAL.radius))
        .border(scale.at(GOAL.border))
        .border_color(accent)
        .bg(surface(GOAL.surface_alpha))
        .font_family(family)
        .child(
            div()
                .w_full()
                .truncate()
                .font_weight(FontWeight::BOLD)
                .text_size(scale.at(GOAL.label_size))
                .text_color(palette.text_primary)
                .child(label),
        )
        .child(track)
        .child(
            div()
                .w_full()
                .truncate()
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(scale.at(GOAL.figure_size))
                .text_color(accent)
                .child(figure),
        )
        .into_any_element()
}

fn strip(
    composition: &PreviewComposition,
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
        .gap(scale.at(TICKER.gap))
        .overflow_hidden()
        .child(
            div()
                .flex_none()
                .truncate()
                .font_weight(FontWeight::BOLD)
                .text_size(scale.at(TICKER.headline_size))
                .text_color(palette.text_primary)
                .child(headline),
        )
        .children(subline.map(|text| {
            div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .text_size(scale.at(TICKER.subline_size))
                .text_color(accent)
                .child(text)
        }));

    div()
        .flex_none()
        .w_full()
        .flex()
        .items_center()
        .gap(scale.at(TICKER.gap))
        .py(scale.at(TICKER.padding_block))
        .px(scale.at(TICKER.padding_inline))
        .border_t(scale.at(TICKER.edge_border))
        .border_b(scale.at(TICKER.edge_border))
        .border_color(accent)
        .bg(surface(TICKER.surface_alpha))
        .font_family(family)
        .child(
            div()
                .flex_none()
                .w(scale.at(TICKER.mark_width))
                .h(scale.at(TICKER.mark_height))
                .rounded(scale.at(TICKER.mark_radius))
                .bg(accent),
        )
        .child(body)
        .into_any_element()
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
