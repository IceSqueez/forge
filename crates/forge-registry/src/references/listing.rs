use crate::references::location::ReferenceLocation;
use crate::references::namespace::ReferenceNamespace;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReferenceField {
    pub key: &'static str,
    pub options_key: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigReference {
    pub namespace: ReferenceNamespace,
    pub value: String,
    pub templated: bool,
    pub location: ReferenceLocation,
    pub(crate) tag: &'static str,
}

impl ConfigReference {
    pub(crate) fn within(mut self, prefix: &ReferenceLocation) -> Self {
        self.location = self.location.within(prefix);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownKind {
    pub kind_id: String,
    pub location: ReferenceLocation,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReferenceListing {
    pub references: Vec<ConfigReference>,
    pub unknown_kinds: Vec<UnknownKind>,
}

impl ReferenceListing {
    pub(crate) fn within(self, prefix: &ReferenceLocation) -> Self {
        Self {
            references: self
                .references
                .into_iter()
                .map(|reference| reference.within(prefix))
                .collect(),
            unknown_kinds: self
                .unknown_kinds
                .into_iter()
                .map(|unknown| UnknownKind {
                    location: unknown.location.within(prefix),
                    kind_id: unknown.kind_id,
                })
                .collect(),
        }
    }

    pub(crate) fn extend(&mut self, other: Self) {
        self.references.extend(other.references);
        self.unknown_kinds.extend(other.unknown_kinds);
    }
}
