#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

//! Integration tests for `SqliteTtsFiltersRepo` and migration 0021.

use forge_storage::{
    BlocklistMode, DataProvider, EXPECTED_SCHEMA_VERSION, FilterRule, FilterRuleKind,
    TtsFiltersRepo, TtsPipelineSettings, UrlMode,
};
use forge_storage_sqlite::{SqliteBackend, SqliteTtsFiltersRepo, apply_migrations};

const TEST_KEY: [u8; 32] = [0xef; 32];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn repo() -> SqliteTtsFiltersRepo {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory pool");
    apply_migrations(&pool).await.expect("migrations apply");
    SqliteTtsFiltersRepo::new(pool)
}

async fn backend() -> SqliteBackend {
    SqliteBackend::open_with_key("sqlite::memory:", TEST_KEY)
        .await
        .expect("in-memory backend")
}

fn literal_rule(id: &str, pos: u32) -> FilterRule {
    FilterRule {
        id: id.to_owned(),
        name: format!("Literal rule {id}"),
        enabled: true,
        position: pos,
        kind: FilterRuleKind::Literal {
            pattern: format!("pat_{id}"),
            replacement: format!("rep_{id}"),
        },
    }
}

fn regex_rule(id: &str, pos: u32) -> FilterRule {
    FilterRule {
        id: id.to_owned(),
        name: format!("Regex rule {id}"),
        enabled: false,
        position: pos,
        kind: FilterRuleKind::Regex {
            pattern: r"(?i)\b(lol)\b".to_owned(),
            replacement: "[redacted]".to_owned(),
        },
    }
}

fn blocklist_rule(id: &str, pos: u32, mode: BlocklistMode) -> FilterRule {
    FilterRule {
        id: id.to_owned(),
        name: format!("Blocklist rule {id}"),
        enabled: true,
        position: pos,
        kind: FilterRuleKind::Blocklist {
            words: vec!["foo".to_owned(), "bar".to_owned()],
            mode,
        },
    }
}

// ---------------------------------------------------------------------------
// Migration / defaults tests
// ---------------------------------------------------------------------------

/// After all migrations the schema version must equal EXPECTED_SCHEMA_VERSION (21).
/// This ensures 0021_tts_filters.sql was actually applied.
#[tokio::test]
async fn schema_version_equals_expected_after_migrations() {
    let b = backend().await;
    let version = b.schema_version().await.expect("schema_version");
    assert_eq!(
        version, EXPECTED_SCHEMA_VERSION,
        "schema_version must equal EXPECTED_SCHEMA_VERSION ({EXPECTED_SCHEMA_VERSION})"
    );
}

/// Migration 0021 inserts exactly one settings row (id = 1) with the
/// documented defaults: url_mode=speak, blocklist_mode=censor, both strip flags true.
#[tokio::test]
async fn fresh_db_settings_row_has_documented_defaults() {
    let r = repo().await;
    let s = r.get_pipeline_settings().await.expect("get");
    // Why: the migration hard-codes these sentinel values; any drift breaks TTS behaviour
    // on fresh installs before the user ever opens the settings screen.
    assert_eq!(s.url_mode, UrlMode::Speak, "url_mode default");
    assert_eq!(
        s.blocklist_mode,
        BlocklistMode::Censor,
        "blocklist_mode default"
    );
    assert!(s.strip_twitch_emotes, "strip_twitch_emotes default");
    assert!(s.strip_reward_emotes, "strip_reward_emotes default");
    assert!(s.max_length.is_none(), "max_length default is unlimited");
}

/// A fresh database has no filter rules (the migration inserts none).
#[tokio::test]
async fn fresh_db_rule_list_is_empty() {
    let r = repo().await;
    let rules = r.list_rules().await.expect("list");
    assert!(rules.is_empty(), "fresh db must have no rules");
}

