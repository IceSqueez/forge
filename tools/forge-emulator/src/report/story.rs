use serde_json::Value;

use super::text::{clip, code, code_list, compact, plural, quoted, span_ms};
use crate::scenario::{ChatViewer, Crowd, StepAction};
use crate::twitch::ViewerBadge;

const SHORT_TEXT_CHARS: usize = 40;
const VALUE_CHARS: usize = 120;

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
        StepAction::TwitchEvent {
            subscription_type, ..
        } => format!(
            "As Twitch I deliver a {} notification to forge's EventSub session",
            code(subscription_type)
        ),
        StepAction::SessionReconnect { within_ms } => format!(
            "As Twitch I ask forge to reconnect its EventSub session and wait up to {} for the successor",
            span_ms(*within_ms)
        ),
        StepAction::OverlayPage { overlay, within_ms } => format!(
            "As a browser source I open the page for overlay {} and wait up to {} for forge to accept its credential",
            code(overlay),
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
        StepAction::KickChatJoined { within_ms } => format!(
            "As Kick I wait up to {} for forge to join the channel's chatroom",
            span_ms(*within_ms)
        ),
        StepAction::KickChat(message) => format!(
            "As a Kick viewer {} I send {} in chat",
            code(&message.sender.username),
            quoted(&message.text)
        ),
        StepAction::KickPusherEvent { event, .. } => format!(
            "As Kick I push a {} event to forge's chatroom connection",
            code(event)
        ),
        StepAction::KickStream { live } => format!(
            "As the streamer I {} the Kick stream",
            if *live { "start" } else { "end" }
        ),
        StepAction::KickChannelPolled { within_ms } => format!(
            "As Kick I wait up to {} for forge to read the channel",
            span_ms(*within_ms)
        ),
        StepAction::YoutubeChatPolled { within_ms } => format!(
            "As YouTube I wait up to {} for forge to read the live chat",
            span_ms(*within_ms)
        ),
        StepAction::YoutubeChat(message) => format!(
            "As a YouTube viewer {} I send {} in the live chat",
            code(&message.author.display_name),
            quoted(&message.text)
        ),
        StepAction::YoutubeChatEvent { author, snippet } => format!(
            "As a YouTube viewer {} I cause a {} in the live chat",
            code(&author.display_name),
            code(
                snippet
                    .get("type")
                    .and_then(|kind| kind.as_str())
                    .unwrap_or("message")
            )
        ),
        StepAction::YoutubeBroadcast { live } => format!(
            "As the streamer I {} the YouTube broadcast",
            if *live { "start" } else { "end" }
        ),
        StepAction::ObsOnline {} => "As the streamer I start OBS".to_owned(),
        StepAction::ObsIdentified { within_ms } => format!(
            "As OBS I wait up to {} for forge to connect and identify",
            span_ms(*within_ms)
        ),
        StepAction::ObsRestart { down_ms } => format!(
            "As the streamer I quit OBS and start it again {} later",
            span_ms(*down_ms)
        ),
        StepAction::ObsSceneSwitch { scene } => {
            format!("As the streamer I switch OBS to scene {}", code(scene))
        }
        StepAction::ObsStream { active } => format!(
            "As the streamer I {} streaming in OBS",
            if *active { "start" } else { "stop" }
        ),
        StepAction::ObsInputMute { input, muted } => format!(
            "As the streamer I {} {} in OBS",
            if *muted { "mute" } else { "unmute" },
            code(input)
        ),
        StepAction::VtubeOnline {} => "As the streamer I start VTube Studio".to_owned(),
        StepAction::VtubeAuthenticated { within_ms } => format!(
            "As VTube Studio I wait up to {} for forge to connect and authenticate",
            span_ms(*within_ms)
        ),
        StepAction::VtubeHotkey { hotkey } => {
            format!(
                "As the streamer I press the VTube Studio hotkey {}",
                code(hotkey)
            )
        }
        StepAction::VtubeModelLoad { model } => {
            format!(
                "As the streamer I load the VTube Studio model {}",
                code(model)
            )
        }
        StepAction::VtubeModelUnload {} => {
            "As the streamer I unload the VTube Studio model".to_owned()
        }
        StepAction::VtubeModelConfigChanged {} => {
            "As the streamer I change the loaded VTube Studio model's settings".to_owned()
        }
        StepAction::VtubeTracking { face_found } => format!(
            "As the streamer I {} the VTube Studio tracker",
            if *face_found {
                "step back in front of"
            } else {
                "step away from"
            }
        ),
        StepAction::VtubeItemAdded { file } => {
            format!(
                "As the streamer I add the item {} to the VTube Studio scene",
                code(file)
            )
        }
        StepAction::VtubeItemRemoved { file } => format!(
            "As the streamer I remove the item {} from the VTube Studio scene",
            code(file)
        ),
        StepAction::VtubeExpression { file, active } => format!(
            "As the streamer I turn the VTube Studio expression {} {}",
            code(file),
            if *active { "on" } else { "off" }
        ),
        StepAction::DonatelloDonation(gift) => format!(
            "As a viewer I donate {} UAH on Donatello as donation {}",
            gift.amount,
            code(&gift.id)
        ),
        StepAction::MonobankTopUp(gift) => format!(
            "As a viewer {} I top up the monobank jar by {} kopiyky as transaction {}",
            code(&gift.sender),
            gift.amount_minor,
            code(&gift.id)
        ),
        StepAction::DonationsPolled { within_ms } => format!(
            "As the donation services I wait up to {} for forge to read every donation list",
            span_ms(*within_ms)
        ),
        StepAction::ForgeRestart { within_ms, offline } => format!(
            "As the streamer I quit forge, {} arrive while it is down, and I start it again and wait up to {} for it to be ready",
            plural(offline.len() as u64, "donation"),
            span_ms(*within_ms)
        ),
    }
}

