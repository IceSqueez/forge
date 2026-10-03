use std::sync::LazyLock;

use forge_events::{DONATION_RECEIVED_KIND, Event, NOW_PLAYING_KIND};
use forge_types::{LATEST_DONATION_SLOT, LatestValue, NOW_PLAYING_SLOT};

use crate::latest::feeds::{
    CHEER_KIND, SUPER_CHAT_KIND, SUPER_STICKER_KIND, read_cheer, read_donation, read_now_playing,
    read_super_chat, read_super_sticker,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotRetention {
    SurvivesRestart,
    LiveOnly,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SlotReading {
    Value(LatestValue),
    Ended { platform: String },
}

pub struct FeedContext<'a> {
    pub anonymous_name: &'a str,
}

pub struct SlotFeed {
    pub kind: &'static str,
    pub read: fn(&Event, &FeedContext<'_>) -> Option<SlotReading>,
}

pub struct LatestSlotDeclaration {
    pub slot: &'static str,
    pub retention: SlotRetention,
    pub feeds: &'static [SlotFeed],
}

pub static LATEST_SLOTS: &[LatestSlotDeclaration] = &[
    LatestSlotDeclaration {
        slot: LATEST_DONATION_SLOT,
        retention: SlotRetention::SurvivesRestart,
        feeds: &[
            SlotFeed {
                kind: DONATION_RECEIVED_KIND,
                read: read_donation,
            },
            SlotFeed {
                kind: SUPER_CHAT_KIND,
                read: read_super_chat,
            },
            SlotFeed {
                kind: SUPER_STICKER_KIND,
                read: read_super_sticker,
            },
            SlotFeed {
                kind: CHEER_KIND,
                read: read_cheer,
            },
        ],
    },
    LatestSlotDeclaration {
        slot: NOW_PLAYING_SLOT,
        retention: SlotRetention::LiveOnly,
        feeds: &[SlotFeed {
            kind: NOW_PLAYING_KIND,
            read: read_now_playing,
        }],
    },
];

static SLOT_IDS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    LATEST_SLOTS
        .iter()
        .map(|declaration| declaration.slot)
        .collect()
});

pub fn latest_slot_ids() -> &'static [&'static str] {
    SLOT_IDS.as_slice()
}

pub fn slot_declaration(slot: &str) -> Option<&'static LatestSlotDeclaration> {
    LATEST_SLOTS
        .iter()
        .find(|declaration| declaration.slot == slot)
}
