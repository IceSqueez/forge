use crate::presentation::ActivePresentation;
use std::time::Duration;

use gpui::{
    AnyElement, App, Global, IntoElement, ParentElement, SharedString, Styled, deferred, div,
};

use crate::footer::FOOTER_HEIGHT;
use crate::icons::Icon;
use crate::palette::ForgePalette;
use crate::toast::{ToastAction, ToastData, ToastKind, toast_card};
use crate::tokens::{Spacing, spacing};

const DEFAULT_TOAST_MS: u64 = 4000;
const TOAST_PRIORITY: usize = 2;

pub struct Toasts {
    items: Vec<ToastData>,
    next_id: u64,
}

impl Toasts {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            next_id: 0,
        }
    }

    pub fn items(&self) -> &[ToastData] {
        &self.items
    }

    fn append(
        &mut self,
        kind: ToastKind,
        message: SharedString,
        icon: Option<Icon>,
        action: Option<ToastAction>,
        secondary_action: Option<ToastAction>,
    ) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.items.push(ToastData {
            id,
            kind,
            message,
            icon,
            action,
            secondary_action,
            progress: None,
        });
        id
    }

    pub fn show(&mut self, mut data: ToastData) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        data.id = id;
        self.items.push(data);
        id
    }

    pub fn replace(&mut self, id: u64, mut data: ToastData) -> bool {
        let Some(slot) = self.items.iter_mut().find(|t| t.id == id) else {
            return false;
        };
        data.id = id;
        *slot = data;
        true
    }

    pub fn contains(&self, id: u64) -> bool {
        self.items.iter().any(|t| t.id == id)
    }

    pub fn dismiss(&mut self, id: u64) {
        self.items.retain(|t| t.id != id);
    }

    pub fn take(&mut self, id: u64) -> Option<ToastData> {
        let idx = self.items.iter().position(|t| t.id == id)?;
        Some(self.items.remove(idx))
    }
}

impl Default for Toasts {
    fn default() -> Self {
        Self::new()
    }
}

impl Global for Toasts {}

pub trait PushToast {
    fn push_toast(&mut self, kind: ToastKind, message: impl Into<SharedString>);

    fn push_toast_full(
        &mut self,
        kind: ToastKind,
        message: impl Into<SharedString>,
        icon: Option<Icon>,
        action: Option<ToastAction>,
        duration: Duration,
    );

    fn push_toast_with_secondary(
        &mut self,
        kind: ToastKind,
        message: impl Into<SharedString>,
        icon: Option<Icon>,
        action: ToastAction,
        secondary_action: ToastAction,
        duration: Duration,
    );
}

impl PushToast for App {
    fn push_toast(&mut self, kind: ToastKind, message: impl Into<SharedString>) {
        self.push_toast_full(
            kind,
            message,
            None,
            None,
            Duration::from_millis(DEFAULT_TOAST_MS),
        );
    }

    fn push_toast_full(
        &mut self,
        kind: ToastKind,
        message: impl Into<SharedString>,
        icon: Option<Icon>,
        action: Option<ToastAction>,
        duration: Duration,
    ) {
        push_toast_parts(self, kind, message.into(), icon, action, None, duration);
    }

    fn push_toast_with_secondary(
        &mut self,
        kind: ToastKind,
        message: impl Into<SharedString>,
        icon: Option<Icon>,
        action: ToastAction,
        secondary_action: ToastAction,
        duration: Duration,
    ) {
        push_toast_parts(
            self,
            kind,
            message.into(),
            icon,
            Some(action),
            Some(secondary_action),
            duration,
        );
    }
}

fn push_toast_parts(
    cx: &mut App,
    kind: ToastKind,
    message: SharedString,
    icon: Option<Icon>,
    action: Option<ToastAction>,
    secondary_action: Option<ToastAction>,
    duration: Duration,
) {
    let id = cx
        .global_mut::<Toasts>()
        .append(kind, message, icon, action, secondary_action);

    if duration.is_zero() {
        return;
    }

    cx.spawn(async move |cx| {
        cx.background_executor().timer(duration).await;
        cx.update(|cx| cx.global_mut::<Toasts>().dismiss(id));
    })
    .detach();
}

