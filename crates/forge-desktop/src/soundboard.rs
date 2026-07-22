use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use forge_audio::{DeviceInfo, list_output_devices};
use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, ChipGlyph, Confirm, ConfirmTone, Density, FONT_XS, FONT_XXS,
    ForgePalette, Icon, InputEvent, OverlayPosition, Radius, SearchState, Spacing, TextInput,
    body_family, chip, confirm_modal, empty_state, fmt_bytes, fmt_clock, ghost_button_with_icon,
    icon, modal, mono_family, overlay, pad_tile, page_frame, primary_button, radius,
    secondary_button, slider, spacing, status_dot, toggle, tr, with_alpha,
};
use forge_events::{Event, EventSource};
use forge_runtime::EventBus;
use forge_soundboard::builtin_library::{
    BUILTIN_SOUNDS, BuiltinSoundEntry, builtin_availability, resolve_builtin_path,
};
use forge_soundboard::{SoundboardPlayer, SoundboardSettings};
use forge_storage::{
    SettingsRepo, SoundboardClipsRepo, StoredClip, set_soundboard_also_headphones,
    set_soundboard_enabled, set_soundboard_master_volume, set_soundboard_output_device,
};
use forge_types::{ClipId, OutputDevice};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, EventEmitter, Pixels, Rgba, SharedString,
    Subscription, Task, Window, div, prelude::*, px, svg,
};
use time::OffsetDateTime;

use crate::async_bridge::{self, BridgeFlow, drain_events};
use crate::presentation::ActivePresentation;

const SCROLL_PAD_X: Pixels = px(22.0);
const SCROLL_PAD_Y: Pixels = px(18.0);
const SECTION_GAP: Pixels = px(14.0);
const HERO_ICON_TILE: Pixels = px(40.0);
const HERO_ICON_TILE_RADIUS: Pixels = px(10.0);
const HERO_GLYPH: Pixels = px(20.0);
const HERO_TITLE_FS: Pixels = px(15.0);
const HERO_GAP: Pixels = px(14.0);
const LABEL_FS: Pixels = px(11.5);
const SECTION_LABEL_FS: Pixels = px(9.5);
const HEADER_ICON: Pixels = px(13.0);
const SEARCH_WIDTH: Pixels = px(220.0);
const GRID_GAP: Pixels = px(10.0);
const PADS_PER_ROW: usize = 4;
const PAD_GLYPH: Pixels = px(15.0);
const HOTKEY_FS: Pixels = px(10.0);
const HOTKEY_RADIUS: Pixels = px(4.0);
const LOOP_ICON: Pixels = px(10.0);
const PROGRESS_WIDTH: f32 = 0.46;
const PAD_ACTION_TILE: Pixels = px(20.0);
const PAD_ACTION_GLYPH: Pixels = px(13.0);
const PAD_ACTION_GAP: Pixels = px(3.0);
const PAD_ACTION_HOVER_ALPHA: f32 = 0.16;
const TICK_INTERVAL: Duration = Duration::from_millis(100);
const STOP_ICON: Pixels = px(12.0);
const ADD_ICON: Pixels = px(13.0);
const ROUTING_PAD: Pixels = px(14.0);
const ROUTING_GAP: Pixels = px(16.0);
const SELECT_RADIUS: Pixels = px(7.0);
const SELECT_PAD_Y: Pixels = px(8.0);
const SELECT_PAD_X: Pixels = px(11.0);
const HINT_FS: Pixels = px(10.5);
const FOOTER_FS: Pixels = px(10.5);
const FOOTER_DOT: Pixels = px(6.0);
const FOOTER_PAD_Y: Pixels = px(7.0);
const FOOTER_PAD_X: Pixels = px(14.0);
const HOTKEY_SEQUENCE: &[&str] = &[
    "1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "Q", "W", "E", "R", "T", "Y",
];
const CATEGORY_ORDER: &[&str] = &["memes", "alerts", "music", "voice"];

struct SoundClip {
    id: ClipId,
    name: String,
    file_path: PathBuf,
    hotkey: Option<String>,
    category: String,
    loop_playback: bool,
    duration_secs: Option<f32>,
    builtin_id: Option<String>,
    glyph: Icon,
}

struct PlaybackProgress {
    started_at: Instant,
    duration_secs: Option<f64>,
    looped: bool,
}

struct ClipDraft {
    edit_id: Option<ClipId>,
    name: String,
    file_path: PathBuf,
    category: String,
}

enum AddModalEvent {
    Submit(ClipDraft),
    Cancel,
}

struct AddModal {
    file_path: Option<PathBuf>,
    name_input: Entity<TextInput>,
    category: String,
    saving: bool,
    error: Option<SharedString>,
    edit_id: Option<ClipId>,
    rt_handle: tokio::runtime::Handle,
    _name_sub: Subscription,
}

impl EventEmitter<AddModalEvent> for AddModal {}

impl AddModal {
    fn new(
        edit_id: Option<ClipId>,
        name: &str,
        category: String,
        file_path: Option<PathBuf>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let name_input = cx.new(|cx| {
            TextInput::new(tr!("soundboard_modal_name_placeholder"), cx).with_palette(palette)
        });
        if !name.is_empty() {
            let name = name.to_owned();
            name_input.update(cx, |ti, cx| ti.set_content(name, cx));
        }
        let name_sub = cx.subscribe(
            &name_input,
            |this, _input, event: &InputEvent, cx| match event {
                InputEvent::Submitted(_) => this.submit(cx),
                InputEvent::Cancelled => this.cancel(cx),
                InputEvent::Changed(_) => cx.notify(),
            },
        );
        AddModal {
            file_path,
            name_input,
            category,
            saving: false,
            error: None,
            edit_id,
            rt_handle,
            _name_sub: name_sub,
        }
    }

    fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.name_input.update(cx, |f, cx| f.focus(window, cx));
    }

    fn set_category(&mut self, category: String, cx: &mut Context<Self>) {
        self.category = category;
        cx.notify();
    }

    fn browse_file(&mut self, cx: &mut Context<Self>) {
        let filter = async_bridge::DialogFilter {
            name: tr!("soundboard_file_filter_audio"),
            extensions: &["mp3", "wav", "ogg", "flac", "aac", "m4a"],
        };
        async_bridge::spawn_dialog(
            &self.rt_handle,
            async_bridge::pick_file(Some(filter)),
            |this, result, cx| {
                if let Ok(path) = result {
                    this.apply_picked_file(path, cx);
                }
            },
            cx,
        );
    }

    fn apply_picked_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.file_path = Some(path.clone());
        self.error = None;
        let name_input = self.name_input.clone();
        if name_input.read(cx).content().trim().is_empty()
            && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
        {
            let stem = stem.to_owned();
            name_input.update(cx, |ti, cx| ti.set_content(stem, cx));
        }
        cx.notify();
    }

    fn is_saveable(&self, cx: &App) -> bool {
        !self.saving
            && self.file_path.is_some()
            && !self.name_input.read(cx).content().trim().is_empty()
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if !self.is_saveable(cx) {
            self.error = Some(tr!("soundboard_modal_validation_error").into());
            cx.notify();
            return;
        }
        let draft = ClipDraft {
            edit_id: self.edit_id,
            name: self.name_input.read(cx).content().trim().to_owned(),
            file_path: self.file_path.clone().unwrap_or_default(),
            category: self.category.clone(),
        };
        self.error = None;
        cx.emit(AddModalEvent::Submit(draft));
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(AddModalEvent::Cancel);
    }
}

impl Render for AddModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        let file_set = self.file_path.is_some();
        let file_label: String = self
            .file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(str::to_owned)
            .unwrap_or_else(|| tr!("soundboard_modal_no_file").to_string());
        let browse = ghost_button_with_icon(
            Icon::FolderOpen,
            tr!("soundboard_modal_browse_btn"),
            &palette,
        )
        .density(density)
        .on_click(
            "sb-modal-browse",
            cx.listener(|this, _: &ClickEvent, _, cx| this.browse_file(cx)),
        );
        let file_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(if file_set {
                        palette.text_secondary
                    } else {
                        palette.text_muted
                    })
                    .child(file_label),
            )
            .child(browse);

        let mut category_row = div().flex().items_center().gap(px(4.0));
        for (idx, cat) in CATEGORY_ORDER.iter().enumerate() {
            let active = self.category == *cat;
            let color = category_color(cat, &palette);
            let value = (*cat).to_owned();
            category_row = category_row.child(
                chip(category_label(cat), ChipGlyph::Dot(color), active, &palette)
                    .density(density)
                    .on_click(
                        ("sb-modal-cat", idx),
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.set_category(value.clone(), cx)
                        }),
                    ),
            );
        }

        let mut body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(field_lite_label(
                tr!("soundboard_modal_section_name"),
                &palette,
            ))
            .child(div().child(self.name_input.clone()))
            .child(field_lite_label(
                tr!("soundboard_modal_section_category"),
                &palette,
            ))
            .child(category_row)
            .child(field_lite_label(
                tr!("soundboard_modal_section_file"),
                &palette,
            ))
            .child(file_row);

        if let Some(error) = self.error.clone() {
            body = body.child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .p(spacing(Spacing::Xs, density))
                    .rounded(radius(Radius::Sm))
                    .bg(with_alpha(palette.random, 0.10))
                    .border(BORDER_THIN)
                    .border_color(with_alpha(palette.random, 0.30))
                    .child(icon(Icon::InfoCircle, FONT_XS, palette.random))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_primary)
                            .child(error),
                    ),
            );
        }

        let saveable = self.is_saveable(cx);
        let hint = div()
            .flex_1()
            .font_family(body_family())
            .text_size(LABEL_FS)
            .text_color(palette.text_faint)
            .child(if saveable {
                tr!("soundboard_modal_ready")
            } else {
                tr!("soundboard_modal_fill_required")
            });
        let cancel = secondary_button(tr!("soundboard_modal_cancel_btn"), &palette).on_click(
            "sb-modal-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
        );
        let save = primary_button(tr!("soundboard_modal_save_btn"), &palette)
            .disabled(!saveable)
            .on_click(
                "sb-modal-save",
                cx.listener(|this, _: &ClickEvent, _, cx| this.submit(cx)),
            );
        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(spacing(Spacing::Sm, density))
            .child(hint)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(cancel)
                    .child(save),
            );

        let title = if self.edit_id.is_some() {
            tr!("soundboard_modal_title_edit")
        } else {
            tr!("soundboard_modal_title_add")
        };
        let card = modal(title, body, &palette)
            .header_icon(Icon::Music, palette.bits)
            .width(px(440.0))
            .footer(footer)
            .on_close(
                "sb-modal-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            );
        let view = cx.entity();
        overlay(card, &palette)
            .position(OverlayPosition::Center)
            .on_dismiss("sb-modal-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel(cx));
            })
            .into_any_element()
    }
}

pub struct SoundboardView {
    clips: Vec<SoundClip>,
    loading: bool,
    error: Option<SharedString>,
    devices: Vec<DeviceInfo>,
    importable: Vec<BuiltinSoundEntry>,
    total_size: Option<u64>,
    playing: HashMap<ClipId, PlaybackProgress>,
    ticking: bool,
    settings: SoundboardSettings,
    device_menu_open: bool,
    search: SearchState,
    category_filter: Option<String>,
    modal: Option<Entity<AddModal>>,
    _modal_sub: Option<Subscription>,
    pending_delete: Confirm<ClipId>,
    _search_sub: Subscription,
    player: Arc<SoundboardPlayer>,
    clips_repo: Arc<dyn SoundboardClipsRepo>,
    settings_repo: Arc<dyn SettingsRepo>,
    rt_handle: tokio::runtime::Handle,
    master_volume_debounce: async_bridge::Debounced,
    reload_gen: async_bridge::Generation,
    _event_bridge: Task<()>,
}

impl SoundboardView {
    pub fn new(
        player: Arc<SoundboardPlayer>,
        clips_repo: Arc<dyn SoundboardClipsRepo>,
        settings_repo: Arc<dyn SettingsRepo>,
        rt_handle: tokio::runtime::Handle,
        bus: Arc<EventBus>,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let search = SearchState::new(cx, palette, tr!("soundboard_search_placeholder"));
        let search_sub = cx.subscribe(search.field(), Self::on_search_event);
        let settings = (*player.settings_handle().load()).clone();

        let event_bridge = cx.spawn(async move |this, cx| {
            drain_events(&bus, cx, move |batch, cx| {
                match this.update(cx, |this, cx| {
                    for event in batch {
                        this.on_bus_event(event, cx);
                    }
                }) {
                    Ok(()) => BridgeFlow::Continue,
                    Err(_) => BridgeFlow::Stop,
                }
            })
            .await;
        });

        let view = Self {
            clips: Vec::new(),
            loading: true,
            error: None,
            devices: Vec::new(),
            importable: Vec::new(),
            total_size: None,
            playing: HashMap::new(),
            ticking: false,
            settings,
            device_menu_open: false,
            search,
            category_filter: None,
            modal: None,
            _modal_sub: None,
            pending_delete: Confirm::default(),
            _search_sub: search_sub,
            player,
            clips_repo,
            settings_repo,
            rt_handle,
            master_volume_debounce: async_bridge::Debounced::new(
                async_bridge::SLIDER_PERSIST_DEBOUNCE,
            ),
            reload_gen: async_bridge::Generation::default(),
            _event_bridge: event_bridge,
        };
        view.reload(cx);
        view.reload_devices(cx);
        view
    }

