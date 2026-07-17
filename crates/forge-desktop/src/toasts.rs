use std::time::Duration;

use forge_components::{Icon, ToastAction, ToastData, ToastKind};
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

    /// Oldest first.
    pub fn items(&self) -> &[ToastData] {
        &self.items
    }

    fn append(
        &mut self,
        kind: ToastKind,
        message: SharedString,
        icon: Option<Icon>,
        action: Option<ToastAction>,
    ) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.items.push(ToastData {
            id,
            kind,
            message,
            icon,
            action,
        });
        id
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

    /// A zero `duration` pins the toast open until dismissed.
    fn push_toast_full(
        &mut self,
        kind: ToastKind,
        message: impl Into<SharedString>,
        icon: Option<Icon>,
        action: Option<ToastAction>,
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
        let id = self
            .global_mut::<Toasts>()
            .append(kind, message.into(), icon, action);

        if duration.is_zero() {
            return;
        }

        self.spawn(async move |cx| {
            cx.background_executor().timer(duration).await;
            cx.update(|cx| cx.global_mut::<Toasts>().dismiss(id));
        })
        .detach();
    }
}
