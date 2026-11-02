use forge_events::{Event, EventSource, TypedEvent};
use forge_registry::{
    ActorDeclaration, EventFilter, KindPlatformContract, MoneySlot, RegistryError,
    TriggerCatalogEntry, TriggerCategory, TriggerKindDescriptor, TriggerRegistry, TriggerVariables,
    money_value,
};
use forge_types::{DeclaredVariable, TriggerConfig, Variant, VariantKind};

use crate::integration::MONOBANK_INTEGRATION;
use crate::jar_progress::{JAR_PROGRESS_KIND, JarProgressed};

const JAR_ID: &str = "jar_id";
const JAR_TITLE: &str = "jar_title";
const JAR_GOAL_AMOUNT_MICROS: &str = "jar_goal_amount_micros";
const JAR_GOAL_AMOUNT: &str = "jar_goal_amount";
const JAR_GOAL_AMOUNT_FORMATTED: &str = "jar_goal_amount_formatted";
const JAR_GOAL_PERCENT: &str = "jar_goal_percent";
const JAR_IS_SYNCED: &str = "jar_is_synced";

const GOAL_SLOTS: [(&str, MoneySlot, VariantKind); 3] = [
    (
        JAR_GOAL_AMOUNT_MICROS,
        MoneySlot::AmountMicros,
        VariantKind::Int,
    ),
    (JAR_GOAL_AMOUNT, MoneySlot::Amount, VariantKind::Float),
    (
        JAR_GOAL_AMOUNT_FORMATTED,
        MoneySlot::AmountFormatted,
        VariantKind::String,
    ),
];

pub fn register_monobank_triggers(reg: &mut TriggerRegistry) -> Result<(), RegistryError> {
    let mut reg = reg.owned_by(MONOBANK_INTEGRATION.id);
    reg.register(Box::new(JarProgressDescriptor))?;
    Ok(())
}

struct JarProgressDescriptor;

impl TriggerCatalogEntry for JarProgressDescriptor {
    fn category(&self) -> TriggerCategory {
        TriggerCategory::Goals
    }

    fn label(&self) -> &str {
        "Jar progress"
    }

    fn summary(&self) -> &str {
        "Fires with the jar balance after connecting and whenever the balance changes"
    }

    fn search_text(&self) -> &str {
        "monobank jar goal progress balance collected amount percent donation"
    }

    fn icon_name(&self) -> &str {
        "flag"
    }
}

impl TriggerKindDescriptor for JarProgressDescriptor {
    fn id(&self) -> &str {
        JAR_PROGRESS_KIND
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::Universal
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Donation),
            kind_prefix: Some(JAR_PROGRESS_KIND.to_owned()),
        }
    }

    fn matches_trigger(&self, _: &TriggerConfig, event: &Event) -> bool {
        event.source == EventSource::Donation && JarProgressed::from_event(event).is_some()
    }

    fn variables(&self) -> Option<TriggerVariables> {
        let variables = TriggerVariables::new()
            .money(|event| JarProgressed::from_event(event).map(|progress| progress.balance()))
            .event_specific(declared(JAR_ID, VariantKind::String), |event| {
                Variant::String(
                    JarProgressed::from_event(event)
                        .map(|progress| progress.jar_id)
                        .unwrap_or_default(),
                )
            })
            .event_specific(declared(JAR_TITLE, VariantKind::String), |event| {
                Variant::String(
                    JarProgressed::from_event(event)
                        .and_then(|progress| progress.jar_title)
                        .unwrap_or_default(),
                )
            });
        let variables = GOAL_SLOTS
            .into_iter()
            .fold(variables, |variables, (name, slot, kind)| {
                variables.event_specific(declared(name, kind), move |event| {
                    money_value(
                        slot,
                        JarProgressed::from_event(event)
                            .and_then(|progress| progress.goal())
                            .as_ref(),
                    )
                })
            });
        Some(
            variables
                .event_specific(declared(JAR_GOAL_PERCENT, VariantKind::Float), |event| {
                    Variant::Float(
                        JarProgressed::from_event(event).map_or(0.0, |progress| progress.percent()),
                    )
                })
                .event_specific(declared(JAR_IS_SYNCED, VariantKind::Bool), |event| {
                    Variant::Bool(
                        JarProgressed::from_event(event).is_some_and(|progress| progress.is_synced),
                    )
                }),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::Actorless
    }
}

fn declared(name: &str, kind: VariantKind) -> DeclaredVariable {
    DeclaredVariable {
        name: name.to_owned(),
        kind,
        synthesis: None,
    }
}
