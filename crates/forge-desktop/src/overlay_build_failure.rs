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

#[cfg(test)]
mod tests {
    use forge_runtime::MaterializePass;

    use super::OverlayBuildFailure;

    fn pass(
        materialized: usize,
        unavailable: usize,
        failed: usize,
    ) -> Result<MaterializePass, String> {
        Ok(MaterializePass {
            materialized,
            unavailable,
            failed,
        })
    }

    #[test]
    fn a_pass_without_failed_pages_raises_nothing_even_when_some_are_unavailable() {
        for outcome in [pass(0, 0, 0), pass(4, 0, 0), pass(2, 3, 0)] {
            assert!(
                OverlayBuildFailure::from_outcome(&outcome).is_none(),
                "{outcome:?}"
            );
        }
    }

    #[test]
    fn failed_pages_are_counted_from_one_upward() {
        for (outcome, expected) in [(pass(0, 0, 1), 1), (pass(5, 1, 3), 3)] {
            assert!(
                matches!(
                    OverlayBuildFailure::from_outcome(&outcome),
                    Some(OverlayBuildFailure::Pages(failed)) if failed == expected
                ),
                "{outcome:?}"
            );
        }
    }

    #[test]
    fn a_pass_that_could_not_run_carries_its_error_text() {
        let outcome: Result<MaterializePass, String> = Err("overlay root is missing".to_owned());

        assert!(matches!(
            OverlayBuildFailure::from_outcome(&outcome),
            Some(OverlayBuildFailure::Pass(reason)) if reason == "overlay root is missing"
        ));
    }
}
