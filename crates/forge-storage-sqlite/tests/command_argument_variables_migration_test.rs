#![allow(clippy::expect_used)]
use std::collections::BTreeMap;

use forge_storage_sqlite::{MIGRATIONS, apply_migrations, connect};
use forge_types::{SubActionStep, Variant};
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use tempfile::TempDir;

const VERSION_BEFORE_COMMAND_ARGUMENTS: i64 = 63;
const QUEUE: &str = "01JQUEUEAAAAAAAAAAAAAAAAAA";
const KICK: &str = "kick.chat.command";
const YOUTUBE: &str = "youtube.chat.command";
const TWITCH: &str = "twitch.chat.command";
const LOG: &str = "core.log.write";
const RUN: &str = "core.action.run";
const MESSAGE: &str = "message";
const ARRAY_SOURCE: &str = "array_source";

const FIRST: &str = "01JACTIONAAAAAAAAAAAAAAAA1";
const SECOND: &str = "01JACTIONAAAAAAAAAAAAAAAA2";
const THIRD: &str = "01JACTIONAAAAAAAAAAAAAAAA3";

fn step(kind_id: &str, key: &str, value: &str) -> SubActionStep {
    SubActionStep {
        kind_id: kind_id.to_owned(),
        config: BTreeMap::from([(key.to_owned(), Variant::String(value.to_owned()))]),
        enabled: true,
        continue_on_error: false,
        condition: None,
        label: None,
    }
}

fn logging(text: &str) -> SubActionStep {
    step(LOG, MESSAGE, text)
}

fn calling(action: &str) -> SubActionStep {
    step(RUN, "action_id", action)
}

fn nested_in_a_branch(inner: &SubActionStep) -> SubActionStep {
    let encoded = Variant::Object(BTreeMap::from([
        ("kind_id".to_owned(), Variant::String(inner.kind_id.clone())),
        ("config".to_owned(), Variant::Object(inner.config.clone())),
        ("enabled".to_owned(), Variant::Bool(true)),
    ]));
    SubActionStep {
        config: BTreeMap::from([("then_chain".to_owned(), Variant::Array(vec![encoded]))]),
        ..step("core.flow.branch", MESSAGE, "")
    }
}

struct Seeded {
    pool: SqlitePool,
    _dir: TempDir,
    actions: usize,
    overrides: String,
}

impl Seeded {
    async fn empty() -> Self {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let url = format!("sqlite://{}", dir.path().join("forge.db").display());
        let pool = connect(&url).await.expect("a database");
        Self::prepared(pool, dir).await
    }

    async fn prepared(pool: SqlitePool, dir: TempDir) -> Self {
        MIGRATIONS
            .run_to(VERSION_BEFORE_COMMAND_ARGUMENTS, &pool)
            .await
            .expect("the schema before the command variables");
        sqlx::query("INSERT INTO queues (id, name, description, blocking, concurrency) VALUES (?, 'q', '', 1, 1)")
            .bind(QUEUE)
            .execute(&pool)
            .await
            .expect("a stored queue");
        Self {
            pool,
            _dir: dir,
            actions: 0,
            overrides: "{}".to_owned(),
        }
    }

    async fn action(&mut self, id: &str, triggers: &[&str], steps: &[SubActionStep]) {
        self.actions += 1;
        sqlx::query("INSERT INTO actions (id, name, queue_id, sub_actions) VALUES (?, ?, ?, ?)")
            .bind(id)
            .bind(format!("action {}", self.actions))
            .bind(QUEUE)
            .bind(serde_json::to_string(steps).expect("the steps encode"))
            .execute(&self.pool)
            .await
            .expect("a stored action");
        for (position, kind_id) in (0_i64..).zip(triggers) {
            let trigger_id = format!("{id}-{position}");
            sqlx::query(
                "INSERT INTO trigger_instances (id, kind_id, name, overrides, user_defined) VALUES (?, ?, 'cmd', ?, 1)",
            )
                .bind(&trigger_id)
                .bind(kind_id)
                .bind(&self.overrides)
                .execute(&self.pool)
                .await
                .expect("a stored trigger");
            sqlx::query(
                "INSERT INTO action_trigger_instances (action_id, trigger_instance_id, position)
                 VALUES (?, ?, ?)",
            )
            .bind(id)
            .bind(&trigger_id)
            .bind(position)
            .execute(&self.pool)
            .await
            .expect("a stored binding");
        }
    }

