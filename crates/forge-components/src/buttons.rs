use crate::HOVER_FILL_ALPHA;
use std::sync::Arc;

use gpui::{
    AnyElement, App, ClickEvent, Div, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Pixels, RenderOnce, Rgba, SharedString, StatefulInteractiveElement, Styled,
    Window, div, px, svg,
};

use crate::icons::{Icon, icon, spinner};
use crate::key_activation::{KeyActivated, SHOWN_OPACITY};
use crate::palette::{ForgePalette, with_alpha};
use crate::presentation::ActivePresentation;
use crate::tokens::{
    BORDER_THIN, DISABLED_OPACITY, Density, FONT_MD, FONT_XS, Radius, Spacing, radius, scaled,
    spacing,
};
use crate::tooltip::tooltip_builder;

type ButtonClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

const ICON_LABEL_GAP: Pixels = px(5.0);
const LABEL_PAD_H: Pixels = px(12.0);
const ICON_BUTTON_GROUP: &str = "forge-icon-button";

const FILL_DISABLED_ALPHA: f32 = 0.4;
const INK_DISABLED_ALPHA: f32 = 0.5;
const OUTLINE_DISABLED_ALPHA: f32 = 0.4;
const SECONDARY_HOVER_WASH: f32 = 0.06;
const ICON_HOVER_WASH: f32 = 0.08;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ButtonVariant {
    Primary,
    Destructive,
    Secondary,
    Ghost,
    Icon,
}

#[derive(Clone, Copy)]
pub(crate) struct ButtonColors {
    pub(crate) fill: Option<Rgba>,
    pub(crate) text: Rgba,
    pub(crate) border: Option<Rgba>,
    pub(crate) hover_fill: Option<Rgba>,
    pub(crate) hover_text: Rgba,
    pub(crate) hover_border: Option<Rgba>,
    pub(crate) disabled_fill: Option<Rgba>,
    pub(crate) disabled_text: Rgba,
    pub(crate) disabled_border: Option<Rgba>,
}

impl ButtonVariant {
    pub(crate) fn colors(self, p: &ForgePalette) -> ButtonColors {
        match self {
            ButtonVariant::Primary => filled(p.brand, p.shell),
            ButtonVariant::Destructive => filled(p.danger, p.shell),
            ButtonVariant::Secondary => ButtonColors {
                fill: None,
                text: p.text_secondary,
                border: Some(p.border_regular),
                hover_fill: Some(with_alpha(p.border_regular, SECONDARY_HOVER_WASH)),
                hover_text: p.text_primary,
                hover_border: Some(p.border_regular),
                disabled_fill: None,
                disabled_text: with_alpha(p.text_secondary, OUTLINE_DISABLED_ALPHA),
                disabled_border: Some(with_alpha(p.border_regular, OUTLINE_DISABLED_ALPHA)),
            },
            ButtonVariant::Ghost => ButtonColors {
                fill: None,
                text: p.text_secondary,
                border: Some(p.border_regular),
                hover_fill: None,
                hover_text: p.text_primary,
                hover_border: Some(p.border_input),
                disabled_fill: None,
                disabled_text: with_alpha(p.text_secondary, OUTLINE_DISABLED_ALPHA),
                disabled_border: Some(with_alpha(p.border_regular, OUTLINE_DISABLED_ALPHA)),
            },
            ButtonVariant::Icon => ButtonColors {
                fill: None,
                text: p.text_secondary,
                border: None,
                hover_fill: Some(with_alpha(p.brand, ICON_HOVER_WASH)),
                hover_text: p.text_primary,
                hover_border: None,
                disabled_fill: None,
                disabled_text: with_alpha(p.text_secondary, OUTLINE_DISABLED_ALPHA),
                disabled_border: None,
            },
        }
    }
}

fn filled(hue: Rgba, ink: Rgba) -> ButtonColors {
    ButtonColors {
        fill: Some(hue),
        text: ink,
        border: None,
        hover_fill: Some(with_alpha(hue, HOVER_FILL_ALPHA)),
        hover_text: ink,
        hover_border: None,
        disabled_fill: Some(with_alpha(hue, FILL_DISABLED_ALPHA)),
        disabled_text: with_alpha(ink, INK_DISABLED_ALPHA),
        disabled_border: None,
    }
}

