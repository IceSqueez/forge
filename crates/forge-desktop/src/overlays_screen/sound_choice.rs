use forge_components::{Density, FONT_XXS, ForgePalette, Spacing, body_family, spacing, tr};
use forge_overlay::{
    MediaIssue, MediaSlot, MediaValue, clip_reference, media_slot, read_media_value,
};
use forge_soundboard::{AdoptionVerdict, SoundboardError};
use forge_storage::StoredClip;
use forge_types::ClipId;
use gpui::{AnyElement, div, prelude::*};

use crate::clip_messages::{clip_refusal_message, failure_message};

pub(super) const NO_SOUND: &str = "";

pub(super) type AdoptionResult = Result<Option<(String, AdoptionVerdict)>, SoundboardError>;

pub(super) enum PickOutcome {
    Accepted,
    Refused(String),
}

pub(super) fn clip_choices(clips: &[StoredClip]) -> Vec<(String, String)> {
    clips
        .iter()
        .map(|clip| (clip_reference(&clip.id.to_string()), clip.name.clone()))
        .collect()
}

pub(super) fn sound_choices(clips: &[(String, String)], selected: &str) -> Vec<(String, String)> {
    let mut options = vec![(NO_SOUND.to_owned(), tr!("overlays_sound_none"))];
    options.extend(clips.iter().cloned());
    if let MediaValue::File(name) = read_media_value(selected) {
        options.push((
            selected.to_owned(),
            tr!("overlays_sound_custom_file", name = name),
        ));
    }
    options
}

pub(super) fn picked_clip(key: &str, value: &str) -> Option<ClipId> {
    if media_slot(key) != Some(MediaSlot::Sound) {
        return None;
    }
    match read_media_value(value) {
        MediaValue::Clip(clip) => clip.parse().ok(),
        MediaValue::Empty | MediaValue::File(_) => None,
    }
}

pub(super) fn pick_outcome(result: AdoptionResult) -> PickOutcome {
    match result {
        Ok(None) => PickOutcome::Refused(tr!("soundboard_error_clip_gone")),
        Ok(Some((
            _,
            AdoptionVerdict::Adopted | AdoptionVerdict::AlreadyManaged | AdoptionVerdict::InFlight,
        ))) => PickOutcome::Accepted,
        Ok(Some((name, AdoptionVerdict::SourceMissing))) => {
            PickOutcome::Refused(tr!("soundboard_error_source_missing", name = name.as_str()))
        }
        Ok(Some((_, AdoptionVerdict::Refused(refusal)))) => {
            PickOutcome::Refused(clip_refusal_message(&refusal))
        }
        Err(error) => PickOutcome::Refused(failure_message(&error)),
    }
}

pub(super) fn issue_message(issue: &MediaIssue) -> String {
    match issue {
        MediaIssue::UnknownClip { .. } => tr!("overlays_sound_issue_unknown_clip"),
        MediaIssue::ClipOutsideLibrary { .. } => tr!("overlays_sound_issue_outside_library"),
        MediaIssue::ClipBytesMissing { .. } => tr!("overlays_sound_issue_bytes_missing"),
        MediaIssue::ClipKindMismatch { format, .. } => {
            tr!(
                "overlays_sound_issue_kind_mismatch",
                format = format.as_str()
            )
        }
        MediaIssue::UnknownGlyph { .. } => tr!("overlays_icon_issue_unknown_glyph"),
        MediaIssue::UnknownImage { .. } => tr!("overlays_icon_issue_unknown_image"),
        MediaIssue::ImageBytesMissing { .. } => tr!("overlays_icon_issue_bytes_missing"),
        MediaIssue::ImageKindMismatch { format, .. } => {
            tr!(
                "overlays_icon_issue_kind_mismatch",
                format = format.as_str()
            )
        }
        MediaIssue::LookupFailed { reason, .. } => {
            tr!(
                "overlays_sound_issue_lookup_failed",
                reason = reason.as_str()
            )
        }
    }
}

pub(super) fn field_notes(
    issues: &[MediaIssue],
    key: &str,
    adopting: bool,
    refusal: Option<&str>,
) -> Vec<String> {
    if adopting {
        return vec![tr!("overlays_sound_adopting")];
    }
    let mut notes: Vec<String> = issues
        .iter()
        .filter(|issue| issue.key() == key)
        .map(issue_message)
        .collect();
    notes.extend(refusal.map(str::to_owned));
    notes
}

pub(super) fn badge_note(issues: &[MediaIssue]) -> Option<String> {
    issues.first().map(issue_message)
}

