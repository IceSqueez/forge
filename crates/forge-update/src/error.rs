use thiserror::Error;

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("the updater could not build its http client: {0}")]
    Client(String),
    #[error("the update request was declined by the rate limiter")]
    RateLimited,
    #[error("the update request failed: {0}")]
    Network(String),
    #[error("the update server answered with status {0}")]
    Status(u16),
    #[error("the update response exceeded {limit} bytes")]
    ResponseTooLarge { limit: u64 },
    #[error("no valid trusted update key is configured")]
    InvalidTrustedKey,
    #[error("the update signature is malformed")]
    MalformedSignature,
    #[error("the update manifest is not signed by a trusted key")]
    UntrustedSignature,
    #[error("the update manifest is malformed: {0}")]
    MalformedManifest(String),
    #[error("the latest release description is malformed: {0}")]
    MalformedRelease(String),
    #[error("update manifest format {0} is not supported")]
    UnsupportedManifestFormat(u32),
    #[error("the update signature carries no signed version")]
    MissingSignedVersion,
    #[error("the signed version {signed} does not match the manifest version {manifest}")]
    VersionMismatch { signed: String, manifest: String },
    #[error("{0:?} is not a valid version")]
    InvalidVersion(String),
    #[error("an update artifact name is not a plain file name")]
    InvalidArtifactName,
    #[error("the update artifact exceeds the {expected} bytes its manifest declares")]
    ArtifactOversized { expected: u64 },
    #[error("the update artifact has {actual} bytes but its manifest declares {expected}")]
    ArtifactSizeMismatch { expected: u64, actual: u64 },
    #[error("the update artifact does not match its manifest checksum")]
    ArtifactHashMismatch,
    #[error("the download was cancelled")]
    Cancelled,
    #[error("an update file operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("the update journal is malformed: {0}")]
    MalformedJournal(String),
    #[error("an update record is malformed: {0}")]
    MalformedRecord(String),
    #[error("no update is waiting to be applied")]
    NoPendingUpdate,
    #[error("another update supervisor is already running")]
    SupervisorBusy,
    #[error("the running forge did not exit in time")]
    ParentStillRunning,
    #[error("the update payload does not contain exactly one application bundle")]
    MalformedPayload,
    #[error("the installer exited with code {0}")]
    InstallerFailed(i32),
    #[error("the installer was terminated before it finished")]
    InstallerTerminated,
    #[error("the installer did not finish in time")]
    InstallerTimedOut,
    #[error("the previous build is missing, so it cannot be restored")]
    PreviousBuildMissing,
    #[error("an install path has no file name")]
    PathWithoutFileName,
}

impl UpdateError {
    pub(crate) fn network(error: reqwest::Error) -> Self {
        Self::Network(error.without_url().to_string())
    }
}
