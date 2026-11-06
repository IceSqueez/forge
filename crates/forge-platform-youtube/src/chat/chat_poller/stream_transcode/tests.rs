use std::sync::Arc;

use forge_net::DedupSet;
use forge_platform_core::{PlatformError, TokenSource, ViewerAvatarSource};
use futures::future::BoxFuture;
use prost::Message;
use serde_json::{Value, json};
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};

use super::super::fetch::page_from_body;
use super::super::wire::item_type;
use super::super::{
    CHAT_ENDED_EVENT_TYPE, DEDUP_WINDOW_SIZE, DEFAULT_POLLING_INTERVAL_MS, YouTubeChatPoller,
};
use super::*;
use crate::broadcast::active_broadcast_id::ActiveBroadcastIdHandle;
use crate::broadcast::live_chat_id::LiveChatIdHandle;
use crate::chat::avatar_book::AvatarBook;
use crate::chat::ban_ledger::observed_ban;
use crate::chat::ban_ledger_test_support::MemoryBanLedger;
use crate::stream_list_proto::live_chat_message_snippet::DisplayedContent as Content;
use crate::stream_list_proto::{LiveChatGiftDetails, LiveChatPollDetails, PageInfo};
use crate::vocabulary::event_kinds;

const PUBLISHED_AT: &str = "2026-11-06T07:20:05.111654+00:00";
const BROADCASTER: &str = "UCbroadcaster";
const AVATAR: &str = "https://yt3.ggpht.com/viewer";

fn token_source() -> TokenSource {
    Arc::new(|| -> BoxFuture<'static, Result<String, PlatformError>> {
        Box::pin(async { Ok("test-token".to_owned()) })
    })
}

fn mapped(item: &Value) -> Option<(String, Value)> {
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let poller = YouTubeChatPoller::new(
        crate::endpoints_test_support::unreachable_data_api(),
        token_source(),
        tx,
        BROADCASTER.to_owned(),
        LiveChatIdHandle::new(),
        ActiveBroadcastIdHandle::new(),
        MemoryBanLedger::shared(),
    );
    poller
        .build_event(item, &mut DedupSet::bounded(DEDUP_WINDOW_SIZE))
        .map(|event| (event.kind, event.payload))
}

fn decoded(bytes: &[u8]) -> LiveChatMessageListResponse {
    LiveChatMessageListResponse::decode(bytes).unwrap()
}

fn backlog_frame() -> LiveChatMessageListResponse {
    decoded(include_bytes!(
        "../../../../tests/fixtures/stream_list/backlog.bin"
    ))
}

fn keep_alive_frame() -> LiveChatMessageListResponse {
    decoded(include_bytes!(
        "../../../../tests/fixtures/stream_list/keep_alive.bin"
    ))
}

fn author() -> LiveChatMessageAuthorDetails {
    LiveChatMessageAuthorDetails {
        channel_id: Some("UCviewer".to_owned()),
        channel_url: Some("https://www.youtube.com/channel/UCviewer".to_owned()),
        display_name: Some("Viewer".to_owned()),
        profile_image_url: Some(AVATAR.to_owned()),
        is_verified: Some(false),
        is_chat_owner: Some(false),
        is_chat_sponsor: Some(true),
        is_chat_moderator: Some(true),
    }
}

fn rest_author() -> Value {
    json!({
        "channelId": "UCviewer",
        "channelUrl": "https://www.youtube.com/channel/UCviewer",
        "displayName": "Viewer",
        "profileImageUrl": AVATAR,
        "isVerified": false,
        "isChatOwner": false,
        "isChatSponsor": true,
        "isChatModerator": true
    })
}

fn message(id: &str, kind: i32, content: Option<Content>) -> LiveChatMessage {
    LiveChatMessage {
        id: Some(id.to_owned()),
        snippet: Some(LiveChatMessageSnippet {
            r#type: Some(kind),
            published_at: Some(PUBLISHED_AT.to_owned()),
            displayed_content: content,
            ..Default::default()
        }),
        author_details: Some(author()),
        ..Default::default()
    }
}

fn rest_item(id: &str, spelled: &str, details: Option<(&str, Value)>) -> Value {
    let mut snippet = json!({ "type": spelled, "publishedAt": PUBLISHED_AT });
    if let Some((key, value)) = details {
        snippet[key] = value;
    }
    json!({ "id": id, "snippet": snippet, "authorDetails": rest_author() })
}

