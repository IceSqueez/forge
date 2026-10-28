use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::bundle::category::{BackupCategory, admits_content, includes_database};
use crate::error::TransferError;

pub(crate) const FORGE_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const BUNDLE_FORMAT_VERSION: u32 = 1;
pub const MANIFEST_ENTRY_NAME: &str = "manifest.json";
pub const ENCRYPTED_PAYLOAD_ENTRY_NAME: &str = "payload.age";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleKind {
    Full,
    Pack,
    Setup,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleOrigin {
    #[default]
    Manual,
    Scheduled,
}

impl BundleKind {
    const FULL_WIRE: &str = "full";
    const PACK_WIRE: &str = "pack";
    const SETUP_WIRE: &str = "setup";

    fn from_wire(kind: &str) -> Option<Self> {
        match kind {
            Self::FULL_WIRE => Some(Self::Full),
            Self::PACK_WIRE => Some(Self::Pack),
            Self::SETUP_WIRE => Some(Self::Setup),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => Self::FULL_WIRE,
            Self::Pack => Self::PACK_WIRE,
            Self::Setup => Self::SETUP_WIRE,
        }
    }
}

impl std::fmt::Display for BundleKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Full => "full backup",
            Self::Pack => "pack",
            Self::Setup => "setup",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptionScheme {
    pub id: String,
    pub version: u32,
}

impl EncryptionScheme {
    pub const AGE_SCRYPT_ID: &str = "age-scrypt";
    pub const AGE_SCRYPT_VERSION: u32 = 1;
    pub const AGE_X25519_WRAPPED_ID: &str = "age-x25519-wrapped";
    pub const AGE_X25519_WRAPPED_VERSION: u32 = 1;

    pub fn age_scrypt() -> Self {
        Self {
            id: Self::AGE_SCRYPT_ID.to_owned(),
            version: Self::AGE_SCRYPT_VERSION,
        }
    }

    pub fn age_x25519_wrapped() -> Self {
        Self {
            id: Self::AGE_X25519_WRAPPED_ID.to_owned(),
            version: Self::AGE_X25519_WRAPPED_VERSION,
        }
    }

    pub fn is_supported(&self) -> bool {
        self.is_passphrase() || self.is_wrapped_recipient()
    }

    pub fn is_passphrase(&self) -> bool {
        self.id == Self::AGE_SCRYPT_ID && self.version == Self::AGE_SCRYPT_VERSION
    }

    pub fn is_wrapped_recipient(&self) -> bool {
        self.id == Self::AGE_X25519_WRAPPED_ID && self.version == Self::AGE_X25519_WRAPPED_VERSION
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicManifest {
    pub format_version: u32,
    pub kind: BundleKind,
    pub encryption: EncryptionScheme,
    pub forge_version: String,
    pub schema_version: u32,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(default)]
    pub origin: BundleOrigin,
    #[serde(default = "BackupCategory::all")]
    pub categories: Vec<BackupCategory>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrapped_identity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentEntry {
    pub path: String,
    pub size: u64,
    pub sha256_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FullManifest {
    pub format_version: u32,
    pub kind: BundleKind,
    pub forge_version: String,
    pub schema_version: u32,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(default = "BackupCategory::all")]
    pub categories: Vec<BackupCategory>,
    pub contents: Vec<ContentEntry>,
}

#[derive(Deserialize)]
struct ManifestHead {
    format_version: u32,
    kind: String,
}

pub(crate) fn check_head(bytes: &[u8]) -> Result<BundleKind, TransferError> {
    let head: ManifestHead = serde_json::from_slice(bytes).map_err(TransferError::damaged)?;
    let kind =
        BundleKind::from_wire(&head.kind).ok_or(TransferError::UnknownKind { kind: head.kind })?;
    if head.format_version > BUNDLE_FORMAT_VERSION {
        return Err(TransferError::NewerFormat {
            found: head.format_version,
            supported: BUNDLE_FORMAT_VERSION,
        });
    }
    Ok(kind)
}

pub(crate) fn check_schema(found: u32) -> Result<(), TransferError> {
    if found > forge_storage::EXPECTED_SCHEMA_VERSION {
        return Err(TransferError::NewerSchema {
            found,
            supported: forge_storage::EXPECTED_SCHEMA_VERSION,
        });
    }
    Ok(())
}

pub(crate) fn expect_kind(found: BundleKind, expected: BundleKind) -> Result<(), TransferError> {
    if found != expected {
        return Err(TransferError::KindMismatch { expected, found });
    }
    Ok(())
}

pub(crate) fn parse_public(bytes: &[u8]) -> Result<PublicManifest, TransferError> {
    expect_kind(check_head(bytes)?, BundleKind::Full)?;
    let manifest: PublicManifest = serde_json::from_slice(bytes).map_err(TransferError::damaged)?;
    check_schema(manifest.schema_version)?;
    if !manifest.encryption.is_supported() {
        return Err(TransferError::UnknownEncryption {
            scheme: manifest.encryption.id,
            version: manifest.encryption.version,
        });
    }
    if !includes_database(&manifest.categories) {
        return Err(TransferError::damaged("the backup lists no database"));
    }
    if manifest.encryption.is_wrapped_recipient() && manifest.wrapped_identity.is_none() {
        return Err(TransferError::damaged("the backup carries no restore key"));
    }
    Ok(manifest)
}

pub(crate) fn parse_full(bytes: &[u8]) -> Result<FullManifest, TransferError> {
    expect_kind(check_head(bytes)?, BundleKind::Full)?;
    let manifest: FullManifest = serde_json::from_slice(bytes).map_err(TransferError::damaged)?;
    check_schema(manifest.schema_version)?;
    if !includes_database(&manifest.categories) {
        return Err(TransferError::damaged("the backup lists no database"));
    }
    if let Some(entry) = manifest
        .contents
        .iter()
        .find(|entry| !admits_content(&manifest.categories, &entry.path))
    {
        return Err(TransferError::ContentsMismatch {
            path: entry.path.clone(),
        });
    }
    Ok(manifest)
}
