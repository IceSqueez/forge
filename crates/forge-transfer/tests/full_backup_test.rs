#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use age::secrecy::SecretString;
use forge_storage::paths::{CREDENTIALS_KEY_FILE_NAME, MEDIA_DIR_NAME, OVERLAYS_DIR_NAME};
use forge_storage::{
    CredentialId, CredentialsRepo, DATABASE_FILE_NAME, DataProvider, DatabaseFileSchema,
    EXPECTED_SCHEMA_VERSION, StorageError, StoredDatabase, StoredSchema,
};
use forge_storage_sqlite::{SqliteBackend, SqliteCredentialsRepo, SqliteDatabaseFileSchema};
use forge_transfer::{
    BUNDLE_FORMAT_VERSION, BackupCategory, BackupDestination, BundleKind, BundleOrigin,
    BundleSummary, ContentEntry, ENCRYPTED_PAYLOAD_ENTRY_NAME, EncryptionScheme, FullBackupRequest,
    FullManifest, MANIFEST_ENTRY_NAME, MAX_DECRYPT_SCRYPT_WORK_FACTOR, Passphrase, PublicManifest,
    StageRequest, StagedRestore, TransferError, discard_staged_restore, read_bundle_summary,
    restore_staging_root, stage_full_restore, write_full_backup,
};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use time::OffsetDateTime;

const PASSPHRASE: &str = "correct horse battery staple";
const OTHER_PASSPHRASE: &str = "incorrect donkey battery clip";
const CHEAP_WORK_FACTOR: u8 = 2;
const DB_ENTRY: &str = "forge/forge.db";
const KEY_ENTRY: &str = "forge/credentials-key";
const MEDIA_ENTRY: &str = "forge/media/clip.ogg";
const DB_BYTES: &[u8] = b"database bytes";
const KEY_BYTES: &[u8] = b"thirty-two bytes of credential k";
const MEDIA_BYTES: &[u8] = b"media bytes";
const SQLITE_KEY: [u8; 32] = [0x42; 32];

type Expected = fn(&TransferError) -> bool;
type Archived<'a> = &'a [(&'a str, &'a [u8])];

struct FakeSchema(Option<StoredDatabase>);

impl DatabaseFileSchema for FakeSchema {
    fn applied_schema_version(&self, _: &Path) -> Result<Option<u32>, StorageError> {
        Ok(None)
    }

    fn inspect(&self, _: &Path) -> Result<Option<StoredDatabase>, StorageError> {
        Ok(self.0)
    }
}

fn schema_state(schema: StoredSchema, holds_credentials: bool) -> Arc<dyn DatabaseFileSchema> {
    Arc::new(FakeSchema(Some(StoredDatabase {
        schema,
        holds_credentials,
    })))
}

fn current_schema() -> Arc<dyn DatabaseFileSchema> {
    schema_state(
        StoredSchema::Current {
            version: EXPECTED_SCHEMA_VERSION,
        },
        false,
    )
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn add_file(builder: &mut tar::Builder<Vec<u8>>, path: &str, bytes: &[u8]) {
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Regular);
    header.set_size(bytes.len() as u64);
    header.set_mode(0o600);
    builder.append_data(&mut header, path, bytes).unwrap();
}

fn add_raw(
    builder: &mut tar::Builder<Vec<u8>>,
    name: &str,
    kind: tar::EntryType,
    link: &str,
    bytes: &[u8],
) {
    let mut header = tar::Header::new_gnu();
    header.as_gnu_mut().unwrap().name[..name.len()].copy_from_slice(name.as_bytes());
    header.as_gnu_mut().unwrap().linkname[..link.len()].copy_from_slice(link.as_bytes());
    header.set_entry_type(kind);
    header.set_size(bytes.len() as u64);
    header.set_mode(0o600);
    header.set_cksum();
    builder.append(&header, bytes).unwrap();
}

fn tar_gz(build: impl FnOnce(&mut tar::Builder<Vec<u8>>)) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    build(&mut builder);
    let tar = builder.into_inner().unwrap();
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&tar).unwrap();
    gz.finish().unwrap()
}

fn legacy(build: impl FnOnce(&mut tar::Builder<Vec<u8>>)) -> Vec<u8> {
    tar_gz(|b| {
        add_file(b, DB_ENTRY, DB_BYTES);
        build(b);
    })
}

fn encrypt(plaintext: &[u8], passphrase: &str) -> Vec<u8> {
    let mut recipient = age::scrypt::Recipient::new(SecretString::from(passphrase.to_owned()));
    recipient.set_work_factor(CHEAP_WORK_FACTOR);
    let encryptor =
        age::Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient))
            .unwrap();
    let mut writer = encryptor.wrap_output(Vec::new()).unwrap();
    writer.write_all(plaintext).unwrap();
    writer.finish().unwrap()
}

fn entry(path: &str, bytes: &[u8]) -> ContentEntry {
    ContentEntry {
        path: path.to_owned(),
        size: bytes.len() as u64,
        sha256_hex: sha256_hex(bytes),
    }
}