pub(super) fn notes_block(notes: Vec<String>, palette: &ForgePalette) -> Option<AnyElement> {
    if notes.is_empty() {
        return None;
    }
    let mut column = div()
        .mt(spacing(Spacing::Xxs, Density::Cozy))
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, Density::Cozy))
        .font_family(body_family())
        .text_size(FONT_XXS)
        .text_color(palette.warning);
    for note in notes {
        column = column.child(note);
    }
    Some(column.into_any_element())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::path::PathBuf;

    use forge_overlay::config::{HEADLINE, SOUND};
    use forge_soundboard::ClipRefusal;
    use forge_storage::MediaFormat;
    use forge_types::OutputDevice;
    use time::OffsetDateTime;

    use super::*;

    const CLIP_ULID: &str = "01J9P4S2M7Q8V3X5Y6Z7A8B9C0";
    const OTHER_ULID: &str = "01J9P4S2M7Q8V3X5Y6Z7A8B9C1";
    const LEGACY_FILE: &str = "fanfare.mp3";

    fn clip(id: &str, name: &str) -> StoredClip {
        StoredClip {
            id: id.parse().expect("a ulid fixture"),
            name: name.to_owned(),
            file_path: PathBuf::from("/library/clip.wav"),
            volume: 1.0,
            output_device: OutputDevice::Default,
            hotkey: None,
            created_at: OffsetDateTime::UNIX_EPOCH,
            category: "General".to_owned(),
            loop_playback: false,
            duration_secs: None,
            builtin_id: None,
        }
    }

    fn library() -> Vec<(String, String)> {
        clip_choices(&[clip(CLIP_ULID, "Fanfare"), clip(OTHER_ULID, "Airhorn")])
    }

    fn values(options: &[(String, String)]) -> Vec<String> {
        options.iter().map(|(value, _)| value.clone()).collect()
    }

    #[test]
    fn the_sound_list_always_opens_with_the_silent_choice_before_the_library() {
        let offered = sound_choices(&library(), &clip_reference(CLIP_ULID));

        assert_eq!(
            values(&offered),
            vec![
                NO_SOUND.to_owned(),
                clip_reference(CLIP_ULID),
                clip_reference(OTHER_ULID),
            ],
            "picking silence must stay reachable and the library order must survive"
        );
        assert_eq!(offered[0].1, "overlays_sound_none");
    }

    #[test]
    fn a_hand_placed_filename_is_offered_back_so_it_can_survive_being_reopened() {
        let offered = sound_choices(&library(), LEGACY_FILE);

        assert_eq!(
            offered.last().expect("the list is never empty"),
            &(
                LEGACY_FILE.to_owned(),
                "overlays_sound_custom_file".to_owned()
            ),
            "a value the library cannot offer must still be selectable, or opening the panel loses it"
        );
    }

    #[test]
    fn a_library_reference_never_gets_a_second_custom_file_entry() {
        let offered = sound_choices(&library(), &clip_reference(CLIP_ULID));

        assert_eq!(
            offered.len(),
            library().len() + 1,
            "a stored reference was offered twice: {offered:?}"
        );
    }

    #[test]
    fn only_a_media_key_holding_a_parseable_reference_names_a_clip_to_adopt() {
        let reference = clip_reference(CLIP_ULID);
        for (key, value, expected, label) in [
            (
                SOUND,
                reference.as_str(),
                Some(CLIP_ULID),
                "a reference under the sound key",
            ),
            (SOUND, "", None, "an empty pick"),
            (SOUND, LEGACY_FILE, None, "a hand-placed filename"),
            (SOUND, "clip:not-a-ulid", None, "an unreadable clip id"),
            (
                HEADLINE,
                reference.as_str(),
                None,
                "wording that opens with the prefix",
            ),
        ] {
            assert_eq!(
                picked_clip(key, value).map(|clip| clip.to_string()),
                expected.map(str::to_owned),
                "{label}"
            );
        }
    }

    #[test]
    fn a_pick_is_accepted_only_when_the_clip_ends_up_in_the_library() {
        for (result, accepted, label) in [
            (
                Ok(Some(("Fanfare".to_owned(), AdoptionVerdict::Adopted))),
                true,
                "a fresh copy",
            ),
            (
                Ok(Some((
                    "Fanfare".to_owned(),
                    AdoptionVerdict::AlreadyManaged,
                ))),
                true,
                "a clip already in the library",
            ),
            (
                Ok(Some(("Fanfare".to_owned(), AdoptionVerdict::InFlight))),
                true,
                "a copy another pass already started",
            ),
            (
                Ok(Some(("Fanfare".to_owned(), AdoptionVerdict::SourceMissing))),
                false,
                "a clip whose original file is gone",
            ),
            (
                Ok(Some((
                    "Fanfare".to_owned(),
                    AdoptionVerdict::Refused(ClipRefusal::Unsupported {
                        label: "notes.txt".to_owned(),
                    }),
                ))),
                false,
                "a refused import",
            ),
            (
                Ok(None),
                false,
                "a clip that vanished between list and pick",
            ),
            (
                Err(SoundboardError::ClipNotFound(CLIP_ULID.to_owned())),
                false,
                "a store that could not answer",
            ),
        ] {
            assert_eq!(refusal_of(result).is_none(), accepted, "{label}");
        }
    }

    fn refusal_of(result: AdoptionResult) -> Option<String> {
        match pick_outcome(result) {
            PickOutcome::Accepted => None,
            PickOutcome::Refused(message) => Some(message),
        }
    }

    #[test]
    fn a_refused_pick_carries_the_reason_the_user_can_act_on() {
        for (result, expected, label) in [
            (
                Ok(Some(("Fanfare".to_owned(), AdoptionVerdict::SourceMissing))),
                "soundboard_error_source_missing",
                "a vanished original",
            ),
            (Ok(None), "soundboard_error_clip_gone", "a vanished row"),
            (
                Ok(Some((
                    "Fanfare".to_owned(),
                    AdoptionVerdict::Refused(ClipRefusal::TypeMismatch {
                        label: "fanfare.wav".to_owned(),
                        claimed: MediaFormat::Wav,
                        detected: MediaFormat::Mp3,
                    }),
                ))),
                "soundboard_import_type_mismatch",
                "a file whose bytes contradict its name",
            ),
        ] {
            assert_eq!(refusal_of(result).as_deref(), Some(expected), "{label}");
        }
    }

    fn issue(kind: usize) -> MediaIssue {
        let key = SOUND.to_owned();
        let clip = CLIP_ULID.to_owned();
        match kind {
            0 => MediaIssue::UnknownClip { key, clip },
            1 => MediaIssue::ClipOutsideLibrary { key, clip },
            2 => MediaIssue::ClipBytesMissing { key, clip },
            3 => MediaIssue::ClipKindMismatch {
                key,
                clip,
                format: MediaFormat::Png.to_string(),
            },
            _ => MediaIssue::LookupFailed {
                key,
                clip,
                reason: "store offline".to_owned(),
            },
        }
    }

    #[test]
    fn every_unresolved_reference_states_its_own_reason() {
        let messages: Vec<String> = (0..5).map(|kind| issue_message(&issue(kind))).collect();

        assert_eq!(
            messages,
            vec![
                "overlays_sound_issue_unknown_clip",
                "overlays_sound_issue_outside_library",
                "overlays_sound_issue_bytes_missing",
                "overlays_sound_issue_kind_mismatch",
                "overlays_sound_issue_lookup_failed",
            ]
        );
    }

    #[test]
    fn a_running_copy_replaces_every_other_note_until_it_settles() {
        let notes = field_notes(&[issue(0)], SOUND, true, Some("refused earlier"));

        assert_eq!(notes, vec!["overlays_sound_adopting".to_owned()]);
    }

    #[test]
    fn a_field_shows_its_own_issues_followed_by_the_refusal_of_the_last_pick() {
        let notes = field_notes(
            &[
                issue(0),
                MediaIssue::UnknownClip {
                    key: HEADLINE.to_owned(),
                    clip: CLIP_ULID.to_owned(),
                },
            ],
            SOUND,
            false,
            Some("refused"),
        );

        assert_eq!(
            notes,
            vec![
                "overlays_sound_issue_unknown_clip".to_owned(),
                "refused".to_owned(),
            ],
            "a field must show only the issues raised against its own key"
        );
    }

    #[test]
    fn a_record_with_nothing_unresolved_carries_no_badge() {
        assert_eq!(badge_note(&[]), None);
        assert_eq!(
            badge_note(&[issue(1)]),
            Some("overlays_sound_issue_outside_library".to_owned())
        );
    }

    fn catalog_entry(locale: &str, key: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("locales")
            .join(locale)
            .join("main.ftl");
        let catalog = std::fs::read_to_string(&path).unwrap();
        let head = format!("{key} =");
        catalog
            .lines()
            .find(|line| line.starts_with(&head))
            .unwrap_or_else(|| panic!("{locale}/main.ftl is missing {key}"))
            .to_owned()
    }

    #[test]
    fn every_sound_picker_message_interpolates_the_argument_its_call_site_passes() {
        for (key, arguments) in [
            ("overlays_sound_none", &[][..]),
            ("overlays_sound_custom_file", &["name"][..]),
            ("overlays_sound_adopting", &[][..]),
            ("overlays_sound_issue_unknown_clip", &[][..]),
            ("overlays_sound_issue_outside_library", &[][..]),
            ("overlays_sound_issue_bytes_missing", &[][..]),
            ("overlays_sound_issue_kind_mismatch", &["format"][..]),
            ("overlays_sound_issue_lookup_failed", &["reason"][..]),
        ] {
            for locale in ["en", "uk"] {
                let entry = catalog_entry(locale, key);
                for argument in arguments {
                    assert!(
                        entry.contains(&format!("${argument}")),
                        "{locale}/main.ftl defines {key} without ${argument}"
                    );
                }
            }
        }
    }
}
