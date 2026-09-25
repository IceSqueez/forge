use std::collections::HashSet;
use std::sync::Arc;

use forge_components::Icon;
use forge_overlay::{OverlayConfig, OverlayKindDescriptor};
use forge_registry::FormField;
use forge_storage::OverlayId;
use gpui::Context;

use super::base_sections::{BaseLaunch, LookSummary};
use super::{OverlaysView, Regenerated, regenerate};
use crate::async_bridge;

pub(super) fn look_summary(descriptor: &dyn OverlayKindDescriptor) -> LookSummary {
    LookSummary {
        kind_id: descriptor.id().to_owned(),
        label: descriptor.label().to_owned(),
        summary: descriptor.summary().to_owned(),
        icon: Icon::from_name(descriptor.icon_name()),
        disposition: descriptor.delivery_disposition(),
        draws: descriptor.has_visual_page(),
    }
}

/// Keeps every stored value the new look also declares - the base's audio and display settings
/// and the shared style - and drops what only the old look understood.
pub(super) fn carried_config(
    descriptor: &dyn OverlayKindDescriptor,
    config: &OverlayConfig,
) -> OverlayConfig {
    let mut declared = HashSet::new();
    for sectioned in descriptor.config_fields() {
        declared_keys(&sectioned.field, &mut declared);
    }
    config
        .iter()
        .filter(|(key, _)| declared.contains(key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn declared_keys(field: &FormField, out: &mut HashSet<&'static str>) {
    out.insert(field.key());
    if let FormField::Optional { inner, .. } = field {
        declared_keys(inner, out);
    }
}

impl OverlaysView {
    pub(super) fn base_launch(
        &self,
        descriptor: &dyn OverlayKindDescriptor,
        id: &OverlayId,
    ) -> BaseLaunch {
        let mut looks: Vec<LookSummary> = self.kinds.all().map(look_summary).collect();
        looks.sort_by(|a, b| a.label.cmp(&b.label));
        BaseLaunch {
            look: look_summary(descriptor),
            looks,
            receiver: self.receiver.as_ref() == Some(id),
        }
    }

    pub(super) fn change_look(
        &mut self,
        id: OverlayId,
        kind_id: String,
        config: OverlayConfig,
        cx: &mut Context<Self>,
    ) {
        let Some(descriptor) = self.kinds.get(&kind_id) else {
            self.report(&forge_components::tr!("overlays_toast_unknown_type"), cx);
            return;
        };
        let carried = carried_config(descriptor, &config);
        let schema_version = descriptor.config_schema_version();
        let repo = Arc::clone(&self.repo);
        let service = self.service.clone();
        let target = id.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let Some(mut definition) = repo.get(&id).await.map_err(|e| e.to_string())? else {
                    return Ok((false, Regenerated::default()));
                };
                definition.kind_id = kind_id;
                definition.config = carried;
                definition.config_schema_version = schema_version;
                repo.save(&definition).await.map_err(|e| e.to_string())?;
                Ok((true, regenerate(&service, &id).await))
            },
            move |this, result: Result<(bool, Regenerated), String>, cx| match result {
                Ok((true, regenerated)) => {
                    this.apply_regenerated(&target, regenerated, cx);
                    this.load(cx);
                }
                Ok((false, _)) => this.report(&forge_components::tr!("overlays_toast_missing"), cx),
                Err(message) => this.report(&message, cx),
            },
            cx,
        );
    }
}
