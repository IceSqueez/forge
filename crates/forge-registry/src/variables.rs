use std::sync::Arc;

use forge_events::Event;
use forge_types::{
    ActorRole, ActorSlot, ArgStack, CanonicalCount, CanonicalVariable, DeclaredVariable,
    PlatformId, VariableSchema, VariableStanding, Variant,
};

const CANONICAL_CLASS: u8 = 0;
const EVENT_SPECIFIC_CLASS: u8 = 1;
const LEGACY_CLASS: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginSlot {
    Declared,
    PlatformHasNone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActorBlock {
    pub role: ActorRole,
    pub platform: PlatformId,
    pub login: LoginSlot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActorIdentity {
    pub id: String,
    pub display_name: String,
    pub login: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorDeclaration {
    Undeclared,
    Actorless,
    Actors(&'static [ActorRole]),
}

impl ActorDeclaration {
    pub const fn principal() -> Self {
        ActorDeclaration::Actors(&[])
    }

    pub fn declares(self, role: ActorRole) -> bool {
        match self {
            ActorDeclaration::Undeclared | ActorDeclaration::Actorless => false,
            ActorDeclaration::Actors(roles) => {
                role == ActorRole::Principal || roles.contains(&role)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TriggerVariable<'a> {
    pub declared: &'a DeclaredVariable,
    pub standing: VariableStanding,
}

type ValueReader = Arc<dyn Fn(&Event) -> Variant + Send + Sync>;

struct VariableEntry {
    declared: DeclaredVariable,
    standing: VariableStanding,
    read: ValueReader,
}

#[derive(Default)]
pub struct TriggerVariables {
    entries: Vec<VariableEntry>,
}

impl TriggerVariables {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn actor<F>(mut self, block: ActorBlock, read: F) -> Self
    where
        F: Fn(&Event) -> ActorIdentity + Send + Sync + 'static,
    {
        let read = Arc::new(read);
        for slot in ActorSlot::ALL {
            if slot == ActorSlot::Login && block.login == LoginSlot::PlatformHasNone {
                continue;
            }
            let canonical = CanonicalVariable::actor(block.role, slot);
            let identity = Arc::clone(&read);
            let platform = block.platform;
            self.entries.push(VariableEntry {
                declared: canonical_declaration(canonical),
                standing: VariableStanding::Canonical(canonical),
                read: Arc::new(move |event| actor_value(slot, platform, &identity(event))),
            });
        }
        self
    }

    pub fn message_text<F>(self, read: F) -> Self
    where
        F: Fn(&Event) -> String + Send + Sync + 'static,
    {
        self.canonical(CanonicalVariable::MessageText, move |event| {
            Variant::String(read(event))
        })
    }

    pub fn count<F>(self, count: CanonicalCount, read: F) -> Self
    where
        F: Fn(&Event) -> i64 + Send + Sync + 'static,
    {
        self.canonical(CanonicalVariable::Count(count), move |event| {
            Variant::Int(read(event))
        })
    }

    pub fn sub_tier<F>(self, read: F) -> Self
    where
        F: Fn(&Event) -> String + Send + Sync + 'static,
    {
        self.canonical(CanonicalVariable::SubTier, move |event| {
            Variant::String(read(event))
        })
    }

    pub fn event_specific<F>(mut self, declared: DeclaredVariable, read: F) -> Self
    where
        F: Fn(&Event) -> Variant + Send + Sync + 'static,
    {
        self.entries.push(VariableEntry {
            declared,
            standing: VariableStanding::EventSpecific,
            read: Arc::new(read),
        });
        self
    }

    pub fn legacy<F>(
        mut self,
        declared: DeclaredVariable,
        superseded_by: CanonicalVariable,
        read: F,
    ) -> Self
    where
        F: Fn(&Event) -> Variant + Send + Sync + 'static,
    {
        self.entries.push(VariableEntry {
            declared,
            standing: VariableStanding::Legacy(superseded_by),
            read: Arc::new(read),
        });
        self
    }

    pub fn declarations(&self) -> Vec<TriggerVariable<'_>> {
        let mut ordered: Vec<&VariableEntry> = self.entries.iter().collect();
        ordered.sort_by_key(|entry| listing_rank(entry.standing));
        ordered
            .into_iter()
            .map(|entry| TriggerVariable {
                declared: &entry.declared,
                standing: entry.standing,
            })
            .collect()
    }

    pub fn schema(&self) -> VariableSchema {
        VariableSchema {
            variables: self
                .declarations()
                .into_iter()
                .map(|variable| variable.declared.clone())
                .collect(),
        }
    }

    pub fn arg_stack(&self, event: &Event) -> ArgStack {
        self.entries.iter().fold(ArgStack::new(), |stack, entry| {
            stack.set(entry.declared.name.clone(), (entry.read)(event))
        })
    }

    fn canonical<F>(mut self, canonical: CanonicalVariable, read: F) -> Self
    where
        F: Fn(&Event) -> Variant + Send + Sync + 'static,
    {
        self.entries.push(VariableEntry {
            declared: canonical_declaration(canonical),
            standing: VariableStanding::Canonical(canonical),
            read: Arc::new(read),
        });
        self
    }
}

fn canonical_declaration(canonical: CanonicalVariable) -> DeclaredVariable {
    DeclaredVariable {
        name: canonical.name().to_owned(),
        kind: canonical.kind(),
        label: canonical.label(),
        synthesis: canonical.synthesis(),
    }
}

fn actor_value(slot: ActorSlot, platform: PlatformId, identity: &ActorIdentity) -> Variant {
    Variant::String(match slot {
        ActorSlot::Id => identity.id.clone(),
        ActorSlot::Name => shown_name(identity),
        ActorSlot::Login => identity.login.clone().unwrap_or_default(),
        ActorSlot::Platform => platform.as_str().to_owned(),
    })
}

fn shown_name(identity: &ActorIdentity) -> String {
    if identity.display_name.is_empty() {
        identity.login.clone().unwrap_or_default()
    } else {
        identity.display_name.clone()
    }
}

const fn listing_rank(standing: VariableStanding) -> (u8, u16) {
    match standing {
        VariableStanding::Canonical(canonical) => (CANONICAL_CLASS, canonical.order()),
        VariableStanding::EventSpecific => (EVENT_SPECIFIC_CLASS, 0),
        VariableStanding::Legacy(_) => (LEGACY_CLASS, 0),
    }
}
