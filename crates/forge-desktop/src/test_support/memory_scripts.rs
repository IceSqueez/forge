use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use forge_storage::{
    ExecutionStatus, ScriptRecord, ScriptReferences, ScriptRepo, ScriptTelemetry, StorageError,
};
use forge_types::ScriptId;
use time::OffsetDateTime;

#[derive(Default)]
struct Shelves {
    live: BTreeMap<ScriptId, ScriptRecord>,
    archived: BTreeMap<ScriptId, ScriptRecord>,
    referrers: HashMap<String, ScriptReferences>,
}

#[derive(Default)]
pub(crate) struct MemoryScripts {
    shelves: Mutex<Shelves>,
}

impl MemoryScripts {
    pub(crate) fn holding(records: impl IntoIterator<Item = ScriptRecord>) -> Self {
        let scripts = Self::default();
        {
            let mut shelves = scripts.shelves.lock().unwrap();
            for record in records {
                shelves.live.insert(record.id, record);
            }
        }
        scripts
    }

    pub(crate) fn referenced_by(self, name: &str, references: ScriptReferences) -> Self {
        self.shelves
            .lock()
            .unwrap()
            .referrers
            .insert(name.to_owned(), references);
        self
    }

    pub(crate) fn live_names(&self) -> Vec<String> {
        let shelves = self.shelves.lock().unwrap();
        let mut names: Vec<_> = shelves.live.values().map(|s| s.name.clone()).collect();
        names.sort();
        names
    }

    pub(crate) fn archived_ids(&self) -> Vec<ScriptId> {
        self.shelves
            .lock()
            .unwrap()
            .archived
            .keys()
            .copied()
            .collect()
    }

    fn sorted(records: impl Iterator<Item = ScriptRecord>) -> Vec<ScriptRecord> {
        let mut records: Vec<_> = records.collect();
        records.sort_by(|a, b| a.name.cmp(&b.name));
        records
    }
}

#[async_trait::async_trait]
impl ScriptRepo for MemoryScripts {
    async fn get(&self, id: ScriptId) -> Result<Option<ScriptRecord>, StorageError> {
        Ok(self.shelves.lock().unwrap().live.get(&id).cloned())
    }

    async fn get_by_name(&self, name: &str) -> Result<Option<ScriptRecord>, StorageError> {
        Ok(self
            .shelves
            .lock()
            .unwrap()
            .live
            .values()
            .find(|s| s.name == name)
            .cloned())
    }

    async fn save(&self, record: ScriptRecord) -> Result<(), StorageError> {
        let mut shelves = self.shelves.lock().unwrap();
        if let Some(shelved) = shelves.archived.get_mut(&record.id) {
            *shelved = record;
        } else {
            shelves.live.insert(record.id, record);
        }
        Ok(())
    }

    async fn delete(&self, id: ScriptId) -> Result<bool, StorageError> {
        let mut shelves = self.shelves.lock().unwrap();
        let live = shelves.live.remove(&id).is_some();
        let archived = shelves.archived.remove(&id).is_some();
        Ok(live || archived)
    }

    async fn list(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        let shelves = self.shelves.lock().unwrap();
        Ok(Self::sorted(shelves.live.values().cloned()))
    }

    async fn list_enabled(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        let shelves = self.shelves.lock().unwrap();
        Ok(Self::sorted(
            shelves.live.values().filter(|s| s.enabled).cloned(),
        ))
    }

    async fn record_execution(
        &self,
        _: ScriptId,
        _: OffsetDateTime,
        _: u64,
        _: ExecutionStatus,
    ) -> Result<(), StorageError> {
        Ok(())
    }

    async fn telemetry(&self, _: ScriptId) -> Result<ScriptTelemetry, StorageError> {
        Ok(ScriptTelemetry::default())
    }

    async fn prune_executions_before(&self, _: OffsetDateTime) -> Result<u64, StorageError> {
        Ok(0)
    }

    async fn rename_cascading(
        &self,
        id: ScriptId,
        new_name: &str,
    ) -> Result<ScriptReferences, StorageError> {
        let mut shelves = self.shelves.lock().unwrap();
        let Some(old_name) = shelves.live.get(&id).map(|s| s.name.clone()) else {
            return Err(StorageError::NotFound {
                key: id.to_string(),
            });
        };
        if old_name == new_name {
            return Ok(ScriptReferences::default());
        }
        let taken = shelves
            .live
            .values()
            .chain(shelves.archived.values())
            .any(|s| s.id != id && s.name == new_name);
        if taken {
            return Err(StorageError::NameCollision {
                name: new_name.to_owned(),
            });
        }
        if let Some(record) = shelves.live.get_mut(&id) {
            new_name.clone_into(&mut record.name);
        }
        let references = shelves.referrers.remove(&old_name).unwrap_or_default();
        shelves
            .referrers
            .insert(new_name.to_owned(), references.clone());
        Ok(references)
    }

    async fn referrers(&self, name: &str) -> Result<ScriptReferences, StorageError> {
        Ok(self
            .shelves
            .lock()
            .unwrap()
            .referrers
            .get(name)
            .cloned()
            .unwrap_or_default())
    }

    async fn archive_script(&self, id: ScriptId) -> Result<bool, StorageError> {
        let mut shelves = self.shelves.lock().unwrap();
        let Some(record) = shelves.live.remove(&id) else {
            return Ok(false);
        };
        shelves.archived.insert(id, record);
        Ok(true)
    }

    async fn restore_script(&self, id: ScriptId) -> Result<bool, StorageError> {
        let mut shelves = self.shelves.lock().unwrap();
        let Some(record) = shelves.archived.remove(&id) else {
            return Ok(false);
        };
        shelves.live.insert(id, record);
        Ok(true)
    }

    async fn list_archived_scripts(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        let shelves = self.shelves.lock().unwrap();
        Ok(Self::sorted(shelves.archived.values().cloned()))
    }
}
