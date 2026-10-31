#![allow(dead_code, clippy::unwrap_used)]
#![expect(
    clippy::panic,
    reason = "a failed child run must fail the calling test"
)]

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use forge_paths::MEDIA_DIR_NAME;
use forge_storage::DATABASE_FILE_NAME;
use forge_storage_sqlite::SqliteBackend;
use forge_transfer::{
    ContentEntry, OverlayDocument, OverlayFileDocument, PACK_DOCUMENTS_VERSION, PackDocuments,
};
use sha2::{Digest, Sha256};

pub const SQLITE_KEY: [u8; 32] = [0x42; 32];
const BUNDLE_MODE: u32 = 0o600;

pub type Tree = BTreeMap<String, Vec<u8>>;

const KEY_FILE_ENV: &str = "FORGE_CREDENTIAL_KEY_FILE";
const CHILD_RUN_ENV: &str = "FORGE_TRANSFER_KEY_OVERRIDE_CHILD_RUN";
const ONE_TEST_PASSED: &str = "1 passed";

pub fn key_file_of_this_child_run() -> Option<PathBuf> {
    std::env::var_os(CHILD_RUN_ENV)?;
    std::env::var_os(KEY_FILE_ENV).map(PathBuf::from)
}

pub fn rerun_in_child_with_key_file(test_name: &str, key_file: &Path) {
    let test_binary = std::env::current_exe().unwrap_or_else(|e| panic!("test binary: {e}"));
    let child = Command::new(test_binary)
        .args(["--exact", test_name])
        .env(KEY_FILE_ENV, key_file)
        .env(CHILD_RUN_ENV, "1")
        .output()
        .unwrap_or_else(|e| panic!("child test run: {e}"));
    let stdout = String::from_utf8_lossy(&child.stdout);
    assert!(
        child.status.success() && stdout.contains(ONE_TEST_PASSED),
        "child run of {test_name} must pass exactly one test\n{stdout}\n{}",
        String::from_utf8_lossy(&child.stderr)
    );
}

pub fn database_url(data_dir: &Path) -> String {
    format!("sqlite://{}", data_dir.join(DATABASE_FILE_NAME).display())
}

pub async fn open_backend(data_dir: &Path) -> SqliteBackend {
    SqliteBackend::open_for_test(
        &database_url(data_dir),
        SQLITE_KEY,
        data_dir.join(MEDIA_DIR_NAME),
        None,
    )
    .await
    .unwrap()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn entry(path: &str, bytes: &[u8]) -> ContentEntry {
    ContentEntry {
        path: path.to_owned(),
        size: bytes.len() as u64,
        sha256_hex: sha256_hex(bytes),
    }
}

pub fn header(name: &str, size: usize) -> tar::Header {
    let mut header = tar::Header::new_gnu();
    header.as_gnu_mut().unwrap().name[..name.len()].copy_from_slice(name.as_bytes());
    header.set_entry_type(tar::EntryType::Regular);
    header.set_size(size as u64);
    header.set_mode(BUNDLE_MODE);
    header.set_cksum();
    header
}

pub fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(bytes).unwrap();
    gz.finish().unwrap()
}

pub fn write_bundle_file(dir: &Path, entries: &[(&str, &[u8])]) -> PathBuf {
    let mut builder = tar::Builder::new(Vec::new());
    for (name, bytes) in entries {
        builder.append(&header(name, bytes.len()), *bytes).unwrap();
    }
    let path = dir.join(format!("{}.forgebundle", ulid::Ulid::generate()));
    std::fs::write(&path, gzip(&builder.into_inner().unwrap())).unwrap();
    path
}

pub fn write_file(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

pub fn files_under(root: &Path) -> Tree {
    fn walk(root: &Path, dir: &Path, out: &mut Tree) {
        for child in std::fs::read_dir(dir).unwrap() {
            let path = child.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(relative, std::fs::read(&path).unwrap());
            }
        }
    }
    let mut out = Tree::new();
    if root.exists() {
        walk(root, root, &mut out);
    }
    out
}

pub fn file_named_anywhere(root: &Path, name: &str) -> bool {
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten() {
            let path = entry.unwrap().path();
            if path.file_name().is_some_and(|n| n == name) {
                return true;
            }
            if path.is_dir() {
                pending.push(path);
            }
        }
    }
    false
}

pub fn empty_documents() -> PackDocuments {
    PackDocuments {
        version: PACK_DOCUMENTS_VERSION,
        actions: Vec::new(),
        triggers: Vec::new(),
        queues: Vec::new(),
        scripts: Vec::new(),
        globals: Vec::new(),
        clips: Vec::new(),
        overlays: Vec::new(),
        media: Vec::new(),
    }
}

pub fn overlay_carrying(id: &str, file: &str, name: &str) -> OverlayDocument {
    OverlayDocument {
        id: id.to_owned(),
        display_name: "Escaper".to_owned(),
        kind_id: "overlay.alert".to_owned(),
        enabled: true,
        position: 0,
        config: Default::default(),
        config_schema_version: 1,
        source_overrides: vec![name.to_owned()],
        files: vec![OverlayFileDocument {
            name: name.to_owned(),
            file: file.to_owned(),
        }],
    }
}
