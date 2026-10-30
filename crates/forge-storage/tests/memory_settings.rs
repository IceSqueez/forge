#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::time::Duration;

use forge_storage::settings::core_settings::{
    CREDENTIALS_KEY_LOSS, INTEGRATION_ENABLED, LANGUAGE, THEME, TTS_DISABLED_ENGINES,
    TTS_ENGINE_PARAMS,
};
use forge_storage::test_support::{
    FAIL_CLOSED_CLASSES, INJECTED_FAILURE_REASON, MemorySettings, SettingsWrite,
};
use forge_storage::{
    PlainText, RowClasses, Setting, SettingDisclosure, SettingPortability, SettingsRepo,
    StorageError, read_family_setting, read_setting, reset_family_setting, write_family_setting,
    write_setting,
};
use tokio::task::yield_now;

const HANG_GUARD: Duration = Duration::from_secs(30);
const ABSENCE_PROBE: Duration = Duration::from_millis(20);

fn is_injected(error: &StorageError) -> bool {
    matches!(error, StorageError::Connection { reason } if reason == INJECTED_FAILURE_REASON)
}

async fn until(condition: impl Fn() -> bool) {
    tokio::time::timeout(HANG_GUARD, async {
        while !condition() {
            yield_now().await;
        }
    })
    .await
    .expect("condition never became true");
}

#[test]
fn every_class_code_round_trips_and_unknown_codes_are_rejected() {
    for class in [
        SettingPortability::Portable,
        SettingPortability::MachineLocal,
        SettingPortability::RepositoryOwned,
    ] {
        assert_eq!(SettingPortability::from_code(class.code()), Some(class));
    }
    for class in [
        SettingDisclosure::Verbatim,
        SettingDisclosure::Kind,
        SettingDisclosure::Presence,
        SettingDisclosure::Withheld,
    ] {
        assert_eq!(SettingDisclosure::from_code(class.code()), Some(class));
    }
    for bad in [
        "",
        "Portable",
        "PORTABLE",
        " portable",
        "machine-local",
        "x",
    ] {
        assert_eq!(SettingPortability::from_code(bad), None, "{bad:?}");
    }
    for bad in ["", "Verbatim", " kind", "hidden", "withheld "] {
        assert_eq!(SettingDisclosure::from_code(bad), None, "{bad:?}");
    }
}

#[test]
fn member_key_rejects_empty_whitespace_and_control_members() {
    for bad in [
        "", " ", "a b", " a", "a ", "a\tb", "a\nb", "a\u{0}b", "a\u{7f}b",
    ] {
        let error = INTEGRATION_ENABLED.member_key(bad).unwrap_err();
        assert!(
            matches!(
                &error,
                StorageError::InvalidSettingMember { family, member }
                    if family == "integration.enabled." && member == bad
            ),
            "{bad:?} produced {error:?}"
        );
    }
}

#[test]
fn member_key_accepts_unicode_and_punctuated_members() {
    for good in ["twitch", "ключ", "a.b-c_d", "x:y"] {
        assert_eq!(
            INTEGRATION_ENABLED.member_key(good).unwrap(),
            format!("integration.enabled.{good}")
        );
    }
}

#[tokio::test]
async fn invalid_family_member_fails_every_family_operation_and_writes_nothing() {
    let store = MemorySettings::new();
    let write = write_family_setting(&store, &INTEGRATION_ENABLED, "a b", &true).await;
    let read = read_family_setting(&store, &INTEGRATION_ENABLED, "").await;
    let reset = reset_family_setting(&store, &INTEGRATION_ENABLED, "a\tb").await;
    let seed = store.seed_family(&INTEGRATION_ENABLED, "", &true);
    assert!(matches!(
        (write, read, reset, seed),
        (
            Err(StorageError::InvalidSettingMember { .. }),
            Err(StorageError::InvalidSettingMember { .. }),
            Err(StorageError::InvalidSettingMember { .. }),
            Err(StorageError::InvalidSettingMember { .. }),
        )
    ));
    assert!(store.rows().is_empty() && store.write_log().is_empty());
}

#[tokio::test]
async fn optional_text_none_deletes_the_stored_row() {
    let store = MemorySettings::new();
    write_setting(&store, &THEME, &Some("mocha".to_owned()))
        .await
        .unwrap();
    assert_eq!(store.row("theme").unwrap().value, "mocha");

    write_setting(&store, &THEME, &None).await.unwrap();

    assert_eq!(store.row("theme"), None);
    assert!(matches!(
        store.write_log().last(),
        Some(SettingsWrite::Delete { key }) if key == "theme"
    ));
    assert_eq!(read_setting(&store, &THEME).await.unwrap(), None);
}

