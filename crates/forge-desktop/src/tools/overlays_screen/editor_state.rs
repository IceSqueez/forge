use crate::shared::option_choices::ChoiceOption;
use std::collections::HashMap;
use std::sync::Arc;

use forge_overlay::config::SOUND_OPTIONS_KEY;
use forge_overlay::{SectionedField, effective_overlay_config};
use forge_speech_types::ENGINE_VOICE_OPTIONS_KEY;
use gpui::{AppContext, Context, Entity, Subscription};

use super::OverlaysView;
use super::base_sections::BaseEvent;
use super::code_pane::{CodeServices, LeaveIntent, OverlayCodeEditor};
use super::custom_accents::CustomAccents;
use super::look_change;
use super::property_panel::{
    AccentListEdit, OverlayPropertyPanel, PanelLaunch, PropertyPanelEvent,
};
use crate::app::async_bridge;
use crate::audio::engine_voice_choices::engine_voice_options;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum EditorMode {
    Design,
    Code,
}

pub(super) struct OpenPanel {
    pub(super) view: Entity<OverlayPropertyPanel>,
    _sub: Subscription,
    _media_sub: Subscription,
    _icon_sub: Subscription,
    _accent_sub: Subscription,
    _base_sub: Subscription,
}

pub(super) struct EditorState {
    pub(super) mode: EditorMode,
    pub(super) code: Entity<OverlayCodeEditor>,
    pub(super) panel: Option<OpenPanel>,
    pub(super) hide_probe_gen: async_bridge::Generation,
    pub(super) motion_probe_gen: async_bridge::Generation,
}

impl EditorState {
    pub(super) fn new(services: CodeServices, cx: &mut Context<OverlaysView>) -> Self {
        Self {
            mode: EditorMode::Design,
            code: cx.new(|cx| OverlayCodeEditor::new(services, cx)),
            panel: None,
            hide_probe_gen: async_bridge::Generation::default(),
            motion_probe_gen: async_bridge::Generation::default(),
        }
    }
}

impl OverlaysView {
    fn panel_choices(&self) -> HashMap<String, Vec<ChoiceOption>> {
        let voices = self
            .handles
            .speak
            .as_ref()
            .map(|speak| engine_voice_options(&speak.available_voices()))
            .unwrap_or_default();
        HashMap::from([
            (
                SOUND_OPTIONS_KEY.to_owned(),
                self.catalog.clip_choices.clone(),
            ),
            (ENGINE_VOICE_OPTIONS_KEY.to_owned(), voices),
        ])
    }

    pub(super) fn mode(&self) -> EditorMode {
        self.editor.mode
    }

    pub(in crate::tools::overlays_screen) fn panel_view(
        &self,
    ) -> Option<Entity<OverlayPropertyPanel>> {
        self.editor.panel.as_ref().map(|open| open.view.clone())
    }

    pub(super) fn set_mode(&mut self, mode: EditorMode, cx: &mut Context<Self>) {
        if self.editor.mode == mode {
            return;
        }
        if mode == EditorMode::Design && self.code_dirty(cx) {
            self.request_leave(LeaveIntent::Design, cx);
            return;
        }
        self.editor.mode = mode;
        if mode == EditorMode::Code {
            self.editor
                .code
                .update(cx, |code, cx| code.invalidate_source(cx));
        }
        self.sync_source(cx);
        cx.notify();
    }

    fn release_panel(&mut self, cx: &mut Context<Self>) {
        let Some(open) = self.editor.panel.take() else {
            return;
        };
        let (id, unsaved) = open.view.update(cx, |panel, cx| {
            (panel.overlay_id().clone(), panel.take_unsaved_config(cx))
        });
        if let Some(config) = unsaved
            && self.index_of(&id).is_some()
        {
            self.save_config(id, config, cx);
        }
    }

