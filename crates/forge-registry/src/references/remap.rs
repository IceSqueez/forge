use std::collections::HashMap;

use crate::references::namespace::InternalNamespace;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReferenceRemap {
    entries: HashMap<InternalNamespace, HashMap<String, String>>,
}

impl ReferenceRemap {
    pub fn insert(
        &mut self,
        namespace: InternalNamespace,
        from: impl Into<String>,
        to: impl Into<String>,
    ) {
        self.entries
            .entry(namespace)
            .or_default()
            .insert(from.into(), to.into());
    }

    pub fn get(&self, namespace: InternalNamespace, from: &str) -> Option<&str> {
        self.entries.get(&namespace)?.get(from).map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
