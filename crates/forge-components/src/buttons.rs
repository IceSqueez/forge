use gpui::{
    App, ClickEvent, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement, Pixels,
    RenderOnce, Rgba, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::icons::{Icon, icon, icon_inherit};
use crate::palette::{ForgePalette, with_alpha};
use crate::tokens::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, Density, FONT_MD, FONT_SM, Radius, Spacing, radius, spacing,
};

/// Boxed click handler carried by a pressable button. gpui passes the click event
/// plus the window and app contexts, through which the caller reaches its entity.
type ButtonClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// Fixed icon→label gap for the leading-icon buttons. The source pins this at a
/// literal 5px (one step below the `Spacing::Xs` 6px used for the trailing-icon
/// gap), so it is carried density-neutrally as a literal rather than snapped onto
/// the `Spacing` scale — mirroring the off-scale disc literals in [`crate::status`].
const ICON_LABEL_GAP: Pixels = px(5.0);

/// Alpha the filled variants fade their fill to on hover.
const FILL_HOVER_ALPHA: f32 = 0.92;
/// Alpha applied to a filled variant's fill when disabled.
const FILL_DISABLED_ALPHA: f32 = 0.4;
/// Alpha applied to a filled variant's ink when disabled.
const INK_DISABLED_ALPHA: f32 = 0.5;
/// Alpha applied to an outlined/ghost variant's ink and border when disabled.
const OUTLINE_DISABLED_ALPHA: f32 = 0.4;
/// Faint tint alpha behind a hovered secondary button (a wash of its border hue).
const SECONDARY_HOVER_WASH: f32 = 0.06;
/// Faint tint alpha behind a hovered icon button (a wash of the brand hue).
const ICON_HOVER_WASH: f32 = 0.08;

/// Which button in the family this is. Each maps to a fixed set of `ForgePalette`
/// fields across the rest, hover and disabled states (see [`ButtonVariant::colors`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ButtonVariant {
    /// Solid brand fill, shell ink — the page's primary affirmative action.
    Primary,
    /// Solid `random` (pink) fill, shell ink — a destructive affirmative action.
    Destructive,
    /// Transparent fill, thin `border_regular` outline, secondary ink.
    Secondary,
    /// Transparent fill, thin outline, muted ink that brightens (with a lighter
    /// border) on hover.
    Ghost,
    /// Borderless square icon-only affordance; a faint brand wash on hover.
    Icon,
}

/// The full per-state color set a variant resolves to, so the rest, hover and
/// disabled looks are all pinned to concrete `ForgePalette` fields up front and
/// the built [`Button`] carries no palette borrow.
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
    /// Resolves the variant's rest/hover/disabled fill, ink and border against the
    /// active theme. The filled variants (Primary, Destructive) share one shape —
    /// an opaque hue that fades on hover and dims when disabled — differing only in
    /// which hue they carry; the outlined variants keep a transparent fill and
    /// instead move their ink and border.
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
                text: p.text_muted,
                border: Some(p.border_regular),
                hover_fill: None,
                hover_text: p.text_primary,
                hover_border: Some(p.border_input),
                disabled_fill: None,
                disabled_text: with_alpha(p.text_muted, OUTLINE_DISABLED_ALPHA),
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

/// Shared color shape of the two solid variants: an opaque `hue` fill inked with
/// `ink`, fading to `hue @ 0.92` on hover and `hue @ 0.4` / `ink @ 0.5` when
/// disabled, with no border in any state.
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

