use forge_registry::ChatEventKind;
use forge_types::{MessageCode, NamesLanguage};

use crate::messages::{self, OVERLAY_MESSAGES};

const FTL_ASSIGNMENT: char = '=';

pub(crate) fn chat_event_label(event: ChatEventKind, language: NamesLanguage) -> Option<String> {
    plain_text(&chat_event_code(event), language)
}

fn chat_event_code(event: ChatEventKind) -> MessageCode {
    match event {
        ChatEventKind::Follow => messages::CHAT_EVENT_FOLLOW,
        ChatEventKind::Subscription => messages::CHAT_EVENT_SUBSCRIPTION,
        ChatEventKind::Resubscription => messages::CHAT_EVENT_RESUBSCRIPTION,
        ChatEventKind::GiftSubscription => messages::CHAT_EVENT_GIFT_SUBSCRIPTION,
        ChatEventKind::Cheer => messages::CHAT_EVENT_CHEER,
        ChatEventKind::Raid => messages::CHAT_EVENT_RAID,
        ChatEventKind::Donation => messages::CHAT_EVENT_DONATION,
        ChatEventKind::SuperChat => messages::CHAT_EVENT_SUPER_CHAT,
        ChatEventKind::SuperSticker => messages::CHAT_EVENT_SUPER_STICKER,
        ChatEventKind::Membership => messages::CHAT_EVENT_MEMBERSHIP,
        ChatEventKind::MembershipGift => messages::CHAT_EVENT_MEMBERSHIP_GIFT,
    }
}

fn plain_text(code: &MessageCode, language: NamesLanguage) -> Option<String> {
    let ftl = match language {
        NamesLanguage::En => OVERLAY_MESSAGES.en_ftl,
        NamesLanguage::Uk => OVERLAY_MESSAGES.uk_ftl,
    };
    let key = code.ftl_key();
    ftl.lines()
        .find_map(|line| {
            line.strip_prefix(key.as_str())?
                .trim_start()
                .strip_prefix(FTL_ASSIGNMENT)
        })
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}
