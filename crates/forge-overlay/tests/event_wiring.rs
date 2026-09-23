#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use forge_overlay::config::{HEADLINE, SUBLINE};
use forge_overlay::descriptor::{
    ConfigSection, DeliveryDisposition, OverlayConfig, OverlayKindDescriptor, SectionedField,
};
use forge_overlay::kinds::alert::AlertOverlayKind;
use forge_overlay::preview::PreviewComposition;
use forge_overlay::{
    EventWiringTrigger, OverlayKindRegistry, PageAssets, accepts_event_wiring, is_curated,
    order_curated_first, register_builtin_kinds, suggested_content,
};
use forge_registry::{TriggerRegistry, TriggerVariable, declared_variables};
use forge_types::{
    ActorRole, ActorSlot, ArgStack, CanonicalCount, CanonicalVariable, DeclaredVariable,
    SynthesisHint, VariableStanding, Variant, VariantKind,
};

const CURATED_IN_PICKER_ORDER: &[&str] = &[
    "twitch.support.subscriber",
    "twitch.support.resubscriber",
    "twitch.support.gift_sub",
    "twitch.support.cheer",
    "twitch.channel.raid_received",
    "twitch.channel.follow",
    "twitch.channel_points.redemption",
];

const UNCURATED: &str = "stub.uncurated";
const STUB_LABEL: &str = "Something happened";
const RESOLVED: &str = "resolved";

struct Retuned {
    disposition: DeliveryDisposition,
    machine_filled: bool,
}

impl OverlayKindDescriptor for Retuned {
    fn id(&self) -> &str {
        "overlay.retuned"
    }

    fn label(&self) -> &str {
        AlertOverlayKind.label()
    }

    fn summary(&self) -> &str {
        AlertOverlayKind.summary()
    }

    fn icon_name(&self) -> &str {
        AlertOverlayKind.icon_name()
    }

    fn delivery_disposition(&self) -> DeliveryDisposition {
        self.disposition
    }

    fn order_sensitive(&self) -> bool {
        AlertOverlayKind.order_sensitive()
    }

    fn config_schema_version(&self) -> u32 {
        AlertOverlayKind.config_schema_version()
    }

    fn default_config(&self) -> OverlayConfig {
        AlertOverlayKind.default_config()
    }

    fn config_fields(&self) -> Vec<SectionedField> {
        AlertOverlayKind.config_fields()
    }

    fn page_assets(&self) -> PageAssets {
        AlertOverlayKind.page_assets()
    }

    fn preview(&self, config: &OverlayConfig) -> PreviewComposition {
        AlertOverlayKind.preview(config)
    }

    fn content_is_machine_filled(&self) -> bool {
        self.machine_filled
    }
}

fn builtin_kinds() -> OverlayKindRegistry {
    let mut reg = OverlayKindRegistry::new();
    register_builtin_kinds(&mut reg).expect("the builtin overlay kinds register");
    reg
}

fn every_platform_trigger() -> TriggerRegistry {
    let mut reg = TriggerRegistry::new();
    forge_platform_twitch::register_twitch_triggers(&mut reg)
        .expect("the twitch triggers register");
    forge_platform_kick::register_kick_triggers(&mut reg).expect("the kick triggers register");
    forge_platform_youtube::register_youtube_triggers(&mut reg)
        .expect("the youtube triggers register");
    reg
}

fn canonical(canonical: CanonicalVariable) -> TriggerVariable {
    TriggerVariable::canonical(canonical)
}

fn declared(name: &str, kind: VariantKind, synthesis: Option<SynthesisHint>) -> DeclaredVariable {
    DeclaredVariable {
        name: name.to_owned(),
        kind,
        label: name.to_owned(),
        synthesis,
    }
}

fn event_specific(
    name: &str,
    kind: VariantKind,
    synthesis: Option<SynthesisHint>,
) -> TriggerVariable {
    TriggerVariable {
        declared: declared(name, kind, synthesis),
        standing: VariableStanding::EventSpecific,
    }
}

fn legacy(name: &str, superseded_by: CanonicalVariable) -> TriggerVariable {
    TriggerVariable {
        declared: declared(name, VariantKind::String, Some(SynthesisHint::Message)),
        standing: VariableStanding::Legacy(superseded_by),
    }
}

fn lines(kind_id: &str, label: &str, variables: &[TriggerVariable]) -> (String, String) {
    let content = suggested_content(
        &AlertOverlayKind,
        &EventWiringTrigger {
            kind_id,
            label,
            variables,
        },
    );
    (text_at(&content, HEADLINE), text_at(&content, SUBLINE))
}

