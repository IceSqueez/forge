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
