#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::collections::BTreeMap;

use forge_storage::{ActionRepo, ScriptRecord, ScriptReferences, ScriptRepo, StorageError};
use forge_storage_sqlite::{
    MIGRATIONS, SqliteActionRepo, SqliteScriptRepo, apply_migrations, connect,
};
use forge_types::{
    Action, ActionId, DEFAULT_QUEUE_ID, ExecutionMode, ScriptContract, ScriptId, SubActionStep,
    Variant,
};
use sqlx::SqlitePool;
use tempfile::TempDir;
use time::OffsetDateTime;

const RUN_NAMED: &str = "script.run.named";
const OLD_NAME: &str = "greet";
const NEW_NAME: &str = "welcome";
const VERSION_BEFORE_ARCHIVE: i64 = 54;

struct Db {
    pool: SqlitePool,
    scripts: SqliteScriptRepo,
    actions: SqliteActionRepo,
    _dir: TempDir,
}

async fn open_at(version: Option<i64>) -> Db {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let url = format!("sqlite://{}", dir.path().join("forge.db").display());
    let pool = connect(&url).await.expect("a database");
    match version {
        Some(version) => MIGRATIONS
            .run_to(version, &pool)
            .await
            .expect("the older schema"),
        None => apply_migrations(&pool).await.expect("migrate"),
    }
    Db {
        scripts: SqliteScriptRepo::new(pool.clone()),
        actions: SqliteActionRepo::new(pool.clone()),
        pool,
        _dir: dir,
    }
}

async fn db() -> Db {
    open_at(None).await
}

fn text(value: &str) -> Variant {
    Variant::String(value.to_owned())
}

fn object(pairs: Vec<(&str, Variant)>) -> Variant {
    Variant::Object(pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}

fn step(kind_id: &str, pairs: Vec<(&str, Variant)>) -> SubActionStep {
    SubActionStep {
        kind_id: kind_id.to_owned(),
        config: pairs
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect::<BTreeMap<_, _>>(),
        enabled: true,
        continue_on_error: false,
        condition: None,
        label: None,
    }
}

fn runs(name: &str) -> SubActionStep {
    step(RUN_NAMED, vec![("script_name", text(name))])
}

fn nested_runs(name: &str) -> Variant {
    object(vec![
        ("kind_id", text(RUN_NAMED)),
        ("config", object(vec![("script_name", text(name))])),
    ])
}

fn branching_around(name: &str) -> SubActionStep {
    step(
        "core.logic.if_then_else",
        vec![
            ("condition", text("1 == 1")),
            ("then_chain", Variant::Array(vec![nested_runs(name)])),
        ],
    )
}

fn switching_around(name: &str) -> SubActionStep {
    step(
        "core.logic.switch_case",
        vec![(
            "cases",
            Variant::Array(vec![object(vec![
                ("match", text("a")),
                ("chain", Variant::Array(vec![nested_runs(name)])),
            ])]),
        )],
    )
}

fn record(name: &str) -> ScriptRecord {
    let now = OffsetDateTime::now_utc();
    ScriptRecord {
        id: ScriptId::new(),
        name: name.to_owned(),
        body: "let x = 1;".to_owned(),
        contract: ScriptContract::default(),
        enabled: true,
        created_at: now,
        last_modified: now,
    }
}

fn field<'a>(value: &'a Variant, key: &str) -> &'a Variant {
    value.as_object().unwrap().get(key).unwrap()
}

fn first(value: &Variant) -> &Variant {
    value.as_array().unwrap().first().unwrap()
}

fn named_in(value: &Variant) -> &str {
    field(field(value, "config"), "script_name")
        .as_str()
        .unwrap()
}

impl Db {
    async fn with_script(&self, name: &str) -> ScriptId {
        let r = record(name);
        self.scripts.save(r.clone()).await.unwrap();
        r.id
    }

    async fn with_action(&self, name: &str, steps: Vec<SubActionStep>) -> ActionId {
        let action = Action {
            id: ActionId::new(),
            name: name.to_owned(),
            group: None,
            queue_id: DEFAULT_QUEUE_ID,
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: ExecutionMode::Sequential,
            description: None,
            sub_actions: steps,
        };
        self.actions.save(&action).await.unwrap();
        action.id
    }

    async fn steps_of(&self, id: ActionId) -> Vec<SubActionStep> {
        self.actions.get(id).await.unwrap().unwrap().sub_actions
    }

    async fn top_level_name(&self, id: ActionId) -> String {
        self.steps_of(id).await[0]
            .config
            .get("script_name")
            .and_then(Variant::as_str)
            .unwrap()
            .to_owned()
    }