fn text_at(content: &OverlayConfig, key: &str) -> String {
    content
        .get(key)
        .and_then(Variant::as_str)
        .unwrap_or_else(|| panic!("the suggestion left {key} unfilled"))
        .to_owned()
}

fn declared_names(variables: &[TriggerVariable]) -> BTreeSet<String> {
    variables
        .iter()
        .map(|variable| variable.declared.name.clone())
        .collect()
}

fn resolving_every(names: &BTreeSet<String>) -> ArgStack {
    names.iter().fold(ArgStack::new(), |stack, name| {
        stack.set(name.clone(), Variant::String(RESOLVED.to_owned()))
    })
}

#[test]
fn a_kind_is_wireable_only_when_its_delivery_is_transient_and_its_content_is_author_written() {
    let accepted: Vec<(DeliveryDisposition, bool)> = [
        DeliveryDisposition::Transient,
        DeliveryDisposition::Replace,
        DeliveryDisposition::Append,
    ]
    .into_iter()
    .flat_map(|disposition| [false, true].map(move |machine_filled| (disposition, machine_filled)))
    .filter(|(disposition, machine_filled)| {
        accepts_event_wiring(&Retuned {
            disposition: *disposition,
            machine_filled: *machine_filled,
        })
    })
    .collect();

    assert_eq!(accepted, vec![(DeliveryDisposition::Transient, false)]);
}

#[test]
fn exactly_the_alert_and_the_ticker_of_the_shipped_kinds_accept_event_wiring() {
    let kinds = builtin_kinds();
    let accepted: BTreeSet<&str> = kinds
        .all()
        .filter(|kind| accepts_event_wiring(*kind))
        .map(OverlayKindDescriptor::id)
        .collect();

    assert_eq!(
        accepted,
        BTreeSet::from(["overlay.alert", "overlay.ticker"]),
    );
}

#[test]
fn the_shipped_kind_refused_despite_a_transient_delivery_is_refused_by_the_machine_filled_clause() {
    let kinds = builtin_kinds();
    let refused: BTreeSet<&str> = kinds
        .all()
        .filter(|kind| kind.delivery_disposition() == DeliveryDisposition::Transient)
        .filter(|kind| !accepts_event_wiring(*kind))
        .map(OverlayKindDescriptor::id)
        .collect();

    assert_eq!(refused, BTreeSet::from(["overlay.audio"]));
    assert!(
        kinds
            .get("overlay.audio")
            .expect("the audio kind ships")
            .content_is_machine_filled(),
    );
}

#[test]
fn every_curated_row_is_admitted_for_the_real_trigger_it_names() {
    let triggers = every_platform_trigger();

    for kind_id in CURATED_IN_PICKER_ORDER {
        assert!(is_curated(kind_id), "{kind_id} left the launch set");
        let descriptor = triggers
            .get(kind_id)
            .unwrap_or_else(|| panic!("{kind_id} is no longer a registered trigger"));
        let variables = declared_variables(descriptor)
            .unwrap_or_else(|| panic!("{kind_id} declares no variables at all"));

        let curated = lines(kind_id, descriptor.label(), &variables);
        let derived = lines(UNCURATED, descriptor.label(), &variables);

        assert_ne!(
            curated, derived,
            "{kind_id} fell back to derived wording, so its curated row names an undeclared token",
        );
    }
}

#[test]
fn a_curated_row_missing_one_declared_token_falls_back_to_derived_wording_whole() {
    let triggers = every_platform_trigger();
    let kind_id = "twitch.support.resubscriber";
    let descriptor = triggers.get(kind_id).expect("the resubscriber ships");
    let label = descriptor.label();
    let full = declared_variables(descriptor).expect("the resubscriber declares variables");
    let pruned: Vec<TriggerVariable> = full
        .iter()
        .filter(|variable| {
            variable.declared.name
                != CanonicalVariable::Count(CanonicalCount::SubCumulativeMonths).name()
        })
        .cloned()
        .collect();

    assert_ne!(
        pruned.len(),
        full.len(),
        "the pruned token was never declared"
    );
    assert_eq!(
        lines(kind_id, label, &pruned),
        lines(UNCURATED, label, &pruned),
        "one absent token left the curated headline standing beside derived wording",
    );
}

