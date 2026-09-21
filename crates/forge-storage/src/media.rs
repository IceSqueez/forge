use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::StorageError;

pub const MEDIA_CONTENT_HASH: &str = "sha256";
pub const MEDIA_CONTENT_DIGEST_BYTES: usize = 32;
const MEDIA_BLOB_ID_SEPARATOR: char = '-';

const BYTES_PER_MIB: u64 = 1024 * 1024;
pub const MAX_AUDIO_BLOB_BYTES: u64 = 50 * BYTES_PER_MIB;
pub const MAX_IMAGE_BLOB_BYTES: u64 = 10 * BYTES_PER_MIB;
pub const MEDIA_BLOB_HARD_CEILING_BYTES: u64 = MAX_AUDIO_BLOB_BYTES;

const MEDIA_LABEL_MAX_CHARS: usize = 120;
const MEDIA_LABEL_FALLBACK: &str = "untitled";
const EXTENSION_SEPARATOR: char = '.';

const MAGIC_RIFF: &[u8] = b"RIFF";
const MAGIC_RIFF_WAVE: &[u8] = b"WAVE";
const MAGIC_RIFF_WEBP: &[u8] = b"WEBP";
const RIFF_FORM_OFFSET: usize = 8;
const MAGIC_OGG: &[u8] = b"OggS";
const MAGIC_FLAC: &[u8] = b"fLaC";
const MAGIC_ID3: &[u8] = b"ID3";
const MP3_FRAME_SYNC_BYTE: u8 = 0xFF;
const MP3_FRAME_SYNC_MASK: u8 = 0xE0;
const MP3_FRAME_SYNC_LEN: usize = 2;
const MAGIC_PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
const MAGIC_GIF87A: &[u8] = b"GIF87a";
const MAGIC_GIF89A: &[u8] = b"GIF89a";
const ISO_BMFF_BOX_TYPE_OFFSET: usize = 4;
const MAGIC_ISO_BMFF_FTYP: &[u8] = b"ftyp";
const ISO_BMFF_BRAND_OFFSET: usize = 8;
const ISO_BMFF_BRAND_LEN: usize = 4;
const ISO_BMFF_AUDIO_BRANDS: [&[u8]; 2] = [b"M4A ", b"M4B "];
const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
const SVG_SNIFF_WINDOW_BYTES: usize = 1024;
const SVG_XML_PROLOG: &str = "<?xml";
const SVG_ROOT_ELEMENT: &str = "<svg";

const FORMAT_TOKEN_WAV: &str = "wav";
const FORMAT_TOKEN_MP3: &str = "mp3";
const FORMAT_TOKEN_OGG: &str = "ogg";
const FORMAT_TOKEN_FLAC: &str = "flac";
const FORMAT_TOKEN_M4A: &str = "m4a";
const FORMAT_TOKEN_PNG: &str = "png";
const FORMAT_TOKEN_GIF: &str = "gif";
const FORMAT_TOKEN_WEBP: &str = "webp";
const FORMAT_TOKEN_SVG: &str = "svg";
const EXTENSION_ALIAS_MP4: &str = "mp4";
const EXTENSION_ALIAS_M4B: &str = "m4b";

const KIND_TOKEN_AUDIO: &str = "audio";
const KIND_TOKEN_IMAGE: &str = "image";

const REFERRER_TOKEN_SOUNDBOARD_CLIP: &str = "soundboard_clip";
const REFERRER_TOKEN_OVERLAY: &str = "overlay";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Audio,
    Image,
}

impl MediaKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Audio => KIND_TOKEN_AUDIO,
            Self::Image => KIND_TOKEN_IMAGE,
        }
    }

    pub const fn max_blob_bytes(self) -> u64 {
        match self {
            Self::Audio => MAX_AUDIO_BLOB_BYTES,
            Self::Image => MAX_IMAGE_BLOB_BYTES,
        }
    }
}

