use std::collections::HashMap;

use forge_components::{ForgePalette, GridPickerGroup, GridPickerItem, GridScope, tr};
use forge_overlay::{OverlayConfig, is_curated, order_curated_first};
use forge_registry::{FormField, TriggerKindDescriptor, TriggerRegistry};
use forge_runtime::{
    OVERLAY_SEND_KIND_ID, OverlayFeed, OverlayWiringPlan, OverlayWiringRefusal, PlannedAction,
    PlannedTrigger,
};
use forge_storage::OverlayDefinition;
use forge_types::{ActionId, Queue, QueueId, Variant};
use gpui::SharedString;

use crate::automation::triggers_screen::{build_kind_groups, platform_kind_line};
use crate::shared::config_field_label::{field_label, field_placeholder};

const SUGGESTED_SCOPE: &str = "suggested";

pub(super) struct FeedRow {
    pub action_id: ActionId,
    pub action_name: String,
    pub action_enabled: bool,
    pub trigger_labels: Vec<String>,
}

pub(super) enum ActionRecord {
    Create { name: String, queue: String },
    Extend { name: String },
}

pub(super) enum TriggerRecord {
    Create(String),
    Reuse(String),
}

pub(super) struct WiringDraft {
    pub action: ActionRecord,
    pub trigger: TriggerRecord,
    pub plan: OverlayWiringPlan,
}

pub(super) struct WordingSpec {
    pub key: String,
    pub label: String,
    pub placeholder: String,
    pub seed: String,
}

pub(super) fn feed_rows(feeds: Vec<OverlayFeed>, registry: &TriggerRegistry) -> Vec<FeedRow> {
    feeds
        .into_iter()
        .map(|feed| FeedRow {
            action_id: feed.action_id,
            action_name: feed.action_name,
            action_enabled: feed.action_enabled,
            trigger_labels: feed
                .triggers
                .iter()
                .map(|instance| {
                    registry.get(&instance.kind_id).map_or_else(
                        || instance.kind_id.clone(),
                        crate::shared::kind_copy::trigger_name,
                    )
                })
                .collect(),
        })
        .collect()
}

pub(super) fn feed_summary(feeds: &[FeedRow]) -> String {
    let labels: Vec<&str> = feeds
        .iter()
        .flat_map(|row| row.trigger_labels.iter())
        .map(String::as_str)
        .collect();
    if labels.is_empty() {
        return tr!("overlays_wire_fires_untriggered");
    }
    let joined = labels.join(", ");
    tr!("overlays_wire_fires_on", triggers = joined.as_str())
}

pub(super) fn offered_events(
    registry: &TriggerRegistry,
    palette: &ForgePalette,
) -> (Vec<GridPickerGroup>, HashMap<SharedString, String>) {
    let (mut groups, picks) = build_kind_groups(registry, palette);
    let mut suggested: Vec<(String, GridPickerItem)> = Vec::new();

    for group in &mut groups {
        let mut kept: Vec<GridPickerItem> = Vec::with_capacity(group.items.len());
        for item in group.items.drain(..) {
            let Some(kind_id) = picks.get(&item.id) else {
                continue;
            };
            if is_curated(kind_id) {
                let name = platform_kind_line(kind_id, registry).into();
                suggested.push((kind_id.clone(), GridPickerItem { name, ..item }));
            } else {
                kept.push(item);
            }
        }
        group.items = kept;
    }
    groups.retain(|group| !group.items.is_empty());

    order_curated_first(&mut suggested, |(kind_id, _)| kind_id.as_str());
    if !suggested.is_empty() {
        groups.insert(
            0,
            GridPickerGroup {
                label: tr!("overlays_wire_scope_suggested").into(),
                dot_color: palette.brand,
                scope: GridScope::Named(SharedString::from(SUGGESTED_SCOPE)),
                items: suggested.into_iter().map(|(_, item)| item).collect(),
            },
        );
    }

    (groups, picks)
}

