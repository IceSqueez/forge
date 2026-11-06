#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct LiveChatMessageListRequest {
    #[prost(string, optional, tag = "1")]
    pub live_chat_id: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "2")]
    pub hl: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(uint32, optional, tag = "3")]
    pub profile_image_size: ::core::option::Option<u32>,
    #[prost(uint32, optional, tag = "98")]
    pub max_results: ::core::option::Option<u32>,
    #[prost(string, optional, tag = "99")]
    pub page_token: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, repeated, tag = "100")]
    pub part: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LiveChatMessageListResponse {
    #[prost(string, optional, tag = "200")]
    pub kind: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "201")]
    pub etag: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "2")]
    pub offline_at: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(message, optional, tag = "1004")]
    pub page_info: ::core::option::Option<PageInfo>,
    #[prost(string, optional, tag = "100602")]
    pub next_page_token: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(message, repeated, tag = "1007")]
    pub items: ::prost::alloc::vec::Vec<LiveChatMessage>,
    #[prost(message, optional, tag = "1008")]
    pub active_poll_item: ::core::option::Option<LiveChatMessage>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LiveChatMessage {
    #[prost(string, optional, tag = "200")]
    pub kind: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "201")]
    pub etag: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "101")]
    pub id: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(message, optional, tag = "2")]
    pub snippet: ::core::option::Option<LiveChatMessageSnippet>,
    #[prost(message, optional, tag = "3")]
    pub author_details: ::core::option::Option<LiveChatMessageAuthorDetails>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct LiveChatMessageAuthorDetails {
    #[prost(string, optional, tag = "10101")]
    pub channel_id: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "102")]
    pub channel_url: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "103")]
    pub display_name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "104")]
    pub profile_image_url: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(bool, optional, tag = "4")]
    pub is_verified: ::core::option::Option<bool>,
    #[prost(bool, optional, tag = "5")]
    pub is_chat_owner: ::core::option::Option<bool>,
    #[prost(bool, optional, tag = "6")]
    pub is_chat_sponsor: ::core::option::Option<bool>,
    #[prost(bool, optional, tag = "7")]
    pub is_chat_moderator: ::core::option::Option<bool>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LiveChatMessageSnippet {
    #[prost(
        enumeration = "live_chat_message_snippet::type_wrapper::Type",
        optional,
        tag = "1"
    )]
    pub r#type: ::core::option::Option<i32>,
    #[prost(string, optional, tag = "201")]
    pub live_chat_id: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "301")]
    pub author_channel_id: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "4")]
    pub published_at: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(bool, optional, tag = "17")]
    pub has_display_content: ::core::option::Option<bool>,
    #[prost(string, optional, tag = "16")]
    pub display_message: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(
        oneof = "live_chat_message_snippet::DisplayedContent",
        tags = "19, 22, 27, 28, 29, 30, 31, 32, 33, 34"
    )]
    pub displayed_content: ::core::option::Option<live_chat_message_snippet::DisplayedContent>,
}
pub mod live_chat_message_snippet {
    #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct TypeWrapper {}
    pub mod type_wrapper {
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration,
        )]
        #[repr(i32)]
        pub enum Type {
            InvalidType = 0,
            TextMessageEvent = 1,
            Tombstone = 2,
            FanFundingEvent = 3,
            ChatEndedEvent = 4,
            SponsorOnlyModeStartedEvent = 5,
            SponsorOnlyModeEndedEvent = 6,
            NewSponsorEvent = 7,
            MemberMilestoneChatEvent = 17,
            MembershipGiftingEvent = 18,
            GiftMembershipReceivedEvent = 19,
            UserBannedEvent = 10,
            SuperChatEvent = 15,
            SuperStickerEvent = 16,
            PollEvent = 20,
            GiftEvent = 21,
        }
        impl Type {
            pub fn as_str_name(&self) -> &'static str {
                match self {
                    Self::InvalidType => "INVALID_TYPE",
                    Self::TextMessageEvent => "TEXT_MESSAGE_EVENT",
                    Self::Tombstone => "TOMBSTONE",
                    Self::FanFundingEvent => "FAN_FUNDING_EVENT",
                    Self::ChatEndedEvent => "CHAT_ENDED_EVENT",
                    Self::SponsorOnlyModeStartedEvent => "SPONSOR_ONLY_MODE_STARTED_EVENT",
                    Self::SponsorOnlyModeEndedEvent => "SPONSOR_ONLY_MODE_ENDED_EVENT",
                    Self::NewSponsorEvent => "NEW_SPONSOR_EVENT",
                    Self::MemberMilestoneChatEvent => "MEMBER_MILESTONE_CHAT_EVENT",
                    Self::MembershipGiftingEvent => "MEMBERSHIP_GIFTING_EVENT",
                    Self::GiftMembershipReceivedEvent => "GIFT_MEMBERSHIP_RECEIVED_EVENT",
                    Self::UserBannedEvent => "USER_BANNED_EVENT",
                    Self::SuperChatEvent => "SUPER_CHAT_EVENT",
                    Self::SuperStickerEvent => "SUPER_STICKER_EVENT",
                    Self::PollEvent => "POLL_EVENT",
                    Self::GiftEvent => "GIFT_EVENT",
                }
            }
            pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
                match value {
                    "INVALID_TYPE" => Some(Self::InvalidType),
                    "TEXT_MESSAGE_EVENT" => Some(Self::TextMessageEvent),
                    "TOMBSTONE" => Some(Self::Tombstone),
                    "FAN_FUNDING_EVENT" => Some(Self::FanFundingEvent),
                    "CHAT_ENDED_EVENT" => Some(Self::ChatEndedEvent),
                    "SPONSOR_ONLY_MODE_STARTED_EVENT" => Some(Self::SponsorOnlyModeStartedEvent),
                    "SPONSOR_ONLY_MODE_ENDED_EVENT" => Some(Self::SponsorOnlyModeEndedEvent),
                    "NEW_SPONSOR_EVENT" => Some(Self::NewSponsorEvent),
                    "MEMBER_MILESTONE_CHAT_EVENT" => Some(Self::MemberMilestoneChatEvent),
                    "MEMBERSHIP_GIFTING_EVENT" => Some(Self::MembershipGiftingEvent),
                    "GIFT_MEMBERSHIP_RECEIVED_EVENT" => Some(Self::GiftMembershipReceivedEvent),
                    "USER_BANNED_EVENT" => Some(Self::UserBannedEvent),
                    "SUPER_CHAT_EVENT" => Some(Self::SuperChatEvent),
                    "SUPER_STICKER_EVENT" => Some(Self::SuperStickerEvent),
                    "POLL_EVENT" => Some(Self::PollEvent),
                    "GIFT_EVENT" => Some(Self::GiftEvent),
                    _ => None,
                }
            }
        }
    }
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum DisplayedContent {
        #[prost(message, tag = "19")]
        TextMessageDetails(super::LiveChatTextMessageDetails),
        #[prost(message, tag = "22")]
        UserBannedDetails(super::LiveChatUserBannedMessageDetails),
        #[prost(message, tag = "27")]
        SuperChatDetails(super::LiveChatSuperChatDetails),
        #[prost(message, tag = "28")]
        SuperStickerDetails(super::LiveChatSuperStickerDetails),
        #[prost(message, tag = "29")]
        NewSponsorDetails(super::LiveChatNewSponsorDetails),
        #[prost(message, tag = "30")]
        MemberMilestoneChatDetails(super::LiveChatMemberMilestoneChatDetails),
        #[prost(message, tag = "31")]
        MembershipGiftingDetails(super::LiveChatMembershipGiftingDetails),
        #[prost(message, tag = "32")]
        GiftMembershipReceivedDetails(super::LiveChatGiftMembershipReceivedDetails),
        #[prost(message, tag = "33")]
        PollDetails(super::LiveChatPollDetails),
        #[prost(message, tag = "34")]
        GiftDetails(super::LiveChatGiftDetails),
    }
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct LiveChatTextMessageDetails {
    #[prost(string, optional, tag = "1")]
    pub message_text: ::core::option::Option<::prost::alloc::string::String>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct LiveChatUserBannedMessageDetails {
    #[prost(message, optional, tag = "1")]
    pub banned_user_details: ::core::option::Option<ChannelProfileDetails>,
    #[prost(
        enumeration = "live_chat_user_banned_message_details::ban_type_wrapper::BanType",
        optional,
        tag = "2"
    )]
    pub ban_type: ::core::option::Option<i32>,
    #[prost(uint64, optional, tag = "4")]
    pub ban_duration_seconds: ::core::option::Option<u64>,
}
pub mod live_chat_user_banned_message_details {
    #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct BanTypeWrapper {}
    pub mod ban_type_wrapper {
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration,
        )]
        #[repr(i32)]
        pub enum BanType {
            Permanent = 1,
            Temporary = 2,
        }
        impl BanType {
            pub fn as_str_name(&self) -> &'static str {
                match self {
                    Self::Permanent => "PERMANENT",
                    Self::Temporary => "TEMPORARY",
                }
            }
            pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
                match value {
                    "PERMANENT" => Some(Self::Permanent),
                    "TEMPORARY" => Some(Self::Temporary),
                    _ => None,
                }
            }
        }
    }
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct LiveChatSuperChatDetails {
    #[prost(uint64, optional, tag = "1")]
    pub amount_micros: ::core::option::Option<u64>,
    #[prost(string, optional, tag = "2")]
    pub currency: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "3")]
    pub amount_display_string: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "4")]
    pub user_comment: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(uint32, optional, tag = "5")]
    pub tier: ::core::option::Option<u32>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct LiveChatSuperStickerDetails {
    #[prost(uint64, optional, tag = "1")]
    pub amount_micros: ::core::option::Option<u64>,
    #[prost(string, optional, tag = "2")]
    pub currency: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "3")]
    pub amount_display_string: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(uint32, optional, tag = "4")]
    pub tier: ::core::option::Option<u32>,
    #[prost(message, optional, tag = "5")]
    pub super_sticker_metadata: ::core::option::Option<SuperStickerMetadata>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct LiveChatFanFundingEventDetails {
    #[prost(uint64, optional, tag = "1")]
    pub amount_micros: ::core::option::Option<u64>,
    #[prost(string, optional, tag = "2")]
    pub currency: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "3")]
    pub amount_display_string: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "4")]
    pub user_comment: ::core::option::Option<::prost::alloc::string::String>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct LiveChatNewSponsorDetails {
    #[prost(string, optional, tag = "1")]
    pub member_level_name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(bool, optional, tag = "2")]
    pub is_upgrade: ::core::option::Option<bool>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct LiveChatMemberMilestoneChatDetails {
    #[prost(string, optional, tag = "1")]
    pub member_level_name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(uint32, optional, tag = "2")]
    pub member_month: ::core::option::Option<u32>,
    #[prost(string, optional, tag = "3")]
    pub user_comment: ::core::option::Option<::prost::alloc::string::String>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct LiveChatMembershipGiftingDetails {
    #[prost(int32, optional, tag = "1")]
    pub gift_memberships_count: ::core::option::Option<i32>,
    #[prost(string, optional, tag = "2")]
    pub gift_memberships_level_name: ::core::option::Option<::prost::alloc::string::String>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct LiveChatGiftMembershipReceivedDetails {
    #[prost(string, optional, tag = "1")]
    pub member_level_name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "2")]
    pub gifter_channel_id: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "3")]
    pub associated_membership_gifting_message_id:
        ::core::option::Option<::prost::alloc::string::String>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LiveChatPollDetails {
    #[prost(message, optional, tag = "1")]
    pub metadata: ::core::option::Option<live_chat_poll_details::PollMetadata>,
    #[prost(
        enumeration = "live_chat_poll_details::poll_status_wrapper::PollStatus",
        optional,
        tag = "2"
    )]
    pub status: ::core::option::Option<i32>,
}
pub mod live_chat_poll_details {
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct PollMetadata {
        #[prost(string, optional, tag = "1")]
        pub question_text: ::core::option::Option<::prost::alloc::string::String>,
        #[prost(message, repeated, tag = "2")]
        pub options: ::prost::alloc::vec::Vec<poll_metadata::PollOption>,
    }
    pub mod poll_metadata {
        #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
        pub struct PollOption {
            #[prost(string, optional, tag = "1")]
            pub option_text: ::core::option::Option<::prost::alloc::string::String>,
            #[prost(int64, optional, tag = "2")]
            pub tally: ::core::option::Option<i64>,
        }
    }
    #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct PollStatusWrapper {}
    pub mod poll_status_wrapper {
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration,
        )]
        #[repr(i32)]
        pub enum PollStatus {
            Unknown = 0,
            Active = 1,
            Closed = 2,
        }
        impl PollStatus {
            pub fn as_str_name(&self) -> &'static str {
                match self {
                    Self::Unknown => "UNKNOWN",
                    Self::Active => "ACTIVE",
                    Self::Closed => "CLOSED",
                }
            }
            pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
                match value {
                    "UNKNOWN" => Some(Self::Unknown),
                    "ACTIVE" => Some(Self::Active),
                    "CLOSED" => Some(Self::Closed),
                    _ => None,
                }
            }
        }
    }
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct LiveChatGiftDetails {
    #[prost(string, optional, tag = "1")]
    pub gift_name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(message, optional, tag = "2")]
    pub gift_duration: ::core::option::Option<::prost_types::Duration>,
    #[prost(int32, optional, tag = "3")]
    pub jewels_amount: ::core::option::Option<i32>,
    #[prost(string, optional, tag = "4")]
    pub gift_url: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "5")]
    pub alt_text: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "6")]
    pub language: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(bool, optional, tag = "7")]
    pub has_visual_effect: ::core::option::Option<bool>,
    #[prost(int32, optional, tag = "8")]
    pub combo_count: ::core::option::Option<i32>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct SuperChatEventSnippet {
    #[prost(string, optional, tag = "101")]
    pub channel_id: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(message, optional, tag = "2")]
    pub supporter_details: ::core::option::Option<ChannelProfileDetails>,
    #[prost(string, optional, tag = "3")]
    pub comment_text: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "4")]
    pub created_at: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(uint64, optional, tag = "5")]
    pub amount_micros: ::core::option::Option<u64>,
    #[prost(string, optional, tag = "6")]
    pub currency: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "7")]
    pub display_string: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(uint32, optional, tag = "8")]
    pub message_type: ::core::option::Option<u32>,
    #[prost(bool, optional, tag = "11")]
    pub is_super_sticker_event: ::core::option::Option<bool>,
    #[prost(message, optional, tag = "12")]
    pub super_sticker_metadata: ::core::option::Option<SuperStickerMetadata>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct SuperStickerMetadata {
    #[prost(string, optional, tag = "1")]
    pub sticker_id: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "2")]
    pub alt_text: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "3")]
    pub alt_text_language: ::core::option::Option<::prost::alloc::string::String>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct ChannelProfileDetails {
    #[prost(string, optional, tag = "101")]
    pub channel_id: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "2")]
    pub channel_url: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "3")]
    pub display_name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "4")]
    pub profile_image_url: ::core::option::Option<::prost::alloc::string::String>,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct PageInfo {
    #[prost(int32, optional, tag = "1")]
    pub total_results: ::core::option::Option<i32>,
    #[prost(int32, optional, tag = "2")]
    pub results_per_page: ::core::option::Option<i32>,
}
