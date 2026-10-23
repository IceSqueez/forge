use std::sync::Arc;

use forge_platform_core::{CollectionRevisionSignal, CollectionRevisions};
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
pub(crate) struct CatalogSignal {
    lists: Arc<watch::Sender<u64>>,
    options: Arc<CollectionRevisionSignal>,
}

impl Default for CatalogSignal {
    fn default() -> Self {
        Self {
            lists: Arc::new(watch::channel(0).0),
            options: Arc::new(CollectionRevisionSignal::new()),
        }
    }
}

impl CatalogSignal {
    pub(crate) fn bump(&self) {
        self.lists.send_modify(|revision| *revision += 1);
        self.options.bump();
    }

    pub(crate) fn bump_options(&self) {
        self.options.bump();
    }

    fn subscribe(&self) -> watch::Receiver<u64> {
        self.lists.subscribe()
    }

    pub(crate) fn option_revisions(&self) -> CollectionRevisions {
        self.options.subscribe()
    }
}

pub(crate) fn choice(value: &str, label: &str) -> Option<VTubeChoice> {
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
        VTubeCatalogChanges(self.catalog_signal().subscribe())
    }

    pub(crate) fn catalog_signal(&self) -> CatalogSignal {
        self.content_state
            .read()
            .map(|snapshot| snapshot.signal.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().signal.clone())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::content::{ExpressionItem, HotkeyItem, ModelItem};

    fn choice(value: &str, label: &str) -> VTubeChoice {
        VTubeChoice {
            value: value.to_owned(),
            label: label.to_owned(),
        }
    }

    #[test]
    fn catalog_labels_fall_back_to_the_value_and_rows_without_a_value_are_dropped() {
        let client = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        {
            let mut snapshot = client.content_state.write().unwrap();
            snapshot.models = vec![
                ModelItem {
                    id: "m-1".to_owned(),
                    name: "Avatar".to_owned(),
                },
                ModelItem {
                    id: String::new(),
                    name: "Ghost".to_owned(),
                },
            ];
            snapshot.hotkeys = vec![
                HotkeyItem {
                    id: "h-1".to_owned(),
                    name: String::new(),
                },
                HotkeyItem {
                    id: "h-2".to_owned(),
                    name: "Wave".to_owned(),
                },
            ];
            snapshot.expressions = vec![ExpressionItem {
                name: String::new(),
                file: "wink.exp3.json".to_owned(),
                active: false,
            }];
        }

        assert_eq!(
            client.catalog(),
            VTubeCatalog {
                models: vec![choice("m-1", "Avatar")],
                hotkeys: vec![choice("h-1", "h-1"), choice("h-2", "Wave")],
                expressions: vec![choice("wink.exp3.json", "wink.exp3.json")],
            }
        );
    }
}
