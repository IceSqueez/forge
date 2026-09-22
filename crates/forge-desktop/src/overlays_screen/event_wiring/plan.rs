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
