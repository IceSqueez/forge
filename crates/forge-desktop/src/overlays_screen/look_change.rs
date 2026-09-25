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

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use forge_overlay::config::{ACCENT, DURATION, HEADLINE, ICON, SOUND, SPEECH, SPEECH_VOICE};
    use forge_overlay::kinds::alert::AlertOverlayKind;
    use forge_overlay::kinds::chat::ChatOverlayKind;
    use forge_overlay::{
        ConfigSection, DeliveryDisposition, PageAssets, PreviewComposition, SectionedField,
    };
    use forge_types::Variant;

    use super::*;

    const GUARDED: &str = "outline";
    const GUARDED_VALUE: &str = "outline_width";

    fn stored(keys: &[&str]) -> OverlayConfig {
        keys.iter()
            .map(|key| ((*key).to_owned(), Variant::String(format!("{key}-value"))))
            .collect()
    }

    fn keys(config: &OverlayConfig) -> Vec<&str> {
        config.keys().map(String::as_str).collect()
    }

    struct GuardedLook;

    impl OverlayKindDescriptor for GuardedLook {
        fn id(&self) -> &str {
            "overlay.guarded"
        }

        fn label(&self) -> &str {
            "Guarded"
        }

        fn summary(&self) -> &str {
            ""
        }

        fn icon_name(&self) -> &str {
            ""
        }

        fn delivery_disposition(&self) -> DeliveryDisposition {
            DeliveryDisposition::Replace
        }

        fn order_sensitive(&self) -> bool {
            false
        }

        fn config_schema_version(&self) -> u32 {
            1
        }

        fn look_fields(&self) -> Vec<SectionedField> {
            vec![SectionedField {
                section: ConfigSection::Style,
                field: FormField::Optional {
                    key: GUARDED,
                    label: "Outline",
                    inner: Box::new(FormField::Integer {
                        key: GUARDED_VALUE,
                        label: "Width",
                        min: 0,
                        max: 10,
                    }),
                },
            }]
        }

        fn page_assets(&self) -> forge_overlay::PageAssets {
            PageAssets {
                markup: "",
                style: "",
                behavior: "",
            }
        }

        fn preview(&self, config: &OverlayConfig) -> PreviewComposition {
            AlertOverlayKind.preview(config)
        }
    }

    #[test]
    fn a_look_change_keeps_the_base_and_shared_style_and_drops_what_only_the_old_look_read() {
        let from_alert = stored(&[
            HEADLINE,
            ICON,
            DURATION,
            ACCENT,
            SOUND,
            SPEECH,
            SPEECH_VOICE,
        ]);

        let carried = carried_config(&ChatOverlayKind, &from_alert);

        assert_eq!(
            keys(&carried),
            vec![ACCENT, SOUND, SPEECH, SPEECH_VOICE],
            "a chat look must keep the sound, speech and accent and drop the alert's own fields"
        );
    }

    #[test]
    fn a_carried_value_keeps_what_the_old_look_stored_rather_than_the_new_looks_default() {
        let from_alert = stored(&[ACCENT, SPEECH]);

        let carried = carried_config(&ChatOverlayKind, &from_alert);

        assert_eq!(carried, from_alert);
    }

    #[test]
    fn the_value_an_optional_field_guards_survives_with_its_toggle() {
        let before = stored(&[GUARDED, GUARDED_VALUE]);

        let carried = carried_config(&GuardedLook, &before);

        assert_eq!(keys(&carried), vec![GUARDED, GUARDED_VALUE]);
    }
}
