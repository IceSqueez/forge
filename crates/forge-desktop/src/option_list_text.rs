use forge_components::{ArgsBuilder, tr, tr_lookup};
use forge_platform_core::{CollectionFailure, OptionHint};

pub(crate) fn option_failure_message(failure: &CollectionFailure) -> String {
    match failure {
        CollectionFailure::NotConnected => tr!("integration_qa_field_unavailable"),
        CollectionFailure::Unauthorized => tr!("integration_qa_options_unauthorized"),
        CollectionFailure::RateLimited => tr!("integration_qa_options_rate_limited"),
        CollectionFailure::InvalidInput { .. } => tr!("integration_qa_options_rejected"),
        CollectionFailure::Transport => tr!("integration_qa_options_transport"),
        CollectionFailure::NotOwned
        | CollectionFailure::CapacityReached
        | CollectionFailure::NotEligible => tr!("integration_qa_options_not_listable"),
    }
}

pub(crate) fn option_hint_text(hint: &OptionHint) -> String {
    let args = hint
        .args
        .iter()
        .fold(ArgsBuilder::new(), |args, (name, value)| {
            args.set(name, value.as_str())
        })
        .build();
    tr_lookup(hint.key, Some(&args))
}

#[cfg(test)]
mod tests {
    use forge_storage::Language;

    use super::*;
    use crate::i18n::{install_language, message_in};

    fn plain(text: String) -> String {
        text.replace(crate::test_support::FORMAT_ISOLATES, "")
    }

    #[test]
    fn every_failure_kind_shows_its_own_forge_message_never_the_integration_text() {
        const INTEGRATION_TEXT: &str = "obs said: scene collection locked";
        install_language(Language::Uk);
        for (failure, key) in [
            (
                CollectionFailure::NotConnected,
                "integration_qa_field_unavailable",
            ),
            (
                CollectionFailure::Unauthorized,
                "integration_qa_options_unauthorized",
            ),
            (
                CollectionFailure::RateLimited,
                "integration_qa_options_rate_limited",
            ),
            (
                CollectionFailure::InvalidInput {
                    field: Some("scene".to_owned()),
                    message: forge_types::MessageRef::new(forge_types::MessageCode::from_static(
                        "obs",
                        "scene_not_in_obs",
                    )),
                },
                "integration_qa_options_rejected",
            ),
            (
                CollectionFailure::Transport,
                "integration_qa_options_transport",
            ),
            (
                CollectionFailure::NotOwned,
                "integration_qa_options_not_listable",
            ),
            (
                CollectionFailure::CapacityReached,
                "integration_qa_options_not_listable",
            ),
            (
                CollectionFailure::NotEligible,
                "integration_qa_options_not_listable",
            ),
        ] {
            let expected = message_in(Language::Uk, key);
            let shown = option_failure_message(&failure);

            assert_ne!(expected, key, "uk {key} is missing");
            assert_eq!(shown, expected, "{failure:?}");
            assert!(!shown.contains(INTEGRATION_TEXT), "{failure:?}");
        }
    }

    #[test]
    fn a_hint_resolves_through_the_ui_bundle_with_its_arguments() {
        install_language(Language::Uk);
        for (hint, expected) in [
            (
                OptionHint::new("option_hint_current_scene").arg("scene", "Main"),
                "Зараз: Main (В ЕФІРІ)",
            ),
            (OptionHint::new("option_hint_live"), "в ефірі"),
            (OptionHint::new("option_hint_hidden"), "приховане"),
        ] {
            assert_eq!(plain(option_hint_text(&hint)), expected, "{}", hint.key);
        }
    }
}