#[tokio::test]
async fn choice_rejects_unknown_text_and_leaves_the_stored_row_untouched() {
    let store = MemorySettings::new();
    write_setting(&store, &LANGUAGE, &"uk".to_owned())
        .await
        .unwrap();

    for bad in ["de", "UK", "", "uk "] {
        let error = write_setting(&store, &LANGUAGE, &bad.to_owned())
            .await
            .unwrap_err();
        assert!(
            matches!(&error, StorageError::UnstorableSetting { key } if key == LANGUAGE.name()),
            "{bad:?} produced {error:?}"
        );
    }

    assert_eq!(store.row(LANGUAGE.name()).unwrap().value, "uk");
    assert_eq!(store.write_log().len(), 1);
}

#[tokio::test]
async fn choice_read_of_an_unlisted_stored_value_falls_back_to_the_default() {
    let store = MemorySettings::new();
    store.seed_foreign(LANGUAGE.name(), "de", None);
    assert_eq!(read_setting(&store, &LANGUAGE).await.unwrap(), "en");
}

#[tokio::test]
async fn seeding_an_absent_or_unstorable_value_follows_the_typed_write_rules() {
    let store = MemorySettings::new();
    store.seed(&THEME, &Some("mocha".to_owned())).unwrap();
    store.seed(&THEME, &None).unwrap();
    let unstorable = store.seed(&LANGUAGE, &"de".to_owned());

    assert_eq!(store.row("theme"), None);
    assert!(matches!(
        unstorable,
        Err(StorageError::UnstorableSetting { .. })
    ));
    assert!(store.write_log().is_empty());
}

#[tokio::test]
async fn seeding_bypasses_faults_and_the_write_log_and_stamps_declared_classes() {
    let store = MemorySettings::new();
    store.fail_writes(true);
    store
        .seed_family(&INTEGRATION_ENABLED, "twitch", &true)
        .unwrap();
    store
        .seed_raw_family(&TTS_ENGINE_PARAMS, "piper", "{not json")
        .unwrap();

    let expected = RowClasses {
        portability: SettingPortability::Portable,
        disclosure: SettingDisclosure::Verbatim,
    };
    assert_eq!(
        (
            store.row("integration.enabled.twitch").unwrap().classes,
            store.row("tts.engine_params.piper").unwrap().value,
            store.row("tts.engine_params.piper").unwrap().classes,
            store.write_log().len(),
        ),
        (expected, "{not json".to_owned(), expected, 0)
    );
}

#[tokio::test]
async fn fail_reads_fails_every_read_but_not_writes_and_recovers_when_cleared() {
    let store = MemorySettings::new();
    store.seed_foreign("k", "v", None);
    store.fail_reads(true);

    let get = store.get_string("k").await.unwrap_err();
    let all = store.load_all().await.unwrap_err();
    let write = store.set_string("k2", "v2").await;

    assert!(is_injected(&get) && is_injected(&all) && write.is_ok());

    store.fail_reads(false);
    assert_eq!(store.get_string("k").await.unwrap().as_deref(), Some("v"));
}

#[tokio::test]
async fn fail_writes_rejects_every_write_kind_and_leaves_state_and_log_unchanged() {
    let store = MemorySettings::new();
    store.seed_foreign("keep", "1", None);
    store.fail_writes(true);

    let raw = store.set_string("keep", "2").await.unwrap_err();
    let delete = store.delete("keep").await.unwrap_err();
    let typed = write_setting(&store, &THEME, &Some("x".to_owned()))
        .await
        .unwrap_err();

    assert!(is_injected(&raw) && is_injected(&delete) && is_injected(&typed));
    assert_eq!(store.row("keep").unwrap().value, "1");
    assert!(store.row("theme").is_none());
    assert!(store.write_log().is_empty());

    store.fail_writes(false);
    store.set_string("keep", "3").await.unwrap();
    assert_eq!(store.write_log().len(), 1);
}

#[tokio::test]
async fn write_log_records_successful_writes_in_call_order_with_their_classes() {
    let store = MemorySettings::new();
    store.fail_writes(true);
    store.set_string("dropped", "x").await.unwrap_err();
    store.fail_writes(false);

    store.set_string("a", "1").await.unwrap();
    write_setting(&store, &THEME, &Some("mocha".to_owned()))
        .await
        .unwrap();
    store.delete("a").await.unwrap();

    assert_eq!(
        store.write_log(),
        vec![
            SettingsWrite::Put {
                key: "a".to_owned(),
                value: "1".to_owned(),
                classes: FAIL_CLOSED_CLASSES,
            },
            SettingsWrite::Put {
                key: "theme".to_owned(),
                value: "mocha".to_owned(),
                classes: RowClasses::from(&THEME.spec()),
            },
            SettingsWrite::Delete {
                key: "a".to_owned()
            },
        ]
    );
}

#[tokio::test]
async fn held_writes_stay_invisible_until_released() {
    let store = MemorySettings::new();
    store.hold_writes();
    let writer = tokio::spawn({
        let store = store.clone();
        async move { store.set_string("k", "v").await }
    });
    until(|| store.peak_concurrent_writes() == 1).await;
    yield_now().await;

    assert!(!writer.is_finished() && store.row("k").is_none() && store.write_log().is_empty());

    store.release_writes();
    writer.await.unwrap().unwrap();
    assert_eq!(store.row("k").unwrap().value, "v");
    assert_eq!(store.write_log().len(), 1);
}

