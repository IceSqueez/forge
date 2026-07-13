use std::time::Duration;

use forge_components::{Icon, ToastAction, ToastData, ToastKind};
use gpui::{App, Global, SharedString};

/// How long a toast stays up before the auto-dismiss timer removes it, unless the
/// caller overrides the duration (a zero duration pins the toast until dismissed).
const DEFAULT_TOAST_MS: u64 = 4000;

/// The live toast queue, installed as a gpui `Global` at boot. Any handler pushes
/// through the [`PushToast`] extension trait; the app shell reads the list at render
/// and repaints on change via `observe_global`. Holds transient UI-feedback state
/// only — never runtime or domain data.
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

    /// The queued toasts, oldest first — the shell lays them bottom-anchored so the
    /// newest sits nearest the bottom-right corner.
    pub fn items(&self) -> &[ToastData] {
        &self.items
    }

    /// Appends a toast with a fresh monotonic id and returns that id, so the caller
    /// can schedule its auto-dismiss.
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

    /// Removes the toast with `id`, if still present (manual dismiss or timer edge).
    pub fn dismiss(&mut self, id: u64) {
        self.items.retain(|t| t.id != id);
    }

    /// Removes and returns the toast with `id`, so its owned action callback can be
    /// invoked after it leaves the queue.
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

/// Render/handler-side entry point for raising a toast. Implemented on `App`, so it
/// is reachable from any `Context<_>` through its `DerefMut<Target = App>` — a screen
/// handler calls `cx.push_toast(kind, message)` and the shell's toast host repaints.
pub trait PushToast {
    /// Raises a message-only toast that auto-dismisses after the default duration.
    fn push_toast(&mut self, kind: ToastKind, message: impl Into<SharedString>);

    /// Raises a toast with an optional glyph override and trailing action. A zero
    /// `duration` pins it open until the user (or the action) dismisses it.
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
        // `global_mut` pushes a NotifyGlobalObservers effect, so the shell repaints
        // the toast host as soon as the append lands.
        let id = self
            .global_mut::<Toasts>()
            .append(kind, message.into(), icon, action);

        if duration.is_zero() {
            return;
        }

        // Auto-dismiss on the background timer, hopping back to the app to mutate the
        // queue. The timer runs off the foreground executor, so it never blocks paint.
        self.spawn(async move |cx| {
            cx.background_executor().timer(duration).await;
            let _ = cx.update(|cx| cx.global_mut::<Toasts>().dismiss(id));
        })
        .detach();
    }
}
