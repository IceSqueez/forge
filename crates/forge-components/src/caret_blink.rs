use std::time::Duration;

use gpui::{Context, FocusHandle, Subscription, Task, Window};

const CARET_BLINK_INTERVAL: Duration = Duration::from_millis(530);

pub(crate) trait CaretHost: Sized + 'static {
    fn caret(&mut self) -> &mut CaretBlink;
}

pub(crate) struct CaretBlink {
    lit: bool,
    ticker: Option<Task<()>>,
    watchers: Vec<Subscription>,
}

impl CaretBlink {
    pub(crate) fn new() -> Self {
        Self {
            lit: true,
            ticker: None,
            watchers: Vec::new(),
        }
    }

    pub(crate) fn is_lit(&self) -> bool {
        self.lit
    }

    pub(crate) fn wake(&mut self) {
        self.lit = true;
    }

    pub(crate) fn watch<T: CaretHost>(
        &mut self,
        focus: &FocusHandle,
        window: &mut Window,
        cx: &mut Context<T>,
    ) {
        if !self.watchers.is_empty() {
            return;
        }
        let on_focus = cx.on_focus(focus, window, |host: &mut T, window, cx| {
            if set_caret_blinking(host, window.is_visible(), cx) {
                cx.notify();
            }
        });
        let on_blur = cx.on_blur(focus, window, |host: &mut T, _, cx| {
            if set_caret_blinking(host, false, cx) {
                cx.notify();
            }
        });
        let focus = focus.clone();
        let on_visibility =
            cx.observe_window_visibility(window, move |host: &mut T, visibility, window, cx| {
                let blinking = visibility.is_visible() && focus.is_focused(window);
                if set_caret_blinking(host, blinking, cx) {
                    cx.notify();
                }
            });
        self.watchers = vec![on_focus, on_blur, on_visibility];
    }
}

pub(crate) fn set_caret_blinking<T: CaretHost>(
    host: &mut T,
    blinking: bool,
    cx: &mut Context<T>,
) -> bool {
    let caret = host.caret();
    if blinking == caret.ticker.is_some() {
        return false;
    }
    caret.lit = true;
    caret.ticker = blinking.then(|| {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(CARET_BLINK_INTERVAL).await;
                let alive = this.update(cx, |host, cx| {
                    let caret = host.caret();
                    caret.lit = !caret.lit;
                    cx.notify();
                });
                if alive.is_err() {
                    break;
                }
            }
        })
    });
    true
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use gpui::{
        Entity, InteractiveElement, IntoElement, Render, VisualTestContext, WindowVisibility, div,
    };

    use super::*;

    struct Host {
        caret: CaretBlink,
        focus: FocusHandle,
    }

    impl CaretHost for Host {
        fn caret(&mut self) -> &mut CaretBlink {
            &mut self.caret
        }
    }

    impl Render for Host {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let focus = self.focus.clone();
            self.caret.watch(&focus, window, cx);
            div().track_focus(&focus)
        }
    }

    fn mount(cx: &mut gpui::TestAppContext) -> (Entity<Host>, &mut VisualTestContext) {
        let (host, vcx) = cx.add_window_view(|_window, cx| Host {
            caret: CaretBlink::new(),
            focus: cx.focus_handle(),
        });
        vcx.update(|window, _cx| window.activate_window());
        vcx.run_until_parked();
        (host, vcx)
    }

    fn focus(host: &Entity<Host>, vcx: &mut VisualTestContext) {
        vcx.update(|window, cx| window.focus(&host.read(cx).focus.clone(), cx));
        vcx.run_until_parked();
    }

    fn blur(vcx: &mut VisualTestContext) {
        vcx.update(|window, cx| window.blur(cx));
        vcx.run_until_parked();
    }

    fn set_visibility(vcx: &mut VisualTestContext, visibility: WindowVisibility) {
        vcx.simulate_visibility_change(visibility);
        vcx.run_until_parked();
    }

    fn ticking(host: &Entity<Host>, vcx: &mut VisualTestContext) -> bool {
        vcx.update(|_window, cx| host.read(cx).caret.ticker.is_some())
    }

    fn lit(host: &Entity<Host>, vcx: &mut VisualTestContext) -> bool {
        vcx.update(|_window, cx| host.read(cx).caret.is_lit())
    }

    fn one_blink(vcx: &mut VisualTestContext) {
        vcx.executor().advance_clock(CARET_BLINK_INTERVAL);
        vcx.run_until_parked();
    }

    #[gpui::test]
    fn an_unfocused_caret_keeps_no_timer_and_stays_lit(cx: &mut gpui::TestAppContext) {
        let (host, vcx) = mount(cx);

        one_blink(vcx);

        assert_eq!((ticking(&host, vcx), lit(&host, vcx)), (false, true));
    }

    #[gpui::test]
    fn focusing_starts_a_timer_that_toggles_the_caret_every_interval(
        cx: &mut gpui::TestAppContext,
    ) {
        let (host, vcx) = mount(cx);
        focus(&host, vcx);

        let mut seen = vec![lit(&host, vcx)];
        for _ in 0..2 {
            one_blink(vcx);
            seen.push(lit(&host, vcx));
        }

        assert_eq!(seen, vec![true, false, true]);
    }

    #[gpui::test]
    fn blurring_drops_the_timer_and_leaves_the_caret_lit(cx: &mut gpui::TestAppContext) {
        let (host, vcx) = mount(cx);
        focus(&host, vcx);
        one_blink(vcx);

        blur(vcx);
        one_blink(vcx);

        assert_eq!((ticking(&host, vcx), lit(&host, vcx)), (false, true));
    }

    #[gpui::test]
    fn hiding_the_window_drops_the_timer_of_a_focused_caret(cx: &mut gpui::TestAppContext) {
        let (host, vcx) = mount(cx);
        focus(&host, vcx);

        set_visibility(vcx, WindowVisibility::Hidden);

        assert!(!ticking(&host, vcx));
    }

    #[gpui::test]
    fn showing_the_window_again_restarts_the_timer_of_a_focused_caret(
        cx: &mut gpui::TestAppContext,
    ) {
        let (host, vcx) = mount(cx);
        focus(&host, vcx);
        set_visibility(vcx, WindowVisibility::Hidden);

        set_visibility(vcx, WindowVisibility::Visible);

        assert!(ticking(&host, vcx));
    }

    #[gpui::test]
    fn showing_the_window_does_not_start_a_timer_for_an_unfocused_caret(
        cx: &mut gpui::TestAppContext,
    ) {
        let (host, vcx) = mount(cx);
        set_visibility(vcx, WindowVisibility::Hidden);

        set_visibility(vcx, WindowVisibility::Visible);

        assert!(!ticking(&host, vcx));
    }

    #[gpui::test]
    fn focusing_while_the_window_is_hidden_starts_no_timer(cx: &mut gpui::TestAppContext) {
        let (host, vcx) = mount(cx);
        set_visibility(vcx, WindowVisibility::Hidden);

        focus(&host, vcx);

        assert!(!ticking(&host, vcx));
    }

    #[gpui::test]
    fn asking_for_the_state_the_caret_already_has_reports_no_change(cx: &mut gpui::TestAppContext) {
        let (host, vcx) = mount(cx);

        let changes = vcx.update(|_window, cx| {
            host.update(cx, |host, cx| {
                [true, true, false, false].map(|blinking| set_caret_blinking(host, blinking, cx))
            })
        });

        assert_eq!(changes, [true, false, true, false]);
    }
}
