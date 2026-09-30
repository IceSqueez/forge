use gpui::{App, Global, Window};
use tokio::sync::watch;

struct WindowPresence {
    visible: watch::Sender<bool>,
}

impl Global for WindowPresence {}

pub struct PresenceGate(Option<watch::Receiver<bool>>);

impl PresenceGate {
    pub fn of(cx: &App) -> Self {
        Self(
            cx.try_global::<WindowPresence>()
                .map(|presence| presence.visible.subscribe()),
        )
    }

    pub async fn until_visible(&mut self) {
        if let Some(visible) = self.0.as_mut()
            && visible.wait_for(|visible| *visible).await.is_err()
        {
            self.0 = None;
        }
    }
}

pub fn track_window_presence(window: &Window, cx: &mut App) {
    let (visible, _) = watch::channel(window.is_visible());
    cx.set_global(WindowPresence { visible });
    window
        .observe_window_visibility(|visibility, _, cx| {
            cx.global::<WindowPresence>()
                .visible
                .send_replace(visibility.is_visible());
        })
        .detach();
}
