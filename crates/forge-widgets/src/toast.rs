use std::time::{Duration, Instant};

use iced::{
    Alignment, Background, Border, Color, Element, Length, Padding,
    widget::{button, column, container, row, text},
};

use crate::icons::{Icon, tabler_icon};
use crate::palette::ForgePalette;
use crate::tokens::{Density, FONT_SM, FontRole, Radius, Spacing, font, radius, spacing};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Warn,
    Error,
    Undo,
}

#[derive(Debug, Clone)]
pub struct ToastAction<Msg> {
    pub label: String,
    pub on_press: Msg,
}

pub struct Toast<Msg> {
    pub id: u64,
    pub kind: ToastKind,
    pub message: String,
    pub action: Option<ToastAction<Msg>>,
    pub created_at: Instant,
    pub duration: Duration,
}

pub struct ToastQueue<Msg> {
    toasts: Vec<Toast<Msg>>,
    next_id: u64,
}

impl<Msg: Clone> ToastQueue<Msg> {
    pub fn new() -> Self {
        Self {
            toasts: Vec::new(),
            next_id: 0,
        }
    }

    pub fn push(
        &mut self,
        kind: ToastKind,
        message: impl Into<String>,
        action: Option<ToastAction<Msg>>,
        duration: Duration,
    ) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.toasts.push(Toast {
            id,
            kind,
            message: message.into(),
            action,
            created_at: Instant::now(),
            duration,
        });
        id
    }

    pub fn dismiss(&mut self, id: u64) {
        self.toasts.retain(|t| t.id != id);
    }

    pub fn prune_expired(&mut self, now: Instant) -> Vec<u64> {
        let mut removed = Vec::new();
        self.toasts.retain(|t| {
            if now.saturating_duration_since(t.created_at) >= t.duration {
                removed.push(t.id);
                false
            } else {
                true
            }
        });
        removed
    }

    pub fn is_empty(&self) -> bool {
        self.toasts.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Toast<Msg>> {
        self.toasts.iter()
    }
}

impl<Msg: Clone> Default for ToastQueue<Msg> {
    fn default() -> Self {
        Self::new()
    }
}

fn kind_color(kind: ToastKind, palette: &ForgePalette) -> Color {
    match kind {
        ToastKind::Info => palette.info,
        ToastKind::Success => palette.success,
        ToastKind::Warn => palette.warning,
        ToastKind::Error => palette.random,
        ToastKind::Undo => palette.brand,
    }
}

fn kind_icon(kind: ToastKind) -> Icon {
    match kind {
        ToastKind::Info => Icon::InfoCircle,
        ToastKind::Success => Icon::CircleCheck,
        ToastKind::Warn => Icon::AlertTriangle,
        ToastKind::Error => Icon::CircleX,
        ToastKind::Undo => Icon::ArrowBackUp,
    }
}

