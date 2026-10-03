use forge_components::tr;
use forge_overlay::config::SLOT;
use forge_types::{LATEST_DONATION_SLOT, NOW_PLAYING_SLOT};

pub(crate) fn latest_option_label(key: &str, option: &str) -> Option<String> {
    (key == SLOT).then(|| slot_label(option)).flatten()
}

pub(crate) fn slot_label(slot: &str) -> Option<String> {
    match slot {
        LATEST_DONATION_SLOT => Some(tr!("latest_slot_donation")),
        NOW_PLAYING_SLOT => Some(tr!("latest_slot_now_playing")),
        _ => None,
    }
}
