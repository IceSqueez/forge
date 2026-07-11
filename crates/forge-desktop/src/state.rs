use gpui::{Context, Window, div, prelude::*, rgb};

/// Aggregate root UI state: a single entity rather than a set of topic-scoped
/// entities.
///
/// Holds only values sourced from the runtime bus. The runtime owns all
/// tokio-side state; `UiState` holds none of it — the boot bridge task feeds
/// values in via `on_timer_tick`.
pub struct UiState {
    /// Runtime uptime in whole seconds, advanced once per `timer.tick` bus event.
    uptime_secs: u64,
}

impl UiState {
    pub fn new() -> Self {
        Self { uptime_secs: 0 }
    }

    /// Advances uptime by one second. Invoked by the runtime→UI bridge on each
    /// observed `timer.tick`; the caller pairs it with `cx.notify()` to repaint.
    pub fn on_timer_tick(&mut self) {
        self.uptime_secs = self.uptime_secs.saturating_add(1);
    }

    pub fn uptime_secs(&self) -> u64 {
        self.uptime_secs
    }
}

impl Default for UiState {
    fn default() -> Self {
        Self::new()
    }
}

impl Render for UiState {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .justify_center()
            .items_center()
            .gap_2()
            .bg(rgb(0x1e1e2e))
            .text_color(rgb(0xcdd6f4))
            .child(div().text_xl().child("forge runtime bridge"))
            .child(
                div()
                    .text_2xl()
                    .child(format!("uptime {}s", self.uptime_secs())),
            )
    }
}
