use std::fs::OpenOptions;
use std::path::Path;

use crate::UpdateError;
use crate::atomic_file::sync_dir;
use crate::record::remove_record;

const RESTORE_SUFFIX: &str = ".restore";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseUnit {
    main: String,
    sidecars: Vec<String>,
}

impl DatabaseUnit {
    pub fn from_file_names(names: impl IntoIterator<Item = String>) -> Option<Self> {
        let mut names = names.into_iter();
        let main = names.next()?;
        Some(Self {
            main,
            sidecars: names.collect(),
        })
    }

    pub fn main(&self) -> &str {
        &self.main
    }

    pub fn sidecars(&self) -> &[String] {
        &self.sidecars
    }
}

pub trait SchemaProbe {
    fn current_schema_version(&self, data_dir: &Path) -> Option<u32>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UnknownSchemaVersion;

impl SchemaProbe for UnknownSchemaVersion {
    fn current_schema_version(&self, _data_dir: &Path) -> Option<u32> {
        None
    }
}

pub(crate) fn restore_database(
    data_dir: &Path,
    unit: &DatabaseUnit,
    snapshot: &Path,
) -> Result<(), UpdateError> {
    let main = data_dir.join(&unit.main);
    let staged = data_dir.join(format!("{}{RESTORE_SUFFIX}", unit.main));
    let copied = std::fs::copy(snapshot, &staged)
        .and_then(|_| OpenOptions::new().write(true).open(&staged))
        .and_then(|file| file.sync_all());
    if let Err(error) = copied {
        let _ = std::fs::remove_file(&staged);
        return Err(error.into());
    }
    for sidecar in &unit.sidecars {
        remove_record(&data_dir.join(sidecar))?;
    }
    std::fs::rename(&staged, &main)?;
    sync_dir(data_dir)?;
    Ok(())
}