pub(super) fn default_action_name(
    overlay: &OverlayDefinition,
    trigger: &dyn TriggerKindDescriptor,
) -> String {
    format!("{} on {}", overlay.display_name, trigger.label())
}

pub(super) fn wording_specs(fields: &[FormField], suggested: &OverlayConfig) -> Vec<WordingSpec> {
    fields
        .iter()
        .filter_map(|spec| match spec {
            FormField::Text {
                key, placeholder, ..
            } => Some((*key, *placeholder)),
            FormField::TextArea { key, .. } => Some((*key, "")),
            _ => None,
        })
        .map(|(key, placeholder)| WordingSpec {
            key: key.to_owned(),
            label: fields.iter().find(|spec| spec.key() == key).map_or_else(
                || key.to_owned(),
                |spec| field_label(OVERLAY_SEND_KIND_ID, spec),
            ),
            placeholder: field_placeholder(OVERLAY_SEND_KIND_ID, key, placeholder),
            seed: suggested
                .get(key)
                .and_then(Variant::as_str)
                .unwrap_or_default()
                .to_owned(),
        })
        .collect()
}

pub(super) fn written_content<'a>(
    lines: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> OverlayConfig {
    lines
        .into_iter()
        .filter(|(_, text)| !text.trim().is_empty())
        .map(|(key, text)| (key.to_owned(), Variant::String(text.to_owned())))
        .collect()
}

pub(super) fn queue_choices(queues: Vec<Queue>) -> Vec<(QueueId, String)> {
    let mut choices: Vec<(QueueId, String)> = queues
        .into_iter()
        .filter(|queue| !queue.is_default())
        .map(|queue| (queue.id, queue.name))
        .collect();
    choices.sort_by_key(|(_, name)| name.to_lowercase());
    choices
}

pub(super) fn draft_wiring(plan: OverlayWiringPlan, registry: &TriggerRegistry) -> WiringDraft {
    let action = match plan.action() {
        PlannedAction::Create { action, queue } => ActionRecord::Create {
            name: action.name.clone(),
            queue: queue.name.clone(),
        },
        PlannedAction::Extend { action } => ActionRecord::Extend {
            name: action.name.clone(),
        },
    };
    let trigger_name = |instance: &forge_types::TriggerInstance| {
        crate::shared::kind_copy::trigger_instance_name(
            &instance.name,
            registry.get(&instance.kind_id),
        )
    };
    let trigger = match plan.trigger() {
        PlannedTrigger::Create(instance) => TriggerRecord::Create(trigger_name(instance)),
        PlannedTrigger::Reuse(instance) => TriggerRecord::Reuse(trigger_name(instance)),
    };
    WiringDraft {
        action,
        trigger,
        plan,
    }
}