fn full_manifest(schema_version: u32, contents: Vec<ContentEntry>) -> Vec<u8> {
    full_manifest_in(schema_version, BackupCategory::all(), contents)
}

fn full_manifest_in(
    schema_version: u32,
    categories: Vec<BackupCategory>,
    contents: Vec<ContentEntry>,
) -> Vec<u8> {
    serde_json::to_vec(&FullManifest {
        format_version: BUNDLE_FORMAT_VERSION,
        kind: BundleKind::Full,
        forge_version: "0.0.0".to_owned(),
        schema_version,
        created_at: OffsetDateTime::UNIX_EPOCH,
        categories,
        contents,
    })
    .unwrap()
}

fn public_manifest() -> Vec<u8> {
    public_manifest_in(BackupCategory::all())
}

fn public_manifest_in(categories: Vec<BackupCategory>) -> Vec<u8> {
    serde_json::to_vec(&PublicManifest {
        format_version: BUNDLE_FORMAT_VERSION,
        kind: BundleKind::Full,
        encryption: EncryptionScheme::age_scrypt(),
        forge_version: "0.0.0".to_owned(),
        schema_version: EXPECTED_SCHEMA_VERSION,
        created_at: OffsetDateTime::UNIX_EPOCH,
        origin: BundleOrigin::Manual,
        categories,
        key_fingerprint: None,
        wrapped_identity: None,
    })
    .unwrap()
}

fn inner(manifest: &[u8], files: &[(&str, &[u8])]) -> Vec<u8> {
    tar_gz(|b| {
        add_file(b, MANIFEST_ENTRY_NAME, manifest);
        for (path, bytes) in files {
            add_file(b, path, bytes);
        }
    })
}

fn honest_inner(files: &[(&str, &[u8])]) -> Vec<u8> {
    let contents = files.iter().map(|(p, b)| entry(p, b)).collect();
    inner(&full_manifest(EXPECTED_SCHEMA_VERSION, contents), files)
}

fn outer(public: &[u8], payload: &[u8]) -> Vec<u8> {
    tar_gz(|b| {
        add_file(b, MANIFEST_ENTRY_NAME, public);
        add_file(b, ENCRYPTED_PAYLOAD_ENTRY_NAME, payload);
    })
}

fn sealed(files: &[(&str, &[u8])]) -> Vec<u8> {
    outer(
        &public_manifest(),
        &encrypt(&honest_inner(files), PASSPHRASE),
    )
}

struct Fixture {
    dir: TempDir,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("data")).unwrap();
        Self { dir }
    }

    fn data_dir(&self) -> PathBuf {
        self.dir.path().join("data")
    }

    fn bundle(&self) -> PathBuf {
        self.dir.path().join("input.forgebundle")
    }

    fn staging_leftovers(&self) -> usize {
        std::fs::read_dir(restore_staging_root(&self.data_dir()))
            .map(|entries| entries.count())
            .unwrap_or(0)
    }

    async fn stage(
        &self,
        bytes: &[u8],
        passphrase: Option<&str>,
        schema: Arc<dyn DatabaseFileSchema>,
    ) -> Result<StagedRestore, TransferError> {
        std::fs::write(self.bundle(), bytes).unwrap();
        stage_full_restore(StageRequest {
            bundle: self.bundle(),
            data_dir: self.data_dir(),
            passphrase: passphrase.map(|p| Passphrase::new(p.to_owned())),
            database_schema: schema,
        })
        .await
    }

    async fn refusal(&self, bytes: &[u8], passphrase: Option<&str>) -> TransferError {
        self.refusal_with(bytes, passphrase, current_schema()).await
    }

    async fn refusal_with(
        &self,
        bytes: &[u8],
        passphrase: Option<&str>,
        schema: Arc<dyn DatabaseFileSchema>,
    ) -> TransferError {
        let error = self.stage(bytes, passphrase, schema).await.unwrap_err();
        assert_eq!(
            self.staging_leftovers(),
            0,
            "staging must be removed after {error:?}"
        );
        error
    }

    async fn summary(&self, bytes: &[u8]) -> Result<BundleSummary, TransferError> {
        std::fs::write(self.bundle(), bytes).unwrap();
        read_bundle_summary(self.bundle()).await
    }
}