pub(crate) fn step_short(action: &StepAction) -> String {
    match action {
        StepAction::ForgeReady { .. } => "startup".to_owned(),
        StepAction::TwitchSubscribed { .. } => "connecting to Twitch".to_owned(),
        StepAction::Chat(message) => format!(
            "chat message {}",
            quoted(&clip(&message.text, SHORT_TEXT_CHARS))
        ),
        StepAction::Crowd(crowd) => format!("a crowd of {} viewers", crowd.viewers),
        StepAction::TwitchEvent {
            subscription_type, ..
        } => format!("a {} notification", code(subscription_type)),
        StepAction::SessionReconnect { .. } => "a Twitch reconnect request".to_owned(),
        StepAction::OverlayPage { overlay, .. } => {
            format!("opening the page for {}", code(overlay))
        }
        StepAction::Pause { .. } => "a pause".to_owned(),
        StepAction::RunAction { action, .. } => format!("running action {}", code(action)),
        StepAction::SetGlobal { name, .. } => format!("setting global {}", code(name)),
        StepAction::KickChatJoined { .. } => "joining Kick chat".to_owned(),
        StepAction::KickChat(message) => format!(
            "Kick chat message {}",
            quoted(&clip(&message.text, SHORT_TEXT_CHARS))
        ),
        StepAction::KickPusherEvent { event, .. } => format!("a Kick {} event", code(event)),
        StepAction::KickStream { live } => format!(
            "{} the Kick stream",
            if *live { "starting" } else { "ending" }
        ),
        StepAction::KickChannelPolled { .. } => "a Kick channel poll".to_owned(),
        StepAction::YoutubeChatPolled { .. } => "a YouTube chat poll".to_owned(),
        StepAction::YoutubeChat(message) => format!(
            "YouTube chat message {}",
            quoted(&clip(&message.text, SHORT_TEXT_CHARS))
        ),
        StepAction::YoutubeChatEvent { snippet, .. } => format!(
            "a YouTube {}",
            code(
                snippet
                    .get("type")
                    .and_then(|kind| kind.as_str())
                    .unwrap_or("message")
            )
        ),
        StepAction::YoutubeBroadcast { live } => format!(
            "{} the YouTube broadcast",
            if *live { "starting" } else { "ending" }
        ),
        StepAction::ObsOnline {} => "starting OBS".to_owned(),
        StepAction::ObsIdentified { .. } => "connecting to OBS".to_owned(),
        StepAction::ObsRestart { .. } => "an OBS restart".to_owned(),
        StepAction::ObsSceneSwitch { scene } => format!("switching OBS to {}", code(scene)),
        StepAction::ObsStream { active } => format!(
            "{} the OBS stream",
            if *active { "starting" } else { "stopping" }
        ),
        StepAction::ObsInputMute { input, .. } => format!("muting {} in OBS", code(input)),
        StepAction::VtubeOnline {} => "starting VTube Studio".to_owned(),
        StepAction::VtubeAuthenticated { .. } => "connecting to VTube Studio".to_owned(),
        StepAction::VtubeHotkey { hotkey } => format!("pressing hotkey {}", code(hotkey)),
        StepAction::VtubeModelLoad { model } => format!("loading model {}", code(model)),
        StepAction::VtubeModelUnload {} => "unloading the model".to_owned(),
        StepAction::VtubeModelConfigChanged {} => "changing the model settings".to_owned(),
        StepAction::VtubeTracking { face_found } => format!(
            "the tracker {} the face",
            if *face_found { "finding" } else { "losing" }
        ),
        StepAction::VtubeItemAdded { file } => format!("adding item {}", code(file)),
        StepAction::VtubeItemRemoved { file } => format!("removing item {}", code(file)),
        StepAction::VtubeExpression { file, .. } => format!("toggling expression {}", code(file)),
        StepAction::DonatelloDonation(gift) => format!("Donatello donation {}", code(&gift.id)),
        StepAction::MonobankTopUp(gift) => format!("monobank top-up {}", code(&gift.id)),
        StepAction::DonationsPolled { .. } => "donation polling".to_owned(),
        StepAction::ForgeRestart { .. } => "a forge restart".to_owned(),
    }
}

