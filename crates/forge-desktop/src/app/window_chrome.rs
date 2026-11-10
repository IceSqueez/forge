use forge_storage::SettingDisclosure::Verbatim;
use forge_storage::SettingPortability::MachineLocal;
use forge_storage::{Flag, Setting};
use gpui::{App, Global, Window, WindowDecorations};

pub(crate) const INTEGRATED_TITLEBAR_ENABLED_BY_DEFAULT: bool = true;

pub(crate) const INTEGRATED_TITLEBAR: Setting<Flag> = Setting::new(
    "app.integrated_titlebar",
    Flag {
        default: INTEGRATED_TITLEBAR_ENABLED_BY_DEFAULT,
    },
    MachineLocal,
    Verbatim,
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WindowChrome {
    pub integrated_titlebar: bool,
}

impl Default for WindowChrome {
    fn default() -> Self {
        Self {
            integrated_titlebar: INTEGRATED_TITLEBAR_ENABLED_BY_DEFAULT,
        }
    }
}

impl Global for WindowChrome {}

pub(crate) trait ActiveWindowChrome {
    fn integrated_titlebar(&self) -> bool;
}

impl ActiveWindowChrome for App {
    fn integrated_titlebar(&self) -> bool {
        self.try_global::<WindowChrome>()
            .copied()
            .unwrap_or_default()
            .integrated_titlebar
    }
}

pub(crate) const fn can_switch_live() -> bool {
    cfg!(target_os = "linux")
}

pub(crate) fn set_integrated_titlebar(window: &mut Window, integrated_titlebar: bool) {
    window.request_decorations(if integrated_titlebar {
        WindowDecorations::Client
    } else {
        WindowDecorations::Server
    });
}

#[cfg(test)]
mod tests {
    use forge_storage::SettingCodec as _;

    use super::{
        INTEGRATED_TITLEBAR, INTEGRATED_TITLEBAR_ENABLED_BY_DEFAULT, WindowChrome, can_switch_live,
    };

    #[test]
    fn the_integrated_titlebar_is_enabled_by_default() {
        assert!(WindowChrome::default().integrated_titlebar);
        assert_eq!(
            INTEGRATED_TITLEBAR.codec().default_value(),
            INTEGRATED_TITLEBAR_ENABLED_BY_DEFAULT
        );
    }

    #[test]
    fn live_switching_is_limited_to_client_side_decoration_platforms() {
        assert_eq!(can_switch_live(), cfg!(target_os = "linux"));
    }
}