    async fn names(&self) -> (Vec<String>, Vec<String>) {
        let names = |records: Vec<ScriptRecord>| records.into_iter().map(|r| r.name).collect();
        (
            names(self.scripts.list().await.unwrap()),
            names(self.scripts.list_archived_scripts().await.unwrap()),
        )
    }
}

fn action_names(references: &ScriptReferences) -> (Vec<String>, Vec<String>) {
    let names = |list: &[forge_storage::ScriptReferrer]| {
        let mut names: Vec<_> = list.iter().map(|r| r.action_name.clone()).collect();
        names.sort();
        names
    };
    (names(&references.literal), names(&references.templated))
}

#[tokio::test]
async fn renaming_rewrites_the_literal_name_in_top_level_and_nested_steps() {
    let db = db().await;
    let id = db.with_script(OLD_NAME).await;
    let greeter = db
        .with_action(
            "Greeter",
            vec![
                runs(OLD_NAME),
                branching_around(OLD_NAME),
                switching_around(OLD_NAME),
            ],
        )
        .await;

    db.scripts.rename_cascading(id, NEW_NAME).await.unwrap();

    let steps = db.steps_of(greeter).await;
    let top = steps[0].config.get("script_name").and_then(Variant::as_str);
    let branch = named_in(first(steps[1].config.get("then_chain").unwrap()));
    let case = named_in(first(field(
        first(steps[2].config.get("cases").unwrap()),
        "chain",
    )));
    assert_eq!((top, branch, case), (Some(NEW_NAME), NEW_NAME, NEW_NAME));
}

#[tokio::test]
async fn renaming_leaves_templated_and_other_scripts_steps_alone_and_reports_both_kinds() {
    let db = db().await;
    let id = db.with_script(OLD_NAME).await;
    db.with_script("other").await;
    db.with_action("Greeter", vec![runs(OLD_NAME)]).await;
    let picker = db.with_action("Picker", vec![runs("%pick%")]).await;
    let other = db.with_action("Other", vec![runs("other")]).await;

    let references = db.scripts.rename_cascading(id, NEW_NAME).await.unwrap();

    assert_eq!(
        (
            action_names(&references),
            db.top_level_name(picker).await,
            db.top_level_name(other).await,
            db.names().await.0,
        ),
        (
            (vec!["Greeter".to_owned()], vec!["Picker".to_owned()]),
            "%pick%".to_owned(),
            "other".to_owned(),
            vec!["other".to_owned(), NEW_NAME.to_owned()],
        )
    );
}

#[tokio::test]
async fn renaming_updates_an_archived_action_without_reporting_it() {
    let db = db().await;
    let id = db.with_script(OLD_NAME).await;
    let shelved = db.with_action("Shelved", vec![runs(OLD_NAME)]).await;
    assert!(db.actions.archive(shelved).await.unwrap());

    let references = db.scripts.rename_cascading(id, NEW_NAME).await.unwrap();
    assert!(db.actions.restore(shelved).await.unwrap());

    assert_eq!(
        (references, db.top_level_name(shelved).await),
        (ScriptReferences::default(), NEW_NAME.to_owned())
    );
}

#[tokio::test]
async fn renaming_to_the_current_name_changes_nothing() {
    let db = db().await;
    let id = db.with_script(OLD_NAME).await;
    db.with_action("Greeter", vec![runs(OLD_NAME)]).await;

    let references = db.scripts.rename_cascading(id, OLD_NAME).await.unwrap();

    assert_eq!(
        (references, db.names().await.0),
        (ScriptReferences::default(), vec![OLD_NAME.to_owned()])
    );
}

#[derive(Debug, PartialEq)]
enum Refusal {
    Invalid,
    Taken,
    Missing,
    Other,
}

fn refusal(result: Result<ScriptReferences, StorageError>) -> Refusal {
    match result {
        Err(StorageError::ValidationFailed { .. }) => Refusal::Invalid,
        Err(StorageError::NameCollision { .. }) => Refusal::Taken,
        Err(StorageError::NotFound { .. }) => Refusal::Missing,
        _ => Refusal::Other,
    }
}