#[derive(IntoElement)]
pub struct Button {
    variant: ButtonVariant,
    label: Option<SharedString>,
    leading: Option<Icon>,
    trailing: Option<Icon>,
    weight: FontWeight,
    colors: ButtonColors,
    density: Option<Density>,
    disabled: bool,
    busy: bool,
    full_width: bool,
    fits_parent: bool,
    grows: bool,
    dims_when_disabled: bool,
    height: Option<Pixels>,
    square: Option<Pixels>,
    padding: Option<(Pixels, Pixels)>,
    gap: Option<Pixels>,
    text_size: Pixels,
    glyph_size: Option<Pixels>,
    corner: Radius,
    focus_ring: Option<Rgba>,
    tooltip: Option<SharedString>,
    reveal_group: Option<SharedString>,
    id: Option<ElementId>,
    on_click: Option<ButtonClick>,
}

impl Button {
    fn new(
        variant: ButtonVariant,
        label: Option<SharedString>,
        leading: Option<Icon>,
        trailing: Option<Icon>,
        weight: FontWeight,
        palette: &ForgePalette,
    ) -> Self {
        Button {
            variant,
            label,
            leading,
            trailing,
            weight,
            colors: variant.colors(palette),
            density: None,
            disabled: false,
            busy: false,
            full_width: false,
            fits_parent: false,
            grows: false,
            dims_when_disabled: false,
            height: None,
            square: None,
            padding: None,
            gap: None,
            text_size: FONT_XS,
            glyph_size: None,
            corner: Radius::Sm,
            focus_ring: None,
            tooltip: None,
            reveal_group: None,
            id: None,
            on_click: None,
        }
    }

    #[must_use]
    pub fn height(mut self, height: Pixels) -> Self {
        self.height = Some(height);
        self
    }

    #[must_use]
    pub fn density(mut self, density: Density) -> Self {
        self.density = Some(density);
        self
    }

    #[must_use]
    pub fn full_width(mut self) -> Self {
        self.full_width = true;
        self
    }

    #[must_use]
    pub fn fits_parent(mut self) -> Self {
        self.fits_parent = true;
        self
    }

    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    #[must_use]
    pub fn busy(mut self, busy: bool) -> Self {
        self.busy = busy;
        self
    }

    #[must_use]
    pub fn ink(mut self, color: Rgba) -> Self {
        self.colors.text = color;
        self.colors.hover_text = color;
        self
    }

    #[must_use]
    pub fn fill(mut self, hue: Rgba, ink: Rgba) -> Self {
        self.colors = filled(hue, ink);
        self
    }

    #[must_use]
    pub fn hover_fill(mut self, color: Rgba) -> Self {
        self.colors.hover_fill = Some(color);
        self
    }

    #[must_use]
    pub fn disabled_tones(mut self, fill: Rgba, ink: Rgba) -> Self {
        self.colors.disabled_fill = Some(fill);
        self.colors.disabled_text = ink;
        self
    }

    #[must_use]
    pub fn dims_when_disabled(mut self) -> Self {
        self.dims_when_disabled = true;
        self
    }

    #[must_use]
    pub fn weight(mut self, weight: FontWeight) -> Self {
        self.weight = weight;
        self
    }

    #[must_use]
    pub fn padding(mut self, vertical: Pixels, horizontal: Pixels) -> Self {
        self.padding = Some((vertical, horizontal));
        self
    }

    #[must_use]
    pub fn gap(mut self, gap: Pixels) -> Self {
        self.gap = Some(gap);
        self
    }

    #[must_use]
    pub fn text_size(mut self, size: Pixels) -> Self {
        self.text_size = size;
        self
    }

    #[must_use]
    pub fn glyph_size(mut self, size: Pixels) -> Self {
        self.glyph_size = Some(size);
        self
    }

    #[must_use]
    pub fn corner(mut self, corner: Radius) -> Self {
        self.corner = corner;
        self
    }

    #[must_use]
    pub fn grows(mut self) -> Self {
        self.grows = true;
        self
    }

    #[must_use]
    pub fn square(mut self, side: Pixels) -> Self {
        self.square = Some(side);
        self.padding = Some((px(0.0), px(0.0)));
        self
    }

