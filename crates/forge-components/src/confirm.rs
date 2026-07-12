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

/// Gap between the icon tile and the title/name column in the header row. Carried
/// as a named off-scale literal (the source pins it at 12px, one notch above the
/// `Spacing::Sm` 10) rather than snapped onto the scale — mirroring the off-scale
/// literals the button family keeps.
const HEADER_GAP: Pixels = px(12.0);
/// Side of the square accent-tinted tile behind the alert glyph.
const ICON_TILE: Pixels = px(36.0);
/// Rendered size of the alert glyph centred in the tile.
const ICON_TILE_GLYPH: Pixels = px(18.0);
/// Alpha of the tone accent behind the alert glyph — a faint wash of the tone hue.
const ICON_TILE_ALPHA: f32 = 0.12;
/// Rendered size of the keyboard glyph leading the inline Escape hint.
const ESC_GLYPH: Pixels = px(12.0);
/// Gap between the keyboard glyph, the `Esc` keycap and the hint phrase.
const ESC_GAP: Pixels = px(5.0);
/// Gap between the cancel and confirm buttons in the footer actions cluster.
const ACTIONS_GAP: Pixels = px(8.0);
/// Alpha the accent confirm button fades its fill to on hover — matched to the
/// filled button family's hover fade so the accent action reads as a filled kit
/// button whose hue happens to be the tone accent.
const ACCENT_HOVER_ALPHA: f32 = 0.92;

/// Boxed click handler for the confirm / cancel actions. gpui passes the click
/// event plus the window and app contexts, through which the caller reaches its
/// own entity to resolve the two-phase gate.
type ActionHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// Severity of the confirmed action, which fixes the accent hue carried by both
/// the header glyph tile and the confirm button.
///
/// `Destructive` paints the `random` (pink) danger accent for irreversible
/// removals; `Warning` paints the `warning` (yellow) caution accent for
/// reversible-but-disruptive actions (e.g. disabling something).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmTone {
    Destructive,
    Warning,
}

impl ConfirmTone {
    /// Resolves the tone's accent hue against the active theme: `Destructive`
    /// keys the `random` field, `Warning` the `warning` field. Kept internal so
    /// the mapping is pinned in one place (and unit-testable in-crate, mirroring
    /// the button family's `colors` resolver).
    pub(crate) fn accent(self, palette: &ForgePalette) -> Rgba {
        match self {
            ConfirmTone::Destructive => palette.random,
            ConfirmTone::Warning => palette.warning,
        }
    }
}

/// A destructive/warning confirmation dialog: an alert-triangle glyph on a
/// tone-tinted tile beside a title over an optional monospace target-name, a
/// muted impact message, then a divider and a footer pairing an inline Escape
/// hint with a secondary cancel and an accent-toned confirm button.
///
/// The card reuses the base modal's outer shell values (`elevated` fill,
/// `border_regular` hairline, `Radius::Lg`, `modal_width`), but lays out its own
/// inner content since the confirm chrome — the tinted tile, the monospace name,
/// the inline Escape hint — diverges from the base header/footer bands. The card
/// is just the surface: wrap it in a centred [`crate::overlay`] to gain the
/// scrim, the enter animation and scrim/Escape dismissal —
/// `overlay(confirm_modal(...), palette).position(OverlayPosition::Center)`.
///
/// The consuming screen owns the two-phase gate (a `pending: Option<_>` field)
/// and renders this only while that field is `Some`; [`ConfirmModal::on_confirm`]
/// / [`ConfirmModal::on_cancel`] wire the buttons to resolve it.
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

/// Builds a confirm dialog titled `title`, with `message` as the impact body and
/// `tone` fixing the accent hue. Defaults to no target-name, no Escape hint and
/// no wired buttons; layer those on through the builder methods.
///
/// The kit carries no localisation, so every visible string — `title`, `message`,
/// the button labels and the Escape-hint phrase — is a caller-supplied, already
/// resolved value; only the tone/accent and the fixed alert glyph stay owned here.
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
    /// Sets the monospace target-name under the title — the specific id, label or
    /// path the action applies to.
    #[must_use]
    pub fn item_name(mut self, name: impl Into<SharedString>) -> Self {
        self.item_name = Some(name.into());
        self
    }

    /// Sets the trailing phrase of the inline Escape hint (e.g. "to cancel"). The
    /// keyboard glyph and the `Esc` keycap ahead of it are structural; only the
    /// phrase is caller-supplied so it can be localised.
    #[must_use]
    pub fn esc_hint(mut self, phrase: impl Into<SharedString>) -> Self {
        self.esc_hint = Some(phrase.into());
        self
    }

    /// Wires the accent confirm button: its label plus a stable [`ElementId`] gpui
    /// needs to promote it to a clickable element, and the handler that mutates the
    /// caller's entity through the passed `cx` to carry the action out.
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

    /// Wires the secondary cancel button — label, [`ElementId`] and the handler
    /// that dismisses the dialog through the caller's entity.
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

/// The accent-filled confirm button. The button family exposes no arbitrary-hue
/// filled constructor and the `Warning` tone needs a warning-hued fill it cannot
/// give, so the confirm action mirrors the filled-button shape (rounded frame,
/// `shell` ink, hover fade) driven by the resolved tone accent — matching the
/// destructive filled button for the `Destructive` tone.
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

        // Header: the accent-tinted tile and the title over the optional
        // monospace target-name.
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

        // Footer: the inline Escape hint pushed against the cancel/confirm cluster,
        // over a hairline rule standing in for the source's explicit divider.
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
