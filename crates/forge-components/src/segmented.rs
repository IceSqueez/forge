use gpui::{
    App, ClickEvent, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement, Pixels,
    RenderOnce, Rgba, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::icons::{Icon, icon};
use crate::palette::{ForgePalette, with_alpha};
use crate::tokens::{BORDER_THIN, body_family};

type SegClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

const SEG_PAD_V: Pixels = px(5.0);
const SEG_PAD_H: Pixels = px(11.0);
const SEG_FS: Pixels = px(11.0);
const SEG_RADIUS: Pixels = px(5.0);
const GROUP_RADIUS: Pixels = px(7.0);
const GROUP_PAD: Pixels = px(2.0);
const SEG_GLYPH_GAP: Pixels = px(5.0);
const HOVER_ALPHA: f32 = 0.06;
const DISABLED_OPACITY: f32 = 0.5;

pub struct Segment {
    id: ElementId,
    label: SharedString,
    active: bool,
    disabled: bool,
    glyph: Option<Icon>,
    on_click: SegClick,
}

pub fn segment(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    active: bool,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Segment {
    Segment {
        id: id.into(),
        label: label.into(),
        active,
        disabled: false,
        glyph: None,
        on_click: Box::new(handler),
    }
}

impl Segment {
    #[must_use]
    pub fn icon(mut self, glyph: Icon) -> Self {
        self.glyph = Some(glyph);
        self
    }

    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

#[derive(IntoElement)]
pub struct SegmentedControl {
    segments: Vec<Segment>,
    joined: bool,
    wrap_gap: Pixels,
    group_radius: Pixels,
    group_pad: Pixels,
    group_border: Rgba,
    group_bg: Rgba,
    active_bg: Rgba,
    active_fg: Rgba,
    inactive_fg: Rgba,
    disabled_fg: Rgba,
    hover_bg: Rgba,
    seg_radius: Pixels,
    seg_pad_x: Pixels,
    seg_pad_y: Pixels,
    seg_fs: Pixels,
    glyph_active: Rgba,
    glyph_idle: Rgba,
}

pub fn segmented(segments: Vec<Segment>, palette: &ForgePalette) -> SegmentedControl {
    SegmentedControl {
        segments,
        joined: true,
        wrap_gap: px(0.0),
        group_radius: GROUP_RADIUS,
        group_pad: GROUP_PAD,
        group_border: palette.surface_overlay,
        group_bg: palette.shell,
        active_bg: palette.brand,
        active_fg: palette.shell,
        inactive_fg: palette.text_secondary,
        disabled_fg: palette.text_faint,
        hover_bg: with_alpha(palette.border_regular, HOVER_ALPHA),
        seg_radius: SEG_RADIUS,
        seg_pad_x: SEG_PAD_H,
        seg_pad_y: SEG_PAD_V,
        seg_fs: SEG_FS,
        glyph_active: palette.brand,
        glyph_idle: palette.text_faint,
    }
}

impl SegmentedControl {
    /// Drops the joined-pill container in favor of a bare wrapping row of chip-chrome segments.
    #[must_use]
    pub fn wrap(mut self, gap: Pixels) -> Self {
        self.joined = false;
        self.wrap_gap = gap;
        self
    }

    /// Marks the active segment with a raised surface instead of the brand fill, for a switch that
    /// sits inside a toolbar rather than acting as the surface's primary control.
    #[must_use]
    pub fn subtle(mut self, palette: &ForgePalette) -> Self {
        self.active_bg = palette.surface_overlay;
        self.active_fg = palette.text_primary;
        self
    }

    /// Tints the active segment's glyph; inactive glyphs stay faint.
    #[must_use]
    pub fn accent(mut self, accent: Rgba) -> Self {
        self.glyph_active = accent;
        self
    }
}

impl RenderOnce for SegmentedControl {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let mut container = if self.joined {
            div()
                .flex()
                .flex_row()
                .p(self.group_pad)
                .rounded(self.group_radius)
                .border(BORDER_THIN)
                .border_color(self.group_border)
                .bg(self.group_bg)
        } else {
            div().flex().flex_row().flex_wrap().gap(self.wrap_gap)
        };

        for seg in self.segments {
            let fg = match (seg.disabled, seg.active) {
                (true, _) => self.disabled_fg,
                (false, true) => self.active_fg,
                (false, false) => self.inactive_fg,
            };
            let weight = if seg.active {
                FontWeight::MEDIUM
            } else {
                FontWeight::NORMAL
            };
            let glyph = seg.glyph.map(|glyph| {
                let tint = match (seg.disabled, seg.active) {
                    (true, _) => self.disabled_fg,
                    (false, true) => self.glyph_active,
                    (false, false) => self.glyph_idle,
                };
                icon(glyph, self.seg_fs, tint)
            });
            let mut chip = div()
                .id(seg.id)
                .flex()
                .items_center()
                .gap(SEG_GLYPH_GAP)
                .py(self.seg_pad_y)
                .px(self.seg_pad_x)
                .rounded(self.seg_radius)
                .font_family(body_family())
                .font_weight(weight)
                .text_size(self.seg_fs)
                .text_color(fg)
                .children(glyph)
                .child(seg.label);
            if seg.active {
                chip = chip.bg(self.active_bg);
            } else if !seg.disabled {
                let hover = self.hover_bg;
                chip = chip.hover(move |s| s.bg(hover));
            }
            if seg.disabled {
                chip = chip.opacity(DISABLED_OPACITY);
            } else {
                chip = chip.cursor_pointer().on_click(seg.on_click);
            }
            container = container.child(chip);
        }

        container
    }
}