    fn on_bus_event(&mut self, event: &Event, cx: &mut Context<Self>) {
        if event.source != EventSource::Audio {
            return;
        }
        let Some(clip_id) = event
            .payload
            .get("clip_id")
            .and_then(|v| v.as_str())
            .and_then(|s| {
                serde_json::from_value::<ClipId>(serde_json::Value::String(s.to_string())).ok()
            })
        else {
            return;
        };
        match event.kind.as_str() {
            "playback.started" => {
                let duration_secs = event.payload.get("duration_secs").and_then(|v| v.as_f64());
                let looped = event
                    .payload
                    .get("looped")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                self.playing.insert(
                    clip_id,
                    PlaybackProgress {
                        started_at: Instant::now(),
                        duration_secs,
                        looped,
                    },
                );
                self.ensure_ticker(cx);
                cx.notify();
            }
            "playback.finished" | "playback.failed" => {
                self.clear_playing(clip_id, cx);
            }
            _ => {}
        }
    }

    fn has_live_progress(&self) -> bool {
        self.playing
            .values()
            .any(|p| !p.looped && p.duration_secs.is_some_and(|d| d > 0.0))
    }

    fn ensure_ticker(&mut self, cx: &mut Context<Self>) {
        if self.ticking || !self.has_live_progress() {
            return;
        }
        self.ticking = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK_INTERVAL).await;
                let keep_going = this.update(cx, |this, cx| {
                    if this.has_live_progress() {
                        cx.notify();
                        true
                    } else {
                        this.ticking = false;
                        false
                    }
                });
                match keep_going {
                    Ok(true) => continue,
                    _ => break,
                }
            }
        })
        .detach();
    }

    fn on_search_event(
        &mut self,
        _input: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if self.search.on_changed(event) {
            cx.notify();
        }
    }

    fn reload(&self, cx: &mut Context<Self>) {
        let ticket = self.reload_gen.next();
        let repo = Arc::clone(&self.clips_repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move { repo.list().await.map_err(|e| e.to_string()) },
            move |this, result, cx| {
                if !this.reload_gen.is_current(ticket) {
                    return;
                }
                match result {
                    Ok(clips) => this.apply_clips(clips, cx),
                    Err(message) => this.on_load_error(message, cx),
                }
            },
            cx,
        );
    }

    fn apply_clips(&mut self, clips: Vec<StoredClip>, cx: &mut Context<Self>) {
        self.clips = clips.into_iter().map(stored_to_clip).collect();
        self.loading = false;
        self.error = None;
        if self
            .category_filter
            .as_ref()
            .is_some_and(|c| !self.clips.iter().any(|clip| &clip.category == c))
        {
            self.category_filter = None;
        }
        self.recompute_size(cx);
        self.refresh_builtins(cx);
        cx.notify();
    }

    fn on_load_error(&mut self, message: String, cx: &mut Context<Self>) {
        self.loading = false;
        self.error = Some(message.into());
        cx.notify();
    }

    fn recompute_size(&self, cx: &mut Context<Self>) {
        let paths: Vec<PathBuf> = self.clips.iter().map(|c| c.file_path.clone()).collect();
        async_bridge::run_blocking(
            &self.rt_handle,
            move || {
                paths
                    .iter()
                    .filter_map(|p| std::fs::metadata(p).ok().map(|m| m.len()))
                    .sum::<u64>()
            },
            |this, total, cx| {
                this.total_size = Some(total);
                cx.notify();
            },
            cx,
        );
    }

    fn refresh_builtins(&self, cx: &mut Context<Self>) {
        let imported_ids: HashSet<String> = self.imported_builtin_ids();
        async_bridge::run_blocking(
            &self.rt_handle,
            move || {
                let data_dir = forge_platform_core::paths::data_dir();
                let available = builtin_availability(&data_dir);
                available
                    .into_iter()
                    .filter(|(entry, present)| *present && !imported_ids.contains(entry.builtin_id))
                    .map(|(entry, _)| entry)
                    .collect::<Vec<BuiltinSoundEntry>>()
            },
            |this, entries, cx| {
                this.importable = entries;
                cx.notify();
            },
            cx,
        );
    }

    fn imported_builtin_ids(&self) -> HashSet<String> {
        self.clips
            .iter()
            .filter_map(|clip| clip.builtin_id.clone())
            .collect()
    }

    fn reload_devices(&self, cx: &mut Context<Self>) {
        async_bridge::run_blocking(
            &self.rt_handle,
            || list_output_devices().map_err(|e| e.to_string()),
            |this, result: Result<Vec<_>, String>, cx| {
                if let Ok(devices) = result {
                    this.devices = devices;
                    cx.notify();
                }
            },
            cx,
        );
    }

    fn toggle_play(&mut self, id: ClipId, cx: &mut Context<Self>) {
        if self.playing.contains_key(&id) {
            self.player.stop(id);
            self.playing.remove(&id);
            cx.notify();
            return;
        }
        let Some(clip) = self.clips.iter().find(|c| c.id == id) else {
            return;
        };
        let known = clip.duration_secs;
        self.error = None;
        self.playing.insert(
            id,
            PlaybackProgress {
                started_at: Instant::now(),
                duration_secs: known.map(f64::from),
                looped: clip.loop_playback,
            },
        );
        self.ensure_ticker(cx);
        cx.notify();

        let player_play = Arc::clone(&self.player);
        let player_dur = Arc::clone(&self.player);
        let rt = self.rt_handle.clone();
        cx.spawn(async move |this, cx| {
            let (tx, rx) = tokio::sync::oneshot::channel();
            rt.spawn(async move {
                let _ = tx.send(player_play.play(id, None).await.map_err(|e| e.to_string()));
            });
            match rx.await {
                Ok(Ok(())) => {}
                Ok(Err(message)) => {
                    let _ = this.update(cx, |this, cx| this.on_play_error(id, message, cx));
                    return;
                }
                Err(_) => return,
            }
            if known.is_none() {
                rt.spawn(async move {
                    let _ = player_dur.ensure_clip_duration(id).await;
                });
            }
        })
        .detach();
    }

    fn on_play_error(&mut self, id: ClipId, message: String, cx: &mut Context<Self>) {
        self.playing.remove(&id);
        self.error = Some(tr!("soundboard_playback_error_prefix", error = message.as_str()).into());
        cx.notify();
    }

    fn clear_playing(&mut self, id: ClipId, cx: &mut Context<Self>) {
        if self.playing.remove(&id).is_some() {
            cx.notify();
        }
    }

    fn stop_all(&mut self, cx: &mut Context<Self>) {
        self.player.stop_all();
        self.playing.clear();
        cx.notify();
    }

    fn set_category_filter(&mut self, filter: Option<String>, cx: &mut Context<Self>) {
        self.category_filter = filter;
        cx.notify();
    }

    fn toggle_enabled(&mut self, cx: &mut Context<Self>) {
        self.settings.enabled = !self.settings.enabled;
        self.player.update_settings(self.settings.clone());
        let repo = Arc::clone(&self.settings_repo);
        let value = self.settings.enabled;
        self.rt_handle.spawn(async move {
            if let Err(e) = set_soundboard_enabled(repo.as_ref(), value).await {
                tracing::warn!(error = %e, "failed to persist soundboard enabled");
            }
        });
        cx.notify();
    }

    fn toggle_headphones(&mut self, cx: &mut Context<Self>) {
        self.settings.also_headphones = !self.settings.also_headphones;
        self.player.update_settings(self.settings.clone());
        let repo = Arc::clone(&self.settings_repo);
        let value = self.settings.also_headphones;
        self.rt_handle.spawn(async move {
            if let Err(e) = set_soundboard_also_headphones(repo.as_ref(), value).await {
                tracing::warn!(error = %e, "failed to persist soundboard headphones");
            }
        });
        cx.notify();
    }

    fn set_master_volume(&mut self, fraction: f32, cx: &mut Context<Self>) {
        let value = (fraction / 100.0).clamp(0.0, 1.0);
        self.settings.master_volume = value;
        self.player.update_settings(self.settings.clone());
        let repo = Arc::clone(&self.settings_repo);
        self.master_volume_debounce.schedule(
            &self.rt_handle,
            "soundboard master volume",
            async move { set_soundboard_master_volume(repo.as_ref(), value).await },
        );
        cx.notify();
    }

    fn set_output_device(&mut self, device_id: Option<String>, cx: &mut Context<Self>) {
        self.settings.output_device_id = device_id.clone();
        self.device_menu_open = false;
        self.player.update_settings(self.settings.clone());
        let repo = Arc::clone(&self.settings_repo);
        self.rt_handle.spawn(async move {
            if let Err(e) = set_soundboard_output_device(repo.as_ref(), device_id).await {
                tracing::warn!(error = %e, "failed to persist soundboard output device");
            }
        });
        cx.notify();
    }

    fn toggle_device_menu(&mut self, cx: &mut Context<Self>) {
        self.device_menu_open = !self.device_menu_open;
        cx.notify();
    }

    fn import_builtin(&mut self, entry: BuiltinSoundEntry, cx: &mut Context<Self>) {
        let hotkey = if self.hotkey_is_free(entry.suggested_hotkey) {
            Some(entry.suggested_hotkey.to_owned())
        } else {
            self.next_free_hotkey()
        };
        let builtin_id = entry.builtin_id.to_owned();
        let name = entry.display_name.to_owned();
        let category = entry.category.to_owned();
        let loop_playback = entry.loop_playback;
        let repo = Arc::clone(&self.clips_repo);
        let player = Arc::clone(&self.player);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let data_dir = forge_platform_core::paths::data_dir();
                let Some(path) = resolve_builtin_path(&data_dir, &builtin_id) else {
                    return Err("builtin audio file missing".to_owned());
                };
                let clip_id = ClipId::new();
                let clip = StoredClip {
                    id: clip_id,
                    name,
                    file_path: path,
                    volume: 1.0,
                    output_device: OutputDevice::Default,
                    hotkey,
                    created_at: OffsetDateTime::now_utc(),
                    category,
                    loop_playback,
                    duration_secs: None,
                    builtin_id: Some(builtin_id),
                };
                let result = repo.save(&clip).await.map_err(|e| e.to_string());
                if result.is_ok() {
                    let _ = player.ensure_clip_duration(clip_id).await;
                }
                result
            },
            |this, result, cx| match result {
                Ok(()) => this.reload(cx),
                Err(message) => this.on_load_error(message, cx),
            },
            cx,
        );
    }

    fn hotkey_is_free(&self, hotkey: &str) -> bool {
        !self
            .clips
            .iter()
            .any(|c| c.hotkey.as_deref() == Some(hotkey))
    }

    fn next_free_hotkey(&self) -> Option<String> {
        HOTKEY_SEQUENCE
            .iter()
            .find(|k| self.hotkey_is_free(k))
            .map(|k| (*k).to_owned())
    }

    fn open_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rt_handle = self.rt_handle.clone();
        let modal =
            cx.new(|cx| AddModal::new(None, "", CATEGORY_ORDER[0].to_owned(), None, rt_handle, cx));
        modal.update(cx, |m, cx| m.focus(window, cx));
        self._modal_sub = Some(cx.subscribe(&modal, Self::on_modal_event));
        self.modal = Some(modal);
        cx.notify();
    }

    fn open_edit(&mut self, id: ClipId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(clip) = self.clips.iter().find(|c| c.id == id) else {
            return;
        };
        let name = clip.name.clone();
        let category = clip.category.clone();
        let file_path = clip.file_path.clone();
        let rt_handle = self.rt_handle.clone();
        let modal =
            cx.new(|cx| AddModal::new(Some(id), &name, category, Some(file_path), rt_handle, cx));
        modal.update(cx, |m, cx| m.focus(window, cx));
        self._modal_sub = Some(cx.subscribe(&modal, Self::on_modal_event));
        self.modal = Some(modal);
        cx.notify();
    }

    fn on_modal_event(
        &mut self,
        _modal: Entity<AddModal>,
        event: &AddModalEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            AddModalEvent::Submit(draft) => self.persist(draft, cx),
            AddModalEvent::Cancel => self.close_modal(cx),
        }
    }

    fn request_delete(&mut self, id: ClipId, cx: &mut Context<Self>) {
        self.pending_delete.request(id);
        cx.notify();
    }

    fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.pending_delete.cancel();
        cx.notify();
    }

    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.pending_delete.take() else {
            return;
        };
        self.player.stop(id);
        self.playing.remove(&id);
        cx.notify();
        let repo = Arc::clone(&self.clips_repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move { repo.delete(id).await.map_err(|e| e.to_string()) },
            |this, result, cx| match result {
                Ok(_) => this.reload(cx),
                Err(message) => this.on_load_error(message, cx),
            },
            cx,
        );
    }

    fn close_modal(&mut self, cx: &mut Context<Self>) {
        self.modal = None;
        self._modal_sub = None;
        cx.notify();
    }

    fn persist(&mut self, draft: &ClipDraft, cx: &mut Context<Self>) {
        let name = draft.name.clone();
        let file_path = draft.file_path.clone();
        let category = draft.category.clone();
        let edit_id = draft.edit_id;

        if let Some(modal) = self.modal.as_ref() {
            modal.update(cx, |m, cx| {
                m.saving = true;
                m.error = None;
                cx.notify();
            });
        }
        let repo = Arc::clone(&self.clips_repo);
        let player = Arc::clone(&self.player);
        let missing_msg = tr!("soundboard_modal_validation_error").to_string();
        let new_hotkey = self.next_free_hotkey();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let (clip, target_id) = match edit_id {
                    Some(id) => {
                        let Some(mut clip) = repo.get(id).await.map_err(|e| e.to_string())? else {
                            return Err(missing_msg);
                        };
                        clip.name = name;
                        if clip.file_path != file_path {
                            clip.file_path = file_path;
                            clip.duration_secs = None;
                        }
                        clip.category = category;
                        (clip, id)
                    }
                    None => {
                        let clip_id = ClipId::new();
                        let clip = StoredClip {
                            id: clip_id,
                            name,
                            file_path,
                            volume: 1.0,
                            output_device: OutputDevice::Default,
                            hotkey: new_hotkey,
                            created_at: OffsetDateTime::now_utc(),
                            category,
                            loop_playback: false,
                            duration_secs: None,
                            builtin_id: None,
                        };
                        (clip, clip_id)
                    }
                };
                let result = repo.save(&clip).await.map_err(|e| e.to_string());
                if result.is_ok() {
                    let _ = player.ensure_clip_duration(target_id).await;
                }
                result
            },
            |this, result, cx| match result {
                Ok(()) => this.on_saved(cx),
                Err(message) => this.on_save_error(message, cx),
            },
            cx,
        );
        cx.notify();
    }

    fn on_saved(&mut self, cx: &mut Context<Self>) {
        self.close_modal(cx);
        self.reload(cx);
    }

    fn on_save_error(&mut self, message: String, cx: &mut Context<Self>) {
        if let Some(modal) = self.modal.as_ref() {
            modal.update(cx, |m, cx| {
                m.saving = false;
                m.error = Some(message.into());
                cx.notify();
            });
        }
        cx.notify();
    }

    fn categories_present(&self) -> Vec<String> {
        let mut seen: Vec<String> = Vec::new();
        for cat in CATEGORY_ORDER {
            if self.clips.iter().any(|c| c.category == *cat) {
                seen.push((*cat).to_owned());
            }
        }
        for clip in &self.clips {
            if !clip.category.is_empty() && !seen.iter().any(|c| c == &clip.category) {
                seen.push(clip.category.clone());
            }
        }
        seen
    }

    fn filtered_indices(&self) -> Vec<usize> {
        self.clips
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                self.category_filter
                    .as_ref()
                    .is_none_or(|f| &c.category == f)
            })
            .filter(|(_, c)| self.search.matches(&c.name))
            .map(|(i, _)| i)
            .collect()
    }

    fn device_short_label(&self) -> String {
        match &self.settings.output_device_id {
            Some(id) => self
                .devices
                .iter()
                .find(|d| &d.id.0 == id)
                .map(|d| d.name.clone())
                .unwrap_or_else(|| tr!("soundboard_device_system_default").to_string()),
            None => tr!("soundboard_device_system_default").to_string(),
        }
    }

    fn output_ready(&self) -> bool {
        match &self.settings.output_device_id {
            Some(id) => self.devices.iter().any(|d| &d.id.0 == id),
            None => true,
        }
    }

    fn render_header_right(&self, palette: &ForgePalette) -> AnyElement {
        let device = self.device_short_label();
        let summary = tr!(
            "soundboard_header_summary",
            device = device.as_str(),
            count = self.clips.len() as i64
        );
        div()
            .flex()
            .items_center()
            .gap(px(5.0))
            .child(icon(Icon::Volume, HEADER_ICON, palette.success))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(LABEL_FS)
                    .text_color(palette.text_muted)
                    .child(summary),
            )
            .into_any_element()
    }

    fn render_hero(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let enabled = self.settings.enabled;
        let label_color = if enabled {
            palette.success
        } else {
            palette.text_faint
        };
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(HERO_GAP)
            .py(ROUTING_PAD)
            .px(spacing(Spacing::Lg, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.elevated)
            .child(
                div()
                    .flex_shrink_0()
                    .size(HERO_ICON_TILE)
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(HERO_ICON_TILE_RADIUS)
                    .bg(palette.surface_overlay)
                    .child(icon(Icon::Music, HERO_GLYPH, palette.bits)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(HERO_TITLE_FS)
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(palette.text_primary)
                            .child(tr!("soundboard_hero_title")),
                    )
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_muted)
                            .child(tr!("soundboard_hero_blurb")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Sm, density))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(LABEL_FS)
                            .text_color(label_color)
                            .child(if enabled {
                                tr!("soundboard_hero_enabled")
                            } else {
                                tr!("soundboard_hero_disabled")
                            }),
                    )
                    .child(toggle(enabled, palette).on_click(
                        "sb-enabled",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_enabled(cx)),
                    )),
            )
            .into_any_element()
    }

    fn render_subheader_left(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut chips = div().flex().items_center().gap(px(4.0)).child(
            chip(
                tr!("soundboard_category_all", count = self.clips.len() as i64),
                ChipGlyph::None,
                self.category_filter.is_none(),
                palette,
            )
            .density(density)
            .on_click(
                "sb-cat-all",
                cx.listener(|this, _: &ClickEvent, _, cx| this.set_category_filter(None, cx)),
            ),
        );
        for (idx, cat) in self.categories_present().into_iter().enumerate() {
            let active = self.category_filter.as_deref() == Some(cat.as_str());
            let color = category_color(&cat, palette);
            let for_click = cat.clone();
            chips = chips.child(
                chip(category_label(&cat), ChipGlyph::Dot(color), active, palette)
                    .density(density)
                    .on_click(
                        ("sb-cat", idx),
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.set_category_filter(Some(for_click.clone()), cx)
                        }),
                    ),
            );
        }

        div()
            .flex()
            .items_center()
            .gap(GRID_GAP)
            .child(div().w(SEARCH_WIDTH).child(self.search.field().clone()))
            .child(chips)
            .into_any_element()
    }

    fn render_subheader_right(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id("sb-stop-all")
            .flex()
            .items_center()
            .gap(px(5.0))
            .cursor_pointer()
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Sm))
            .hover(|s| s.bg(palette.surface_overlay))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.stop_all(cx)))
            .child(icon(Icon::PlayerStop, STOP_ICON, palette.random))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.random)
                    .child(tr!("soundboard_stop_all")),
            )
            .into_any_element()
    }

    fn render_pad(
        &self,
        index: usize,
        clip: &SoundClip,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = clip.id;
        let progress = self.playing.get(&id);
        let playing = progress.is_some();
        let color = category_color(&clip.category, palette);
        let glyph = if playing {
            Icon::PlayerPause
        } else {
            clip.glyph
        };

        let hotkey_badge = clip.hotkey.clone().map(|hk| {
            div()
                .font_family(mono_family())
                .text_size(HOTKEY_FS)
                .text_color(palette.text_secondary)
                .bg(palette.shell)
                .border(BORDER_THIN)
                .border_color(palette.surface_overlay)
                .rounded(HOTKEY_RADIUS)
                .px(px(6.0))
                .py(px(1.0))
                .child(hk)
        });

        let edit_btn = self.pad_action_button(
            ("sb-pad-edit", index),
            format!("sb-pad-edit-{index}").into(),
            Icon::Pencil,
            palette.brand,
            move |this, _ev, window, cx| this.open_edit(id, window, cx),
            cx,
        );
        let delete_btn = self.pad_action_button(
            ("sb-pad-del", index),
            format!("sb-pad-del-{index}").into(),
            Icon::Trash,
            palette.random,
            move |this, _ev, _window, cx| this.request_delete(id, cx),
            cx,
        );
        let top_right = div()
            .flex()
            .items_center()
            .gap(PAD_ACTION_GAP)
            .child(edit_btn)
            .child(delete_btn)
            .children(hotkey_badge);

        let dur_color = if playing { color } else { palette.text_faint };
        let mut sublabel = div().flex().items_center().gap(px(5.0)).mt(px(3.0));
        if clip.loop_playback {
            sublabel = sublabel.child(icon(Icon::Repeat, LOOP_ICON, palette.text_faint));
        }
        sublabel = sublabel.child(
            div()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(dur_color)
                .child(if playing {
                    tr!("soundboard_pad_playing")
                } else {
                    duration_label(clip.duration_secs)
                }),
        );

        let mut pad = pad_tile(
            (gpui::ElementId::from("sb-pad"), id.to_string()),
            icon(glyph, PAD_GLYPH, color),
            clip.name.clone(),
            palette,
        )
        .top_right(top_right)
        .sublabel(sublabel)
        .selected(playing)
        .accent(color)
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_play(id, cx)));

        if let Some(prog) = progress {
            let indeterminate = prog.looped || !prog.duration_secs.is_some_and(|d| d > 0.0);
            let fraction = if indeterminate {
                PROGRESS_WIDTH
            } else {
                let elapsed = Instant::now()
                    .saturating_duration_since(prog.started_at)
                    .as_secs_f64();
                (elapsed / prog.duration_secs.unwrap_or(0.0)).clamp(0.0, 1.0) as f32
            };
            pad = pad.progress(fraction, color);
        }
        pad.into_any_element()
    }

    fn pad_action_button(
        &self,
        id: impl Into<gpui::ElementId>,
        group: SharedString,
        glyph: Icon,
        tint: Rgba,
        handler: impl Fn(&mut Self, &ClickEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let idle = cx.palette().text_faint;
        div()
            .id(id.into())
            .group(group.clone())
            .flex()
            .items_center()
            .justify_center()
            .size(PAD_ACTION_TILE)
            .rounded(radius(Radius::Sm))
            .cursor_pointer()
            .hover(move |s| s.bg(with_alpha(tint, PAD_ACTION_HOVER_ALPHA)))
            .on_click(cx.listener(move |this, ev: &ClickEvent, window, cx| {
                cx.stop_propagation();
                handler(this, ev, window, cx);
            }))
            .child(
                svg()
                    .flex_none()
                    .size(PAD_ACTION_GLYPH)
                    .path(glyph.path())
                    .text_color(idle)
                    .group_hover(group, move |s| s.text_color(tint)),
            )
            .into_any_element()
    }

    fn render_grid(&self, elements: Vec<AnyElement>) -> AnyElement {
        let mut grid = div().w_full().flex().flex_col().gap(GRID_GAP);
        let mut iter = elements.into_iter().peekable();
        while iter.peek().is_some() {
            let mut row = div().w_full().flex().flex_row().gap(GRID_GAP);
            for _ in 0..PADS_PER_ROW {
                match iter.next() {
                    Some(el) => row = row.child(div().flex_1().min_w_0().child(el)),
                    None => row = row.child(div().flex_1()),
                }
            }
            grid = grid.child(row);
        }
        grid.into_any_element()
    }

    fn render_pads(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let indices = self.filtered_indices();
        if indices.is_empty() {
            let message = if self.clips.is_empty() {
                tr!("soundboard_empty_title")
            } else {
                tr!("soundboard_no_matches")
            };
            return empty_state(message, palette)
                .glyph(Icon::Music)
                .density(density)
                .into_any_element();
        }
        let pads: Vec<AnyElement> = indices
            .into_iter()
            .map(|i| self.render_pad(i, &self.clips[i], palette, cx))
            .collect();
        self.render_grid(pads)
    }

    fn render_library(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.importable.is_empty() {
            return None;
        }
        let cards: Vec<AnyElement> = self
            .importable
            .iter()
            .map(|entry| self.render_library_pad(*entry, palette, cx))
            .collect();
        Some(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density))
                .child(section_label(tr!("soundboard_library_section"), palette))
                .child(self.render_grid(cards))
                .into_any_element(),
        )
    }

    fn render_library_pad(
        &self,
        entry: BuiltinSoundEntry,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let color = category_color(entry.category, palette);
        let glyph =
            glyph_for_name(entry.icon_name).unwrap_or_else(|| category_glyph(entry.category));
        pad_tile(
            (
                gpui::ElementId::from("sb-lib"),
                SharedString::new_static(entry.builtin_id),
            ),
            icon(glyph, PAD_GLYPH, color),
            entry.display_name.to_owned(),
            palette,
        )
        .top_right(icon(Icon::Plus, HOTKEY_FS, palette.text_faint))
        .sublabel(
            div()
                .mt(px(3.0))
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(tr!("soundboard_library_import")),
        )
        .hover_border(color)
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.import_builtin(entry, cx)))
        .into_any_element()
    }

    fn render_add_bar(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        pad_tile(
            "sb-add-bar",
            icon(Icon::Plus, ADD_ICON, palette.bits),
            tr!("soundboard_add_sound"),
            palette,
        )
        .bar(palette)
        .title_color(palette.bits)
        .hover_border(palette.bits)
        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open_add(window, cx)))
        .into_any_element()
    }

    fn render_routing(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let device_label = self.device_short_label();
        let mut device_col = div()
            .flex_1()
            .flex()
            .flex_col()
            .child(field_lite_label(tr!("soundboard_routing_device"), palette))
            .child(self.render_device_select(&device_label, palette, cx));
        device_col = device_col.child(
            div()
                .mt(px(5.0))
                .font_family(body_family())
                .text_size(HINT_FS)
                .text_color(palette.text_faint)
                .child(tr!("soundboard_routing_hint")),
        );

        let pct = (self.settings.master_volume * 100.0).round() as i64;
        let volume_col = div()
            .flex_1()
            .flex()
            .flex_col()
            .child(field_lite_label(
                tr!("soundboard_routing_volume", pct = pct),
                palette,
            ))
            .child(
                slider(self.settings.master_volume * 100.0, 0.0, 100.0, palette)
                    .accent(palette.bits)
                    .on_change(
                        "sb-master-volume",
                        cx.listener(|this, value: &f32, _, cx| this.set_master_volume(*value, cx)),
                    ),
            )
            .child(
                div()
                    .mt(spacing(Spacing::Xs, density))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        toggle(self.settings.also_headphones, palette)
                            .on_color(palette.bits)
                            .on_click(
                                "sb-headphones",
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.toggle_headphones(cx)
                                }),
                            ),
                    )
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(LABEL_FS)
                            .text_color(palette.text_secondary)
                            .child(tr!("soundboard_routing_headphones")),
                    ),
            );

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .p(ROUTING_PAD)
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.elevated)
            .child(section_label(tr!("soundboard_routing_section"), palette))
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap(ROUTING_GAP)
                    .child(device_col)
                    .child(volume_col),
            )
            .into_any_element()
    }

    fn render_device_select(
        &self,
        current_label: &str,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let trigger = div()
            .id("sb-device-trigger")
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .py(SELECT_PAD_Y)
            .px(SELECT_PAD_X)
            .rounded(SELECT_RADIUS)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.shell)
            .cursor_pointer()
            .hover(|s| s.border_color(palette.border_active))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_device_menu(cx)))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(current_label.to_owned()),
            )
            .child(icon(Icon::ChevronDown, HOTKEY_FS, palette.text_faint));

        let mut col = div().w_full().flex().flex_col().gap(px(4.0)).child(trigger);
        if self.device_menu_open {
            let mut list = div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .p(px(4.0))
                .rounded(SELECT_RADIUS)
                .border(BORDER_THIN)
                .border_color(palette.border_regular)
                .bg(palette.elevated)
                .child(self.device_option(
                    "sb-dev-default",
                    tr!("soundboard_device_system_default"),
                    self.settings.output_device_id.is_none(),
                    None,
                    palette,
                    cx,
                ));
            for (idx, device) in self.devices.iter().enumerate() {
                let selected =
                    self.settings.output_device_id.as_deref() == Some(device.id.0.as_str());
                let id = device.id.0.clone();
                list = list.child(self.device_option(
                    ("sb-dev", idx),
                    device.name.clone(),
                    selected,
                    Some(id),
                    palette,
                    cx,
                ));
            }
            col = col.child(list);
        }
        col.into_any_element()
    }

    fn device_option(
        &self,
        id: impl Into<gpui::ElementId>,
        label: impl Into<SharedString>,
        selected: bool,
        value: Option<String>,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ink = if selected {
            palette.text_primary
        } else {
            palette.text_secondary
        };
        div()
            .id(id.into())
            .w_full()
            .flex()
            .items_center()
            .py(px(6.0))
            .px(px(8.0))
            .rounded(radius(Radius::Sm))
            .cursor_pointer()
            .when(selected, |s| s.bg(palette.surface_overlay))
            .hover(|s| s.bg(palette.surface_overlay))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.set_output_device(value.clone(), cx)
            }))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(ink)
                    .child(label.into()),
            )
            .into_any_element()
    }

    fn render_footer(&self, palette: &ForgePalette) -> AnyElement {
        let category_count = self.categories_present().len();
        let size_label = self
            .total_size
            .map(fmt_bytes)
            .unwrap_or_else(|| "\u{2014}".to_owned());
        let ready = self.output_ready();
        let (dot, status_text) = if ready {
            (palette.success, tr!("soundboard_output_ready"))
        } else {
            (palette.warning, tr!("soundboard_output_missing"))
        };
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .py(FOOTER_PAD_Y)
            .px(FOOTER_PAD_X)
            .border_t(BORDER_THIN)
            .border_color(palette.surface_overlay)
            .bg(palette.shell)
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FOOTER_FS)
                    .text_color(palette.text_faint)
                    .child(tr!(
                        "soundboard_footer_left",
                        sounds = self.clips.len() as i64,
                        categories = category_count as i64,
                        size = size_label.as_str()
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(status_dot(dot, FOOTER_DOT))
                    .child(
                        div()
                            .font_family(mono_family())
                            .text_size(FOOTER_FS)
                            .text_color(palette.text_faint)
                            .child(status_text),
                    ),
            )
            .into_any_element()
    }

    fn render_delete_confirm(
        &self,
        id: ClipId,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = self
            .clips
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        let card = confirm_modal(
            tr!("soundboard_delete_title"),
            tr!("soundboard_delete_body"),
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(name)
        .on_cancel(
            "sb-del-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_delete(cx)),
        )
        .on_confirm(
            "sb-del-confirm",
            tr!("common_delete"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_delete(cx)),
        );
        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("sb-del-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_delete(cx));
            })
            .into_any_element()
    }
}

