use std::sync::Arc;

use gpui::{
    AnyElement, App, ClickEvent, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Pixels, RenderOnce, Rgba, SharedString, StatefulInteractiveElement, Styled,
    Window, div, px,
};

use crate::icons::{Icon, icon, spinner};
use crate::palette::ForgePalette;
use crate::status::status_dot;
use crate::tokens::{BORDER_THIN, FONT_XXS, Radius, body_family, mono_family, radius};
use crate::tooltip::tooltip_builder;

pub const HUB_CARD_HEIGHT: Pixels = px(158.0);
const CARD_PAD_TOP: Pixels = px(14.0);
const CARD_PAD_H: Pixels = px(16.0);
const CARD_PAD_BOTTOM: Pixels = px(12.0);

const HEADER_GAP: Pixels = px(12.0);
const TITLE_GAP: Pixels = px(8.0);
const TITLE_MB: Pixels = px(3.0);
const TITLE_SIZE: Pixels = px(14.0);
const DESC_SIZE: Pixels = px(11.5);
const DESC_LINE: Pixels = px(16.0);
const DESC_LINES: usize = 2;

pub const HUB_TILE_SIZE: Pixels = px(40.0);
const TILE_CORNER_DIVISOR: f32 = 4.0;
const TILE_GLYPH_DIVISOR: f32 = 2.0;

const NOTE_HEIGHT: Pixels = px(20.0);
const NOTE_MT: Pixels = px(8.0);
const NOTE_GAP: Pixels = px(6.0);
const NOTE_ICON: Pixels = px(11.0);
const NOTE_SIZE: Pixels = px(11.0);

const FOOTER_HEIGHT: Pixels = px(36.0);
const FOOTER_PAD_TOP: Pixels = px(9.0);
const FOOTER_GAP: Pixels = px(8.0);

const BADGE_SIZE: Pixels = px(9.5);
const BADGE_PAD_V: Pixels = px(1.0);
const BADGE_PAD_H: Pixels = px(6.0);
const BADGE_RADIUS: Pixels = px(8.0);
const BADGE_GAP: Pixels = px(4.0);
const BADGE_GLYPH: Pixels = px(10.0);
const BADGE_DOT: Pixels = px(5.0);

const COMPACT_SIZE: Pixels = px(11.0);
const COMPACT_ICON: Pixels = px(11.0);
const COMPACT_ICON_ONLY: Pixels = px(12.0);
const COMPACT_GAP: Pixels = px(5.0);
const COMPACT_PAD_V: Pixels = px(3.0);
const GHOST_PAD_H: Pixels = px(9.0);
const PRIMARY_PAD_H: Pixels = px(10.0);
const ICON_ONLY_PAD_V: Pixels = px(4.0);
const ICON_ONLY_PAD_H: Pixels = px(6.0);

const SECTION_GAP: Pixels = px(10.0);
const SECTION_MB: Pixels = px(10.0);
const SECTION_LABEL_SIZE: Pixels = px(9.5);
const SECTION_BLURB_SIZE: Pixels = px(11.0);

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub enum HubTileGlyph {
    Letter(SharedString),
    Icon(Icon),
}

pub fn hub_tile(
    glyph: HubTileGlyph,
    color: Rgba,
    muted: bool,
    size: Pixels,
    palette: &ForgePalette,
) -> impl IntoElement {
    let frame = div()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(size)
        .rounded(size / TILE_CORNER_DIVISOR);
    let glyph_size = size / TILE_GLYPH_DIVISOR;
    match glyph {
        HubTileGlyph::Letter(letter) => {
            let (bg, fg) = if muted {
                (palette.surface_overlay, palette.text_faint)
            } else {
                (color, palette.shell)
            };
            frame
                .bg(bg)
                .font_family(body_family())
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(glyph_size)
                .text_color(fg)
                .child(letter)
        }
        HubTileGlyph::Icon(glyph) => {
            let tint = if muted { palette.text_faint } else { color };
            frame
                .bg(palette.surface_overlay)
                .child(icon(glyph, glyph_size, tint))
        }
    }
}

pub enum BadgeMarker {
    None,
    Dot(Rgba),
    Glyph(Icon),
    Spinner(ElementId),
}

pub fn lifecycle_badge(
    marker: BadgeMarker,
    label: impl Into<SharedString>,
    color: Rgba,
    palette: &ForgePalette,
) -> impl IntoElement {
    let mut row = div()
        .flex_none()
        .flex()
        .items_center()
        .gap(BADGE_GAP)
        .py(BADGE_PAD_V)
        .px(BADGE_PAD_H)
        .rounded(BADGE_RADIUS)
        .bg(palette.surface_overlay)
        .font_family(body_family())
        .font_weight(FontWeight::MEDIUM)
        .text_size(BADGE_SIZE)
        .text_color(color);
    row = match marker {
        BadgeMarker::None => row,
        BadgeMarker::Dot(dot) => row.child(status_dot(dot, BADGE_DOT)),
        BadgeMarker::Glyph(glyph) => row.child(icon(glyph, BADGE_GLYPH, color)),
        BadgeMarker::Spinner(id) => row.child(spinner(id, Icon::Loader2, BADGE_GLYPH, color)),
    };
    row.child(label.into())
}

