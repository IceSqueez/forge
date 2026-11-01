use crate::category::TriggerCategory;
use crate::descriptor::TriggerKindDescriptor;
use crate::event_filter::EventFilter;
use crate::form::FormField;
use crate::kind_platform_contract::KindPlatformContract;
use crate::variables::TriggerVariables;
use forge_events::{Event, EventSource};
use forge_types::{
    DeclaredVariable, MessageCode, SynthesisHint, TriggerConfig, UserText, Variant, VariantKind,
};

#[derive(Clone)]
pub enum TriggerCondition {
    Fixed(MessageCode),
    FromConfig(fn(&TriggerConfig) -> UserText),
}

#[derive(Clone)]
pub enum TriggerMatcher {
    Kind(&'static str),
    KindWhere(&'static str, fn(&TriggerConfig, &Event) -> bool),
    Custom(fn(&TriggerConfig, &Event) -> bool),
}

#[derive(Clone)]
pub struct EventTriggerSpec {
    pub id: &'static str,
    pub source: EventSource,
    pub label: &'static str,
    pub summary: &'static str,
    pub search_text: &'static str,
    pub icon: &'static str,
    pub prefix: &'static str,
    pub matcher: TriggerMatcher,
    pub condition: TriggerCondition,
    pub config_fields: fn() -> Vec<FormField>,
    pub variables: fn() -> Option<TriggerVariables>,
}

pub fn no_config_fields() -> Vec<FormField> {
    Vec::new()
}

impl TriggerKindDescriptor for EventTriggerSpec {
    fn id(&self) -> &str {
        self.id
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Integration
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
        KindPlatformContract::Universal
    }

    fn config_fields(&self) -> Vec<FormField> {
        (self.config_fields)()
    }

    fn condition_display(&self, config: &TriggerConfig) -> UserText {
        match &self.condition {
            TriggerCondition::Fixed(code) => code.clone().into(),
            TriggerCondition::FromConfig(display) => display(config),
        }
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(self.source),
            kind_prefix: Some(self.prefix.to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        match self.matcher {
            TriggerMatcher::Kind(kind) => event.kind == kind,
            TriggerMatcher::KindWhere(kind, admits) => event.kind == kind && admits(config, event),
            TriggerMatcher::Custom(matches) => matches(config, event),
        }
    }

    fn variables(&self) -> Option<TriggerVariables> {
        (self.variables)()
    }
}

#[derive(Clone, Copy)]
enum Read {
    Text(&'static str),
    Flag(&'static str),
    Int(&'static str),
    Float(&'static str),
    TextList(&'static str),
    Custom(fn(&Event) -> Option<Variant>),
}

impl Read {
    fn variant(self, event: &Event) -> Option<Variant> {
        match self {
            Read::Text(key) => event
                .payload
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(|text| Variant::String(text.to_owned())),
            Read::Flag(key) => event
                .payload
                .get(key)
                .and_then(serde_json::Value::as_bool)
                .map(Variant::Bool),
            Read::Int(key) => event
                .payload
                .get(key)
                .and_then(serde_json::Value::as_i64)
                .map(Variant::Int),
            Read::Float(key) => event
                .payload
                .get(key)
                .and_then(serde_json::Value::as_f64)
                .map(Variant::Float),
            Read::TextList(key) => event
                .payload
                .get(key)
                .and_then(serde_json::Value::as_array)
                .map(|items| {
                    Variant::Array(
                        items
                            .iter()
                            .filter_map(serde_json::Value::as_str)
                            .map(|item| Variant::String(item.to_owned()))
                            .collect(),
                    )
                }),
            Read::Custom(read) => read(event),
        }
    }
}

#[derive(Clone)]
pub struct EventVariable {
    name: &'static str,
    kind: VariantKind,
    synthesis: Option<SynthesisHint>,
    read: Read,
}

impl EventVariable {
    const fn new(name: &'static str, kind: VariantKind, read: Read) -> Self {
        Self {
            name,
            kind,
            synthesis: None,
            read,
        }
    }

    pub const fn text(name: &'static str, key: &'static str) -> Self {
        Self::new(name, VariantKind::String, Read::Text(key))
    }

    pub const fn flag(name: &'static str, key: &'static str) -> Self {
        Self::new(name, VariantKind::Bool, Read::Flag(key))
    }

    pub const fn int(name: &'static str, key: &'static str) -> Self {
        Self::new(name, VariantKind::Int, Read::Int(key))
    }

    pub const fn float(name: &'static str, key: &'static str) -> Self {
        Self::new(name, VariantKind::Float, Read::Float(key))
    }

    pub const fn text_list(name: &'static str, key: &'static str) -> Self {
        Self::new(name, VariantKind::Array, Read::TextList(key))
    }

    pub const fn synthesis(mut self, hint: SynthesisHint) -> Self {
        self.synthesis = Some(hint);
        self
    }

    pub const fn custom(
        name: &'static str,
        kind: VariantKind,
        read: fn(&Event) -> Option<Variant>,
    ) -> Self {
        Self::new(name, kind, Read::Custom(read))
    }
}

pub fn event_variables(table: &'static [EventVariable]) -> Option<TriggerVariables> {
    Some(
        table
            .iter()
            .fold(TriggerVariables::new(), |variables, variable| {
                let read = variable.read;
                variables.event_specific_optional(
                    DeclaredVariable {
                        name: variable.name.to_owned(),
                        kind: variable.kind,
                        synthesis: variable.synthesis.clone(),
                    },
                    move |event| read.variant(event),
                )
            }),
    )
}
