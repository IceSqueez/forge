use serde::Deserialize;

use crate::UpdateError;
use crate::digest::Sha256Digest;
use crate::platform::{ArtifactKind, Platform};
use crate::trust::TrustedKeys;
use crate::version::{Version, parse_version};

pub const MANIFEST_FORMAT: u32 = 1;

const SIGNED_VERSION_KEY: &str = "version:";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateOffer {
    pub version: Version,
    pub published_at: String,
    pub release_page: String,
    pub artifacts: Vec<Artifact>,
}

impl UpdateOffer {
    pub fn artifact_for(&self, platform: Platform, kind: ArtifactKind) -> Option<&Artifact> {
        self.artifacts
            .iter()
            .find(|artifact| artifact.platform == platform.id() && artifact.kind == kind.id())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    pub platform: String,
    pub kind: String,
    pub name: String,
    pub url: String,
    pub sha256: Sha256Digest,
    pub size: u64,
}

#[derive(Deserialize)]
struct ManifestDocument {
    format: u32,
    version: String,
    published_at: String,
    release_page: String,
    artifacts: Vec<ArtifactEntry>,
}

#[derive(Deserialize)]
struct ArtifactEntry {
    platform: String,
    kind: String,
    name: String,
    url: String,
    sha256: String,
    size: u64,
}

pub fn verify_manifest(
    body: &[u8],
    signature: &str,
    keys: &TrustedKeys,
) -> Result<UpdateOffer, UpdateError> {
    let trusted_comment = keys.verified_trusted_comment(body, signature)?;
    let document: ManifestDocument = serde_json::from_slice(body)
        .map_err(|error| UpdateError::MalformedManifest(error.to_string()))?;
    if document.format != MANIFEST_FORMAT {
        return Err(UpdateError::UnsupportedManifestFormat(document.format));
    }

    let version = parse_version(&document.version)
        .ok_or_else(|| UpdateError::InvalidVersion(document.version.clone()))?;
    let signed_raw = signed_version(&trusted_comment).ok_or(UpdateError::MissingSignedVersion)?;
    let signed = parse_version(signed_raw)
        .ok_or_else(|| UpdateError::InvalidVersion(signed_raw.to_owned()))?;
    if signed != version {
        return Err(UpdateError::VersionMismatch {
            signed: signed.to_string(),
            manifest: version.to_string(),
        });
    }

    let artifacts = document
        .artifacts
        .into_iter()
        .map(artifact_from_entry)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(UpdateOffer {
        version,
        published_at: document.published_at,
        release_page: document.release_page,
        artifacts,
    })
}

fn signed_version(trusted_comment: &str) -> Option<&str> {
    trusted_comment
        .split_whitespace()
        .find_map(|token| token.strip_prefix(SIGNED_VERSION_KEY))
}

fn artifact_from_entry(entry: ArtifactEntry) -> Result<Artifact, UpdateError> {
    if !is_plain_file_name(&entry.name) {
        return Err(UpdateError::InvalidArtifactName);
    }
    let sha256 = Sha256Digest::from_hex(&entry.sha256).ok_or_else(|| {
        UpdateError::MalformedManifest(format!("artifact {} has an invalid sha256", entry.name))
    })?;
    Ok(Artifact {
        platform: entry.platform,
        kind: entry.kind,
        name: entry.name,
        url: entry.url,
        sha256,
        size: entry.size,
    })
}

pub(crate) fn is_plain_file_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'))
}
