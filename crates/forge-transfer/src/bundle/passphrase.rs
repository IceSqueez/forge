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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passphrase_strength_counts_characters_not_bytes_at_the_minimum() {
        for (case, passphrase, accepted) in [
            ("one short", "a".repeat(MIN_PASSPHRASE_CHARS - 1), false),
            ("at the minimum", "a".repeat(MIN_PASSPHRASE_CHARS), true),
            (
                "multibyte one short",
                "ї".repeat(MIN_PASSPHRASE_CHARS - 1),
                false,
            ),
            (
                "multibyte at the minimum",
                "ї".repeat(MIN_PASSPHRASE_CHARS),
                true,
            ),
            ("empty", String::new(), false),
        ] {
            let checked = Passphrase::new(passphrase).ensure_strong_enough();
            assert_eq!(checked.is_ok(), accepted, "{case}");
        }
    }
}