fn transcoded(message: LiveChatMessage) -> Value {
    let response = LiveChatMessageListResponse {
        items: vec![message],
        ..Default::default()
    };
    transcode_response(&response)["items"][0].clone()
}

fn profile(channel_id: &str, name: &str) -> ChannelProfileDetails {
    ChannelProfileDetails {
        channel_id: Some(channel_id.to_owned()),
        channel_url: Some(format!("https://www.youtube.com/channel/{channel_id}")),
        display_name: Some(name.to_owned()),
        profile_image_url: Some(format!("https://yt3.ggpht.com/{channel_id}")),
    }
}

fn rest_profile(channel_id: &str, name: &str) -> Value {
    json!({
        "channelId": channel_id,
        "channelUrl": format!("https://www.youtube.com/channel/{channel_id}"),
        "displayName": name,
        "profileImageUrl": format!("https://yt3.ggpht.com/{channel_id}")
    })
}

fn ban(ban_type: BanType, seconds: Option<u64>) -> Content {
    Content::UserBannedDetails(LiveChatUserBannedMessageDetails {
        banned_user_details: Some(profile("UCbanned", "Troll")),
        ban_type: Some(ban_type as i32),
        ban_duration_seconds: seconds,
    })
}

struct Case {
    name: &'static str,
    proto: LiveChatMessage,
    rest: Value,
    kind: &'static str,
}

