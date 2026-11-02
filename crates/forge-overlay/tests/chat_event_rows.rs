mod common;
use common::{CHAT_KIND, text, with_kind};

use std::collections::BTreeSet;

use forge_overlay::config::EVENT;
use forge_overlay::{OVERLAY_MESSAGES, OverlayConfig, delivered_content};
use forge_registry::{CHAT_EVENT_VARIABLE, ChatEventKind};
use forge_types::{ArgStack, NamesLanguage, Variant};

fn delivered_event(stored: &OverlayConfig, supplied: &OverlayConfig, args: &ArgStack) -> Variant {
    with_kind(CHAT_KIND, |descriptor| {
        delivered_content(descriptor, stored, supplied, args)
    })
    .remove(EVENT)
    .unwrap_or_else(|| text("missing"))
}

fn implied_label(kind: ChatEventKind, language: NamesLanguage) -> String {
    let args = kind.marked(ArgStack::new().with_names_language(language));
    match delivered_event(&OverlayConfig::new(), &OverlayConfig::new(), &args) {
        Variant::String(label) => label,
        _ => String::new(),
    }
}

fn one(key: &str, value: &str) -> OverlayConfig {
    OverlayConfig::from([(key.to_owned(), text(value))])
}

#[test]
fn every_chat_event_kind_shows_a_label_taken_from_its_language_catalog() {
    for (language, ftl) in [
        (NamesLanguage::En, OVERLAY_MESSAGES.en_ftl),
        (NamesLanguage::Uk, OVERLAY_MESSAGES.uk_ftl),
    ] {
        for kind in ChatEventKind::ALL {
            let label = implied_label(kind, language);
            assert!(!label.trim().is_empty(), "{kind:?} {language:?}");
            assert!(
                ftl.lines()
                    .any(|line| line.ends_with(&format!("= {label}"))),
                "{kind:?} {language:?}: {label:?} is not a catalog value"
            );
        }
    }
}

#[test]
fn each_chat_event_kind_shows_its_own_label() {
    for language in [NamesLanguage::En, NamesLanguage::Uk] {
        let labels: BTreeSet<String> = ChatEventKind::ALL
            .into_iter()
            .map(|kind| implied_label(kind, language))
            .collect();
        assert_eq!(labels.len(), ChatEventKind::ALL.len(), "{language:?}");
    }
}

#[test]
fn the_label_follows_the_run_language() {
    let in_language = |language| {
        ChatEventKind::ALL
            .into_iter()
            .map(|kind| implied_label(kind, language))
            .collect::<Vec<_>>()
    };

    assert_ne!(
        in_language(NamesLanguage::En),
        in_language(NamesLanguage::Uk)
    );
}

#[test]
fn a_run_without_a_known_chat_event_shows_no_event_label() {
    let unknown = ArgStack::new().set(CHAT_EVENT_VARIABLE.to_owned(), text("host"));
    for (args, case) in [(ArgStack::new(), "absent"), (unknown, "unknown")] {
        assert_eq!(
            delivered_event(&OverlayConfig::new(), &OverlayConfig::new(), &args),
            text(""),
            "{case}"
        );
    }
}

#[test]
fn an_explicit_event_overrides_the_one_the_trigger_implies() {
    let args = ChatEventKind::Raid.marked(ArgStack::new());
    for (stored, supplied, expected, case) in [
        (
            OverlayConfig::new(),
            one(EVENT, "Big raid"),
            "Big raid",
            "step",
        ),
        (
            one(EVENT, "Incoming"),
            OverlayConfig::new(),
            "Incoming",
            "stored overlay",
        ),
        (
            one(EVENT, "Incoming"),
            one(EVENT, "Big raid"),
            "Big raid",
            "step over stored overlay",
        ),
    ] {
        assert_eq!(
            delivered_event(&stored, &supplied, &args),
            text(expected),
            "{case}"
        );
    }
}