    #[must_use]
    pub fn focus_ring(mut self, color: Rgba) -> Self {
        self.focus_ring = Some(color);
        self
    }

    #[must_use]
    pub fn tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    #[must_use]
    pub fn revealed_by(mut self, group: impl Into<SharedString>) -> Self {
        self.reveal_group = Some(group.into());
        self
    }

    #[must_use]
    pub fn label(mut self, label: Option<SharedString>) -> Self {
        self.label = label;
        self
    }

    pub fn on_click(
        mut self,
        id: impl Into<ElementId>,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.id = Some(id.into());
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl Button {
    fn default_gap(&self, density: Density) -> Pixels {
        if self.busy || self.leading.is_some() {
            scaled(ICON_LABEL_GAP, density)
        } else if self.trailing.is_some() {
            spacing(Spacing::Xs, density)
        } else {
            px(0.0)
        }
    }

    fn resolved_padding(&self, d: Density) -> (Pixels, Pixels) {
        self.padding.unwrap_or_else(|| {
            if self.variant == ButtonVariant::Icon {
                (spacing(Spacing::Xs, d), spacing(Spacing::Xs, d))
            } else {
                (spacing(Spacing::Xxs, d), scaled(LABEL_PAD_H, d))
            }
        })
    }

    fn sized(&self, mut root: Div) -> Div {
        if self.grows {
            root = root.flex_1().justify_center();
        } else if self.full_width {
            root = root.w_full().justify_start();
        } else {
            root = root.flex_none();
        }
        if self.fits_parent {
            root = root.max_w_full();
        }
        if let Some(h) = self.height {
            root = root.h(h);
        }
        if let Some(side) = self.square {
            root = root.size(side).justify_center();
        }
        root
    }

    fn busy_clock(&self) -> ElementId {
        self.id
            .clone()
            .map(|id| ElementId::NamedChild(Arc::new(id), SharedString::new_static("busy")))
            .unwrap_or_else(|| {
                let label = self.label.as_deref().unwrap_or_default();
                ElementId::Name(format!("forge-button-busy-{label}").into())
            })
    }

    fn interactive(self, root: Div, text: Rgba) -> AnyElement {
        let c = self.colors;
        let hover_fill = c.hover_fill;
        let hover_text = c.hover_text;
        let hover_border = c.hover_border;
        let reveals = self.reveal_group.is_some();
        let root = root.hover(move |mut style| {
            if reveals {
                style = style.opacity(SHOWN_OPACITY);
            }
            if let Some(fill) = hover_fill {
                style = style.bg(fill);
            }
            style = style.text_color(hover_text);
            if let Some(border) = hover_border {
                style = style.border_color(border);
            }
            style
        });

        let Some(id) = self.id else {
            return root.into_any_element();
        };
        let mut root = root.debug_selector(|| id.to_string()).id(id);
        if let Some(text) = self.tooltip {
            root = root.tooltip(tooltip_builder(text));
        }
        match self.on_click {
            Some(handler) => {
                let ring = self.focus_ring.unwrap_or(text);
                let root = match self.reveal_group {
                    Some(group) => root.key_activated_revealed_by(group, ring),
                    None => root.key_activated(ring),
                };
                root.cursor_pointer().on_click(handler).into_any_element()
            }
            None => root.into_any_element(),
        }
    }
}

impl RenderOnce for Button {
    fn render(mut self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = self.colors;
        let d = self.density.unwrap_or_else(|| cx.density());
        let icon_only = self.variant == ButtonVariant::Icon;
        let disabled = self.disabled || self.busy;

        let (fill, text, border) = if disabled && !self.dims_when_disabled {
            (c.disabled_fill, c.disabled_text, c.disabled_border)
        } else {
            (c.fill, c.text, c.border)
        };

        let gap = self.gap.unwrap_or_else(|| self.default_gap(d));
        let (pad_v, pad_h) = self.resolved_padding(d);
        let glyph_size = self
            .glyph_size
            .unwrap_or(if icon_only { FONT_MD } else { FONT_XS });

        let root = div()
            .flex()
            .items_center()
            .gap(gap)
            .py(pad_v)
            .px(pad_h)
            .rounded(radius(self.corner))
            .font_family(cx.body_family())
            .font_weight(self.weight)
            .text_size(self.text_size)
            .text_color(text);
        let mut root = self.sized(root);

        if let Some(fill) = fill {
            root = root.bg(fill);
        }
        if let Some(border) = border {
            root = root.border(BORDER_THIN).border_color(border);
        }

        if self.busy {
            root = root.child(spinner(self.busy_clock(), Icon::Loader2, glyph_size, text));
        } else if let Some(leading) = self.leading {
            root = if icon_only && !disabled {
                let hover_text = c.hover_text;
                root.group(ICON_BUTTON_GROUP).child(
                    svg()
                        .flex_none()
                        .size(glyph_size)
                        .path(leading.path())
                        .text_color(text)
                        .group_hover(ICON_BUTTON_GROUP, move |s| s.text_color(hover_text)),
                )
            } else {
                root.child(icon(leading, glyph_size, text))
            };
        }
        if let Some(label) = self.label.take() {
            root = root.child(div().child(label));
        }
        if let Some(trailing) = self.trailing {
            root = root.child(icon(trailing, glyph_size, text));
        }

        if disabled {
            if self.dims_when_disabled {
                root = root.opacity(DISABLED_OPACITY);
            }
            return root.into_any_element();
        }

        self.interactive(root, text)
    }
}

pub fn primary_button(label: impl Into<SharedString>, palette: &ForgePalette) -> Button {
    Button::new(
        ButtonVariant::Primary,
        Some(label.into()),
        None,
        None,
        FontWeight::NORMAL,
        palette,
    )
}

pub fn primary_button_with_icon(
    icon: Icon,
    label: impl Into<SharedString>,
    palette: &ForgePalette,
) -> Button {
    Button::new(
        ButtonVariant::Primary,
        Some(label.into()),
        Some(icon),
        None,
        FontWeight::SEMIBOLD,
        palette,
    )
}

pub fn destructive_button(label: impl Into<SharedString>, palette: &ForgePalette) -> Button {
    Button::new(
        ButtonVariant::Destructive,
        Some(label.into()),
        None,
        None,
        FontWeight::NORMAL,
        palette,
    )
}

pub fn destructive_button_with_icon(
    icon: Icon,
    label: impl Into<SharedString>,
    palette: &ForgePalette,
) -> Button {
    Button::new(
        ButtonVariant::Destructive,
        Some(label.into()),
        Some(icon),
        None,
        FontWeight::SEMIBOLD,
        palette,
    )
}

pub fn secondary_button(label: impl Into<SharedString>, palette: &ForgePalette) -> Button {
    Button::new(
        ButtonVariant::Secondary,
        Some(label.into()),
        None,
        None,
        FontWeight::NORMAL,
        palette,
    )
}

pub fn ghost_button(label: impl Into<SharedString>, palette: &ForgePalette) -> Button {
    Button::new(
        ButtonVariant::Ghost,
        Some(label.into()),
        None,
        None,
        FontWeight::NORMAL,
        palette,
    )
}

pub fn ghost_button_with_icon(
    icon: Icon,
    label: impl Into<SharedString>,
    palette: &ForgePalette,
) -> Button {
    Button::new(
        ButtonVariant::Ghost,
        Some(label.into()),
        Some(icon),
        None,
        FontWeight::NORMAL,
        palette,
    )
}

pub fn icon_button(icon: Icon, palette: &ForgePalette) -> Button {
    Button::new(
        ButtonVariant::Icon,
        None,
        Some(icon),
        None,
        FontWeight::NORMAL,
        palette,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::FORGE_DEFAULT as P;

    const EPS: f32 = 1e-6;

    type Slot = Option<(Rgba, f32)>;

    fn same_rgb(a: Rgba, b: Rgba) -> bool {
        (a.r - b.r).abs() < EPS && (a.g - b.g).abs() < EPS && (a.b - b.b).abs() < EPS
    }

    fn assert_slot(actual: Option<Rgba>, expected: Slot, label: &str) {
        match (actual, expected) {
            (None, None) => {}
            (Some(got), Some((hue, alpha))) => {
                assert!(
                    same_rgb(got, hue),
                    "{label}: hue rgb mismatch - got ({},{},{}), want ({},{},{})",
                    got.r,
                    got.g,
                    got.b,
                    hue.r,
                    hue.g,
                    hue.b,
                );
                assert!(
                    (got.a - alpha).abs() < EPS,
                    "{label}: alpha mismatch - got {}, want {alpha}",
                    got.a,
                );
            }
            _ => panic!("{label}: presence mismatch - got {actual:?}, want {expected:?}"),
        }
    }

    #[test]
    fn rest_colors_resolve_per_variant() {
        let cases: [(ButtonVariant, Slot, Slot, Slot); 5] = [
            (
                ButtonVariant::Primary,
                Some((P.brand, 1.0)),
                Some((P.shell, 1.0)),
                None,
            ),
            (
                ButtonVariant::Destructive,
                Some((P.danger, 1.0)),
                Some((P.shell, 1.0)),
                None,
            ),
            (
                ButtonVariant::Secondary,
                None,
                Some((P.text_secondary, 1.0)),
                Some((P.border_regular, 1.0)),
            ),
            (
                ButtonVariant::Ghost,
                None,
                Some((P.text_secondary, 1.0)),
                Some((P.border_regular, 1.0)),
            ),
            (
                ButtonVariant::Icon,
                None,
                Some((P.text_secondary, 1.0)),
                None,
            ),
        ];
        for (variant, fill, ink, border) in cases {
            let c = variant.colors(&P);
            assert_slot(c.fill, fill, &format!("{variant:?} rest fill"));
            assert_slot(Some(c.text), ink, &format!("{variant:?} rest ink"));
            assert_slot(c.border, border, &format!("{variant:?} rest border"));
        }
    }

    #[test]
    fn hover_colors_resolve_per_variant() {
        let cases: [(ButtonVariant, Slot, Slot, Slot); 5] = [
            (
                ButtonVariant::Primary,
                Some((P.brand, 0.92)),
                Some((P.shell, 1.0)),
                None,
            ),
            (
                ButtonVariant::Destructive,
                Some((P.danger, 0.92)),
                Some((P.shell, 1.0)),
                None,
            ),
            (
                ButtonVariant::Secondary,
                Some((P.border_regular, 0.06)),
                Some((P.text_primary, 1.0)),
                Some((P.border_regular, 1.0)),
            ),
            (
                ButtonVariant::Ghost,
                None,
                Some((P.text_primary, 1.0)),
                Some((P.border_input, 1.0)),
            ),
            (
                ButtonVariant::Icon,
                Some((P.brand, 0.08)),
                Some((P.text_primary, 1.0)),
                None,
            ),
        ];
        for (variant, fill, ink, border) in cases {
            let c = variant.colors(&P);
            assert_slot(c.hover_fill, fill, &format!("{variant:?} hover fill"));
            assert_slot(Some(c.hover_text), ink, &format!("{variant:?} hover ink"));
            assert_slot(c.hover_border, border, &format!("{variant:?} hover border"));
        }
    }

    #[test]
    fn disabled_colors_resolve_per_variant() {
        let cases: [(ButtonVariant, Slot, Slot, Slot); 5] = [
            (
                ButtonVariant::Primary,
                Some((P.brand, 0.4)),
                Some((P.shell, 0.5)),
                None,
            ),
            (
                ButtonVariant::Destructive,
                Some((P.danger, 0.4)),
                Some((P.shell, 0.5)),
                None,
            ),
            (
                ButtonVariant::Secondary,
                None,
                Some((P.text_secondary, 0.4)),
                Some((P.border_regular, 0.4)),
            ),
            (
                ButtonVariant::Ghost,
                None,
                Some((P.text_secondary, 0.4)),
                Some((P.border_regular, 0.4)),
            ),
            (
                ButtonVariant::Icon,
                None,
                Some((P.text_secondary, 0.4)),
                None,
            ),
        ];
        for (variant, fill, ink, border) in cases {
            let c = variant.colors(&P);
            assert_slot(c.disabled_fill, fill, &format!("{variant:?} disabled fill"));
            assert_slot(
                Some(c.disabled_text),
                ink,
                &format!("{variant:?} disabled ink"),
            );
            assert_slot(
                c.disabled_border,
                border,
                &format!("{variant:?} disabled border"),
            );
        }
    }
}
