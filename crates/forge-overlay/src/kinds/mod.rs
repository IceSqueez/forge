pub mod alert;
pub mod chat;
pub mod frame;
pub mod goal;
pub mod ticker;

use crate::error::OverlayError;
use crate::registry::OverlayKindRegistry;

pub fn register_builtin_kinds(reg: &mut OverlayKindRegistry) -> Result<(), OverlayError> {
    reg.register(Box::new(alert::AlertOverlayKind))?;
    reg.register(Box::new(chat::ChatOverlayKind))?;
    reg.register(Box::new(frame::FrameOverlayKind))?;
    reg.register(Box::new(goal::GoalOverlayKind))?;
    reg.register(Box::new(ticker::TickerOverlayKind))?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use std::collections::BTreeSet;

    use forge_registry::FormField;

    use super::*;
    use crate::config::validate_overlay_config;
    use crate::preview::PreviewShape;

    const BUILTIN_IDS: &[&str] = &[
        "overlay.alert",
        "overlay.chat",
        "overlay.frame",
        "overlay.goal",
        "overlay.ticker",
    ];

    fn registry() -> OverlayKindRegistry {
        let mut reg = OverlayKindRegistry::new();
        register_builtin_kinds(&mut reg).expect("register the builtin kinds");
        reg
    }

    fn field_key(field: &FormField) -> &'static str {
        match field {
            FormField::Text { key, .. }
            | FormField::TextArea { key, .. }
            | FormField::Code { key, .. }
            | FormField::Integer { key, .. }
            | FormField::Slider { key, .. }
            | FormField::Toggle { key, .. }
            | FormField::FilePicker { key, .. }
            | FormField::DateTime { key, .. }
            | FormField::Select { key, .. }
            | FormField::DynamicSelect { key, .. }
            | FormField::DependentSelect { key, .. }
            | FormField::Swatch { key, .. }
            | FormField::Optional { key, .. }
            | FormField::SubChain { key, .. }
            | FormField::CaseList { key, .. } => key,
        }
    }

    #[test]
    fn builtin_kinds_register_under_the_ids_stored_configs_reference() {
        let reg = registry();

        for id in BUILTIN_IDS {
            assert!(reg.get(id).is_some(), "no overlay kind registered as {id}");
        }
    }

    #[test]
    fn every_builtin_default_config_satisfies_its_own_field_constraints() {
        for descriptor in registry().all() {
            validate_overlay_config(descriptor, &descriptor.default_config()).unwrap_or_else(|e| {
                panic!("the {} default config is invalid: {e}", descriptor.id())
            });
        }
    }

    #[test]
    fn every_builtin_defaults_exactly_the_keys_its_form_declares() {
        for descriptor in registry().all() {
            let declared: BTreeSet<&str> = descriptor
                .config_fields()
                .iter()
                .map(|f| field_key(&f.field))
                .collect();
            let defaults = descriptor.default_config();
            let defaulted: BTreeSet<&str> = defaults.keys().map(String::as_str).collect();

            assert_eq!(
                defaulted,
                declared,
                "{} defaults and form fields disagree",
                descriptor.id()
            );
        }
    }

    #[test]
    fn every_builtin_kind_previews_a_distinct_shape() {
        let reg = registry();
        let shapes: Vec<PreviewShape> = BUILTIN_IDS
            .iter()
            .map(|id| {
                let descriptor = reg.get(id).expect("registered kind");
                descriptor.preview(&descriptor.default_config()).shape
            })
            .collect();

        for (index, shape) in shapes.iter().enumerate() {
            for (other_index, other) in shapes.iter().enumerate().skip(index + 1) {
                assert_ne!(
                    shape, other,
                    "{} and {} preview as the same shape",
                    BUILTIN_IDS[index], BUILTIN_IDS[other_index]
                );
            }
        }
    }
}