pub fn toast_layer(palette: &ForgePalette, cx: &App) -> Option<AnyElement> {
    let density = cx.density();
    let toasts = cx.global::<Toasts>();
    if toasts.items().is_empty() {
        return None;
    }
    let mut column = div()
        .flex()
        .flex_col()
        .items_end()
        .gap(spacing(Spacing::Sm, density));

    for data in toasts.items() {
        let id = data.id;
        let mut card = toast_card(
            ("toast-card", id as usize),
            data.kind,
            data.message.clone(),
            palette,
        )
        .on_dismiss(("toast-dismiss", id as usize), move |_, _, cx: &mut App| {
            cx.global_mut::<Toasts>().dismiss(id);
        });
        if let Some(glyph) = data.icon {
            card = card.icon(glyph);
        }
        if let Some(fraction) = data.progress {
            card = card.progress(fraction);
        }
        if let Some(action) = &data.action {
            card = card.action(
                ("toast-action", id as usize),
                action.label.clone(),
                move |_, window, cx: &mut App| {
                    if let Some(data) = cx.global_mut::<Toasts>().take(id)
                        && let Some(action) = data.action
                    {
                        (action.on_action)(window, cx);
                    }
                },
            );
        }
        if let Some(secondary) = &data.secondary_action {
            card = card.secondary_action(
                ("toast-secondary", id as usize),
                secondary.label.clone(),
                move |_, window, cx: &mut App| {
                    if let Some(data) = cx.global_mut::<Toasts>().take(id)
                        && let Some(secondary) = data.secondary_action
                    {
                        (secondary.on_action)(window, cx);
                    }
                },
            );
        }
        column = column.child(card);
    }

    Some(
        deferred(
            div()
                .absolute()
                .right(spacing(Spacing::Md, density))
                .bottom(FOOTER_HEIGHT + spacing(Spacing::Sm, density))
                .child(column),
        )
        .with_priority(TOAST_PRIORITY)
        .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use gpui::TestAppContext;

    use super::{PushToast, Toasts};
    use crate::toast::{ToastAction, ToastData, ToastKind};

    const TIMED: Duration = Duration::from_secs(12);
    const LONG_AFTER: Duration = Duration::from_secs(60 * 60);

    fn push_pair(cx: &mut TestAppContext, duration: Duration) {
        cx.update(|cx| {
            cx.push_toast_with_secondary(
                ToastKind::Info,
                "update available",
                None,
                ToastAction::new("Download", |_window, _cx| {}),
                ToastAction::new("Skip", |_window, _cx| {}),
                duration,
            );
        });
    }

    fn action_labels(cx: &mut TestAppContext) -> Vec<(Option<String>, Option<String>)> {
        cx.update(|cx| {
            cx.global::<Toasts>()
                .items()
                .iter()
                .map(|toast| {
                    (
                        toast.action.as_ref().map(|a| a.label.to_string()),
                        toast.secondary_action.as_ref().map(|a| a.label.to_string()),
                    )
                })
                .collect()
        })
    }

    #[gpui::test]
    fn a_zero_duration_two_action_toast_survives_any_amount_of_time(cx: &mut TestAppContext) {
        cx.update(|cx| cx.set_global(Toasts::new()));
        push_pair(cx, Duration::ZERO);

        cx.executor().advance_clock(LONG_AFTER);
        cx.run_until_parked();

        assert_eq!(
            action_labels(cx),
            vec![(Some("Download".to_owned()), Some("Skip".to_owned()))],
        );
    }

    #[gpui::test]
    fn a_timed_two_action_toast_expires_once_its_duration_elapses(cx: &mut TestAppContext) {
        cx.update(|cx| cx.set_global(Toasts::new()));
        push_pair(cx, TIMED);

        cx.executor().advance_clock(TIMED);
        cx.run_until_parked();

        assert_eq!(action_labels(cx), Vec::new());
    }

    #[test]
    fn replacing_a_dismissed_toast_does_not_bring_it_back() {
        let mut toasts = Toasts::new();
        let id = toasts.show(ToastData::new(0, ToastKind::Info, "downloading"));
        toasts.dismiss(id);

        assert!(!toasts.replace(id, ToastData::new(0, ToastKind::Info, "42%")));
        assert!(toasts.items().is_empty());
    }

    #[test]
    fn replace_keeps_the_toast_id_and_position() {
        let mut toasts = Toasts::new();
        let first = toasts.show(ToastData::new(0, ToastKind::Info, "first"));
        let second = toasts.show(ToastData::new(0, ToastKind::Info, "second"));
        toasts.replace(first, ToastData::new(99, ToastKind::Info, "first, updated"));

        let shown: Vec<(u64, String)> = toasts
            .items()
            .iter()
            .map(|t| (t.id, t.message.to_string()))
            .collect();
        assert_eq!(
            shown,
            vec![
                (first, "first, updated".to_owned()),
                (second, "second".to_owned())
            ]
        );
    }
}
