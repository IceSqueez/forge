use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use rand::rand_core::Rng;

use crate::error::SqliteStorageError;

const NONCE_LEN: usize = 12;

pub fn load_or_create_key() -> Result<CredentialsKey, SqliteStorageError> {
    let path = forge_paths::credentials_key_path();
    let (key, minted) = load_or_create_file_key(&path)?;
    Ok(CredentialsKey { key, minted })
}

pub struct CredentialsKey {
    pub key: [u8; 32],
    pub minted: bool,
}

fn key_file_error(error: impl std::fmt::Display) -> SqliteStorageError {
    SqliteStorageError::KeyFile {
        reason: error.to_string(),
    }
}

fn read_key_file(path: &std::path::Path) -> Result<[u8; 32], SqliteStorageError> {
    let hex = std::fs::read_to_string(path).map_err(key_file_error)?;
    hex_to_key(hex.trim())
}

fn load_or_create_file_key(path: &std::path::Path) -> Result<([u8; 32], bool), SqliteStorageError> {
    if path.exists() {
        return read_key_file(path).map(|key| (key, false));
    }

    let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
    if let Some(parent) = parent {
        std::fs::create_dir_all(parent).map_err(key_file_error)?;
    }

    let key = generate_key();
    let staged = unique_staging_path(path);
    write_staged_key(&staged, key_to_hex(&key).as_bytes())?;
    let published = std::fs::hard_link(&staged, path);
    discard_staged(&staged);
    match published {
        Ok(()) => {
            sync_directory(parent.unwrap_or_else(|| std::path::Path::new(".")))?;
            Ok((key, true))
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            read_key_file(path).map(|existing| (existing, false))
        }
        Err(error) => Err(key_file_error(error)),
    }
}

fn unique_staging_path(path: &std::path::Path) -> std::path::PathBuf {
    static NEXT_STAGING_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let staging_id = NEXT_STAGING_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    path.with_extension(format!("{}-{staging_id}.tmp", std::process::id()))
}

fn discard_staged(path: &std::path::Path) {
    if let Err(error) = std::fs::remove_file(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::debug!(%error, "staged credentials key could not be removed");
    }
}

fn write_staged_key(path: &std::path::Path, contents: &[u8]) -> Result<(), SqliteStorageError> {
    use std::io::Write;

    discard_staged(path);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(key_file_error)?;
    file.write_all(contents).map_err(key_file_error)?;
    file.sync_all().map_err(key_file_error)
}

#[cfg(unix)]
fn sync_directory(dir: &std::path::Path) -> Result<(), SqliteStorageError> {
    std::fs::File::open(dir)
        .and_then(|handle| handle.sync_all())
        .map_err(key_file_error)
}

#[cfg(not(unix))]
fn sync_directory(_dir: &std::path::Path) -> Result<(), SqliteStorageError> {
    Ok(())
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
mod tests {
    use super::*;

    #[test]
    fn file_key_is_minted_only_on_first_call() {
        let dir = tempfile::tempdir().unwrap();
        let key_path = dir.path().join("test-credentials-key");

        let (key1, minted_first) = load_or_create_file_key(&key_path).unwrap();
        let (key2, minted_second) = load_or_create_file_key(&key_path).unwrap();

        assert!(minted_first);
        assert!(!minted_second);
        assert_eq!(key1, key2);
    }

    #[cfg(unix)]
    #[test]
    fn minted_key_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let key_path = dir.path().join("test-credentials-key");

        load_or_create_file_key(&key_path).unwrap();

        let mode = std::fs::metadata(&key_path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn concurrent_mints_converge_on_one_key_with_one_winner() {
        let dir = tempfile::tempdir().unwrap();
        let key_path = std::sync::Arc::new(dir.path().join("test-credentials-key"));
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));

        let handles: Vec<_> = (0..8)
            .map(|_| {
                let path = key_path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    load_or_create_file_key(&path)
                })
            })
            .collect();
        let results: Vec<_> = handles
            .into_iter()
            .map(|h| h.join().unwrap().unwrap())
            .collect();

        assert_eq!(results.iter().filter(|(_, minted)| *minted).count(), 1);
        assert!(results.iter().all(|(key, _)| *key == results[0].0));
    }

    #[test]
    fn existing_corrupt_key_file_is_rejected_not_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let key_path = dir.path().join("test-credentials-key");
        std::fs::write(&key_path, "not-hex").unwrap();

        let err = load_or_create_file_key(&key_path).unwrap_err();

        assert!(matches!(err, SqliteStorageError::KeyFile { .. }));
        assert_eq!(std::fs::read_to_string(&key_path).unwrap(), "not-hex");
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

    #[test]
    fn decrypt_wrong_length_nonce_returns_crypto_error() {
        let key = [0x0cu8; 32];
        let (ciphertext, _) = encrypt(&key, "payload").unwrap();
        for bad_len in [0, NONCE_LEN - 1, NONCE_LEN + 1, 2 * NONCE_LEN] {
            let bad_nonce = vec![0u8; bad_len];
            let err = decrypt(&key, &ciphertext, &bad_nonce).unwrap_err();
            assert!(
                matches!(err, SqliteStorageError::Crypto { .. }),
                "nonce len {bad_len}: expected Crypto error, got {err:?}"
            );
        }
    }
}
