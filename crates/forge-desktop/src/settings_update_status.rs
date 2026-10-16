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
                time = clock_label(*checked_at)
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

pub fn clock_label(at: OffsetDateTime) -> String {
    format!("{:02}:{:02}", at.hour(), at.minute())
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