/// A pressable button: an optional leading icon, an optional label and an optional
/// trailing icon inside a rounded, `Density`-scaled frame whose fill, ink and
/// border follow its [`ButtonVariant`] across rest, hover and disabled states.
///
/// Build one through a family constructor ([`primary_button`], [`ghost_button`],
/// …), which fixes the variant, weight and glyphs and resolves colors from the
/// active theme. Attach [`Button::on_click`] to make it fire (the caller's handler
/// mutates its own entity via the passed `cx`); [`Button::disabled`] renders the
/// dimmed, inert look; [`Button::density`] rescales the padding.
#[derive(IntoElement)]
pub struct Button {
    variant: ButtonVariant,
    label: Option<SharedString>,
    leading: Option<Icon>,
    trailing: Option<Icon>,
    /// Label weight. Plain text buttons stay at `NORMAL`; only the icon-bearing
    /// primary buttons raise their label to `SEMIBOLD`, mirroring the source.
    weight: FontWeight,
    colors: ButtonColors,
    density: Density,
    disabled: bool,
    full_width: bool,
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
            id: None,
            on_click: None,
        }
    }

    /// Overrides the density used to scale the button's padding. A bare
    /// constructor resolves it at `Density::Cozy`.
    pub fn density(mut self, density: Density) -> Self {
        self.density = density;
        self
    }

    /// Stretches the button to fill its parent's width and left-aligns its
    /// content, for the full-column nav rows of a sectioned screen. A bare
    /// constructor hugs its content instead.
    pub fn full_width(mut self) -> Self {
        self.full_width = true;
        self
    }

    /// Renders the button in its dimmed, inert state: disabled colors, no hover
    /// feedback and no click handling regardless of [`Button::on_click`].
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Makes the button fire. gpui needs a stable [`ElementId`] to promote the
    /// frame to a stateful, clickable element, so the caller supplies one
    /// alongside the handler (which mutates its own entity via the passed `cx`).
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

        // Resolve the state the frame paints in. Disabled shortcuts the whole
        // hover/click apparatus; enabled carries its rest colors plus the hover
        // deltas.
        let (fill, text, border) = if self.disabled {
            (c.disabled_fill, c.disabled_text, c.disabled_border)
        } else {
            (c.fill, c.text, c.border)
        };

        // The single flex gap between the icon and label. Only one glyph slot is
        // ever occupied per constructor: a leading icon sits `ICON_LABEL_GAP` from
        // the label, a trailing icon a `Spacing::Xs` step away.
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
            (spacing(Spacing::Xxs, d), spacing(Spacing::Sm, d))
        };

        let glyph_size = if icon_only { FONT_MD } else { FONT_SM };

        let mut root = div()
            .flex()
            .items_center()
            .gap(gap)
            .py(pad_v)
            .px(pad_h)
            .rounded(radius(Radius::Sm))
            .font_family(DEFAULT_BODY_FAMILY)
            .font_weight(self.weight)
            .text_size(FONT_SM)
            .text_color(text);

        if self.full_width {
            root = root.w_full().justify_start();
        }
        if let Some(fill) = fill {
            root = root.bg(fill);
        }
        if let Some(border) = border {
            root = root.border(BORDER_THIN).border_color(border);
        }

        // The text-button icons carry an explicit tint (the resolved state ink), so
        // they do NOT re-tint under the frame's hover text color — matching the
        // source, where the svg glyph keeps its color while the label brightens.
        // The icon-only button is the exception: its source rendered a char glyph
        // as text, which brightened on hover, so its glyph inherits the frame's ink
        // (via `icon_inherit`) and follows the rest→hover→disabled text color.
        if let Some(leading) = self.leading {
            root = if icon_only {
                root.child(icon_inherit(leading, glyph_size))
            } else {
                root.child(icon(leading, glyph_size, text))
            };
        }
        if let Some(label) = self.label {
            // The label sets no color of its own so it inherits the frame's ink,
            // letting the hover text color reach it.
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

/// Solid brand button with shell ink and a `NORMAL`-weight label — the primary
/// affirmative action.
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

/// Primary button with a leading icon. The label is raised to `SEMIBOLD` (the only
/// buttons in the family that bold their label), the icon inked with the shell ink.
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

/// Primary button with a trailing icon and a `SEMIBOLD` label.
pub fn primary_button_with_icon_right(
    label: impl Into<SharedString>,
    icon: Icon,
    palette: &ForgePalette,
) -> Button {
    Button::new(
        ButtonVariant::Primary,
        Some(label.into()),
        None,
        Some(icon),
        FontWeight::SEMIBOLD,
        palette,
    )
}

/// Solid `random`-hue (pink) button with shell ink and a `NORMAL`-weight label —
/// a destructive affirmative action.
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

/// Outlined button: transparent fill, thin `border_regular` outline, secondary ink
/// that brightens to primary (over a faint border wash) on hover.
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

/// Ghost button: transparent fill, thin outline, muted ink; on hover the ink
/// brightens to primary and the border lightens to `border_input`.
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

/// Ghost button with a leading icon and a `NORMAL`-weight label.
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

/// Borderless square icon-only button: no fill at rest, a faint brand wash on
/// hover. The glyph renders one step larger (`FONT_MD`) than a text button's icon.
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
    use crate::palette::CATPPUCCIN_MOCHA as P;

    const EPS: f32 = 1e-6;

    /// A resolved slot's expected shape: its hue comes from a `ForgePalette` field
    /// (compared channel-wise), its alpha is pinned as a literal so a visual
    /// regression that shifts a wash/dim constant fails here. `None` means the slot
    /// is expected to carry no color (transparent fill / borderless).
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
                    "{label}: hue rgb mismatch — got ({},{},{}), want ({},{},{})",
                    got.r,
                    got.g,
                    got.b,
                    hue.r,
                    hue.g,
                    hue.b,
                );
                assert!(
                    (got.a - alpha).abs() < EPS,
                    "{label}: alpha mismatch — got {}, want {alpha}",
                    got.a,
                );
            }
            _ => panic!("{label}: presence mismatch — got {actual:?}, want {expected:?}"),
        }
    }

    // Each state test asserts the full (fill, ink, border) triple a variant paints
    // in that state across all five variants. The load-bearing content is which
    // palette field each hue comes from (mis-wire to a neighbour is caught by the
    // distinct-hue test) and the alpha literal (a shifted wash/dim constant is a
    // silent visual regression these pin).

    #[test]
    fn rest_colors_resolve_per_variant() {
        let cases: [(ButtonVariant, Slot, Slot, Slot); 5] = [
            // variant, fill, ink, border
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
                Some((P.text_muted, 1.0)),
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
            // variant, hover_fill, hover_ink, hover_border
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
            // variant, disabled_fill, disabled_ink, disabled_border
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
                Some((P.text_muted, 0.4)),
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

    /// The per-variant resolution tests assert each hue channel-wise against a
    /// specific palette field; that only catches a mis-wire to a neighbouring field
    /// if the two fields are actually different hues. Pin that assumption: every
    /// field the button family keys on is a distinct hue under Mocha.
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
                    "{na} and {nb} share a hue — a mis-wire between them would pass unnoticed",
                );
            }
        }
    }
}
