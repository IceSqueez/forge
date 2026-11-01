use forge_events::{DeliveryLane, Event, EventSource};
use forge_registry::{
    ActorDeclaration, ChatTriggerFamily, ConfigProblem, EventFilter, FormField,
    KindPlatformContract, TriggerCategory, TriggerKindDescriptor, TriggerVariables, any_condition,
};
use forge_types::{
    DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, UserText, Variant, VariantKind,
};

use super::payload_read;

#[derive(Clone, Copy)]
pub(super) struct TwitchTriggerSpec {
    pub id: &'static str,
    pub category: TriggerCategory,
    pub label: &'static str,
    pub summary: &'static str,
    pub search_text: &'static str,
    pub icon: &'static str,
    pub kind: &'static str,
    pub actors: ActorDeclaration,
    pub variables: fn() -> Option<TriggerVariables>,
    pub matches: fn(&TriggerConfig, &Event) -> bool,
    pub default_config: fn() -> TriggerConfig,
    pub config_fields: fn() -> Vec<FormField>,
    pub condition_display: fn(&TriggerConfig) -> UserText,
    pub check_config: fn(&TriggerConfig) -> Result<(), ConfigProblem>,
    pub chat_family: Option<ChatTriggerFamily>,
    pub lane: DeliveryLane,
}

fn no_variables() -> Option<TriggerVariables> {
    None
}

fn always_matches(_: &TriggerConfig, _: &Event) -> bool {
    true
}

fn empty_config() -> TriggerConfig {
    TriggerConfig::new()
}

fn no_fields() -> Vec<FormField> {
    Vec::new()
}

fn any_event(_: &TriggerConfig) -> UserText {
    any_condition()
}

fn valid_config(_: &TriggerConfig) -> Result<(), ConfigProblem> {
    Ok(())
}

impl TwitchTriggerSpec {
    pub(super) const BASE: Self = Self {
        id: "",
        category: TriggerCategory::Core,
        label: "",
        summary: "",
        search_text: "",
        icon: "",
        kind: "",
        actors: ActorDeclaration::Undeclared,
        variables: no_variables,
        matches: always_matches,
        default_config: empty_config,
        config_fields: no_fields,
        condition_display: any_event,
        check_config: valid_config,
        chat_family: None,
        lane: DeliveryLane::Priority,
    };
}

impl TriggerKindDescriptor for TwitchTriggerSpec {
    fn id(&self) -> &str {
        self.id
    }

    fn category(&self) -> TriggerCategory {
        self.category
    }

    fn label(&self) -> &str {
        self.label
    }

    fn summary(&self) -> &str {
        self.summary
    }

    fn search_text(&self) -> &str {
        self.search_text
    }

    fn icon_name(&self) -> &str {
        self.icon
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Twitch)
    }

    fn default_config(&self) -> TriggerConfig {
        (self.default_config)()
    }

    fn config_fields(&self) -> Vec<FormField> {
        (self.config_fields)()
    }

    fn check_config(&self, config: &TriggerConfig) -> Result<(), ConfigProblem> {
        (self.check_config)(config)
    }

    fn condition_display(&self, config: &TriggerConfig) -> UserText {
        (self.condition_display)(config)
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some(self.kind.to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        (self.matches)(config, event)
    }

    fn variables(&self) -> Option<TriggerVariables> {
        (self.variables)()
    }

    fn actors(&self) -> ActorDeclaration {
        self.actors
    }

    fn chat_trigger_family(&self) -> Option<ChatTriggerFamily> {
        self.chat_family
    }

    fn delivery_lane(&self) -> DeliveryLane {
        self.lane
    }
}

#[derive(Clone, Copy)]
enum Read {
    NestedText(&'static str, &'static str),
    NestedInt(&'static str, &'static str),
    NestedFlag(&'static str, &'static str),
    NestedTimestamp(&'static str, &'static str),
    Custom(fn(&Event) -> Variant),
}

impl Read {
    fn variant(self, event: &Event) -> Option<Variant> {
        match self {
            Read::NestedText(object, key) => Some(Variant::String(payload_read::nested_text(
                event, object, key,
            ))),
            Read::NestedInt(object, key) => Some(Variant::Int(payload_read::nested_number(
                event, object, key,
            ))),
            Read::NestedFlag(object, key) => {
                Some(Variant::Bool(payload_read::nested_flag(event, object, key)))
            }
            Read::NestedTimestamp(object, key) => {
                payload_read::nested_timestamp(event, object, key)
            }
            Read::Custom(read) => Some(read(event)),
        }
    }
}

#[derive(Clone)]
pub(super) struct Variable {
    name: &'static str,
    kind: VariantKind,
    label: &'static str,
    synthesis: Option<SynthesisHint>,
    read: Read,
}

impl Variable {
    const fn new(name: &'static str, kind: VariantKind, label: &'static str, read: Read) -> Self {
        Self {
            name,
            kind,
            label,
            synthesis: None,
            read,
        }
    }

    pub(super) const fn nested_text(
        name: &'static str,
        label: &'static str,
        object: &'static str,
        key: &'static str,
    ) -> Self {
        Self::new(
            name,
            VariantKind::String,
            label,
            Read::NestedText(object, key),
        )
    }

    pub(super) const fn nested_int(
        name: &'static str,
        label: &'static str,
        object: &'static str,
        key: &'static str,
    ) -> Self {
        Self::new(name, VariantKind::Int, label, Read::NestedInt(object, key))
    }

    pub(super) const fn nested_flag(
        name: &'static str,
        label: &'static str,
        object: &'static str,
        key: &'static str,
    ) -> Self {
        Self::new(
            name,
            VariantKind::Bool,
            label,
            Read::NestedFlag(object, key),
        )
    }

    pub(super) const fn nested_timestamp(
        name: &'static str,
        label: &'static str,
        object: &'static str,
        key: &'static str,
    ) -> Self {
        Self::new(
            name,
            VariantKind::Datetime,
            label,
            Read::NestedTimestamp(object, key),
        )
    }

    pub(super) const fn custom(
        name: &'static str,
        kind: VariantKind,
        label: &'static str,
        read: fn(&Event) -> Variant,
    ) -> Self {
        Self::new(name, kind, label, Read::Custom(read))
    }

    pub(super) const fn synthesis(mut self, hint: SynthesisHint) -> Self {
        self.synthesis = Some(hint);
        self
    }

    pub(super) const fn bounded(mut self, min: i64, max: i64) -> Self {
        self.synthesis = Some(SynthesisHint::BoundedInt { min, max });
        self
    }

    fn declared(&self) -> DeclaredVariable {
        DeclaredVariable {
            name: self.name.to_owned(),
            kind: self.kind,
            label: self.label.to_owned(),
            synthesis: self.synthesis.clone(),
        }
    }
}

pub(super) fn table_variables(table: &'static [Variable]) -> TriggerVariables {
    table
        .iter()
        .fold(TriggerVariables::new(), |variables, variable| {
            extend(variables, variable.clone())
        })
}

pub(super) fn extend(variables: TriggerVariables, variable: Variable) -> TriggerVariables {
    let read = variable.read;
    variables.event_specific_optional(variable.declared(), move |event| read.variant(event))
}