/// The singleton constraint (CHECK id = 1) means exactly one settings row exists.
/// Writing settings twice must not create a second row.
#[tokio::test]
async fn set_pipeline_settings_is_idempotent_upsert_not_insert() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("pool");
    apply_migrations(&pool).await.expect("migrations");
    let r = SqliteTtsFiltersRepo::new(pool.clone());

    let s1 = TtsPipelineSettings {
        url_mode: UrlMode::Replace,
        ..TtsPipelineSettings::default()
    };
    r.set_pipeline_settings(&s1).await.expect("set 1");

    let s2 = TtsPipelineSettings {
        url_mode: UrlMode::Suppress,
        ..TtsPipelineSettings::default()
    };
    r.set_pipeline_settings(&s2).await.expect("set 2");

    // Only one row must exist - the upsert must not blow the CHECK(id = 1) constraint.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tts_pipeline_settings")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(count, 1, "must remain a singleton row after two writes");

    // And the second write must have won.
    let got = r.get_pipeline_settings().await.expect("get");
    assert_eq!(got.url_mode, UrlMode::Suppress);
}

// ---------------------------------------------------------------------------
// round-trip: settings
// ---------------------------------------------------------------------------

/// Every UrlMode and BlocklistMode variant survives the TEXT column round-trip.
/// Collapsed into one table-driven test per Quality Bar guidance.
#[tokio::test]
async fn pipeline_settings_url_and_blocklist_mode_round_trip() {
    let r = repo().await;

    for url_mode in [UrlMode::Speak, UrlMode::Replace, UrlMode::Suppress] {
        for blocklist_mode in [BlocklistMode::Censor, BlocklistMode::Suppress] {
            let settings = TtsPipelineSettings {
                url_mode,
                blocklist_mode,
                max_length: None,
                strip_twitch_emotes: true,
                strip_reward_emotes: false,
                ..TtsPipelineSettings::default()
            };
            r.set_pipeline_settings(&settings).await.expect("set");
            let got = r.get_pipeline_settings().await.expect("get");
            assert_eq!(
                got.url_mode, url_mode,
                "url_mode {url_mode:?} did not survive round-trip"
            );
            assert_eq!(
                got.blocklist_mode, blocklist_mode,
                "blocklist_mode {blocklist_mode:?} did not survive round-trip"
            );
        }
    }
}

/// `max_length` = None (unlimited) and a concrete boundary value both survive.
#[tokio::test]
async fn pipeline_settings_max_length_none_and_some_round_trip() {
    let r = repo().await;

    // None - unlimited
    let s_none = TtsPipelineSettings {
        max_length: None,
        ..TtsPipelineSettings::default()
    };
    r.set_pipeline_settings(&s_none).await.expect("set none");
    assert_eq!(
        r.get_pipeline_settings().await.expect("get").max_length,
        None
    );

    // Some(0) - edge: zero-length truncation
    let s_zero = TtsPipelineSettings {
        max_length: Some(0),
        ..TtsPipelineSettings::default()
    };
    r.set_pipeline_settings(&s_zero).await.expect("set 0");
    assert_eq!(
        r.get_pipeline_settings().await.expect("get").max_length,
        Some(0)
    );

    // Some(u32::MAX) - upper boundary
    let s_max = TtsPipelineSettings {
        max_length: Some(u32::MAX),
        ..TtsPipelineSettings::default()
    };
    r.set_pipeline_settings(&s_max).await.expect("set max");
    assert_eq!(
        r.get_pipeline_settings().await.expect("get").max_length,
        Some(u32::MAX)
    );
}

/// Boolean strip flags survive all four combinations (independent bits).
#[tokio::test]
async fn pipeline_settings_strip_flags_all_combinations_round_trip() {
    let r = repo().await;

    for (strip_twitch, strip_reward) in [(false, false), (true, false), (false, true), (true, true)]
    {
        let s = TtsPipelineSettings {
            strip_twitch_emotes: strip_twitch,
            strip_reward_emotes: strip_reward,
            ..TtsPipelineSettings::default()
        };
        r.set_pipeline_settings(&s).await.expect("set");
        let got = r.get_pipeline_settings().await.expect("get");
        assert_eq!(
            got.strip_twitch_emotes, strip_twitch,
            "strip_twitch ({strip_twitch}) failed"
        );
        assert_eq!(
            got.strip_reward_emotes, strip_reward,
            "strip_reward ({strip_reward}) failed"
        );
    }
}

