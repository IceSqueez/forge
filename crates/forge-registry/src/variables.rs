use std::sync::Arc;

use forge_events::Event;
use forge_types::{ArgStack, DeclaredVariable, MoneyAmount, PlatformId, VariableSchema, Variant};

use crate::descriptor::TriggerKindDescriptor;
use crate::first_chatter::{first_message_declaration, is_first_chat_message};
use crate::message_emotes::{chat_emote_codes, message_emotes_declaration};
use crate::vocabulary::{
    ActorRole, ActorSlot, CanonicalCount, CanonicalVariable, MoneySlot, VariableStanding,
};

const CANONICAL_CLASS: u8 = 0;
const EVENT_SPECIFIC_CLASS: u8 = 1;
const LEGACY_CLASS: u8 = 2;

const LEGACY_CHAT_MESSAGE_ID: &str = "chat.message_id";
const LEGACY_CHAT_MESSAGE_ID_LABEL: &str = "Message ID";

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcedActor {
    pub identity: ActorIdentity,
    pub source: String,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriggerVariable {
    pub declared: DeclaredVariable,
    pub standing: VariableStanding,
}

impl TriggerVariable {
    pub fn canonical(canonical: CanonicalVariable) -> Self {
        TriggerVariable {
            declared: canonical_declaration(canonical),
            standing: VariableStanding::Canonical(canonical),
        }
    }
}

pub fn declared_variables(descriptor: &dyn TriggerKindDescriptor) -> Option<Vec<TriggerVariable>> {
    DescriptorDeclaration::of(descriptor).listed
}

pub(crate) struct DescriptorDeclaration {
    variables: Option<TriggerVariables>,
    listed: Option<Vec<TriggerVariable>>,
}

impl DescriptorDeclaration {
    pub(crate) fn of(descriptor: &dyn TriggerKindDescriptor) -> Self {
        let variables = descriptor.variables();
        let listed = match &variables {
            Some(variables) => Some(variables.declarations()),
            None => descriptor.output_schema().map(event_specific_listing),
        };
        DescriptorDeclaration { variables, listed }
    }

    pub(crate) fn listed(&self) -> Option<&[TriggerVariable]> {
        self.listed.as_deref()
    }

    pub(crate) fn variables(&self) -> Option<&TriggerVariables> {
        self.variables.as_ref()
    }
}

fn event_specific_listing(schema: VariableSchema) -> Vec<TriggerVariable> {
    schema
        .variables
        .into_iter()
        .map(|declared| TriggerVariable {
            declared,
            standing: VariableStanding::EventSpecific,
        })
        .collect()
}

type ValueReader = Arc<dyn Fn(&Event) -> Option<Variant> + Send + Sync>;

struct VariableEntry {
    declared: DeclaredVariable,
    standing: VariableStanding,
    read: ValueReader,
}

struct LegacyEntry {
    declared: DeclaredVariable,
    superseded_by: CanonicalVariable,
}

#[derive(Default)]
pub struct TriggerVariables {
    entries: Vec<VariableEntry>,
    legacy: Vec<LegacyEntry>,
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
                read: Arc::new(move |event| Some(actor_value(slot, platform, &identity(event)))),
            });
        }
        self
    }

    pub fn sourced_actor<F>(mut self, role: ActorRole, login: LoginSlot, read: F) -> Self
    where
        F: Fn(&Event) -> SourcedActor + Send + Sync + 'static,
    {
        let read = Arc::new(read);
        for slot in ActorSlot::ALL {
            if slot == ActorSlot::Login && login == LoginSlot::PlatformHasNone {
                continue;
            }
            let canonical = CanonicalVariable::actor(role, slot);
            let actor = Arc::clone(&read);
            self.entries.push(VariableEntry {
                declared: canonical_declaration(canonical),
                standing: VariableStanding::Canonical(canonical),
                read: Arc::new(move |event| {
                    let actor = actor(event);
                    Some(match slot {
                        ActorSlot::Platform => Variant::String(actor.source),
                        ActorSlot::Id | ActorSlot::Name | ActorSlot::Login => {
                            Variant::String(identity_value(slot, &actor.identity))
                        }
                    })
                }),
            });
        }
        self
    }

    pub fn money<F>(mut self, read: F) -> Self
    where
        F: Fn(&Event) -> Option<MoneyAmount> + Send + Sync + 'static,
    {
        let read = Arc::new(read);
        for slot in MoneySlot::ALL {
            let amount = Arc::clone(&read);
            self = self.canonical(CanonicalVariable::Money(slot), move |event| {
                money_value(slot, amount(event).as_ref())
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

    pub fn chat_message_id<F>(self, read: F) -> Self
    where
        F: Fn(&Event) -> String + Send + Sync + 'static,
    {
        self.canonical(CanonicalVariable::MessageId, move |event| {
            Variant::String(read(event))
        })
        .legacy(
            LEGACY_CHAT_MESSAGE_ID,
            LEGACY_CHAT_MESSAGE_ID_LABEL,
            CanonicalVariable::MessageId,
        )
    }

    pub fn first_chat_message(self) -> Self {
        self.event_specific(first_message_declaration(), |event| {
            Variant::Bool(is_first_chat_message(event))
        })
    }

    pub fn message_emotes(self) -> Self {
        self.event_specific(message_emotes_declaration(), |event| {
            Variant::Array(
                chat_emote_codes(event)
                    .into_iter()
                    .map(Variant::String)
                    .collect(),
            )
        })
    }

    pub fn count<F>(self, count: CanonicalCount, read: F) -> Self
    where
        F: Fn(&Event) -> i64 + Send + Sync + 'static,
    {
        self.canonical(CanonicalVariable::Count(count), move |event| {
            Variant::Int(read(event).max(count.floor()))
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

    pub fn event_specific<F>(self, declared: DeclaredVariable, read: F) -> Self
    where
        F: Fn(&Event) -> Variant + Send + Sync + 'static,
    {
        self.event_specific_optional(declared, move |event| Some(read(event)))
    }

    pub fn event_specific_optional<F>(mut self, declared: DeclaredVariable, read: F) -> Self
    where
        F: Fn(&Event) -> Option<Variant> + Send + Sync + 'static,
    {
        self.entries.push(VariableEntry {
            declared,
            standing: VariableStanding::EventSpecific,
            read: Arc::new(read),
        });
        self
    }

    pub fn legacy(mut self, name: &str, label: &str, superseded_by: CanonicalVariable) -> Self {
        self.legacy.push(LegacyEntry {
            declared: DeclaredVariable {
                name: name.to_owned(),
                label: label.to_owned(),
                ..canonical_declaration(superseded_by)
            },
            superseded_by,
        });
        self
    }

    pub fn declarations(&self) -> Vec<TriggerVariable> {
        let mut ordered: Vec<TriggerVariable> = self
            .entries
            .iter()
            .map(|entry| TriggerVariable {
                declared: entry.declared.clone(),
                standing: entry.standing,
            })
            .chain(self.legacy.iter().map(|entry| TriggerVariable {
                declared: entry.declared.clone(),
                standing: VariableStanding::Legacy(entry.superseded_by),
            }))
            .collect();
        ordered.sort_by_key(|variable| listing_rank(variable.standing));
        ordered
    }

    pub fn schema(&self) -> VariableSchema {
        VariableSchema {
            variables: self
                .declarations()
                .into_iter()
                .map(|variable| variable.declared)
                .collect(),
        }
    }

    pub fn arg_stack(&self, event: &Event) -> ArgStack {
        let read_directly = self
            .entries
            .iter()
            .map(|entry| (&entry.declared.name, (entry.read)(event)));
        let legacy = self.legacy.iter().map(|entry| {
            (
                &entry.declared.name,
                self.canonical_value(entry.superseded_by, event),
            )
        });
        read_directly
            .chain(legacy)
            .fold(ArgStack::new(), |stack, (name, value)| match value {
                Some(value) => stack.set(name.clone(), value),
                None => stack,
            })
    }

    fn canonical_value(&self, canonical: CanonicalVariable, event: &Event) -> Option<Variant> {
        self.entries
            .iter()
            .find(|entry| entry.standing == VariableStanding::Canonical(canonical))
            .and_then(|entry| (entry.read)(event))
    }

    fn canonical<F>(mut self, canonical: CanonicalVariable, read: F) -> Self
    where
        F: Fn(&Event) -> Variant + Send + Sync + 'static,
    {
        self.entries.push(VariableEntry {
            declared: canonical_declaration(canonical),
            standing: VariableStanding::Canonical(canonical),
            read: Arc::new(move |event| Some(read(event))),
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
        ActorSlot::Platform => platform.as_str().to_owned(),
        ActorSlot::Id | ActorSlot::Name | ActorSlot::Login => identity_value(slot, identity),
    })
}

fn identity_value(slot: ActorSlot, identity: &ActorIdentity) -> String {
    match slot {
        ActorSlot::Id => identity.id.clone(),
        ActorSlot::Name => shown_name(identity),
        ActorSlot::Login => identity.login.clone().unwrap_or_default(),
        ActorSlot::Platform => String::new(),
    }
}

pub fn money_value(slot: MoneySlot, amount: Option<&MoneyAmount>) -> Variant {
    match slot {
        MoneySlot::AmountMicros => Variant::Int(amount.map_or(0, |amount| {
            i64::try_from(amount.micros()).unwrap_or(i64::MAX)
        })),
        MoneySlot::Amount => Variant::Float(amount.map_or(0.0, MoneyAmount::major_units)),
        MoneySlot::Currency => Variant::String(
            amount.map_or_else(String::new, |amount| amount.currency().as_str().to_owned()),
        ),
        MoneySlot::AmountFormatted => {
            Variant::String(amount.map_or_else(String::new, MoneyAmount::formatted))
        }
    }
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
