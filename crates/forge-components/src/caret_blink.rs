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