pub struct HubNote {
    glyph: Icon,
    tint: Rgba,
    text: SharedString,
    emphasized: bool,
    tooltip: Option<SharedString>,
    action: Option<AnyElement>,
}

pub fn hub_note(glyph: Icon, tint: Rgba, text: impl Into<SharedString>) -> HubNote {
    HubNote {
        glyph,
        tint,
        text: text.into(),
        emphasized: false,
        tooltip: None,
        action: None,
    }
}

impl HubNote {
    #[must_use]
    pub fn emphasized(mut self) -> Self {
        self.emphasized = true;
        self
    }

    #[must_use]
    pub fn tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    #[must_use]
    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.action = Some(action.into_any_element());
        self
    }
}

#[derive(IntoElement)]
pub struct HubCard {
    id: ElementId,
    muted: bool,
    tile: Option<AnyElement>,
    title: SharedString,
    badge: Option<AnyElement>,
    description: SharedString,
    trailing: Option<AnyElement>,
    note: Option<HubNote>,
    footer_left: Option<AnyElement>,
    footer_actions: Vec<AnyElement>,
    on_click: Option<ClickHandler>,
    palette: ForgePalette,
}

pub fn hub_card(
    id: impl Into<ElementId>,
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
    palette: &ForgePalette,
) -> HubCard {
    HubCard {
        id: id.into(),
        muted: false,
        tile: None,
        title: title.into(),
        badge: None,
        description: description.into(),
        trailing: None,
        note: None,
        footer_left: None,
        footer_actions: Vec::new(),
        on_click: None,
        palette: *palette,
    }
}

impl HubCard {
    #[must_use]
    pub fn muted(mut self, muted: bool) -> Self {
        self.muted = muted;
        self
    }

    #[must_use]
    pub fn tile(mut self, tile: impl IntoElement) -> Self {
        self.tile = Some(tile.into_any_element());
        self
    }

    #[must_use]
    pub fn badge(mut self, badge: impl IntoElement) -> Self {
        self.badge = Some(badge.into_any_element());
        self
    }

    #[must_use]
    pub fn trailing(mut self, trailing: impl IntoElement) -> Self {
        self.trailing = Some(trailing.into_any_element());
        self
    }

    #[must_use]
    pub fn note(mut self, note: Option<HubNote>) -> Self {
        self.note = note;
        self
    }

    #[must_use]
    pub fn footer_left(mut self, el: impl IntoElement) -> Self {
        self.footer_left = Some(el.into_any_element());
        self
    }

    #[must_use]
    pub fn footer_action(mut self, el: impl IntoElement) -> Self {
        self.footer_actions.push(el.into_any_element());
        self
    }

    #[must_use]
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }

    fn render_header(&mut self) -> AnyElement {
        let palette = &self.palette;
        let title_color = if self.muted {
            palette.text_secondary
        } else {
            palette.text_primary
        };
        let title_row = div()
            .flex()
            .items_center()
            .gap(TITLE_GAP)
            .mb(TITLE_MB)
            .overflow_hidden()
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(TITLE_SIZE)
                    .text_color(title_color)
                    .child(self.title.clone()),
            )
            .children(self.badge.take());
        let description = div()
            .h(DESC_LINE * DESC_LINES as f32)
            .overflow_hidden()
            .font_family(body_family())
            .text_size(DESC_SIZE)
            .line_height(DESC_LINE)
            .text_color(palette.text_muted)
            .line_clamp(DESC_LINES)
            .child(self.description.clone());
        div()
            .flex()
            .items_start()
            .gap(HEADER_GAP)
            .children(self.tile.take())
            .child(div().flex_1().min_w_0().child(title_row).child(description))
            .children(self.trailing.take())
            .into_any_element()
    }

    fn render_note(&mut self) -> AnyElement {
        let mut row = div()
            .id(ElementId::NamedChild(
                Arc::new(self.id.clone()),
                SharedString::new_static("note"),
            ))
            .h(NOTE_HEIGHT)
            .mt(NOTE_MT)
            .flex_none()
            .flex()
            .items_center()
            .gap(NOTE_GAP)
            .min_w_0();
        if let Some(note) = self.note.take() {
            let ink = if note.emphasized {
                self.palette.text_primary
            } else {
                self.palette.text_muted
            };
            if let Some(full) = note.tooltip {
                row = row.tooltip(tooltip_builder(full, &self.palette));
            }
            row = row
                .child(icon(note.glyph, NOTE_ICON, note.tint))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_family(body_family())
                        .text_size(NOTE_SIZE)
                        .text_color(ink)
                        .child(note.text),
                )
                .children(note.action);
        }
        row.into_any_element()
    }

    fn render_footer(&mut self) -> AnyElement {
        div()
            .mt_auto()
            .h(FOOTER_HEIGHT)
            .pt(FOOTER_PAD_TOP)
            .flex_none()
            .flex()
            .items_center()
            .gap(FOOTER_GAP)
            .border_t(BORDER_THIN)
            .border_color(self.palette.border_regular)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .children(self.footer_left.take()),
            )
            .children(std::mem::take(&mut self.footer_actions))
            .into_any_element()
    }
}

