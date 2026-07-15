use gpui::{
    App, ClickEvent, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement, Pixels,
    RenderOnce, Rgba, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::buttons::secondary_button;
use crate::icons::{Icon, icon};
use crate::palette::{ForgePalette, with_alpha};
use crate::tokens::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_MD, FONT_SM, FONT_XS,
    ModalSize, Radius, Spacing, modal_width, radius, spacing,
};

const HEADER_GAP: Pixels = px(12.0);
const ICON_TILE: Pixels = px(36.0);
const ICON_TILE_GLYPH: Pixels = px(18.0);
const ICON_TILE_ALPHA: f32 = 0.12;
const ESC_GLYPH: Pixels = px(12.0);
const ESC_GAP: Pixels = px(5.0);
const ACTIONS_GAP: Pixels = px(8.0);
const ACCENT_HOVER_ALPHA: f32 = 0.92;

type ActionHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmTone {
    Destructive,
    Warning,
}

impl ConfirmTone {
    pub(crate) fn accent(self, palette: &ForgePalette) -> Rgba {
        match self {
            ConfirmTone::Destructive => palette.random,
            ConfirmTone::Warning => palette.warning,
        }
    }
}

/// The card is only the surface — wrap it in a centred [`crate::overlay`] for the scrim, animation and Escape/scrim dismissal.
#[derive(IntoElement)]
pub struct ConfirmModal {
    title: SharedString,
    message: SharedString,
    item_name: Option<SharedString>,
    esc_hint: Option<SharedString>,
    tone: ConfirmTone,
    palette: ForgePalette,
    confirm: Option<(ElementId, SharedString, ActionHandler)>,
    cancel: Option<(ElementId, SharedString, ActionHandler)>,
}

pub fn confirm_modal(
    title: impl Into<SharedString>,
    message: impl Into<SharedString>,
    tone: ConfirmTone,
    palette: &ForgePalette,
) -> ConfirmModal {
    ConfirmModal {
        title: title.into(),
        message: message.into(),
        item_name: None,
        esc_hint: None,
        tone,
        palette: *palette,
        confirm: None,
        cancel: None,
    }
}

impl ConfirmModal {
    #[must_use]
    pub fn item_name(mut self, name: impl Into<SharedString>) -> Self {
        self.item_name = Some(name.into());
        self
    }

    #[must_use]
    pub fn esc_hint(mut self, phrase: impl Into<SharedString>) -> Self {
        self.esc_hint = Some(phrase.into());
        self
    }

    #[must_use]
    pub fn on_confirm(
        mut self,
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.confirm = Some((id.into(), label.into(), Box::new(handler)));
        self
    }

    #[must_use]
    pub fn on_cancel(
        mut self,
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.cancel = Some((id.into(), label.into(), Box::new(handler)));
        self
    }
}

/// Bespoke because the button family has no arbitrary-hue filled constructor and the `Warning` tone needs a warning-hued fill.
fn accent_confirm_button(
    id: ElementId,
    label: SharedString,
    accent: Rgba,
    ink: Rgba,
    handler: ActionHandler,
) -> impl IntoElement {
    div()
        .id(id)
        .flex()
        .items_center()
        .py(spacing(Spacing::Xxs, Density::Cozy))
        .px(spacing(Spacing::Sm, Density::Cozy))
        .rounded(radius(Radius::Sm))
        .bg(accent)
        .font_family(DEFAULT_BODY_FAMILY)
        .text_size(FONT_SM)
        .text_color(ink)
        .cursor_pointer()
        .hover(move |style| style.bg(with_alpha(accent, ACCENT_HOVER_ALPHA)))
        .on_click(handler)
        .child(label)
}

impl RenderOnce for ConfirmModal {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let ConfirmModal {
            title,
            message,
            item_name,
            esc_hint,
            tone,
            palette,
            confirm,
            cancel,
        } = self;

        let accent = tone.accent(&palette);

        let tile = div()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(ICON_TILE)
            .rounded(radius(Radius::Md))
            .bg(with_alpha(accent, ICON_TILE_ALPHA))
            .child(icon(Icon::AlertTriangle, ICON_TILE_GLYPH, accent));

        let mut titles = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_MD)
                    .text_color(palette.text_primary)
                    .child(title),
            );
        if let Some(name) = item_name {
            titles = titles.child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(name),
            );
        }

        let header = div()
            .flex()
            .items_center()
            .gap(HEADER_GAP)
            .child(tile)
            .child(titles);

        let hint = div()
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FONT_SM)
            .text_color(palette.text_muted)
            .child(message);

        let body = div()
            .w_full()
            .p(spacing(Spacing::Lg, Density::Cozy))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(header)
            .child(hint);

        let esc = esc_hint.map(|phrase| {
            div()
                .flex()
                .items_center()
                .gap(ESC_GAP)
                .child(icon(Icon::Keyboard, ESC_GLYPH, palette.text_faint))
                .child(
                    div()
                        .font_family(DEFAULT_MONO_FAMILY)
                        .text_size(FONT_XS)
                        .text_color(palette.text_faint)
                        .child("Esc"),
                )
                .child(
                    div()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .text_size(FONT_XS)
                        .text_color(palette.text_faint)
                        .child(phrase),
                )
        });

        let has_actions = cancel.is_some() || confirm.is_some();
        let mut actions = div().flex().items_center().gap(ACTIONS_GAP);
        if let Some((id, label, handler)) = cancel {
            actions = actions.child(secondary_button(label, &palette).on_click(id, handler));
        }
        if let Some((id, label, handler)) = confirm {
            actions = actions.child(accent_confirm_button(
                id,
                label,
                accent,
                palette.shell,
                handler,
            ));
        }

        let mut card = div()
            .flex()
            .flex_col()
            .w(modal_width(ModalSize::Sm))
            .bg(palette.elevated)
            .rounded(radius(Radius::Lg))
            .overflow_hidden()
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(body);

        if esc.is_some() || has_actions {
            card = card.child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .py(spacing(Spacing::Sm, Density::Cozy))
                    .px(spacing(Spacing::Lg, Density::Cozy))
                    .border_t(BORDER_THIN)
                    .border_color(palette.border_regular)
                    .child(
                        esc.map_or_else(|| div().into_any_element(), IntoElement::into_any_element),
                    )
                    .child(actions),
            );
        }

        card
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;

    fn assert_same_hue(got: Rgba, want: Rgba) {
        assert_eq!(got.r, want.r, "red channel");
        assert_eq!(got.g, want.g, "green channel");
        assert_eq!(got.b, want.b, "blue channel");
        assert_eq!(got.a, want.a, "alpha channel");
    }

    // Pins the tone->palette-field mapping so a mis-wire (e.g. Warning keyed onto
    // `random`, `brand`, or `text_muted`) fails here. Channel-wise so it also
    // catches a swap to any other field that happened to share a component.
    #[test]
    fn tone_resolves_to_its_severity_palette_field() {
        let p = &CATPPUCCIN_MOCHA;
        for (tone, want) in [
            (ConfirmTone::Destructive, p.random),
            (ConfirmTone::Warning, p.warning),
        ] {
            assert_same_hue(tone.accent(p), want);
        }
    }

    // Guard giving the mapping test teeth: the two tones must land on visibly
    // different hues, so pinning each to a distinct field is a real constraint
    // rather than two names for one colour.
    #[test]
    fn destructive_and_warning_are_distinct_hues() {
        let p = &CATPPUCCIN_MOCHA;
        assert_ne!(
            ConfirmTone::Destructive.accent(p),
            ConfirmTone::Warning.accent(p),
        );
    }
}