fn every_event_type() -> Vec<Case> {
    vec![
        Case {
            name: "text",
            proto: message(
                "text-1",
                Type::TextMessageEvent as i32,
                Some(Content::TextMessageDetails(LiveChatTextMessageDetails {
                    message_text: Some("hello".to_owned()),
                })),
            ),
            rest: rest_item(
                "text-1",
                "textMessageEvent",
                Some(("textMessageDetails", json!({ "messageText": "hello" }))),
            ),
            kind: event_kinds::CHAT_MESSAGE,
        },
        Case {
            name: "super chat",
            proto: message(
                "sc-1",
                Type::SuperChatEvent as i32,
                Some(Content::SuperChatDetails(LiveChatSuperChatDetails {
                    amount_micros: Some(5_000_000),
                    currency: Some("USD".to_owned()),
                    amount_display_string: Some("$5.00".to_owned()),
                    user_comment: Some("great stream".to_owned()),
                    tier: Some(2),
                })),
            ),
            rest: rest_item(
                "sc-1",
                "superChatEvent",
                Some((
                    "superChatDetails",
                    json!({
                        "amountMicros": "5000000",
                        "currency": "USD",
                        "amountDisplayString": "$5.00",
                        "userComment": "great stream",
                        "tier": 2
                    }),
                )),
            ),
            kind: event_kinds::CHAT_SUPER_CHAT,
        },
        Case {
            name: "super sticker",
            proto: message(
                "ss-1",
                Type::SuperStickerEvent as i32,
                Some(Content::SuperStickerDetails(LiveChatSuperStickerDetails {
                    amount_micros: Some(2_000_000),
                    currency: Some("EUR".to_owned()),
                    amount_display_string: Some("EUR 2.00".to_owned()),
                    tier: Some(1),
                    super_sticker_metadata: Some(SuperStickerMetadata {
                        sticker_id: Some("sticker-7".to_owned()),
                        alt_text: Some("a waving cat".to_owned()),
                        alt_text_language: Some("en".to_owned()),
                    }),
                })),
            ),
            rest: rest_item(
                "ss-1",
                "superStickerEvent",
                Some((
                    "superStickerDetails",
                    json!({
                        "amountMicros": "2000000",
                        "currency": "EUR",
                        "amountDisplayString": "EUR 2.00",
                        "tier": 1,
                        "superStickerMetadata": {
                            "stickerId": "sticker-7",
                            "altText": "a waving cat",
                            "altTextLanguage": "en"
                        }
                    }),
                )),
            ),
            kind: event_kinds::CHAT_SUPER_STICKER,
        },
        Case {
            name: "new sponsor",
            proto: message(
                "ns-1",
                Type::NewSponsorEvent as i32,
                Some(Content::NewSponsorDetails(LiveChatNewSponsorDetails {
                    member_level_name: Some("Gold".to_owned()),
                    is_upgrade: Some(true),
                })),
            ),
            rest: rest_item(
                "ns-1",
                "newSponsorEvent",
                Some((
                    "newSponsorDetails",
                    json!({ "memberLevelName": "Gold", "isUpgrade": true }),
                )),
            ),
            kind: event_kinds::CHANNEL_MEMBER,
        },
        Case {
            name: "member milestone",
            proto: message(
                "mm-1",
                Type::MemberMilestoneChatEvent as i32,
                Some(Content::MemberMilestoneChatDetails(
                    LiveChatMemberMilestoneChatDetails {
                        member_level_name: Some("Gold".to_owned()),
                        member_month: Some(12),
                        user_comment: Some("one year".to_owned()),
                    },
                )),
            ),
            rest: rest_item(
                "mm-1",
                "memberMilestoneChatEvent",
                Some((
                    "memberMilestoneChatDetails",
                    json!({
                        "memberLevelName": "Gold",
                        "memberMonth": 12,
                        "userComment": "one year"
                    }),
                )),
            ),
            kind: event_kinds::CHANNEL_MEMBER_MILESTONE,
        },
        Case {
            name: "membership gifting",
            proto: message(
                "mg-1",
                Type::MembershipGiftingEvent as i32,
                Some(Content::MembershipGiftingDetails(
                    LiveChatMembershipGiftingDetails {
                        gift_memberships_count: Some(5),
                        gift_memberships_level_name: Some("Gold".to_owned()),
                    },
                )),
            ),
            rest: rest_item(
                "mg-1",
                "membershipGiftingEvent",
                Some((
                    "membershipGiftingDetails",
                    json!({ "giftMembershipsCount": 5, "giftMembershipsLevelName": "Gold" }),
                )),
            ),
            kind: event_kinds::CHANNEL_MEMBER_GIFT,
        },
        Case {
            name: "gift membership received",
            proto: message(
                "gr-1",
                Type::GiftMembershipReceivedEvent as i32,
                Some(Content::GiftMembershipReceivedDetails(
                    LiveChatGiftMembershipReceivedDetails {
                        member_level_name: Some("Gold".to_owned()),
                        gifter_channel_id: Some("UCgifter".to_owned()),
                        associated_membership_gifting_message_id: Some("mg-1".to_owned()),
                    },
                )),
            ),
            rest: rest_item(
                "gr-1",
                "giftMembershipReceivedEvent",
                Some((
                    "giftMembershipReceivedDetails",
                    json!({
                        "memberLevelName": "Gold",
                        "gifterChannelId": "UCgifter",
                        "associatedMembershipGiftingMessageId": "mg-1"
                    }),
                )),
            ),
            kind: event_kinds::CHANNEL_MEMBER_GIFT_RECEIVED,
        },
        Case {
            name: "permanent ban",
            proto: message(
                "ban-1",
                Type::UserBannedEvent as i32,
                Some(ban(BanType::Permanent, None)),
            ),
            rest: rest_item(
                "ban-1",
                "userBannedEvent",
                Some((
                    "userBannedDetails",
                    json!({
                        "bannedUserDetails": rest_profile("UCbanned", "Troll"),
                        "banType": "permanent"
                    }),
                )),
            ),
            kind: event_kinds::CHANNEL_USER_BANNED,
        },
        Case {
            name: "temporary ban",
            proto: message(
                "ban-2",
                Type::UserBannedEvent as i32,
                Some(ban(BanType::Temporary, Some(600))),
            ),
            rest: rest_item(
                "ban-2",
                "userBannedEvent",
                Some((
                    "userBannedDetails",
                    json!({
                        "bannedUserDetails": rest_profile("UCbanned", "Troll"),
                        "banType": "temporary",
                        "banDurationSeconds": "600"
                    }),
                )),
            ),
            kind: event_kinds::CHANNEL_USER_BANNED,
        },
        Case {
            name: "tombstone",
            proto: message("tomb-1", Type::Tombstone as i32, None),
            rest: rest_item("tomb-1", "tombstone", None),
            kind: event_kinds::CHAT_MESSAGE_DELETED,
        },
    ]
}

#[test]
fn each_event_type_transcodes_to_its_rest_item_shape() {
    for case in every_event_type() {
        assert_eq!(transcoded(case.proto), case.rest, "{}", case.name);
    }
}

#[test]
fn each_transcoded_event_type_publishes_the_same_event_as_its_rest_item() {
    for case in every_event_type() {
        let from_stream = mapped(&transcoded(case.proto));
        let from_rest = mapped(&case.rest);
        assert_eq!(
            from_stream.as_ref().map(|(kind, _)| kind.as_str()),
            Some(case.kind),
            "{}",
            case.name
        );
        assert_eq!(from_stream, from_rest, "{}", case.name);
    }
}

