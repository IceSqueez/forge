mod apply;
mod apply_state;
mod check;
mod database;
mod digest;
mod download;
mod error;
mod github;
mod health;
mod http;
mod in_flight;
mod install;
mod journal;
mod leftovers;
mod manifest;
mod platform;
mod process;
mod record;
mod report;
mod source;
mod supervisor;
mod supervisor_lock;
mod trust;
mod version;

#[cfg(test)]
mod tests;

pub use apply_state::{ApplyMode, HandoffPlan, HandoffRequest};
pub use check::check_for_update;
pub use database::{DatabaseUnit, SchemaProbe};
pub use digest::Sha256Digest;
pub use download::ProgressReceiver;
pub use download::ProgressSender;
pub use download::{
    CancelSignal, DownloadProgress, StagedArtifact, download_artifact, progress_channel,
};
pub use error::UpdateError;
pub use github::{GithubRepo, PublishedRelease, fetch_latest_release, parse_latest_release};
pub use health::write_health_marker;
pub use http::UpdateClient;
pub use in_flight::update_in_flight;
pub use install::{
    InstallEnvironment, InstallKind, InstallLayout, ManualReason, UpdatableInstall,
    classify_install, detect_install, is_release_build, leftover_machine_install,
};
pub use manifest::{Artifact, UpdateOffer, verify_manifest};
pub use platform::{ArtifactKind, Platform};
pub use process::{ChildExit, ChildProcess, ProcessHost, SystemProcessHost};
pub use report::{ApplyFailure, DatabaseRestore, RollbackReason, UpdateReport, take_update_report};
pub use source::ManifestSource;
pub use supervisor::{
    CANDIDATE_FLAG, LaunchRole, StartAction, SupervisorContext, SupervisorOutcome,
    SupervisorSettings, launch_role, launch_supervisor, pending_start_action, run_supervisor,
};
pub use trust::TrustedKeys;
pub use version::Version;
pub use version::{is_newer, parse_version};

#[cfg(test)]
pub(crate) use health::{HealthMarker, read_health_marker};
#[cfg(test)]
pub(crate) use report::{OUTCOME_FILE, OUTCOME_FORMAT};
#[cfg(test)]
pub(crate) use supervisor::SUPERVISOR_FLAG;
