#![allow(clippy::unwrap_used, clippy::expect_used)]

use forge_events::Event;
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
    TriggerVariables, declared_variables,
};
use forge_types::{
    CanonicalCount, CanonicalVariable, DeclaredVariable, PlatformId, TriggerConfig, VariableSchema,
    VariableStanding, Variant, VariantKind,
};

struct StubTrigger {
    id: &'static str,
    variables: Option<fn() -> TriggerVariables>,
    schema: Option<VariableSchema>,
}

impl StubTrigger {
    fn declaring(id: &'static str, variables: fn() -> TriggerVariables) -> Self {
        StubTrigger {
            id,
            variables: Some(variables),
            schema: None,
        }
    }

    fn hand_written(id: &'static str, schema: VariableSchema) -> Self {
        StubTrigger {
            id,
            variables: None,
            schema: Some(schema),
        }
    }

    fn silent(id: &'static str) -> Self {
        StubTrigger {
            id,
            variables: None,
            schema: None,
        }
    }
}

impl TriggerKindDescriptor for StubTrigger {
    fn id(&self) -> &str {
        self.id
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        self.id
    }

    fn summary(&self) -> &str {
        self.id
    }

    fn search_text(&self) -> &str {
        self.id
    }

    fn icon_name(&self) -> &str {
        "chat"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Kick)
    }

    fn default_config(&self) -> TriggerConfig {
        TriggerConfig::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        Vec::new()
    }

    fn condition_display(&self, _config: &TriggerConfig) -> String {
        String::new()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: None,
            kind_prefix: None,
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        self.variables.map(|build| build())
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        self.schema
            .clone()
            .or_else(|| self.variables().map(|variables| variables.schema()))
    }
}

fn declared(name: &str, kind: VariantKind) -> DeclaredVariable {
    DeclaredVariable {
        name: name.to_owned(),
        kind,
        label: name.to_owned(),
        synthesis: None,
    }
}

fn listed(descriptor: &dyn TriggerKindDescriptor) -> Vec<(String, VariableStanding)> {
    declared_variables(descriptor)
        .unwrap_or_default()
        .into_iter()
        .map(|variable| (variable.declared.name, variable.standing))
        .collect()
}

fn one_of_each_standing() -> TriggerVariables {
    TriggerVariables::new()
        .legacy(
            declared("months", VariantKind::Int),
            CanonicalVariable::Count(CanonicalCount::SubCumulativeMonths),
            |_| Variant::Int(0),
        )
        .event_specific(declared("share_streak", VariantKind::Bool), |_| {
            Variant::Bool(true)
        })
        .sub_tier(|_| String::new())
        .message_text(|_| String::new())
        .count(CanonicalCount::ViewerCount, |_| 0)
}

#[test]
fn a_declaring_descriptor_reaches_the_screen_canonical_block_first_and_legacy_names_last() {
    assert_eq!(
        listed(&StubTrigger::declaring("stub.mixed", one_of_each_standing)),
        vec![
            (
                "message_text".to_owned(),
                VariableStanding::Canonical(CanonicalVariable::MessageText),
            ),
            (
                "viewer_count".to_owned(),
                VariableStanding::Canonical(CanonicalVariable::Count(CanonicalCount::ViewerCount,)),
            ),
            (
                "sub_tier".to_owned(),
                VariableStanding::Canonical(CanonicalVariable::SubTier),
            ),
            ("share_streak".to_owned(), VariableStanding::EventSpecific),
            (
                "months".to_owned(),
                VariableStanding::Legacy(CanonicalVariable::Count(
                    CanonicalCount::SubCumulativeMonths,
                )),
            ),
        ],
    );
}

#[test]
fn a_descriptor_with_only_a_hand_written_schema_reaches_the_screen_as_event_specific() {
    let schema = VariableSchema {
        variables: vec![
            declared("scene_name", VariantKind::String),
            declared("is_studio_mode", VariantKind::Bool),
        ],
    };
    assert_eq!(
        listed(&StubTrigger::hand_written("stub.obs", schema)),
        vec![
            ("scene_name".to_owned(), VariableStanding::EventSpecific),
            ("is_studio_mode".to_owned(), VariableStanding::EventSpecific),
        ],
    );
}

#[test]
fn declaring_an_empty_list_stays_distinguishable_from_declaring_nothing() {
    assert_eq!(
        declared_variables(&StubTrigger::declaring("stub.empty", TriggerVariables::new))
            .map(|variables| variables.len()),
        Some(0),
    );
    assert!(declared_variables(&StubTrigger::silent("stub.silent")).is_none());
}
