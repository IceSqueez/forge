use std::fmt;

use age::secrecy::SecretString;

use crate::error::TransferError;

pub const MIN_PASSPHRASE_CHARS: usize = 12;

pub struct Passphrase(SecretString);

impl Passphrase {
    pub fn new(passphrase: String) -> Self {
        Self(SecretString::from(passphrase))
    }

    pub(crate) fn secret(&self) -> SecretString {
        self.0.clone()
    }

    pub(crate) fn ensure_strong_enough(&self) -> Result<(), TransferError> {
        use age::secrecy::ExposeSecret;
        if self.0.expose_secret().chars().count() < MIN_PASSPHRASE_CHARS {
            return Err(TransferError::PassphraseTooShort {
                minimum: MIN_PASSPHRASE_CHARS,
            });
        }
        Ok(())
    }
}

impl fmt::Debug for Passphrase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}
