use std::collections::BTreeMap;
use std::fmt;

use forge_types::Variant;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LocationSegment {
    Key(String),
    Index(usize),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct ReferenceLocation(Vec<LocationSegment>);

impl ReferenceLocation {
    pub fn root() -> Self {
        Self::default()
    }

    pub fn segments(&self) -> &[LocationSegment] {
        &self.0
    }

    pub fn key(&self, key: &str) -> Self {
        self.with(LocationSegment::Key(key.to_owned()))
    }

    pub fn index(&self, index: usize) -> Self {
        self.with(LocationSegment::Index(index))
    }

    pub fn within(&self, prefix: &Self) -> Self {
        Self(prefix.0.iter().chain(&self.0).cloned().collect())
    }

    fn with(&self, segment: LocationSegment) -> Self {
        let mut segments = self.0.clone();
        segments.push(segment);
        Self(segments)
    }

    pub(crate) fn slot_mut<'c>(
        &self,
        config: &'c mut BTreeMap<String, Variant>,
    ) -> Option<&'c mut Variant> {
        let (first, rest) = self.0.split_first()?;
        let LocationSegment::Key(first) = first else {
            return None;
        };
        let mut slot = config.get_mut(first)?;
        for segment in rest {
            slot = match (segment, slot) {
                (LocationSegment::Key(key), Variant::Object(map)) => map.get_mut(key)?,
                (LocationSegment::Index(index), Variant::Array(items)) => items.get_mut(*index)?,
                _ => return None,
            };
        }
        Some(slot)
    }
}

impl fmt::Display for ReferenceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (position, segment) in self.0.iter().enumerate() {
            match segment {
                LocationSegment::Key(key) if position == 0 => f.write_str(key)?,
                LocationSegment::Key(key) => write!(f, ".{key}")?,
                LocationSegment::Index(index) => write!(f, "[{index}]")?,
            }
        }
        Ok(())
    }
}
