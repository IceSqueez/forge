#![allow(clippy::expect_used)]
use async_trait::async_trait;
use forge_storage::settings::core_settings::{INTEGRATION_ENABLED, SERVER_OVERLAY_ROOT, THEME};
use forge_storage::test_support::{FAIL_CLOSED_CLASSES, MemorySettings, StoredSettingRow};
use forge_storage::{
    RowClasses, SettingDisclosure, SettingPortability, SettingsRepo, write_family_setting,
    write_setting,
};
use forge_storage_sqlite::SqliteBackend;

mod common;
use common::{Sandboxed, TEST_KEY};

const PORTABLE_KIND: RowClasses = RowClasses {
    portability: SettingPortability::Portable,
    disclosure: SettingDisclosure::Kind,
};
const OVERLAY_CLASSES: RowClasses = RowClasses {
    portability: SettingPortability::MachineLocal,
    disclosure: SettingDisclosure::Presence,
};
const THEME_CLASSES: RowClasses = RowClasses {
    portability: SettingPortability::Portable,
    disclosure: SettingDisclosure::Verbatim,
};
const FOREIGN_CLASSES: RowClasses = RowClasses {
    portability: SettingPortability::RepositoryOwned,
    disclosure: SettingDisclosure::Kind,
};

#[async_trait]
trait Store {
    fn repo(&self) -> &dyn SettingsRepo;
    async fn seed(&self, key: &str, value: &str, classes: Option<RowClasses>);
    async fn row(&self, key: &str) -> Option<StoredSettingRow>;
}

#[async_trait]
impl Store for MemorySettings {
    fn repo(&self) -> &dyn SettingsRepo {
        self
    }

    async fn seed(&self, key: &str, value: &str, classes: Option<RowClasses>) {
        self.seed_foreign(key, value, classes);
    }

    async fn row(&self, key: &str) -> Option<StoredSettingRow> {
        MemorySettings::row(self, key)
    }
}

#[async_trait]
impl Store for Sandboxed<SqliteBackend> {
    fn repo(&self) -> &dyn SettingsRepo {
        &**self
    }

    async fn seed(&self, key: &str, value: &str, classes: Option<RowClasses>) {
        self.seed_foreign_setting(key, value, classes)
            .await
            .expect("seed a foreign row");
    }

    async fn row(&self, key: &str) -> Option<StoredSettingRow> {
        self.stored_setting_rows()
            .await
            .expect("stored rows")
            .into_iter()
            .find(|row| row.key == key)
    }
}

async fn both_stores() -> (MemorySettings, Sandboxed<SqliteBackend>) {
    let sqlite = common::sandboxed_backend(":memory:", TEST_KEY).await;
    (MemorySettings::new(), sqlite)
}

fn classes_of(row: Option<StoredSettingRow>) -> Option<RowClasses> {
    row.map(|row| row.classes)
}

async fn a_new_raw_row_is_fail_closed(store: &dyn Store) {
    store
        .repo()
        .set_string("raw.fresh", "1")
        .await
        .expect("raw write");

    let row = store.row("raw.fresh").await.expect("the row");
    assert_eq!(
        (row.value.as_str(), row.classes),
        ("1", FAIL_CLOSED_CLASSES)
    );
}

async fn a_raw_write_keeps_the_classes_of_an_existing_row(store: &dyn Store) {
    store.seed("raw.known", "old", Some(PORTABLE_KIND)).await;

    store
        .repo()
        .set_string("raw.known", "new")
        .await
        .expect("raw write");

    let row = store.row("raw.known").await.expect("the row");
    assert_eq!((row.value.as_str(), row.classes), ("new", PORTABLE_KIND));
}

async fn a_typed_write_restamps_a_fail_closed_row_with_the_declared_classes(store: &dyn Store) {
    store.seed(SERVER_OVERLAY_ROOT.name(), "/old", None).await;
    assert_eq!(
        classes_of(store.row(SERVER_OVERLAY_ROOT.name()).await),
        Some(FAIL_CLOSED_CLASSES)
    );

    write_setting(store.repo(), &SERVER_OVERLAY_ROOT, &Some("/new".to_owned()))
        .await
        .expect("typed write");

    let row = store
        .row(SERVER_OVERLAY_ROOT.name())
        .await
        .expect("the row");
    assert_eq!((row.value.as_str(), row.classes), ("/new", OVERLAY_CLASSES));
}