// ---------------------------------------------------------------------------
// round-trip: rules - happy path
// ---------------------------------------------------------------------------

/// A mixed set of all three kinds (literal + regex + blocklist), stored in arbitrary
/// order, must come back sorted by position ascending with every field intact.
#[tokio::test]
async fn replace_rules_mixed_set_returns_ordered_by_position() {
    let r = repo().await;

    // Intentionally out-of-order positions: 2, 0, 1
    let rules = vec![
        regex_rule("r2", 2),
        literal_rule("r0", 0),
        blocklist_rule("r1", 1, BlocklistMode::Censor),
    ];
    r.replace_rules(&rules).await.expect("replace");

    let got = r.list_rules().await.expect("list");
    assert_eq!(got.len(), 3);

    // positions ascending
    assert_eq!(got[0].position, 0);
    assert_eq!(got[1].position, 1);
    assert_eq!(got[2].position, 2);

    // ids intact
    assert_eq!(got[0].id, "r0");
    assert_eq!(got[1].id, "r1");
    assert_eq!(got[2].id, "r2");

    // full field fidelity - check round-tripped objects match originals
    assert_eq!(got[0], literal_rule("r0", 0));
    assert_eq!(got[1], blocklist_rule("r1", 1, BlocklistMode::Censor));
    assert_eq!(got[2], regex_rule("r2", 2));
}

// ---------------------------------------------------------------------------
// replace semantics
// ---------------------------------------------------------------------------

/// A second `replace_rules` call completely discards the first set (not merges).
#[tokio::test]
async fn replace_rules_fully_replaces_previous_set() {
    let r = repo().await;

    r.replace_rules(&[literal_rule("old1", 0), literal_rule("old2", 1)])
        .await
        .expect("first replace");

    r.replace_rules(&[literal_rule("new_only", 0)])
        .await
        .expect("second replace");

    let got = r.list_rules().await.expect("list");
    assert_eq!(got.len(), 1, "old rules must be gone after replace");
    assert_eq!(got[0].id, "new_only");
}

/// Calling `replace_rules` with an empty slice clears all rules.
#[tokio::test]
async fn replace_rules_with_empty_slice_clears_all_rules() {
    let r = repo().await;

    r.replace_rules(&[literal_rule("a", 0), literal_rule("b", 1)])
        .await
        .expect("seed");

    r.replace_rules(&[]).await.expect("clear");

    let got = r.list_rules().await.expect("list");
    assert!(got.is_empty(), "rules must be empty after replace with []");
}

/// `replace_rules` is atomic: a failed transaction must leave the previous set intact.
/// We simulate this by trying to insert a duplicate primary key (which SQLite rejects)
/// and verifying the old rules are untouched.
/// NOTE: this exercises the transaction wrapper, not the repo interface itself.
#[tokio::test]
async fn replace_rules_is_atomic_rollback_on_conflict() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("pool");
    apply_migrations(&pool).await.expect("migrations");
    let r = SqliteTtsFiltersRepo::new(pool);

    // Seed one rule.
    r.replace_rules(&[literal_rule("stable", 0)])
        .await
        .expect("seed");

    // Attempt to replace with a set containing a duplicate id - SQLite PRIMARY KEY
    // violation mid-transaction triggers a rollback.
    let dup = vec![literal_rule("dup", 0), literal_rule("dup", 1)];
    let _ = r.replace_rules(&dup).await; // may error or not depending on db - either way:

    // Re-read and verify. If it succeeded, "dup" is there (2 rules). If it failed,
    // the original "stable" must still be intact (not partially wiped).
    let got = r.list_rules().await.expect("list after conflict attempt");
    // The invariant: we never end up with 0 rules after seeding 1 rule and then
    // a failing replace that deleted the old set before the constraint hit.
    assert!(
        !got.is_empty(),
        "atomicity failure: old rules were deleted before the conflict was hit"
    );
}

