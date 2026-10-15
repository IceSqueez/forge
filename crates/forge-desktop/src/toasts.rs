use std::time::Duration;

use forge_components::{Icon, ToastAction, ToastData, ToastKind, tr};
use gpui::{App, Global, SharedString};

const DEFAULT_TOAST_MS: u64 = 4000;

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

pub fn copy_to_clipboard(text: impl Into<String>, cx: &mut App) {
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.into()));
    cx.push_toast(ToastKind::Success, tr!("widget_copied_toast"));
}

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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use forge_components::{ToastAction, ToastKind};
    use gpui::TestAppContext;

    use super::{PushToast, Toasts};

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
}
