use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use forge_components::{
    Density, ForgePalette, Icon, Spacing, icon, mono_family, secondary_button, spacing, tr,
};
use forge_soundboard::{AdoptionVerdict, ClipAvailability, ClipRefusal, SoundboardError};
use forge_storage::StoredClip;
use forge_types::ClipId;
use gpui::{AnyElement, ClickEvent, Context, SharedString, div, prelude::*};

use super::footer::{FOOTER_FS, FOOTER_PAD_X, FOOTER_PAD_Y};
use super::pad::LOOP_ICON;
use super::{SoundClip, SoundboardView};
use crate::async_bridge;
use crate::clip_messages::{clip_refusal_message, failure_message};

const ADOPT_SUMMARY_SEPARATOR: &str = " · ";

type AdoptionResults = Vec<(ClipId, Result<AdoptionVerdict, SoundboardError>)>;

impl SoundboardView {
    pub(super) fn apply_settled_adoption(
        &mut self,
        clip_id: ClipId,
        settled: SettledAdoption,
        cx: &mut Context<Self>,
    ) {
        let availability = match settled {
            SettledAdoption::Copied => {
                self.refresh_library_size(cx);
                ClipAvailability::Managed
            }
            SettledAdoption::AlreadyManaged => ClipAvailability::Managed,
            SettledAdoption::SourceMissing => ClipAvailability::Missing,
            SettledAdoption::Refused => {
                self.read_refusal(clip_id, cx);
                return;
            }
        };
        self.refusal_reads.remove(&clip_id);
        self.availability.insert(clip_id, availability);
        cx.notify();
    }

    fn read_refusal(&mut self, clip_id: ClipId, cx: &mut Context<Self>) {
        self.refusal_reads.insert(clip_id);
        let library = Arc::clone(&self.library);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let clip = library.get(clip_id).await?;
                Ok::<_, SoundboardError>(match clip {
                    Some(clip) => Some(library.availability(&clip).await),
                    None => None,
                })
            },
            move |this, result, cx| {
                if !this.refusal_reads.remove(&clip_id) {
                    return;
                }
                match result {
                    Ok(Some(availability)) => {
                        this.availability.insert(clip_id, availability);
                        cx.notify();
                    }
                    Ok(None) => {}
                    Err(error) => {
                        tracing::warn!(clip_id = %clip_id, error = %error, "clip refusal lookup failed");
                    }
                }
            },
            cx,
        );
    }

    pub(super) fn refresh_availability(&self, clips: Vec<StoredClip>, cx: &mut Context<Self>) {
        let ticket = self.availability_gen.next();
        let library = Arc::clone(&self.library);
        async_bridge::run_async(
            &self.rt_handle,
            async move { library.availability_of(&clips).await },
            move |this, states, cx| {
                if !this.availability_gen.is_current(ticket) {
                    return;
                }
                this.availability = states.into_iter().collect();
                cx.notify();
            },
            cx,
        );
    }

    fn reload_availability(&self, cx: &mut Context<Self>) {
        let ticket = self.availability_gen.next();
        let library = Arc::clone(&self.library);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let clips = library.list().await?;
                Ok::<_, SoundboardError>(library.availability_of(&clips).await)
            },
            move |this, result, cx| {
                if !this.availability_gen.is_current(ticket) {
                    return;
                }
                match result {
                    Ok(states) => {
                        this.availability = states.into_iter().collect();
                        cx.notify();
                    }
                    Err(error) => {
                        tracing::warn!(error = %error, "clip availability refresh failed");
                    }
                }
            },
            cx,
        );
    }

    fn adopt_all(&mut self, cx: &mut Context<Self>) {
        let targets: HashSet<ClipId> = unadopted_ids(&self.clips, &self.availability)
            .into_iter()
            .collect();
        if self.adopting_all || targets.is_empty() {
            return;
        }
        self.adopting_all = true;
        self.adopt_summary = None;
        cx.notify();

        let library = Arc::clone(&self.library);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let clips: Vec<StoredClip> = library
                    .list()
                    .await?
                    .into_iter()
                    .filter(|clip| targets.contains(&clip.id))
                    .collect();
                Ok::<_, SoundboardError>(library.adopt_all_now(&clips).await)
            },
            |this, result, cx| this.on_adopted(&result, cx),
            cx,
        );
    }

    fn on_adopted(
        &mut self,
        result: &Result<AdoptionResults, SoundboardError>,
        cx: &mut Context<Self>,
    ) {
        self.adopting_all = false;
        self.adopt_summary = Some(match result {
            Ok(verdicts) => {
                adoption_summary(&tally_adoption(verdicts.iter().map(|(_, r)| r))).into()
            }
            Err(error) => failure_message(error).into(),
        });
        self.reload_availability(cx);
        self.refresh_library_size(cx);
        cx.notify();
    }

    pub(super) fn source_is_missing(&self, id: ClipId) -> bool {
        matches!(self.availability.get(&id), Some(ClipAvailability::Missing))
    }

    pub(super) fn render_adoption_strip(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let pending = unadopted_ids(&self.clips, &self.availability).len();
        if pending == 0 && self.adopt_summary.is_none() {
            return None;
        }
        let mut row = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(spacing(Spacing::Sm, density))
            .py(FOOTER_PAD_Y)
            .px(FOOTER_PAD_X);

        let mut left = div()
            .flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap(spacing(Spacing::Xs, density));
        if pending > 0 {
            left = left
                .child(icon(Icon::Copy, LOOP_ICON, palette.text_muted))
                .child(
                    div()
                        .font_family(mono_family())
                        .text_size(FOOTER_FS)
                        .text_color(palette.text_muted)
                        .child(tr!("soundboard_footer_unadopted", count = pending as i64)),
                );
        }
        if let Some(summary) = self.adopt_summary.clone() {
            left = left.child(
                div()
                    .ml(spacing(Spacing::Sm, density))
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .font_family(mono_family())
                    .text_size(FOOTER_FS)
                    .text_color(palette.text_faint)
                    .child(summary),
            );
        }
        row = row.child(left);

        if pending > 0 {
            let label = if self.adopting_all {
                tr!("soundboard_footer_adopt_busy")
            } else {
                tr!("soundboard_footer_adopt_action", count = pending as i64)
            };
            row = row.child(
                secondary_button(label, palette)
                    .density(density)
                    .disabled(self.adopting_all)
                    .on_click(
                        "sb-adopt-all",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.adopt_all(cx)),
                    ),
            );
        }
        Some(row.into_any_element())
    }
}

