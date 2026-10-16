#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;

use forge_hotkey::{
    DEFAULT_HOLD_CEILING_SECS, HOTKEY_HOLD_CEILING_KEY, load_hold_ceiling, save_hold_ceiling,
};
use forge_storage::SettingsRepo;
use support::store;

#[tokio::test]
async fn load_hold_ceiling_treats_zero_as_off_and_falls_back_to_the_default() {
    let store = store().await;
    let settings = Arc::clone(&store.provider) as Arc<dyn SettingsRepo>;

    assert_eq!(
        load_hold_ceiling(settings.as_ref()).await,
        Some(DEFAULT_HOLD_CEILING_SECS),
        "a fresh install runs with the ceiling on"
    );

    let cases = [
        ("0", None),
        ("1", Some(1)),
        ("45", Some(45)),
        (" 45 ", Some(45)),
        ("3600", Some(3600)),
        ("", Some(DEFAULT_HOLD_CEILING_SECS)),
        ("-1", Some(DEFAULT_HOLD_CEILING_SECS)),
        ("30s", Some(DEFAULT_HOLD_CEILING_SECS)),
    ];
    for (stored, expected) in cases {
        settings
            .set_string(HOTKEY_HOLD_CEILING_KEY, stored)
            .await
            .unwrap();
        assert_eq!(
            load_hold_ceiling(settings.as_ref()).await,
            expected,
            "wrong ceiling for stored {stored:?}"
        );
    }
}

#[tokio::test]
async fn save_hold_ceiling_round_trips_the_off_state_through_a_stored_zero() {
    let store = store().await;
    let settings = Arc::clone(&store.provider) as Arc<dyn SettingsRepo>;

    save_hold_ceiling(settings.as_ref(), None).await.unwrap();
    assert_eq!(load_hold_ceiling(settings.as_ref()).await, None);

    save_hold_ceiling(settings.as_ref(), Some(45))
        .await
        .unwrap();
    assert_eq!(load_hold_ceiling(settings.as_ref()).await, Some(45));
}
