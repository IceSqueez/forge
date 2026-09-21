use crate::config::SOUND;
use crate::descriptor::OverlayConfig;
use crate::error::OverlayError;

pub const CLIP_REFERENCE_PREFIX: &str = "clip:";

pub const GENERATED_MEDIA_DIRECTORY: &str = "forge-media";

/// The only keys a reference is ever read from, so wording that opens with the prefix stays wording.
pub const MEDIA_KEYS: &[&str] = &[SOUND];

pub fn key_holds_media(key: &str) -> bool {
    MEDIA_KEYS.contains(&key)
}

const PAGE_PATH_SEPARATOR: char = '/';
const EXTENSION_SEPARATOR: char = '.';

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaValue<'a> {
    Empty,
    Clip(&'a str),
    File(&'a str),
}

pub fn read_media_value(stored: &str) -> MediaValue<'_> {
    match stored.strip_prefix(CLIP_REFERENCE_PREFIX) {
        Some(clip) => MediaValue::Clip(clip),
        None if stored.is_empty() => MediaValue::Empty,
        None => MediaValue::File(stored),
    }
}

pub fn clip_reference(clip_id: &str) -> String {
    format!("{CLIP_REFERENCE_PREFIX}{clip_id}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipReference {
    pub key: String,
    pub clip: String,
}

pub fn clip_references(config: &OverlayConfig) -> Vec<ClipReference> {
    config
        .iter()
        .filter(|(key, _)| key_holds_media(key))
        .filter_map(|(key, value)| match read_media_value(value.as_str()?) {
            MediaValue::Clip(clip) => Some(ClipReference {
                key: key.clone(),
                clip: clip.to_owned(),
            }),
            MediaValue::Empty | MediaValue::File(_) => None,
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMedia {
    key: String,
    file_name: String,
    bytes: Vec<u8>,
}

impl ResolvedMedia {
    pub fn new(
        key: impl Into<String>,
        identity: &str,
        extension: &str,
        bytes: Vec<u8>,
    ) -> Result<Self, OverlayError> {
        Ok(Self {
            key: key.into(),
            file_name: generated_file_name(identity, extension)?,
            bytes,
        })
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    pub fn page_path(&self) -> String {
        format!(
            "{GENERATED_MEDIA_DIRECTORY}{PAGE_PATH_SEPARATOR}{}",
            self.file_name
        )
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

fn generated_file_name(identity: &str, extension: &str) -> Result<String, OverlayError> {
    let safe = is_token(identity, true) && is_token(extension, false);
    if !safe {
        return Err(OverlayError::UnsafeIdentity(format!(
            "{identity}{EXTENSION_SEPARATOR}{extension}"
        )));
    }
    Ok(format!("{identity}{EXTENSION_SEPARATOR}{extension}"))
}

fn is_token(value: &str, hyphens_allowed: bool) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || (hyphens_allowed && byte == b'-')
        })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaIssue {
    UnknownClip {
        key: String,
        clip: String,
    },
    ClipOutsideLibrary {
        key: String,
        clip: String,
    },
    ClipBytesMissing {
        key: String,
        clip: String,
    },
    ClipKindMismatch {
        key: String,
        clip: String,
        format: String,
    },
    LookupFailed {
        key: String,
        clip: String,
        reason: String,
    },
}

impl MediaIssue {
    pub fn key(&self) -> &str {
        match self {
            Self::UnknownClip { key, .. }
            | Self::ClipOutsideLibrary { key, .. }
            | Self::ClipBytesMissing { key, .. }
            | Self::ClipKindMismatch { key, .. }
            | Self::LookupFailed { key, .. } => key,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OverlayMedia {
    pub resolved: Vec<ResolvedMedia>,
    pub issues: Vec<MediaIssue>,
}

impl OverlayMedia {
    pub fn for_key(&self, key: &str) -> Option<&ResolvedMedia> {
        self.resolved.iter().find(|media| media.key == key)
    }
}

/// `None` leaves the stored value on the page untouched, which is what keeps a hand-placed file working.
pub fn emitted_media_value(
    key: &str,
    stored: &str,
    resolved: Option<&ResolvedMedia>,
) -> Option<String> {
    if !key_holds_media(key) {
        return None;
    }
    match read_media_value(stored) {
        MediaValue::Clip(_) => Some(resolved.map(ResolvedMedia::page_path).unwrap_or_default()),
        MediaValue::Empty | MediaValue::File(_) => None,
    }
}