impl std::fmt::Display for MediaKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaFormat {
    Wav,
    Mp3,
    Ogg,
    Flac,
    M4a,
    Png,
    Gif,
    Webp,
    Svg,
}

impl MediaFormat {
    pub const ACCEPTED: &'static [MediaFormat] = &[
        Self::Wav,
        Self::Mp3,
        Self::Ogg,
        Self::Flac,
        Self::M4a,
        Self::Png,
        Self::Gif,
        Self::Webp,
        Self::Svg,
    ];

    /// Doubles as the persisted token and the on-disk file extension.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Wav => FORMAT_TOKEN_WAV,
            Self::Mp3 => FORMAT_TOKEN_MP3,
            Self::Ogg => FORMAT_TOKEN_OGG,
            Self::Flac => FORMAT_TOKEN_FLAC,
            Self::M4a => FORMAT_TOKEN_M4A,
            Self::Png => FORMAT_TOKEN_PNG,
            Self::Gif => FORMAT_TOKEN_GIF,
            Self::Webp => FORMAT_TOKEN_WEBP,
            Self::Svg => FORMAT_TOKEN_SVG,
        }
    }

    pub const fn media_type(self) -> &'static str {
        match self {
            Self::Wav => "audio/wav",
            Self::Mp3 => "audio/mpeg",
            Self::Ogg => "audio/ogg",
            Self::Flac => "audio/flac",
            Self::M4a => "audio/mp4",
            Self::Png => "image/png",
            Self::Gif => "image/gif",
            Self::Webp => "image/webp",
            Self::Svg => "image/svg+xml",
        }
    }

    pub const fn kind(self) -> MediaKind {
        match self {
            Self::Wav | Self::Mp3 | Self::Ogg | Self::Flac | Self::M4a => MediaKind::Audio,
            Self::Png | Self::Gif | Self::Webp | Self::Svg => MediaKind::Image,
        }
    }

    pub fn parse(token: &str) -> Option<Self> {
        Self::ACCEPTED
            .iter()
            .copied()
            .find(|format| format.as_str() == token)
    }

    /// A caller-supplied extension is a first guess only; [`sniff`] decides.
    pub fn from_extension(extension: &str) -> Option<Self> {
        let lowered = extension.to_ascii_lowercase();
        match lowered.as_str() {
            EXTENSION_ALIAS_MP4 | EXTENSION_ALIAS_M4B => Some(Self::M4a),
            other => Self::parse(other),
        }
    }
}

impl std::fmt::Display for MediaFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

fn starts_with(bytes: &[u8], magic: &[u8]) -> bool {
    bytes.len() >= magic.len() && &bytes[..magic.len()] == magic
}

fn matches_at(bytes: &[u8], offset: usize, magic: &[u8]) -> bool {
    let end = offset + magic.len();
    bytes.len() >= end && &bytes[offset..end] == magic
}

fn sniff_riff(bytes: &[u8]) -> Option<MediaFormat> {
    if !starts_with(bytes, MAGIC_RIFF) {
        return None;
    }
    if matches_at(bytes, RIFF_FORM_OFFSET, MAGIC_RIFF_WAVE) {
        return Some(MediaFormat::Wav);
    }
    if matches_at(bytes, RIFF_FORM_OFFSET, MAGIC_RIFF_WEBP) {
        return Some(MediaFormat::Webp);
    }
    None
}

fn sniff_iso_bmff_audio(bytes: &[u8]) -> Option<MediaFormat> {
    if !matches_at(bytes, ISO_BMFF_BOX_TYPE_OFFSET, MAGIC_ISO_BMFF_FTYP) {
        return None;
    }
    let end = ISO_BMFF_BRAND_OFFSET + ISO_BMFF_BRAND_LEN;
    if bytes.len() < end {
        return None;
    }
    let brand = &bytes[ISO_BMFF_BRAND_OFFSET..end];
    ISO_BMFF_AUDIO_BRANDS
        .contains(&brand)
        .then_some(MediaFormat::M4a)
}

