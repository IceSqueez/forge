use std::io::Write;

use age::stream::StreamWriter;

use crate::bundle::encryption::{encrypt_into, encrypt_to_recipient};
use crate::bundle::manifest::{BundleOrigin, EncryptionScheme};
use crate::bundle::passphrase::Passphrase;
use crate::bundle::schedule_key::{ScheduleKey, parse_recipient};
use crate::error::TransferError;

pub(crate) enum Protection {
    Passphrase(Passphrase),
    Scheduled {
        key: ScheduleKey,
        recipient: age::x25519::Recipient,
    },
}

impl Protection {
    pub(crate) fn scheduled(key: ScheduleKey) -> Result<Self, TransferError> {
        let recipient = parse_recipient(&key.recipient)?;
        Ok(Self::Scheduled { key, recipient })
    }

    pub(crate) fn scheme(&self) -> EncryptionScheme {
        match self {
            Self::Passphrase(_) => EncryptionScheme::age_scrypt(),
            Self::Scheduled { .. } => EncryptionScheme::age_x25519_wrapped(),
        }
    }

    pub(crate) fn origin(&self) -> BundleOrigin {
        match self {
            Self::Passphrase(_) => BundleOrigin::Manual,
            Self::Scheduled { .. } => BundleOrigin::Scheduled,
        }
    }

    pub(crate) fn key_fingerprint(&self) -> Option<String> {
        match self {
            Self::Passphrase(_) => None,
            Self::Scheduled { key, .. } => Some(key.fingerprint()),
        }
    }

    pub(crate) fn wrapped_identity(&self) -> Option<String> {
        match self {
            Self::Passphrase(_) => None,
            Self::Scheduled { key, .. } => Some(key.wrapped_identity.clone()),
        }
    }

    pub(crate) fn seal<W: Write>(&self, output: W) -> Result<StreamWriter<W>, TransferError> {
        match self {
            Self::Passphrase(passphrase) => encrypt_into(passphrase, output),
            Self::Scheduled { recipient, .. } => encrypt_to_recipient(recipient, output),
        }
    }
}