    async fn migrated(&self) {
        apply_migrations(&self.pool)
            .await
            .expect("the migrations apply");
    }

    async fn steps(&self, id: &str) -> Vec<SubActionStep> {
        let stored: String = sqlx::query_scalar("SELECT sub_actions FROM actions WHERE id = ?")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .expect("the action reads");
        serde_json::from_str(&stored).expect("the migrated steps still decode")
    }

    async fn stored_value(&self, id: &str, position: usize, key: &str) -> Variant {
        self.steps(id).await[position].config[key].clone()
    }

    async fn stored_text(&self, id: &str) -> Variant {
        self.stored_value(id, 0, MESSAGE).await
    }
}

fn text(value: &str) -> Variant {
    Variant::String(value.to_owned())
}

async fn after_migration(kind_id: &str, steps: &[SubActionStep]) -> Seeded {
    let mut seeded = Seeded::empty().await;
    seeded.action(FIRST, &[kind_id], steps).await;
    seeded.migrated().await;
    seeded
}

#[tokio::test]
async fn each_platform_reference_moves_to_the_variable_of_the_same_shape() {
    for (kind_id, old, new) in [
        (KICK, "got %args%!", "got %command.tail%!"),
        (
            KICK,
            "%args.0% and %args.12%",
            "%command.args.0% and %command.args.12%",
        ),
        (YOUTUBE, "got %args%!", "got %command.args%!"),
        (YOUTUBE, "%args.0%", "%command.args.0%"),
    ] {
        let seeded = after_migration(kind_id, &[logging(old)]).await;

        assert_eq!(
            seeded.stored_text(FIRST).await,
            text(new),
            "{kind_id} {old}"
        );
    }
}

#[tokio::test]
async fn names_that_only_start_like_the_old_variable_stay_as_they_are() {
    for name in ["%args_count%", "%argsy%", "%my.args%", "args", "100%args"] {
        let seeded = after_migration(KICK, &[logging(name)]).await;

        assert_eq!(seeded.stored_text(FIRST).await, text(name), "{name}");
    }
}

#[tokio::test]
async fn an_array_source_of_either_spelling_moves_to_the_argument_array_on_both_platforms() {
    for kind_id in [KICK, YOUTUBE] {
        for (old, new) in [("args", "command.args"), ("%args%", "%command.args%")] {
            let looping = step("core.flow.foreach", ARRAY_SOURCE, old);

            let seeded = after_migration(kind_id, &[looping]).await;

            assert_eq!(
                seeded.stored_value(FIRST, 0, ARRAY_SOURCE).await,
                text(new),
                "{kind_id} {old}"
            );
        }
    }
}

#[tokio::test]
async fn an_action_that_writes_its_own_args_keeps_every_reference() {
    for (kind_id, field) in [
        ("core.args.set", "name"),
        ("core.math.evaluate", "into_var"),
        ("core.globals.get", "into_arg"),
        ("core.random.int", "target_var"),
        ("core.string.regex_match", "captures_into_var"),
        ("core.action.schedule", "due_at_into_var"),
        ("core.action.schedule", "id_into_var"),
    ] {
        for spelling in ["args", "%args%"] {
            let seeded =
                after_migration(KICK, &[logging("%args%"), step(kind_id, field, spelling)]).await;

            assert_eq!(
                seeded.stored_text(FIRST).await,
                text("%args%"),
                "{kind_id} {field} {spelling}"
            );
        }
    }
}

#[tokio::test]
async fn a_writer_nested_inside_a_branch_also_keeps_the_action_as_it_is() {
    let writer = step("core.args.set", "name", "args");

    let seeded = after_migration(KICK, &[logging("%args%"), nested_in_a_branch(&writer)]).await;

    assert_eq!(seeded.stored_text(FIRST).await, text("%args%"));
}

