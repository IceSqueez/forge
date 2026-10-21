use std::io::{Read, Write};

use age::DecryptError;
use age::stream::{StreamReader, StreamWriter};

use crate::error::TransferError;
use crate::limits::{ENCRYPT_SCRYPT_WORK_FACTOR, MAX_DECRYPT_SCRYPT_WORK_FACTOR};
use crate::passphrase::Passphrase;

pub(crate) fn encrypt_into<W: Write>(
    passphrase: &Passphrase,
    output: W,
) -> Result<StreamWriter<W>, TransferError> {
    let mut recipient = age::scrypt::Recipient::new(passphrase.secret());
    recipient.set_work_factor(ENCRYPT_SCRYPT_WORK_FACTOR);
    let encryptor =
        age::Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient))
            .map_err(|e| TransferError::Encryption {
                reason: e.to_string(),
            })?;
    Ok(encryptor.wrap_output(output)?)
}

pub(crate) fn encrypt_to_recipient<W: Write>(
    recipient: &age::x25519::Recipient,
    output: W,
) -> Result<StreamWriter<W>, TransferError> {
    let encryptor =
        age::Encryptor::with_recipients(std::iter::once(recipient as &dyn age::Recipient))
            .map_err(|e| TransferError::Encryption {
                reason: e.to_string(),
            })?;
    Ok(encryptor.wrap_output(output)?)
}

pub(crate) fn decrypt_with_identity<R: Read>(
    identity: &age::x25519::Identity,
    input: R,
) -> Result<StreamReader<R>, TransferError> {
    let decryptor = age::Decryptor::new(input).map_err(classify)?;
    if decryptor.is_scrypt() {
        return Err(TransferError::damaged(
            "the payload is not encrypted to a restore key",
        ));
    }
    decryptor
        .decrypt(std::iter::once(identity as &dyn age::Identity))
        .map_err(classify)
}

pub(crate) fn decrypt_from<R: Read>(
    passphrase: &Passphrase,
    input: R,
) -> Result<StreamReader<R>, TransferError> {
    let decryptor = age::Decryptor::new(input).map_err(classify)?;
    if !decryptor.is_scrypt() {
        return Err(TransferError::damaged(
            "the payload is not passphrase-encrypted",
        ));
    }
    let mut identity = age::scrypt::Identity::new(passphrase.secret());
    identity.set_max_work_factor(MAX_DECRYPT_SCRYPT_WORK_FACTOR);
    decryptor
        .decrypt(std::iter::once(&identity as &dyn age::Identity))
        .map_err(classify)
}

pub(crate) fn classify(error: DecryptError) -> TransferError {
    match error {
        DecryptError::DecryptionFailed | DecryptError::KeyDecryptionFailed => {
            TransferError::WrongPassphrase
        }
        DecryptError::ExcessiveWork { required, .. } => TransferError::ExcessiveWorkFactor {
            required,
            cap: MAX_DECRYPT_SCRYPT_WORK_FACTOR,
        },
        other => TransferError::damaged(other),
    }
}