fn sniff_mp3(bytes: &[u8]) -> Option<MediaFormat> {
    if starts_with(bytes, MAGIC_ID3) {
        return Some(MediaFormat::Mp3);
    }
    if bytes.len() < MP3_FRAME_SYNC_LEN {
        return None;
    }
    (bytes[0] == MP3_FRAME_SYNC_BYTE && bytes[1] & MP3_FRAME_SYNC_MASK == MP3_FRAME_SYNC_MASK)
        .then_some(MediaFormat::Mp3)
}

fn sniff_svg(bytes: &[u8]) -> Option<MediaFormat> {
    let body = bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes);
    let window = &body[..body.len().min(SVG_SNIFF_WINDOW_BYTES)];
    let text = match std::str::from_utf8(window) {
        Ok(text) => text,
        Err(error) => std::str::from_utf8(&window[..error.valid_up_to()]).ok()?,
    };
    let opening = text.trim_start();
    let prologue_is_markup =
        opening.starts_with(SVG_XML_PROLOG) || opening.starts_with(SVG_ROOT_ELEMENT);
    (prologue_is_markup && text.contains(SVG_ROOT_ELEMENT)).then_some(MediaFormat::Svg)
}

pub fn sniff(bytes: &[u8]) -> Option<MediaFormat> {
    if starts_with(bytes, MAGIC_PNG) {
        return Some(MediaFormat::Png);
    }
    if starts_with(bytes, MAGIC_GIF87A) || starts_with(bytes, MAGIC_GIF89A) {
        return Some(MediaFormat::Gif);
    }
    if let Some(format) = sniff_riff(bytes) {
        return Some(format);
    }
    if starts_with(bytes, MAGIC_OGG) {
        return Some(MediaFormat::Ogg);
    }
    if starts_with(bytes, MAGIC_FLAC) {
        return Some(MediaFormat::Flac);
    }
    if let Some(format) = sniff_iso_bmff_audio(bytes) {
        return Some(format);
    }
    if let Some(format) = sniff_svg(bytes) {
        return Some(format);
    }
    sniff_mp3(bytes)
}

/// Display text only; no part of it ever reaches a path component.
pub fn sanitize_label(raw: &str) -> String {
    let mut label = String::with_capacity(raw.len());
    let mut pending_space = false;

    for ch in raw.chars() {
        if std::path::is_separator(ch) || ch.is_control() {
            continue;
        }
        if ch.is_whitespace() {
            pending_space = !label.is_empty();
            continue;
        }
        if pending_space {
            label.push(' ');
            pending_space = false;
        }
        label.push(ch);
    }

    let trimmed = label.trim_start_matches(EXTENSION_SEPARATOR).trim();
    let clipped: String = trimmed.chars().take(MEDIA_LABEL_MAX_CHARS).collect();

    if clipped.is_empty() {
        MEDIA_LABEL_FALLBACK.to_owned()
    } else {
        clipped
    }
}

