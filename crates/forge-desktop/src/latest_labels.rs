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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_slot_key_is_labelled_and_only_for_slots_this_build_knows() {
        assert!(latest_option_label(SLOT, LATEST_DONATION_SLOT).is_some());
        assert!(latest_option_label(SLOT, NOW_PLAYING_SLOT).is_some());
        assert_ne!(
            latest_option_label(SLOT, LATEST_DONATION_SLOT),
            latest_option_label(SLOT, NOW_PLAYING_SLOT)
        );
        assert_eq!(latest_option_label(SLOT, "written_by_a_newer_build"), None);
        assert_eq!(latest_option_label("platform", LATEST_DONATION_SLOT), None);
    }
}
