use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use rand::rand_core::Rng;

use crate::error::SqliteStorageError;

const NONCE_LEN: usize = 12;

pub fn load_or_create_key() -> Result<[u8; 32], SqliteStorageError> {
    if let Ok(path) = std::env::var("LOOM_CREDENTIAL_KEY_FILE") {
        return load_or_create_file_key(&std::path::PathBuf::from(path));
    }

    let entry = keyring_core::Entry::new("streamer-loom", "credentials-key").map_err(|e| {
        SqliteStorageError::Keyring {
            reason: e.to_string(),
        }
    })?;

    match entry.get_password() {
        Ok(hex) => hex_to_key(&hex),
        Err(keyring_core::Error::NoEntry) => {
            let key = generate_key();
            let hex = key_to_hex(&key);
            entry
                .set_password(&hex)
                .map_err(|e| SqliteStorageError::Keyring {
                    reason: e.to_string(),
                })?;
            Ok(key)
        }
        Err(e) => {
            let data_home = xdg_data_home();
            let path = data_home.join("credentials-key");
            load_or_create_file_key(&path).map_err(|_| SqliteStorageError::Keyring {
                reason: e.to_string(),
            })
        }
    }
}

fn load_or_create_file_key(path: &std::path::Path) -> Result<[u8; 32], SqliteStorageError> {
    use std::io::{Read, Write};

    if path.exists() {
        let mut file = std::fs::File::open(path).map_err(|e| SqliteStorageError::Keyring {
            reason: e.to_string(),
        })?;
        let mut hex = String::new();
        file.read_to_string(&mut hex)
            .map_err(|e| SqliteStorageError::Keyring {
                reason: e.to_string(),
            })?;
        return hex_to_key(hex.trim());
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| SqliteStorageError::Keyring {
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
                .map_err(|e| SqliteStorageError::Keyring {
                    reason: e.to_string(),
                })?
        }
        #[cfg(not(unix))]
        {
            std::fs::File::create_new(path).map_err(|e| SqliteStorageError::Keyring {
                reason: e.to_string(),
            })?
        }
    };

    file.write_all(hex.as_bytes())
        .map_err(|e| SqliteStorageError::Keyring {
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
    let nonce = Nonce::from_slice(&nonce_bytes);

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

    let nonce = Nonce::from_slice(nonce);

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
        return Err(SqliteStorageError::Keyring {
            reason: format!("key file corrupt: expected 64 hex chars, got {}", hex.len()),
        });
    }

    let mut key = [0u8; 32];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let byte_str = std::str::from_utf8(chunk).map_err(|e| SqliteStorageError::Keyring {
            reason: e.to_string(),
        })?;
        key[i] = u8::from_str_radix(byte_str, 16).map_err(|e| SqliteStorageError::Keyring {
            reason: e.to_string(),
        })?;
    }

    Ok(key)
}

fn xdg_data_home() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("XDG_DATA_HOME") {
        return std::path::PathBuf::from(dir).join("streamer-loom");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("streamer-loom")
}