pub(super) fn refusal_message(refusal: &OverlayWiringRefusal) -> String {
    match refusal {
        OverlayWiringRefusal::OverlayKindNotEligible { .. } => tr!("overlays_wire_refused_overlay"),
        OverlayWiringRefusal::ActionNameEmpty => tr!("overlays_wire_refused_name"),
        OverlayWiringRefusal::ActionMissing { .. } => tr!("overlays_wire_refused_action"),
        OverlayWiringRefusal::QueueMissing { .. } => tr!("overlays_wire_refused_queue"),
        OverlayWiringRefusal::TargetMissing { .. } | OverlayWiringRefusal::PlanOutdated => {
            tr!("overlays_wire_refused_missing")
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use forge_components::ThemeId;
    use forge_registry::TriggerCategory;
    use forge_storage::Language;
    use forge_types::{
        ActionId, PermissionRung, PlatformScope, TriggerConfig, TriggerInstance, TriggerInstanceId,
    };

    use super::*;
    use crate::app::i18n::{install_language, message_in};
    use crate::test_support::{Declares, StubTrigger, trigger_registry};

    const SUBSCRIBER: &str = "twitch.support.subscriber";
    const FOLLOW: &str = "twitch.channel.follow";
    const PLAIN: &str = "stub.plain_chat";
    const SILENT: &str = "stub.silent";
    const BARREN: &str = "stub.barren";
    const GONE: &str = "vendor.kind_this_build_lacks";

    fn en() {
        install_language(Language::En);
    }

    fn card(kind_id: &str) -> String {
        format!("kind-{kind_id}")
    }

    fn speaks(id: &str, label: &str, category: TriggerCategory) -> StubTrigger {
        StubTrigger::new(id, label, category, Declares::APrincipal)
    }

    fn offered(stubs: Vec<StubTrigger>) -> Vec<(String, Vec<String>)> {
        let registry = trigger_registry(stubs);
        let (groups, _) = offered_events(&registry, &ThemeId::ForgeDefault.palette());
        groups
            .into_iter()
            .map(|group| {
                (
                    group.scope.name().unwrap_or_default().to_owned(),
                    group
                        .items
                        .into_iter()
                        .map(|item| item.id.to_string())
                        .collect(),
                )
            })
            .collect()
    }

    fn instance(kind_id: &str) -> TriggerInstance {
        TriggerInstance {
            id: TriggerInstanceId::new(),
            kind_id: kind_id.to_owned(),
            name: kind_id.to_owned(),
            overrides: TriggerConfig::new(),
            enabled: true,
            user_defined: true,
            platform_scope: PlatformScope::Any,
            cooldown_secs: 0,
            cooldown_global: true,
            permission_rung: PermissionRung::Everyone,
        }
    }

    fn feed(name: &str, kind_ids: &[&str]) -> OverlayFeed {
        OverlayFeed {
            action_id: ActionId::new(),
            action_name: name.to_owned(),
            action_enabled: true,
            triggers: kind_ids.iter().map(|kind_id| instance(kind_id)).collect(),
        }
    }

    fn row(labels: &[&str]) -> FeedRow {
        FeedRow {
            action_id: ActionId::new(),
            action_name: "Alert".to_owned(),
            action_enabled: true,
            trigger_labels: labels.iter().map(|label| (*label).to_owned()).collect(),
        }
    }

    #[test]
    fn an_event_that_declares_no_variables_is_still_offered() {
        en();

        let groups = offered(vec![
            StubTrigger::new(SILENT, "Silent", TriggerCategory::Chat, Declares::Nothing),
            StubTrigger::new(
                BARREN,
                "Barren",
                TriggerCategory::Chat,
                Declares::AnEmptyList,
            ),
        ]);

        let offered_ids: BTreeSet<String> = groups.into_iter().flat_map(|(_, ids)| ids).collect();
        assert_eq!(offered_ids, BTreeSet::from([card(SILENT), card(BARREN)]));
    }

    #[test]
    fn the_queue_choices_leave_out_the_default_queue_and_sort_by_name_ignoring_case() {
        let queue = |id: QueueId, name: &str| Queue {
            id,
            name: name.to_owned(),
            description: String::new(),
            concurrency: std::num::NonZeroU32::MIN,
        };
        let zeta = QueueId::new();
        let alpha = QueueId::new();

        let choices = queue_choices(vec![
            queue(zeta, "zeta"),
            queue(forge_types::DEFAULT_QUEUE_ID, "Default"),
            queue(alpha, "Alpha"),
        ]);

        assert_eq!(
            choices,
            vec![(alpha, "Alpha".to_owned()), (zeta, "zeta".to_owned())]
        );
    }

    #[test]
    fn a_curated_event_is_moved_into_a_leading_suggested_group_rather_than_copied() {
        en();

        let groups = offered(vec![
            speaks(SUBSCRIBER, "Subscribed", TriggerCategory::Subscriptions),
            speaks(PLAIN, "Plain chat", TriggerCategory::Chat),
        ]);

        assert_eq!(
            groups
                .first()
                .map(|(scope, ids)| (scope.as_str(), ids.clone())),
            Some((SUGGESTED_SCOPE, vec![card(SUBSCRIBER)])),
            "the curated event does not head the offer",
        );
        let offered_ids: Vec<&String> = groups.iter().flat_map(|(_, ids)| ids).collect();
        let distinct: BTreeSet<&&String> = offered_ids.iter().collect();
        assert_eq!(
            offered_ids.len(),
            distinct.len(),
            "a card was copied into Suggested instead of moved: {offered_ids:?}",
        );
        assert!(
            groups[1..].iter().any(|(_, ids)| ids == &vec![card(PLAIN)]),
            "the uncurated event lost its own category",
        );
    }

    #[test]
    fn the_suggested_group_lists_the_curated_events_in_the_curated_order() {
        en();

        let groups = offered(vec![
            speaks(FOLLOW, "Aaa followed", TriggerCategory::Subscriptions),
            speaks(SUBSCRIBER, "Zzz subscribed", TriggerCategory::Subscriptions),
        ]);

        assert_eq!(
            groups.first().map(|(_, ids)| ids.clone()),
            Some(vec![card(SUBSCRIBER), card(FOLLOW)]),
            "Suggested kept the catalogue's alphabetical order instead of the curated one",
        );
    }

    #[test]
    fn a_suggested_event_is_named_after_its_platform() {
        en();
        let registry = trigger_registry(vec![speaks(
            SUBSCRIBER,
            "Subscribed",
            TriggerCategory::Subscriptions,
        )]);

        let (groups, _) = offered_events(&registry, &ThemeId::ForgeDefault.palette());

        assert_eq!(
            groups
                .first()
                .and_then(|group| group.items.first())
                .map(|item| item.name.replace(crate::test_support::FORMAT_ISOLATES, "")),
            Some("Twitch \u{b7} New subscriber".to_owned())
        );
    }

    #[test]
    fn a_feeds_events_are_labelled_by_the_registry_and_an_unknown_kind_keeps_its_id() {
        let registry = trigger_registry(vec![speaks(
            FOLLOW,
            "Followed",
            TriggerCategory::Subscriptions,
        )]);

        let rows = feed_rows(vec![feed("Alert on follow", &[FOLLOW, GONE])], &registry);

        assert_eq!(
            rows.first().map(|row| row.trigger_labels.clone()),
            Some(vec!["Followed".to_owned(), GONE.to_owned()]),
        );
    }

    #[test]
    fn the_summary_names_every_event_that_drives_the_overlay() {
        en();

        let summary = feed_summary(&[row(&["Followed"]), row(&["Subscribed", "Cheered"])]);

        assert!(
            summary.contains("Followed, Subscribed, Cheered"),
            "the summary dropped or reordered an event: {summary}",
        );
    }

    #[test]
    fn a_summary_of_actions_no_event_drives_reads_differently() {
        en();

        let untriggered = feed_summary(&[row(&[])]);

        assert_eq!(untriggered, feed_summary(&[]));
        assert!(
            untriggered.contains("no event"),
            "an overlay no event drives reads as a driven one with the events left out: {untriggered}",
        );
    }

    #[test]
    fn each_refusal_explains_itself_in_its_own_words() {
        install_language(Language::En);
        for (refusal, key) in [
            (
                OverlayWiringRefusal::OverlayKindNotEligible {
                    overlay_kind_id: "blank".to_owned(),
                },
                "overlays_wire_refused_overlay",
            ),
            (
                OverlayWiringRefusal::TargetMissing {
                    trigger_kind_id: SILENT.to_owned(),
                },
                "overlays_wire_refused_missing",
            ),
        ] {
            assert_eq!(
                refusal_message(&refusal),
                message_in(Language::En, key),
                "{refusal:?}"
            );
        }
    }
}