fn files_under(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for child in std::fs::read_dir(dir).unwrap() {
            let path = child.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                out.insert(relative, std::fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

struct LiveData {
    dir: TempDir,
    provider: Arc<dyn DataProvider>,
}

impl LiveData {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let url = format!("sqlite://{}", dir.path().join(DATABASE_FILE_NAME).display());
        let backend =
            SqliteBackend::open_for_test(&url, SQLITE_KEY, dir.path().join(MEDIA_DIR_NAME), None)
                .await
                .unwrap();
        let pool = forge_storage_sqlite::connect(&url).await.unwrap();
        SqliteCredentialsRepo::new_with_key(pool.clone(), SQLITE_KEY)
            .store(&CredentialId::new("twitch:broadcaster"), "token")
            .await
            .unwrap();
        pool.close().await;
        let data = dir.path();
        std::fs::write(data.join(CREDENTIALS_KEY_FILE_NAME), KEY_BYTES).unwrap();
        std::fs::create_dir_all(data.join(MEDIA_DIR_NAME).join("nested")).unwrap();
        std::fs::write(data.join(MEDIA_DIR_NAME).join("clip.ogg"), MEDIA_BYTES).unwrap();
        std::fs::write(
            data.join(MEDIA_DIR_NAME).join("nested").join("кліп.png"),
            b"unicode-named media",
        )
        .unwrap();
        std::fs::create_dir_all(data.join(OVERLAYS_DIR_NAME).join("alerts")).unwrap();
        std::fs::write(
            data.join(OVERLAYS_DIR_NAME)
                .join("alerts")
                .join("index.html"),
            b"<html></html>",
        )
        .unwrap();
        std::fs::create_dir_all(data.join("logs")).unwrap();
        std::fs::write(data.join("logs").join("forge.log"), b"not part of a backup").unwrap();
        Self {
            dir,
            provider: Arc::new(backend),
        }
    }

    fn data_dir(&self) -> PathBuf {
        self.dir.path().to_path_buf()
    }

    fn expected_payload(&self) -> BTreeMap<String, Vec<u8>> {
        let mut expected = files_under(self.dir.path());
        expected.retain(|path, _| {
            path == CREDENTIALS_KEY_FILE_NAME
                || path.starts_with(MEDIA_DIR_NAME)
                || path.starts_with(OVERLAYS_DIR_NAME)
        });
        expected
    }
}

async fn stage_real(bundle: PathBuf, data_dir: PathBuf, passphrase: Option<&str>) -> StagedRestore {
    stage_full_restore(StageRequest {
        bundle,
        data_dir,
        passphrase: passphrase.map(|p| Passphrase::new(p.to_owned())),
        database_schema: Arc::new(SqliteDatabaseFileSchema),
    })
    .await
    .unwrap()
}

fn staged_payload_without_database(staged: &StagedRestore) -> BTreeMap<String, Vec<u8>> {
    let mut staged_files = files_under(&staged.payload_dir());
    staged_files.remove(DATABASE_FILE_NAME);
    staged_files
}

#[tokio::test]
async fn full_backup_round_trip_stages_the_backup_scope_byte_for_byte() {
    let live = LiveData::new().await;
    let bundle = write_full_backup(FullBackupRequest {
        provider: live.provider.clone(),
        data_dir: live.data_dir(),
        destination: BackupDestination::BackupsDir,
        passphrase: Passphrase::new(PASSPHRASE.to_owned()),
    })
    .await
    .unwrap();
    live.provider.shutdown().await;

    let backups_after_write = files_under(bundle.parent().unwrap());
    let summary = read_bundle_summary(bundle.clone()).await.unwrap();
    let staged = stage_real(bundle.clone(), live.data_dir(), Some(PASSPHRASE)).await;

    assert_eq!(
        backups_after_write.keys().collect::<Vec<_>>(),
        [&bundle.file_name().unwrap().to_string_lossy().into_owned()],
        "no plaintext snapshot or partial file may survive the write"
    );

    assert!(matches!(
        &summary,
        BundleSummary::Encrypted(m) if m.kind == BundleKind::Full && m.schema_version == EXPECTED_SCHEMA_VERSION
    ));
    assert_eq!(
        staged_payload_without_database(&staged),
        live.expected_payload()
    );
    let database = staged.database_file();
    let inspected =
        tokio::task::spawn_blocking(move || SqliteDatabaseFileSchema.inspect(&database))
            .await
            .unwrap()
            .unwrap();
    assert_eq!(
        inspected,
        Some(StoredDatabase {
            schema: StoredSchema::Current {
                version: EXPECTED_SCHEMA_VERSION
            },
            holds_credentials: true,
        })
    );
}

#[tokio::test]
async fn full_backup_refuses_a_short_passphrase_before_writing() {
    let live = LiveData::new().await;
    let destination = live.dir.path().join("out.forgebundle");

    let result = write_full_backup(FullBackupRequest {
        provider: live.provider.clone(),
        data_dir: live.data_dir(),
        destination: BackupDestination::File(destination.clone()),
        passphrase: Passphrase::new("eleven char".to_owned()),
    })
    .await;
    live.provider.shutdown().await;

    assert!(matches!(
        result,
        Err(TransferError::PassphraseTooShort { .. })
    ));
    assert!(!destination.exists());
}

#[test]
fn passphrase_debug_output_hides_the_secret() {
    let shown = format!("{:?}", Passphrase::new(PASSPHRASE.to_owned()));

    assert!(!shown.contains("horse"));
}

#[tokio::test]
async fn wrong_passphrase_is_reported_as_wrong_passphrase() {
    let fx = Fixture::new();
    let bundle = sealed(&[(DB_ENTRY, DB_BYTES)]);

    let error = fx.refusal(&bundle, Some(OTHER_PASSPHRASE)).await;

    assert!(matches!(error, TransferError::WrongPassphrase));
}

fn flip_byte(bytes: &mut [u8], index: usize) {
    bytes[index] ^= 0x01;
}

fn tamper_header_mac(payload: &mut [u8]) {
    let marker = b"\n--- ";
    let at = payload
        .windows(marker.len())
        .position(|w| w == marker)
        .unwrap()
        + marker.len();
    payload[at] = if payload[at] == b'A' { b'B' } else { b'A' };
}

fn with_work_factor(payload: &[u8], log_n: u8) -> Vec<u8> {
    let stanza = b"-> scrypt ";
    let start = payload
        .windows(stanza.len())
        .position(|w| w == stanza)
        .unwrap();
    let line_end = start + payload[start..].iter().position(|&b| b == b'\n').unwrap();
    let factor_start = start
        + payload[start..line_end]
            .iter()
            .rposition(|&b| b == b' ')
            .unwrap()
        + 1;
    let mut rewritten = payload[..factor_start].to_vec();
    rewritten.extend_from_slice(log_n.to_string().as_bytes());
    rewritten.extend_from_slice(&payload[line_end..]);
    rewritten
}

#[tokio::test]
async fn damage_is_reported_as_damaged_never_as_wrong_passphrase() {
    let fx = Fixture::new();
    let payload = encrypt(
        &honest_inner(&[(DB_ENTRY, DB_BYTES), (MEDIA_ENTRY, &[7_u8; 4096])]),
        PASSPHRASE,
    );
    let mut flipped = payload.clone();
    flip_byte(&mut flipped, payload.len() - 20);
    let truncated_payload = payload[..payload.len() - 20].to_vec();
    let mut mac_tampered = payload.clone();
    tamper_header_mac(&mut mac_tampered);
    let whole = outer(&public_manifest(), &payload);
    let truncated_file = whole[..whole.len() * 3 / 4].to_vec();

    for (case, bytes) in [
        ("flipped payload byte", outer(&public_manifest(), &flipped)),
        (
            "truncated payload",
            outer(&public_manifest(), &truncated_payload),
        ),
        ("truncated file", truncated_file),
        (
            "tampered header mac",
            outer(&public_manifest(), &mac_tampered),
        ),
    ] {
        let error = fx.refusal(&bytes, Some(PASSPHRASE)).await;
        assert!(
            matches!(error, TransferError::Damaged { .. }),
            "{case}: {error:?}"
        );
    }
}

#[tokio::test]
async fn work_factor_above_the_cap_is_refused() {
    let fx = Fixture::new();
    let payload = encrypt(&honest_inner(&[(DB_ENTRY, DB_BYTES)]), PASSPHRASE);
    let hostile = with_work_factor(&payload, MAX_DECRYPT_SCRYPT_WORK_FACTOR + 1);

    let error = fx
        .refusal(&outer(&public_manifest(), &hostile), Some(PASSPHRASE))
        .await;

    assert!(matches!(
        error,
        TransferError::ExcessiveWorkFactor { required, cap }
            if required == MAX_DECRYPT_SCRYPT_WORK_FACTOR + 1 && cap == MAX_DECRYPT_SCRYPT_WORK_FACTOR
    ));
}

#[tokio::test]
async fn payload_sealed_to_a_key_instead_of_a_passphrase_is_damaged() {
    let fx = Fixture::new();
    let identity = age::x25519::Identity::generate();
    let recipient = identity.to_public();
    let encryptor =
        age::Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient))
            .unwrap();
    let mut writer = encryptor.wrap_output(Vec::new()).unwrap();
    writer
        .write_all(&honest_inner(&[(DB_ENTRY, DB_BYTES)]))
        .unwrap();
    let payload = writer.finish().unwrap();

    let error = fx
        .refusal(&outer(&public_manifest(), &payload), Some(PASSPHRASE))
        .await;

    assert!(matches!(error, TransferError::Damaged { .. }));
}

