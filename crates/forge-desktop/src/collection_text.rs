use forge_components::tr;
use forge_platform_core::CollectionFailure;

pub(crate) fn localized_collection_text(english: &str) -> String {
    match english {
        "Channel point rewards" => tr!("collection_text_channel_point_rewards"),
        "Title" => tr!("collection_text_title"),
        "Cost (Channel Points)" => tr!("collection_text_cost"),
        "Prompt" => tr!("collection_text_prompt"),
        "optional" => tr!("collection_text_optional"),
        "Require viewer input" => tr!("collection_text_require_input"),
        "Background color (#RRGGBB)" => tr!("collection_text_background_color"),
        "Max per stream (0 = unlimited)" => tr!("collection_text_max_per_stream"),
        "Max per viewer per stream (0 = unlimited)" => tr!("collection_text_max_per_viewer"),
        "Global cooldown seconds (0 = off)" => tr!("collection_text_global_cooldown"),
        "Enabled" => tr!("collection_text_enabled"),
        "Paused" => tr!("collection_text_paused"),
        "Created outside forge - edit it in the Twitch dashboard" => {
            tr!("collection_text_created_outside")
        }
        "Title is required" => tr!("collection_text_title_required"),
        "Background color must be empty or #RRGGBB" => tr!("collection_text_color_format"),
        "Must be 0 or more" => tr!("collection_text_zero_or_more"),
        "A reward with this title already exists" => tr!("collection_text_duplicate_title"),
        "Twitch AutoMod rejected the reward text" => tr!("collection_text_automod_rejected"),
        "Twitch rejected the reward settings" => tr!("collection_text_settings_rejected"),
        "This reward no longer exists" => tr!("collection_text_reward_gone"),
        other => other.to_owned(),
    }
}

pub(crate) fn collection_failure_message(failure: &CollectionFailure) -> String {
    match failure {
        CollectionFailure::NotConnected => tr!("collection_failure_not_connected"),
        CollectionFailure::Unauthorized => tr!("collection_failure_unauthorized"),
        CollectionFailure::NotOwned => tr!("collection_failure_not_owned"),
        CollectionFailure::CapacityReached => tr!("collection_failure_capacity"),
        CollectionFailure::NotEligible => tr!("collection_failure_not_eligible"),
        CollectionFailure::InvalidInput { message, .. } => localized_collection_text(message),
        CollectionFailure::RateLimited => tr!("collection_failure_rate_limited"),
        CollectionFailure::Transport => tr!("collection_failure_transport"),
    }
}
