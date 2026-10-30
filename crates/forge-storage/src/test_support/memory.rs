use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use async_trait::async_trait;
use tokio::sync::{Semaphore, watch};

use super::{FAIL_CLOSED_CLASSES, INJECTED_FAILURE_REASON, SettingsWrite, StoredSettingRow};
use crate::{
    RowClasses, RowGate, Setting, SettingCodec, SettingFamily, SettingRow, SettingsRepo,
    StorageError,
};

#[derive(Clone)]
pub struct MemorySettings {
    shared: Arc<Shared>,
}

struct Shared {
    rows: Mutex<BTreeMap<String, StoredSettingRow>>,
    writes: Mutex<Vec<SettingsWrite>>,
    fail_reads: AtomicBool,
    failing_keys: Mutex<BTreeSet<String>>,
    fail_writes: AtomicBool,
    writes_open: watch::Sender<bool>,
    reads_open: watch::Sender<bool>,
    writes_in_flight: AtomicUsize,
    writes_peak: AtomicUsize,
    reads_started: Semaphore,
}

struct InFlightWrite<'a>(&'a AtomicUsize);

impl<'a> InFlightWrite<'a> {
    fn enter(shared: &'a Shared) -> Self {
        let now = shared.writes_in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        shared.writes_peak.fetch_max(now, Ordering::SeqCst);
        Self(&shared.writes_in_flight)
    }
}

impl Drop for InFlightWrite<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn injected_failure() -> StorageError {
    StorageError::Connection {
        reason: INJECTED_FAILURE_REASON.to_owned(),
    }
}

async fn wait_until_open(gate: &watch::Sender<bool>) {
    let mut receiver = gate.subscribe();
    if receiver.wait_for(|open| *open).await.is_err() {
        tracing::warn!("settings gate closed while waiting");
    }
}

impl Default for MemorySettings {
    fn default() -> Self {
        Self::new()
    }
}