fn manifest_json(
    kind: &str,
    format: u32,
    schema: u32,
    scheme: &str,
    scheme_version: u32,
) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "format_version": format,
        "kind": kind,
        "encryption": { "id": scheme, "version": scheme_version },
        "forge_version": "9.9.9",
        "schema_version": schema,
        "created_at": "2030-01-01T00:00:00Z",
    }))
    .unwrap()
}

fn manifest_only(manifest: &[u8]) -> Vec<u8> {
    tar_gz(|b| add_file(b, MANIFEST_ENTRY_NAME, manifest))
}

#[tokio::test]
async fn public_manifest_refusals_check_kind_then_format_then_schema_then_scheme() {
    let fx = Fixture::new();
    let newer_format = BUNDLE_FORMAT_VERSION + 1;
    let newer_schema = EXPECTED_SCHEMA_VERSION + 1;
    let age = EncryptionScheme::AGE_SCRYPT_ID;
    let age_version = EncryptionScheme::AGE_SCRYPT_VERSION;

    let cases: [(&str, Vec<u8>, Expected); 5] = [
        (
            "unknown kind wins",
            manifest_json("not-a-kind", newer_format, newer_schema, "argon2", 1),
            |e| matches!(e, TransferError::UnknownKind { kind } if kind == "not-a-kind"),
        ),
        (
            "newer format beats schema and scheme",
            manifest_json("full", newer_format, newer_schema, "argon2", 1),
            |e| matches!(e, TransferError::NewerFormat { found, .. } if *found == BUNDLE_FORMAT_VERSION + 1),
        ),
        (
            "newer schema beats scheme",
            manifest_json("full", BUNDLE_FORMAT_VERSION, newer_schema, "argon2", 1),
            |e| matches!(e, TransferError::NewerSchema { found, .. } if *found == EXPECTED_SCHEMA_VERSION + 1),
        ),
        (
            "unknown scheme id",
            manifest_json(
                "full",
                BUNDLE_FORMAT_VERSION,
                EXPECTED_SCHEMA_VERSION,
                "argon2",
                1,
            ),
            |e| matches!(e, TransferError::UnknownEncryption { scheme, .. } if scheme == "argon2"),
        ),
        (
            "unknown scheme version",
            manifest_json(
                "full",
                BUNDLE_FORMAT_VERSION,
                EXPECTED_SCHEMA_VERSION,
                age,
                age_version + 1,
            ),
            |e| matches!(e, TransferError::UnknownEncryption { version, .. } if *version == EncryptionScheme::AGE_SCRYPT_VERSION + 1),
        ),
    ];
    for (case, manifest, expected) in cases {
        let error = fx.summary(&manifest_only(&manifest)).await.unwrap_err();
        assert!(expected(&error), "{case}: {error:?}");
    }
}