enum AdoptionBadge {
    Blocked(ClipRefusal),
    Pending,
}

pub(super) enum SettledAdoption {
    Copied,
    AlreadyManaged,
    SourceMissing,
    Refused,
}

pub(super) fn settled_adoption(payload: &serde_json::Value) -> Option<SettledAdoption> {
    match payload.get("verdict").and_then(|v| v.as_str())? {
        "adopted" => Some(SettledAdoption::Copied),
        "already_managed" => Some(SettledAdoption::AlreadyManaged),
        "source_missing" => Some(SettledAdoption::SourceMissing),
        "refused" => Some(SettledAdoption::Refused),
        _ => None,
    }
}

pub(super) enum LibraryBadge {
    Blocked(SharedString),
    Pending,
}

pub(super) fn library_badge(availability: Option<&ClipAvailability>) -> Option<LibraryBadge> {
    match adoption_badge(availability)? {
        AdoptionBadge::Blocked(refusal) => {
            Some(LibraryBadge::Blocked(clip_refusal_message(&refusal).into()))
        }
        AdoptionBadge::Pending => Some(LibraryBadge::Pending),
    }
}

fn adoption_badge(availability: Option<&ClipAvailability>) -> Option<AdoptionBadge> {
    match availability {
        Some(ClipAvailability::Unadopted {
            refusal: Some(refusal),
        }) => Some(AdoptionBadge::Blocked(refusal.clone())),
        Some(ClipAvailability::Unadopted { refusal: None }) => Some(AdoptionBadge::Pending),
        _ => None,
    }
}