impl MemorySettings {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Shared {
                rows: Mutex::new(BTreeMap::new()),
                writes: Mutex::new(Vec::new()),
                fail_reads: AtomicBool::new(false),
                failing_keys: Mutex::new(BTreeSet::new()),
                fail_writes: AtomicBool::new(false),
                writes_open: watch::channel(true).0,
                reads_open: watch::channel(true).0,
                writes_in_flight: AtomicUsize::new(0),
                writes_peak: AtomicUsize::new(0),
                reads_started: Semaphore::new(0),
            }),
        }
    }

    pub fn seed<C: SettingCodec>(
        &self,
        setting: &Setting<C>,
        value: &C::Value,
    ) -> Result<(), StorageError> {
        if setting.codec().is_absent(value) {
            locked(&self.shared.rows).remove(setting.name());
            return Ok(());
        }
        let encoded =
            setting
                .codec()
                .encode(value)
                .ok_or_else(|| StorageError::UnstorableSetting {
                    key: setting.name().to_owned(),
                })?;
        self.seed_foreign(
            setting.name(),
            &encoded,
            Some(RowClasses::from(&setting.spec())),
        );
        Ok(())
    }

    pub fn seed_family<C: SettingCodec>(
        &self,
        family: &SettingFamily<C>,
        member: &str,
        value: &C::Value,
    ) -> Result<(), StorageError> {
        let key = family.member_key(member)?;
        let encoded = family
            .codec()
            .encode(value)
            .ok_or_else(|| StorageError::UnstorableSetting { key: key.clone() })?;
        self.seed_foreign(&key, &encoded, Some(RowClasses::from(&family.spec())));
        Ok(())
    }

    pub fn seed_raw<C: SettingCodec>(&self, setting: &Setting<C>, raw: &str) {
        self.seed_foreign(setting.name(), raw, Some(RowClasses::from(&setting.spec())));
    }

    pub fn seed_raw_family<C: SettingCodec>(
        &self,
        family: &SettingFamily<C>,
        member: &str,
        raw: &str,
    ) -> Result<(), StorageError> {
        let key = family.member_key(member)?;
        self.seed_foreign(&key, raw, Some(RowClasses::from(&family.spec())));
        Ok(())
    }

    pub fn seed_foreign(&self, key: &str, value: &str, classes: Option<RowClasses>) {
        let mut rows = locked(&self.shared.rows);
        let classes = classes
            .or_else(|| rows.get(key).map(|row| row.classes))
            .unwrap_or(FAIL_CLOSED_CLASSES);
        rows.insert(
            key.to_owned(),
            StoredSettingRow {
                key: key.to_owned(),
                value: value.to_owned(),
                classes,
            },
        );
    }

    pub fn rows(&self) -> Vec<StoredSettingRow> {
        locked(&self.shared.rows).values().cloned().collect()
    }

    pub fn row(&self, key: &str) -> Option<StoredSettingRow> {
        locked(&self.shared.rows).get(key).cloned()
    }

    pub fn write_log(&self) -> Vec<SettingsWrite> {
        locked(&self.shared.writes).clone()
    }

    pub fn peak_concurrent_writes(&self) -> usize {
        self.shared.writes_peak.load(Ordering::SeqCst)
    }

    pub fn fail_reads(&self, fail: bool) {
        self.shared.fail_reads.store(fail, Ordering::SeqCst);
    }

    pub fn fail_reads_for(&self, key: &str) {
        locked(&self.shared.failing_keys).insert(key.to_owned());
    }

    pub fn fail_writes(&self, fail: bool) {
        self.shared.fail_writes.store(fail, Ordering::SeqCst);
    }

    pub fn hold_writes(&self) {
        self.shared.writes_open.send_replace(false);
    }

    pub fn release_writes(&self) {
        self.shared.writes_open.send_replace(true);
    }

    pub fn hold_reads(&self) {
        self.shared.reads_open.send_replace(false);
    }

    pub fn release_reads(&self) {
        self.shared.reads_open.send_replace(true);
    }

    pub async fn wait_for_read_start(&self) {
        if let Ok(permit) = self.shared.reads_started.acquire().await {
            permit.forget();
        }
    }

    async fn begin_read(&self) -> Result<(), StorageError> {
        self.shared.reads_started.add_permits(1);
        wait_until_open(&self.shared.reads_open).await;
        if self.shared.fail_reads.load(Ordering::SeqCst) {
            return Err(injected_failure());
        }
        Ok(())
    }

    async fn write(&self, write: SettingsWrite) -> Result<bool, StorageError> {
        let _in_flight = InFlightWrite::enter(&self.shared);
        wait_until_open(&self.shared.writes_open).await;
        if self.shared.fail_writes.load(Ordering::SeqCst) {
            return Err(injected_failure());
        }
        let mut rows = locked(&self.shared.rows);
        let changed = match &write {
            SettingsWrite::Put {
                key,
                value,
                classes,
            } => {
                rows.insert(
                    key.clone(),
                    StoredSettingRow {
                        key: key.clone(),
                        value: value.clone(),
                        classes: *classes,
                    },
                );
                true
            }
            SettingsWrite::Delete { key } => rows.remove(key).is_some(),
        };
        locked(&self.shared.writes).push(write);
        Ok(changed)
    }
}

#[async_trait]
impl SettingsRepo for MemorySettings {
    async fn get_raw(&self, _gate: RowGate, key: &str) -> Result<Option<String>, StorageError> {
        self.begin_read().await?;
        if locked(&self.shared.failing_keys).contains(key) {
            return Err(injected_failure());
        }
        Ok(locked(&self.shared.rows)
            .get(key)
            .map(|row| row.value.clone()))
    }

    async fn delete_raw(&self, _gate: RowGate, key: &str) -> Result<bool, StorageError> {
        self.write(SettingsWrite::Delete {
            key: key.to_owned(),
        })
        .await
    }

    async fn load_rows(&self, gate: RowGate) -> Result<Vec<SettingRow>, StorageError> {
        self.begin_read().await?;
        Ok(locked(&self.shared.rows)
            .values()
            .map(|row| {
                SettingRow::from_storage(gate, row.key.clone(), row.value.clone(), row.classes)
            })
            .collect())
    }

    async fn put_row(
        &self,
        _gate: RowGate,
        key: &str,
        value: &str,
        classes: RowClasses,
    ) -> Result<(), StorageError> {
        self.write(SettingsWrite::Put {
            key: key.to_owned(),
            value: value.to_owned(),
            classes,
        })
        .await
        .map(|_| ())
    }
}
