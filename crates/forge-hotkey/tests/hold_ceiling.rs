use forge_hotkey::{DEFAULT_HOLD_CEILING_SECS, HOLD_CEILING, load_hold_ceiling, save_hold_ceiling};
use forge_storage::test_support::MemorySettings;

#[tokio::test]
async fn load_hold_ceiling_treats_zero_as_off_and_falls_back_to_the_default() {
    let settings = MemorySettings::new();

    assert_eq!(
        load_hold_ceiling(&settings).await,
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
        settings.seed_raw(&HOLD_CEILING, stored);
        assert_eq!(
            load_hold_ceiling(&settings).await,
            expected,
            "wrong ceiling for stored {stored:?}"
        );
    }
}

#[tokio::test]
async fn save_hold_ceiling_round_trips_the_off_state_through_a_stored_zero() {
    let settings = MemorySettings::new();

    save_hold_ceiling(&settings, None).await.unwrap();
    assert_eq!(load_hold_ceiling(&settings).await, None);

    save_hold_ceiling(&settings, Some(45)).await.unwrap();
    assert_eq!(load_hold_ceiling(&settings).await, Some(45));
}
