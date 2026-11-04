#![expect(clippy::expect_used, reason = "test fixtures assert directly")]
mod common;
use common::{
    ALERT_KIND, BLOB_IDENTITY, CHAT_KIND, IDENTITY, UNSAFE_IDENTITIES, names_in, registry,
    sub_alert_instance, unborn_root, with_kind,
};

use std::fs;

use forge_overlay::config::{HEADLINE, IMAGE, SOUND};
use forge_overlay::{
    OverlayConfig, OverlayError, RUNTIME_SOURCE, ResolvedMedia, SHOW_IMAGE, SHOW_MEDIA_DIRECTORY,
    SHOW_SOUND, clear_show_media, clip_reference, image_reference, materialize_overlay,
    place_show_media, show_media_overrides, show_media_page_path, without_show_media,
};
use forge_types::{ArgStack, Variant};
use tempfile::TempDir;

const CLIP_ULID: &str = "01J9P4S2M7Q8V3X5Y6Z7A8B9C0";
const PNG: &str = "png";
const PICTURE_BYTES: &[u8] = b"\x89PNG\r\n\x1a\n";

fn text(value: &str) -> Variant {
    Variant::String(value.to_owned())
}

fn supplied(entries: &[(&str, &str)]) -> OverlayConfig {
    entries
        .iter()
        .map(|(key, value)| ((*key).to_owned(), text(value)))
        .collect()
}

fn overrides(kind_id: &str, step: &OverlayConfig, args: &ArgStack) -> OverlayConfig {
    with_kind(kind_id, |descriptor| {
        show_media_overrides(descriptor, step, args)
    })
}

fn picture() -> ResolvedMedia {
    ResolvedMedia::new(IMAGE, BLOB_IDENTITY, PNG, PICTURE_BYTES.to_vec())
        .expect("a content id is a safe token")
}

#[test]
fn a_step_pick_replaces_the_overlay_image_and_sound_and_nothing_else() {
    let step = supplied(&[
        (HEADLINE, "New follower"),
        (SHOW_IMAGE, &image_reference(BLOB_IDENTITY)),
        (SHOW_SOUND, &clip_reference(CLIP_ULID)),
    ]);

    assert_eq!(
        overrides(ALERT_KIND, &step, &ArgStack::new()),
        supplied(&[
            (IMAGE, &image_reference(BLOB_IDENTITY)),
            (SOUND, &clip_reference(CLIP_ULID)),
        ])
    );
}

#[test]
fn a_blank_step_field_leaves_the_overlay_its_own_media() {
    for blank in ["", "   ", "\t\n"] {
        let step = supplied(&[(SHOW_IMAGE, blank), (SHOW_SOUND, blank)]);

        assert!(
            overrides(ALERT_KIND, &step, &ArgStack::new()).is_empty(),
            "{blank:?} replaced the overlay's own media"
        );
    }
}

#[test]
fn a_step_pick_written_as_a_variable_is_read_from_the_run() {
    let args = ArgStack::new().set("sound".to_owned(), text(&clip_reference(CLIP_ULID)));
    let step = supplied(&[(SHOW_SOUND, " %sound% ")]);

    assert_eq!(
        overrides(ALERT_KIND, &step, &args),
        supplied(&[(SOUND, &clip_reference(CLIP_ULID))])
    );
}

#[test]
fn a_look_that_draws_no_image_ignores_a_picked_image_but_still_plays_a_picked_sound() {
    let step = supplied(&[
        (SHOW_IMAGE, &image_reference(BLOB_IDENTITY)),
        (SHOW_SOUND, &clip_reference(CLIP_ULID)),
    ]);

    assert_eq!(
        overrides(CHAT_KIND, &step, &ArgStack::new()),
        supplied(&[(SOUND, &clip_reference(CLIP_ULID))])
    );
}

#[test]
fn kept_content_drops_the_show_media_so_a_reconnecting_page_never_replays_it() {
    let delivered = supplied(&[
        (HEADLINE, "New follower"),
        (SHOW_IMAGE, "forge-show-media/a.png"),
        (SHOW_SOUND, "forge-show-media/b.wav"),
    ]);

    assert_eq!(
        without_show_media(&delivered),
        supplied(&[(HEADLINE, "New follower")])
    );
}

#[test]
fn placing_show_media_for_an_overlay_without_a_folder_writes_nothing() {
    let home = TempDir::new().unwrap();
    let root = unborn_root(&home);
    fs::create_dir_all(&root).unwrap();

    let placed =
        place_show_media(&root, IDENTITY, &[picture()]).expect("an absent folder is no error");

    assert!(!placed);
    assert!(
        names_in(&root).is_empty(),
        "show media created an overlay folder"
    );
}

#[test]
fn the_page_path_of_placed_show_media_names_the_file_written_beside_the_page() {
    let home = TempDir::new().unwrap();
    let root = unborn_root(&home);
    let report = materialize_overlay(&root, &sub_alert_instance(ALERT_KIND), &registry())
        .expect("the overlay materializes");

    let placed = place_show_media(&root, IDENTITY, &[picture()]).expect("the media is placed");

    assert!(placed);
    assert_eq!(
        fs::read(report.directory.join(show_media_page_path(&picture()))).unwrap(),
        PICTURE_BYTES,
        "the page would ask for a file the show media was not written to"
    );
}

