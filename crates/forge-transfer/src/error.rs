use forge_storage::StorageError;

#[derive(Debug, thiserror::Error)]
pub enum TransferError {
    #[error("this file is not a forge bundle or backup")]
    NotABundle,

    #[error("this file holds an unknown kind of forge data ('{kind}')")]
    UnknownKind { kind: String },

    #[error("this file uses bundle format {found}; this forge reads up to format {supported}")]
    NewerFormat { found: u32, supported: u32 },

    #[error("this file holds schema {found}; this forge supports up to schema {supported}")]
    NewerSchema { found: u32, supported: u32 },

    #[error(
        "this backup's database (legacy schema {found}) is too old to restore; open it once with forge {release} first"
    )]
    SchemaNotAdoptable { found: u32, release: &'static str },

    #[error("this backup's database is not one forge can open")]
    UnrecognizedDatabase,

    #[error("this file is encrypted with an unknown scheme ('{scheme}' version {version})")]
    UnknownEncryption { scheme: String, version: u32 },

    #[error("this backup is encrypted; a passphrase is required")]
    PassphraseRequired,

    #[error("the passphrase must be at least {minimum} characters")]
    PassphraseTooShort { minimum: usize },

    #[error("wrong passphrase")]
    WrongPassphrase,

    #[error("this backup demands scrypt work factor {required}; forge accepts at most {cap}")]
    ExcessiveWorkFactor { required: u8, cap: u8 },

    #[error("the file is damaged: {reason}")]
    Damaged { reason: String },

    #[error("archive entry '{path}' is not allowed: {reason}")]
    UnsafeEntry { path: String, reason: &'static str },

    #[error("archive entry '{path}' is {size} bytes, over the {limit} byte limit")]
    EntryTooLarge { path: String, size: u64, limit: u64 },

    #[error("the archive unpacks to more than {limit} bytes")]
    ArchiveTooLarge { limit: u64 },

    #[error("the archive has more than {limit} entries")]
    TooManyEntries { limit: usize },

    #[error("archive entry '{path}' does not match its recorded digest or size")]
    DigestMismatch { path: String },

    #[error("archive entry '{path}' is missing from the archive or its manifest")]
    ContentsMismatch { path: String },

    #[error("the backup has no database")]
    MissingDatabase,

    #[error("the backup holds logins but not the key that decrypts them")]
    MissingCredentialsKey,

    #[error("'{path}' changed while the backup was being written; try again")]
    DataChangedDuringBackup { path: String },

    #[error("'{path}' has a name that cannot be stored in a backup")]
    UnsupportedFileName { path: String },

    #[error("encryption failed: {reason}")]
    Encryption { reason: String },

    #[error("the operation was interrupted: {reason}")]
    Interrupted { reason: String },

    #[error("an update is ready or being installed; finish or dismiss it before restoring")]
    UpdateInProgress,

    #[error("a restore is already pending")]
    RestoreAlreadyPending,

    #[error("the staged restore is not inside this data folder")]
    ForeignStaging,

    #[error("a restore bookkeeping file is unreadable: {reason}")]
    MalformedRecord { reason: String },

    #[error("'{path}' is in the way of the restore")]
    RestoreBlocked { path: String },

    #[error(transparent)]
    Storage(#[from] StorageError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl TransferError {
    pub(crate) fn damaged(reason: impl std::fmt::Display) -> Self {
        Self::Damaged {
            reason: reason.to_string(),
        }
    }
}
