use std::collections::BTreeMap;
use std::fmt;
use std::marker::PhantomData;

use forge_events::{Event, EventSource};
use serde::de::{self, DeserializeOwned, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ValueMatcher {
    Equals(Value),
    /// Matches only a string value holding this substring.
    Contains(String),
    /// `true` when the pointer resolves, including to a JSON null.
    Present(bool),
}

impl ValueMatcher {
    pub fn matches(&self, resolved: Option<&Value>) -> bool {
        match self {
            Self::Equals(expected) => resolved == Some(expected),
            Self::Contains(needle) => resolved
                .and_then(Value::as_str)
                .is_some_and(|text| text.contains(needle.as_str())),
            Self::Present(present) => resolved.is_some() == *present,
        }
    }
}

/// A JSON object whose repeated key is an error rather than a silent overwrite.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub struct UniqueMap<V>(pub BTreeMap<String, V>);

impl<V> Default for UniqueMap<V> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}

impl<'de, V: DeserializeOwned> Deserialize<'de> for UniqueMap<V> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct UniqueVisitor<V>(PhantomData<V>);

        impl<'de, V: DeserializeOwned> Visitor<'de> for UniqueVisitor<V> {
            type Value = UniqueMap<V>;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an object with distinct keys")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
                let mut entries = BTreeMap::new();
                while let Some(key) = access.next_key::<String>()? {
                    if entries.contains_key(&key) {
                        return Err(de::Error::custom(format!("duplicate key `{key}`")));
                    }
                    let value = access.next_value::<V>()?;
                    entries.insert(key, value);
                }
                Ok(UniqueMap(entries))
            }
        }

        deserializer.deserialize_map(UniqueVisitor(PhantomData))
    }
}

/// Keys are JSON pointers into the event payload (`/user/login`); `""` names the whole payload.
pub type PayloadMatchers = UniqueMap<ValueMatcher>;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EventPattern<'a> {
    pub source: Option<EventSource>,
    pub kind: &'a str,
    pub payload: &'a PayloadMatchers,
}

impl EventPattern<'_> {
    pub fn matches(&self, event: &Event) -> bool {
        self.source.is_none_or(|source| source == event.source)
            && event.kind == self.kind
            && self
                .payload
                .0
                .iter()
                .all(|(pointer, matcher)| matcher.matches(event.payload.pointer(pointer)))
    }

    /// The string an `equals` matcher pins at `pointer`, if any.
    pub fn pinned_string(&self, pointer: &str) -> Option<&str> {
        match self.payload.0.get(pointer) {
            Some(ValueMatcher::Equals(Value::String(text))) => Some(text),
            _ => None,
        }
    }
}
