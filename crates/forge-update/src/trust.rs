use minisign_verify::{PublicKey, Signature};

use crate::UpdateError;

const PRODUCTION_KEYS: &[&str] = &["RWSnIBLixZkSGKfrUmzQlvGSdqIOV+BMHIGyIgJae9w4oG4knfnc5xlq"];

const ALLOW_LEGACY_SIGNATURES: bool = false;

#[derive(Debug, Clone)]
pub struct TrustedKeys {
    keys: Vec<PublicKey>,
}

impl TrustedKeys {
    pub fn production() -> Result<Self, UpdateError> {
        Self::from_base64(PRODUCTION_KEYS)
    }

    pub fn from_base64<S: AsRef<str>>(encoded: &[S]) -> Result<Self, UpdateError> {
        if encoded.is_empty() {
            return Err(UpdateError::InvalidTrustedKey);
        }
        let keys = encoded
            .iter()
            .map(|key| {
                PublicKey::from_base64(key.as_ref().trim())
                    .map_err(|_| UpdateError::InvalidTrustedKey)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { keys })
    }

    pub(crate) fn verified_trusted_comment(
        &self,
        body: &[u8],
        signature: &str,
    ) -> Result<String, UpdateError> {
        let signature =
            Signature::decode(signature).map_err(|_| UpdateError::MalformedSignature)?;
        if self.keys.iter().any(|key| {
            key.verify(body, &signature, ALLOW_LEGACY_SIGNATURES)
                .is_ok()
        }) {
            Ok(signature.trusted_comment().to_owned())
        } else {
            Err(UpdateError::UntrustedSignature)
        }
    }
}
