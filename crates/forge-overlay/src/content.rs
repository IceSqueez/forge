use forge_types::{ArgStack, Variant};

use crate::config::effective_overlay_config;
use crate::descriptor::{ConfigSection, OverlayConfig, OverlayKindDescriptor};

pub fn delivered_content(
    descriptor: &dyn OverlayKindDescriptor,
    stored: &OverlayConfig,
    supplied: &OverlayConfig,
    args: &ArgStack,
) -> OverlayConfig {
    let configured = effective_overlay_config(descriptor, stored);
    let implied = descriptor.implied_content(args);

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
            let delivered = match (expanded(value, args), implied.get(key)) {
                (blank, Some(implied)) if is_blank(&blank) => implied.clone(),
                (delivered, _) => delivered,
            };
            Some((key.to_owned(), delivered))
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
    use crate::kinds::chat::ChatOverlayKind;

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

    #[test]
    fn the_chat_platform_follows_the_sender_unless_the_wording_is_set() {
        let rows: [(&str, &str, &str, &str, &str); 11] = [
            ("twitch", "", "", "twitch", "twitch sender"),
            ("youtube", "", "", "youtube", "youtube sender"),
            ("kick", "", "", "kick", "kick sender"),
            (" kick ", "", "", "kick", "padded wire id"),
            ("donatello", "", "", "", "unsupported platform"),
            ("you_tube", "", "", "", "near-miss spelling"),
            ("", "", "", "", "empty platform argument"),
            ("kick", "youtube", "", "youtube", "explicit step value"),
            ("twitch", "", "kick", "kick", "explicit saved overlay value"),
            ("kick", "%user_platform%", "", "kick", "step token"),
            ("twitch", "", "%user_platform%", "twitch", "saved token"),
        ];
        for (sender, supplied, stored, expected, label) in rows {
            let content = delivered_content(
                &ChatOverlayKind,
                &one(config::PLATFORM, stored),
                &one(config::PLATFORM, supplied),
                &args(&[("user_platform", sender)]),
            );

            assert_eq!(text_at(&content, config::PLATFORM), expected, "{label}");
        }
    }

    #[test]
    fn the_chat_platform_is_empty_when_the_event_carries_no_sender_platform() {
        let content = delivered_content(
            &ChatOverlayKind,
            &OverlayConfig::new(),
            &OverlayConfig::new(),
            &ArgStack::new(),
        );

        assert_eq!(
            content.get(config::PLATFORM),
            Some(&Variant::String(String::new()))
        );
    }

    #[test]
    fn a_kind_without_implied_content_never_gains_a_platform_field() {
        let content = delivered_content(
            &AlertOverlayKind,
            &OverlayConfig::new(),
            &OverlayConfig::new(),
            &args(&[("user_platform", "kick")]),
        );

        assert!(!content.contains_key(config::PLATFORM));
    }
}