#[test]
fn derived_wording_pairs_the_principal_name_with_the_first_detail_it_finds() {
    let user_name = canonical(CanonicalVariable::actor(
        ActorRole::Principal,
        ActorSlot::Name,
    ));
    let message = canonical(CanonicalVariable::MessageText);
    let tier = canonical(CanonicalVariable::SubTier);
    let user_id = canonical(CanonicalVariable::actor(
        ActorRole::Principal,
        ActorSlot::Id,
    ));

    for (variables, expected, shape) in [
        (
            vec![user_name.clone(), message.clone()],
            (
                "%user_name%".to_owned(),
                format!("{STUB_LABEL}: %message_text%"),
            ),
            "an actor and a detail",
        ),
        (
            vec![user_name.clone(), tier],
            ("%user_name%".to_owned(), STUB_LABEL.to_owned()),
            "an actor with nothing detailed beside it",
        ),
        (
            vec![message],
            (STUB_LABEL.to_owned(), "%message_text%".to_owned()),
            "a detail with no actor",
        ),
        (
            vec![user_id],
            (STUB_LABEL.to_owned(), STUB_LABEL.to_owned()),
            "neither an actor name nor a detail",
        ),
    ] {
        assert_eq!(
            lines(UNCURATED, STUB_LABEL, &variables),
            expected,
            "{shape} produced the wrong derived wording",
        );
    }
}

#[test]
fn a_declaration_without_a_synthesis_hint_an_actor_slot_and_a_legacy_name_are_all_passed_over() {
    let detail = event_specific(
        "reward_title",
        VariantKind::String,
        Some(SynthesisHint::Message),
    );

    for (skipped, what) in [
        (
            event_specific("redeemed_at", VariantKind::Datetime, None),
            "a declaration carrying no synthesis hint",
        ),
        (
            canonical(CanonicalVariable::actor(
                ActorRole::Principal,
                ActorSlot::Login,
            )),
            "an actor slot that carries a hint",
        ),
        (
            legacy("cheer_message", CanonicalVariable::MessageText),
            "a legacy twin that carries a hint",
        ),
    ] {
        let variables = vec![
            canonical(CanonicalVariable::actor(
                ActorRole::Principal,
                ActorSlot::Name,
            )),
            skipped,
            detail.clone(),
        ];

        assert_eq!(
            lines(UNCURATED, STUB_LABEL, &variables).1,
            format!("{STUB_LABEL}: %reward_title%"),
            "{what} was taken as the detail",
        );
    }
}

#[test]
fn ordering_lifts_the_curated_rows_into_table_order_and_leaves_the_rest_as_offered() {
    let tail = ["zzz.last", "aaa.first", "mmm.middle"];
    let expected: Vec<String> = CURATED_IN_PICKER_ORDER
        .iter()
        .chain(tail.iter())
        .map(|id| (*id).to_owned())
        .collect();

    for shuffle in [[6, 0, 5, 1, 4, 2, 3], [3, 2, 4, 1, 5, 0, 6]] {
        let mut offered: Vec<String> = shuffle
            .iter()
            .map(|index| CURATED_IN_PICKER_ORDER[*index].to_owned())
            .chain(tail.iter().map(|id| (*id).to_owned()))
            .collect();

        order_curated_first(&mut offered, |id| id.as_str());

        assert_eq!(offered, expected, "{shuffle:?}");
    }
}

#[test]
fn a_content_key_the_model_has_no_line_for_is_left_out_rather_than_invented() {
    let kinds = builtin_kinds();
    let goal = kinds.get("overlay.goal").expect("the goal kind ships");
    let content_keys: BTreeSet<&str> = goal
        .config_fields()
        .iter()
        .filter(|sectioned| sectioned.section == ConfigSection::Content)
        .map(|sectioned| sectioned.field.key())
        .collect();

    assert!(
        !content_keys.contains(HEADLINE) && !content_keys.contains(SUBLINE),
        "the goal kind grew a headline, so it no longer exercises the unfilled-key path",
    );
    assert!(
        suggested_content(
            goal,
            &EventWiringTrigger {
                kind_id: UNCURATED,
                label: STUB_LABEL,
                variables: &[canonical(CanonicalVariable::MessageText)],
            },
        )
        .is_empty(),
    );
}

#[test]
fn every_registered_trigger_is_suggested_wording_its_own_delivery_can_carry() {
    let triggers = every_platform_trigger();

    for descriptor in triggers.all() {
        let variables = declared_variables(descriptor).unwrap_or_default();
        let stack = resolving_every(&declared_names(&variables));
        let (headline, subline) = lines(descriptor.id(), descriptor.label(), &variables);

        for line in [headline, subline] {
            assert!(
                !stack.interpolate(&line).contains('%'),
                "{} is suggested {line:?}, which names something it never declares",
                descriptor.id(),
            );
            assert!(
                !line.is_empty(),
                "{} is suggested a blank line, which a delivery replaces with the overlay's own stale wording",
                descriptor.id(),
            );
        }
    }
}
