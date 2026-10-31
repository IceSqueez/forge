use std::sync::Arc;

use forge_platform_core::{CollectionRevisionSignal, CollectionRevisions};
use forge_types::RecoverRwLock;

use crate::client::VTubeClient;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VTubeChoice {
    pub(crate) value: String,
    pub(crate) label: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CatalogSignal(Arc<CollectionRevisionSignal>);

impl CatalogSignal {
    pub(crate) fn bump(&self) {
        self.0.bump();
    }

    pub(crate) fn revisions(&self) -> CollectionRevisions {
        self.0.subscribe()
    }
}

pub(crate) fn choice(value: &str, label: &str) -> Option<VTubeChoice> {
    (!value.is_empty()).then(|| VTubeChoice {
        value: value.to_owned(),
        label: if label.is_empty() { value } else { label }.to_owned(),
    })
}

impl VTubeClient {
    pub(crate) fn catalog_signal(&self) -> CatalogSignal {
        self.content_state.read_recover().signal.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_choice_label_falls_back_to_the_value_and_a_row_without_a_value_is_dropped() {
        let expected = |value: &str, label: &str| VTubeChoice {
            value: value.to_owned(),
            label: label.to_owned(),
        };

        assert_eq!(
            (
                choice("m-1", "Avatar"),
                choice("h-1", ""),
                choice("", "Ghost"),
            ),
            (
                Some(expected("m-1", "Avatar")),
                Some(expected("h-1", "h-1")),
                None,
            )
        );
    }
}