#[test]
fn a_transcoded_chat_ended_message_is_recognised_as_the_end_of_chat() {
    let item = transcoded(message("end-1", Type::ChatEndedEvent as i32, None));
    assert_eq!(item_type(&item), CHAT_ENDED_EVENT_TYPE);
}

#[test]
fn absent_proto_fields_are_omitted_rather_than_written_as_null() {
    let bare = |kind: i32, content: Option<Content>| LiveChatMessage {
        id: Some("m-1".to_owned()),
        snippet: Some(LiveChatMessageSnippet {
            r#type: Some(kind),
            displayed_content: content,
            ..Default::default()
        }),
        author_details: Some(LiveChatMessageAuthorDetails::default()),
        ..Default::default()
    };
    let cases = [
        (
            "super chat",
            bare(
                Type::SuperChatEvent as i32,
                Some(Content::SuperChatDetails(
                    LiveChatSuperChatDetails::default(),
                )),
            ),
            json!({ "type": "superChatEvent", "superChatDetails": {} }),
        ),
        (
            "super sticker",
            bare(
                Type::SuperStickerEvent as i32,
                Some(Content::SuperStickerDetails(
                    LiveChatSuperStickerDetails::default(),
                )),
            ),
            json!({ "type": "superStickerEvent", "superStickerDetails": {} }),
        ),
        (
            "ban of unknown type",
            bare(
                Type::UserBannedEvent as i32,
                Some(Content::UserBannedDetails(
                    LiveChatUserBannedMessageDetails {
                        ban_type: Some(99),
                        ..Default::default()
                    },
                )),
            ),
            json!({ "type": "userBannedEvent", "userBannedDetails": {} }),
        ),
        (
            "poll",
            bare(
                Type::PollEvent as i32,
                Some(Content::PollDetails(LiveChatPollDetails::default())),
            ),
            json!({ "type": "pollEvent" }),
        ),
        (
            "gift",
            bare(
                Type::GiftEvent as i32,
                Some(Content::GiftDetails(LiveChatGiftDetails::default())),
            ),
            json!({ "type": "giftEvent" }),
        ),
        (
            "invalid type",
            bare(Type::InvalidType as i32, None),
            json!({}),
        ),
        ("unknown type", bare(99, None), json!({})),
    ];
    for (name, proto, snippet) in cases {
        assert_eq!(
            transcoded(proto),
            json!({ "id": "m-1", "snippet": snippet, "authorDetails": {} }),
            "{name}"
        );
    }
}

#[test]
fn a_message_without_snippet_or_author_carries_neither_key() {
    let proto = LiveChatMessage {
        id: Some("m-1".to_owned()),
        ..Default::default()
    };
    assert_eq!(transcoded(proto), json!({ "id": "m-1" }));
}

#[test]
fn types_the_chat_mapper_does_not_know_publish_no_event() {
    let cases = [
        (
            "invalid type",
            message("x-1", Type::InvalidType as i32, None),
        ),
        ("unknown type", message("x-2", 99, None)),
        (
            "poll",
            message(
                "x-3",
                Type::PollEvent as i32,
                Some(Content::PollDetails(LiveChatPollDetails::default())),
            ),
        ),
        (
            "gift",
            message(
                "x-4",
                Type::GiftEvent as i32,
                Some(Content::GiftDetails(LiveChatGiftDetails::default())),
            ),
        ),
    ];
    for (name, proto) in cases {
        assert_eq!(mapped(&transcoded(proto)), None, "{name}");
    }
}

#[test]
fn response_envelope_fields_are_carried_under_their_rest_keys() {
    let response = LiveChatMessageListResponse {
        kind: Some("youtube#liveChatMessageListResponse".to_owned()),
        etag: Some("etag-9".to_owned()),
        offline_at: Some("2026-11-06T09:00:00+00:00".to_owned()),
        next_page_token: Some("page-2".to_owned()),
        page_info: Some(PageInfo {
            total_results: Some(0),
            results_per_page: Some(0),
        }),
        ..Default::default()
    };
    assert_eq!(
        transcode_response(&response),
        json!({
            "kind": "youtube#liveChatMessageListResponse",
            "etag": "etag-9",
            "offlineAt": "2026-11-06T09:00:00+00:00",
            "nextPageToken": "page-2",
            "items": []
        })
    );
}

