use forge_types::{MessageArg, MessageRef};

pub(crate) fn coded_text(message: &MessageRef) -> String {
    let mut text = message.code().to_string();
    let args: Vec<String> = message
        .args()
        .map(|(name, value)| format!("{name}={}", arg_text(value)))
        .collect();
    if !args.is_empty() {
        text.push_str(&format!(" ({})", args.join(", ")));
    }
    if let Some(detail) = message.detail() {
        text.push_str(&format!(": {}", detail.as_str()));
    }
    text
}

fn arg_text(value: &MessageArg) -> String {
    match value {
        MessageArg::Int(number) => number.to_string(),
        MessageArg::Float(number) => number.get().to_string(),
        MessageArg::Datetime(at) | MessageArg::TimeOfDay(at) => at.to_string(),
        MessageArg::Duration(span) => format!("{}s", span.as_secs()),
        MessageArg::Verbatim(text) => text.as_str().to_owned(),
        MessageArg::Integration(integration) => integration.to_string(),
        MessageArg::Platform(platform) => platform.as_str().to_owned(),
        MessageArg::ValueKind(kind) => kind.contract_name().to_owned(),
        MessageArg::StepFields { keys, .. } => keys.join(", "),
        MessageArg::Message(message) => coded_text(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_types::{MessageCode, UpstreamDetail, VerbatimText};

    fn code(name: &'static str) -> MessageCode {
        MessageCode::from_static("test", name)
    }

    #[test]
    fn a_message_without_arguments_or_detail_is_just_its_code() {
        assert_eq!(coded_text(&MessageRef::new(code("plain"))), "test.plain");
    }

    #[test]
    fn arguments_follow_the_code_in_order_as_name_value_pairs() {
        let message = MessageRef::new(code("pair"))
            .with_arg("first", 7_i64)
            .with_arg("second", VerbatimText::new("two"));

        assert_eq!(coded_text(&message), "test.pair (first=7, second=two)");
    }

    #[test]
    fn upstream_detail_is_appended_after_the_arguments() {
        let message = MessageRef::new(code("failed"))
            .with_arg("target", VerbatimText::new("kick"))
            .with_detail(UpstreamDetail::from_redacted("boom"));

        assert_eq!(coded_text(&message), "test.failed (target=kick): boom");
    }

    #[test]
    fn upstream_detail_alone_follows_the_code_without_an_argument_list() {
        let message =
            MessageRef::new(code("failed")).with_detail(UpstreamDetail::from_redacted("boom"));

        assert_eq!(coded_text(&message), "test.failed: boom");
    }

    #[test]
    fn a_nested_message_argument_renders_its_own_code_arguments_and_detail() {
        let inner = MessageRef::new(code("inner"))
            .with_arg("n", 3_i64)
            .with_detail(UpstreamDetail::from_redacted("deep"));
        let outer =
            MessageRef::new(code("outer")).with_arg("cause", MessageArg::Message(Box::new(inner)));

        assert_eq!(
            coded_text(&outer),
            "test.outer (cause=test.inner (n=3): deep)"
        );
    }
}
