use forge_storage::SettingDisclosure::Verbatim;
use forge_storage::SettingPortability::MachineLocal;
use forge_storage::{Flag, Setting};
use gpui::{App, Decorations, Global, Window, WindowDecorations};

pub(crate) const INTEGRATED_BY_DEFAULT: bool = true;

pub(crate) const APPLIES_LIVE: bool = cfg!(target_os = "linux");

pub(crate) const INTEGRATED_TITLEBAR: Setting<Flag> = Setting::new(
    "app.integrated_titlebar",
    Flag {
        default: INTEGRATED_BY_DEFAULT,
    },
    MachineLocal,
    Verbatim,
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TitlebarChoice {
    chosen_integrated: bool,
    launched_integrated: bool,
}

impl Global for TitlebarChoice {}

impl TitlebarChoice {
    pub(crate) fn launched(integrated: bool) -> Self {
        Self {
            chosen_integrated: integrated,
            launched_integrated: integrated,
        }
    }

    pub(crate) fn current(cx: &App) -> Self {
        cx.try_global::<Self>()
            .copied()
            .unwrap_or_else(|| Self::launched(INTEGRATED_BY_DEFAULT))
    }

    pub(crate) fn integrated(self) -> bool {
        self.chosen_integrated
    }

    pub(crate) fn choose(self, integrated: bool) -> Self {
        Self {
            chosen_integrated: integrated,
            ..self
        }
    }

    pub(crate) fn restart_pending(self) -> bool {
        !APPLIES_LIVE && self.chosen_integrated != self.launched_integrated
    }

    fn launched_integrated(self) -> bool {
        self.launched_integrated
    }
}

pub(crate) fn requested_decorations(integrated: bool) -> WindowDecorations {
    if integrated {
        WindowDecorations::Client
    } else {
        WindowDecorations::Server
    }
}

pub(crate) fn apply_titlebar_choice(integrated: bool, window: &mut Window) {
    if APPLIES_LIVE {
        window.request_decorations(requested_decorations(integrated));
    }
}

pub(crate) fn forge_titlebar_shown(window: &Window, cx: &App) -> bool {
    if APPLIES_LIVE {
        client_decorated(window) || TitlebarChoice::current(cx).integrated()
    } else {
        TitlebarChoice::current(cx).launched_integrated()
    }
}

pub(crate) fn forge_window_controls_shown(window: &Window, cx: &App) -> bool {
    if APPLIES_LIVE {
        client_decorated(window)
    } else {
        !cfg!(target_os = "macos") && forge_titlebar_shown(window, cx)
    }
}

fn client_decorated(window: &Window) -> bool {
    matches!(window.window_decorations(), Decorations::Client { .. })
}

pub(crate) fn repaint_on_decoration_change(window: &Window) {
    window
        .observe_window_appearance(|window, _| window.refresh())
        .detach();
}

#[cfg(test)]
mod tests {
    use super::{APPLIES_LIVE, TitlebarChoice, forge_titlebar_shown};

    #[test]
    fn switching_away_from_the_launched_mode_needs_a_restart_only_where_it_cannot_apply_live() {
        for launched in [true, false] {
            let switched = TitlebarChoice::launched(launched).choose(!launched);

            assert_eq!(
                switched.restart_pending(),
                !APPLIES_LIVE,
                "launched integrated = {launched}"
            );
        }
    }

    #[test]
    fn choosing_the_launched_mode_again_clears_the_pending_restart() {
        for launched in [true, false] {
            let returned = TitlebarChoice::launched(launched)
                .choose(!launched)
                .choose(launched);

            assert!(
                !returned.restart_pending(),
                "launched integrated = {launched}"
            );
        }
    }

    fn titlebar_shown_after(choice: TitlebarChoice, cx: &mut gpui::TestAppContext) -> bool {
        cx.update(|cx| cx.set_global(choice));
        cx.add_empty_window()
            .update(|window, cx| forge_titlebar_shown(window, cx))
    }

    #[cfg(target_os = "linux")]
    #[gpui::test]
    fn the_forge_title_bar_follows_the_live_choice_on_a_server_decorated_window(
        cx: &mut gpui::TestAppContext,
    ) {
        let shown = (
            titlebar_shown_after(TitlebarChoice::launched(true).choose(false), cx),
            titlebar_shown_after(TitlebarChoice::launched(false).choose(true), cx),
        );

        assert_eq!(shown, (false, true));
    }

    #[cfg(target_os = "linux")]
    #[gpui::test]
    fn a_server_decorated_window_never_draws_forge_window_controls(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.set_global(TitlebarChoice::launched(true)));

        let controls = cx
            .add_empty_window()
            .update(|window, cx| super::forge_window_controls_shown(window, cx));

        assert!(!controls);
    }

    #[cfg(not(target_os = "linux"))]
    #[gpui::test]
    fn the_forge_title_bar_keeps_the_launched_mode_until_a_restart(cx: &mut gpui::TestAppContext) {
        let shown = (
            titlebar_shown_after(TitlebarChoice::launched(true).choose(false), cx),
            titlebar_shown_after(TitlebarChoice::launched(false).choose(true), cx),
        );

        assert_eq!(shown, (true, false));
    }
}