#[test]
fn an_empty_response_transcodes_to_an_empty_item_list_only() {
    assert_eq!(
        transcode_response(&LiveChatMessageListResponse::default()),
        json!({ "items": [] })
    );
}

#[test]
fn a_real_keep_alive_frame_reads_as_an_empty_page_that_carries_the_next_token() {
    let page = page_from_body(&transcode_response(&keep_alive_frame()));
    assert!(page.items.is_empty());
    assert_eq!(page.next_page_token.as_deref(), Some("next_page_token-1"));
    assert_eq!(page.polling_interval_millis, DEFAULT_POLLING_INTERVAL_MS);
}

#[test]
fn a_real_backlog_message_transcodes_to_its_rest_item() {
    let page = page_from_body(&transcode_response(&backlog_frame()));
    let rest = json!({
        "id": "id-1",
        "etag": "etag-2",
        "snippet": {
            "type": "textMessageEvent",
            "liveChatId": "live_chat_id-1",
            "authorChannelId": "author_channel_id-1",
            "publishedAt": "2026-11-06T07:20:05.111654+00:00",
            "hasDisplayContent": true,
            "displayMessage": "display_message-1",
            "textMessageDetails": { "messageText": "message_text-1" }
        },
        "authorDetails": {
            "channelId": "channel_id-1",
            "channelUrl": "https://yt3.ggpht.com/channel_url-1",
            "displayName": "display_name-1",
            "profileImageUrl": "https://yt3.ggpht.com/profile_image_url-1",
            "isVerified": false,
            "isChatOwner": false,
            "isChatSponsor": false,
            "isChatModerator": false
        }
    });
    assert_eq!(page.items.first(), Some(&rest));
    assert_eq!(mapped(&page.items[0]), mapped(&rest));
}

#[test]
fn every_message_of_a_real_backlog_frame_publishes_a_chat_message_in_order() {
    let frame = backlog_frame();
    let ids: Vec<String> = frame
        .items
        .iter()
        .map(|item| item.id.clone().unwrap())
        .collect();
    let page = page_from_body(&transcode_response(&frame));

    let published: Vec<(String, Value)> = page
        .items
        .iter()
        .filter_map(mapped)
        .map(|(kind, payload)| (kind, payload["message_id"].clone()))
        .collect();

    assert_eq!(ids.len(), 67);
    assert_eq!(
        published,
        ids.iter()
            .map(|id| (event_kinds::CHAT_MESSAGE.to_owned(), json!(id)))
            .collect::<Vec<_>>()
    );
}

fn published_at() -> OffsetDateTime {
    OffsetDateTime::parse(PUBLISHED_AT, &Rfc3339).unwrap()
}

fn later() -> OffsetDateTime {
    published_at() + Duration::hours(2)
}

#[test]
fn a_transcoded_temporary_ban_is_recorded_until_its_stream_timestamp_plus_duration() {
    let item = transcoded(message(
        "ban-2",
        Type::UserBannedEvent as i32,
        Some(ban(BanType::Temporary, Some(600))),
    ));
    let entry = observed_ban(&item, BROADCASTER, later()).unwrap();
    assert_eq!(entry.key.viewer_id, "UCbanned");
    assert_eq!(entry.banned_at, published_at());
    assert_eq!(
        entry.expires_at,
        Some(published_at() + Duration::seconds(600))
    );
}

#[test]
fn a_transcoded_permanent_ban_is_recorded_without_expiry() {
    let item = transcoded(message(
        "ban-1",
        Type::UserBannedEvent as i32,
        Some(ban(BanType::Permanent, None)),
    ));
    let entry = observed_ban(&item, BROADCASTER, later()).unwrap();
    assert_eq!(entry.key.viewer_id, "UCbanned");
    assert_eq!(entry.expires_at, None);
}

#[test]
fn a_transcoded_message_records_its_author_avatar() {
    let item = transcoded(message(
        "text-1",
        Type::TextMessageEvent as i32,
        Some(Content::TextMessageDetails(LiveChatTextMessageDetails {
            message_text: Some("hello".to_owned()),
        })),
    ));
    let avatars = AvatarBook::default();
    avatars.record(&item);
    assert_eq!(avatars.remembered("UCviewer").as_deref(), Some(AVATAR));
}
