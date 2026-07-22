use gpui::{
    App, ClickEvent, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement, Pixels,
    RenderOnce, Rgba, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::palette::{ForgePalette, with_alpha};
use crate::tokens::{BORDER_THIN, DEFAULT_BODY_FAMILY};

type SegClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

const SEG_PAD_V: Pixels = px(5.0);
const SEG_PAD_H: Pixels = px(11.0);
const SEG_FS: Pixels = px(11.0);
const SEG_RADIUS: Pixels = px(5.0);
const GROUP_RADIUS: Pixels = px(7.0);
const GROUP_PAD: Pixels = px(2.0);
const HOVER_ALPHA: f32 = 0.06;

pub struct Segment {
    id: ElementId,
    label: SharedString,
    active: bool,
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
        on_click: Box::new(handler),
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
    hover_bg: Rgba,
    seg_radius: Pixels,
    seg_pad_x: Pixels,
    seg_pad_y: Pixels,
    seg_fs: Pixels,
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
        hover_bg: with_alpha(palette.border_regular, HOVER_ALPHA),
        seg_radius: SEG_RADIUS,
        seg_pad_x: SEG_PAD_H,
        seg_pad_y: SEG_PAD_V,
        seg_fs: SEG_FS,
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
            let fg = if seg.active {
                self.active_fg
            } else {
                self.inactive_fg
            };
            let weight = if seg.active {
                FontWeight::MEDIUM
            } else {
                FontWeight::NORMAL
            };
            let mut chip = div()
                .id(seg.id)
                .py(self.seg_pad_y)
                .px(self.seg_pad_x)
                .rounded(self.seg_radius)
                .cursor_pointer()
                .font_family(DEFAULT_BODY_FAMILY)
                .font_weight(weight)
                .text_size(self.seg_fs)
                .text_color(fg)
                .on_click(seg.on_click)
                .child(seg.label);
            if seg.active {
                chip = chip.bg(self.active_bg);
            } else {
                let hover = self.hover_bg;
                chip = chip.hover(move |s| s.bg(hover));
            }
            container = container.child(chip);
        }

        container
    }
}
