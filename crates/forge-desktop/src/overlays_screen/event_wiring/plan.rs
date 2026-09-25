use std::collections::HashMap;

use forge_components::{ForgePalette, GridPickerGroup, GridPickerItem, tr};
use forge_overlay::{
    ConfigSection, OverlayConfig, OverlayKindDescriptor, SampleTrigger, delivered_content,
    is_curated, order_curated_first, sample_context,
};
use forge_registry::{TriggerKindDescriptor, TriggerRegistry, declared_variables};
use forge_runtime::actions::{
    OverlayFeed, OverlayWiringRecords, OverlayWiringRefusal, plan_overlay_wiring,
};
use forge_storage::OverlayDefinition;
use forge_types::{ActionId, Variant};
use gpui::SharedString;

use crate::triggers_screen::build_kind_groups;

const SUGGESTED_SCOPE: &str = "suggested";

pub(super) struct FeedRow {
    pub action_id: ActionId,
    pub action_name: String,
    pub action_enabled: bool,
    pub trigger_labels: Vec<String>,
}

pub(super) struct WiringDraft {
    pub action_name: String,
    pub trigger_name: String,
    pub queue_name: String,
    pub wording: Vec<String>,
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
                    registry
                        .get(&instance.kind_id)
                        .map_or_else(|| instance.kind_id.clone(), |held| held.label().to_owned())
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

pub(super) fn declares_variables(descriptor: &dyn TriggerKindDescriptor) -> bool {
    declared_variables(descriptor).is_some_and(|declared| !declared.is_empty())
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
            if !registry.get(kind_id).is_some_and(declares_variables) {
                continue;
            }
            if is_curated(kind_id) {
                suggested.push((kind_id.clone(), item));
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
                scope: SharedString::from(SUGGESTED_SCOPE),
                items: suggested.into_iter().map(|(_, item)| item).collect(),
            },
        );
    }

    (groups, picks)
}

pub(super) fn draft_wiring(
    overlay: &OverlayDefinition,
    kind: &dyn OverlayKindDescriptor,
    trigger: &dyn TriggerKindDescriptor,
) -> Result<WiringDraft, OverlayWiringRefusal> {
    let plan = plan_overlay_wiring(overlay, kind, trigger)?;
    let supplied: OverlayConfig = plan
        .action
        .steps
        .first()
        .map(|step| step.config.clone())
        .unwrap_or_default();
    let context = sample_context(&[SampleTrigger {
        kind_id: trigger.id().to_owned(),
        contract: trigger.platform_contract(),
        variables: declared_variables(trigger).unwrap_or_default(),
    }]);

    Ok(WiringDraft {
        action_name: plan.action.name,
        trigger_name: plan.trigger.name,
        queue_name: plan.queue.name.to_owned(),
        wording: content_lines(
            kind,
            &delivered_content(kind, &overlay.config, &supplied, &context.args()),
        ),
    })
}

fn content_lines(kind: &dyn OverlayKindDescriptor, content: &OverlayConfig) -> Vec<String> {
    kind.config_fields()
        .iter()
        .filter(|sectioned| sectioned.section == ConfigSection::Content)
        .filter_map(|sectioned| {
            content
                .get(sectioned.field.key())
                .and_then(Variant::as_str)
                .filter(|line| !line.is_empty())
                .map(ToOwned::to_owned)
        })
        .collect()
}

pub(super) fn refusal_message(refusal: &OverlayWiringRefusal) -> String {
    match refusal {
        OverlayWiringRefusal::OverlayKindNotEligible { .. } => tr!("overlays_wire_refused_overlay"),
        OverlayWiringRefusal::TriggerDeclaresNoVariables { .. } => {
            tr!("overlays_wire_refused_trigger")
        }
    }
}

pub(super) fn landed_note(records: &OverlayWiringRecords) -> String {
    let mut landed: Vec<String> = Vec::new();
    if records.action_id.is_some() {
        landed.push(tr!("overlays_wire_landed_action"));
    }
    if records.trigger_instance_id.is_some() {
        landed.push(tr!("overlays_wire_landed_trigger"));
    }
    if records.linked {
        landed.push(tr!("overlays_wire_landed_link"));
    }
    if landed.is_empty() {
        return tr!("overlays_wire_landed_nothing");
    }
    landed.join(", ")
}

