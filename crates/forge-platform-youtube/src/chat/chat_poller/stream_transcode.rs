use serde_json::{Map, Value};

use crate::stream_list_proto::live_chat_message_snippet::DisplayedContent;
use crate::stream_list_proto::live_chat_message_snippet::type_wrapper::Type;
use crate::stream_list_proto::live_chat_user_banned_message_details::ban_type_wrapper::BanType;
use crate::stream_list_proto::{
    ChannelProfileDetails, LiveChatGiftMembershipReceivedDetails,
    LiveChatMemberMilestoneChatDetails, LiveChatMembershipGiftingDetails, LiveChatMessage,
    LiveChatMessageAuthorDetails, LiveChatMessageListResponse, LiveChatMessageSnippet,
    LiveChatNewSponsorDetails, LiveChatSuperChatDetails, LiveChatSuperStickerDetails,
    LiveChatTextMessageDetails, LiveChatUserBannedMessageDetails, SuperStickerMetadata,
};

mod key {
    pub(super) const ITEMS: &str = "items";
    pub(super) const KIND: &str = "kind";
    pub(super) const ETAG: &str = "etag";
    pub(super) const ID: &str = "id";
    pub(super) const NEXT_PAGE_TOKEN: &str = "nextPageToken";
    pub(super) const OFFLINE_AT: &str = "offlineAt";
    pub(super) const SNIPPET: &str = "snippet";
    pub(super) const AUTHOR_DETAILS: &str = "authorDetails";
    pub(super) const TYPE: &str = "type";
    pub(super) const LIVE_CHAT_ID: &str = "liveChatId";
    pub(super) const AUTHOR_CHANNEL_ID: &str = "authorChannelId";
    pub(super) const PUBLISHED_AT: &str = "publishedAt";
    pub(super) const HAS_DISPLAY_CONTENT: &str = "hasDisplayContent";
    pub(super) const DISPLAY_MESSAGE: &str = "displayMessage";
    pub(super) const TEXT_MESSAGE_DETAILS: &str = "textMessageDetails";
    pub(super) const MESSAGE_TEXT: &str = "messageText";
    pub(super) const USER_BANNED_DETAILS: &str = "userBannedDetails";
    pub(super) const BANNED_USER_DETAILS: &str = "bannedUserDetails";
    pub(super) const BAN_TYPE: &str = "banType";
    pub(super) const BAN_DURATION_SECONDS: &str = "banDurationSeconds";
    pub(super) const SUPER_CHAT_DETAILS: &str = "superChatDetails";
    pub(super) const SUPER_STICKER_DETAILS: &str = "superStickerDetails";
    pub(super) const SUPER_STICKER_METADATA: &str = "superStickerMetadata";
    pub(super) const STICKER_ID: &str = "stickerId";
    pub(super) const ALT_TEXT: &str = "altText";
    pub(super) const ALT_TEXT_LANGUAGE: &str = "altTextLanguage";
    pub(super) const AMOUNT_MICROS: &str = "amountMicros";
    pub(super) const CURRENCY: &str = "currency";
    pub(super) const AMOUNT_DISPLAY_STRING: &str = "amountDisplayString";
    pub(super) const USER_COMMENT: &str = "userComment";
    pub(super) const TIER: &str = "tier";
    pub(super) const NEW_SPONSOR_DETAILS: &str = "newSponsorDetails";
    pub(super) const MEMBER_LEVEL_NAME: &str = "memberLevelName";
    pub(super) const IS_UPGRADE: &str = "isUpgrade";
    pub(super) const MEMBER_MILESTONE_CHAT_DETAILS: &str = "memberMilestoneChatDetails";
    pub(super) const MEMBER_MONTH: &str = "memberMonth";
    pub(super) const MEMBERSHIP_GIFTING_DETAILS: &str = "membershipGiftingDetails";
    pub(super) const GIFT_MEMBERSHIPS_COUNT: &str = "giftMembershipsCount";
    pub(super) const GIFT_MEMBERSHIPS_LEVEL_NAME: &str = "giftMembershipsLevelName";
    pub(super) const GIFT_MEMBERSHIP_RECEIVED_DETAILS: &str = "giftMembershipReceivedDetails";
    pub(super) const GIFTER_CHANNEL_ID: &str = "gifterChannelId";
    pub(super) const ASSOCIATED_MEMBERSHIP_GIFTING_MESSAGE_ID: &str =
        "associatedMembershipGiftingMessageId";
    pub(super) const CHANNEL_ID: &str = "channelId";
    pub(super) const CHANNEL_URL: &str = "channelUrl";
    pub(super) const DISPLAY_NAME: &str = "displayName";
    pub(super) const PROFILE_IMAGE_URL: &str = "profileImageUrl";
    pub(super) const IS_VERIFIED: &str = "isVerified";
    pub(super) const IS_CHAT_OWNER: &str = "isChatOwner";
    pub(super) const IS_CHAT_SPONSOR: &str = "isChatSponsor";
    pub(super) const IS_CHAT_MODERATOR: &str = "isChatModerator";
}

