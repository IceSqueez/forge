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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use gpui::{Empty, VisualTestContext, WindowVisibility};

    use super::*;

    fn tracked_window(cx: &mut gpui::TestAppContext) -> &mut VisualTestContext {
        let (_view, vcx) = cx.add_window_view(|window, cx| {
            track_window_presence(window, cx);
            Empty
        });
        vcx
    }

    fn wait_in_background(gate: PresenceGate, vcx: &mut VisualTestContext) -> Rc<Cell<bool>> {
        let passed = Rc::new(Cell::new(false));
        let flag = Rc::clone(&passed);
        let mut gate = gate;
        vcx.update(|_window, cx| {
            cx.spawn(async move |_cx| {
                gate.until_visible().await;
                flag.set(true);
            })
            .detach();
        });
        vcx.run_until_parked();
        passed
    }

    #[gpui::test]
    fn a_gate_without_a_tracked_window_never_blocks(cx: &mut gpui::TestAppContext) {
        let (_view, vcx) = cx.add_window_view(|_window, _cx| Empty);
        let gate = vcx.update(|_window, cx| PresenceGate::of(cx));

        let passed = wait_in_background(gate, vcx);

        assert!(passed.get());
    }

    #[gpui::test]
    fn a_visible_window_lets_the_loop_through_at_once(cx: &mut gpui::TestAppContext) {
        let vcx = tracked_window(cx);
        let gate = vcx.update(|_window, cx| PresenceGate::of(cx));

        let passed = wait_in_background(gate, vcx);

        assert!(passed.get());
    }

    #[gpui::test]
    fn a_window_already_hidden_when_tracking_starts_holds_the_loop(cx: &mut gpui::TestAppContext) {
        let (_view, vcx) = cx.add_window_view(|_window, _cx| Empty);
        vcx.simulate_visibility_change(WindowVisibility::Hidden);
        vcx.update(|window, cx| track_window_presence(window, cx));
        let gate = vcx.update(|_window, cx| PresenceGate::of(cx));

        let passed = wait_in_background(gate, vcx);

        assert!(!passed.get());
    }

    #[gpui::test]
    fn showing_the_window_releases_a_held_loop_without_waiting_for_a_tick(
        cx: &mut gpui::TestAppContext,
    ) {
        let vcx = tracked_window(cx);
        vcx.simulate_visibility_change(WindowVisibility::Hidden);
        let gate = vcx.update(|_window, cx| PresenceGate::of(cx));
        let passed = wait_in_background(gate, vcx);

        vcx.simulate_visibility_change(WindowVisibility::Visible);
        vcx.run_until_parked();

        assert!(passed.get());
    }

    #[gpui::test]
    fn a_gate_taken_while_visible_holds_once_the_window_is_hidden(cx: &mut gpui::TestAppContext) {
        let vcx = tracked_window(cx);
        let gate = vcx.update(|_window, cx| PresenceGate::of(cx));
        vcx.simulate_visibility_change(WindowVisibility::Hidden);

        let passed = wait_in_background(gate, vcx);

        assert!(!passed.get());
    }
}
