#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use forge_overlay::config::{HEADLINE, SUBLINE};
use forge_overlay::kinds::alert::AlertOverlayKind;
use forge_overlay::{OverlayConfig, SampleContext, SampleTrigger, sample_content, sample_context};
use forge_registry::{KindPlatformContract, TriggerVariable};
use forge_types::{
    CanonicalCount, CanonicalVariable, DeclaredVariable, PlatformId, VariableStanding, Variant,
    VariantKind,
};

const CHATTER: &str = "stub.chatter";
const RAIDER: &str = "stub.raider";
const UNSHIPPED: &str = "vendor.kind_this_build_lacks";
const MESSAGE_TOKEN: &str = "%message_text%";
const VIEWER_TOKEN: &str = "%viewer_count%";
const STAMP_TOKEN: &str = "%stamped_at%";
const STAMP_NAME: &str = "stamped_at";
const UNIX_EPOCH_TEXT: &str = "1970-01-01T00:00:00Z";

fn feed(kind_id: &str, variables: Vec<TriggerVariable>) -> SampleTrigger {
    SampleTrigger {
        kind_id: kind_id.to_owned(),
        contract: KindPlatformContract::PlatformSpecific(PlatformId::Twitch),
        variables,
    }
}

fn chatter() -> SampleTrigger {
    feed(
        CHATTER,
        vec![TriggerVariable::canonical(CanonicalVariable::MessageText)],
    )
}

fn raider() -> SampleTrigger {
    feed(
        RAIDER,
        vec![TriggerVariable::canonical(CanonicalVariable::Count(
            CanonicalCount::ViewerCount,
        ))],
    )
}

fn unshipped() -> SampleTrigger {
    feed(UNSHIPPED, Vec::new())
}

fn stamped() -> SampleTrigger {
    feed(
        CHATTER,
        vec![TriggerVariable {
            declared: DeclaredVariable {
                name: STAMP_NAME.to_owned(),
                kind: VariantKind::Datetime,
                label: "Stamped at".to_owned(),
                synthesis: None,
            },
            standing: VariableStanding::EventSpecific,
        }],
    )
}

fn stored(headline: &str, subline: &str) -> OverlayConfig {
    OverlayConfig::from([
        (HEADLINE.to_owned(), Variant::String(headline.to_owned())),
        (SUBLINE.to_owned(), Variant::String(subline.to_owned())),
    ])
}

fn rendered(context: &SampleContext, headline: &str, subline: &str) -> (String, String) {
    let content = sample_content(&AlertOverlayKind, &stored(headline, subline), context);
    let read = |key: &str| {
        content
            .get(key)
            .and_then(Variant::as_str)
            .unwrap_or_else(|| panic!("the sample left {key} unfilled"))
            .to_owned()
    };
    (read(HEADLINE), read(SUBLINE))
}

fn resolves(context: &SampleContext, token: &str) -> bool {
    rendered(context, token, token).0 != token
}

#[test]
fn a_lone_feeding_kind_samples_the_variables_that_kind_declares_and_leaves_the_others_raw() {
    for (feeding, resolved, raw) in [
        (chatter(), MESSAGE_TOKEN, VIEWER_TOKEN),
        (raider(), VIEWER_TOKEN, MESSAGE_TOKEN),
    ] {
        let kind_id = feeding.kind_id.clone();
        let context = sample_context(&[feeding]);

        assert_eq!(
            rendered(&context, resolved, raw).1,
            raw,
            "{kind_id} sampled {raw}, which it never declares",
        );
        assert!(
            resolves(&context, resolved),
            "{kind_id} left its own {resolved} unresolved",
        );
    }
}

#[test]
fn an_overlay_fed_by_nothing_or_by_several_kinds_samples_the_neutral_vocabulary() {
    for (feeding, wiring) in [
        (Vec::new(), "nothing at all"),
        (vec![chatter(), raider()], "two kinds that disagree"),
    ] {
        assert_eq!(
            sample_context(&feeding),
            SampleContext::neutral(),
            "an overlay fed by {wiring} sampled something other than the neutral vocabulary",
        );
    }
}

#[test]
fn the_neutral_vocabulary_resolves_every_canonical_name() {
    let neutral = SampleContext::neutral();

    for canonical in CanonicalVariable::all() {
        let token = format!("%{}%", canonical.name());

        assert!(
            resolves(&neutral, &token),
            "an unwired overlay naming {token} previews a raw token",
        );
    }
}

#[test]
fn several_feeds_of_one_kind_still_sample_that_kind() {
    let many = sample_context(&[chatter(), chatter(), chatter()]);

    assert_eq!(many, sample_context(&[chatter()]));
    assert_ne!(many, SampleContext::neutral());
}

#[test]
fn a_feeding_kind_this_build_does_not_carry_still_counts_as_a_kind_of_its_own() {
    assert_eq!(
        sample_context(&[chatter(), unshipped()]),
        SampleContext::neutral(),
        "an unknown kind vanished, letting two feeds masquerade as one",
    );
    assert!(
        !resolves(&sample_context(&[unshipped()]), MESSAGE_TOKEN),
        "an unknown kind borrowed a canonical layer it never declared",
    );
}

#[test]
fn a_sample_is_minted_by_the_stable_constructor_so_a_regenerated_preview_repeats_itself() {
    assert_eq!(
        rendered(&sample_context(&[stamped()]), STAMP_TOKEN, STAMP_TOKEN).0,
        UNIX_EPOCH_TEXT,
        "a sampled timestamp moves with the wall clock, so every boot rewrites the preview",
    );
}
