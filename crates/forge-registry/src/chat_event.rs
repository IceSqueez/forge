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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_round_trips_through_its_wire_id() {
        for kind in ChatEventKind::ALL {
            assert_eq!(ChatEventKind::from_wire_id(kind.wire_id()), Some(kind));
        }
    }

    #[test]
    fn from_wire_id_rejects_unknown_and_non_canonical_ids() {
        for wire in [
            "",
            "host",
            "Raid",
            "raid ",
            " raid",
            "super-chat",
            "gift sub",
        ] {
            assert_eq!(ChatEventKind::from_wire_id(wire), None, "{wire:?}");
        }
    }

    #[test]
    fn of_run_reads_back_the_kind_a_run_was_marked_with() {
        for kind in ChatEventKind::ALL {
            assert_eq!(
                ChatEventKind::of_run(&kind.marked(ArgStack::new())),
                Some(kind)
            );
        }
    }

    #[test]
    fn of_run_ignores_an_absent_unknown_or_non_string_marker() {
        let unknown = ArgStack::new().set(
            CHAT_EVENT_VARIABLE.to_owned(),
            Variant::String("host".to_owned()),
        );
        let non_string = ArgStack::new().set(CHAT_EVENT_VARIABLE.to_owned(), Variant::Int(5));
        for args in [ArgStack::new(), unknown, non_string] {
            assert_eq!(ChatEventKind::of_run(&args), None);
        }
    }
}