#[tokio::test]
async fn an_action_with_no_kick_or_youtube_command_keeps_its_references() {
    let mut seeded = Seeded::empty().await;
    seeded.action(FIRST, &[TWITCH], &[logging("%args%")]).await;
    seeded.action(SECOND, &[], &[logging("%args.0%")]).await;
    seeded
        .action(THIRD, &["kick.chat.message"], &[logging("%args%")])
        .await;

    seeded.migrated().await;

    assert_eq!(seeded.stored_text(FIRST).await, text("%args%"));
    assert_eq!(seeded.stored_text(SECOND).await, text("%args.0%"));
    assert_eq!(seeded.stored_text(THIRD).await, text("%args%"));
}

#[tokio::test]
async fn an_action_on_both_command_triggers_takes_the_kick_text_reference() {
    let mut seeded = Seeded::empty().await;
    seeded
        .action(FIRST, &[KICK, YOUTUBE], &[logging("%args% %args.0%")])
        .await;

    seeded.migrated().await;

    assert_eq!(
        seeded.stored_text(FIRST).await,
        text("%command.tail% %command.args.0%")
    );
}

#[tokio::test]
async fn every_action_down_a_call_chain_follows_the_command_that_reaches_it() {
    let mut seeded = Seeded::empty().await;
    seeded
        .action(FIRST, &[YOUTUBE], &[logging("%args%"), calling(SECOND)])
        .await;
    seeded
        .action(SECOND, &[], &[logging("%args%"), calling(THIRD)])
        .await;
    seeded.action(THIRD, &[], &[logging("%args.1%")]).await;

    seeded.migrated().await;

    assert_eq!(seeded.stored_text(FIRST).await, text("%command.args%"));
    assert_eq!(seeded.stored_text(SECOND).await, text("%command.args%"));
    assert_eq!(seeded.stored_text(THIRD).await, text("%command.args.1%"));
}

#[tokio::test]
async fn a_call_cycle_finishes_and_upgrades_both_of_its_actions() {
    let mut seeded = Seeded::empty().await;
    seeded
        .action(FIRST, &[KICK], &[logging("%args%"), calling(SECOND)])
        .await;
    seeded
        .action(SECOND, &[], &[logging("%args%"), calling(FIRST)])
        .await;

    seeded.migrated().await;

    assert_eq!(seeded.stored_text(FIRST).await, text("%command.tail%"));
    assert_eq!(seeded.stored_text(SECOND).await, text("%command.tail%"));
}

#[tokio::test]
async fn references_in_conditions_labels_and_nested_steps_move_with_the_rest() {
    let nested = nested_in_a_branch(&logging("%args.0%"));
    let guarded = SubActionStep {
        condition: Some(r#"%args% == "go""#.to_owned()),
        label: Some("said %args%".to_owned()),
        ..logging("plain")
    };

    let seeded = after_migration(KICK, &[nested, guarded]).await;

    let steps = seeded.steps(FIRST).await;
    assert_eq!(
        steps[0].config["then_chain"],
        Variant::Array(vec![Variant::Object(BTreeMap::from([
            ("kind_id".to_owned(), text(LOG)),
            (
                "config".to_owned(),
                Variant::Object(BTreeMap::from([(
                    MESSAGE.to_owned(),
                    text("%command.args.0%")
                )]))
            ),
            ("enabled".to_owned(), Variant::Bool(true)),
        ]))])
    );
    assert_eq!(
        steps[1].condition.as_deref(),
        Some(r#"%command.tail% == "go""#)
    );
    assert_eq!(steps[1].label.as_deref(), Some("said %command.tail%"));
}

#[tokio::test]
async fn the_migration_leaves_no_temporary_table_behind() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let url = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("forge.db").display()
    );
    let one_connection = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("a database on one connection");
    let mut seeded = Seeded::prepared(one_connection, dir).await;
    seeded.action(FIRST, &[KICK], &[logging("%args%")]).await;

    seeded.migrated().await;

    let left: Vec<String> = sqlx::query_scalar("SELECT name FROM sqlite_temp_master WHERE tbl_name IN ('command_argument_readers', 'command_argument_writers', 'variable_output_fields', 'kick_command_words', 'command_name_edits')")
        .fetch_all(&seeded.pool)
        .await
        .expect("the temporary schema reads");
    assert_eq!(left, Vec::<String>::new());
}

