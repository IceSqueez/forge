use forge_storage::ScriptRepo;
use forge_types::{ScriptContract, ScriptId, ScriptInput, VariantKind};

mod common;
use common::setup;

fn make_record(name: &str, enabled: bool) -> forge_storage::ScriptRecord {
    forge_storage::ScriptRecord {
        id: ScriptId::new(),
        name: name.to_owned(),
        body: r#"print("hello");"#.to_owned(),
        contract: ScriptContract::default(),
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
async fn saving_an_existing_script_replaces_its_body_and_refreshes_only_last_modified() {
    const AN_OLD_EPOCH_MS: i64 = 1_000;
    let pools = common::migrated_pools().await;
    let repo = forge_storage_sqlite::SqliteScriptRepo::new(pools.clone());
    let mut record = make_record("mutable", true);
    let id = record.id;
    repo.save(record.clone()).await.expect("save 1");
    sqlx::query("UPDATE scripts SET created_at = ?, last_modified = ? WHERE id = ?")
        .bind(AN_OLD_EPOCH_MS)
        .bind(AN_OLD_EPOCH_MS)
        .bind(id.to_string())
        .execute(pools.writer())
        .await
        .expect("backdate");
    let old = repo.get(id).await.expect("get before").unwrap().created_at;

    record.body = r#"let x = 42;"#.to_owned();
    repo.save(record).await.expect("save 2");
    let got = repo.get(id).await.expect("get after").unwrap();

    assert_eq!(got.body, r#"let x = 42;"#);
    assert_eq!(got.created_at, old);
    assert!(got.last_modified > old);
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