    pub(super) fn sync_panel(&mut self, cx: &mut Context<Self>) {
        let target = self
            .selected_definition()
            .filter(|definition| self.handles.kinds.get(&definition.kind_id).is_some())
            .cloned();

        let Some(definition) = target else {
            self.release_panel(cx);
            return;
        };
        if self.editor.panel.as_ref().is_some_and(|open| {
            let panel = open.view.read(cx);
            panel.overlay_id() == &definition.id && panel.look_kind() == definition.kind_id
        }) {
            self.probe_hide_timing(&definition, cx);
            self.probe_motion_notices(&definition, cx);
            return;
        }
        let Some(descriptor) = self.handles.kinds.get(&definition.kind_id) else {
            self.release_panel(cx);
            return;
        };

        let effective = effective_overlay_config(descriptor, &definition.config);
        let specs: Vec<SectionedField> = look_change::panel_specs(descriptor);

        let launch = PanelLaunch {
            overlay_id: definition.id.clone(),
            specs,
            defaults: descriptor.default_config(),
            stored: definition.config.clone(),
            effective,
            choices: self.panel_choices(),
            icon_images: self.icon_host.read(cx).images().to_vec(),
            custom_accents: self.custom_accents.read(cx).colors().to_vec(),
            overridden_files: definition.source_overrides.clone(),
            service: self.handles.service.clone(),
            samples: Arc::clone(&self.handles.samples),
            rt_handle: self.handles.rt_handle.clone(),
        };

        let issues = self.issues_of(&definition.id).to_vec();
        let base = self.base_launch(descriptor, &definition.id);
        let latest = self.latest.clone();
        self.release_panel(cx);
        let view = cx.new(|cx| OverlayPropertyPanel::new(launch, cx));
        view.update(cx, |panel, cx| {
            panel.set_media_issues(issues, cx);
            panel.adopt_base(base, cx);
            panel.bind_latest(latest, cx);
        });
        let sub = cx.subscribe(&view, Self::on_panel_event);
        let media_sub = cx.subscribe(&view, Self::on_adopt_requested);
        let icon_sub = cx.subscribe(&view, Self::on_icon_pick_requested);
        let base_sub = cx.subscribe(&view, Self::on_base_event);
        let accent_sub = cx.subscribe(&view, Self::on_accent_list_edit);
        self.editor.panel = Some(OpenPanel {
            view,
            _sub: sub,
            _media_sub: media_sub,
            _icon_sub: icon_sub,
            _accent_sub: accent_sub,
            _base_sub: base_sub,
        });
        self.load_clips(cx);
        self.probe_hide_timing(&definition, cx);
        self.probe_motion_notices(&definition, cx);
    }

    fn on_accent_list_edit(
        &mut self,
        _view: Entity<OverlayPropertyPanel>,
        event: &AccentListEdit,
        cx: &mut Context<Self>,
    ) {
        self.custom_accents.update(cx, |store, cx| match event {
            AccentListEdit::Add(color) => store.add(color, cx),
            AccentListEdit::Remove(color) => store.remove(color, cx),
        });
    }

    pub(super) fn on_custom_accents_changed(
        &mut self,
        store: Entity<CustomAccents>,
        cx: &mut Context<Self>,
    ) {
        let Some(panel) = self.editor.panel.as_ref().map(|open| open.view.clone()) else {
            return;
        };
        let colors = store.read(cx).colors().to_vec();
        panel.update(cx, |panel, cx| panel.set_custom_accents(colors, cx));
    }

    fn on_base_event(
        &mut self,
        view: Entity<OverlayPropertyPanel>,
        event: &BaseEvent,
        cx: &mut Context<Self>,
    ) {
        let id = view.read(cx).overlay_id().clone();
        match event {
            BaseEvent::ChangeLook { kind_id, config } => {
                self.change_look(id, kind_id.clone(), config.clone(), cx);
            }
            BaseEvent::SetReceiver(on) => self.set_receiver(id, *on, cx),
        }
    }

    fn on_panel_event(
        &mut self,
        view: Entity<OverlayPropertyPanel>,
        event: &PropertyPanelEvent,
        cx: &mut Context<Self>,
    ) {
        let PropertyPanelEvent::Save(config) = event;
        let id = view.read(cx).overlay_id().clone();
        self.save_config(id, config.clone(), cx);
    }
}