fn unadopted_ids(
    clips: &[SoundClip],
    availability: &HashMap<ClipId, ClipAvailability>,
) -> Vec<ClipId> {
    clips
        .iter()
        .map(|clip| clip.id)
        .filter(|id| {
            matches!(
                availability.get(id),
                Some(ClipAvailability::Unadopted { .. })
            )
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum AdoptionOutcome {
    Copied,
    Refused,
    Missing,
    Unfinished,
}

fn classify_adoption(result: &Result<AdoptionVerdict, SoundboardError>) -> AdoptionOutcome {
    match result {
        Ok(AdoptionVerdict::Adopted | AdoptionVerdict::AlreadyManaged) => AdoptionOutcome::Copied,
        Ok(AdoptionVerdict::Refused(_)) => AdoptionOutcome::Refused,
        Ok(AdoptionVerdict::SourceMissing) => AdoptionOutcome::Missing,
        Ok(AdoptionVerdict::InFlight) | Err(_) => AdoptionOutcome::Unfinished,
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
struct AdoptionTally {
    copied: usize,
    refused: usize,
    missing: usize,
    unfinished: usize,
}

fn tally_adoption<'a>(
    results: impl IntoIterator<Item = &'a Result<AdoptionVerdict, SoundboardError>>,
) -> AdoptionTally {
    let mut tally = AdoptionTally::default();
    for result in results {
        match classify_adoption(result) {
            AdoptionOutcome::Copied => tally.copied += 1,
            AdoptionOutcome::Refused => tally.refused += 1,
            AdoptionOutcome::Missing => tally.missing += 1,
            AdoptionOutcome::Unfinished => tally.unfinished += 1,
        }
    }
    tally
}

fn adoption_summary(tally: &AdoptionTally) -> String {
    let mut parts: Vec<String> = Vec::new();
    if tally.copied > 0 {
        parts.push(tr!("soundboard_adopt_copied", count = tally.copied as i64));
    }
    if tally.refused > 0 {
        parts.push(tr!(
            "soundboard_adopt_refused",
            count = tally.refused as i64
        ));
    }
    if tally.missing > 0 {
        parts.push(tr!(
            "soundboard_adopt_missing",
            count = tally.missing as i64
        ));
    }
    if tally.unfinished > 0 {
        parts.push(tr!(
            "soundboard_adopt_unfinished",
            count = tally.unfinished as i64
        ));
    }
    if parts.is_empty() {
        return tr!("soundboard_adopt_nothing");
    }
    parts.join(ADOPT_SUMMARY_SEPARATOR)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::path::PathBuf;

    use forge_storage::{MediaFormat, MediaKind};

    use super::*;

    fn sound_clip(id: ClipId) -> SoundClip {
        SoundClip {
            id,
            name: String::new(),
            file_path: PathBuf::new(),
            hotkey: None,
            category: String::new(),
            loop_playback: false,
            duration_secs: None,
            builtin_id: None,
            glyph: Icon::Music,
        }
    }

    fn unsupported(label: &str) -> ClipRefusal {
        ClipRefusal::Unsupported {
            label: label.to_owned(),
        }
    }

    #[test]
    fn unadopted_ids_names_every_row_the_library_has_not_taken_in() {
        let ids: Vec<ClipId> = (0..5).map(|_| ClipId::new()).collect();
        let clips: Vec<SoundClip> = ids.iter().copied().map(sound_clip).collect();
        let availability = HashMap::from([
            (ids[0], ClipAvailability::Managed),
            (ids[1], ClipAvailability::Unadopted { refusal: None }),
            (ids[2], ClipAvailability::Missing),
            (
                ids[3],
                ClipAvailability::Unadopted {
                    refusal: Some(unsupported("notes.txt")),
                },
            ),
        ]);

        assert_eq!(unadopted_ids(&clips, &availability), vec![ids[1], ids[3]]);
    }

    #[test]
    fn a_run_of_adoptions_is_counted_by_what_each_verdict_means_for_the_user() {
        let results = [
            Ok(AdoptionVerdict::Adopted),
            Ok(AdoptionVerdict::AlreadyManaged),
            Ok(AdoptionVerdict::Refused(unsupported("notes.txt"))),
            Ok(AdoptionVerdict::SourceMissing),
            Ok(AdoptionVerdict::InFlight),
            Err(SoundboardError::Storage("disk is busy".to_owned())),
        ];

        assert_eq!(
            tally_adoption(results.iter()),
            AdoptionTally {
                copied: 2,
                refused: 1,
                missing: 1,
                unfinished: 2,
            }
        );
    }

    #[test]
    fn the_adoption_summary_names_only_the_outcomes_that_happened() {
        for (tally, expected) in [
            (
                AdoptionTally {
                    copied: 2,
                    refused: 1,
                    missing: 1,
                    unfinished: 1,
                },
                "soundboard_adopt_copied · soundboard_adopt_refused · soundboard_adopt_missing · soundboard_adopt_unfinished",
            ),
            (
                AdoptionTally {
                    copied: 3,
                    ..AdoptionTally::default()
                },
                "soundboard_adopt_copied",
            ),
            (
                AdoptionTally {
                    refused: 1,
                    unfinished: 2,
                    ..AdoptionTally::default()
                },
                "soundboard_adopt_refused · soundboard_adopt_unfinished",
            ),
            (AdoptionTally::default(), "soundboard_adopt_nothing"),
        ] {
            assert_eq!(adoption_summary(&tally), expected, "{tally:?}");
        }
    }

    #[test]
    fn a_blocked_badge_explains_itself_with_the_key_that_matches_the_refusal() {
        for (refusal, expected_key) in [
            (unsupported("notes.txt"), "soundboard_import_unsupported"),
            (
                ClipRefusal::TypeMismatch {
                    label: "logo.mp3".to_owned(),
                    claimed: MediaFormat::Mp3,
                    detected: MediaFormat::Png,
                },
                "soundboard_import_type_mismatch",
            ),
            (
                ClipRefusal::TooLarge {
                    label: "huge.wav".to_owned(),
                    size: 60 * 1024 * 1024,
                    limit: 50 * 1024 * 1024,
                    kind: MediaKind::Audio,
                },
                "soundboard_import_too_large",
            ),
        ] {
            assert_eq!(clip_refusal_message(&refusal), expected_key, "{refusal:?}");
        }
    }

    fn settled(payload: serde_json::Value) -> Option<(&'static str, Option<String>)> {
        settled_adoption(&payload).map(|settled| match settled {
            SettledAdoption::Copied => ("copied", None),
            SettledAdoption::AlreadyManaged => ("already_managed", None),
            SettledAdoption::SourceMissing => ("source_missing", None),
            SettledAdoption::Refused => ("refused", None),
        })
    }

    fn badge(availability: Option<ClipAvailability>) -> Option<(&'static str, Option<String>)> {
        library_badge(availability.as_ref()).map(|badge| match badge {
            LibraryBadge::Blocked(reason) => ("blocked", Some(reason.to_string())),
            LibraryBadge::Pending => ("pending", None),
        })
    }

    #[test]
    fn each_settled_adoption_verdict_maps_to_its_outcome() {
        for (payload, expected) in [
            (
                serde_json::json!({ "verdict": "adopted" }),
                ("copied", None),
            ),
            (
                serde_json::json!({ "verdict": "already_managed" }),
                ("already_managed", None),
            ),
            (
                serde_json::json!({ "verdict": "source_missing" }),
                ("source_missing", None),
            ),
            (
                serde_json::json!({ "verdict": "refused", "reason": "notes.txt is not audio" }),
                ("refused", None),
            ),
            (
                serde_json::json!({ "verdict": "refused" }),
                ("refused", None),
            ),
            (
                serde_json::json!({ "verdict": "refused", "reason": "" }),
                ("refused", None),
            ),
            (
                serde_json::json!({ "verdict": "refused", "reason": 3 }),
                ("refused", None),
            ),
        ] {
            assert_eq!(settled(payload.clone()), Some(expected), "{payload}");
        }
    }

    #[test]
    fn an_adoption_event_without_a_usable_verdict_settles_nothing() {
        for payload in [
            serde_json::json!({}),
            serde_json::json!({ "verdict": "in_flight" }),
            serde_json::json!({ "verdict": "ADOPTED" }),
            serde_json::json!({ "verdict": 1 }),
        ] {
            assert_eq!(settled(payload.clone()), None, "{payload}");
        }
    }

    #[test]
    fn the_pad_badge_follows_the_stored_availability() {
        let refusal = unsupported("notes.txt");
        for (availability, expected) in [
            (None, None),
            (Some(ClipAvailability::Managed), None),
            (Some(ClipAvailability::Missing), None),
            (
                Some(ClipAvailability::Unadopted { refusal: None }),
                Some(("pending", None)),
            ),
            (
                Some(ClipAvailability::Unadopted {
                    refusal: Some(refusal.clone()),
                }),
                Some(("blocked", Some(clip_refusal_message(&refusal)))),
            ),
        ] {
            assert_eq!(badge(availability.clone()), expected, "{availability:?}");
        }
    }
}