#[tokio::test]
async fn newer_format_is_refused_at_stage_before_a_passphrase_is_asked() {
    let fx = Fixture::new();
    let manifest = manifest_json(
        "full",
        BUNDLE_FORMAT_VERSION + 1,
        EXPECTED_SCHEMA_VERSION,
        EncryptionScheme::AGE_SCRYPT_ID,
        EncryptionScheme::AGE_SCRYPT_VERSION,
    );

    let error = fx.refusal(&manifest_only(&manifest), None).await;

    assert!(matches!(error, TransferError::NewerFormat { .. }));
}

#[tokio::test]
async fn encrypted_bundle_without_a_passphrase_asks_for_one() {
    let fx = Fixture::new();

    let error = fx.refusal(&sealed(&[(DB_ENTRY, DB_BYTES)]), None).await;

    assert!(matches!(error, TransferError::PassphraseRequired));
}

#[tokio::test]
async fn inner_manifest_schema_is_rechecked_despite_a_current_public_manifest() {
    let fx = Fixture::new();
    let files: [(&str, &[u8]); 1] = [(DB_ENTRY, DB_BYTES)];
    let manifest = full_manifest(
        EXPECTED_SCHEMA_VERSION + 1,
        files.iter().map(|(p, b)| entry(p, b)).collect(),
    );
    let bundle = outer(
        &public_manifest(),
        &encrypt(&inner(&manifest, &files), PASSPHRASE),
    );

    let error = fx.refusal(&bundle, Some(PASSPHRASE)).await;

    assert!(matches!(error, TransferError::NewerSchema { .. }));
}

#[tokio::test]
async fn unsafe_archive_entries_are_refused() {
    let fx = Fixture::new();
    let regular = tar::EntryType::Regular;
    let cases = [
        ("parent climb", "forge/media/../../escape", regular, ""),
        ("absolute path", "/forge/media/clip.ogg", regular, ""),
        (
            "symlink",
            "forge/media/link",
            tar::EntryType::Symlink,
            "/etc/passwd",
        ),
        (
            "hardlink",
            "forge/media/hard",
            tar::EntryType::Link,
            DB_ENTRY,
        ),
        ("outside the forge folder", "other/clip.ogg", regular, ""),
        ("unlisted top-level file", "forge/notes.txt", regular, ""),
        ("excluded folder", "forge/logs/forge.log", regular, ""),
        ("reserved character", "forge/media/a:b.ogg", regular, ""),
    ];
    for (case, name, kind, link) in cases {
        let bytes = legacy(|b| add_raw(b, name, kind, link, b"x"));
        let error = fx.refusal(&bytes, None).await;
        assert!(
            matches!(error, TransferError::UnsafeEntry { .. }),
            "{case}: {error:?}"
        );
    }
}

#[tokio::test]
async fn duplicate_archive_entry_is_refused() {
    let fx = Fixture::new();
    let bytes = legacy(|b| {
        add_file(b, MEDIA_ENTRY, MEDIA_BYTES);
        add_file(b, MEDIA_ENTRY, b"replacement");
    });

    let error = fx.refusal(&bytes, None).await;

    assert!(matches!(error, TransferError::UnsafeEntry { path, .. } if path == MEDIA_ENTRY));
}

