use std::sync::Arc;

use forge_components::{ConfirmTone, ForgePalette, OverlayPosition, confirm_modal, overlay, tr};
use forge_storage::{DataProvider, StoredClip};
use forge_types::{ClipId, TriggerInstanceId};
use gpui::{AnyElement, ClickEvent, Context, SharedString, prelude::*};

use crate::clip_hotkeys::{HotkeySyncedClipsRepo, clear_clip_combo, stored_combo};
use crate::hotkey_bindings::{BindingRow, delete_binding};
use crate::hotkey_sync::HotkeyReconciler;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipKey {
    pub id: ClipId,
    pub name: String,
    pub combo: String,
}

pub fn clip_keys(clips: &[StoredClip]) -> Vec<ClipKey> {
    clips
        .iter()
        .filter_map(|clip| {
            stored_combo(clip).map(|combo| ClipKey {
                id: clip.id,
                name: clip.name.clone(),
                combo,
            })
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Claimant {
    /// A new half may join a combo whose row still has its other edge free.
    NewTrigger,
    Trigger(TriggerInstanceId),
    Clip(Option<ClipId>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComboHolder {
    Action(Option<String>),
    Clip { id: ClipId, name: String },
}

impl ComboHolder {
    pub fn label(&self) -> String {
        match self {
            ComboHolder::Action(Some(name)) => name.clone(),
            ComboHolder::Action(None) => tr!("hotkeys_conflict_holder_unassigned"),
            ComboHolder::Clip { name, .. } => {
                tr!("hotkeys_conflict_holder_clip", name = name.as_str())
            }
        }
    }
}

/// Excludes the target by half membership, not by row key: a hold's release half is keyed by its press partner.
pub fn combo_holder<'a>(
    rows: &'a [BindingRow],
    combo: &str,
    target: Option<TriggerInstanceId>,
) -> Option<&'a BindingRow> {
    rows.iter().find(|row| {
        row.combo == combo
            && !row
                .halves()
                .any(|(_, half)| Some(half.instance_id) == target)
    })
}

pub fn holder_of_combo(
    rows: &[BindingRow],
    clips: &[ClipKey],
    combo: &str,
    claimant: Claimant,
) -> Option<ComboHolder> {
    let row = match claimant {
        Claimant::NewTrigger => {
            combo_holder(rows, combo, None).filter(|row| row.free_edge().is_none())
        }
        Claimant::Trigger(target) => combo_holder(rows, combo, Some(target)),
        Claimant::Clip(_) => combo_holder(rows, combo, None),
    };
    if let Some(row) = row {
        return Some(ComboHolder::Action(
            row.primary_action().map(|(_, name)| name.clone()),
        ));
    }
    clips
        .iter()
        .find(|clip| clip.combo == combo && claimant != Claimant::Clip(Some(clip.id)))
        .map(|clip| ComboHolder::Clip {
            id: clip.id,
            name: clip.name.clone(),
        })
}

#[derive(Clone, Default)]
pub struct ComboHolders {
    pub rows: Arc<Vec<BindingRow>>,
    pub clips: Vec<ClipKey>,
}

impl ComboHolders {
    pub fn holder_of(&self, combo: &str, claimant: Claimant) -> Option<ComboHolder> {
        holder_of_combo(&self.rows, &self.clips, combo, claimant)
    }
}

pub async fn release_holder(
    holder: ComboHolder,
    combo: String,
    reconciler: Arc<HotkeyReconciler>,
    backend: Arc<dyn DataProvider>,
) -> Result<(), String> {
    match holder {
        ComboHolder::Action(_) => delete_binding(reconciler, backend, combo).await,
        ComboHolder::Clip { id, .. } => {
            let repo =
                HotkeySyncedClipsRepo::wrap(backend.soundboard_clips_repo(), Some(reconciler));
            clear_clip_combo(repo, id).await
        }
    }
}

pub fn conflict_prompt<V: 'static>(
    scope: &'static str,
    item_name: impl Into<SharedString>,
    holder: &str,
    palette: &ForgePalette,
    cx: &mut Context<V>,
    on_cancel: fn(&mut V, &mut Context<V>),
    on_replace: fn(&mut V, &mut Context<V>),
) -> AnyElement {
    let card = confirm_modal(
        tr!("hotkeys_conflict_title"),
        tr!("hotkeys_conflict_body", holder = holder),
        ConfirmTone::Destructive,
        palette,
    )
    .item_name(item_name)
    .on_cancel(
        SharedString::from(format!("{scope}-conflict-cancel")),
        tr!("common_cancel"),
        cx.listener(move |this, _: &ClickEvent, _, cx| on_cancel(this, cx)),
    )
    .on_confirm(
        SharedString::from(format!("{scope}-conflict-replace")),
        tr!("hotkeys_conflict_replace"),
        cx.listener(move |this, _: &ClickEvent, _, cx| on_replace(this, cx)),
    );

    let weak = cx.entity().downgrade();
    overlay(card, palette)
        .position(OverlayPosition::Center)
        .on_dismiss(
            SharedString::from(format!("{scope}-conflict-dismiss")),
            move |_window, cx| {
                let _ = weak.update(cx, on_cancel);
            },
        )
        .into_any_element()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::BTreeMap;

    use forge_storage::Language;
    use forge_types::{
        ActionId, OutputDevice, PermissionRung, PlatformScope, TriggerInstance, Variant,
    };
    use time::OffsetDateTime;

    use super::*;
    use crate::hotkey_bindings::{BindingHalf, COMBO_FIELD, HOTKEY_PRESSED_KIND};
    use crate::test_support::{Sandboxed, sandboxed_backend};

    const TEST_KEY: [u8; 32] = [0x11; 32];

    fn half(instance_id: TriggerInstanceId, action: Option<&str>) -> BindingHalf {
        BindingHalf {
            instance_id,
            enabled: true,
            action: action.map(|name| (ActionId::new(), name.to_owned())),
        }
    }

    fn press_row(combo: &str, action: Option<&str>) -> BindingRow {
        let press = TriggerInstanceId::new();
        BindingRow {
            key: press,
            combo: combo.to_owned(),
            registered: true,
            press: Some(half(press, action)),
            release: None,
        }
    }

    /// A grouped hold row plus the instance ids of its press and release halves.
    fn hold_row(combo: &str) -> (BindingRow, TriggerInstanceId, TriggerInstanceId) {
        let press = TriggerInstanceId::new();
        let release = TriggerInstanceId::new();
        let row = BindingRow {
            key: press,
            combo: combo.to_owned(),
            registered: true,
            press: Some(half(press, Some("Mute mic"))),
            release: Some(half(release, None)),
        };
        (row, press, release)
    }

    fn clip_key(combo: &str, name: &str) -> ClipKey {
        ClipKey {
            id: ClipId::new(),
            name: name.to_owned(),
            combo: combo.to_owned(),
        }
    }

    fn stored_clip(name: &str, hotkey: Option<&str>) -> StoredClip {
        StoredClip {
            id: ClipId::new(),
            name: name.to_owned(),
            file_path: format!("/clips/{name}.wav").into(),
            volume: 1.0,
            output_device: OutputDevice::Default,
            hotkey: hotkey.map(str::to_owned),
            created_at: OffsetDateTime::now_utc(),
            category: String::new(),
            loop_playback: false,
            duration_secs: None,
            builtin_id: None,
        }
    }

    #[test]
    fn a_hold_is_one_holder_for_whichever_half_is_being_edited() {
        // A hold's row is keyed by its PRESS half, so a key comparison would report the row as
        // its own conflict while the user edits the RELEASE half - and confirming Replace
        // deletes both halves of the row being edited.
        let (row, press, release) = hold_row("Ctrl+F1");
        let rows = vec![row];

        for (case, claimant, expect_holder) in [
            ("editing the press half", Claimant::Trigger(press), false),
            (
                "editing the release half",
                Claimant::Trigger(release),
                false,
            ),
            (
                "editing an unrelated binding",
                Claimant::Trigger(TriggerInstanceId::new()),
                true,
            ),
            ("adding a new binding", Claimant::NewTrigger, true),
        ] {
            assert_eq!(
                holder_of_combo(&rows, &[], "Ctrl+F1", claimant).is_some(),
                expect_holder,
                "wrong holder verdict while {case}"
            );
        }
    }

    #[test]
    fn combo_holder_ignores_rows_bound_to_another_combo() {
        let (row, _, _) = hold_row("Ctrl+F1");
        let rows = vec![row];

        assert!(combo_holder(&rows, "Ctrl+F2", None).is_none());
    }

    #[test]
    fn only_a_new_trigger_may_join_a_row_whose_other_edge_is_free() {
        let rows = vec![press_row("F9", Some("Scene"))];

        for (claimant, expected) in [
            (Claimant::NewTrigger, None),
            (
                Claimant::Clip(None),
                Some(ComboHolder::Action(Some("Scene".to_owned()))),
            ),
        ] {
            assert_eq!(
                holder_of_combo(&rows, &[], "F9", claimant),
                expected,
                "{claimant:?}"
            );
        }
    }

    #[test]
    fn a_trigger_row_is_named_before_a_clip_sharing_its_combo() {
        let rows = vec![press_row("F9", Some("Scene"))];
        let clips = vec![clip_key("F9", "airhorn")];

        assert_eq!(
            holder_of_combo(&rows, &clips, "F9", Claimant::Clip(None)),
            Some(ComboHolder::Action(Some("Scene".to_owned())))
        );
    }

    #[test]
    fn a_clip_holds_its_combo_against_every_claimant_but_itself() {
        let held = clip_key("F9", "airhorn");
        let clips = vec![held.clone()];
        let as_holder = Some(ComboHolder::Clip {
            id: held.id,
            name: held.name.clone(),
        });

        for (case, claimant, expected) in [
            (
                "the clip recapturing its own key",
                Claimant::Clip(Some(held.id)),
                None,
            ),
            (
                "another clip",
                Claimant::Clip(Some(ClipId::new())),
                as_holder.clone(),
            ),
            (
                "a clip being added",
                Claimant::Clip(None),
                as_holder.clone(),
            ),
            ("a new trigger", Claimant::NewTrigger, as_holder.clone()),
            (
                "a trigger being rebound",
                Claimant::Trigger(TriggerInstanceId::new()),
                as_holder.clone(),
            ),
        ] {
            assert_eq!(
                holder_of_combo(&[], &clips, "F9", claimant),
                expected,
                "wrong holder for {case}"
            );
        }
    }

    #[test]
    fn a_hand_typed_clip_key_conflicts_with_its_captured_canonical_spelling() {
        let typed = stored_clip("airhorn", Some("ctrl+f9"));
        let clips = clip_keys(&[typed.clone(), stored_clip("bell", Some("  "))]);

        assert_eq!(
            holder_of_combo(&[], &clips, "Ctrl+F9", Claimant::NewTrigger),
            Some(ComboHolder::Clip {
                id: typed.id,
                name: typed.name,
            })
        );
    }

    #[test]
    fn a_holder_is_labelled_by_its_action_or_its_soundboard_clip() {
        crate::i18n::install_language(Language::En);

        for (holder, expected) in [
            (ComboHolder::Action(Some("Scene".to_owned())), "Scene"),
            (ComboHolder::Action(None), "an unassigned binding"),
            (
                ComboHolder::Clip {
                    id: ClipId::new(),
                    name: "airhorn".to_owned(),
                },
                "Soundboard: airhorn",
            ),
        ] {
            let label = holder.label().replace(['\u{2068}', '\u{2069}'], "");
            assert_eq!(label, expected, "{holder:?}");
        }
    }

    struct SilentPublisher;

    impl forge_events::EventPublisher for SilentPublisher {
        fn publish(&self, _: forge_events::Event) {}
    }

    struct Rig {
        backend: Sandboxed<Arc<dyn DataProvider>>,
        reconciler: Arc<HotkeyReconciler>,
    }

    impl Rig {
        async fn start() -> Self {
            let backend = sandboxed_backend("sqlite::memory:", TEST_KEY)
                .await
                .map(|backend| Arc::new(backend) as Arc<dyn DataProvider>);
            let (client, _recorder) = forge_hotkey::testing::test_client(
                forge_hotkey::HotkeyConfig::default(),
                Arc::new(SilentPublisher),
            );
            let reconciler = HotkeyReconciler::new(
                client,
                backend.trigger_instance_repo(),
                backend.soundboard_clips_repo(),
            );
            Self {
                backend,
                reconciler,
            }
        }

        async fn seed_clip(&self, name: &str, combo: &str) -> StoredClip {
            let clip = stored_clip(name, Some(combo));
            self.backend
                .soundboard_clips_repo()
                .save(&clip)
                .await
                .unwrap();
            clip
        }

        async fn seed_trigger(&self, combo: &str) {
            let instance = TriggerInstance {
                id: TriggerInstanceId::new(),
                kind_id: HOTKEY_PRESSED_KIND.to_owned(),
                name: combo.to_owned(),
                overrides: BTreeMap::from([(
                    COMBO_FIELD.to_owned(),
                    Variant::String(combo.to_owned()),
                )]),
                enabled: true,
                user_defined: true,
                platform_scope: PlatformScope::default(),
                cooldown_secs: 0,
                cooldown_global: true,
                permission_rung: PermissionRung::Everyone,
            };
            self.backend
                .trigger_instance_repo()
                .save(&instance)
                .await
                .unwrap();
        }

        async fn release(&self, holder: ComboHolder, combo: &str) {
            release_holder(
                holder,
                combo.to_owned(),
                Arc::clone(&self.reconciler),
                Arc::clone(&self.backend),
            )
            .await
            .unwrap();
        }

        async fn clip_hotkey(&self, id: ClipId) -> Option<String> {
            self.backend
                .soundboard_clips_repo()
                .get(id)
                .await
                .unwrap()
                .expect("the clip survives")
                .hotkey
        }

        fn registered(&self) -> Vec<String> {
            let mut combos: Vec<String> = self
                .reconciler
                .client()
                .registered_combos()
                .into_iter()
                .map(|(_, combo)| combo.as_str().to_owned())
                .collect();
            combos.sort();
            combos
        }
    }

    #[tokio::test]
    async fn releasing_a_clip_holder_unbinds_only_that_clip_and_frees_its_key() {
        let rig = Rig::start().await;
        let held = rig.seed_clip("airhorn", "F9").await;
        let other = rig.seed_clip("bell", "F10").await;
        rig.reconciler.reconcile().await;

        rig.release(
            ComboHolder::Clip {
                id: held.id,
                name: held.name.clone(),
            },
            "F9",
        )
        .await;

        assert_eq!(
            (
                rig.clip_hotkey(held.id).await,
                rig.clip_hotkey(other.id).await,
                rig.registered(),
            ),
            (None, Some("F10".to_owned()), vec!["F10".to_owned()])
        );
    }

    #[tokio::test]
    async fn releasing_a_trigger_holder_leaves_a_clip_on_the_same_key_bound_and_live() {
        let rig = Rig::start().await;
        rig.seed_trigger("F9").await;
        let clip = rig.seed_clip("airhorn", "F9").await;
        rig.reconciler.reconcile().await;

        rig.release(ComboHolder::Action(Some("Scene".to_owned())), "F9")
            .await;

        let triggers = rig
            .backend
            .trigger_instance_repo()
            .list_all()
            .await
            .unwrap();
        assert_eq!(
            (
                triggers.len(),
                rig.clip_hotkey(clip.id).await,
                rig.registered()
            ),
            (0, Some("F9".to_owned()), vec!["F9".to_owned()])
        );
    }
}
