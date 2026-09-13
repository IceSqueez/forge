use serde_json::Value;

use super::text::{clip, code, code_list, compact, plural, quoted, span_ms};
use crate::scenario::{ChatViewer, Crowd, StepAction};
use crate::twitch::ViewerBadge;

const SHORT_TEXT_CHARS: usize = 40;
const VALUE_CHARS: usize = 120;

/// `As <actor> I <stimulus>`, without the closing period.
pub(crate) fn story(action: &StepAction) -> String {
    match action {
        StepAction::ForgeReady { within_ms } => format!(
            "As the streamer I start forge on the seeded fixture and wait up to {} for it to be ready",
            span_ms(*within_ms)
        ),
        StepAction::TwitchSubscribed { types, within_ms } => format!(
            "As the streamer I connect forge to Twitch and wait up to {} for its {} subscription(s)",
            span_ms(*within_ms),
            code_list(types)
        ),
        StepAction::Chat(message) => format!(
            "As a Twitch viewer {} I send {} in chat",
            viewer(&message.viewer),
            quoted(&message.text)
        ),
        StepAction::Crowd(crowd) => crowd_story(crowd),
        StepAction::SessionReconnect { within_ms } => format!(
            "As Twitch I ask forge to reconnect its EventSub session and wait up to {} for the successor",
            span_ms(*within_ms)
        ),
        StepAction::Pause { ms, reason } => {
            format!("As the streamer I wait {} ({reason})", span_ms(*ms))
        }
        StepAction::RunAction { action, args } if args.is_empty() => {
            format!("As the streamer I run action {}", code(action))
        }
        StepAction::RunAction { action, args } => format!(
            "As the streamer I run action {} with arguments {}",
            code(action),
            code(&compact(&Value::Object(args.clone()), VALUE_CHARS))
        ),
        StepAction::SetGlobal {
            name,
            value,
            persisted,
        } => format!(
            "As the streamer I set {}global {} to {}",
            if *persisted { "persisted " } else { "" },
            code(name),
            code(&compact(value, VALUE_CHARS))
        ),
    }
}

/// Fits after "after" in a title.
pub(crate) fn step_short(action: &StepAction) -> String {
    match action {
        StepAction::ForgeReady { .. } => "startup".to_owned(),
        StepAction::TwitchSubscribed { .. } => "connecting to Twitch".to_owned(),
        StepAction::Chat(message) => format!(
            "chat message {}",
            quoted(&clip(&message.text, SHORT_TEXT_CHARS))
        ),
        StepAction::Crowd(crowd) => format!("a crowd of {} viewers", crowd.viewers),
        StepAction::SessionReconnect { .. } => "a Twitch reconnect request".to_owned(),
        StepAction::Pause { .. } => "a pause".to_owned(),
        StepAction::RunAction { action, .. } => format!("running action {}", code(action)),
        StepAction::SetGlobal { name, .. } => format!("setting global {}", code(name)),
    }
}

pub(crate) fn action_title(action: &StepAction) -> String {
    match action {
        StepAction::TwitchSubscribed { .. } => "Twitch subscriptions not established".to_owned(),
        StepAction::Chat(_) => "Chat message not delivered to forge".to_owned(),
        StepAction::Crowd(_) => "Crowd not delivered to forge".to_owned(),
        StepAction::SessionReconnect { .. } => {
            "forge did not reconnect its EventSub session".to_owned()
        }
        StepAction::RunAction { action, .. } => format!("Action {} could not be run", code(action)),
        StepAction::SetGlobal { name, .. } => format!("Global {} could not be set", code(name)),
        StepAction::ForgeReady { .. } | StepAction::Pause { .. } => {
            format!("Step {} failed", code(action.keyword()))
        }
    }
}

pub(crate) fn action_expected(action: &StepAction) -> String {
    match action {
        StepAction::ForgeReady { within_ms } => {
            format!("forge becomes ready within {}", span_ms(*within_ms))
        }
        StepAction::TwitchSubscribed { types, within_ms } => format!(
            "forge holds a live EventSub subscription of each type {} within {}",
            code_list(types),
            span_ms(*within_ms)
        ),
        StepAction::Chat(_) => {
            "the fake Twitch delivers the message to forge's EventSub session".to_owned()
        }
        StepAction::Crowd(crowd) => format!(
            "the fake Twitch delivers all {} to forge's EventSub session",
            plural(crowd.message_count(), "message")
        ),
        StepAction::SessionReconnect { within_ms } => format!(
            "forge opens a successor EventSub session within {}",
            span_ms(*within_ms)
        ),
        StepAction::Pause { .. } => "the pause completes".to_owned(),
        StepAction::RunAction { action, .. } => {
            format!("forge accepts the request to run action {}", code(action))
        }
        StepAction::SetGlobal { name, .. } => {
            format!("forge accepts setting global {}", code(name))
        }
    }
}

fn viewer(viewer: &ChatViewer) -> String {
    let mut text = code(&viewer.login);
    if let Some(display_name) = viewer
        .display_name
        .as_ref()
        .filter(|name| **name != viewer.login)
    {
        text.push_str(&format!(" (shown as {})", code(display_name)));
    }
    if !viewer.badges.is_empty() {
        let badges: Vec<String> = viewer
            .badges
            .iter()
            .map(|badge| badge_name(*badge))
            .collect();
        text.push_str(&format!(" with badges {}", badges.join(", ")));
    }
    text
}

fn badge_name(badge: ViewerBadge) -> String {
    match badge {
        ViewerBadge::Broadcaster => "broadcaster".to_owned(),
        ViewerBadge::Moderator => "moderator".to_owned(),
        ViewerBadge::Vip => "vip".to_owned(),
        ViewerBadge::Subscriber { months } => {
            format!("subscriber ({})", plural(u64::from(months), "month"))
        }
    }
}

fn crowd_story(crowd: &Crowd) -> String {
    let commands = crowd.message_count() - crowd.chatter_count();
    let mut text = format!(
        "As a crowd of {} Twitch viewers I send {} in chat",
        crowd.viewers,
        plural(crowd.message_count(), "message")
    );
    if commands > 0 {
        text.push_str(&format!(
            ", {commands} of them commands ({})",
            code_list(&crowd.commands)
        ));
    }
    if crowd.spacing_ms > 0 {
        text.push_str(&format!(", {} apart", span_ms(crowd.spacing_ms)));
    }
    text
}
