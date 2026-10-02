use gpui::{
    AnyElement, App, ClickEvent, Div, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Pixels, RenderOnce, Rgba, SharedString, StatefulInteractiveElement, Styled,
    Window, div, px,
};

use crate::icons::{Icon, icon};
use crate::palette::{ForgePalette, with_alpha};
use crate::tokens::{BORDER_THIN, Radius, body_family, mono_family, radius};

const HEALTH_TILE: Pixels = px(18.0);
const HEALTH_CORNER: Pixels = px(5.0);
const HEALTH_GLYPH: Pixels = px(12.0);
const HEALTH_TILE_ALPHA: f32 = 0.14;

const WARN_FILL_ALPHA: f32 = 0.06;
const WARN_BORDER_ALPHA: f32 = 0.2;

const INACTIVE_SIZE: Pixels = px(9.5);
const INACTIVE_GLYPH: Pixels = px(10.0);
const INACTIVE_GAP: Pixels = px(4.0);
const INACTIVE_PAD_V: Pixels = px(1.0);
const INACTIVE_PAD_H: Pixels = px(6.0);

const REASON_SIZE: Pixels = px(10.0);
const REASON_GLYPH: Pixels = px(10.0);
const REASON_GAP: Pixels = px(4.0);
const REASON_PAD_V: Pixels = px(1.0);
const REASON_PAD_H: Pixels = px(7.0);
const REASON_FILL_ALPHA: f32 = 0.12;

const NOTICE_GAP: Pixels = px(8.0);
const NOTICE_PAD_V: Pixels = px(5.0);
const NOTICE_PAD_LEFT: Pixels = px(10.0);
const NOTICE_PAD_RIGHT: Pixels = px(6.0);
const NOTICE_GLYPH: Pixels = px(12.0);
const NOTICE_TEXT: Pixels = px(11.5);
const ENABLE_SIZE: Pixels = px(11.0);
const ENABLE_GLYPH: Pixels = px(11.0);
const ENABLE_GAP: Pixels = px(5.0);
const ENABLE_PAD_V: Pixels = px(2.0);
const ENABLE_PAD_H: Pixels = px(9.0);

type EnableHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub fn health_tile(glyph: Icon, tint: Rgba) -> Div {
    div()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(HEALTH_TILE)
        .rounded(HEALTH_CORNER)
        .bg(with_alpha(tint, HEALTH_TILE_ALPHA))
        .child(icon(glyph, HEALTH_GLYPH, tint))
}

pub fn integration_inactive_badge(
    label: impl Into<SharedString>,
    palette: &ForgePalette,
) -> impl IntoElement {
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(INACTIVE_GAP)
        .py(INACTIVE_PAD_V)
        .px(INACTIVE_PAD_H)
        .whitespace_nowrap()
        .rounded(radius(Radius::Sm))
        .bg(with_alpha(palette.warning, WARN_FILL_ALPHA))
        .border(BORDER_THIN)
        .border_color(with_alpha(palette.warning, WARN_BORDER_ALPHA))
        .font_family(mono_family())
        .font_weight(FontWeight::MEDIUM)
        .text_size(INACTIVE_SIZE)
        .text_color(palette.warning)
        .child(icon(Icon::PlugOff, INACTIVE_GLYPH, palette.warning))
        .child(label.into())
}

pub fn integration_disabled_reason(
    label: impl Into<SharedString>,
    palette: &ForgePalette,
) -> impl IntoElement {
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(REASON_GAP)
        .py(REASON_PAD_V)
        .px(REASON_PAD_H)
        .whitespace_nowrap()
        .rounded(radius(Radius::Sm))
        .bg(with_alpha(palette.random, REASON_FILL_ALPHA))
        .font_family(mono_family())
        .font_weight(FontWeight::MEDIUM)
        .text_size(REASON_SIZE)
        .text_color(palette.random)
        .child(icon(Icon::PlugOff, REASON_GLYPH, palette.random))
        .child(label.into())
}

#[derive(IntoElement)]
pub struct IntegrationDisabledNotice {
    id: ElementId,
    message: AnyElement,
    enable_label: SharedString,
    on_enable: Option<EnableHandler>,
    palette: ForgePalette,
}

pub fn integration_disabled_notice(
    id: impl Into<ElementId>,
    message: impl IntoElement,
    enable_label: impl Into<SharedString>,
    palette: &ForgePalette,
) -> IntegrationDisabledNotice {
    IntegrationDisabledNotice {
        id: id.into(),
        message: message.into_any_element(),
        enable_label: enable_label.into(),
        on_enable: None,
        palette: *palette,
    }
}

impl IntegrationDisabledNotice {
    pub fn on_enable(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_enable = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for IntegrationDisabledNotice {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let palette = self.palette;
        let hover_border = palette.border_input;
        let mut enable = div()
            .id(self.id.clone())
            .flex_none()
            .flex()
            .items_center()
            .gap(ENABLE_GAP)
            .py(ENABLE_PAD_V)
            .px(ENABLE_PAD_H)
            .whitespace_nowrap()
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .cursor_pointer()
            .hover(move |s| s.border_color(hover_border))
            .font_family(body_family())
            .text_size(ENABLE_SIZE)
            .text_color(palette.text_primary)
            .child(icon(Icon::Power, ENABLE_GLYPH, palette.success))
            .child(self.enable_label);
        if let Some(handler) = self.on_enable {
            enable = enable.on_click(move |event, window, cx| {
                cx.stop_propagation();
                handler(event, window, cx);
            });
        }
        div()
            .id(ElementId::Name(format!("{}-notice", self.id).into()))
            .w_full()
            .flex()
            .items_center()
            .gap(NOTICE_GAP)
            .py(NOTICE_PAD_V)
            .pl(NOTICE_PAD_LEFT)
            .pr(NOTICE_PAD_RIGHT)
            .rounded(radius(Radius::Sm))
            .bg(with_alpha(palette.warning, WARN_FILL_ALPHA))
            .border(BORDER_THIN)
            .border_color(with_alpha(palette.warning, WARN_BORDER_ALPHA))
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(icon(Icon::PlugOff, NOTICE_GLYPH, palette.warning))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .font_family(body_family())
                    .text_size(NOTICE_TEXT)
                    .text_color(palette.text_secondary)
                    .child(self.message),
            )
            .child(enable)
    }
}
