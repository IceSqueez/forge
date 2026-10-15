use gpui::{
    AnimationExt, App, BoxShadow, ClickEvent, ElementId, InteractiveElement, IntoElement,
    ParentElement, Pixels, RenderOnce, Rgba, SharedString, StatefulInteractiveElement, Styled,
    Window, div, hsla, point, px,
};

use crate::icons::{Icon, icon};
use crate::overlay::enter_animation;
use crate::palette::ForgePalette;
use crate::progress_bar::progress_bar;
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
    pub secondary_action: Option<ToastAction>,
    pub progress: Option<f32>,
}

impl ToastData {
    pub fn new(id: u64, kind: ToastKind, message: impl Into<SharedString>) -> Self {
        Self {
            id,
            kind,
            message: message.into(),
            icon: None,
            action: None,
            secondary_action: None,
            progress: None,
        }
    }
}

#[derive(IntoElement)]
pub struct ToastCard {
    anim_id: ElementId,
    kind: ToastKind,
    message: SharedString,
    icon: Option<Icon>,
    action_label: Option<SharedString>,
    action_id: Option<ElementId>,
    on_action: Option<ToastHandler>,
    secondary_id: Option<ElementId>,
    secondary_label: Option<SharedString>,
    on_secondary: Option<ToastHandler>,
    dismiss_id: Option<ElementId>,
    on_dismiss: Option<ToastHandler>,
    progress: Option<f32>,
    track_palette: ForgePalette,
    card_bg: Rgba,
    border: Rgba,
    message_color: Rgba,
    action_color: Rgba,
    secondary_color: Rgba,
    dismiss_color: Rgba,
    accent: Rgba,
}

