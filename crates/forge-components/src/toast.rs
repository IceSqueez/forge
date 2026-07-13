use gpui::{
    App, BoxShadow, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement, Pixels,
    RenderOnce, Rgba, SharedString, StatefulInteractiveElement, Styled, Window, div, hsla, point,
    px,
};

use crate::icons::{Icon, icon};
use crate::palette::ForgePalette;
use crate::tokens::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, Density, FONT_SM, Radius, Spacing, radius, spacing,
};

/// Width of the kind-colored accent stripe down the card's leading edge.
const STRIPE_WIDTH: Pixels = px(2.0);
/// Rendered size of the leading kind glyph. Design pins this at 14px, which the
/// small font token reproduces exactly.
const KIND_ICON: Pixels = px(14.0);
/// Rendered size of the trailing dismiss glyph. Design pins it at 11px; the extra-
/// small font token (12px) is the nearest scale step, kept a touch larger so the
/// affordance stays legible and clickable.
const DISMISS_ICON: Pixels = px(12.0);
/// Fixed card width. Matches the viewport's 360px envelope so a stack of cards
/// reads as one aligned column regardless of message length.
const CARD_WIDTH: Pixels = px(360.0);

/// Resolves a chrome inset at the fixed default density. A toast is transient
/// chrome sized once, so its insets snap to the `Spacing` scale at the density-
/// neutral `Cozy` multiplier rather than tracking the app's density knob.
fn pad(s: Spacing) -> Pixels {
    spacing(s, Density::Cozy)
}

/// The five semantic classes a toast can carry. Each maps to a fixed
/// `ForgePalette` hue and a default leading glyph, so a card re-tints with the
/// active theme and reads its intent at a glance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Warn,
    Error,
    Undo,
}

impl ToastKind {
    /// The kind's accent hue — the stripe, glyph, and any emphasis draw from it.
    /// `Undo` is deliberately muted (a reversible, low-urgency action) rather than
    /// an alarm hue.
    pub fn accent(self, palette: &ForgePalette) -> Rgba {
        match self {
            ToastKind::Info => palette.info,
            ToastKind::Success => palette.success,
            ToastKind::Warn => palette.warning,
            ToastKind::Error => palette.random,
            ToastKind::Undo => palette.text_muted,
        }
    }

    /// The glyph shown when the caller supplies no override.
    pub fn default_icon(self) -> Icon {
        match self {
            ToastKind::Info => Icon::InfoCircle,
            ToastKind::Success => Icon::CircleCheck,
            ToastKind::Warn => Icon::AlertTriangle,
            ToastKind::Error => Icon::CircleX,
            ToastKind::Undo => Icon::ArrowBackUp,
        }
    }
}

/// Boxed click handler, shared by the action button and the dismiss control.
/// Mirrors the button/modal families: gpui hands the click event plus the window
/// and app contexts, through which the caller reaches whatever state it dismisses
/// or dispatches against.
type ToastHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// The work a toast action runs when pressed. Context-only (no click event): the
/// host reaches its own state through the passed window and app contexts.
pub type ActionCallback = Box<dyn Fn(&mut Window, &mut App) + 'static>;

/// An optional trailing action on a queued toast: a label plus the work to run
/// when it is pressed. Held in the toast queue (not rebuilt each frame), so the
/// callback is context-only.
pub struct ToastAction {
    pub label: SharedString,
    pub on_action: ActionCallback,
}

impl ToastAction {
    pub fn new(
        label: impl Into<SharedString>,
        on_action: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            label: label.into(),
            on_action: Box::new(on_action),
        }
    }
}

/// One queued toast's data: a stable id, its kind, the message, an optional glyph
/// override, and an optional trailing action. The host owns a list of these and
/// renders each through [`toast_card`]; the id keys both auto-dismiss and manual
/// dismissal.
pub struct ToastData {
    pub id: u64,
    pub kind: ToastKind,
    pub message: SharedString,
    pub icon: Option<Icon>,
    pub action: Option<ToastAction>,
}

impl ToastData {
    pub fn new(id: u64, kind: ToastKind, message: impl Into<SharedString>) -> Self {
        Self {
            id,
            kind,
            message: message.into(),
            icon: None,
            action: None,
        }
    }
}