#[cfg(test)]
#[allow(clippy::panic, clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeSet;

    use forge_components::ThemeId;
    use forge_overlay::config;
    use forge_overlay::kinds::alert::{self, AlertOverlayKind};
    use forge_overlay::kinds::blank::{self, BlankOverlayKind};
    use forge_registry::TriggerCategory;
    use forge_storage::Language;
    use forge_types::{
        ActionId, PermissionRung, PlatformScope, TriggerConfig, TriggerInstance, TriggerInstanceId,
    };

    use super::*;
    use crate::i18n::install_language;
    use crate::test_support::{Declares, StubTrigger, overlay_definition, trigger_registry};

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
                    group.scope.to_string(),
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

    fn text(value: &str) -> Variant {
        Variant::String(value.to_owned())
    }

    #[test]
    fn an_event_with_nothing_an_overlay_could_show_is_never_offered() {
        en();

        let groups = offered(vec![
            speaks(SUBSCRIBER, "Subscribed", TriggerCategory::Subscriptions),
            StubTrigger::new(SILENT, "Silent", TriggerCategory::Chat, Declares::Nothing),
            StubTrigger::new(
                BARREN,
                "Barren",
                TriggerCategory::Chat,
                Declares::AnEmptyList,
            ),
        ]);

        assert_eq!(
            groups,
            vec![(SUGGESTED_SCOPE.to_owned(), vec![card(SUBSCRIBER)])],
            "an event declaring nothing reached the picker, or its emptied category stayed behind",
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
    fn a_plan_the_wiring_refuses_is_handed_back_with_the_cause_it_named() {
        let speaking = speaks(SUBSCRIBER, "Subscribed", TriggerCategory::Subscriptions);
        let silent = StubTrigger::new(SILENT, "Silent", TriggerCategory::Chat, Declares::Nothing);

        let Err(drawless) = draft_wiring(
            &overlay_definition(blank::KIND_ID, OverlayConfig::new()),
            &BlankOverlayKind,
            &speaking,
        ) else {
            panic!("a look that draws nothing was drafted a wiring")
        };
        let Err(wordless) = draft_wiring(
            &overlay_definition(alert::KIND_ID, OverlayConfig::new()),
            &AlertOverlayKind,
            &silent,
        ) else {
            panic!("an event declaring nothing was drafted a wiring")
        };

        assert!(matches!(
            drawless,
            OverlayWiringRefusal::OverlayKindNotEligible { .. }
        ));
        assert!(matches!(
            wordless,
            OverlayWiringRefusal::TriggerDeclaresNoVariables { .. }
        ));
    }

    #[test]
    fn the_preview_reads_the_plans_own_wording_with_its_tokens_filled_in() {
        let stored = OverlayConfig::from([
            (config::HEADLINE.to_owned(), text("Stored headline")),
            (config::SUBLINE.to_owned(), text("Stored subline")),
        ]);

        let draft = draft_wiring(
            &overlay_definition(alert::KIND_ID, stored),
            &AlertOverlayKind,
            &speaks(SUBSCRIBER, "Subscribed", TriggerCategory::Subscriptions),
        )
        .unwrap_or_else(|_| panic!("a curated event on an alert overlay is wireable"));

        assert!(
            draft
                .wording
                .first()
                .is_some_and(|line| line.ends_with(" just subscribed!")),
            "the preview is not the wording the plan writes: {:?}",
            draft.wording,
        );
        assert!(
            !draft.wording.iter().any(|line| line.contains('%')),
            "the preview left a raw token where a sampled value belongs: {:?}",
            draft.wording,
        );
        assert!(
            !draft.wording.iter().any(|line| line.contains("Stored")),
            "the preview showed the stored line the wiring is about to override: {:?}",
            draft.wording,
        );
    }

    #[test]
    fn the_wording_block_keeps_the_kinds_content_order_and_skips_a_line_left_blank() {
        for (headline, subline, expected) in [
            ("Lead", "Detail", vec!["Lead", "Detail"]),
            ("", "Detail", vec!["Detail"]),
            ("Lead", "", vec!["Lead"]),
            ("", "", Vec::new()),
        ] {
            let content = OverlayConfig::from([
                (config::HEADLINE.to_owned(), text(headline)),
                (config::SUBLINE.to_owned(), text(subline)),
                (config::ACCENT.to_owned(), text("mauve")),
            ]);

            assert_eq!(
                content_lines(&AlertOverlayKind, &content),
                expected,
                "headline {headline:?} with subline {subline:?} read wrong",
            );
        }
    }

    #[test]
    fn the_landed_note_names_exactly_the_records_that_landed() {
        en();
        let action = ActionId::new();
        let trigger = TriggerInstanceId::new();

        for (records, expected) in [
            (OverlayWiringRecords::default(), "nothing"),
            (
                OverlayWiringRecords {
                    action_id: Some(action),
                    ..OverlayWiringRecords::default()
                },
                "the action",
            ),
            (
                OverlayWiringRecords {
                    trigger_instance_id: Some(trigger),
                    ..OverlayWiringRecords::default()
                },
                "the trigger",
            ),
            (
                OverlayWiringRecords {
                    action_id: Some(action),
                    trigger_instance_id: Some(trigger),
                    linked: true,
                    ..OverlayWiringRecords::default()
                },
                "the action, the trigger, the link between them",
            ),
        ] {
            assert_eq!(landed_note(&records), expected);
        }
    }
}
