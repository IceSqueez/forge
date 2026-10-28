use std::str::FromStr;

use age::secrecy::{ExposeSecret, SecretString};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use sha2::{Digest, Sha256};

use crate::bundle::digest::to_hex;
use crate::bundle::encryption::classify;
use crate::bundle::limits::{ENCRYPT_SCRYPT_WORK_FACTOR, MAX_DECRYPT_SCRYPT_WORK_FACTOR};
use crate::bundle::passphrase::Passphrase;
use crate::error::TransferError;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScheduleKey {
    pub recipient: String,
    pub wrapped_identity: String,
}

impl ScheduleKey {
    pub fn encode(&self) -> Result<String, TransferError> {
        serde_json::to_string(self).map_err(|reason| TransferError::Encryption {
            reason: reason.to_string(),
        })
    }

    pub fn decode(stored: &str) -> Result<Self, TransferError> {
        serde_json::from_str(stored).map_err(|reason| TransferError::Encryption {
            reason: reason.to_string(),
        })
    }

    pub fn fingerprint(&self) -> String {
        to_hex(&Sha256::digest(self.recipient.as_bytes()))
    }
}

pub fn create_schedule_key(passphrase: &Passphrase) -> Result<ScheduleKey, TransferError> {
    passphrase.ensure_strong_enough()?;
    let identity = age::x25519::Identity::generate();
    Ok(ScheduleKey {
        recipient: identity.to_public().to_string(),
        wrapped_identity: wrap_identity(&identity, passphrase)?,
    })
}

pub fn change_schedule_passphrase(
    current_wrapped_identity: &str,
    current: &Passphrase,
    replacement: &Passphrase,
) -> Result<ScheduleKey, TransferError> {
    unwrap_identity(current_wrapped_identity, current)?;
    create_schedule_key(replacement)
}

pub(crate) fn parse_recipient(recipient: &str) -> Result<age::x25519::Recipient, TransferError> {
    age::x25519::Recipient::from_str(recipient).map_err(|reason| TransferError::Encryption {
        reason: reason.to_owned(),
    })
}

fn wrap_identity(
    identity: &age::x25519::Identity,
    passphrase: &Passphrase,
) -> Result<String, TransferError> {
    let mut recipient = age::scrypt::Recipient::new(passphrase.secret());
    recipient.set_work_factor(ENCRYPT_SCRYPT_WORK_FACTOR);
    let secret = identity.to_string();
    let sealed = age::encrypt(&recipient, secret.expose_secret().as_bytes()).map_err(|e| {
        TransferError::Encryption {
            reason: e.to_string(),
        }
    })?;
    Ok(STANDARD.encode(sealed))
}

pub(crate) fn unwrap_identity(
    wrapped: &str,
    passphrase: &Passphrase,
) -> Result<age::x25519::Identity, TransferError> {
    let sealed = STANDARD
        .decode(wrapped)
        .map_err(|_| TransferError::damaged("the restore key is not readable"))?;
    let mut unlock = age::scrypt::Identity::new(passphrase.secret());
    unlock.set_max_work_factor(MAX_DECRYPT_SCRYPT_WORK_FACTOR);
    let opened = age::decrypt(&unlock, &sealed).map_err(classify)?;
    let secret = String::from_utf8(opened)
        .map(SecretString::from)
        .map_err(|_| TransferError::damaged("the restore key is not readable"))?;
    age::x25519::Identity::from_str(secret.expose_secret())
        .map_err(|_| TransferError::damaged("the restore key is not readable"))
}
