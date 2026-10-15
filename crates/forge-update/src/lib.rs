mod atomic_file;
mod check;
mod digest;
mod download;
mod error;
mod http;
mod install;
mod journal;
mod manifest;
mod platform;
mod source;
mod trust;
mod version;

pub use check::{check_for_update, fetch_offer};
pub use digest::Sha256Digest;
pub use download::{
    CancelSignal, DownloadProgress, ProgressReceiver, ProgressSender, STAGING_PREFIX,
    StagedArtifact, download_artifact, progress_channel, staged_path,
};
pub use error::UpdateError;
pub use http::UpdateClient;
pub use install::{
    InstallEnvironment, InstallKind, InstallLayout, ManualReason, UpdatableInstall,
    classify_install, detect_install, is_release_build, leftover_machine_install,
};
pub use journal::{JOURNAL_FILE, JOURNAL_FORMAT, UpdateJournal};
pub use manifest::{Artifact, MANIFEST_FORMAT, UpdateOffer, verify_manifest};
pub use platform::{ArtifactKind, Platform};
pub use source::{MANIFEST_ASSET, ManifestSource, SIGNATURE_ASSET};
pub use trust::TrustedKeys;
pub use version::{Version, is_newer, parse_version};
