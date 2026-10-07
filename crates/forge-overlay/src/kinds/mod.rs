pub mod alert;
pub mod blank;
pub mod chat;
pub mod frame;
pub mod goal;
pub mod latest;
pub mod ticker;

use crate::error::OverlayError;
use crate::registry::OverlayKindRegistry;

pub fn register_builtin_kinds(reg: &mut OverlayKindRegistry) -> Result<(), OverlayError> {
    reg.register(Box::new(alert::AlertOverlayKind))?;
    reg.register(Box::new(blank::BlankOverlayKind))?;
    reg.register(Box::new(chat::ChatOverlayKind))?;
    reg.register(Box::new(frame::FrameOverlayKind))?;
    reg.register(Box::new(goal::GoalOverlayKind))?;
    reg.register(Box::new(latest::LatestOverlayKind))?;
    reg.register(Box::new(ticker::TickerOverlayKind))?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use std::collections::BTreeSet;

    use forge_registry::FormField;

    use super::*;
    use crate::config::{
        ELEMENT_HEIGHT, ELEMENT_WIDTH, effective_overlay_config, validate_overlay_config,
    };
    use crate::descriptor::OverlayConfig;

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
            | FormField::UnitAmount { key, .. }
            | FormField::Duration { key, .. }
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
    fn every_builtin_defaults_the_keys_its_form_declares_apart_from_the_sides_a_page_may_size() {
        let optional: BTreeSet<&str> = BTreeSet::from([ELEMENT_WIDTH, ELEMENT_HEIGHT]);

        for descriptor in registry().all() {
            let declared: BTreeSet<&str> = descriptor
                .config_fields()
                .iter()
                .map(|f| field_key(&f.field))
                .collect();
            let defaults = descriptor.default_config();
            let defaulted: BTreeSet<&str> = defaults.keys().map(String::as_str).collect();

            let undeclared: Vec<&&str> = defaulted.difference(&declared).collect();
            assert!(
                undeclared.is_empty(),
                "{} defaults {undeclared:?}, which no form field offers",
                descriptor.id()
            );

            let undefaulted: BTreeSet<&str> = declared.difference(&defaulted).copied().collect();
            let expected: BTreeSet<&str> = declared.intersection(&optional).copied().collect();
            assert_eq!(
                undefaulted,
                expected,
                "{} leaves a field without a default, or hands one to a side the page sizes itself",
                descriptor.id()
            );
        }
    }

    #[test]
    fn every_look_that_draws_a_page_previews_in_a_shape_no_other_look_draws() {
        let reg = registry();
        let mut shapes = Vec::new();

        for descriptor in reg.all().filter(|descriptor| descriptor.has_visual_page()) {
            let shape = descriptor
                .preview(&effective_overlay_config(descriptor, &OverlayConfig::new()))
                .shape;
            if let Some((other, _)) = shapes.iter().find(|(_, drawn)| *drawn == shape) {
                panic!(
                    "{} previews as the same {shape:?} as {other}, so the editor draws it with the \
                     other look's geometry instead of its own page",
                    descriptor.id()
                );
            }
            shapes.push((descriptor.id().to_owned(), shape));
        }
    }
}
