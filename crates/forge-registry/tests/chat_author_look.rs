use forge_events::{Event, EventSource};
use forge_registry::{
    TriggerVariables, USER_AVATAR_VARIABLE, USER_BADGE_IMAGES_VARIABLE, USER_BADGES_VARIABLE,
    USER_COLOR_VARIABLE, message_badge_images,
};
use forge_types::{ArgStack, BadgeImage, ChatPayload, UserBadge, Variant, VariantKind};
use serde_json::{Value, json};

const LOOK_NAMES: [&str; 3] = [
    USER_BADGES_VARIABLE,
    USER_COLOR_VARIABLE,
    USER_AVATAR_VARIABLE,
];

fn envelope(badges: Vec<UserBadge>, author_color: Option<&str>) -> Value {
    pictured(badges, author_color, Vec::new())
}

fn pictured(
    badges: Vec<UserBadge>,
    author_color: Option<&str>,
    badge_images: Vec<BadgeImage>,
) -> Value {
    serde_json::to_value(ChatPayload {
        badge_images,
        platform_msg_id: "m-1".to_owned(),
        author: "viewer".to_owned(),
        author_color: author_color.map(str::to_owned),
        segments: Vec::new(),
        badges,
        is_event: false,
        event_detail: None,
        moderation: Default::default(),
    })
    .unwrap_or(Value::Null)
}

fn chat_line(chat: Value) -> Event {
    Event::new(
        EventSource::Twitch,
        "twitch.channel.chat.message",
        json!({ ChatPayload::KEY: chat }),
    )
}

fn look_of(event: &Event, avatar: Option<&'static str>) -> ArgStack {
    TriggerVariables::new()
        .chat_author_look(move |_| avatar.map(str::to_owned))
        .arg_stack(event)
}

fn text(value: &str) -> Option<Variant> {
    Some(Variant::String(value.to_owned()))
}

#[test]
fn every_badge_reads_as_its_label_in_the_order_the_platform_sent_it() {
    let badges = vec![
        UserBadge::Broadcaster,
        UserBadge::Moderator,
        UserBadge::Vip,
        UserBadge::Subscriber { months: 14 },
        UserBadge::Member {
            level: "gold".to_owned(),
        },
        UserBadge::Bot,
        UserBadge::Partner,
        UserBadge::Premium,
        UserBadge::Founder,
        UserBadge::Turbo,
        UserBadge::HypeTrain,
        UserBadge::Bits { amount: 1000 },
        UserBadge::BitsLeader { rank: 1 },
    ];

    let stack = look_of(&chat_line(envelope(badges, None)), None);

    assert_eq!(
        stack.get(USER_BADGES_VARIABLE).cloned(),
        text(
            "BROADCASTER MOD VIP SUB MEMBER BOT PARTNER PRIME FOUNDER TURBO HYPE BITS BITS-LEADER"
        )
    );
}

#[test]
fn a_chatter_without_badges_publishes_an_empty_badge_list() {
    let stack = look_of(&chat_line(envelope(Vec::new(), None)), None);

    assert_eq!(stack.get(USER_BADGES_VARIABLE).cloned(), text(""));
}

#[test]
fn the_name_color_is_the_chat_envelopes_author_color() {
    let stack = look_of(&chat_line(envelope(Vec::new(), Some("#1E90FF"))), None);

    assert_eq!(stack.get(USER_COLOR_VARIABLE).cloned(), text("#1E90FF"));
}

#[test]
fn a_chatter_who_never_picked_a_color_publishes_an_empty_color() {
    let stack = look_of(&chat_line(envelope(Vec::new(), None)), None);

    assert_eq!(stack.get(USER_COLOR_VARIABLE).cloned(), text(""));
}

#[test]
fn the_avatar_is_whatever_the_platform_reader_found_or_empty() {
    let event = chat_line(envelope(Vec::new(), None));
    for (found, published) in [
        (
            Some("https://cdn.example/a.png"),
            "https://cdn.example/a.png",
        ),
        (None, ""),
    ] {
        assert_eq!(
            look_of(&event, found).get(USER_AVATAR_VARIABLE).cloned(),
            text(published),
            "reader returned {found:?}"
        );
    }
}

#[test]
fn a_line_without_a_readable_chat_envelope_still_publishes_every_look_name_empty() {
    for (case, event) in [
        (
            "no envelope",
            Event::new(EventSource::Kick, "kick.chat.message.sent", json!({})),
        ),
        (
            "malformed envelope",
            chat_line(json!({ "badges": "not-a-list" })),
        ),
    ] {
        let stack = look_of(&event, None);
        for name in LOOK_NAMES {
            assert_eq!(stack.get(name).cloned(), text(""), "{case}: '{name}'");
        }
    }
}

#[test]
fn the_look_names_are_declared_as_text_for_the_variable_picker() {
    let declared: Vec<(String, VariantKind)> = TriggerVariables::new()
        .chat_author_look(|_| None)
        .schema()
        .variables
        .into_iter()
        .map(|variable| (variable.name, variable.kind))
        .collect();

    assert_eq!(
        declared,
        [
            USER_BADGES_VARIABLE,
            USER_BADGE_IMAGES_VARIABLE,
            USER_COLOR_VARIABLE,
            USER_AVATAR_VARIABLE,
        ]
        .map(|name| (name.to_owned(), VariantKind::String))
    );
}

fn badge_image(title: &str, url: &str) -> BadgeImage {
    BadgeImage {
        title: title.to_owned(),
        url: url.to_owned(),
    }
}

#[test]
fn the_badge_pictures_travel_from_the_chat_line_to_the_overlay_reader_in_order() {
    let pictures = vec![
        badge_image("Broadcaster", "https://static-cdn.jtvnw.net/badges/v1/a/2"),
        badge_image("Subscriber", "https://static-cdn.jtvnw.net/badges/v1/b/2"),
    ];
    let stack = look_of(
        &chat_line(pictured(
            vec![UserBadge::Broadcaster, UserBadge::Subscriber { months: 3 }],
            None,
            pictures.clone(),
        )),
        None,
    );

    assert_eq!(message_badge_images(&stack), pictures);
}

#[test]
fn a_line_without_badge_pictures_reads_as_an_empty_picture_list() {
    for (case, event) in [
        (
            "no pictures",
            chat_line(envelope(vec![UserBadge::Moderator], None)),
        ),
        (
            "no envelope",
            Event::new(EventSource::Kick, "kick.chat.message.sent", json!({})),
        ),
    ] {
        let stack = look_of(&event, None);
        assert_eq!(
            stack.get(USER_BADGE_IMAGES_VARIABLE).cloned(),
            text("[]"),
            "{case}"
        );
        assert_eq!(message_badge_images(&stack), Vec::new(), "{case}");
    }
}

#[test]
fn an_unreadable_picture_list_gives_the_overlay_no_pictures() {
    for listed in ["", "not json", "{\"title\":\"a\"}", "[{\"title\":\"a\"}]"] {
        let stack = ArgStack::new().set(
            USER_BADGE_IMAGES_VARIABLE.to_owned(),
            Variant::String(listed.to_owned()),
        );
        assert_eq!(
            message_badge_images(&stack),
            Vec::new(),
            "listed {listed:?}"
        );
    }
}
