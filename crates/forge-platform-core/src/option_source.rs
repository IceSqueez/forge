use std::collections::BTreeMap;
use std::fmt;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::builtin::{QuickActionFieldValue, SectionIcon};
use crate::collections::{CollectionOutcome, CollectionRevisions};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OptionListId(pub String);

impl OptionListId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OptionListId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionListMetadata {
    pub id: OptionListId,
    pub depends_on: Vec<String>,
}

impl OptionListMetadata {
    pub fn new(id: OptionListId) -> Self {
        Self {
            id,
            depends_on: Vec::new(),
        }
    }

    pub fn depending_on(mut self, field_key: impl Into<String>) -> Self {
        self.depends_on.push(field_key.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionHint {
    pub key: &'static str,
    pub args: BTreeMap<&'static str, String>,
}

impl OptionHint {
    pub fn new(key: &'static str) -> Self {
        Self {
            key,
            args: BTreeMap::new(),
        }
    }

    pub fn arg(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.args.insert(name, value.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionEntry {
    pub value: String,
    pub label: String,
    pub hint: Option<OptionHint>,
    pub icon: Option<SectionIcon>,
    pub current: bool,
}

impl OptionEntry {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            hint: None,
            icon: None,
            current: false,
        }
    }

    pub fn with_hint(self, hint: OptionHint) -> Self {
        Self {
            hint: Some(hint),
            ..self
        }
    }

    pub fn with_icon(self, icon: SectionIcon) -> Self {
        Self {
            icon: Some(icon),
            ..self
        }
    }

    pub fn as_current(self) -> Self {
        Self {
            current: true,
            ..self
        }
    }
}

#[async_trait]
pub trait BuiltinOptionSource: Send + Sync {
    fn option_lists(&self) -> Vec<OptionListMetadata>;

    fn revisions(&self) -> CollectionRevisions;

    async fn resolve(
        &self,
        list: &OptionListId,
        depends_on_values: &BTreeMap<String, QuickActionFieldValue>,
    ) -> CollectionOutcome<Vec<OptionEntry>>;
}