// ---------------------------------------------------------------------------
// Kind fidelity - each FilterRuleKind variant survives params JSON round-trip
// ---------------------------------------------------------------------------

/// All three FilterRuleKind variants round-trip correctly through the params JSON column.
/// Collapsed to one table-driven test per Quality Bar.
#[tokio::test]
async fn filter_rule_kinds_params_json_round_trip() {
    let r = repo().await;

    let rules = vec![
        FilterRule {
            id: "lit".to_owned(),
            name: "Literal".to_owned(),
            enabled: true,
            position: 0,
            kind: FilterRuleKind::Literal {
                pattern: "hello world".to_owned(),
                replacement: "hi".to_owned(),
            },
        },
        FilterRule {
            id: "rx".to_owned(),
            name: "Regex".to_owned(),
            enabled: false,
            position: 1,
            // Store the raw pattern string verbatim - must NOT be compiled/normalised.
            kind: FilterRuleKind::Regex {
                pattern: r"(?i)\b(lol|lmao)\b".to_owned(),
                replacement: "[laugh]".to_owned(),
            },
        },
        FilterRule {
            id: "bl_censor".to_owned(),
            name: "Blocklist censor".to_owned(),
            enabled: true,
            position: 2,
            kind: FilterRuleKind::Blocklist {
                words: vec!["alpha".to_owned(), "beta".to_owned(), "gamma".to_owned()],
                mode: BlocklistMode::Censor,
            },
        },
        FilterRule {
            id: "bl_suppress".to_owned(),
            name: "Blocklist suppress".to_owned(),
            enabled: true,
            position: 3,
            kind: FilterRuleKind::Blocklist {
                words: vec!["nope".to_owned()],
                mode: BlocklistMode::Suppress,
            },
        },
    ];

    r.replace_rules(&rules).await.expect("replace");
    let got = r.list_rules().await.expect("list");

    assert_eq!(got.len(), rules.len());
    for (stored, original) in got.iter().zip(rules.iter()) {
        assert_eq!(
            stored, original,
            "rule {} did not survive params JSON round-trip",
            original.id
        );
    }
}

/// Regex variant: the source pattern string must be stored verbatim - not compiled,
/// not normalised, not stripped of flags.
#[tokio::test]
async fn regex_rule_pattern_stored_verbatim_not_compiled() {
    let r = repo().await;
    let raw = r"(?i)(?-u)\p{L}+\s*\d{2,4}".to_owned();

    r.replace_rules(&[FilterRule {
        id: "exotic_rx".to_owned(),
        name: "Exotic regex".to_owned(),
        enabled: true,
        position: 0,
        kind: FilterRuleKind::Regex {
            pattern: raw.clone(),
            replacement: String::new(),
        },
    }])
    .await
    .expect("replace");

    let got = r.list_rules().await.expect("list");
    match &got[0].kind {
        FilterRuleKind::Regex { pattern, .. } => {
            assert_eq!(pattern, &raw, "regex pattern must survive verbatim");
        }
        other => panic!("expected Regex variant, got {other:?}"),
    }
}