#[tokio::test]
async fn a_callee_inheriting_a_callers_own_args_keeps_its_reference() {
    let mut seeded = Seeded::empty().await;
    seeded
        .action(
            FIRST,
            &[KICK],
            &[step("core.args.set", "name", "args"), calling(SECOND)],
        )
        .await;
    seeded.action(SECOND, &[], &[logging("%args%")]).await;

    seeded.migrated().await;

    assert_eq!(seeded.stored_text(SECOND).await, text("%args%"));
}

#[tokio::test]
async fn a_callee_also_reached_by_a_plain_chain_follows_that_chain() {
    let mut seeded = Seeded::empty().await;
    seeded
        .action(
            FIRST,
            &[KICK],
            &[step("core.args.set", "name", "args"), calling(THIRD)],
        )
        .await;
    seeded
        .action(SECOND, &[KICK], &[logging("hi"), calling(THIRD)])
        .await;
    seeded.action(THIRD, &[], &[logging("%args%")]).await;

    seeded.migrated().await;

    assert_eq!(seeded.stored_text(THIRD).await, text("%command.tail%"));
}

#[tokio::test]
async fn a_global_named_args_does_not_stop_the_upgrade() {
    for kind_id in ["core.globals.set", "core.globals.increment"] {
        let seeded =
            after_migration(KICK, &[logging("%args%"), step(kind_id, "name", "args")]).await;

        assert_eq!(
            seeded.stored_text(FIRST).await,
            text("%command.tail%"),
            "{kind_id}"
        );
    }
}

#[tokio::test]
async fn a_kind_outside_the_writers_does_not_stop_the_upgrade_through_a_shared_field() {
    let seeded = after_migration(
        KICK,
        &[
            logging("%args%"),
            step("core.log.write", "into_var", "args"),
        ],
    )
    .await;

    assert_eq!(seeded.stored_text(FIRST).await, text("%command.tail%"));
}

fn phrase_overrides(phrase: &str) -> String {
    serde_json::to_string(&BTreeMap::from([(
        "phrase".to_owned(),
        Variant::String(phrase.to_owned()),
    )]))
    .expect("the overrides encode")
}

fn guarded(condition: &str) -> SubActionStep {
    SubActionStep {
        condition: Some(condition.to_owned()),
        ..logging("plain")
    }
}

async fn after_migration_on_phrase(phrase: &str, steps: &[SubActionStep]) -> Seeded {
    let mut seeded = Seeded::empty().await;
    seeded.overrides = phrase_overrides(phrase);
    seeded.action(FIRST, &[KICK], steps).await;
    seeded.migrated().await;
    seeded
}

#[tokio::test]
async fn a_kick_condition_on_the_command_name_loses_the_marker_of_its_phrase() {
    for (phrase, old, new) in [
        (
            "!so",
            r#"%command_name% == "!so""#,
            r#"%command_name% == "so""#,
        ),
        (
            "~so",
            r#""~so" != %command_name%"#,
            r#""so" != %command_name%"#,
        ),
        ("!!so", "%command_name% == `!!so`", "%command_name% == `so`"),
        (
            "!shout out",
            r#"%command_name% == "!shout""#,
            r#"%command_name% == "shout""#,
        ),
    ] {
        let seeded = after_migration_on_phrase(phrase, &[guarded(old)]).await;

        assert_eq!(
            seeded.steps(FIRST).await[0].condition.as_deref(),
            Some(new),
            "{phrase}"
        );
    }
}