impl RenderOnce for HubCard {
    fn render(mut self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let header = self.render_header();
        let note = self.render_note();
        let footer = self.render_footer();
        let bg = if self.muted {
            self.palette.shell
        } else {
            self.palette.elevated
        };
        let hover_border = self.palette.border_input;
        let mut frame = div()
            .id(self.id.clone())
            .h(HUB_CARD_HEIGHT)
            .min_w_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .pt(CARD_PAD_TOP)
            .px(CARD_PAD_H)
            .pb(CARD_PAD_BOTTOM)
            .bg(bg)
            .border(BORDER_THIN)
            .border_color(self.palette.border_regular)
            .rounded(radius(Radius::Md))
            .child(header)
            .child(note)
            .child(footer);
        if let Some(handler) = self.on_click.take() {
            frame = frame
                .cursor_pointer()
                .hover(move |s| s.border_color(hover_border))
                .on_click(handler);
        }
        frame
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CompactTone {
    Ghost,
    Primary,
}

#[derive(IntoElement)]
pub struct CompactButton {
    id: ElementId,
    tone: CompactTone,
    glyph: Icon,
    label: Option<SharedString>,
    tooltip: Option<SharedString>,
    on_click: Option<ClickHandler>,
    palette: ForgePalette,
}

pub fn compact_button(
    id: impl Into<ElementId>,
    tone: CompactTone,
    glyph: Icon,
    palette: &ForgePalette,
) -> CompactButton {
    CompactButton {
        id: id.into(),
        tone,
        glyph,
        label: None,
        tooltip: None,
        on_click: None,
        palette: *palette,
    }
}

impl CompactButton {
    #[must_use]
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    #[must_use]
    pub fn tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    #[must_use]
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for CompactButton {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let palette = self.palette;
        let (ink, glyph_size, pad_v, pad_h) = match (self.tone, self.label.is_some()) {
            (CompactTone::Primary, _) => {
                (palette.shell, COMPACT_ICON, COMPACT_PAD_V, PRIMARY_PAD_H)
            }
            (CompactTone::Ghost, true) => (
                palette.text_secondary,
                COMPACT_ICON,
                COMPACT_PAD_V,
                GHOST_PAD_H,
            ),
            (CompactTone::Ghost, false) => (
                palette.text_secondary,
                COMPACT_ICON_ONLY,
                ICON_ONLY_PAD_V,
                ICON_ONLY_PAD_H,
            ),
        };
        let mut button = div()
            .id(self.id)
            .flex_none()
            .flex()
            .items_center()
            .gap(COMPACT_GAP)
            .py(pad_v)
            .px(pad_h)
            .rounded(radius(Radius::Sm))
            .cursor_pointer()
            .font_family(body_family())
            .text_size(COMPACT_SIZE)
            .text_color(ink)
            .child(icon(self.glyph, glyph_size, ink));
        button = match self.tone {
            CompactTone::Primary => button.bg(palette.brand).font_weight(FontWeight::MEDIUM),
            CompactTone::Ghost => {
                let hover_border = palette.border_input;
                let hover_ink = palette.text_primary;
                button
                    .border(BORDER_THIN)
                    .border_color(palette.border_regular)
                    .hover(move |s| s.border_color(hover_border).text_color(hover_ink))
            }
        };
        if let Some(label) = self.label {
            button = button.child(label);
        }
        if let Some(text) = self.tooltip {
            button = button.tooltip(tooltip_builder(text, &palette));
        }
        if let Some(handler) = self.on_click {
            button = button.on_click(move |event, window, cx| {
                cx.stop_propagation();
                handler(event, window, cx);
            });
        }
        button
    }
}

pub fn hub_section_header(
    label: impl Into<SharedString>,
    blurb: impl Into<SharedString>,
    count: Option<SharedString>,
    palette: &ForgePalette,
) -> impl IntoElement {
    let label: SharedString = label.into();
    div()
        .flex()
        .items_center()
        .gap(SECTION_GAP)
        .mb(SECTION_MB)
        .child(
            div()
                .flex_none()
                .font_family(mono_family())
                .text_size(SECTION_LABEL_SIZE)
                .text_color(palette.text_muted)
                .child(SharedString::from(label.to_uppercase())),
        )
        .child(
            div()
                .flex_none()
                .font_family(body_family())
                .text_size(SECTION_BLURB_SIZE)
                .text_color(palette.text_faint)
                .child(blurb.into()),
        )
        .child(div().flex_1().h(BORDER_THIN).bg(palette.border_regular))
        .children(count.map(|count| {
            div()
                .flex_none()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(count)
        }))
}
