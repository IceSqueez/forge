use gpui::{
    App, BoxShadow, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement, Pixels,
    RenderOnce, Rgba, SharedString, StatefulInteractiveElement, Styled, Window, div, hsla, point,
    px,
};

use crate::icons::{Icon, icon};
use crate::palette::ForgePalette;
use crate::tokens::{BORDER_THIN, Density, FONT_SM, Radius, Spacing, body_family, radius, spacing};

const STRIPE_WIDTH: Pixels = px(2.0);
const KIND_ICON: Pixels = px(14.0);
const DISMISS_ICON: Pixels = px(12.0);
const CARD_WIDTH: Pixels = px(360.0);

fn pad(s: Spacing) -> Pixels {
    spacing(s, Density::Cozy)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Warn,
    Error,
    Undo,
}

impl ToastKind {
    /// `Undo` is deliberately muted rather than an alarm hue.
    pub fn accent(self, palette: &ForgePalette) -> Rgba {
        match self {
            ToastKind::Info => palette.info,
            ToastKind::Success => palette.success,
            ToastKind::Warn => palette.warning,
            ToastKind::Error => palette.random,
            ToastKind::Undo => palette.text_muted,
        }
    }

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

type ToastHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub type ActionCallback = Box<dyn Fn(&mut Window, &mut App) + 'static>;

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

/// Defaults to the kind's own glyph with no action and no dismiss control; layer those on through the builder methods.
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
    #[must_use]
    pub fn icon(mut self, glyph: Icon) -> Self {
        self.icon = Some(glyph);
        self
    }

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

        // The card row keeps flex's default cross-axis stretch, so this height-less stripe fills the full card height.
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
                    .font_family(body_family())
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
                    .font_family(body_family())
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
                inset: false,
            }])
            .child(stripe)
            .child(content)
    }
}
