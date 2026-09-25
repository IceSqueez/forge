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
