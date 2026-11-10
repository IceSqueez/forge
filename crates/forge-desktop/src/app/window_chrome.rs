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