#[tokio::test]
async fn peak_concurrent_writes_counts_simultaneously_pending_writes_only() {
    let sequential = MemorySettings::new();
    for key in ["a", "b", "c"] {
        sequential.set_string(key, "x").await.unwrap();
    }

    let overlapping = MemorySettings::new();
    overlapping.hold_writes();
    let writers: Vec<_> = ["a", "b", "c"]
        .into_iter()
        .map(|key| {
            let store = overlapping.clone();
            tokio::spawn(async move { store.set_string(key, "x").await })
        })
        .collect();
    until(|| overlapping.peak_concurrent_writes() == 3).await;
    overlapping.release_writes();
    for writer in writers {
        writer.await.unwrap().unwrap();
    }

    assert_eq!(
        (
            sequential.peak_concurrent_writes(),
            overlapping.peak_concurrent_writes(),
        ),
        (1, 3)
    );
}

#[tokio::test]
async fn failed_writes_do_not_leave_the_in_flight_counter_raised() {
    let store = MemorySettings::new();
    store.fail_writes(true);
    store.set_string("a", "x").await.unwrap_err();
    store.fail_writes(false);
    store.set_string("b", "x").await.unwrap();

    assert_eq!(store.peak_concurrent_writes(), 1);
}

#[tokio::test]
async fn held_reads_signal_their_start_and_complete_only_after_release() {
    let store = MemorySettings::new();
    store.seed_foreign("k", "v", None);
    store.hold_reads();
    let reader = tokio::spawn({
        let store = store.clone();
        async move { store.get_string("k").await }
    });

    tokio::time::timeout(HANG_GUARD, store.wait_for_read_start())
        .await
        .expect("the read never started");
    yield_now().await;
    assert!(!reader.is_finished());

    store.release_reads();
    assert_eq!(reader.await.unwrap().unwrap().as_deref(), Some("v"));
}

#[tokio::test]
async fn wait_for_read_start_stays_pending_when_no_read_was_issued() {
    let store = MemorySettings::new();
    store.seed_foreign("k", "v", None);
    store.set_string("k", "w").await.unwrap();

    let outcome = tokio::time::timeout(ABSENCE_PROBE, store.wait_for_read_start()).await;

    assert!(outcome.is_err());
}

#[tokio::test]
async fn each_read_releases_exactly_one_start_wait() {
    let store = MemorySettings::new();
    store.get_string("a").await.unwrap();
    store.get_string("b").await.unwrap();

    store.wait_for_read_start().await;
    store.wait_for_read_start().await;
    let third = tokio::time::timeout(ABSENCE_PROBE, store.wait_for_read_start()).await;

    assert!(third.is_err());
}

const NOTE: Setting<PlainText> = Setting::new(
    "test.note",
    PlainText { default: "none" },
    SettingPortability::Portable,
    SettingDisclosure::Verbatim,
);

#[tokio::test]
async fn tally_round_trips_the_u64_extremes_and_defaults_on_non_counts() {
    let store = MemorySettings::new();
    for value in [0_u64, 1, u64::MAX] {
        write_setting(&store, &CREDENTIALS_KEY_LOSS, &value)
            .await
            .unwrap();
        assert_eq!(
            read_setting(&store, &CREDENTIALS_KEY_LOSS).await.unwrap(),
            value
        );
    }
    for bad in ["-1", "1.5", "", "18446744073709551616", "abc"] {
        store.seed_foreign(CREDENTIALS_KEY_LOSS.name(), bad, None);
        assert_eq!(
            read_setting(&store, &CREDENTIALS_KEY_LOSS).await.unwrap(),
            0,
            "{bad:?}"
        );
    }
}

#[tokio::test]
async fn plain_text_keeps_empty_and_multiline_values_verbatim_and_empty_is_not_absent() {
    let store = MemorySettings::new();
    for value in ["", " padded ", "line1\r\nline2", "Привіт"] {
        write_setting(&store, &NOTE, &value.to_owned())
            .await
            .unwrap();
        assert_eq!(read_setting(&store, &NOTE).await.unwrap(), value);
        assert!(store.row("test.note").is_some());
    }
}

#[tokio::test]
async fn json_document_falls_back_to_its_default_on_malformed_or_mistyped_json() {
    let store = MemorySettings::new();
    write_setting(&store, &TTS_DISABLED_ENGINES, &vec!["piper".to_owned()])
        .await
        .unwrap();
    assert_eq!(
        read_setting(&store, &TTS_DISABLED_ENGINES).await.unwrap(),
        vec!["piper".to_owned()]
    );
    for bad in ["{oops", "", "{\"a\":1}", "[1,2]"] {
        store.seed_foreign(TTS_DISABLED_ENGINES.name(), bad, None);
        assert!(
            read_setting(&store, &TTS_DISABLED_ENGINES)
                .await
                .unwrap()
                .is_empty(),
            "{bad:?}"
        );
    }
}
