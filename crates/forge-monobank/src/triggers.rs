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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use forge_events::{Event, EventSource, TypedEvent};
    use forge_registry::{CanonicalVariable, MoneySlot, TriggerRegistry};
    use forge_types::{CurrencyCode, MoneyAmount, TriggerConfig, Variant, VariantKind};

    use super::register_monobank_triggers;
    use crate::jar_progress::{JAR_PROGRESS_KIND, JarProgressed};

    fn registry() -> TriggerRegistry {
        let mut registry = TriggerRegistry::new();
        register_monobank_triggers(&mut registry).unwrap();
        registry
    }

    fn progress(goal_micros: Option<u64>, jar_title: Option<&str>) -> JarProgressed {
        JarProgressed {
            jar_id: "jarAAA111".to_owned(),
            jar_title: jar_title.map(str::to_owned),
            balance_micros: 250_000_000,
            goal_micros,
            currency: uah(),
            is_synced: true,
        }
    }

    fn uah() -> CurrencyCode {
        CurrencyCode::parse("UAH").unwrap()
    }

    fn args_of(progress: JarProgressed) -> BTreeMap<String, Variant> {
        registry()
            .get(JAR_PROGRESS_KIND)
            .unwrap()
            .build_arg_stack(&progress.into_event(EventSource::Donation, None))
            .snapshot()
    }

    fn money(slot: MoneySlot) -> String {
        CanonicalVariable::Money(slot).name().to_owned()
    }

    #[test]
    fn every_declared_output_is_set_with_its_declared_kind() {
        let registry = registry();
        let descriptor = registry.get(JAR_PROGRESS_KIND).unwrap();
        let args = args_of(progress(Some(1_000_000_000), Some("Drone")));

        let mismatched: Vec<String> = descriptor
            .output_schema()
            .unwrap()
            .variables
            .into_iter()
            .filter(|declared| {
                args.get(&declared.name).map(VariantKind::from_variant) != Some(declared.kind)
            })
            .map(|declared| declared.name)
            .collect();

        assert_eq!(mismatched, Vec::<String>::new());
    }

    #[test]
    fn arg_stack_carries_the_balance_the_goal_and_the_share_collected() {
        let args = args_of(progress(Some(1_000_000_000), Some("Drone")));

        let expected = BTreeMap::from([
            (money(MoneySlot::AmountMicros), Variant::Int(250_000_000)),
            (money(MoneySlot::Amount), Variant::Float(250.0)),
            (
                money(MoneySlot::Currency),
                Variant::String("UAH".to_owned()),
            ),
            (
                "jar_goal_amount_micros".to_owned(),
                Variant::Int(1_000_000_000),
            ),
            ("jar_goal_amount".to_owned(), Variant::Float(1_000.0)),
            (
                "jar_goal_amount_formatted".to_owned(),
                Variant::String(MoneyAmount::from_micros(1_000_000_000, uah()).formatted()),
            ),
            ("jar_goal_percent".to_owned(), Variant::Float(25.0)),
            ("jar_id".to_owned(), Variant::String("jarAAA111".to_owned())),
            ("jar_title".to_owned(), Variant::String("Drone".to_owned())),
            ("jar_is_synced".to_owned(), Variant::Bool(true)),
        ]);
        let actual: BTreeMap<String, Variant> = expected
            .keys()
            .filter_map(|name| args.get(name).map(|value| (name.clone(), value.clone())))
            .collect();

        assert_eq!(actual, expected);
    }

    #[test]
    fn arg_stack_without_a_goal_or_title_reports_zero_goal_and_empty_title() {
        let args = args_of(progress(None, None));

        let actual: Vec<Option<&Variant>> = [
            "jar_goal_amount_micros",
            "jar_goal_amount",
            "jar_goal_amount_formatted",
            "jar_goal_percent",
            "jar_title",
        ]
        .into_iter()
        .map(|name| args.get(name))
        .collect();

        assert_eq!(
            actual,
            [
                Some(&Variant::Int(0)),
                Some(&Variant::Float(0.0)),
                Some(&Variant::String(String::new())),
                Some(&Variant::Float(0.0)),
                Some(&Variant::String(String::new())),
            ]
        );
    }

    #[test]
    fn trigger_matches_only_jar_progress_from_the_donation_source() {
        let registry = registry();
        let descriptor = registry.get(JAR_PROGRESS_KIND).unwrap();
        let payload = serde_json::to_value(progress(None, None)).unwrap();

        for (event, expected) in [
            (
                Event::new(EventSource::Donation, JAR_PROGRESS_KIND, payload.clone()),
                true,
            ),
            (
                Event::new(EventSource::Twitch, JAR_PROGRESS_KIND, payload.clone()),
                false,
            ),
            (
                Event::new(EventSource::Donation, "donation.received", payload.clone()),
                false,
            ),
            (
                Event::new(
                    EventSource::Donation,
                    JAR_PROGRESS_KIND,
                    serde_json::json!({ "jar_id": "jarAAA111" }),
                ),
                false,
            ),
        ] {
            assert_eq!(
                descriptor.matches_trigger(&TriggerConfig::new(), &event),
                expected,
                "{} from {:?}",
                event.kind,
                event.source
            );
        }
    }
}