#[tokio::test]
async fn a_refused_rename_names_its_reason_and_changes_nothing() {
    let db = db().await;
    let live = db.with_script(OLD_NAME).await;
    db.with_script("taken").await;
    let shelved = db.with_script("shelved").await;
    assert!(db.scripts.archive_script(shelved).await.unwrap());
    let greeter = db.with_action("Greeter", vec![runs(OLD_NAME)]).await;
    let before = db.names().await;

    for (id, name, expected) in [
        (live, "   ", Refusal::Invalid),
        (live, "taken", Refusal::Taken),
        (live, "shelved", Refusal::Taken),
        (shelved, NEW_NAME, Refusal::Missing),
        (ScriptId::new(), NEW_NAME, Refusal::Missing),
    ] {
        let outcome = refusal(db.scripts.rename_cascading(id, name).await);
        assert_eq!(outcome, expected, "{name:?}");
    }

    assert_eq!(
        (db.names().await, db.top_level_name(greeter).await),
        (before, OLD_NAME.to_owned())
    );
}

#[tokio::test]
async fn saving_a_new_script_under_an_archived_scripts_name_is_a_name_collision() {
    let db = db().await;
    let shelved = db.with_script(OLD_NAME).await;
    assert!(db.scripts.archive_script(shelved).await.unwrap());

    let saved = db.scripts.save(record(OLD_NAME)).await;

    assert!(
        matches!(&saved, Err(StorageError::NameCollision { name }) if name == OLD_NAME),
        "{saved:?}"
    );
}

#[tokio::test]
async fn referrers_lists_live_actions_naming_or_templating_the_script_at_any_depth() {
    let db = db().await;
    db.with_script(OLD_NAME).await;
    db.with_action("Greeter", vec![switching_around(OLD_NAME)])
        .await;
    db.with_action("Picker", vec![branching_around("%pick%")])
        .await;
    db.with_action("Other", vec![runs("other")]).await;
    let shelved = db.with_action("Shelved", vec![runs(OLD_NAME)]).await;
    assert!(db.actions.archive(shelved).await.unwrap());

    let references = db.scripts.referrers(OLD_NAME).await.unwrap();

    assert_eq!(
        action_names(&references),
        (vec!["Greeter".to_owned()], vec!["Picker".to_owned()])
    );
}

#[tokio::test]
async fn an_archived_script_is_hidden_from_every_live_read() {
    let db = db().await;
    let id = db.with_script(OLD_NAME).await;
    db.with_script("other").await;

    assert!(db.scripts.archive_script(id).await.unwrap());

    let enabled: Vec<_> = db
        .scripts
        .list_enabled()
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.name)
        .collect();
    assert_eq!(
        (
            db.scripts.get(id).await.unwrap().is_none(),
            db.scripts.get_by_name(OLD_NAME).await.unwrap().is_none(),
            enabled,
            db.names().await,
        ),
        (
            true,
            true,
            vec!["other".to_owned()],
            (vec!["other".to_owned()], vec![OLD_NAME.to_owned()])
        )
    );
}

#[tokio::test]
async fn archive_and_restore_each_report_whether_they_moved_the_script() {
    let db = db().await;
    let id = db.with_script(OLD_NAME).await;

    let steps = [
        db.scripts.restore_script(id).await.unwrap(),
        db.scripts.archive_script(id).await.unwrap(),
        db.scripts.archive_script(id).await.unwrap(),
        db.scripts.restore_script(id).await.unwrap(),
        db.scripts.archive_script(ScriptId::new()).await.unwrap(),
    ];

    assert_eq!(
        (
            steps,
            db.scripts
                .get_by_name(OLD_NAME)
                .await
                .unwrap()
                .map(|r| r.id)
        ),
        ([false, true, false, true, false], Some(id))
    );
}

#[tokio::test]
async fn upgrading_from_the_schema_before_archiving_keeps_existing_scripts_live() {
    let db = open_at(Some(VERSION_BEFORE_ARCHIVE)).await;
    let id = ScriptId::new();
    sqlx::query(
        "INSERT INTO scripts (id, name, body, contract_json, enabled, created_at, last_modified) \
         VALUES (?, ?, 'let x = 1;', ?, 1, 0, 0)",
    )
    .bind(id.to_string())
    .bind(OLD_NAME)
    .bind(serde_json::to_string(&ScriptContract::default()).unwrap())
    .execute(&db.pool)
    .await
    .expect("a script row in the older schema");

    apply_migrations(&db.pool).await.expect("upgrade");
    let live = db.scripts.get(id).await.unwrap().map(|r| r.name);
    let archived = db.scripts.archive_script(id).await.unwrap();

    assert_eq!(
        (live, archived, db.names().await),
        (
            Some(OLD_NAME.to_owned()),
            true,
            (Vec::new(), vec![OLD_NAME.to_owned()])
        )
    );
}