impl Render for SoundboardView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let header_right = self.render_header_right(&palette);
        let subheader_left = self.render_subheader_left(&palette, density, cx);
        let subheader_right = self.render_subheader_right(&palette, density, cx);

        let inner = if self.loading {
            empty_state(tr!("soundboard_loading"), &palette)
                .loading("soundboard-loading")
                .density(density)
                .into_any_element()
        } else {
            let error_banner = self.error.clone().map(|message| {
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .p(spacing(Spacing::Xs, density))
                    .rounded(radius(Radius::Sm))
                    .bg(with_alpha(palette.random, 0.10))
                    .border(BORDER_THIN)
                    .border_color(with_alpha(palette.random, 0.30))
                    .child(icon(Icon::AlertCircle, FONT_XS, palette.random))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_primary)
                            .child(message),
                    )
            });

            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(SECTION_GAP)
                .children(error_banner)
                .child(self.render_hero(&palette, density, cx))
                .child(self.render_pads(&palette, density, cx))
                .children(self.render_library(&palette, density, cx))
                .child(self.render_add_bar(&palette, cx))
                .child(self.render_routing(&palette, density, cx))
                .child(self.render_footer(&palette))
                .into_any_element()
        };

        let body = div()
            .id("sb-scroll")
            .flex_1()
            .h_full()
            .overflow_y_scroll()
            .bg(palette.base)
            .child(
                div()
                    .w_full()
                    .py(SCROLL_PAD_Y)
                    .px(SCROLL_PAD_X)
                    .child(inner),
            );

        let frame = page_frame(
            vec![
                BreadcrumbCrumb::leaf(tr!("soundboard_breadcrumb_builtin")),
                BreadcrumbCrumb::leaf(tr!("soundboard_breadcrumb_soundboard")),
            ],
            &palette,
        )
        .header_right(header_right)
        .subheader_left(subheader_left)
        .subheader_right(subheader_right)
        .density(density)
        .body(body);

        let active_overlay = if let Some(modal) = self.modal.as_ref() {
            Some(modal.clone().into_any_element())
        } else {
            self.pending_delete
                .get()
                .copied()
                .map(|id| self.render_delete_confirm(id, &palette, cx))
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(frame)
            .children(active_overlay)
    }
}