pub fn toast_card(
    id: impl Into<ElementId>,
    kind: ToastKind,
    message: impl Into<SharedString>,
    palette: &ForgePalette,
) -> ToastCard {
    ToastCard {
        anim_id: id.into(),
        kind,
        message: message.into(),
        icon: None,
        action_label: None,
        action_id: None,
        on_action: None,
        secondary_id: None,
        secondary_label: None,
        on_secondary: None,
        dismiss_id: None,
        on_dismiss: None,
        progress: None,
        track_palette: *palette,
        card_bg: palette.elevated,
        border: palette.border_input,
        message_color: palette.text_primary,
        action_color: palette.brand,
        secondary_color: palette.text_muted,
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
    pub fn secondary_action(
        mut self,
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.secondary_id = Some(id.into());
        self.secondary_label = Some(label.into());
        self.on_secondary = Some(Box::new(handler));
        self
    }

    #[must_use]
    pub fn progress(mut self, fraction: f32) -> Self {
        self.progress = Some(fraction);
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

impl ToastCard {
    fn message_text(&self) -> impl IntoElement {
        div()
            .flex_1()
            .w_full()
            .min_w(px(0.0))
            .font_family(body_family())
            .text_size(FONT_SM)
            .text_color(self.message_color)
            .child(self.message.clone())
    }

    fn take_secondary(&mut self) -> Option<impl IntoElement + use<>> {
        let (Some(id), Some(label), Some(handler)) = (
            self.secondary_id.take(),
            self.secondary_label.take(),
            self.on_secondary.take(),
        ) else {
            return None;
        };
        Some(action_button(id, label, self.secondary_color, handler))
    }

    fn take_primary(&mut self) -> Option<impl IntoElement + use<>> {
        let (Some(id), Some(label), Some(handler)) = (
            self.action_id.take(),
            self.action_label.take(),
            self.on_action.take(),
        ) else {
            return None;
        };
        Some(action_button(id, label, self.action_color, handler))
    }

    fn take_dismiss(&mut self) -> Option<impl IntoElement + use<>> {
        let (Some(id), Some(handler)) = (self.dismiss_id.take(), self.on_dismiss.take()) else {
            return None;
        };
        Some(
            div()
                .id(id)
                .flex_none()
                .py(pad(Spacing::Xxs))
                .px(pad(Spacing::Xxs))
                .cursor_pointer()
                .on_click(handler)
                .child(icon(Icon::X, DISMISS_ICON, self.dismiss_color)),
        )
    }

    fn has_footer(&self) -> bool {
        self.progress.is_some() || self.on_action.is_some() || self.on_secondary.is_some()
    }

    fn inline_content(mut self, glyph: Icon) -> gpui::Div {
        let secondary = self.take_secondary();
        let primary = self.take_primary();
        let dismiss = self.take_dismiss();
        div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .gap(pad(Spacing::Sm))
            .py(pad(Spacing::Sm))
            .px(pad(Spacing::Sm))
            .child(icon(glyph, KIND_ICON, self.accent))
            .child(self.message_text())
            .children(secondary)
            .children(primary)
            .children(dismiss)
    }

    fn stacked_content(mut self, glyph: Icon) -> gpui::Div {
        let secondary = self.take_secondary();
        let primary = self.take_primary();
        let dismiss = self.take_dismiss();
        let has_actions = secondary.is_some() || primary.is_some();
        let progress = self
            .progress
            .map(|fraction| progress_bar(fraction, self.accent, &self.track_palette));

        let headline = div()
            .w_full()
            .flex()
            .items_center()
            .gap(pad(Spacing::Sm))
            .child(icon(glyph, KIND_ICON, self.accent))
            .child(self.message_text())
            .children(dismiss);

        let actions = has_actions.then(|| {
            div()
                .w_full()
                .flex()
                .justify_end()
                .items_center()
                .gap(pad(Spacing::Xs))
                .children(secondary)
                .children(primary)
        });

        div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(pad(Spacing::Xs))
            .py(pad(Spacing::Sm))
            .px(pad(Spacing::Sm))
            .child(headline)
            .children(progress)
            .children(actions)
    }
}

fn action_button(
    id: ElementId,
    label: SharedString,
    color: Rgba,
    handler: ToastHandler,
) -> impl IntoElement {
    div()
        .id(id)
        .flex_none()
        .py(pad(Spacing::Xxs))
        .px(pad(Spacing::Xs))
        .rounded(radius(Radius::Sm))
        .cursor_pointer()
        .font_family(body_family())
        .text_size(FONT_SM)
        .text_color(color)
        .on_click(handler)
        .child(label)
}

impl RenderOnce for ToastCard {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let glyph = self.icon.unwrap_or_else(|| self.kind.default_icon());
        let anim_id = self.anim_id.clone();
        let stripe = div().flex_none().w(STRIPE_WIDTH).bg(self.accent);
        let border = self.border;
        let card_bg = self.card_bg;

        let content = if self.has_footer() {
            self.stacked_content(glyph)
        } else {
            self.inline_content(glyph)
        };

        div()
            .w(CARD_WIDTH)
            .flex()
            .overflow_hidden()
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(border)
            .bg(card_bg)
            .shadow(vec![BoxShadow {
                color: hsla(0.0, 0.0, 0.0, 0.4),
                offset: point(px(0.0), px(8.0)),
                blur_radius: px(24.0),
                spread_radius: px(0.0),
                inset: false,
            }])
            .child(stripe)
            .child(content)
            .with_animation(anim_id, enter_animation(), |el, delta| el.opacity(delta))
    }
}

#[cfg(test)]
mod tests {
    use super::{ToastCard, ToastKind, toast_card};
    use crate::palette::FORGE_DEFAULT;

    fn card() -> ToastCard {
        toast_card("toast", ToastKind::Info, "message", &FORGE_DEFAULT)
    }

    #[test]
    fn only_a_toast_with_actions_or_progress_stacks_a_footer() {
        for (label, card, expected) in [
            ("plain", card(), false),
            (
                "plain with dismiss",
                card().on_dismiss("x", |_, _, _| {}),
                false,
            ),
            ("progress", card().progress(0.0), true),
            (
                "primary action",
                card().action("a", "Go", |_, _, _| {}),
                true,
            ),
            (
                "secondary action",
                card().secondary_action("s", "Skip", |_, _, _| {}),
                true,
            ),
        ] {
            assert_eq!(card.has_footer(), expected, "{label}");
        }
    }
}