#[tokio::test]
async fn entries_disagreeing_with_the_inner_manifest_are_refused() {
    let fx = Fixture::new();
    let files: [(&str, &[u8]); 2] = [(DB_ENTRY, DB_BYTES), (MEDIA_ENTRY, MEDIA_BYTES)];
    let mut wrong_size = entry(MEDIA_ENTRY, MEDIA_BYTES);
    wrong_size.size += 1;
    let mut wrong_digest = entry(MEDIA_ENTRY, MEDIA_BYTES);
    wrong_digest.sha256_hex = sha256_hex(b"other bytes");
    let db = entry(DB_ENTRY, DB_BYTES);

    let cases: [(&str, Vec<ContentEntry>, Archived<'_>, Expected); 4] = [
        (
            "size mismatch",
            vec![db.clone(), wrong_size],
            &files,
            |e| matches!(e, TransferError::DigestMismatch { path } if path == MEDIA_ENTRY),
        ),
        (
            "digest mismatch",
            vec![db.clone(), wrong_digest],
            &files,
            |e| matches!(e, TransferError::DigestMismatch { path } if path == MEDIA_ENTRY),
        ),
        (
            "entry missing from the manifest",
            vec![db.clone()],
            &files,
            |e| matches!(e, TransferError::ContentsMismatch { path } if path == MEDIA_ENTRY),
        ),
        (
            "manifest entry missing from the archive",
            vec![db.clone(), entry(MEDIA_ENTRY, MEDIA_BYTES)],
            &files[..1],
            |e| matches!(e, TransferError::ContentsMismatch { path } if path == MEDIA_ENTRY),
        ),
    ];
    for (case, contents, archived, expected) in cases {
        let manifest = full_manifest(EXPECTED_SCHEMA_VERSION, contents);
        let bundle = outer(
            &public_manifest(),
            &encrypt(&inner(&manifest, archived), PASSPHRASE),
        );
        let error = fx.refusal(&bundle, Some(PASSPHRASE)).await;
        assert!(expected(&error), "{case}: {error:?}");
    }
}

#[tokio::test]
async fn data_after_the_encrypted_payload_is_refused() {
    let fx = Fixture::new();
    let payload = encrypt(&honest_inner(&[(DB_ENTRY, DB_BYTES)]), PASSPHRASE);
    let bundle = tar_gz(|b| {
        add_file(b, MANIFEST_ENTRY_NAME, &public_manifest());
        add_file(b, ENCRYPTED_PAYLOAD_ENTRY_NAME, &payload);
        add_file(b, "smuggled.bin", b"extra");
    });

    let error = fx.refusal(&bundle, Some(PASSPHRASE)).await;

    assert!(matches!(error, TransferError::Damaged { .. }));
}

fn previous_writer_layout(data_dir: &Path) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    builder.follow_symlinks(false);
    let root = Path::new("forge");
    builder
        .append_path_with_name(data_dir.join("snapshot.db"), root.join(DATABASE_FILE_NAME))
        .unwrap();
    builder
        .append_path_with_name(
            data_dir.join(CREDENTIALS_KEY_FILE_NAME),
            root.join(CREDENTIALS_KEY_FILE_NAME),
        )
        .unwrap();
    builder
        .append_dir_all(root.join(MEDIA_DIR_NAME), data_dir.join(MEDIA_DIR_NAME))
        .unwrap();
    let tar = builder.into_inner().unwrap();
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&tar).unwrap();
    gz.finish().unwrap()
}

#[tokio::test]
async fn manifestless_archive_in_the_previous_layout_stages_as_legacy() {
    let fx = Fixture::new();
    let source = tempfile::tempdir().unwrap();
    std::fs::write(source.path().join("snapshot.db"), DB_BYTES).unwrap();
    std::fs::write(source.path().join(CREDENTIALS_KEY_FILE_NAME), KEY_BYTES).unwrap();
    std::fs::create_dir_all(source.path().join(MEDIA_DIR_NAME).join("sub")).unwrap();
    std::fs::write(
        source
            .path()
            .join(MEDIA_DIR_NAME)
            .join("sub")
            .join("clip.ogg"),
        MEDIA_BYTES,
    )
    .unwrap();

    let staged = fx
        .stage(
            &previous_writer_layout(source.path()),
            None,
            schema_state(
                StoredSchema::Current {
                    version: EXPECTED_SCHEMA_VERSION,
                },
                true,
            ),
        )
        .await
        .unwrap();

    assert_eq!(
        files_under(&staged.payload_dir()),
        BTreeMap::from([
            (DATABASE_FILE_NAME.to_owned(), DB_BYTES.to_vec()),
            (CREDENTIALS_KEY_FILE_NAME.to_owned(), KEY_BYTES.to_vec()),
            (
                Path::new(MEDIA_DIR_NAME)
                    .join("sub")
                    .join("clip.ogg")
                    .to_string_lossy()
                    .into_owned(),
                MEDIA_BYTES.to_vec(),
            ),
        ])
    );
}

#[tokio::test]
async fn legacy_archive_summary_needs_no_passphrase() {
    let fx = Fixture::new();

    let summary = fx.summary(&legacy(|_| {})).await.unwrap();

    assert_eq!(summary, BundleSummary::LegacyUnencrypted);
}

