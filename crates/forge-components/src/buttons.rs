use gpui::{
    App, ClickEvent, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement, Pixels,
    RenderOnce, Rgba, SharedString, StatefulInteractiveElement, Styled, Window, div, px, svg,
};

use crate::icons::{Icon, icon};
use crate::palette::{ForgePalette, with_alpha};
use crate::tokens::{
    BORDER_THIN, Density, FONT_MD, FONT_XS, Radius, Spacing, body_family, radius, spacing,
};

type ButtonClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

const ICON_LABEL_GAP: Pixels = px(5.0);
const ICON_BUTTON_GROUP: &str = "forge-icon-button";

const FILL_HOVER_ALPHA: f32 = 0.92;
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
            ButtonVariant::Destructive => filled(p.random, p.shell),
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
        hover_fill: Some(with_alpha(hue, FILL_HOVER_ALPHA)),
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
    density: Density,
    disabled: bool,
    full_width: bool,
    height: Option<Pixels>,
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
            density: Density::default(),
            disabled: false,
            full_width: false,
            height: None,
            id: None,
            on_click: None,
        }
    }

    pub fn height(mut self, height: Pixels) -> Self {
        self.height = Some(height);
        self
    }

    pub fn density(mut self, density: Density) -> Self {
        self.density = density;
        self
    }

    pub fn full_width(mut self) -> Self {
        self.full_width = true;
        self
    }

    /// Suppresses hover feedback and click handling even when [`Button::on_click`] is set.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Overrides resting and hover ink so an outline variant can carry a semantic tint.
    pub fn ink(mut self, color: Rgba) -> Self {
        self.colors.text = color;
        self.colors.hover_text = color;
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

impl RenderOnce for Button {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let c = self.colors;
        let d = self.density;
        let icon_only = self.variant == ButtonVariant::Icon;

        let (fill, text, border) = if self.disabled {
            (c.disabled_fill, c.disabled_text, c.disabled_border)
        } else {
            (c.fill, c.text, c.border)
        };

        let gap = if self.leading.is_some() {
            ICON_LABEL_GAP
        } else if self.trailing.is_some() {
            spacing(Spacing::Xs, d)
        } else {
            px(0.0)
        };

        let (pad_v, pad_h) = if icon_only {
            (spacing(Spacing::Xs, d), spacing(Spacing::Xs, d))
        } else {
            (px(4.0), px(12.0))
        };

        let glyph_size = if icon_only { FONT_MD } else { FONT_XS };

        let mut root = div()
            .flex()
            .items_center()
            .gap(gap)
            .py(pad_v)
            .px(pad_h)
            .rounded(radius(Radius::Sm))
            .font_family(body_family())
            .font_weight(self.weight)
            .text_size(FONT_XS)
            .text_color(text);

        if self.full_width {
            root = root.w_full().justify_start();
        }
        if let Some(h) = self.height {
            root = root.h(h);
        }
        if let Some(fill) = fill {
            root = root.bg(fill);
        }
        if let Some(border) = border {
            root = root.border(BORDER_THIN).border_color(border);
        }

        if let Some(leading) = self.leading {
            root = if icon_only && !self.disabled {
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
        if let Some(label) = self.label {
            root = root.child(div().child(label));
        }
        if let Some(trailing) = self.trailing {
            root = root.child(icon(trailing, glyph_size, text));
        }

        if self.disabled {
            return root.into_any_element();
        }

        let hover_fill = c.hover_fill;
        let hover_text = c.hover_text;
        let hover_border = c.hover_border;
        root = root.hover(move |mut style| {
            if let Some(fill) = hover_fill {
                style = style.bg(fill);
            }
            style = style.text_color(hover_text);
            if let Some(border) = hover_border {
                style = style.border_color(border);
            }
            style
        });

        match (self.id, self.on_click) {
            (Some(id), Some(handler)) => root
                .id(id)
                .cursor_pointer()
                .on_click(handler)
                .into_any_element(),
            _ => root.into_any_element(),
        }
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

    #[allow(clippy::panic)]
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
                Some((P.random, 1.0)),
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
                Some((P.random, 0.92)),
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
                Some((P.random, 0.4)),
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

    #[test]
    fn keyed_palette_fields_are_distinct_hues() {
        let fields = [
            ("brand", P.brand),
            ("random", P.random),
            ("shell", P.shell),
            ("text_primary", P.text_primary),
            ("text_secondary", P.text_secondary),
            ("text_muted", P.text_muted),
            ("border_regular", P.border_regular),
            ("border_input", P.border_input),
        ];
        for i in 0..fields.len() {
            for j in (i + 1)..fields.len() {
                let (na, a) = fields[i];
                let (nb, b) = fields[j];
                assert!(
                    !same_rgb(a, b),
                    "{na} and {nb} share a hue - a mis-wire between them would pass unnoticed",
                );
            }
        }
    }
}
