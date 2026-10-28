use forge_components::FONT_FIELD;
use forge_components::FONT_LABEL;
use forge_components::FONT_META;
use forge_components::Spacing;
use forge_components::spacing;
use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, Density, FONT_MD, FONT_XS, ForgePalette, Icon, Radius,
    body_family, grow_cell, icon, mono_family, page_frame, radius, scaled, status_dot, tr,
};
use gpui::{
    AnyElement, AnyView, ClickEvent, Context, EventEmitter, FontWeight, Pixels, Rgba, SharedString,
    div, prelude::*, px,
};

use crate::screen::Screen;
use crate::sidebar::NavRequested;

const BODY_PAD_V: Pixels = px(18.0);
const BODY_PAD_H: Pixels = px(22.0);
const SECTION_GAP: Pixels = px(14.0);
const COLUMN_GAP: Pixels = px(12.0);

const HERO_PAD_H: Pixels = px(18.0);
const HERO_TEXT_GAP: Pixels = px(2.0);
const HERO_TILE: Pixels = px(48.0);
const HERO_TILE_RADIUS: Pixels = px(11.0);
const HERO_GLYPH: Pixels = px(24.0);

const CARD_HEAD_PAD_V: Pixels = px(12.0);
const CARD_HEAD_PAD_H: Pixels = px(14.0);
const CARD_BODY_PAD: Pixels = px(14.0);
const CARD_TITLE_SIZE: Pixels = FONT_FIELD;
const CARD_TITLE_GAP: Pixels = px(7.0);
const CARD_GLYPH: Pixels = px(14.0);

const STEP_PAD_V: Pixels = px(7.0);
const STEP_NUMBER_SIZE: Pixels = FONT_META;
const STEP_TEXT_SIZE: Pixels = FONT_LABEL;
const STEP_LINE_HEIGHT: Pixels = px(17.25);
const INTRO_LINE_HEIGHT: Pixels = px(17.8);

const HEADER_STATUS_GAP: Pixels = px(5.0);
const HEADER_DOT_SIZE: Pixels = px(7.0);
const HEADER_STATUS_SIZE: Pixels = FONT_LABEL;

const LEFT_COLUMN_GROW: f32 = 10.0;
const RIGHT_COLUMN_GROW: f32 = 12.0;

pub(crate) type GuideStep = Vec<Vec<AnyElement>>;

pub(crate) struct ConnectPage {
    pub scroll_id: &'static str,
    pub crumb_id: &'static str,
    pub hero_glyph: Icon,
    pub hero_tint: Rgba,
    pub step_number_tint: Rgba,
    pub title_key: &'static str,
    pub subtitle_key: &'static str,
    pub guide_title_key: &'static str,
    pub guide_intro_key: &'static str,
    pub settings_title_key: &'static str,
    pub steps: Vec<GuideStep>,
    pub form: AnyView,
}

impl ConnectPage {
    pub fn render<V: EventEmitter<NavRequested>>(
        self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<V>,
    ) -> impl IntoElement + use<V> {
        let title = tr!(self.title_key);
        let columns = div()
            .w_full()
            .flex()
            .items_stretch()
            .gap(scaled(COLUMN_GAP, density))
            .mb(scaled(SECTION_GAP, density))
            .child(grow_cell(
                guide_card(
                    tr!(self.guide_title_key),
                    tr!(self.guide_intro_key),
                    self.steps,
                    self.step_number_tint,
                    palette,
                    density,
                ),
                LEFT_COLUMN_GROW,
            ))
            .child(grow_cell(
                settings_card(tr!(self.settings_title_key), self.form, palette, density),
                RIGHT_COLUMN_GROW,
            ));

        let body = div()
            .w_full()
            .flex()
            .flex_col()
            .child(div().w_full().mb(scaled(SECTION_GAP, density)).child(hero(
                self.hero_glyph,
                self.hero_tint,
                title.clone(),
                tr!(self.subtitle_key),
                palette,
                density,
            )))
            .child(columns);

        let scroll = div()
            .id(self.scroll_id)
            .flex_1()
            .overflow_y_scroll()
            .bg(palette.base)
            .child(
                div()
                    .w_full()
                    .py(scaled(BODY_PAD_V, density))
                    .px(scaled(BODY_PAD_H, density))
                    .child(body),
            );

        page_frame(
            vec![
                BreadcrumbCrumb::link(
                    tr!("integrations_breadcrumb"),
                    self.crumb_id,
                    cx.listener(|_, _: &ClickEvent, _, cx| {
                        cx.emit(NavRequested(Screen::Integrations(None)))
                    }),
                ),
                BreadcrumbCrumb::leaf(title),
            ],
            palette,
        )
        .density(density)
        .header_right(header_status(palette, density))
        .body(scroll)
    }
}

pub(crate) fn fragment(text: impl Into<SharedString>, tint: Rgba) -> Vec<AnyElement> {
    words(text, tint, body_family())
}

pub(crate) fn mono_fragment(text: impl Into<SharedString>, tint: Rgba) -> Vec<AnyElement> {
    words(text, tint, mono_family())
}