#[tokio::test]
async fn files_that_are_not_bundles_are_refused_as_not_a_bundle() {
    let fx = Fixture::new();
    let gzip_of_text = {
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(b"plain text, not a tar").unwrap();
        gz.finish().unwrap()
    };
    let cases = [
        ("empty file", Vec::new()),
        ("not gzip", b"PK\x03\x04 zip bytes".to_vec()),
        ("gzip of non-tar", gzip_of_text),
        ("empty tar.gz", tar_gz(|_| {})),
        (
            "other first entry",
            tar_gz(|b| add_file(b, "README.md", b"hello")),
        ),
        (
            "database not first",
            tar_gz(|b| {
                add_file(b, MEDIA_ENTRY, MEDIA_BYTES);
                add_file(b, DB_ENTRY, DB_BYTES);
            }),
        ),
    ];
    for (case, bytes) in cases {
        let error = fx.refusal(&bytes, None).await;
        assert!(
            matches!(error, TransferError::NotABundle),
            "{case}: {error:?}"
        );
    }
}

#[tokio::test]
async fn archived_database_state_gates_staging() {
    let fx = Fixture::new();
    let cases: [(&str, Option<StoredSchema>, Expected); 4] = [
        (
            "newer schema",
            Some(StoredSchema::Current {
                version: EXPECTED_SCHEMA_VERSION + 1,
            }),
            |e| matches!(e, TransferError::NewerSchema { .. }),
        ),
        (
            "pre-baseline",
            Some(StoredSchema::PreBaseline { found: 44 }),
            |e| matches!(e, TransferError::SchemaNotAdoptable { found: 44, .. }),
        ),
        ("unrecognized", Some(StoredSchema::Unrecognized), |e| {
            matches!(e, TransferError::UnrecognizedDatabase)
        }),
        ("no database", None, |e| {
            matches!(e, TransferError::MissingDatabase)
        }),
    ];
    for (case, schema, expected) in cases {
        let probe: Arc<dyn DatabaseFileSchema> =
            Arc::new(FakeSchema(schema.map(|schema| StoredDatabase {
                schema,
                holds_credentials: false,
            })));
        let error = fx.refusal_with(&legacy(|_| {}), None, probe).await;
        assert!(expected(&error), "{case}: {error:?}");
    }
}

#[tokio::test]
async fn adoptable_legacy_database_is_staged() {
    let fx = Fixture::new();

    let staged = fx
        .stage(
            &legacy(|_| {}),
            None,
            schema_state(StoredSchema::AdoptableLegacy { version: 46 }, false),
        )
        .await;

    assert!(staged.is_ok());
}

#[tokio::test]
async fn database_holding_logins_without_its_key_is_refused() {
    let fx = Fixture::new();
    let probe = schema_state(
        StoredSchema::Current {
            version: EXPECTED_SCHEMA_VERSION,
        },
        true,
    );

    let error = fx.refusal_with(&legacy(|_| {}), None, probe).await;

    assert!(matches!(error, TransferError::MissingCredentialsKey));
}

#[tokio::test]
async fn database_without_logins_is_staged_without_a_key() {
    let fx = Fixture::new();

    let staged = fx
        .stage(&legacy(|_| {}), None, current_schema())
        .await
        .unwrap();

    assert_eq!(staged.credentials_key_file(), None);
}

#[tokio::test]
async fn encrypted_bundle_with_its_key_stages_the_key() {
    let fx = Fixture::new();
    let probe = schema_state(
        StoredSchema::Current {
            version: EXPECTED_SCHEMA_VERSION,
        },
        true,
    );

    let staged = fx
        .stage(
            &sealed(&[(DB_ENTRY, DB_BYTES), (KEY_ENTRY, KEY_BYTES)]),
            Some(PASSPHRASE),
            probe,
        )
        .await
        .unwrap();

    assert_eq!(
        std::fs::read(staged.credentials_key_file().unwrap()).unwrap(),
        KEY_BYTES
    );
}

#[tokio::test]
async fn discarding_a_staged_restore_removes_its_files() {
    let fx = Fixture::new();
    let staged = fx
        .stage(&legacy(|_| {}), None, current_schema())
        .await
        .unwrap();
    let staging_dir = staged.staging_dir().to_path_buf();

    discard_staged_restore(staged).await.unwrap();

    assert!(!staging_dir.exists());
}

fn sealed_with_labels(
    public: Vec<BackupCategory>,
    inner_categories: Vec<BackupCategory>,
    files: &[(&str, &[u8])],
) -> Vec<u8> {
    let contents = files.iter().map(|(p, b)| entry(p, b)).collect();
    let manifest = full_manifest_in(EXPECTED_SCHEMA_VERSION, inner_categories, contents);
    outer(
        &public_manifest_in(public),
        &encrypt(&inner(&manifest, files), PASSPHRASE),
    )
}

