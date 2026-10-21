use forge_components::{DateShape, fmt_display};
use forge_components::{FONT_XS, ForgePalette, body_family, tr};
use gpui::{IntoElement, Rgba, div, prelude::*};
use time::OffsetDateTime;

use crate::update_check::{CheckResult, UpdatePlan};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCheckStatus {
    Idle,
    Checking,
    UpToDate { checked_at: OffsetDateTime },
    Found { version: String, downloading: bool },
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusTone {
    Muted,
    Success,
    Accent,
    Warning,
}

impl UpdateCheckStatus {
    pub fn from_result(result: CheckResult) -> Self {
        match result {
            CheckResult::NothingNew { checked_at } => UpdateCheckStatus::UpToDate { checked_at },
            CheckResult::Found { version, plan } => UpdateCheckStatus::Found {
                version,
                downloading: plan == UpdatePlan::Download,
            },
            CheckResult::Failed => UpdateCheckStatus::Failed,
        }
    }

    pub fn is_checking(&self) -> bool {
        matches!(self, UpdateCheckStatus::Checking)
    }

    pub fn offers_release_page(&self) -> bool {
        matches!(self, UpdateCheckStatus::Failed)
    }

    pub fn tone(&self) -> StatusTone {
        match self {
            UpdateCheckStatus::Idle | UpdateCheckStatus::Checking => StatusTone::Muted,
            UpdateCheckStatus::UpToDate { .. } => StatusTone::Success,
            UpdateCheckStatus::Found { .. } => StatusTone::Accent,
            UpdateCheckStatus::Failed => StatusTone::Warning,
        }
    }

    pub fn label(&self) -> Option<String> {
        match self {
            UpdateCheckStatus::Idle => None,
            UpdateCheckStatus::Checking => Some(tr!("settings_version_checking")),
            UpdateCheckStatus::UpToDate { checked_at } => Some(tr!(
                "settings_version_up_to_date",
                time = fmt_display(DateShape::Time, *checked_at)
            )),
            UpdateCheckStatus::Found {
                version,
                downloading: true,
            } => Some(tr!(
                "settings_version_update_downloading",
                version = version.clone()
            )),
            UpdateCheckStatus::Found {
                version,
                downloading: false,
            } => Some(tr!(
                "settings_version_update_found",
                version = version.clone()
            )),
            UpdateCheckStatus::Failed => Some(tr!("settings_version_check_failed")),
        }
    }
}

fn tone_color(tone: StatusTone, palette: &ForgePalette) -> Rgba {
    match tone {
        StatusTone::Muted => palette.text_muted,
        StatusTone::Success => palette.success,
        StatusTone::Accent => palette.brand,
        StatusTone::Warning => palette.warning,
    }
}

pub fn status_line(status: &UpdateCheckStatus, palette: &ForgePalette) -> Option<impl IntoElement> {
    let label = status.label()?;
    Some(
        div()
            .font_family(body_family())
            .text_size(FONT_XS)
            .text_color(tone_color(status.tone(), palette))
            .child(label),
    )
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use forge_components::{DateShape, fmt_display};
    use forge_storage::Language;
    use time::{OffsetDateTime, Time};

    use super::{StatusTone, UpdateCheckStatus};
    use crate::i18n::install_language;
    use crate::update_check::{CheckResult, UpdatePlan};

    fn at(hour: u8, minute: u8, second: u8) -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH
            .replace_time(Time::from_hms(hour, minute, second).expect("valid time"))
    }

    fn every_status() -> [UpdateCheckStatus; 6] {
        [
            UpdateCheckStatus::Idle,
            UpdateCheckStatus::Checking,
            UpdateCheckStatus::UpToDate {
                checked_at: at(12, 0, 0),
            },
            UpdateCheckStatus::Found {
                version: "0.9.0".to_owned(),
                downloading: true,
            },
            UpdateCheckStatus::Found {
                version: "0.9.0".to_owned(),
                downloading: false,
            },
            UpdateCheckStatus::Failed,
        ]
    }

    #[test]
    fn from_result_maps_every_check_result_and_downloads_only_a_download_plan() {
        let checked_at = at(14, 7, 0);
        for (result, expected) in [
            (
                CheckResult::NothingNew { checked_at },
                UpdateCheckStatus::UpToDate { checked_at },
            ),
            (
                CheckResult::Found {
                    version: "0.9.0".to_owned(),
                    plan: UpdatePlan::Download,
                },
                UpdateCheckStatus::Found {
                    version: "0.9.0".to_owned(),
                    downloading: true,
                },
            ),
            (
                CheckResult::Found {
                    version: "0.9.0".to_owned(),
                    plan: UpdatePlan::Offer,
                },
                UpdateCheckStatus::Found {
                    version: "0.9.0".to_owned(),
                    downloading: false,
                },
            ),
            (CheckResult::Failed, UpdateCheckStatus::Failed),
        ] {
            assert_eq!(
                UpdateCheckStatus::from_result(result.clone()),
                expected,
                "{result:?}"
            );
        }
    }

    #[test]
    fn only_a_running_check_reports_checking() {
        for status in every_status() {
            assert_eq!(
                status.is_checking(),
                status == UpdateCheckStatus::Checking,
                "{status:?}",
            );
        }
    }

    #[test]
    fn only_a_failed_check_offers_the_release_page() {
        for status in every_status() {
            assert_eq!(
                status.offers_release_page(),
                status == UpdateCheckStatus::Failed,
                "{status:?}",
            );
        }
    }

    #[test]
    fn tone_is_neutral_while_pending_and_distinct_for_each_outcome() {
        for (status, expected) in every_status().into_iter().zip([
            StatusTone::Muted,
            StatusTone::Muted,
            StatusTone::Success,
            StatusTone::Accent,
            StatusTone::Accent,
            StatusTone::Warning,
        ]) {
            assert_eq!(status.tone(), expected, "{status:?}");
        }
    }

    #[test]
    fn up_to_date_label_carries_the_check_time_in_the_display_time_shape() {
        install_language(Language::En);
        let checked_at = at(22, 30, 0);

        let label = UpdateCheckStatus::UpToDate { checked_at }
            .label()
            .expect("an up-to-date status has a label");

        assert!(
            label.contains(&fmt_display(DateShape::Time, checked_at)),
            "{label:?}"
        );
    }
}