async fn a_typed_write_overrides_foreign_non_default_classes(store: &dyn Store) {
    store.seed(THEME.name(), "old", Some(FOREIGN_CLASSES)).await;

    write_setting(store.repo(), &THEME, &Some("mocha".to_owned()))
        .await
        .expect("typed write");

    assert_eq!(classes_of(store.row("theme").await), Some(THEME_CLASSES));
}

async fn a_family_write_stamps_the_family_classes_on_a_new_member(store: &dyn Store) {
    write_family_setting(store.repo(), &INTEGRATION_ENABLED, "twitch", &true)
        .await
        .expect("family write");

    let row = store
        .row("integration.enabled.twitch")
        .await
        .expect("the row");
    assert_eq!((row.value.as_str(), row.classes), ("true", THEME_CLASSES));
}

async fn optional_text_none_removes_the_row(store: &dyn Store) {
    write_setting(store.repo(), &THEME, &Some("mocha".to_owned()))
        .await
        .expect("typed write");
    write_setting(store.repo(), &THEME, &None)
        .await
        .expect("typed clear");

    assert_eq!(store.row("theme").await, None);
}

async fn seeding_without_classes_keeps_an_existing_rows_classes(store: &dyn Store) {
    store.seed("seed.twice", "1", Some(PORTABLE_KIND)).await;
    store.seed("seed.twice", "2", None).await;

    let row = store.row("seed.twice").await.expect("the row");
    assert_eq!((row.value.as_str(), row.classes), ("2", PORTABLE_KIND));
}

macro_rules! on_both_stores {
    ($name:ident, $scenario:ident) => {
        #[tokio::test]
        async fn $name() {
            let (memory, sqlite) = both_stores().await;
            $scenario(&memory).await;
            $scenario(&sqlite).await;
        }
    };
}

on_both_stores!(
    a_new_raw_row_is_stored_fail_closed_on_both_stores,
    a_new_raw_row_is_fail_closed
);
on_both_stores!(
    a_raw_write_keeps_an_existing_rows_classes_on_both_stores,
    a_raw_write_keeps_the_classes_of_an_existing_row
);
on_both_stores!(
    a_typed_write_restamps_a_fail_closed_row_on_both_stores,
    a_typed_write_restamps_a_fail_closed_row_with_the_declared_classes
);
on_both_stores!(
    a_typed_write_overrides_foreign_classes_on_both_stores,
    a_typed_write_overrides_foreign_non_default_classes
);
on_both_stores!(
    a_family_write_stamps_family_classes_on_both_stores,
    a_family_write_stamps_the_family_classes_on_a_new_member
);
on_both_stores!(
    optional_text_none_removes_the_row_on_both_stores,
    optional_text_none_removes_the_row
);
on_both_stores!(
    seeding_without_classes_keeps_existing_classes_on_both_stores,
    seeding_without_classes_keeps_an_existing_rows_classes
);

#[tokio::test]
async fn seeding_a_legacy_row_on_sqlite_stores_the_column_defaults() {
    let sqlite = common::sandboxed_backend(":memory:", TEST_KEY).await;
    sqlite
        .seed_foreign_setting("legacy.row", "v", None)
        .await
        .expect("seed");

    let rows = sqlite.stored_setting_rows().await.expect("rows");

    let legacy = rows.iter().find(|row| row.key == "legacy.row");
    assert_eq!(legacy.map(|row| row.classes), Some(FAIL_CLOSED_CLASSES));
}

#[tokio::test]
async fn stored_setting_rows_are_ordered_by_key() {
    let sqlite = common::sandboxed_backend(":memory:", TEST_KEY).await;
    for key in ["zz.last", "aa.first", "mm.middle"] {
        sqlite
            .seed_foreign_setting(key, "v", None)
            .await
            .expect("seed");
    }

    let keys: Vec<String> = sqlite
        .stored_setting_rows()
        .await
        .expect("rows")
        .into_iter()
        .map(|row| row.key)
        .filter(|key| key.ends_with(".first") || key.ends_with(".middle") || key.ends_with(".last"))
        .collect();

    assert_eq!(keys, ["aa.first", "mm.middle", "zz.last"]);
}