#[tokio::test]
async fn public_and_inner_category_labels_must_agree() {
    let fx = Fixture::new();
    let database_only = vec![BackupCategory::Database];
    let with_media = vec![BackupCategory::Database, BackupCategory::Media];
    let files: [(&str, &[u8]); 1] = [(DB_ENTRY, DB_BYTES)];
    let cases = [
        (
            "public narrower than inner",
            database_only.clone(),
            with_media.clone(),
        ),
        ("public wider than inner", with_media, database_only),
    ];
    for (case, public, inner_categories) in cases {
        let bundle = sealed_with_labels(public, inner_categories, &files);
        let error = fx.refusal(&bundle, Some(PASSPHRASE)).await;
        assert!(
            matches!(error, TransferError::Damaged { .. }),
            "{case}: {error:?}"
        );
    }
}

#[tokio::test]
async fn content_outside_the_listed_categories_is_refused() {
    let fx = Fixture::new();
    let database_only = vec![BackupCategory::Database];
    let files: [(&str, &[u8]); 2] = [(DB_ENTRY, DB_BYTES), (MEDIA_ENTRY, MEDIA_BYTES)];

    let error = fx
        .refusal(
            &sealed_with_labels(database_only.clone(), database_only, &files),
            Some(PASSPHRASE),
        )
        .await;

    assert!(matches!(error, TransferError::ContentsMismatch { path } if path == MEDIA_ENTRY));
}

#[tokio::test]
async fn a_bundle_labelled_without_the_database_is_refused() {
    let fx = Fixture::new();
    let media_only = vec![BackupCategory::Media];
    let files: [(&str, &[u8]); 1] = [(DB_ENTRY, DB_BYTES)];
    let cases = [
        ("public label", media_only.clone(), BackupCategory::all()),
        ("inner label", BackupCategory::all(), media_only),
    ];
    for (case, public, inner_categories) in cases {
        let bundle = sealed_with_labels(public, inner_categories, &files);
        let error = fx.refusal(&bundle, Some(PASSPHRASE)).await;
        assert!(
            matches!(error, TransferError::Damaged { .. }),
            "{case}: {error:?}"
        );
    }
}

#[tokio::test]
async fn restore_key_scheme_without_a_wrapped_key_is_refused_at_the_summary() {
    let fx = Fixture::new();
    let manifest = manifest_json(
        "full",
        BUNDLE_FORMAT_VERSION,
        EXPECTED_SCHEMA_VERSION,
        EncryptionScheme::AGE_X25519_WRAPPED_ID,
        EncryptionScheme::AGE_X25519_WRAPPED_VERSION,
    );

    let error = fx.summary(&manifest_only(&manifest)).await.unwrap_err();

    assert!(matches!(error, TransferError::Damaged { .. }));
}

fn manifest_without_categories(contents: &[ContentEntry]) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "format_version": BUNDLE_FORMAT_VERSION,
        "kind": "full",
        "forge_version": "0.6.0",
        "schema_version": EXPECTED_SCHEMA_VERSION,
        "created_at": "2026-10-01T00:00:00Z",
        "contents": contents,
    }))
    .unwrap()
}

#[tokio::test]
async fn bundle_written_before_categories_reads_as_a_manual_backup_of_everything() {
    let fx = Fixture::new();
    let manifest = manifest_json(
        "full",
        BUNDLE_FORMAT_VERSION,
        EXPECTED_SCHEMA_VERSION,
        EncryptionScheme::AGE_SCRYPT_ID,
        EncryptionScheme::AGE_SCRYPT_VERSION,
    );

    let summary = fx.summary(&manifest_only(&manifest)).await.unwrap();

    assert!(matches!(
        &summary,
        BundleSummary::Encrypted(m)
            if m.categories == BackupCategory::all()
                && m.origin == BundleOrigin::Manual
                && m.wrapped_identity.is_none()
    ));
}

#[tokio::test]
async fn bundle_written_before_categories_restores_every_folder() {
    let fx = Fixture::new();
    let overlay_entry = "forge/overlays/alerts/index.html";
    let files: [(&str, &[u8]); 3] = [
        (DB_ENTRY, DB_BYTES),
        (MEDIA_ENTRY, MEDIA_BYTES),
        (overlay_entry, b"<html></html>"),
    ];
    let contents: Vec<ContentEntry> = files.iter().map(|(p, b)| entry(p, b)).collect();
    let public = manifest_json(
        "full",
        BUNDLE_FORMAT_VERSION,
        EXPECTED_SCHEMA_VERSION,
        EncryptionScheme::AGE_SCRYPT_ID,
        EncryptionScheme::AGE_SCRYPT_VERSION,
    );
    let payload = encrypt(
        &inner(&manifest_without_categories(&contents), &files),
        PASSPHRASE,
    );

    let staged = fx
        .stage(
            &outer(&public, &payload),
            Some(PASSPHRASE),
            current_schema(),
        )
        .await
        .unwrap();

    let staged_files = files_under(&staged.payload_dir());
    let expected: BTreeMap<String, Vec<u8>> = files
        .iter()
        .map(|(path, bytes)| {
            let relative: PathBuf = path.split('/').skip(1).collect();
            (relative.to_string_lossy().into_owned(), bytes.to_vec())
        })
        .collect();
    assert_eq!(staged_files, expected);
}