fn toast_row<'a, Msg: Clone + 'a>(
    toast: &'a Toast<Msg>,
    on_dismiss: &dyn Fn(u64) -> Msg,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let sm = spacing(Spacing::Sm, Density::Cozy);
    let md = spacing(Spacing::Md, Density::Cozy);
    let accent = kind_color(toast.kind, palette);
    let icon = kind_icon(toast.kind);

    let bar = container(iced::widget::Space::new().width(2))
        .height(Length::Fill)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(accent)),
            ..container::Style::default()
        });

    let icon_el = tabler_icon(icon, FONT_SM, accent);

    let msg_el = text(toast.message.as_str())
        .size(FONT_SM)
        .font(font(FontRole::Body))
        .color(palette.text_primary);

    let dismiss_color = palette.text_faint;
    let dismiss_hover = palette.text_secondary;
    let dismiss_btn: Element<'a, Msg> = {
        let dismiss_msg = on_dismiss(toast.id);
        button(tabler_icon(Icon::X, FONT_SM, dismiss_color))
            .on_press(dismiss_msg)
            .padding([2, 4])
            .style(move |_theme: &iced::Theme, status| button::Style {
                background: None,
                text_color: match status {
                    button::Status::Hovered | button::Status::Pressed => dismiss_hover,
                    _ => dismiss_color,
                },
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: radius(Radius::Sm).into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            })
            .into()
    };

    let mut content_children: Vec<Element<'a, Msg>> = vec![
        icon_el,
        msg_el.into(),
        iced::widget::Space::new().width(Length::Fill).into(),
    ];

    if let Some(action) = &toast.action {
        let action_msg = action.on_press.clone();
        let action_color = palette.text_secondary;
        let action_hover = palette.text_primary;
        let action_label = action.label.clone();
        let action_btn: Element<'a, Msg> = button(
            text(action_label)
                .size(FONT_SM)
                .font(font(FontRole::Body))
                .color(action_color),
        )
        .on_press(action_msg)
        .padding([2, 6])
        .style(move |_theme: &iced::Theme, status| button::Style {
            background: None,
            text_color: match status {
                button::Status::Hovered | button::Status::Pressed => action_hover,
                _ => action_color,
            },
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: radius(Radius::Sm).into(),
            },
            shadow: iced::Shadow::default(),
            snap: false,
        })
        .into();
        content_children.push(action_btn);
    }

    content_children.push(dismiss_btn);

    let content_row = row(content_children)
        .spacing(sm as f32)
        .align_y(Alignment::Center);

    let content_padding = Padding {
        top: sm as f32,
        right: md as f32,
        bottom: sm as f32,
        left: sm as f32,
    };

    let elevated = palette.elevated;
    let border_color = palette.border_input;

    container(
        row![bar, container(content_row).padding(content_padding)]
            .spacing(0)
            .align_y(Alignment::Center),
    )
    .width(Length::Fixed(360.0))
    .style(move |_theme: &iced::Theme| container::Style {
        background: Some(Background::Color(elevated)),
        border: Border {
            color: border_color,
            width: 0.5,
            radius: radius(Radius::Md).into(),
        },
        ..container::Style::default()
    })
    .into()
}

pub fn toast_viewport<'a, Msg: Clone + 'a>(
    queue: &'a ToastQueue<Msg>,
    on_dismiss: impl Fn(u64) -> Msg + 'a,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    if queue.is_empty() {
        return iced::widget::Space::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    }

    let sm = spacing(Spacing::Sm, Density::Cozy);
    let md = spacing(Spacing::Md, Density::Cozy);

    let rows: Vec<Element<'a, Msg>> = queue
        .toasts
        .iter()
        .rev()
        .map(|t| toast_row(t, &on_dismiss, palette))
        .collect();

    let stack_col = column(rows).spacing(sm as f32);

    container(stack_col)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Right)
        .align_y(iced::alignment::Vertical::Bottom)
        .padding(md)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_queue() -> ToastQueue<()> {
        ToastQueue::new()
    }

    #[test]
    fn push_assigns_monotonic_ids() {
        let mut q = make_queue();
        let a = q.push(ToastKind::Info, "a", None, Duration::from_secs(4));
        let b = q.push(ToastKind::Success, "b", None, Duration::from_secs(4));
        let c = q.push(ToastKind::Error, "c", None, Duration::from_secs(4));
        assert!(a < b);
        assert!(b < c);
    }

    #[test]
    fn dismiss_removes_by_id() {
        let mut q = make_queue();
        let id = q.push(ToastKind::Warn, "msg", None, Duration::from_secs(4));
        assert!(!q.is_empty());
        q.dismiss(id);
        assert!(q.is_empty());
    }

    #[test]
    fn prune_expired_returns_dismissed_ids() {
        let mut q = make_queue();
        let id1 = q.push(ToastKind::Info, "fast", None, Duration::ZERO);
        let id2 = q.push(ToastKind::Error, "slow", None, Duration::from_secs(60));
        let pruned = q.prune_expired(Instant::now());
        assert_eq!(pruned, vec![id1]);
        assert!(!q.is_empty());
        assert_eq!(q.iter().count(), 1);
        assert_eq!(q.iter().next().map(|t| t.id), Some(id2));
    }

    #[test]
    fn is_empty_reflects_state() {
        let mut q = make_queue();
        assert!(q.is_empty());
        let id = q.push(ToastKind::Undo, "x", None, Duration::from_secs(1));
        assert!(!q.is_empty());
        q.dismiss(id);
        assert!(q.is_empty());
    }
}
