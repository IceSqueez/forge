use forge_components::{ArgsBuilder, DateShape, fmt_display, fmt_uptime, tr_find_warned};
use forge_types::{MessageArg, MessageRef, UPSTREAM_FAILURE, UserText};

use crate::chat::platform_display_name;
use crate::integration_switch::integration_name;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedText {
    pub primary: String,
    pub secondary: Option<String>,
}

pub fn resolve_user_text(text: &UserText) -> ResolvedText {
    match text {
        UserText::Message(message) => resolve_message(message),
        UserText::Verbatim(verbatim) => ResolvedText {
            primary: verbatim.as_str().to_owned(),
            secondary: None,
        },
    }
}

pub fn resolve_message(message: &MessageRef) -> ResolvedText {
    let detail = message.detail().map(|detail| detail.as_str().to_owned());
    let args = message
        .args()
        .fold(ArgsBuilder::new(), |args, (name, value)| {
            with_fluent_arg(args, name, value)
        })
        .build();
    if let Some(primary) = tr_find_warned(&message.code().ftl_key(), Some(&args)) {
        return ResolvedText {
            primary,
            secondary: detail,
        };
    }
    let diagnostic = message.code().to_string();
    match tr_find_warned(&UPSTREAM_FAILURE.ftl_key(), None) {
        Some(primary) => ResolvedText {
            primary,
            secondary: detail.or(Some(diagnostic)),
        },
        None => ResolvedText {
            primary: diagnostic,
            secondary: detail,
        },
    }
}

fn with_fluent_arg<'a>(
    args: ArgsBuilder<'a>,
    name: &'a str,
    value: &MessageArg,
) -> ArgsBuilder<'a> {
    match value {
        MessageArg::Int(number) => args.set(name, *number),
        MessageArg::Float(number) => args.set(name, number.get()),
        MessageArg::Datetime(at) => args.set(name, fmt_display(DateShape::DateTime, *at)),
        MessageArg::Duration(span) => args.set(name, fmt_uptime(span.as_secs())),
        MessageArg::Verbatim(text) => args.set(name, text.as_str().to_owned()),
        MessageArg::Integration(id) => args.set(name, integration_name(id).to_string()),
        MessageArg::Platform(platform) => args.set(name, platform_display_name(*platform)),
    }
}