mod spelling {
    pub(super) const TEXT_MESSAGE_EVENT: &str = "textMessageEvent";
    pub(super) const TOMBSTONE: &str = "tombstone";
    pub(super) const FAN_FUNDING_EVENT: &str = "fanFundingEvent";
    pub(super) const CHAT_ENDED_EVENT: &str = "chatEndedEvent";
    pub(super) const SPONSOR_ONLY_MODE_STARTED_EVENT: &str = "sponsorOnlyModeStartedEvent";
    pub(super) const SPONSOR_ONLY_MODE_ENDED_EVENT: &str = "sponsorOnlyModeEndedEvent";
    pub(super) const NEW_SPONSOR_EVENT: &str = "newSponsorEvent";
    pub(super) const MEMBER_MILESTONE_CHAT_EVENT: &str = "memberMilestoneChatEvent";
    pub(super) const MEMBERSHIP_GIFTING_EVENT: &str = "membershipGiftingEvent";
    pub(super) const GIFT_MEMBERSHIP_RECEIVED_EVENT: &str = "giftMembershipReceivedEvent";
    pub(super) const USER_BANNED_EVENT: &str = "userBannedEvent";
    pub(super) const SUPER_CHAT_EVENT: &str = "superChatEvent";
    pub(super) const SUPER_STICKER_EVENT: &str = "superStickerEvent";
    pub(super) const POLL_EVENT: &str = "pollEvent";
    pub(super) const GIFT_EVENT: &str = "giftEvent";
    pub(super) const BAN_PERMANENT: &str = "permanent";
    pub(super) const BAN_TEMPORARY: &str = "temporary";
}

fn type_spelling(kind: Type) -> Option<&'static str> {
    match kind {
        Type::InvalidType => None,
        Type::TextMessageEvent => Some(spelling::TEXT_MESSAGE_EVENT),
        Type::Tombstone => Some(spelling::TOMBSTONE),
        Type::FanFundingEvent => Some(spelling::FAN_FUNDING_EVENT),
        Type::ChatEndedEvent => Some(spelling::CHAT_ENDED_EVENT),
        Type::SponsorOnlyModeStartedEvent => Some(spelling::SPONSOR_ONLY_MODE_STARTED_EVENT),
        Type::SponsorOnlyModeEndedEvent => Some(spelling::SPONSOR_ONLY_MODE_ENDED_EVENT),
        Type::NewSponsorEvent => Some(spelling::NEW_SPONSOR_EVENT),
        Type::MemberMilestoneChatEvent => Some(spelling::MEMBER_MILESTONE_CHAT_EVENT),
        Type::MembershipGiftingEvent => Some(spelling::MEMBERSHIP_GIFTING_EVENT),
        Type::GiftMembershipReceivedEvent => Some(spelling::GIFT_MEMBERSHIP_RECEIVED_EVENT),
        Type::UserBannedEvent => Some(spelling::USER_BANNED_EVENT),
        Type::SuperChatEvent => Some(spelling::SUPER_CHAT_EVENT),
        Type::SuperStickerEvent => Some(spelling::SUPER_STICKER_EVENT),
        Type::PollEvent => Some(spelling::POLL_EVENT),
        Type::GiftEvent => Some(spelling::GIFT_EVENT),
    }
}

fn ban_spelling(ban: BanType) -> &'static str {
    match ban {
        BanType::Permanent => spelling::BAN_PERMANENT,
        BanType::Temporary => spelling::BAN_TEMPORARY,
    }
}

#[derive(Default)]
struct Fields(Map<String, Value>);

impl Fields {
    fn text(&mut self, name: &str, value: &Option<String>) {
        if let Some(text) = value {
            self.0.insert(name.to_owned(), Value::String(text.clone()));
        }
    }

    fn flag(&mut self, name: &str, value: Option<bool>) {
        if let Some(flag) = value {
            self.0.insert(name.to_owned(), Value::Bool(flag));
        }
    }

