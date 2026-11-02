use forge_types::{ArgStack, Variant};

pub const CHAT_EVENT_VARIABLE: &str = "chat_event";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChatEventKind {
    Follow,
    Subscription,
    Resubscription,
    GiftSubscription,
    Cheer,
    Raid,
    Donation,
    SuperChat,
    SuperSticker,
    Membership,
    MembershipGift,
}

impl ChatEventKind {
    pub const ALL: [ChatEventKind; 11] = [
        ChatEventKind::Follow,
        ChatEventKind::Subscription,
        ChatEventKind::Resubscription,
        ChatEventKind::GiftSubscription,
        ChatEventKind::Cheer,
        ChatEventKind::Raid,
        ChatEventKind::Donation,
        ChatEventKind::SuperChat,
        ChatEventKind::SuperSticker,
        ChatEventKind::Membership,
        ChatEventKind::MembershipGift,
    ];

    pub const fn wire_id(self) -> &'static str {
        match self {
            ChatEventKind::Follow => "follow",
            ChatEventKind::Subscription => "subscription",
            ChatEventKind::Resubscription => "resubscription",
            ChatEventKind::GiftSubscription => "gift_subscription",
            ChatEventKind::Cheer => "cheer",
            ChatEventKind::Raid => "raid",
            ChatEventKind::Donation => "donation",
            ChatEventKind::SuperChat => "super_chat",
            ChatEventKind::SuperSticker => "super_sticker",
            ChatEventKind::Membership => "membership",
            ChatEventKind::MembershipGift => "membership_gift",
        }
    }

    pub fn from_wire_id(wire: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.wire_id() == wire)
    }

    pub fn of_run(args: &ArgStack) -> Option<Self> {
        args.get(CHAT_EVENT_VARIABLE)
            .and_then(Variant::as_str)
            .and_then(Self::from_wire_id)
    }

    pub fn marked(self, args: ArgStack) -> ArgStack {
        args.set(
            CHAT_EVENT_VARIABLE.to_owned(),
            Variant::String(self.wire_id().to_owned()),
        )
    }
}