fn hero(
    glyph: Icon,
    tint: Rgba,
    title: String,
    subtitle: String,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let tile = div()
        .flex_none()
        .size(HERO_TILE)
        .flex()
        .items_center()
        .justify_center()
        .rounded(HERO_TILE_RADIUS)
        .bg(palette.surface_overlay)
        .child(icon(glyph, HERO_GLYPH, tint));

    let text = div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(scaled(HERO_TEXT_GAP, density))
        .child(
            div()
                .font_family(body_family())
                .font_weight(FontWeight::MEDIUM)
                .text_size(FONT_MD)
                .text_color(palette.text_primary)
                .child(title),
        )
        .child(
            div()
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child(subtitle),
        );

    card_shell(palette)
        .flex()
        .items_center()
        .gap(spacing(Spacing::Md, density))
        .py(spacing(Spacing::Md, density))
        .px(scaled(HERO_PAD_H, density))
        .child(tile)
        .child(text)
        .into_any_element()
}

fn guide_card(
    title: String,
    intro: String,
    steps: Vec<GuideStep>,
    number_tint: Rgba,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let count = steps.len();
    let rows = steps.into_iter().enumerate().map(|(index, parts)| {
        let number = index + 1;
        step_row(
            SharedString::from(format!("{number}.")),
            parts,
            number == count,
            number_tint,
            palette,
            density,
        )
    });

    let intro = div()
        .font_family(body_family())
        .text_size(STEP_TEXT_SIZE)
        .text_color(palette.text_muted)
        .line_height(INTRO_LINE_HEIGHT)
        .mb(scaled(SECTION_GAP, density))
        .child(intro);

    let body = div()
        .w_full()
        .flex()
        .flex_col()
        .p(scaled(CARD_BODY_PAD, density))
        .child(intro)
        .children(rows);

    card_shell(palette)
        .flex()
        .flex_col()
        .child(card_header(
            Icon::InfoCircle,
            palette.info,
            title,
            palette,
            density,
        ))
        .child(body)
        .into_any_element()
}

fn settings_card(
    title: String,
    form: AnyView,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let body = div()
        .w_full()
        .flex()
        .flex_col()
        .p(scaled(CARD_BODY_PAD, density))
        .child(form);

    card_shell(palette)
        .flex()
        .flex_col()
        .child(card_header(
            Icon::Plug,
            palette.success,
            title,
            palette,
            density,
        ))
        .child(body)
        .into_any_element()
}

fn header_status(palette: &ForgePalette, density: Density) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(scaled(HEADER_STATUS_GAP, density))
        .child(status_dot(palette.text_faint, HEADER_DOT_SIZE))
        .child(
            div()
                .font_family(body_family())
                .text_size(HEADER_STATUS_SIZE)
                .text_color(palette.text_muted)
                .child(tr!("common_status_not_connected")),
        )
        .into_any_element()
}

fn card_shell(palette: &ForgePalette) -> gpui::Div {
    div()
        .w_full()
        .rounded(radius(Radius::Md))
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .bg(palette.elevated)
}

fn card_header(
    glyph: Icon,
    tint: Rgba,
    title: String,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(scaled(CARD_TITLE_GAP, density))
        .py(scaled(CARD_HEAD_PAD_V, density))
        .px(scaled(CARD_HEAD_PAD_H, density))
        .border_b(BORDER_THIN)
        .border_color(palette.border_regular)
        .child(icon(glyph, CARD_GLYPH, tint))
        .child(
            div()
                .font_family(body_family())
                .font_weight(FontWeight::MEDIUM)
                .text_size(CARD_TITLE_SIZE)
                .text_color(palette.text_primary)
                .child(title),
        )
}

fn words(text: impl Into<SharedString>, tint: Rgba, family: SharedString) -> Vec<AnyElement> {
    let text: SharedString = text.into();
    text.split_whitespace()
        .map(|word| {
            div()
                .font_family(family.clone())
                .text_size(STEP_TEXT_SIZE)
                .text_color(tint)
                .line_height(STEP_LINE_HEIGHT)
                .child(word.to_owned())
                .into_any_element()
        })
        .collect()
}

fn step_row(
    number: SharedString,
    parts: GuideStep,
    last: bool,
    number_tint: Rgba,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let text = div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .flex_row()
        .flex_wrap()
        .gap_x(spacing(Spacing::Xxs, density))
        .children(parts.into_iter().flatten());

    let mut row = div()
        .w_full()
        .flex()
        .items_start()
        .gap(spacing(Spacing::Sm, density))
        .py(scaled(STEP_PAD_V, density))
        .child(
            div()
                .flex_none()
                .font_family(mono_family())
                .font_weight(FontWeight::MEDIUM)
                .text_size(STEP_NUMBER_SIZE)
                .text_color(number_tint)
                .line_height(STEP_LINE_HEIGHT)
                .child(number),
        )
        .child(text);
    if !last {
        row = row
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular);
    }
    row.into_any_element()
}
