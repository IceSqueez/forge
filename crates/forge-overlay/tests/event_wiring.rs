#![allow(clippy::expect_used, clippy::panic)]
mod common;
use common::{ALERT_KIND, BLANK_KIND, GOAL_KIND, TICKER_KIND};

use std::collections::BTreeSet;

use forge_overlay::PreviewComposition;
use forge_overlay::config::{HEADLINE, SUBLINE};
use forge_overlay::kinds::alert::AlertOverlayKind;
use forge_overlay::{
    ConfigSection, DeliveryDisposition, OverlayConfig, OverlayKindDescriptor, SectionedField,
};
use forge_overlay::{
    EventWiringTrigger, OverlayKindRegistry, PageAssets, accepts_event_wiring, order_curated_first,
    register_builtin_kinds, suggested_content,
};
use forge_registry::{ActorRole, ActorSlot, CanonicalVariable, TriggerVariable, VariableStanding};
use forge_types::{DeclaredVariable, SynthesisHint, Variant, VariantKind};

const CURATED_IN_PICKER_ORDER: &[&str] = &[
    "twitch.support.subscriber",
    "twitch.support.resubscriber",
    "twitch.support.gift_sub",
    "twitch.support.cheer",
    "donation.received",
    "twitch.channel.raid_received",
    "twitch.channel.follow",
    "twitch.channel_points.redemption",
];

const UNCURATED: &str = "stub.uncurated";
const STUB_LABEL: &str = "Something happened";

struct Retuned {
    disposition: DeliveryDisposition,
    draws: bool,
}

impl OverlayKindDescriptor for Retuned {
    fn id(&self) -> &str {
        "overlay.retuned"
    }

    fn label(&self) -> forge_types::MessageCode {
        AlertOverlayKind.label()
    }

    fn summary(&self) -> forge_types::MessageCode {
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

    fn preview(
        &self,
        config: &OverlayConfig,
        language: forge_types::NamesLanguage,
    ) -> PreviewComposition {
        AlertOverlayKind.preview(config, language)
    }

    fn has_visual_page(&self) -> bool {
        self.draws
    }
}

fn builtin_kinds() -> OverlayKindRegistry {
    let mut reg = OverlayKindRegistry::new();
    register_builtin_kinds(&mut reg).expect("the builtin overlay kinds register");
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

#[test]
fn a_kind_is_wireable_only_when_its_delivery_is_transient_and_it_draws_a_page() {
    let accepted: Vec<(DeliveryDisposition, bool)> = [
        DeliveryDisposition::Transient,
        DeliveryDisposition::Replace,
        DeliveryDisposition::Append,
    ]
    .into_iter()
    .flat_map(|disposition| [false, true].map(move |draws| (disposition, draws)))
    .filter(|(disposition, draws)| {
        accepts_event_wiring(&Retuned {
            disposition: *disposition,
            draws: *draws,
        })
    })
    .collect();

    assert_eq!(accepted, vec![(DeliveryDisposition::Transient, true)]);
}

#[test]
fn exactly_the_alert_and_the_ticker_of_the_shipped_kinds_accept_event_wiring() {
    let kinds = builtin_kinds();
    let accepted: BTreeSet<&str> = kinds
        .all()
        .filter(|kind| accepts_event_wiring(*kind))
        .map(OverlayKindDescriptor::id)
        .collect();

    assert_eq!(accepted, BTreeSet::from([ALERT_KIND, TICKER_KIND]),);
}

#[test]
fn the_shipped_kind_refused_despite_a_transient_delivery_is_the_one_that_draws_nothing() {
    let kinds = builtin_kinds();
    let refused: BTreeSet<&str> = kinds
        .all()
        .filter(|kind| kind.delivery_disposition() == DeliveryDisposition::Transient)
        .filter(|kind| !accepts_event_wiring(*kind))
        .map(OverlayKindDescriptor::id)
        .collect();

    assert_eq!(refused, BTreeSet::from([BLANK_KIND]));
    assert!(
        !kinds
            .get(BLANK_KIND)
            .expect("the blank look ships")
            .has_visual_page(),
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

    for shuffle in [[7, 0, 6, 1, 5, 2, 4, 3], [3, 2, 4, 1, 5, 0, 6, 7]] {
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
    let goal = kinds.get(GOAL_KIND).expect("the goal kind ships");
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
