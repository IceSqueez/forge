pub mod alert;
pub mod blank;
pub mod card;
pub mod chat;
pub mod frame;
pub mod goal;
pub mod latest;
pub mod ticker;

use crate::catalog::registry::OverlayKindRegistry;
use crate::error::OverlayError;

pub fn register_builtin_kinds(reg: &mut OverlayKindRegistry) -> Result<(), OverlayError> {
    reg.register(Box::new(alert::AlertOverlayKind))?;
    reg.register(Box::new(blank::BlankOverlayKind))?;
    reg.register(Box::new(card::CardOverlayKind))?;
    reg.register(Box::new(chat::ChatOverlayKind))?;
    reg.register(Box::new(frame::FrameOverlayKind))?;
    reg.register(Box::new(goal::GoalOverlayKind))?;
    reg.register(Box::new(latest::LatestOverlayKind))?;
    reg.register(Box::new(ticker::TickerOverlayKind))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::catalog::descriptor::OverlayConfig;
    use crate::config::{effective_overlay_config, validate_overlay_config};

    const BUILTIN_IDS: &[&str] = &[
        "overlay.alert",
        "overlay.blank",
        "overlay.card",
        "overlay.chat",
        "overlay.frame",
        "overlay.goal",
        "overlay.latest",
        "overlay.ticker",
    ];

    fn registry() -> OverlayKindRegistry {
        let mut reg = OverlayKindRegistry::new();
        register_builtin_kinds(&mut reg).expect("register the builtin kinds");
        reg
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
    fn every_builtin_defaults_the_keys_its_form_declares() {
        for descriptor in registry().all() {
            let declared: BTreeSet<&str> = descriptor
                .config_fields()
                .iter()
                .map(|f| f.field.key())
                .collect();
            let defaults = descriptor.default_config();
            let defaulted: BTreeSet<&str> = defaults.keys().map(String::as_str).collect();

            let undeclared: Vec<&&str> = defaulted.difference(&declared).collect();
            assert!(
                undeclared.is_empty(),
                "{} defaults {undeclared:?}, which no form field offers",
                descriptor.id()
            );

            let undefaulted: Vec<&&str> = declared.difference(&defaulted).collect();
            assert!(
                undefaulted.is_empty(),
                "{} leaves {undefaulted:?} without a default",
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
                .preview(
                    &effective_overlay_config(descriptor, &OverlayConfig::new()),
                    forge_types::NamesLanguage::En,
                )
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
