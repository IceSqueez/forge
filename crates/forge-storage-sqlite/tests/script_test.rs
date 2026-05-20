#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage::ScriptRepo;
use forge_storage_sqlite::SqliteBackend;
use forge_types::{ScriptContract, ScriptId, ScriptInput, VariantKind};

const TEST_KEY: [u8; 32] = [0xab; 32];

async fn setup() -> SqliteBackend {
    SqliteBackend::open_with_key("sqlite::memory:", TEST_KEY)
        .await
        .expect("open backend")
}

fn make_record(name: &str, enabled: bool) -> forge_storage::ScriptRecord {
    forge_storage::ScriptRecord {
        id: ScriptId::new(),
        name: name.to_owned(),
        body: r#"print("hello");"#.to_owned(),
        contract: ScriptContract::default(),
        body_hash: String::new(),
        enabled,
        created_at: time::OffsetDateTime::now_utc(),
        last_modified: time::OffsetDateTime::now_utc(),
    }
}

#[tokio::test]
async fn save_get_roundtrip() {
    let backend = setup().await;
    let record = make_record("greet_chat", true);
    let id = record.id;

    backend.save(record).await.expect("save");
    let got = backend.get(id).await.expect("get").unwrap();

    assert_eq!(got.id, id);
    assert_eq!(got.name, "greet_chat");
    assert_eq!(got.body, r#"print("hello");"#);
    assert!(got.enabled);
    assert_eq!(got.contract, ScriptContract::default());
}

#[tokio::test]
async fn body_hash_computed_on_save() {
    use sha2::{Digest, Sha256};

    let backend = setup().await;
    let body = r#"print("hello");"#;
    let record = make_record("hash_test", true);
    let id = record.id;

    backend.save(record).await.expect("save");
    let got = backend.get(id).await.expect("get").unwrap();

    let expected = {
        use std::fmt::Write as _;
        let mut hasher = Sha256::new();
        hasher.update(body.as_bytes());
        let digest = hasher.finalize();
        let mut s = String::with_capacity(digest.len() * 2);
        for b in digest.iter() {
            let _ = write!(s, "{b:02x}");
        }
        s
    };
    assert_eq!(got.body_hash, expected);
}

#[tokio::test]
async fn save_ignores_caller_provided_hash_and_recomputes() {
    use sha2::{Digest, Sha256};

    let backend = setup().await;
    let body = r#"let x = 1;"#;
    let mut record = make_record("recompute_hash", true);
    record.body = body.to_owned();
    record.body_hash = "wrong_hash_from_caller".to_owned();
    let id = record.id;

    backend.save(record).await.expect("save");
    let got = backend.get(id).await.expect("get").unwrap();

    let expected = {
        use std::fmt::Write as _;
        let mut hasher = Sha256::new();
        hasher.update(body.as_bytes());
        let digest = hasher.finalize();
        let mut s = String::with_capacity(digest.len() * 2);
        for b in digest.iter() {
            let _ = write!(s, "{b:02x}");
        }
        s
    };
    assert_eq!(got.body_hash, expected, "impl must always recompute hash");
}

#[tokio::test]
async fn get_missing_id_returns_none() {
    let backend = setup().await;
    let got = backend.get(ScriptId::new()).await.expect("get");
    assert!(got.is_none());
}

#[tokio::test]
async fn get_by_name_finds_existing_script() {
    let backend = setup().await;
    backend
        .save(make_record("known_name", true))
        .await
        .expect("save");

    let got = backend
        .get_by_name("known_name")
        .await
        .expect("get_by_name")
        .unwrap();
    assert_eq!(got.name, "known_name");
}

#[tokio::test]
async fn get_by_name_missing_returns_none() {
    let backend = setup().await;
    let got = backend.get_by_name("ghost").await.expect("get_by_name");
    assert!(got.is_none());
}

#[tokio::test]
async fn save_twice_same_name_updates_body_and_bumps_last_modified() {
    let backend = setup().await;
    let mut record = make_record("mutable", true);
    let id = record.id;
    backend.save(record.clone()).await.expect("save 1");

    let before = backend.get(id).await.expect("get before").unwrap();

    record.body = r#"let x = 42;"#.to_owned();
    backend.save(record).await.expect("save 2");
    let got = backend.get(id).await.expect("get after").unwrap();

    assert_eq!(got.body, r#"let x = 42;"#);
    assert!(
        got.last_modified >= before.last_modified,
        "last_modified must not regress"
    );
}

#[tokio::test]
async fn list_enabled_filters_correctly() {
    let backend = setup().await;
    backend
        .save(make_record("active_one", true))
        .await
        .expect("save");
    backend
        .save(make_record("active_two", true))
        .await
        .expect("save");
    backend
        .save(make_record("inactive_one", false))
        .await
        .expect("save");

    let enabled = backend.list_enabled().await.expect("list_enabled");
    assert_eq!(enabled.len(), 2);
    assert!(enabled.iter().all(|r| r.enabled));
    assert!(enabled.iter().any(|r| r.name == "active_one"));
    assert!(enabled.iter().any(|r| r.name == "active_two"));
}

#[tokio::test]
async fn list_returns_all_ordered_by_name() {
    let backend = setup().await;
    backend
        .save(make_record("zebra", true))
        .await
        .expect("save");
    backend
        .save(make_record("alpha", false))
        .await
        .expect("save");
    backend
        .save(make_record("middle", true))
        .await
        .expect("save");

    let all = backend.list().await.expect("list");
    assert_eq!(all.len(), 3);
    assert_eq!(all[0].name, "alpha");
    assert_eq!(all[1].name, "middle");
    assert_eq!(all[2].name, "zebra");
}

#[tokio::test]
async fn delete_existing_returns_true_and_removes() {
    let backend = setup().await;
    let record = make_record("to_delete", true);
    let id = record.id;
    backend.save(record).await.expect("save");

    let deleted = backend.delete(id).await.expect("delete");
    assert!(deleted);

    let all = backend.list().await.expect("list");
    assert!(all.is_empty());
}

#[tokio::test]
async fn delete_missing_returns_false() {
    let backend = setup().await;
    let deleted = backend
        .delete(ScriptId::new())
        .await
        .expect("delete missing");
    assert!(!deleted);
}

#[tokio::test]
async fn contract_roundtrip_with_inputs_and_return() {
    let backend = setup().await;
    let contract = ScriptContract {
        inputs: vec![
            ScriptInput {
                name: "user".to_owned(),
                kind: VariantKind::String,
            },
            ScriptInput {
                name: "count".to_owned(),
                kind: VariantKind::Int,
            },
        ],
        returns: Some(VariantKind::Bool),
    };
    let mut record = make_record("contract_script", true);
    record.contract = contract.clone();
    let id = record.id;

    backend.save(record).await.expect("save");
    let got = backend.get(id).await.expect("get").unwrap();

    assert_eq!(got.contract, contract);
    assert_eq!(got.contract.inputs.len(), 2);
    assert_eq!(got.contract.inputs[0].name, "user");
    assert_eq!(got.contract.inputs[0].kind, VariantKind::String);
    assert_eq!(got.contract.returns, Some(VariantKind::Bool));
}

#[tokio::test]
async fn contract_default_survives_roundtrip() {
    let backend = setup().await;
    let record = make_record("no_contract", true);
    let id = record.id;

    backend.save(record).await.expect("save");
    let got = backend.get(id).await.expect("get").unwrap();

    assert_eq!(got.contract, ScriptContract::default());
    assert!(got.contract.inputs.is_empty());
    assert!(got.contract.returns.is_none());
}
