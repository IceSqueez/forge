use std::sync::Arc;

use tokio::sync::watch;

use crate::client::VTubeClient;
use crate::content::ContentSnapshot;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VTubeChoice {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VTubeCatalog {
    pub models: Vec<VTubeChoice>,
    pub hotkeys: Vec<VTubeChoice>,
    pub expressions: Vec<VTubeChoice>,
}

pub struct VTubeCatalogChanges(watch::Receiver<u64>);

impl VTubeCatalogChanges {
    pub async fn changed(&mut self) -> bool {
        self.0.changed().await.is_ok()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CatalogSignal(Arc<watch::Sender<u64>>);

impl Default for CatalogSignal {
    fn default() -> Self {
        Self(Arc::new(watch::channel(0).0))
    }
}

impl CatalogSignal {
    pub(crate) fn bump(&self) {
        self.0.send_modify(|revision| *revision += 1);
    }

    fn subscribe(&self) -> watch::Receiver<u64> {
        self.0.subscribe()
    }
}

fn choice(value: &str, label: &str) -> Option<VTubeChoice> {
    (!value.is_empty()).then(|| VTubeChoice {
        value: value.to_owned(),
        label: if label.is_empty() { value } else { label }.to_owned(),
    })
}

impl ContentSnapshot {
    pub(crate) fn catalog(&self) -> VTubeCatalog {
        VTubeCatalog {
            models: self
                .models
                .iter()
                .filter_map(|m| choice(&m.id, &m.name))
                .collect(),
            hotkeys: self
                .hotkeys
                .iter()
                .filter_map(|h| choice(&h.id, &h.name))
                .collect(),
            expressions: self
                .expressions
                .iter()
                .filter_map(|e| choice(&e.file, &e.name))
                .collect(),
        }
    }
}

impl VTubeClient {
    pub fn catalog(&self) -> VTubeCatalog {
        self.content_state
            .read()
            .map(|snapshot| snapshot.catalog())
            .unwrap_or_default()
    }

    pub fn catalog_changes(&self) -> VTubeCatalogChanges {
        let receiver = self
            .content_state
            .read()
            .map(|snapshot| snapshot.signal.subscribe())
            .unwrap_or_else(|poisoned| poisoned.into_inner().signal.subscribe());
        VTubeCatalogChanges(receiver)
    }
}
