use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use forge_components::{ActivePresentation, Confirm, Density, ForgePalette, Icon, SearchState, tr};
use forge_events::Event;
use forge_soundboard::ClipAvailability;
use forge_types::ClipId;
use gpui::{
    AnyElement, App, ClickEvent, Context, EventEmitter, SharedString, Subscription, Window,
    prelude::*,
};

use super::SoundboardHandles;
use super::adoption::SettledAdoption;
use crate::app::async_bridge::{self, Loadable};
use crate::app::runtime_event::{self, ClipPlayback, RuntimeEvent};
use crate::audio::clip_editor::ClipEditorLaunch;
use crate::audio::clip_key_state::{ClipKeyState, canonical_combo};
use crate::shared::load_notice::{LoadNotice, NoticeWidth, load_notice};

pub(super) struct SoundClip {
    pub(super) id: ClipId,
    pub(super) name: String,
    pub(super) file_path: PathBuf,
    pub(super) hotkey: Option<String>,
    pub(super) category: String,
    pub(super) loop_playback: bool,
    pub(super) duration_secs: Option<f32>,
    pub(super) builtin_id: Option<String>,
    pub(super) glyph: Icon,
}

pub(super) struct PlaybackProgress {
    pub(super) play_id: Option<u64>,
    pub(super) started_at: Instant,
    pub(super) duration_secs: Option<f64>,
    pub(super) looped: bool,
}

pub(super) enum PadsEvent {
    ClipsLoaded,
    LibraryGrew,
    HoldersChanged,
}

pub(super) type EditRequest = Rc<dyn Fn(ClipId, &mut Window, &mut App)>;

pub(super) struct SoundboardPads {
    pub(super) clips: Vec<SoundClip>,
    pub(super) list_load: Loadable<()>,
    pub(super) error: Option<SharedString>,
    pub(super) availability: HashMap<ClipId, ClipAvailability>,
    pub(super) refusal_reads: HashSet<ClipId>,
    pub(super) playing: HashMap<ClipId, PlaybackProgress>,
    pub(super) ticking: bool,
    pub(super) search: SearchState,
    pub(super) category_filter: Option<String>,
    pub(super) pending_delete: Confirm<ClipId>,
    pub(super) keys: ClipKeyState,
    pub(super) handles: SoundboardHandles,
    pub(super) on_edit: EditRequest,
    pub(super) reload_gen: async_bridge::Generation,
    pub(super) availability_gen: async_bridge::Generation,
    _search_sub: Subscription,
}

impl EventEmitter<PadsEvent> for SoundboardPads {}

impl SoundboardPads {
    pub(super) fn new(
        handles: SoundboardHandles,
        keys: ClipKeyState,
        on_edit: EditRequest,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = SearchState::new(cx, tr!("soundboard_search_placeholder"));
        let search_sub = cx.subscribe(search.field(), Self::on_search_event);
        let view = Self {
            clips: Vec::new(),
            list_load: Loadable::Loading,
            error: None,
            availability: HashMap::new(),
            refusal_reads: HashSet::new(),
            playing: HashMap::new(),
            ticking: false,
            search,
            category_filter: None,
            pending_delete: Confirm::default(),
            keys,
            handles,
            on_edit,
            reload_gen: async_bridge::Generation::default(),
            availability_gen: async_bridge::Generation::default(),
            _search_sub: search_sub,
        };
        view.reload(cx);
        view.reload_key_holders(cx);
        view
    }

    pub(super) fn on_bus_event(&mut self, event: &Event, cx: &mut Context<Self>) {
        match runtime_event::decode(event) {
            Some(RuntimeEvent::HotkeyRegistrationsChanged) => {
                self.keys.refresh_live();
                cx.notify();
            }
            Some(RuntimeEvent::ClipPlayback(ClipPlayback::Started(started))) => {
                let Some(clip_id) = started.clip_id else {
                    return;
                };
                self.playing.insert(
                    clip_id,
                    PlaybackProgress {
                        play_id: Some(started.play_id),
                        started_at: Instant::now(),
                        duration_secs: started.duration_secs,
                        looped: started.looped,
                    },
                );
                self.ensure_ticker(cx);
                cx.notify();
            }
            Some(RuntimeEvent::ClipPlayback(
                ended @ (ClipPlayback::Finished(_) | ClipPlayback::Failed(_)),
            )) => {
                if let Some(clip_id) = ended.clip_id() {
                    self.clear_ended_play(clip_id, Some(ended.play_id()), cx);
                }
            }
            Some(RuntimeEvent::ClipAdoptionSettled(settled)) => {
                self.apply_settled_adoption(
                    settled.clip_id,
                    SettledAdoption::from(&settled.verdict),
                    cx,
                );
            }
            _ => {}
        }
    }

    pub(super) fn clip_count(&self) -> usize {
        self.clips.len()
    }

    pub(super) fn error(&self) -> Option<SharedString> {
        self.error.clone()
    }

    pub(super) fn keys(&self) -> &ClipKeyState {
        &self.keys
    }

    pub(super) fn imported_builtin_ids(&self) -> HashSet<String> {
        self.clips
            .iter()
            .filter_map(|clip| clip.builtin_id.clone())
            .collect()
    }

    pub(super) fn edit_launch(&self, id: ClipId) -> Option<ClipEditorLaunch> {
        let clip = self.clips.iter().find(|c| c.id == id)?;
        Some(ClipEditorLaunch {
            edit_id: Some(id),
            name: clip.name.clone(),
            category: clip.category.clone(),
            file_path: Some(clip.file_path.clone()),
            loop_playback: clip.loop_playback,
            hotkey: clip.hotkey.as_deref().map(canonical_combo),
        })
    }

    pub(super) fn render_load_notice(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        load_notice(
            &self.list_load,
            LoadNotice {
                id: "soundboard-load",
                loading: tr!("soundboard_loading"),
                width: NoticeWidth::Pane,
            },
            cx.listener(|this, _: &ClickEvent, _, cx| this.retry_list_load(cx)),
            palette,
            density,
            cx,
        )
        .filter(|_| self.clips.is_empty())
    }

    pub(super) fn render_delete_overlay(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.pending_delete
            .get()
            .copied()
            .map(|id| self.render_delete_confirm(id, palette, cx))
    }
}

impl Render for SoundboardPads {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        self.render_pads(&palette, density, cx)
    }
}
