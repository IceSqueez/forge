use sha2::{Digest, Sha256};

use crate::config::{ICON, SOUND};
use crate::descriptor::OverlayConfig;
use crate::error::OverlayError;
use crate::icons::curated_icon;

pub const CLIP_REFERENCE_PREFIX: &str = "clip:";

pub const IMAGE_REFERENCE_PREFIX: &str = "image:";

pub const GENERATED_MEDIA_DIRECTORY: &str = "forge-media";

/// The only keys a reference is ever read from, so wording that opens with a prefix stays wording.
pub const MEDIA_KEYS: &[&str] = &[SOUND, ICON];

const PAGE_PATH_SEPARATOR: char = '/';
const EXTENSION_SEPARATOR: char = '.';
const IDENTITY_SEPARATOR: char = '-';
const CONTENT_DIGEST_LABEL: &str = "sha256";
const GLYPH_EXTENSION: &str = "svg";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaSlot {
    Sound,
    Icon,
}

pub fn media_slot(key: &str) -> Option<MediaSlot> {
    match key {
        SOUND => Some(MediaSlot::Sound),
        ICON => Some(MediaSlot::Icon),
        _ => None,
    }
}

pub fn key_holds_media(key: &str) -> bool {
    media_slot(key).is_some()
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconValue<'a> {
    Empty,
    Glyph(&'a str),
    Image(&'a str),
}

pub fn read_icon_value(stored: &str) -> IconValue<'_> {
    match stored.strip_prefix(IMAGE_REFERENCE_PREFIX) {
        Some(image) => IconValue::Image(image),
        None if stored.is_empty() => IconValue::Empty,
        None => IconValue::Glyph(stored),
    }
}

pub fn image_reference(blob_id: &str) -> String {
    format!("{IMAGE_REFERENCE_PREFIX}{blob_id}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipReference {
    pub key: String,
    pub clip: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageReference {
    pub key: String,
    pub image: String,
}

pub fn clip_references(config: &OverlayConfig) -> Vec<ClipReference> {
    stored_values(config, MediaSlot::Sound)
        .filter_map(|(key, value)| match read_media_value(value) {
            MediaValue::Clip(clip) => Some(ClipReference {
                key,
                clip: clip.to_owned(),
            }),
            MediaValue::Empty | MediaValue::File(_) => None,
        })
        .collect()
}

pub fn image_references(config: &OverlayConfig) -> Vec<ImageReference> {
    stored_values(config, MediaSlot::Icon)
        .filter_map(|(key, value)| match read_icon_value(value) {
            IconValue::Image(image) => Some(ImageReference {
                key,
                image: image.to_owned(),
            }),
            IconValue::Empty | IconValue::Glyph(_) => None,
        })
        .collect()
}

pub fn glyph_media(config: &OverlayConfig) -> OverlayMedia {
    let mut media = OverlayMedia::default();
    for (key, value) in stored_values(config, MediaSlot::Icon) {
        let IconValue::Glyph(name) = read_icon_value(value) else {
            continue;
        };
        match resolve_glyph(&key, name) {
            Ok(resolved) => media.resolved.push(resolved),
            Err(issue) => media.issues.push(issue),
        }
    }
    media
}

fn resolve_glyph(key: &str, name: &str) -> Result<ResolvedMedia, MediaIssue> {
    let Some(icon) = curated_icon(name) else {
        return Err(MediaIssue::UnknownGlyph {
            key: key.to_owned(),
            glyph: name.to_owned(),
        });
    };

    let bytes = icon.bytes();
    ResolvedMedia::new(
        key,
        &content_identity(bytes),
        GLYPH_EXTENSION,
        bytes.to_vec(),
    )
    .map_err(|error| MediaIssue::LookupFailed {
        key: key.to_owned(),
        reference: name.to_owned(),
        reason: error.to_string(),
    })
}

fn content_identity(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let digest = Sha256::digest(bytes);
    let mut identity = String::with_capacity(CONTENT_DIGEST_LABEL.len() + 1 + digest.len() * 2);
    identity.push_str(CONTENT_DIGEST_LABEL);
    identity.push(IDENTITY_SEPARATOR);
    for byte in digest {
        let _ = write!(identity, "{byte:02x}");
    }
    identity
}

fn stored_values(config: &OverlayConfig, slot: MediaSlot) -> impl Iterator<Item = (String, &str)> {
    config
        .iter()
        .filter(move |(key, _)| media_slot(key) == Some(slot))
        .filter_map(|(key, value)| Some((key.clone(), value.as_str()?)))
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
    UnknownGlyph {
        key: String,
        glyph: String,
    },
    UnknownImage {
        key: String,
        image: String,
    },
    ImageBytesMissing {
        key: String,
        image: String,
    },
    ImageKindMismatch {
        key: String,
        image: String,
        format: String,
    },
    LookupFailed {
        key: String,
        reference: String,
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
            | Self::UnknownGlyph { key, .. }
            | Self::UnknownImage { key, .. }
            | Self::ImageBytesMissing { key, .. }
            | Self::ImageKindMismatch { key, .. }
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
    if media_slot(key) != Some(MediaSlot::Sound) {
        return None;
    }
    match read_media_value(stored) {
        MediaValue::Clip(_) => Some(resolved.map(ResolvedMedia::page_path).unwrap_or_default()),
        MediaValue::Empty | MediaValue::File(_) => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmittedIcon {
    pub file: String,
    pub tintable: bool,
}

pub fn emitted_icon(stored: &str, resolved: Option<&ResolvedMedia>) -> EmittedIcon {
    let file = resolved.map(ResolvedMedia::page_path).unwrap_or_default();
    match read_icon_value(stored) {
        IconValue::Empty => EmittedIcon {
            file: String::new(),
            tintable: false,
        },
        IconValue::Glyph(_) => EmittedIcon {
            file,
            tintable: true,
        },
        IconValue::Image(_) => EmittedIcon {
            file,
            tintable: false,
        },
    }
}
