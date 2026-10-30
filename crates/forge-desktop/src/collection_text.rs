use forge_components::tr;
use forge_platform_core::CollectionFailure;

use crate::user_text::resolve_message;

pub(crate) fn collection_failure_message(failure: &CollectionFailure) -> String {
    match failure {
        CollectionFailure::NotConnected => tr!("collection_failure_not_connected"),
        CollectionFailure::Unauthorized => tr!("collection_failure_unauthorized"),
        CollectionFailure::NotOwned => tr!("collection_failure_not_owned"),
        CollectionFailure::CapacityReached => tr!("collection_failure_capacity"),
        CollectionFailure::NotEligible => tr!("collection_failure_not_eligible"),
        CollectionFailure::InvalidInput { message, .. } => resolve_message(message).inline(),
        CollectionFailure::RateLimited => tr!("collection_failure_rate_limited"),
        CollectionFailure::Transport => tr!("collection_failure_transport"),
    }
}