fn section_label(label: impl Into<SharedString>, palette: &ForgePalette) -> impl IntoElement {
    div()
        .font_family(mono_family())
        .text_size(SECTION_LABEL_FS)
        .text_color(palette.text_muted)
        .child(label.into())
}

fn field_lite_label(label: impl Into<SharedString>, palette: &ForgePalette) -> impl IntoElement {
    div()
        .mb(px(5.0))
        .font_family(mono_family())
        .text_size(HOTKEY_FS)
        .text_color(palette.text_muted)
        .child(label.into())
}

fn category_color(cat: &str, palette: &ForgePalette) -> Rgba {
    match cat {
        "memes" => palette.bits,
        "alerts" => palette.random,
        "music" => palette.brand,
        "voice" => palette.info,
        _ => palette.text_muted,
    }
}

fn category_label(cat: &str) -> SharedString {
    match cat {
        "memes" => tr!("soundboard_category_memes").into(),
        "alerts" => tr!("soundboard_category_alerts").into(),
        "music" => tr!("soundboard_category_music").into(),
        "voice" => tr!("soundboard_category_voice").into(),
        other => other.to_owned().into(),
    }
}

fn category_glyph(cat: &str) -> Icon {
    match cat {
        "memes" => Icon::MoodSmile,
        "alerts" => Icon::Star,
        "music" => Icon::Music,
        "voice" => Icon::MessageCircle,
        _ => Icon::Music,
    }
}

