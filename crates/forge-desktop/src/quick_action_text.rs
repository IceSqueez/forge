use forge_types::{MessageCode, MessageRef};

use crate::user_text::resolve_message;

pub(crate) fn quick_action_text(label: &str) -> String {
    match MessageCode::parse(label) {
        Ok(code) => resolve_message(&MessageRef::new(code)).primary,
        Err(_) => label.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use forge_platform_core::test_donation_quick_action;
    use forge_storage::Language;
    use forge_types::IntegrationId;

    use super::quick_action_text;
    use crate::i18n::install_language;

    #[test]
    fn a_catalogued_label_code_resolves_in_the_active_language() {
        let label = test_donation_quick_action(&IntegrationId::new("donatello")).label;

        install_language(Language::En);
        let english = quick_action_text(&label);
        install_language(Language::Uk);
        let ukrainian = quick_action_text(&label);

        assert_eq!(english, "Send test donation");
        assert_eq!(ukrainian, "Надіслати тестовий донат");
    }

    #[test]
    fn a_label_that_is_not_a_message_code_is_shown_verbatim() {
        install_language(Language::En);

        for label in ["Send chat message", "Clear chat (all)", "v1.5 notes", ""] {
            assert_eq!(quick_action_text(label), label);
        }
    }
}
