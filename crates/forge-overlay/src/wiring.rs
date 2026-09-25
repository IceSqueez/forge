use std::collections::BTreeSet;

use forge_registry::TriggerVariable;
use forge_types::{ActorRole, ActorSlot, CanonicalVariable, VariableStanding, variable_references};

use crate::config;
use crate::descriptor::{ConfigSection, DeliveryDisposition, OverlayConfig, OverlayKindDescriptor};

const PRINCIPAL_DISPLAY_NAME: CanonicalVariable =
    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Name);

/// `variables` is the kind's own `declarations()`, whose canonical-first order the derived
/// wording reads as a preference order.
pub struct EventWiringTrigger<'a> {
    pub kind_id: &'a str,
    pub label: &'a str,
    pub variables: &'a [TriggerVariable],
}

struct CuratedWording {
    kind_id: &'static str,
    headline: &'static str,
    subline: &'static str,
}

const CURATED: &[CuratedWording] = &[
    CuratedWording {
        kind_id: "twitch.support.subscriber",
        headline: "%user_name% just subscribed!",
        subline: "Welcome to the crew",
    },
    CuratedWording {
        kind_id: "twitch.support.resubscriber",
        headline: "%user_name% resubscribed!",
        subline: "%sub_cumulative_months% months and counting",
    },
    CuratedWording {
        kind_id: "twitch.support.gift_sub",
        headline: "%gifter_name% gifted %gift_count% subs!",
        subline: "Thanks for the gifts",
    },
    CuratedWording {
        kind_id: "twitch.support.cheer",
        headline: "%user_name% cheered %bits_amount% bits!",
        subline: "%message_text%",
    },
    CuratedWording {
        kind_id: "twitch.channel.raid_received",
        headline: "%user_name% is raiding!",
        subline: "%viewer_count% viewers incoming",
    },
    CuratedWording {
        kind_id: "twitch.channel.follow",
        headline: "%user_name% just followed!",
        subline: "Thanks for the follow",
    },
    CuratedWording {
        kind_id: "twitch.channel_points.redemption",
        headline: "%user_name% redeemed %reward.title%",
        subline: "%message_text%",
    },
];

struct Wording {
    headline: String,
    subline: String,
}

pub fn accepts_event_wiring(descriptor: &dyn OverlayKindDescriptor) -> bool {
    descriptor.delivery_disposition() == DeliveryDisposition::Transient
        && !descriptor.content_is_machine_filled()
}

pub fn is_curated(trigger_kind_id: &str) -> bool {
    curated_row(trigger_kind_id).is_some()
}

pub fn order_curated_first<T>(offered: &mut [T], kind_id: impl Fn(&T) -> &str) {
    offered.sort_by_key(|item| curated_rank(kind_id(item)));
}

pub fn suggested_content(
    descriptor: &dyn OverlayKindDescriptor,
    trigger: &EventWiringTrigger<'_>,
) -> OverlayConfig {
    let wording = wording(trigger);

    descriptor
        .config_fields()
        .iter()
        .filter(|sectioned| sectioned.section == ConfigSection::Content)
        .filter_map(|sectioned| {
            let key = sectioned.field.key();
            let line = match key {
                config::HEADLINE => wording.headline.as_str(),
                config::SUBLINE => wording.subline.as_str(),
                _ => return None,
            };
            Some((key.to_owned(), config::text(line)))
        })
        .collect()
}

fn wording(trigger: &EventWiringTrigger<'_>) -> Wording {
    let declared: BTreeSet<&str> = trigger
        .variables
        .iter()
        .map(|variable| variable.declared.name.as_str())
        .collect();

    curated_row(trigger.kind_id)
        .filter(|row| row.names_nothing_outside(&declared))
        .map_or_else(|| derived_wording(trigger), CuratedWording::wording)
}

fn derived_wording(trigger: &EventWiringTrigger<'_>) -> Wording {
    let label = trigger.label.to_owned();

    match (
        canonical_name(trigger, PRINCIPAL_DISPLAY_NAME),
        detail_name(trigger),
    ) {
        (Some(actor), Some(detail)) => Wording {
            headline: token(actor),
            subline: format!("{label}: {}", token(detail)),
        },
        (Some(actor), None) => Wording {
            headline: token(actor),
            subline: label,
        },
        (None, Some(detail)) => Wording {
            headline: label,
            subline: token(detail),
        },
        (None, None) => Wording {
            headline: label.clone(),
            subline: label,
        },
    }
}

fn canonical_name<'a>(
    trigger: &EventWiringTrigger<'a>,
    canonical: CanonicalVariable,
) -> Option<&'a str> {
    trigger
        .variables
        .iter()
        .find(|variable| variable.standing == VariableStanding::Canonical(canonical))
        .map(|variable| variable.declared.name.as_str())
}

fn detail_name<'a>(trigger: &EventWiringTrigger<'a>) -> Option<&'a str> {
    trigger
        .variables
        .iter()
        .find(|variable| carries_detail(variable))
        .map(|variable| variable.declared.name.as_str())
}

fn carries_detail(variable: &TriggerVariable) -> bool {
    if variable.declared.synthesis.is_none() {
        return false;
    }
    match variable.standing {
        VariableStanding::Canonical(canonical) => {
            !matches!(canonical, CanonicalVariable::Actor { .. })
        }
        VariableStanding::EventSpecific => true,
        VariableStanding::Legacy(_) => false,
    }
}

fn curated_row(kind_id: &str) -> Option<&'static CuratedWording> {
    CURATED.iter().find(|row| row.kind_id == kind_id)
}

fn curated_rank(kind_id: &str) -> usize {
    CURATED
        .iter()
        .position(|row| row.kind_id == kind_id)
        .unwrap_or(CURATED.len())
}

fn token(name: &str) -> String {
    format!("%{name}%")
}

impl CuratedWording {
    fn names_nothing_outside(&self, declared: &BTreeSet<&str>) -> bool {
        variable_references(self.headline)
            .chain(variable_references(self.subline))
            .all(|name| declared.contains(name))
    }

    fn wording(&self) -> Wording {
        Wording {
            headline: self.headline.to_owned(),
            subline: self.subline.to_owned(),
        }
    }
}