    fn number(&mut self, name: &str, value: Option<impl Into<i64>>) {
        if let Some(number) = value {
            self.0.insert(name.to_owned(), Value::from(number.into()));
        }
    }

    fn decimal_string(&mut self, name: &str, value: Option<u64>) {
        if let Some(number) = value {
            self.0
                .insert(name.to_owned(), Value::String(number.to_string()));
        }
    }

    fn nested(&mut self, name: &str, value: Fields) {
        self.0.insert(name.to_owned(), Value::Object(value.0));
    }

    fn finish(self) -> Value {
        Value::Object(self.0)
    }
}

pub(crate) fn transcode_response(response: &LiveChatMessageListResponse) -> Value {
    let mut body = Fields::default();
    body.text(key::KIND, &response.kind);
    body.text(key::ETAG, &response.etag);
    body.text(key::NEXT_PAGE_TOKEN, &response.next_page_token);
    body.text(key::OFFLINE_AT, &response.offline_at);
    body.0.insert(
        key::ITEMS.to_owned(),
        Value::Array(response.items.iter().map(transcode_message).collect()),
    );
    body.finish()
}

fn transcode_message(message: &LiveChatMessage) -> Value {
    let mut item = Fields::default();
    item.text(key::KIND, &message.kind);
    item.text(key::ETAG, &message.etag);
    item.text(key::ID, &message.id);
    if let Some(snippet) = &message.snippet {
        item.nested(key::SNIPPET, transcode_snippet(snippet));
    }
    if let Some(author) = &message.author_details {
        item.nested(key::AUTHOR_DETAILS, transcode_author(author));
    }
    item.finish()
}

fn transcode_author(author: &LiveChatMessageAuthorDetails) -> Fields {
    let mut fields = Fields::default();
    fields.text(key::CHANNEL_ID, &author.channel_id);
    fields.text(key::CHANNEL_URL, &author.channel_url);
    fields.text(key::DISPLAY_NAME, &author.display_name);
    fields.text(key::PROFILE_IMAGE_URL, &author.profile_image_url);
    fields.flag(key::IS_VERIFIED, author.is_verified);
    fields.flag(key::IS_CHAT_OWNER, author.is_chat_owner);
    fields.flag(key::IS_CHAT_SPONSOR, author.is_chat_sponsor);
    fields.flag(key::IS_CHAT_MODERATOR, author.is_chat_moderator);
    fields
}

fn transcode_snippet(snippet: &LiveChatMessageSnippet) -> Fields {
    let mut fields = Fields::default();
    let spelled = snippet
        .r#type
        .and_then(|raw| Type::try_from(raw).ok())
        .and_then(type_spelling);
    if let Some(name) = spelled {
        fields.0.insert(key::TYPE.to_owned(), Value::from(name));
    }
    fields.text(key::LIVE_CHAT_ID, &snippet.live_chat_id);
    fields.text(key::AUTHOR_CHANNEL_ID, &snippet.author_channel_id);
    fields.text(key::PUBLISHED_AT, &snippet.published_at);
    fields.flag(key::HAS_DISPLAY_CONTENT, snippet.has_display_content);
    fields.text(key::DISPLAY_MESSAGE, &snippet.display_message);
    if let Some(content) = &snippet.displayed_content {
        transcode_content(&mut fields, content);
    }
    fields
}

fn transcode_content(fields: &mut Fields, content: &DisplayedContent) {
    match content {
        DisplayedContent::TextMessageDetails(details) => {
            fields.nested(key::TEXT_MESSAGE_DETAILS, text_message(details));
        }
        DisplayedContent::UserBannedDetails(details) => {
            fields.nested(key::USER_BANNED_DETAILS, user_banned(details));
        }
        DisplayedContent::SuperChatDetails(details) => {
            fields.nested(key::SUPER_CHAT_DETAILS, super_chat(details));
        }
        DisplayedContent::SuperStickerDetails(details) => {
            fields.nested(key::SUPER_STICKER_DETAILS, super_sticker(details));
        }
        DisplayedContent::NewSponsorDetails(details) => {
            fields.nested(key::NEW_SPONSOR_DETAILS, new_sponsor(details));
        }
        DisplayedContent::MemberMilestoneChatDetails(details) => {
            fields.nested(
                key::MEMBER_MILESTONE_CHAT_DETAILS,
                member_milestone(details),
            );
        }
        DisplayedContent::MembershipGiftingDetails(details) => {
            fields.nested(key::MEMBERSHIP_GIFTING_DETAILS, membership_gifting(details));
        }
        DisplayedContent::GiftMembershipReceivedDetails(details) => {
            fields.nested(
                key::GIFT_MEMBERSHIP_RECEIVED_DETAILS,
                gift_membership_received(details),
            );
        }
        DisplayedContent::PollDetails(_) | DisplayedContent::GiftDetails(_) => {}
    }
}