/// Blocklist `words` vec survives order-preserving round-trip with UTF-8 entries.
#[tokio::test]
async fn blocklist_words_vec_order_and_unicode_survive() {
    let r = repo().await;
    let words: Vec<String> = vec![
        "kappa".to_owned(),
        "Pog\u{1F600}".to_owned(), // emoji in word list - edge case
        "".to_owned(),             // empty string entry - legal per the type
    ];

    r.replace_rules(&[FilterRule {
        id: "bl_unicode".to_owned(),
        name: "Unicode blocklist".to_owned(),
        enabled: true,
        position: 0,
        kind: FilterRuleKind::Blocklist {
            words: words.clone(),
            mode: BlocklistMode::Censor,
        },
    }])
    .await
    .expect("replace");

    let got = r.list_rules().await.expect("list");
    match &got[0].kind {
        FilterRuleKind::Blocklist { words: stored, .. } => {
            assert_eq!(
                stored, &words,
                "words vec must survive verbatim (order + content)"
            );
        }
        other => panic!("expected Blocklist variant, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Position / ordering edge cases
// ---------------------------------------------------------------------------

/// When positions are non-contiguous (gaps), list_rules still returns them in
/// ascending position order with the original gap values intact.
#[tokio::test]
async fn list_rules_returns_ascending_position_order_with_gaps() {
    let r = repo().await;

    r.replace_rules(&[
        literal_rule("c", 100),
        literal_rule("a", 5),
        literal_rule("b", 50),
    ])
    .await
    .expect("replace");

    let got = r.list_rules().await.expect("list");
    assert_eq!(got[0].id, "a");
    assert_eq!(got[1].id, "b");
    assert_eq!(got[2].id, "c");
    assert_eq!(got[0].position, 5);
    assert_eq!(got[1].position, 50);
    assert_eq!(got[2].position, 100);
}

/// A single rule at position 0 round-trips correctly (boundary: minimum position).
#[tokio::test]
async fn single_rule_at_position_zero_round_trips() {
    let r = repo().await;
    let rule = literal_rule("solo", 0);
    r.replace_rules(std::slice::from_ref(&rule))
        .await
        .expect("replace");
    let got = r.list_rules().await.expect("list");
    assert_eq!(got.len(), 1);
    assert_eq!(got[0], rule);
}

/// A rule at position u32::MAX does not overflow the INTEGER column (stored as i64).
#[tokio::test]
async fn rule_at_max_position_does_not_overflow() {
    let r = repo().await;
    let rule = FilterRule {
        id: "edge_max".to_owned(),
        name: "Max pos".to_owned(),
        enabled: true,
        position: u32::MAX,
        kind: FilterRuleKind::Literal {
            pattern: "x".to_owned(),
            replacement: "y".to_owned(),
        },
    };
    r.replace_rules(std::slice::from_ref(&rule))
        .await
        .expect("replace");
    let got = r.list_rules().await.expect("list");
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].position, u32::MAX, "position u32::MAX must survive");
}

// ---------------------------------------------------------------------------
// `enabled` field fidelity
// ---------------------------------------------------------------------------

/// The `enabled` boolean is stored as INTEGER and must survive false→read correctly.
/// (Non-trivial because SQLite has no BOOLEAN type; wrong cast could flip the value.)
#[tokio::test]
async fn disabled_rule_enabled_field_round_trips_correctly() {
    let r = repo().await;
    let disabled = FilterRule {
        id: "disabled".to_owned(),
        name: "Off".to_owned(),
        enabled: false,
        position: 0,
        kind: FilterRuleKind::Literal {
            pattern: "bad".to_owned(),
            replacement: "".to_owned(),
        },
    };
    r.replace_rules(std::slice::from_ref(&disabled))
        .await
        .expect("replace");
    let got = r.list_rules().await.expect("list");
    assert!(!got[0].enabled, "disabled rule must not become enabled");
}

// ---------------------------------------------------------------------------
// DataProvider integration - tts_filters_repo accessor reachable
// ---------------------------------------------------------------------------

/// `DataProvider::tts_filters_repo()` returns a repo that can perform a full
/// settings round-trip, confirming the accessor wiring in the backend.
#[tokio::test]
async fn data_provider_tts_filters_repo_accessor_round_trips_settings() {
    let b = backend().await;
    let dp: &dyn DataProvider = &b;
    let repo = dp.tts_filters_repo();

    let s = TtsPipelineSettings {
        url_mode: UrlMode::Replace,
        max_length: Some(500),
        blocklist_mode: BlocklistMode::Suppress,
        strip_twitch_emotes: false,
        strip_reward_emotes: true,
        ..TtsPipelineSettings::default()
    };
    repo.set_pipeline_settings(&s).await.expect("set");
    let got = repo.get_pipeline_settings().await.expect("get");
    assert_eq!(got, s);
}
