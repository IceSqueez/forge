use forge_types::{ArgStack, Variant};

use crate::config::effective_overlay_config;
use crate::descriptor::{ConfigSection, OverlayConfig, OverlayKindDescriptor};

/// A supplied value wins unless it is an empty string; every other content key falls back to the
/// overlay's own configured value, and both sides expand against the same stack. Keys the kind
/// does not declare as content are dropped, and a key neither side holds is omitted rather than
/// invented.
pub fn delivered_content(
    descriptor: &dyn OverlayKindDescriptor,
    stored: &OverlayConfig,
    supplied: &OverlayConfig,
    args: &ArgStack,
) -> OverlayConfig {
    let configured = effective_overlay_config(descriptor, stored);

    descriptor
        .config_fields()
        .iter()
        .filter(|sectioned| sectioned.section == ConfigSection::Content)
        .filter_map(|sectioned| {
            let key = sectioned.field.key();
            let value = supplied
                .get(key)
                .filter(|value| !is_blank(value))
                .or_else(|| configured.get(key))?;
            Some((key.to_owned(), expanded(value, args)))
        })
        .collect()
}

fn is_blank(value: &Variant) -> bool {
    matches!(value, Variant::String(text) if text.is_empty())
}

fn expanded(value: &Variant, args: &ArgStack) -> Variant {
    match value {
        Variant::String(template) => Variant::String(args.interpolate(template)),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use crate::kinds::alert::AlertOverlayKind;

    fn args(pairs: &[(&str, &str)]) -> ArgStack {
        pairs.iter().fold(ArgStack::new(), |stack, (name, value)| {
            stack.set((*name).to_owned(), Variant::String((*value).to_owned()))
        })
    }

    fn one(key: &str, value: &str) -> OverlayConfig {
        OverlayConfig::from([(key.to_owned(), Variant::String(value.to_owned()))])
    }

    fn text_at(content: &OverlayConfig, key: &str) -> String {
        content
            .get(key)
            .and_then(Variant::as_str)
            .unwrap_or_default()
            .to_owned()
    }

    #[test]
    fn a_supplied_value_wins_unless_the_step_left_it_blank() {
        for (supplied, expected, label) in [
            (
                "Nova raided!",
                "Nova raided!",
                "a step that filled the field",
            ),
            ("", "stored headline", "a step that left the field blank"),
        ] {
            let content = delivered_content(
                &AlertOverlayKind,
                &one(config::HEADLINE, "stored headline"),
                &one(config::HEADLINE, supplied),
                &ArgStack::new(),
            );

            assert_eq!(
                text_at(&content, config::HEADLINE),
                expected,
                "{label} received the wrong headline"
            );
        }
    }

    #[test]
    fn a_supplied_key_outside_the_kinds_content_group_never_reaches_the_page() {
        let mut supplied = one(config::ACCENT, "red");
        supplied.insert(config::DURATION.to_owned(), Variant::Int(99));
        supplied.insert("vendor.future_key".to_owned(), Variant::Bool(true));
        supplied.insert(
            config::HEADLINE.to_owned(),
            Variant::String("kept".to_owned()),
        );

        let content = delivered_content(
            &AlertOverlayKind,
            &OverlayConfig::new(),
            &supplied,
            &ArgStack::new(),
        );

        assert_eq!(text_at(&content, config::HEADLINE), "kept");
        for smuggled in [config::ACCENT, config::DURATION, "vendor.future_key"] {
            assert!(
                !content.contains_key(smuggled),
                "a step overrode '{smuggled}', which belongs to the overlay and not to a delivery"
            );
        }
    }

    #[test]
    fn the_supplied_side_expands_against_the_same_arguments_as_the_stored_side() {
        let content = delivered_content(
            &AlertOverlayKind,
            &one(config::SUBLINE, "%tier% for %user%"),
            &one(config::HEADLINE, "%user% subscribed"),
            &args(&[("user", "Nova"), ("tier", "1000")]),
        );

        assert_eq!(
            text_at(&content, config::HEADLINE),
            "Nova subscribed",
            "a step's own wording reached the page with its tokens unexpanded"
        );
        assert_eq!(
            text_at(&content, config::SUBLINE),
            "1000 for Nova",
            "the overlay's own wording expanded against a different stack than the step's"
        );
    }
}