fn text_message(details: &LiveChatTextMessageDetails) -> Fields {
    let mut fields = Fields::default();
    fields.text(key::MESSAGE_TEXT, &details.message_text);
    fields
}

fn profile(details: &ChannelProfileDetails) -> Fields {
    let mut fields = Fields::default();
    fields.text(key::CHANNEL_ID, &details.channel_id);
    fields.text(key::CHANNEL_URL, &details.channel_url);
    fields.text(key::DISPLAY_NAME, &details.display_name);
    fields.text(key::PROFILE_IMAGE_URL, &details.profile_image_url);
    fields
}

fn user_banned(details: &LiveChatUserBannedMessageDetails) -> Fields {
    let mut fields = Fields::default();
    if let Some(banned) = &details.banned_user_details {
        fields.nested(key::BANNED_USER_DETAILS, profile(banned));
    }
    let ban = details
        .ban_type
        .and_then(|raw| BanType::try_from(raw).ok())
        .map(ban_spelling);
    if let Some(name) = ban {
        fields.0.insert(key::BAN_TYPE.to_owned(), Value::from(name));
    }
    fields.decimal_string(key::BAN_DURATION_SECONDS, details.ban_duration_seconds);
    fields
}

fn super_chat(details: &LiveChatSuperChatDetails) -> Fields {
    let mut fields = Fields::default();
    fields.decimal_string(key::AMOUNT_MICROS, details.amount_micros);
    fields.text(key::CURRENCY, &details.currency);
    fields.text(key::AMOUNT_DISPLAY_STRING, &details.amount_display_string);
    fields.text(key::USER_COMMENT, &details.user_comment);
    fields.number(key::TIER, details.tier);
    fields
}

fn sticker_metadata(metadata: &SuperStickerMetadata) -> Fields {
    let mut fields = Fields::default();
    fields.text(key::STICKER_ID, &metadata.sticker_id);
    fields.text(key::ALT_TEXT, &metadata.alt_text);
    fields.text(key::ALT_TEXT_LANGUAGE, &metadata.alt_text_language);
    fields
}

fn super_sticker(details: &LiveChatSuperStickerDetails) -> Fields {
    let mut fields = Fields::default();
    fields.decimal_string(key::AMOUNT_MICROS, details.amount_micros);
    fields.text(key::CURRENCY, &details.currency);
    fields.text(key::AMOUNT_DISPLAY_STRING, &details.amount_display_string);
    fields.number(key::TIER, details.tier);
    if let Some(metadata) = &details.super_sticker_metadata {
        fields.nested(key::SUPER_STICKER_METADATA, sticker_metadata(metadata));
    }
    fields
}

fn new_sponsor(details: &LiveChatNewSponsorDetails) -> Fields {
    let mut fields = Fields::default();
    fields.text(key::MEMBER_LEVEL_NAME, &details.member_level_name);
    fields.flag(key::IS_UPGRADE, details.is_upgrade);
    fields
}

fn member_milestone(details: &LiveChatMemberMilestoneChatDetails) -> Fields {
    let mut fields = Fields::default();
    fields.text(key::MEMBER_LEVEL_NAME, &details.member_level_name);
    fields.number(key::MEMBER_MONTH, details.member_month);
    fields.text(key::USER_COMMENT, &details.user_comment);
    fields
}

fn membership_gifting(details: &LiveChatMembershipGiftingDetails) -> Fields {
    let mut fields = Fields::default();
    fields.number(key::GIFT_MEMBERSHIPS_COUNT, details.gift_memberships_count);
    fields.text(
        key::GIFT_MEMBERSHIPS_LEVEL_NAME,
        &details.gift_memberships_level_name,
    );
    fields
}

fn gift_membership_received(details: &LiveChatGiftMembershipReceivedDetails) -> Fields {
    let mut fields = Fields::default();
    fields.text(key::MEMBER_LEVEL_NAME, &details.member_level_name);
    fields.text(key::GIFTER_CHANNEL_ID, &details.gifter_channel_id);
    fields.text(
        key::ASSOCIATED_MEMBERSHIP_GIFTING_MESSAGE_ID,
        &details.associated_membership_gifting_message_id,
    );
    fields
}

#[cfg(test)]
mod tests;