/// A single toast notification card: an `elevated` surface with a kind-colored
/// leading stripe, a tinted glyph, the message, an optional action button, and a
/// dismiss control. Bottom-right stacking, the auto-dismiss timer, and the queue
/// itself are the host's concern; this component is only the card.
///
/// Build one with [`toast_card`], then layer on `.icon`, `.action` and
/// `.on_dismiss`. Every ink resolves from the palette up front, so the built value
/// carries no palette borrow.
#[derive(IntoElement)]
pub struct ToastCard {
    kind: ToastKind,
    message: SharedString,
    icon: Option<Icon>,
    action_label: Option<SharedString>,
    action_id: Option<ElementId>,
    on_action: Option<ToastHandler>,
    dismiss_id: Option<ElementId>,
    on_dismiss: Option<ToastHandler>,
    card_bg: Rgba,
    border: Rgba,
    message_color: Rgba,
    action_color: Rgba,
    dismiss_color: Rgba,
    accent: Rgba,
}

/// Build a toast card of `kind` carrying `message`, resolving every ink from
/// `palette`. Defaults to the kind's own glyph, no action, and no dismiss control;
/// layer those on through the builder methods.
pub fn toast_card(
    kind: ToastKind,
    message: impl Into<SharedString>,
    palette: &ForgePalette,
) -> ToastCard {
    ToastCard {
        kind,
        message: message.into(),
        icon: None,
        action_label: None,
        action_id: None,
        on_action: None,
        dismiss_id: None,
        on_dismiss: None,
        card_bg: palette.elevated,
        border: palette.border_input,
        message_color: palette.text_primary,
        action_color: palette.brand,
        dismiss_color: palette.text_faint,
        accent: kind.accent(palette),
    }
}

impl ToastCard {
    /// Overrides the kind's default leading glyph.
    #[must_use]
    pub fn icon(mut self, glyph: Icon) -> Self {
        self.icon = Some(glyph);
        self
    }

    /// Adds a trailing action button. gpui needs a stable [`ElementId`] to promote
    /// it to a clickable element; `handler` runs the action (and typically dismisses
    /// the toast) through the passed contexts.
    #[must_use]
    pub fn action(
        mut self,
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.action_id = Some(id.into());
        self.action_label = Some(label.into());
        self.on_action = Some(Box::new(handler));
        self
    }

    /// Makes the trailing dismiss control live. `handler` removes the toast from the
    /// host's queue through the passed contexts.
    #[must_use]
    pub fn on_dismiss(
        mut self,
        id: impl Into<ElementId>,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.dismiss_id = Some(id.into());
        self.on_dismiss = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for ToastCard {
    fn render(mut self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let glyph = self.icon.unwrap_or_else(|| self.kind.default_icon());

        // The kind-colored stripe. The card row keeps flex's default cross-axis
        // stretch (the content row carries the vertical centering), so a 2px-wide
        // child fills the card's full height.
        let stripe = div().flex_none().w(STRIPE_WIDTH).bg(self.accent);

        let mut content = div()
            .flex_1()
            .flex()
            .items_center()
            .gap(pad(Spacing::Sm))
            .py(pad(Spacing::Sm))
            .px(pad(Spacing::Sm))
            .child(icon(glyph, KIND_ICON, self.accent))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(self.message_color)
                    .child(self.message.clone()),
            );

        if let (Some(id), Some(label), Some(handler)) = (
            self.action_id.take(),
            self.action_label.take(),
            self.on_action.take(),
        ) {
            content = content.child(
                div()
                    .id(id)
                    .flex_none()
                    .py(pad(Spacing::Xxs))
                    .px(pad(Spacing::Xs))
                    .rounded(radius(Radius::Sm))
                    .cursor_pointer()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(self.action_color)
                    .on_click(handler)
                    .child(label),
            );
        }

        if let (Some(id), Some(handler)) = (self.dismiss_id.take(), self.on_dismiss.take()) {
            content = content.child(
                div()
                    .id(id)
                    .flex_none()
                    .py(pad(Spacing::Xxs))
                    .px(pad(Spacing::Xxs))
                    .cursor_pointer()
                    .on_click(handler)
                    .child(icon(Icon::X, DISMISS_ICON, self.dismiss_color)),
            );
        }

        // Design card shadow: `0 8px 24px rgba(0,0,0,0.4)`.
        div()
            .w(CARD_WIDTH)
            .flex()
            .overflow_hidden()
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(self.border)
            .bg(self.card_bg)
            .shadow(vec![BoxShadow {
                color: hsla(0.0, 0.0, 0.0, 0.4),
                offset: point(px(0.0), px(8.0)),
                blur_radius: px(24.0),
                spread_radius: px(0.0),
            }])
            .child(stripe)
            .child(content)
    }
}