pub(crate) fn action_title(action: &StepAction) -> String {
    match action {
        StepAction::TwitchSubscribed { .. } => "Twitch subscriptions not established".to_owned(),
        StepAction::Chat(_) => "Chat message not delivered to forge".to_owned(),
        StepAction::Crowd(_) => "Crowd not delivered to forge".to_owned(),
        StepAction::TwitchEvent {
            subscription_type, ..
        } => format!(
            "Notification {} not delivered to forge",
            code(subscription_type)
        ),
        StepAction::SessionReconnect { .. } => {
            "forge did not reconnect its EventSub session".to_owned()
        }
        StepAction::OverlayPage { overlay, .. } => {
            format!("Overlay page {} could not be opened", code(overlay))
        }
        StepAction::RunAction { action, .. } => format!("Action {} could not be run", code(action)),
        StepAction::SetGlobal { name, .. } => format!("Global {} could not be set", code(name)),
        StepAction::KickChatJoined { .. } => "forge did not join Kick chat".to_owned(),
        StepAction::KickChat(_) | StepAction::KickPusherEvent { .. } => {
            "Kick event not delivered to forge".to_owned()
        }
        StepAction::KickStream { .. } => "The fake Kick could not change the stream".to_owned(),
        StepAction::KickChannelPolled { .. } => "forge did not read the Kick channel".to_owned(),
        StepAction::YoutubeChatPolled { .. } => "forge did not read the YouTube chat".to_owned(),
        StepAction::YoutubeChat(_) | StepAction::YoutubeChatEvent { .. } => {
            "YouTube chat message not added to the live chat".to_owned()
        }
        StepAction::YoutubeBroadcast { .. } => {
            "The fake YouTube could not change the broadcast".to_owned()
        }
        StepAction::ObsOnline {} => "The fake OBS could not start".to_owned(),
        StepAction::ObsIdentified { .. } => "forge did not connect to OBS".to_owned(),
        StepAction::ObsRestart { .. } => "The fake OBS could not restart".to_owned(),
        StepAction::ObsSceneSwitch { .. }
        | StepAction::ObsStream { .. }
        | StepAction::ObsInputMute { .. } => "OBS event not delivered to forge".to_owned(),
        StepAction::VtubeOnline {} => "The fake VTube Studio could not start".to_owned(),
        StepAction::VtubeAuthenticated { .. } => {
            "forge did not authenticate with VTube Studio".to_owned()
        }
        StepAction::VtubeExpression { .. } => {
            "The fake VTube Studio could not change the expression".to_owned()
        }
        StepAction::VtubeHotkey { .. }
        | StepAction::VtubeModelLoad { .. }
        | StepAction::VtubeModelUnload {}
        | StepAction::VtubeModelConfigChanged {}
        | StepAction::VtubeTracking { .. }
        | StepAction::VtubeItemAdded { .. }
        | StepAction::VtubeItemRemoved { .. } => {
            "VTube Studio event not delivered to forge".to_owned()
        }
        StepAction::DonatelloDonation(_) | StepAction::MonobankTopUp(_) => {
            "The fake donation service could not take the donation".to_owned()
        }
        StepAction::DonationsPolled { .. } => "forge did not read every donation list".to_owned(),
        StepAction::ForgeRestart { .. } => "forge did not come back after a restart".to_owned(),
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
        StepAction::TwitchEvent {
            subscription_type, ..
        } => format!(
            "the fake Twitch delivers the {} notification to a live session subscribed to it",
            code(subscription_type)
        ),
        StepAction::SessionReconnect { within_ms } => format!(
            "forge opens a successor EventSub session within {}",
            span_ms(*within_ms)
        ),
        StepAction::OverlayPage { overlay, within_ms } => format!(
            "forge serves the page for {} and accepts the credential its config.json carries, within {}",
            code(overlay),
            span_ms(*within_ms)
        ),
        StepAction::Pause { .. } => "the pause completes".to_owned(),
        StepAction::RunAction { action, .. } => {
            format!("forge accepts the request to run action {}", code(action))
        }
        StepAction::SetGlobal { name, .. } => {
            format!("forge accepts setting global {}", code(name))
        }
        StepAction::KickChatJoined { within_ms } => format!(
            "forge subscribes a live chat connection to the chatroom within {}",
            span_ms(*within_ms)
        ),
        StepAction::KickChat(_) | StepAction::KickPusherEvent { .. } => {
            "the fake Kick pushes the event to a chat connection that joined the chatroom"
                .to_owned()
        }
        StepAction::KickStream { .. } => {
            "the fake Kick reports the new stream state on forge's next channel read".to_owned()
        }
        StepAction::KickChannelPolled { within_ms } => format!(
            "forge reads the channel from the public API within {}",
            span_ms(*within_ms)
        ),
        StepAction::YoutubeChatPolled { within_ms } => format!(
            "forge resolves the live broadcast and reads its chat within {}",
            span_ms(*within_ms)
        ),
        StepAction::YoutubeChat(_) | StepAction::YoutubeChatEvent { .. } => {
            "the fake YouTube serves the message on forge's next chat poll".to_owned()
        }
        StepAction::YoutubeBroadcast { .. } => {
            "the fake YouTube reports the new broadcast state to forge's next poll".to_owned()
        }
        StepAction::ObsOnline {} => "the fake OBS accepts connections".to_owned(),
        StepAction::ObsIdentified { within_ms } => format!(
            "forge holds an identified OBS WebSocket session within {}",
            span_ms(*within_ms)
        ),
        StepAction::ObsRestart { .. } => {
            "the fake OBS drops every connection and accepts new ones again".to_owned()
        }
        StepAction::ObsSceneSwitch { .. }
        | StepAction::ObsStream { .. }
        | StepAction::ObsInputMute { .. } => {
            "the fake OBS pushes the event to an identified session subscribed to it".to_owned()
        }
        StepAction::VtubeOnline {} => "the fake VTube Studio accepts connections".to_owned(),
        StepAction::VtubeAuthenticated { within_ms } => format!(
            "forge holds an authenticated VTube Studio session within {}",
            span_ms(*within_ms)
        ),
        StepAction::VtubeExpression { .. } => {
            "the fake VTube Studio flips the expression for forge's next state poll".to_owned()
        }
        StepAction::VtubeHotkey { .. }
        | StepAction::VtubeModelLoad { .. }
        | StepAction::VtubeModelUnload {}
        | StepAction::VtubeModelConfigChanged {}
        | StepAction::VtubeTracking { .. }
        | StepAction::VtubeItemAdded { .. }
        | StepAction::VtubeItemRemoved { .. } => {
            "the fake VTube Studio pushes the event to an authenticated session subscribed to it"
                .to_owned()
        }
        StepAction::DonatelloDonation(_) | StepAction::MonobankTopUp(_) => {
            "the fake donation service lists the donation on its next poll".to_owned()
        }
        StepAction::DonationsPolled { within_ms } => format!(
            "every fake donation service answers a donation list request within {}",
            span_ms(*within_ms)
        ),
        StepAction::ForgeRestart { within_ms, .. } => format!(
            "forge stops cleanly and becomes ready again on the same data within {}",
            span_ms(*within_ms)
        ),
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
