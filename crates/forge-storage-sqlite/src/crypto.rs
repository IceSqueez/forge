use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use rand::rand_core::Rng;

use crate::error::SqliteStorageError;

const NONCE_LEN: usize = 12;

// File-canonical key avoids secret-service session-collection amnesia: the OS keyring on Linux
// often stores entries in a *session* collection that is wiped on logout, so a later boot mints a
// different key and all previously-encrypted credentials become permanently unreadable.
pub fn load_or_create_key() -> Result<[u8; 32], SqliteStorageError> {
    let path = if let Ok(p) = std::env::var("FORGE_CREDENTIAL_KEY_FILE") {
        std::path::PathBuf::from(p)
    } else {
        forge_platform_core::paths::data_dir().join("credentials-key")
    };
    load_or_create_file_key(&path)
}

fn load_or_create_file_key(path: &std::path::Path) -> Result<[u8; 32], SqliteStorageError> {
    use std::io::{Read, Write};

    if path.exists() {
        let mut file = std::fs::File::open(path).map_err(|e| SqliteStorageError::KeyFile {
            reason: e.to_string(),
        })?;
        let mut hex = String::new();
        file.read_to_string(&mut hex)
            .map_err(|e| SqliteStorageError::KeyFile {
                reason: e.to_string(),
            })?;
        return hex_to_key(hex.trim());
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| SqliteStorageError::KeyFile {
            reason: e.to_string(),
        })?;
    }

    let key = generate_key();
    let hex = key_to_hex(&key);

    let mut file = {
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path)
                .map_err(|e| SqliteStorageError::KeyFile {
                    reason: e.to_string(),
                })?
        }
        #[cfg(not(unix))]
        {
            std::fs::File::create_new(path).map_err(|e| SqliteStorageError::KeyFile {
                reason: e.to_string(),
            })?
        }
    };

    file.write_all(hex.as_bytes())
        .map_err(|e| SqliteStorageError::KeyFile {
            reason: e.to_string(),
        })?;

    Ok(key)
}

pub fn encrypt(key: &[u8; 32], plaintext: &str) -> Result<(Vec<u8>, Vec<u8>), SqliteStorageError> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| SqliteStorageError::Crypto {
        reason: e.to_string(),
    })?;

    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::rng().fill_bytes(&mut nonce_bytes);
    let nonce =
        <&Nonce<_>>::try_from(nonce_bytes.as_slice()).map_err(|e| SqliteStorageError::Crypto {
            reason: e.to_string(),
        })?;

    let ciphertext =
        cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| SqliteStorageError::Crypto {
                reason: e.to_string(),
            })?;

    Ok((ciphertext, nonce_bytes.to_vec()))
}

pub fn decrypt(
    key: &[u8; 32],
    ciphertext: &[u8],
    nonce: &[u8],
) -> Result<String, SqliteStorageError> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| SqliteStorageError::Crypto {
        reason: e.to_string(),
    })?;

    let nonce = <&Nonce<_>>::try_from(nonce).map_err(|e| SqliteStorageError::Crypto {
        reason: e.to_string(),
    })?;

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| SqliteStorageError::Crypto {
            reason: e.to_string(),
        })?;

    String::from_utf8(plaintext)
        .map_err(|e| SqliteStorageError::Decode(format!("utf8 decode: {e}")))
}

fn generate_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    rand::rng().fill_bytes(&mut key);
    key
}

fn key_to_hex(key: &[u8; 32]) -> String {
    key.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_to_key(hex: &str) -> Result<[u8; 32], SqliteStorageError> {
    let hex = hex.trim();
    if hex.len() != 64 {
        return Err(SqliteStorageError::KeyFile {
            reason: format!("key file corrupt: expected 64 hex chars, got {}", hex.len()),
        });
    }

    let mut key = [0u8; 32];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let byte_str = std::str::from_utf8(chunk).map_err(|e| SqliteStorageError::KeyFile {
            reason: e.to_string(),
        })?;
        key[i] = u8::from_str_radix(byte_str, 16).map_err(|e| SqliteStorageError::KeyFile {
            reason: e.to_string(),
        })?;
    }

    Ok(key)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn file_key_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let key_path = dir.path().join("test-credentials-key");

        let key1 = load_or_create_file_key(&key_path).unwrap();
        let key2 = load_or_create_file_key(&key_path).unwrap();

        assert_eq!(key1, key2);
        assert_eq!(key1.len(), 32);
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = [0xabu8; 32];
        let plaintext = "super-secret-token";
        let (ciphertext, nonce) = encrypt(&key, plaintext).unwrap();
        let recovered = decrypt(&key, &ciphertext, &nonce).unwrap();
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn decrypt_rejects_wrong_key() {
        let key = [0x01u8; 32];
        let wrong_key = [0x02u8; 32];
        let (ciphertext, nonce) = encrypt(&key, "payload").unwrap();
        assert!(decrypt(&wrong_key, &ciphertext, &nonce).is_err());
    }
}