#[tokio::test]
async fn the_default_phrase_stands_in_for_a_trigger_without_an_override() {
    let seeded = after_migration(KICK, &[guarded(r#"%command_name% == "!command""#)]).await;

    assert_eq!(
        seeded.steps(FIRST).await[0].condition.as_deref(),
        Some(r#"%command_name% == "command""#)
    );
}

#[tokio::test]
async fn a_condition_outside_the_command_name_or_the_phrase_stays_as_it_is() {
    for (phrase, condition) in [
        ("!so", r#"%message% == "!so""#),
        ("!so", r#"%command_name% == "!other""#),
        ("!so", r#"%command_name% == "!so now""#),
        ("so", r#"%command_name% == "so""#),
        ("!!!", r#"%command_name% == "!!!""#),
    ] {
        let seeded = after_migration_on_phrase(phrase, &[guarded(condition)]).await;

        assert_eq!(
            seeded.steps(FIRST).await[0].condition.as_deref(),
            Some(condition),
            "{phrase}"
        );
    }
}

#[tokio::test]
async fn every_phrase_of_an_action_is_rewritten_down_its_call_chain() {
    let mut seeded = Seeded::empty().await;
    seeded.overrides = phrase_overrides("!one");
    seeded.action(FIRST, &[KICK], &[calling(SECOND)]).await;
    seeded.overrides = phrase_overrides("!two");
    seeded.action(THIRD, &[KICK], &[calling(SECOND)]).await;
    seeded
        .action(
            SECOND,
            &[],
            &[
                guarded(r#"%command_name% == "!one""#),
                guarded(r#"%command_name% == "!two""#),
            ],
        )
        .await;

    seeded.migrated().await;

    let steps = seeded.steps(SECOND).await;
    assert_eq!(
        steps[0].condition.as_deref(),
        Some(r#"%command_name% == "one""#)
    );
    assert_eq!(
        steps[1].condition.as_deref(),
        Some(r#"%command_name% == "two""#)
    );
}

#[tokio::test]
async fn a_caller_writing_its_own_args_still_gets_its_name_condition_rewritten() {
    let seeded = after_migration_on_phrase(
        "!so",
        &[
            guarded(r#"%command_name% == "!so""#),
            step("core.args.set", "name", "args"),
        ],
    )
    .await;

    assert_eq!(
        seeded.steps(FIRST).await[0].condition.as_deref(),
        Some(r#"%command_name% == "so""#)
    );
}

#[tokio::test]
async fn branch_loop_and_nested_conditions_follow_the_step_condition() {
    let old = r#"%command_name% == "!so""#;
    let new = r#"%command_name% == "so""#;
    for key in ["condition", "while_condition"] {
        let flow = step("core.logic.if_then_else", key, old);
        let rewritten = step("core.logic.if_then_else", key, new);

        let seeded =
            after_migration_on_phrase("!so", &[flow.clone(), nested_in_a_branch(&flow)]).await;

        assert_eq!(seeded.stored_value(FIRST, 0, key).await, text(new), "{key}");
        assert_eq!(
            seeded.stored_value(FIRST, 1, "then_chain").await,
            nested_in_a_branch(&rewritten).config["then_chain"],
            "{key} nested"
        );
    }
}

fn switch_on(value: &str, cases: &[&str]) -> SubActionStep {
    let cases = cases
        .iter()
        .map(|matched| {
            Variant::Object(BTreeMap::from([
                ("match".to_owned(), text(matched)),
                ("chain".to_owned(), Variant::Array(Vec::new())),
            ]))
        })
        .collect();
    let mut switch = step("core.logic.switch_case", "expression", value);
    switch
        .config
        .insert("cases".to_owned(), Variant::Array(cases));
    switch
}

#[tokio::test]
async fn a_switch_on_the_command_name_renames_its_matching_cases_only() {
    let seeded = after_migration_on_phrase(
        "!so",
        &[
            switch_on("%command_name%", &["!so", "!other"]),
            switch_on("%message%", &["!so"]),
        ],
    )
    .await;

    assert_eq!(
        seeded.steps(FIRST).await,
        vec![
            switch_on("%command_name%", &["so", "!other"]),
            switch_on("%message%", &["!so"]),
        ]
    );
}

#[tokio::test]
async fn a_youtube_condition_on_the_command_name_stays_as_it_is() {
    let condition = r#"%command_name% == "!so""#;

    let seeded = after_migration(YOUTUBE, &[guarded(condition)]).await;

    assert_eq!(
        seeded.steps(FIRST).await[0].condition.as_deref(),
        Some(condition)
    );
}

#[tokio::test]
async fn a_row_of_malformed_steps_does_not_stop_the_migration() {
    let mut seeded = Seeded::empty().await;
    seeded.action(FIRST, &[KICK], &[logging("%args%")]).await;
    sqlx::query("UPDATE actions SET sub_actions = 'not json' WHERE id = ?")
        .bind(FIRST)
        .execute(&seeded.pool)
        .await
        .expect("a malformed row");
    seeded.action(SECOND, &[KICK], &[logging("%args%")]).await;

    seeded.migrated().await;

    assert_eq!(seeded.stored_text(SECOND).await, text("%command.tail%"));
}
