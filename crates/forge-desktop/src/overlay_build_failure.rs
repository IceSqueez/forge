use std::fmt::Display;
use std::time::Duration;

use forge_components::{ToastKind, tr};
use forge_runtime::MaterializePass;
use gpui::App;

use crate::toasts::PushToast;

pub enum OverlayBuildFailure {
    Pages(usize),
    Pass(String),
}

impl OverlayBuildFailure {
    pub fn from_outcome<E: Display>(outcome: &Result<MaterializePass, E>) -> Option<Self> {
        match outcome {
            Ok(pass) if pass.failed > 0 => Some(Self::Pages(pass.failed)),
            Ok(_) => None,
            Err(error) => Some(Self::Pass(error.to_string())),
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::Pages(failed) => tr!(
                "overlays_toast_build_failed_pages",
                count = i64::try_from(*failed).unwrap_or(i64::MAX)
            ),
            Self::Pass(reason) => tr!("overlays_toast_build_failed_pass", reason = reason.as_str()),
        }
    }

    pub fn raise(&self, cx: &mut App) {
        cx.push_toast_full(ToastKind::Error, self.message(), None, None, Duration::ZERO);
    }
}