#[test]
fn show_media_never_lands_in_the_namespace_a_materialize_pass_sweeps() {
    let home = TempDir::new().unwrap();
    let root = unborn_root(&home);
    let reg = registry();
    let report = materialize_overlay(&root, &sub_alert_instance(ALERT_KIND), &reg)
        .expect("the overlay materializes");
    place_show_media(&root, IDENTITY, &[picture()]).expect("the media is placed");

    materialize_overlay(&root, &sub_alert_instance(ALERT_KIND), &reg).expect("a second pass");

    assert_eq!(
        names_in(&report.directory.join(SHOW_MEDIA_DIRECTORY)),
        vec![picture().file_name().to_owned()],
        "regenerating the page took a file a show on screen still plays"
    );
}

#[test]
fn clearing_show_media_takes_its_folder_and_leaves_the_page_alone() {
    let home = TempDir::new().unwrap();
    let root = unborn_root(&home);
    let report = materialize_overlay(&root, &sub_alert_instance(ALERT_KIND), &registry())
        .expect("the overlay materializes");
    let page = names_in(&report.directory);
    place_show_media(&root, IDENTITY, &[picture()]).expect("the media is placed");

    clear_show_media(&root, IDENTITY).expect("the folder clears");

    assert_eq!(names_in(&report.directory), page);
}

#[test]
fn clearing_show_media_that_was_never_placed_is_no_error() {
    let home = TempDir::new().unwrap();
    let root = unborn_root(&home);

    assert!(clear_show_media(&root, IDENTITY).is_ok(), "a missing root");
    materialize_overlay(&root, &sub_alert_instance(ALERT_KIND), &registry())
        .expect("the overlay materializes");
    assert!(
        clear_show_media(&root, IDENTITY).is_ok(),
        "a missing folder"
    );
}

#[cfg(unix)]
#[test]
fn a_show_media_folder_that_is_a_symbolic_link_is_refused_both_ways() {
    let home = TempDir::new().unwrap();
    let root = unborn_root(&home);
    let elsewhere = home.path().join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    fs::write(elsewhere.join("keep.txt"), b"victim").unwrap();
    let report = materialize_overlay(&root, &sub_alert_instance(ALERT_KIND), &registry())
        .expect("the overlay materializes");
    std::os::unix::fs::symlink(&elsewhere, report.directory.join(SHOW_MEDIA_DIRECTORY)).unwrap();

    let placed = place_show_media(&root, IDENTITY, &[picture()]);
    let cleared = clear_show_media(&root, IDENTITY);

    assert!(
        matches!(placed, Err(OverlayError::SymlinkedPath { .. })),
        "{placed:?}"
    );
    assert!(
        matches!(cleared, Err(OverlayError::SymlinkedPath { .. })),
        "{cleared:?}"
    );
    assert_eq!(names_in(&elsewhere), vec!["keep.txt".to_owned()]);
}

#[test]
fn show_media_refuses_an_identity_that_is_not_a_plain_directory_name() {
    let home = TempDir::new().unwrap();
    let root = unborn_root(&home);
    fs::create_dir_all(&root).unwrap();

    for &(identity, _) in UNSAFE_IDENTITIES {
        assert!(
            place_show_media(&root, identity, &[picture()]).is_err(),
            "{identity:?} was placed"
        );
        assert!(
            clear_show_media(&root, identity).is_err(),
            "{identity:?} was cleared"
        );
    }
}

#[test]
fn the_runtime_plays_a_picked_show_sound_in_place_of_the_overlay_sound_and_hides_it_from_the_look()
{
    let deliver = common::js_function(RUNTIME_SOURCE, "deliver");
    let stripped = common::js_function(RUNTIME_SOURCE, "withoutShowToken");
    let present = common::js_function(RUNTIME_SOURCE, "present");

    assert!(RUNTIME_SOURCE.contains(&format!("var SHOW_SOUND_KEY = \"{SHOW_SOUND}\";")));
    assert!(deliver.contains("values[SHOW_SOUND_KEY]"));
    assert!(
        stripped.contains("key !== SHOW_SOUND_KEY"),
        "a look would receive the show sound and could play it a second time"
    );
    assert!(
        present.contains("showSound || (config && config.sound)"),
        "a picked show sound no longer wins over the overlay's own sound"
    );
}

#[test]
fn the_alert_page_draws_a_picked_show_image_in_place_of_its_own() {
    let reg = registry();
    let script = reg
        .get(ALERT_KIND)
        .expect("the alert kind ships")
        .page_assets()
        .behavior;

    assert!(script.contains(&format!("var SHOW_IMAGE_KEY = \"{SHOW_IMAGE}\";")));
    assert!(script.contains("values[SHOW_IMAGE_KEY] || config.image"));
}
