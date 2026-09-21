use forge_components::{Density, FONT_XXS, ForgePalette, Spacing, body_family, spacing, tr};
use forge_overlay::{MediaIssue, MediaValue, clip_reference, key_holds_media, read_media_value};
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
    if !key_holds_media(key) {
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