fn glyph_for_name(name: &str) -> Option<Icon> {
    Some(match name {
        "music" => Icon::Music,
        "repeat" => Icon::Repeat,
        "star" => Icon::Star,
        "flag" => Icon::Flag,
        "speakerphone" => Icon::Speakerphone,
        "sparkles" => Icon::Sparkles,
        "wave-sine" => Icon::WaveSine,
        "wave-saw-tool" => Icon::WaveSawTool,
        "ripple" => Icon::Ripple,
        "hand-click" => Icon::HandClick,
        "user-plus" => Icon::UserPlus,
        "mood-crazy-happy" => Icon::MoodCrazyHappy,
        "mood-sad" => Icon::MoodSmile,
        "alert-triangle" => Icon::AlertTriangle,
        "player-skip-forward" => Icon::PlayerSkipForward,
        "bolt" => Icon::Bolt,
        "volume" => Icon::Volume,
        "eye" => Icon::Eye,
        "x" => Icon::X,
        "message-circle" => Icon::MessageCircle,
        _ => return None,
    })
}

fn stored_to_clip(c: StoredClip) -> SoundClip {
    let glyph = c
        .builtin_id
        .as_deref()
        .and_then(|id| BUILTIN_SOUNDS.iter().find(|e| e.builtin_id == id))
        .and_then(|e| glyph_for_name(e.icon_name).or_else(|| Some(category_glyph(e.category))))
        .unwrap_or(Icon::Music);
    SoundClip {
        id: c.id,
        name: c.name,
        file_path: c.file_path,
        hotkey: c.hotkey,
        category: c.category,
        loop_playback: c.loop_playback,
        duration_secs: c.duration_secs,
        builtin_id: c.builtin_id,
        glyph,
    }
}

fn duration_label(secs: Option<f32>) -> String {
    match secs {
        Some(s) => fmt_clock(s.max(0.0).round() as u64),
        None => "\u{2014}".to_owned(),
    }
}