fn claimed_format(label: &str) -> Option<MediaFormat> {
    let extension = label.rsplit_once(EXTENSION_SEPARATOR).map(|(_, ext)| ext)?;
    MediaFormat::from_extension(extension)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedMedia {
    pub format: MediaFormat,
    pub label: String,
}

/// The single admission gate: every backend routes incoming bytes through it.
pub fn accept_media(label: &str, bytes: &[u8]) -> Result<AcceptedMedia, StorageError> {
    let sanitized = sanitize_label(label);

    let Some(detected) = sniff(bytes) else {
        return Err(StorageError::MediaUnsupported { label: sanitized });
    };

    if let Some(claimed) = claimed_format(label)
        && claimed != detected
    {
        return Err(StorageError::MediaTypeMismatch {
            label: sanitized,
            claimed,
            detected,
        });
    }

    let size = bytes.len() as u64;
    let kind = detected.kind();
    let limit = kind.max_blob_bytes();
    if size > limit {
        return Err(StorageError::MediaTooLarge {
            label: sanitized,
            size,
            limit,
            kind,
        });
    }

    Ok(AcceptedMedia {
        format: detected,
        label: sanitized,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MediaBlobId(String);

impl MediaBlobId {
    pub fn from_digest(digest: &[u8; MEDIA_CONTENT_DIGEST_BYTES]) -> Self {
        use std::fmt::Write as _;

        let mut id =
            String::with_capacity(MEDIA_CONTENT_HASH.len() + 1 + MEDIA_CONTENT_DIGEST_BYTES * 2);
        id.push_str(MEDIA_CONTENT_HASH);
        id.push(MEDIA_BLOB_ID_SEPARATOR);
        for byte in digest {
            let _ = write!(id, "{byte:02x}");
        }
        Self(id)
    }

    pub fn from_stored(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for MediaBlobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaBlob {
    pub id: MediaBlobId,
    pub format: MediaFormat,
    pub byte_size: u64,
    pub label: String,
    pub imported_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaReferrerKind {
    SoundboardClip,
    Overlay,
}

impl MediaReferrerKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SoundboardClip => REFERRER_TOKEN_SOUNDBOARD_CLIP,
            Self::Overlay => REFERRER_TOKEN_OVERLAY,
        }
    }

    pub fn parse(token: &str) -> Option<Self> {
        match token {
            REFERRER_TOKEN_SOUNDBOARD_CLIP => Some(Self::SoundboardClip),
            REFERRER_TOKEN_OVERLAY => Some(Self::Overlay),
            _ => None,
        }
    }
}

impl std::fmt::Display for MediaReferrerKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A domain row plus the slot inside it, so one row can point at several blobs.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MediaReferrer {
    pub kind: MediaReferrerKind,
    pub id: String,
    pub slot: String,
}

impl MediaReferrer {
    pub fn new(kind: MediaReferrerKind, id: impl Into<String>, slot: impl Into<String>) -> Self {
        Self {
            kind,
            id: id.into(),
            slot: slot.into(),
        }
    }
}

#[cfg_attr(feature = "test-mocks", mockall::automock)]
#[async_trait]
pub trait MediaRepo: Send + Sync {
    /// Admits bytes under a content identity; storing identical content twice
    /// yields the same blob and keeps the first label and import time.
    async fn store(&self, label: &str, bytes: Vec<u8>) -> Result<MediaBlob, StorageError>;

    /// Reads `source` outside the caller's task, then admits it as [`Self::store`] does.
    async fn import_file(&self, source: &Path) -> Result<MediaBlob, StorageError>;

    async fn get(&self, id: &MediaBlobId) -> Result<Option<MediaBlob>, StorageError>;

    async fn list(&self) -> Result<Vec<MediaBlob>, StorageError>;

    async fn total_bytes(&self) -> Result<u64, StorageError>;

    /// [`StorageError::NotFound`] also covers an indexed blob whose file is gone.
    async fn resolve(&self, id: &MediaBlobId) -> Result<PathBuf, StorageError>;

    async fn read(&self, id: &MediaBlobId) -> Result<Vec<u8>, StorageError>;

    /// Refuses with [`StorageError::MediaReferenced`] while any referrer holds it.
    async fn delete(&self, id: &MediaBlobId) -> Result<bool, StorageError>;

    /// Replaces whatever that referrer's slot held.
    async fn retain(&self, referrer: &MediaReferrer, id: &MediaBlobId) -> Result<(), StorageError>;

    /// Returns true if the slot held anything.
    async fn release(&self, referrer: &MediaReferrer) -> Result<bool, StorageError>;

    /// Drops every slot of one domain row; returns how many were held.
    async fn release_all(
        &self,
        kind: MediaReferrerKind,
        referrer_id: &str,
    ) -> Result<u64, StorageError>;

    async fn referrers(&self, id: &MediaBlobId) -> Result<Vec<MediaReferrer>, StorageError>;

    async fn blob_of(&self, referrer: &MediaReferrer) -> Result<Option<MediaBlobId>, StorageError>;
}
