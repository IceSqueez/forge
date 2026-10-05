use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use forge_components::{
    BORDER_THIN, FONT_XXS, ForgePalette, Picker, PickerEvent, PickerItem, PickerLabels,
    accent_swatch, body_family, field_label, section_label, tr,
};
use forge_overlay::config::{ACCENT, RETIRED_KEYS, SOUND_OPTIONS_KEY};
use forge_overlay::{ConfigSection, MediaIssue, MediaSlot, SectionedField, media_slot};
use forge_registry::FormField;
use forge_runtime::OverlayServiceHandle;
use forge_storage::{OverlayConfig, OverlayId, OverlayRepo};
use forge_types::ClipId;
use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, Pixels, Rgba, SharedString, Subscription,
    Window, div, prelude::*, px,
};

use super::base_sections::{
    BaseEvent, BaseState, FIELD_SECTIONS, PanelSection, base_hint, base_label, hinted,
};
use super::icon_choice::{IconImage, icon_field_row};
use super::latest_section::LatestState;
use super::motion_notices::MotionNotices;
use super::motion_timing::{motion_hint, motion_label};
use super::sound_choice::{PickOutcome, field_notes, notes_block, picked_clip, sound_choices};
use super::store_config;
use crate::async_bridge;
use crate::config_form::{
    ChoiceDropdown, ChoiceSupport, ConfigField, ConfigFieldHandlers, FoldContext,
    collect_field_values, fold_config_field, render_config_control, resolve_dependent_choices,
    sparse_overrides,
};
use crate::presentation::ActivePresentation;

const PANE_W: Pixels = px(246.0);
const PANE_PAD: Pixels = px(14.0);

pub(super) const SECTION_GAP: Pixels = px(10.0);
pub(super) const SECTION_TOP_GAP: Pixels = px(16.0);
pub(super) const FIELD_GAP: Pixels = px(12.0);

pub(super) const NOTICE_PAD: Pixels = px(9.0);
pub(super) const NOTICE_RADIUS: Pixels = px(6.0);
pub(super) const NOTICE_LINE_H: Pixels = px(15.0);

const SLIDE_SETTLE: Duration = Duration::from_millis(400);

const RELEASE_PERSIST_CONTEXT: &str = "overlay property panel teardown";

pub(super) enum PropertyPanelEvent {
    Save(OverlayConfig),
}

pub(super) struct AdoptClipRequested {
    pub(super) key: String,
    pub(super) value: String,
    pub(super) clip: ClipId,
}

pub(super) struct IconPickRequested {
    pub(super) key: String,
}

pub(super) enum IconPickResult {
    Chosen(String),
    Refused(String),
}

struct PendingPick {
    key: String,
    value: String,
}

pub(super) struct PanelLaunch {
    pub(super) overlay_id: OverlayId,
    pub(super) specs: Vec<SectionedField>,
    pub(super) defaults: OverlayConfig,
    pub(super) stored: OverlayConfig,
    pub(super) effective: OverlayConfig,
    pub(super) choices: HashMap<String, Vec<(String, String)>>,
    pub(super) icon_images: Vec<IconImage>,
    pub(super) overridden_files: Vec<String>,
    pub(super) repo: Arc<dyn OverlayRepo>,
    pub(super) service: OverlayServiceHandle,
    pub(super) rt_handle: tokio::runtime::Handle,
}

struct ChoicePicker {
    key: String,
    picker: Entity<Picker>,
    _sub: Subscription,
}

pub(super) struct OverlayPropertyPanel {
    pub(super) overlay_id: OverlayId,
    pub(super) defaults: OverlayConfig,
    pub(super) stored: OverlayConfig,
    labels: HashMap<String, String>,
    sections: HashMap<String, PanelSection>,
    pub(super) fields: Vec<ConfigField>,
    choices: HashMap<String, Vec<(String, String)>>,
    icon_images: Vec<IconImage>,
    overridden_files: Vec<String>,
    picker: Option<ChoicePicker>,
    media_issues: Vec<MediaIssue>,
    pub(super) motion_notices: MotionNotices,
    pending_pick: Option<PendingPick>,
    pick_refusal: Option<(String, String)>,
    settle_epoch: u64,
    repo: Arc<dyn OverlayRepo>,
    pub(super) service: OverlayServiceHandle,
    pub(super) rt_handle: tokio::runtime::Handle,
    pub(super) base: BaseState,
    pub(super) latest: LatestState,
    _release: Subscription,
}

impl EventEmitter<PropertyPanelEvent> for OverlayPropertyPanel {}

impl EventEmitter<AdoptClipRequested> for OverlayPropertyPanel {}

impl EventEmitter<IconPickRequested> for OverlayPropertyPanel {}

impl EventEmitter<BaseEvent> for OverlayPropertyPanel {}

impl OverlayPropertyPanel {
    pub(super) fn new(launch: PanelLaunch, cx: &mut Context<Self>) -> Self {
        let palette = cx.palette();
        let fold = FoldContext {
            config: &launch.effective,
            defaults: &launch.defaults,
            palette: &palette,
            choices: ChoiceSupport::Picker(&launch.choices),
            on_committed: Self::on_field_committed,
        };
        let mut fields: Vec<ConfigField> = Vec::new();
        for spec in &launch.specs {
            fold_config_field(&spec.field, None, &fold, &mut fields, cx);
        }
        let index = index_fields(&launch.specs);
        let release = cx.on_release(|this, cx| this.persist_on_release(cx));

        let mut panel = Self {
            overlay_id: launch.overlay_id,
            defaults: launch.defaults,
            stored: launch.stored,
            labels: index.labels,
            sections: index.sections,
            fields,
            choices: launch.choices,
            icon_images: launch.icon_images,
            overridden_files: launch.overridden_files,
            picker: None,
            media_issues: Vec::new(),
            motion_notices: MotionNotices::default(),
            pending_pick: None,
            pick_refusal: None,
            settle_epoch: 0,
            repo: launch.repo,
            service: launch.service,
            rt_handle: launch.rt_handle,
            base: BaseState::unset(),
            latest: LatestState::default(),
            _release: release,
        };
        panel.refresh_media_choices();
        panel
    }

    pub(super) fn set_sound_choices(
        &mut self,
        clips: Vec<(String, String)>,
        cx: &mut Context<Self>,
    ) {
        self.choices.insert(SOUND_OPTIONS_KEY.to_owned(), clips);
        self.refresh_media_choices();
        cx.notify();
    }

    pub(super) fn set_media_issues(&mut self, issues: Vec<MediaIssue>, cx: &mut Context<Self>) {
        self.media_issues = issues;
        cx.notify();
    }

    pub(super) fn set_icon_images(&mut self, images: Vec<IconImage>, cx: &mut Context<Self>) {
        self.icon_images = images;
        cx.notify();
    }

    pub(super) fn begin_icon_import(&mut self, key: String, cx: &mut Context<Self>) {
        self.pick_refusal = None;
        self.pending_pick = Some(PendingPick {
            key,
            value: String::new(),
        });
        cx.notify();
    }

    pub(super) fn cancel_icon_import(&mut self, key: String, cx: &mut Context<Self>) {
        if !self.awaits_icon(&key) {
            return;
        }
        self.pending_pick = None;
        cx.notify();
    }

    pub(super) fn settle_icon_import(
        &mut self,
        key: String,
        result: IconPickResult,
        cx: &mut Context<Self>,
    ) {
        if !self.awaits_icon(&key) {
            return;
        }
        self.settle_icon_pick(key, result, cx);
    }

    pub(super) fn settle_icon_pick(
        &mut self,
        key: String,
        result: IconPickResult,
        cx: &mut Context<Self>,
    ) {
        self.pending_pick = None;
        self.pick_refusal = None;
        match result {
            IconPickResult::Chosen(value) => self.commit_choice(key, value, cx),
            IconPickResult::Refused(message) => {
                self.pick_refusal = Some((key, message));
                cx.notify();
            }
        }
    }

    fn accent_tint(&self, palette: &ForgePalette) -> Rgba {
        self.fields
            .iter()
            .find_map(|field| match field {
                ConfigField::Swatch { key, selected, .. } if key == ACCENT => {
                    accent_swatch(selected, palette)
                }
                _ => None,
            })
            .unwrap_or(palette.brand)
    }

    fn awaits_icon(&self, key: &str) -> bool {
        self.pending_pick
            .as_ref()
            .is_some_and(|pending| pending.key == key)
    }

    fn refresh_media_choices(&mut self) {
        let clips = self
            .choices
            .get(SOUND_OPTIONS_KEY)
            .cloned()
            .unwrap_or_default();
        for field in &mut self.fields {
            if let ConfigField::Choice {
                key,
                options,
                selected,
                ..
            } = field
                && media_slot(key) == Some(MediaSlot::Sound)
            {
                *options = sound_choices(&clips, selected);
            }
        }
    }

    pub(super) fn overlay_id(&self) -> &OverlayId {
        &self.overlay_id
    }

    pub(super) fn pending_config(&self, cx: &App) -> OverlayConfig {
        let mut buffer = self.defaults.clone();
        for (key, value) in &self.stored {
            if RETIRED_KEYS.contains(&key.as_str()) {
                continue;
            }
            buffer.insert(key.clone(), value.clone());
        }
        collect_field_values(&self.fields, &mut buffer, cx);
        sparse_overrides(&self.defaults, &buffer)
    }

    pub(super) fn emit_save(&mut self, cx: &mut Context<Self>) {
        let sparse = self.pending_config(cx);
        if sparse == self.stored {
            return;
        }
        self.stored = sparse.clone();
        cx.emit(PropertyPanelEvent::Save(sparse));
        self.refresh_latest(cx);
    }

    pub(super) fn take_unsaved_config(&mut self, cx: &App) -> Option<OverlayConfig> {
        let sparse = self.pending_config(cx);
        if sparse == self.stored {
            return None;
        }
        self.stored = sparse.clone();
        Some(sparse)
    }

    fn persist_on_release(&mut self, cx: &mut App) {
        let Some(sparse) = self.take_unsaved_config(cx) else {
            return;
        };
        let repo = Arc::clone(&self.repo);
        let service = self.service.clone();
        let id = self.overlay_id.clone();
        async_bridge::detached(&self.rt_handle, RELEASE_PERSIST_CONTEXT, async move {
            if store_config(repo.as_ref(), &id, sparse).await? {
                service.materialize(&id).await.map_err(|e| e.to_string())?;
            }
            Ok::<(), String>(())
        });
    }

    fn settle(&mut self, epoch: u64, cx: &mut Context<Self>) {
        if epoch != self.settle_epoch {
            return;
        }
        self.emit_save(cx);
    }

    fn schedule_settle(&mut self, cx: &mut Context<Self>) {
        self.settle_epoch = self.settle_epoch.wrapping_add(1);
        let epoch = self.settle_epoch;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SLIDE_SETTLE).await;
            let _ = this.update(cx, |this, cx| this.settle(epoch, cx));
        })
        .detach();
    }

    fn on_field_committed(&mut self, event: &forge_components::InputEvent, cx: &mut Context<Self>) {
        if matches!(
            event,
            forge_components::InputEvent::Submitted(_) | forge_components::InputEvent::Blurred(_)
        ) {
            self.bound_design_inputs(cx);
            self.emit_save(cx);
            cx.notify();
        }
    }

    fn toggle_field(&mut self, key: String, cx: &mut Context<Self>) {
        for field in &mut self.fields {
            if let ConfigField::Bool { key: k, value, .. } = field
                && *k == key
            {
                *value = !*value;
            }
        }
        self.emit_save(cx);
        cx.notify();
    }

    fn slide_field(&mut self, key: String, next: i64, cx: &mut Context<Self>) {
        let mut moved = false;
        for field in &mut self.fields {
            if let ConfigField::Slide { key: k, value, .. } = field
                && *k == key
                && *value != next
            {
                *value = next;
                moved = true;
            }
        }
        if !moved {
            return;
        }
        self.schedule_settle(cx);
        cx.notify();
    }

    fn pick_swatch(&mut self, key: String, choice: String, cx: &mut Context<Self>) {
        for field in &mut self.fields {
            if let ConfigField::Swatch {
                key: k, selected, ..
            } = field
                && *k == key
            {
                selected.clone_from(&choice);
            }
        }
        self.emit_save(cx);
        cx.notify();
    }

    fn request_icon_pick(&mut self, key: String, cx: &mut Context<Self>) {
        self.picker = None;
        self.pick_refusal = None;
        cx.emit(IconPickRequested { key });
        cx.notify();
    }

    fn open_choice(&mut self, key: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.picker.as_ref().is_some_and(|open| open.key == key) {
            self.close_choice(cx);
            return;
        }
        let Some(ConfigField::Choice {
            options, selected, ..
        }) = self
            .fields
            .iter()
            .find(|field| matches!(field, ConfigField::Choice { key: k, .. } if *k == key))
        else {
            return;
        };

        let items: Vec<PickerItem> = options
            .iter()
            .map(|(value, label)| PickerItem {
                id: SharedString::from(value.clone()),
                label: SharedString::from(label.clone()),
                sublabel: None,
                icon: None,
            })
            .collect();
        let labels = PickerLabels {
            placeholder: tr!("widget_picker_search_placeholder").into(),
            empty: tr!("overlays_panel_choice_empty").into(),
            loading: tr!("widget_picker_loading").into(),
        };
        let current = Some(SharedString::from(selected.clone()));
        let palette = cx.palette();
        let picker = cx.new(|cx| Picker::new(labels, items, palette, cx).with_current(current));
        let sub = cx.subscribe(&picker, Self::on_picker_event);
        picker.update(cx, |picker, cx| picker.focus(window, cx));
        self.picker = Some(ChoicePicker {
            key,
            picker,
            _sub: sub,
        });
        cx.notify();
    }

    fn on_picker_event(
        &mut self,
        _picker: Entity<Picker>,
        event: &PickerEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            PickerEvent::Selected(value) => self.apply_choice(value.to_string(), cx),
            PickerEvent::Cancelled => self.close_choice(cx),
        }
    }

    fn apply_choice(&mut self, value: String, cx: &mut Context<Self>) {
        let Some(key) = self.picker.as_ref().map(|open| open.key.clone()) else {
            return;
        };
        self.picker = None;
        self.pick_refusal = None;

        if let Some(clip) = picked_clip(&key, &value) {
            self.pending_pick = Some(PendingPick {
                key: key.clone(),
                value: value.clone(),
            });
            cx.emit(AdoptClipRequested { key, value, clip });
            cx.notify();
            return;
        }
        self.commit_choice(key, value, cx);
    }

    pub(super) fn settle_clip_pick(
        &mut self,
        key: String,
        value: String,
        outcome: PickOutcome,
        cx: &mut Context<Self>,
    ) {
        let stale = match self.pending_pick.as_ref() {
            Some(pending) => pending.key != key || pending.value != value,
            None => true,
        };
        if stale {
            return;
        }
        self.pending_pick = None;
        match outcome {
            PickOutcome::Accepted => self.commit_choice(key, value, cx),
            PickOutcome::Refused(message) => {
                self.pick_refusal = Some((key, message));
                cx.notify();
            }
        }
    }

    fn commit_choice(&mut self, key: String, value: String, cx: &mut Context<Self>) {
        for field in &mut self.fields {
            if let ConfigField::Choice {
                key: k, selected, ..
            } = field
                && *k == key
            {
                selected.clone_from(&value);
            }
        }
        let Self {
            fields, choices, ..
        } = self;
        resolve_dependent_choices(fields, choices, cx);
        self.refresh_media_choices();
        self.emit_save(cx);
        cx.notify();
    }

    fn close_choice(&mut self, cx: &mut Context<Self>) {
        self.picker = None;
        cx.notify();
    }

    pub(super) fn label_of(&self, key: &str) -> String {
        if let Some(label) = base_label(key).or_else(|| motion_label(key)) {
            return label;
        }
        self.labels
            .get(key)
            .cloned()
            .unwrap_or_else(|| key.to_owned())
    }

    fn section_of(&self, key: &str) -> PanelSection {
        self.sections
            .get(key)
            .copied()
            .unwrap_or(PanelSection::Content)
    }

    fn notes_for(&self, key: &str) -> Vec<String> {
        let adopting = self
            .pending_pick
            .as_ref()
            .is_some_and(|pending| pending.key == key);
        let refusal = self
            .pick_refusal
            .as_ref()
            .filter(|(refused_key, _)| refused_key == key)
            .map(|(_, message)| message.as_str());
        let mut notes = field_notes(&self.media_issues, key, adopting, refusal);
        notes.extend(self.motion_notices.notes_for(key));
        notes
    }

    pub(super) fn handlers(&self) -> ConfigFieldHandlers<Self> {
        ConfigFieldHandlers {
            toggle: Self::toggle_field,
            slide: Self::slide_field,
            pick: Self::pick_swatch,
            choice: Some(ChoiceDropdown {
                open: Self::open_choice,
                close: Self::close_choice,
                active: self
                    .picker
                    .as_ref()
                    .map(|open| (open.key.clone(), open.picker.clone())),
            }),
        }
    }

    fn icon_control(
        &self,
        field: &ConfigField,
        palette: &ForgePalette,
        view: &Entity<Self>,
    ) -> Option<AnyElement> {
        let ConfigField::Choice { key, selected, .. } = field else {
            return None;
        };
        if media_slot(key) != Some(MediaSlot::Icon) {
            return None;
        }
        let open_key = key.clone();
        let view = view.clone();
        let hover_border = palette.brand;
        Some(
            icon_field_row(
                selected,
                &self.icon_images,
                self.accent_tint(palette),
                palette,
            )
            .id(SharedString::from(format!("overlays-panel-{key}")))
            .cursor_pointer()
            .hover(move |style| style.border_color(hover_border))
            .on_click(
                move |_: &gpui::ClickEvent, _window: &mut Window, cx: &mut App| {
                    view.update(cx, |this, cx| this.request_icon_pick(open_key.clone(), cx));
                },
            )
            .into_any_element(),
        )
    }

    fn render_section(
        &self,
        section: PanelSection,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let members: Vec<&ConfigField> = self
            .fields
            .iter()
            .filter(|field| self.section_of(field.key()) == section)
            .collect();
        if members.is_empty() && section != PanelSection::Display {
            return None;
        }

        let mut column = div().flex().flex_col().pt(SECTION_TOP_GAP).child(
            div()
                .pb(SECTION_GAP)
                .child(section_label(section.heading().to_uppercase(), palette)),
        );

        let view = cx.entity();
        let handlers = self.handlers();
        for field in members {
            let control = match self.icon_control(field, palette, &view) {
                Some(icon_control) => icon_control,
                None => render_config_control(field, palette, "overlays-panel", &view, &handlers),
            };
            let body = match notes_block(self.notes_for(field.key()), palette) {
                Some(notes) => div()
                    .flex()
                    .flex_col()
                    .child(control)
                    .child(notes)
                    .into_any_element(),
                None => control,
            };
            let body = match base_hint(field.key()).or_else(|| motion_hint(field.key())) {
                Some(hint) => hinted(body, hint, palette),
                None => body,
            };
            column = column.child(
                div().pb(FIELD_GAP).child(
                    field_label(palette, self.label_of(field.key()).to_uppercase(), body)
                        .tone(palette.text_faint),
                ),
            );
        }
        if section == PanelSection::Display {
            column = column.children(self.render_display_extras(palette, cx));
        }

        Some(column.into_any_element())
    }
}

pub(super) fn override_notice(files: &[String], palette: &ForgePalette) -> Option<AnyElement> {
    if files.is_empty() {
        return None;
    }
    let named = files.join(", ");
    Some(
        div()
            .p(NOTICE_PAD)
            .mb(SECTION_TOP_GAP)
            .rounded(NOTICE_RADIUS)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.base)
            .font_family(body_family())
            .text_size(FONT_XXS)
            .line_height(NOTICE_LINE_H)
            .text_color(palette.text_muted)
            .child(tr!(
                "overlays_panel_override_notice",
                files = named.as_str()
            ))
            .into_any_element(),
    )
}

struct FieldIndex {
    labels: HashMap<String, String>,
    sections: HashMap<String, PanelSection>,
}

fn index_fields(specs: &[SectionedField]) -> FieldIndex {
    let mut index = FieldIndex {
        labels: HashMap::new(),
        sections: HashMap::new(),
    };
    for spec in specs {
        index_field(&spec.field, spec.section, &mut index);
    }
    index
}

fn index_field(spec: &FormField, section: ConfigSection, out: &mut FieldIndex) {
    match spec {
        FormField::Text { key, label, .. }
        | FormField::TextArea { key, label, .. }
        | FormField::Code { key, label, .. }
        | FormField::Integer { key, label, .. }
        | FormField::Slider { key, label, .. }
        | FormField::Toggle { key, label }
        | FormField::FilePicker { key, label }
        | FormField::DateTime { key, label }
        | FormField::Select { key, label, .. }
        | FormField::DynamicSelect { key, label, .. }
        | FormField::DependentSelect { key, label, .. }
        | FormField::Swatch { key, label, .. }
        | FormField::SubChain { key, label }
        | FormField::CaseList { key, label } => {
            out.labels.insert((*key).to_owned(), (*label).to_owned());
            out.sections
                .insert((*key).to_owned(), PanelSection::of(key, section));
        }
        FormField::Optional { key, label, inner } => {
            out.labels.insert((*key).to_owned(), (*label).to_owned());
            out.sections
                .insert((*key).to_owned(), PanelSection::of(key, section));
            index_field(inner, section, out);
        }
    }
}

impl Render for OverlayPropertyPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();

        let mut body = div()
            .id("overlays-panel-scroll")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .p(PANE_PAD)
            .flex()
            .flex_col();

        body = body
            .children(self.render_sizing_notice(&palette, cx))
            .children(override_notice(&self.overridden_files, &palette))
            .child(self.render_look_section(&palette, cx))
            .children(self.render_latest_section(&palette, cx));

        for section in FIELD_SECTIONS {
            body = body.children(match section {
                PanelSection::Source => self.render_source_section(&palette, cx),
                _ => self.render_section(section, &palette, cx),
            });
        }
        body = body
            .children(self.render_section(PanelSection::Display, &palette, cx))
            .child(self.render_receiver_section(&palette, cx));

        div()
            .flex_none()
            .w(PANE_W)
            .min_w(PANE_W)
            .max_w(PANE_W)
            .h_full()
            .flex()
            .flex_col()
            .bg(palette.shell)
            .border_l(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(body)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
pub(crate) mod tests {
    use std::sync::Mutex;

    use forge_components::{Density, ThemeId};
    use forge_overlay::config::{
        ACCENT_OPTIONS, ELEMENT_WIDTH, HEADLINE, ICON, ICON_OPTIONS_KEY, SOUND,
    };
    use forge_overlay::config::{
        DESIGN_HEIGHT, DESIGN_SIZE_MAX_PX, DESIGN_SIZE_MIN_PX, DESIGN_WIDTH, MARGIN_BOTTOM,
        MARGIN_LEFT, MARGIN_MAX_PERCENT, MARGIN_RIGHT, MARGIN_TOP, MIGRATION_ACKNOWLEDGED,
    };
    use forge_overlay::kinds::frame::FrameOverlayKind;
    use forge_overlay::kinds::goal::GoalOverlayKind;
    use forge_overlay::{OverlayKindDescriptor, OverlayKindRegistry, image_reference};
    use forge_registry::FormField;
    use forge_runtime::EventBus;
    use forge_storage::{OverlayCredential, OverlayDefinition, StorageError};
    use forge_types::Variant;
    use time::OffsetDateTime;

    use super::super::base_sections::BaseLaunch;
    use super::super::look_change::look_summary;
    use super::super::sizing_section::is_source_box_key;
    use super::*;
    use crate::presentation::Presentation;
    use crate::test_support::{StubEventLog, pump, runtime, test_backend};

    const OVERLAY: &str = "alert-one";
    const KIND: &str = "overlay.alert";
    const TOKEN: &str = "token";
    const DEFAULT_HEADLINE: &str = "hello";
    const EDITED_HEADLINE: &str = "goodbye";
    const CANVAS_WIDTH: &str = "canvas_width";
    const CANVAS_HEIGHT: &str = "canvas_height";

    pub(crate) struct RecordingOverlays {
        definition: Mutex<OverlayDefinition>,
        saved: Mutex<Vec<OverlayConfig>>,
    }

    impl RecordingOverlays {
        fn new() -> Arc<Self> {
            let now = OffsetDateTime::UNIX_EPOCH;
            Arc::new(Self {
                definition: Mutex::new(OverlayDefinition {
                    id: OverlayId::new(OVERLAY),
                    display_name: OVERLAY.to_owned(),
                    kind_id: KIND.to_owned(),
                    enabled: true,
                    position: 0,
                    config: OverlayConfig::new(),
                    config_schema_version: 1,
                    generator_version: 1,
                    source_overrides: Vec::new(),
                    credential: OverlayCredential::new(TOKEN),
                    created_at: now,
                    updated_at: now,
                }),
                saved: Mutex::new(Vec::new()),
            })
        }

        fn saved(&self) -> Vec<OverlayConfig> {
            self.saved.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl OverlayRepo for RecordingOverlays {
        async fn list(&self) -> Result<Vec<OverlayDefinition>, StorageError> {
            Ok(vec![self.definition.lock().unwrap().clone()])
        }

        async fn get(&self, _: &OverlayId) -> Result<Option<OverlayDefinition>, StorageError> {
            Ok(Some(self.definition.lock().unwrap().clone()))
        }

        async fn get_by_credential(
            &self,
            _: &OverlayCredential,
        ) -> Result<Option<OverlayDefinition>, StorageError> {
            Ok(None)
        }

        async fn create(
            &self,
            _: &str,
            _: &str,
            _: u32,
        ) -> Result<OverlayDefinition, StorageError> {
            unreachable!("the property panel never creates an overlay")
        }

        async fn save(&self, definition: &OverlayDefinition) -> Result<(), StorageError> {
            self.saved.lock().unwrap().push(definition.config.clone());
            Ok(())
        }

        async fn set_enabled(&self, _: &OverlayId, _: bool) -> Result<bool, StorageError> {
            unreachable!("the property panel never toggles an overlay")
        }

        async fn delete(&self, _: &OverlayId) -> Result<bool, StorageError> {
            unreachable!("the property panel never deletes an overlay")
        }

        async fn get_retained_content(
            &self,
            _: &OverlayId,
        ) -> Result<Option<OverlayConfig>, StorageError> {
            Ok(None)
        }

        async fn set_retained_content(
            &self,
            _: &OverlayId,
            _: &OverlayConfig,
        ) -> Result<(), StorageError> {
            unreachable!("the property panel never writes overlay content")
        }
    }

    struct Saves {
        seen: Vec<OverlayConfig>,
        _sub: Subscription,
    }

    fn config(entries: &[(&str, Variant)]) -> OverlayConfig {
        entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect()
    }

    pub(crate) fn defaults() -> OverlayConfig {
        config(&[
            (HEADLINE, Variant::String(DEFAULT_HEADLINE.into())),
            (ACCENT, Variant::String("mauve".into())),
        ])
    }

    pub(crate) fn specs() -> Vec<SectionedField> {
        vec![
            SectionedField {
                section: ConfigSection::Content,
                field: FormField::Text {
                    key: HEADLINE,
                    label: "Headline",
                    placeholder: "",
                },
            },
            SectionedField {
                section: ConfigSection::Style,
                field: FormField::Integer {
                    key: ELEMENT_WIDTH,
                    label: "Width",
                    min: 40,
                    max: 1920,
                },
            },
            SectionedField {
                section: ConfigSection::Style,
                field: FormField::Swatch {
                    key: ACCENT,
                    label: "Accent",
                    options: ACCENT_OPTIONS,
                },
            },
        ]
    }

    pub(crate) fn launch(
        cx: &mut gpui::TestAppContext,
        stored: OverlayConfig,
        specs: Vec<SectionedField>,
        defaults: OverlayConfig,
        choices: HashMap<String, Vec<(String, String)>>,
    ) -> (PanelLaunch, Arc<RecordingOverlays>, tokio::runtime::Runtime) {
        cx.update(|cx| {
            cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
        });
        let rt = runtime();
        let repo = RecordingOverlays::new();
        let (backend, _writes) = test_backend();
        let service = OverlayServiceHandle::new(
            Arc::clone(&repo) as Arc<dyn OverlayRepo>,
            backend as Arc<dyn forge_storage::SettingsRepo>,
            Arc::new(OverlayKindRegistry::new()),
            EventBus::new(Arc::new(StubEventLog)),
            None,
        );
        let mut effective = defaults.clone();
        for (key, value) in &stored {
            effective.insert(key.clone(), value.clone());
        }
        let launch = PanelLaunch {
            overlay_id: OverlayId::new(OVERLAY),
            specs,
            defaults,
            stored,
            effective,
            choices,
            icon_images: Vec::new(),
            overridden_files: Vec::new(),
            repo: Arc::clone(&repo) as Arc<dyn OverlayRepo>,
            service,
            rt_handle: rt.handle().clone(),
        };
        (launch, repo, rt)
    }

    struct Fixture {
        panel: Option<Entity<OverlayPropertyPanel>>,
        saves: Entity<Saves>,
        repo: Arc<RecordingOverlays>,
        rt: tokio::runtime::Runtime,
    }

    impl Fixture {
        fn new(cx: &mut gpui::TestAppContext, stored: OverlayConfig) -> Self {
            Self::build(cx, stored, specs(), HashMap::new())
        }

        fn with_sound_library(
            cx: &mut gpui::TestAppContext,
            stored: OverlayConfig,
            clips: Vec<(String, String)>,
        ) -> Self {
            let mut sound_specs = specs();
            sound_specs.push(SectionedField {
                section: ConfigSection::Behavior,
                field: FormField::DynamicSelect {
                    key: SOUND,
                    label: "Sound",
                    options_key: SOUND_OPTIONS_KEY,
                },
            });
            Self::build(
                cx,
                stored,
                sound_specs,
                HashMap::from([(SOUND_OPTIONS_KEY.to_owned(), clips)]),
            )
        }

        fn build(
            cx: &mut gpui::TestAppContext,
            stored: OverlayConfig,
            specs: Vec<SectionedField>,
            choices: HashMap<String, Vec<(String, String)>>,
        ) -> Self {
            Self::build_over(cx, stored, specs, defaults(), choices)
        }

        fn build_over(
            cx: &mut gpui::TestAppContext,
            stored: OverlayConfig,
            specs: Vec<SectionedField>,
            defaults: OverlayConfig,
            choices: HashMap<String, Vec<(String, String)>>,
        ) -> Self {
            let (launch, repo, rt) = launch(cx, stored, specs, defaults, choices);
            let panel = cx.update(|cx| cx.new(|cx| OverlayPropertyPanel::new(launch, cx)));
            let saves = cx.update(|cx| {
                cx.new(|cx| Saves {
                    seen: Vec::new(),
                    _sub: cx.subscribe(&panel, |this: &mut Saves, _panel, event, _cx| {
                        let PropertyPanelEvent::Save(config) = event;
                        this.seen.push(config.clone());
                    }),
                })
            });
            Self {
                panel: Some(panel),
                saves,
                repo,
                rt,
            }
        }

        fn panel(&self) -> &Entity<OverlayPropertyPanel> {
            self.panel.as_ref().unwrap()
        }

        fn type_into(&self, cx: &mut gpui::TestAppContext, key: &str, text: &str) {
            self.panel().update(cx, |panel, cx| {
                for field in &panel.fields {
                    if let ConfigField::Input {
                        key: field_key,
                        input,
                        ..
                    } = field
                        && field_key == key
                    {
                        input.update(cx, |input, cx| input.set_content(text.to_owned(), cx));
                    }
                }
            });
        }

        fn commit(&self, cx: &mut gpui::TestAppContext) {
            self.panel().update(cx, |panel, cx| panel.emit_save(cx));
            cx.run_until_parked();
        }

        fn saves(&self, cx: &mut gpui::TestAppContext) -> Vec<OverlayConfig> {
            cx.update(|cx| self.saves.read(cx).seen.clone())
        }

        fn release(&mut self, cx: &mut gpui::TestAppContext) {
            self.panel = None;
            cx.update(|_cx| {});
            cx.run_until_parked();
            pump(&self.rt);
        }
    }

    #[gpui::test]
    fn a_commit_that_moves_nothing_never_reaches_the_record(cx: &mut gpui::TestAppContext) {
        let fixture = Fixture::new(cx, OverlayConfig::new());

        fixture.commit(cx);
        fixture.commit(cx);

        assert!(fixture.saves(cx).is_empty());
    }

    #[gpui::test]
    fn an_edited_field_saves_once_and_the_commit_that_repeats_it_is_silent(
        cx: &mut gpui::TestAppContext,
    ) {
        let fixture = Fixture::new(cx, OverlayConfig::new());
        fixture.type_into(cx, HEADLINE, EDITED_HEADLINE);

        fixture.commit(cx);
        fixture.commit(cx);

        assert_eq!(
            fixture.saves(cx),
            vec![config(&[(
                HEADLINE,
                Variant::String(EDITED_HEADLINE.into())
            )])]
        );
    }

    #[gpui::test]
    fn a_save_forgets_the_retired_canvas_keys_and_carries_every_other_one_forward(
        cx: &mut gpui::TestAppContext,
    ) {
        let fixture = Fixture::new(
            cx,
            config(&[
                (CANVAS_WIDTH, Variant::Int(1920)),
                (CANVAS_HEIGHT, Variant::Int(1080)),
                (ACCENT, Variant::String("sky".into())),
            ]),
        );

        fixture.commit(cx);

        assert_eq!(
            fixture.saves(cx),
            vec![config(&[(ACCENT, Variant::String("sky".into()))])]
        );
    }

    #[gpui::test]
    fn clearing_the_width_drops_the_element_width_key(cx: &mut gpui::TestAppContext) {
        let fixture = Fixture::new(cx, config(&[(ELEMENT_WIDTH, Variant::Int(600))]));
        fixture.type_into(cx, ELEMENT_WIDTH, "");

        fixture.commit(cx);

        assert_eq!(fixture.saves(cx), vec![OverlayConfig::new()]);
    }

    #[gpui::test]
    fn a_panel_torn_down_over_an_uncommitted_edit_writes_it_straight_through_the_repo(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut fixture = Fixture::new(cx, OverlayConfig::new());
        fixture.type_into(cx, HEADLINE, EDITED_HEADLINE);

        fixture.release(cx);

        assert_eq!(
            fixture.repo.saved(),
            vec![config(&[(
                HEADLINE,
                Variant::String(EDITED_HEADLINE.into())
            )])]
        );
    }

    #[gpui::test]
    fn a_panel_torn_down_after_its_commit_landed_writes_nothing_more(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut fixture = Fixture::new(cx, OverlayConfig::new());
        fixture.type_into(cx, HEADLINE, EDITED_HEADLINE);
        fixture.commit(cx);

        fixture.release(cx);

        assert!(fixture.repo.saved().is_empty());
    }

    #[gpui::test]
    fn a_panel_torn_down_over_an_untouched_form_writes_nothing(cx: &mut gpui::TestAppContext) {
        let mut fixture = Fixture::new(cx, config(&[(ELEMENT_WIDTH, Variant::Int(600))]));

        fixture.release(cx);

        assert!(fixture.repo.saved().is_empty());
    }

    #[gpui::test]
    fn the_unsaved_config_is_handed_over_once_and_only_when_the_form_moved(
        cx: &mut gpui::TestAppContext,
    ) {
        let edited = config(&[(HEADLINE, Variant::String(EDITED_HEADLINE.into()))]);
        for (typed, expected) in [
            (None, [None, None]),
            (Some(EDITED_HEADLINE), [Some(edited), None]),
        ] {
            let fixture = Fixture::new(cx, OverlayConfig::new());
            if let Some(text) = typed {
                fixture.type_into(cx, HEADLINE, text);
            }

            let taken = [(); 2].map(|()| {
                fixture
                    .panel()
                    .update(cx, |panel, cx| panel.take_unsaved_config(cx))
            });

            assert_eq!(taken, expected, "typed {typed:?}");
        }
    }

    #[gpui::test]
    fn a_panel_torn_down_after_its_edit_was_handed_over_writes_nothing_more(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut fixture = Fixture::new(cx, OverlayConfig::new());
        fixture.type_into(cx, HEADLINE, EDITED_HEADLINE);
        fixture
            .panel()
            .update(cx, |panel, cx| panel.take_unsaved_config(cx));

        fixture.release(cx);

        assert!(fixture.repo.saved().is_empty());
    }

    const CLIP_A: &str = "clip:01J9P4S2M7Q8V3X5Y6Z7A8B9C0";
    const CLIP_B: &str = "clip:01J9P4S2M7Q8V3X5Y6Z7A8B9C1";

    impl Fixture {
        fn sound_options(&self, cx: &mut gpui::TestAppContext) -> Vec<String> {
            self.panel().read_with(cx, |panel, _| {
                panel
                    .fields
                    .iter()
                    .find_map(|field| match field {
                        ConfigField::Choice { key, options, .. } if key == SOUND => {
                            Some(options.iter().map(|(value, _)| value.clone()).collect())
                        }
                        _ => None,
                    })
                    .expect("the sound field is a choice over the library")
            })
        }

        fn sound_selection(&self, cx: &mut gpui::TestAppContext) -> String {
            self.panel().read_with(cx, |panel, _| {
                panel
                    .fields
                    .iter()
                    .find_map(|field| match field {
                        ConfigField::Choice { key, selected, .. } if key == SOUND => {
                            Some(selected.clone())
                        }
                        _ => None,
                    })
                    .expect("the sound field is a choice over the library")
            })
        }

        fn expect_pick(&self, cx: &mut gpui::TestAppContext, value: &str) {
            self.panel().update(cx, |panel, _| {
                panel.pending_pick = Some(PendingPick {
                    key: SOUND.to_owned(),
                    value: value.to_owned(),
                });
            });
        }

        fn settle(&self, cx: &mut gpui::TestAppContext, value: &str, outcome: PickOutcome) {
            self.panel().update(cx, |panel, cx| {
                panel.settle_clip_pick(SOUND.to_owned(), value.to_owned(), outcome, cx);
            });
            cx.run_until_parked();
        }

        fn sound_notes(&self, cx: &mut gpui::TestAppContext) -> Vec<String> {
            self.panel()
                .read_with(cx, |panel, _| panel.notes_for(SOUND))
        }
    }

    #[gpui::test]
    fn a_sound_field_offers_silence_before_the_library_it_was_launched_with(
        cx: &mut gpui::TestAppContext,
    ) {
        let fixture = Fixture::with_sound_library(
            cx,
            OverlayConfig::new(),
            vec![(CLIP_A.to_owned(), "Fanfare".to_owned())],
        );

        assert_eq!(
            fixture.sound_options(cx),
            vec![String::new(), CLIP_A.to_owned()]
        );
    }

    #[gpui::test]
    fn a_library_that_arrives_after_the_panel_opens_still_reaches_the_sound_field(
        cx: &mut gpui::TestAppContext,
    ) {
        let fixture = Fixture::with_sound_library(cx, OverlayConfig::new(), Vec::new());

        fixture.panel().update(cx, |panel, cx| {
            panel.set_sound_choices(vec![(CLIP_B.to_owned(), "Airhorn".to_owned())], cx);
        });

        assert_eq!(
            fixture.sound_options(cx),
            vec![String::new(), CLIP_B.to_owned()],
            "the picker must offer clips the screen loaded after the panel was built"
        );
    }

    #[gpui::test]
    fn an_adoption_answer_for_a_pick_the_user_moved_past_never_moves_the_field(
        cx: &mut gpui::TestAppContext,
    ) {
        let fixture = Fixture::with_sound_library(
            cx,
            OverlayConfig::new(),
            vec![
                (CLIP_A.to_owned(), "Fanfare".to_owned()),
                (CLIP_B.to_owned(), "Airhorn".to_owned()),
            ],
        );
        fixture.expect_pick(cx, CLIP_B);

        fixture.settle(cx, CLIP_A, PickOutcome::Accepted);

        assert_eq!(
            fixture.sound_selection(cx),
            "",
            "a late answer about an earlier pick overwrote the one the user is waiting on"
        );
        assert!(fixture.saves(cx).is_empty());
    }

    #[gpui::test]
    fn an_answer_with_nothing_pending_is_ignored(cx: &mut gpui::TestAppContext) {
        let fixture = Fixture::with_sound_library(
            cx,
            OverlayConfig::new(),
            vec![(CLIP_A.to_owned(), "Fanfare".to_owned())],
        );

        fixture.settle(cx, CLIP_A, PickOutcome::Accepted);

        assert_eq!(fixture.sound_selection(cx), "");
        assert!(fixture.saves(cx).is_empty());
    }

    #[gpui::test]
    fn a_clip_that_reached_the_library_is_committed_and_saved(cx: &mut gpui::TestAppContext) {
        let fixture = Fixture::with_sound_library(
            cx,
            OverlayConfig::new(),
            vec![(CLIP_A.to_owned(), "Fanfare".to_owned())],
        );
        fixture.expect_pick(cx, CLIP_A);

        fixture.settle(cx, CLIP_A, PickOutcome::Accepted);

        assert_eq!(fixture.sound_selection(cx), CLIP_A);
        assert_eq!(
            fixture
                .saves(cx)
                .last()
                .and_then(|config| config.get(SOUND).cloned()),
            Some(Variant::String(CLIP_A.to_owned()))
        );
    }

    #[gpui::test]
    fn a_refused_clip_leaves_the_field_alone_and_states_the_reason(cx: &mut gpui::TestAppContext) {
        let fixture = Fixture::with_sound_library(
            cx,
            OverlayConfig::new(),
            vec![(CLIP_A.to_owned(), "Fanfare".to_owned())],
        );
        fixture.expect_pick(cx, CLIP_A);

        fixture.settle(
            cx,
            CLIP_A,
            PickOutcome::Refused("the file is not audio".to_owned()),
        );

        assert_eq!(fixture.sound_selection(cx), "");
        assert_eq!(fixture.sound_notes(cx), vec!["the file is not audio"]);
        assert!(fixture.saves(cx).is_empty());
    }

    #[gpui::test]
    fn an_unresolved_reference_reported_by_the_pass_is_shown_on_its_own_field(
        cx: &mut gpui::TestAppContext,
    ) {
        let fixture = Fixture::with_sound_library(cx, OverlayConfig::new(), Vec::new());

        fixture.panel().update(cx, |panel, cx| {
            panel.set_media_issues(
                vec![MediaIssue::ClipOutsideLibrary {
                    key: SOUND.to_owned(),
                    clip: "01J9P4S2M7Q8V3X5Y6Z7A8B9C0".to_owned(),
                }],
                cx,
            );
        });

        assert_eq!(
            fixture.sound_notes(cx),
            vec!["overlays_sound_issue_outside_library"]
        );
        assert!(
            fixture
                .panel()
                .read_with(cx, |panel, _| panel.notes_for(HEADLINE).is_empty()),
            "an issue raised against the sound key was shown on another field"
        );
    }

    const GLYPH: &str = "heart";
    const OTHER_GLYPH: &str = "gift";
    const IMAGE_BLOB: &str =
        "sha256-610f5ae4d76e332636a17bd357fd6ce99029316a99d320280d4d77a746bf29e8";
    const IMPORT_REFUSAL: &str = "the file is not an image";

    impl Fixture {
        fn with_icon_field(cx: &mut gpui::TestAppContext, stored: OverlayConfig) -> Self {
            let mut icon_specs = specs();
            icon_specs.push(SectionedField {
                section: ConfigSection::Style,
                field: FormField::DynamicSelect {
                    key: ICON,
                    label: "Icon",
                    options_key: ICON_OPTIONS_KEY,
                },
            });
            Self::build(cx, stored, icon_specs, HashMap::new())
        }

        fn icon_selection(&self, cx: &mut gpui::TestAppContext) -> String {
            self.panel().read_with(cx, |panel, _| {
                panel
                    .fields
                    .iter()
                    .find_map(|field| match field {
                        ConfigField::Choice { key, selected, .. } if key == ICON => {
                            Some(selected.clone())
                        }
                        _ => None,
                    })
                    .expect("the icon field is a choice over the curated roster")
            })
        }

        fn icon_notes(&self, cx: &mut gpui::TestAppContext) -> Vec<String> {
            self.panel().read_with(cx, |panel, _| panel.notes_for(ICON))
        }

        fn saved_icons(&self, cx: &mut gpui::TestAppContext) -> Vec<Option<String>> {
            self.saves(cx)
                .iter()
                .map(|config| {
                    config
                        .get(ICON)
                        .and_then(Variant::as_str)
                        .map(str::to_owned)
                })
                .collect()
        }

        fn begin_import(&self, cx: &mut gpui::TestAppContext) {
            self.panel()
                .update(cx, |panel, cx| panel.begin_icon_import(ICON.to_owned(), cx));
        }

        fn pick_icon(&self, cx: &mut gpui::TestAppContext, value: &str) {
            self.panel().update(cx, |panel, cx| {
                panel.settle_icon_pick(
                    ICON.to_owned(),
                    IconPickResult::Chosen(value.to_owned()),
                    cx,
                );
            });
            cx.run_until_parked();
        }

        fn settle_import(&self, cx: &mut gpui::TestAppContext, result: IconPickResult) {
            self.panel().update(cx, |panel, cx| {
                panel.settle_icon_import(ICON.to_owned(), result, cx);
            });
            cx.run_until_parked();
        }

        fn cancel_import(&self, cx: &mut gpui::TestAppContext) {
            self.panel().update(cx, |panel, cx| {
                panel.cancel_icon_import(ICON.to_owned(), cx)
            });
            cx.run_until_parked();
        }

        fn icon_tint(&self, cx: &mut gpui::TestAppContext) -> Rgba {
            self.panel()
                .read_with(cx, |panel, cx| panel.accent_tint(&cx.palette()))
        }
    }

    #[gpui::test]
    fn a_late_import_answer_for_an_icon_the_user_moved_past_never_moves_the_field(
        cx: &mut gpui::TestAppContext,
    ) {
        let fixture = Fixture::with_icon_field(cx, OverlayConfig::new());
        fixture.begin_import(cx);
        fixture.pick_icon(cx, GLYPH);

        fixture.settle_import(cx, IconPickResult::Chosen(image_reference(IMAGE_BLOB)));

        assert_eq!(
            fixture.icon_selection(cx),
            GLYPH,
            "the file the user abandoned overwrote the glyph they picked instead"
        );
        assert_eq!(fixture.saved_icons(cx), vec![Some(GLYPH.to_owned())]);
    }

    #[gpui::test]
    fn an_imported_image_the_field_is_still_waiting_on_is_committed_and_saved(
        cx: &mut gpui::TestAppContext,
    ) {
        let fixture = Fixture::with_icon_field(cx, OverlayConfig::new());
        fixture.begin_import(cx);

        fixture.settle_import(cx, IconPickResult::Chosen(image_reference(IMAGE_BLOB)));

        assert_eq!(fixture.icon_selection(cx), image_reference(IMAGE_BLOB));
        assert_eq!(
            fixture.saved_icons(cx),
            vec![Some(image_reference(IMAGE_BLOB))]
        );
    }

    #[gpui::test]
    fn a_refused_icon_import_leaves_the_field_alone_and_states_the_reason(
        cx: &mut gpui::TestAppContext,
    ) {
        let fixture =
            Fixture::with_icon_field(cx, config(&[(ICON, Variant::String(GLYPH.into()))]));
        fixture.begin_import(cx);

        fixture.settle_import(cx, IconPickResult::Refused(IMPORT_REFUSAL.to_owned()));

        assert_eq!(fixture.icon_selection(cx), GLYPH);
        assert_eq!(fixture.icon_notes(cx), vec![IMPORT_REFUSAL]);
        assert!(fixture.saves(cx).is_empty());
    }

    #[gpui::test]
    fn a_dialog_cancel_that_lands_after_a_direct_pick_leaves_the_settled_icon_alone(
        cx: &mut gpui::TestAppContext,
    ) {
        let fixture =
            Fixture::with_icon_field(cx, config(&[(ICON, Variant::String(GLYPH.into()))]));
        fixture.begin_import(cx);
        fixture.pick_icon(cx, OTHER_GLYPH);

        fixture.cancel_import(cx);

        assert_eq!(fixture.icon_selection(cx), OTHER_GLYPH);
        assert_eq!(fixture.saved_icons(cx), vec![Some(OTHER_GLYPH.to_owned())]);
    }

    #[gpui::test]
    fn the_icon_tile_is_tinted_by_the_stored_accent_and_by_the_brand_when_it_names_none(
        cx: &mut gpui::TestAppContext,
    ) {
        let named = Fixture::new(cx, config(&[(ACCENT, Variant::String("sky".into()))]));
        let unknown = Fixture::new(
            cx,
            config(&[(ACCENT, Variant::String("chartreuse".into()))]),
        );
        let palette = cx.update(|cx| cx.palette());

        assert_eq!(
            named.icon_tint(cx),
            palette.info,
            "the icon tile ignored the accent the overlay stores"
        );
        assert_eq!(
            unknown.icon_tint(cx),
            palette.brand,
            "an accent this palette cannot name has to fall back to the brand tint"
        );
    }

    const PANEL_H: f32 = 1200.0;
    const SCAN_STEP: f32 = 4.0;
    const HELD_WIDTH: i64 = 1500;

    fn source_specs() -> Vec<SectionedField> {
        FrameOverlayKind
            .config_fields()
            .into_iter()
            .filter(|sectioned| is_source_box_key(sectioned.field.key()))
            .collect()
    }

    fn int_at(config: &OverlayConfig, key: &str) -> Option<i64> {
        config.get(key).and_then(Variant::as_int)
    }

    impl Fixture {
        fn with_source_box(cx: &mut gpui::TestAppContext, stored: OverlayConfig) -> Self {
            Self::build_over(
                cx,
                stored,
                source_specs(),
                FrameOverlayKind.default_config(),
                HashMap::new(),
            )
        }

        fn input_text(&self, cx: &mut gpui::TestAppContext, key: &str) -> String {
            self.panel().update(cx, |panel, cx| {
                panel
                    .fields
                    .iter()
                    .find_map(|field| match field {
                        ConfigField::Input {
                            key: field_key,
                            input,
                            ..
                        } if field_key == key => Some(input.read(cx).content().to_owned()),
                        _ => None,
                    })
                    .expect("the design size is a typed field")
            })
        }

        fn submit(&self, cx: &mut gpui::TestAppContext, key: &str) {
            let input = self.panel().read_with(cx, |panel, _| {
                panel
                    .fields
                    .iter()
                    .find_map(|field| match field {
                        ConfigField::Input {
                            key: field_key,
                            input,
                            ..
                        } if field_key == key => Some(input.clone()),
                        _ => None,
                    })
                    .expect("the design size is a typed field")
            });
            input.update(cx, |input, cx| {
                let typed = SharedString::from(input.content().to_owned());
                cx.emit(forge_components::InputEvent::Submitted(typed));
            });
            cx.run_until_parked();
        }
    }

    #[gpui::test]
    fn a_design_width_typed_past_the_page_bound_is_saved_at_the_bound(
        cx: &mut gpui::TestAppContext,
    ) {
        let fixture = Fixture::with_source_box(cx, OverlayConfig::new());
        fixture.type_into(cx, DESIGN_WIDTH, "99999");

        fixture.submit(cx, DESIGN_WIDTH);

        assert_eq!(
            fixture
                .saves(cx)
                .last()
                .and_then(|config| int_at(config, DESIGN_WIDTH)),
            Some(DESIGN_SIZE_MAX_PX)
        );
    }

    #[gpui::test]
    fn a_design_height_typed_under_the_page_bound_shows_the_bound_it_was_saved_at(
        cx: &mut gpui::TestAppContext,
    ) {
        let fixture = Fixture::with_source_box(cx, OverlayConfig::new());
        fixture.type_into(cx, DESIGN_HEIGHT, "3");

        fixture.submit(cx, DESIGN_HEIGHT);

        assert_eq!(
            fixture.input_text(cx, DESIGN_HEIGHT),
            DESIGN_SIZE_MIN_PX.to_string(),
            "the field kept showing a size the overlay never received"
        );
    }

    #[gpui::test]
    fn a_cleared_design_width_snaps_back_to_the_size_it_held_and_saves_nothing(
        cx: &mut gpui::TestAppContext,
    ) {
        let fixture =
            Fixture::with_source_box(cx, config(&[(DESIGN_WIDTH, Variant::Int(HELD_WIDTH))]));
        fixture.type_into(cx, DESIGN_WIDTH, "");

        fixture.submit(cx, DESIGN_WIDTH);

        assert_eq!(fixture.input_text(cx, DESIGN_WIDTH), HELD_WIDTH.to_string());
        assert!(fixture.saves(cx).is_empty());
    }

    #[gpui::test]
    fn a_margin_stored_past_its_bound_is_saved_back_inside_it(cx: &mut gpui::TestAppContext) {
        let fixture = Fixture::with_source_box(
            cx,
            config(&[
                (MARGIN_TOP, Variant::Int(99)),
                (MARGIN_LEFT, Variant::Int(-5)),
            ]),
        );

        fixture.commit(cx);

        let saved = fixture.saves(cx).last().cloned().unwrap_or_default();
        assert_eq!(
            [int_at(&saved, MARGIN_TOP), int_at(&saved, MARGIN_LEFT)],
            [Some(MARGIN_MAX_PERCENT), None],
            "a margin outside 0-45 must settle at the bound; 0 is the default and stays sparse"
        );
    }

    struct Heard {
        saves: Vec<OverlayConfig>,
        looks: Vec<(String, OverlayConfig)>,
        _subs: [Subscription; 2],
    }

    struct Shown<'a> {
        heard: Entity<Heard>,
        vcx: &'a mut gpui::VisualTestContext,
        _rt: tokio::runtime::Runtime,
    }

    fn show(cx: &mut gpui::TestAppContext, stored: OverlayConfig) -> Shown<'_> {
        cx.update(forge_components::bind_picker_keys);
        let (launch, _repo, rt) = launch(
            cx,
            stored,
            source_specs(),
            FrameOverlayKind.default_config(),
            HashMap::new(),
        );
        let (panel, vcx) = cx.add_window_view(|_window, cx| OverlayPropertyPanel::new(launch, cx));
        vcx.simulate_resize(gpui::size(PANE_W, px(PANEL_H)));
        let heard = vcx.update(|_window, cx| {
            panel.update(cx, |panel, cx| {
                panel.adopt_base(
                    BaseLaunch {
                        look: look_summary(&FrameOverlayKind),
                        looks: vec![
                            look_summary(&GoalOverlayKind),
                            look_summary(&FrameOverlayKind),
                        ],
                        receiver: false,
                    },
                    cx,
                );
            });
            cx.new(|cx| Heard {
                saves: Vec::new(),
                looks: Vec::new(),
                _subs: [
                    cx.subscribe(&panel, |this: &mut Heard, _panel, event, _cx| {
                        let PropertyPanelEvent::Save(config) = event;
                        this.saves.push(config.clone());
                    }),
                    cx.subscribe(&panel, |this: &mut Heard, _panel, event, _cx| {
                        if let BaseEvent::ChangeLook { kind_id, config } = event {
                            this.looks.push((kind_id.clone(), config.clone()));
                        }
                    }),
                ],
            })
        });
        vcx.run_until_parked();
        Shown {
            heard,
            vcx,
            _rt: rt,
        }
    }

    impl Shown<'_> {
        fn click_first_until(
            &mut self,
            reached: impl Fn(&Window, &App, &Heard) -> bool,
        ) -> Option<gpui::Point<Pixels>> {
            let mut y = 0.0;
            while y < PANEL_H {
                let mut x = 0.0;
                while x < f32::from(PANE_W) {
                    let at = gpui::point(px(x), px(y));
                    self.vcx.simulate_click(at, gpui::Modifiers::none());
                    if self.reached(&reached) {
                        return Some(at);
                    }
                    x += SCAN_STEP;
                }
                y += SCAN_STEP;
            }
            None
        }

        fn reached(&mut self, test: &impl Fn(&Window, &App, &Heard) -> bool) -> bool {
            let heard = self.heard.clone();
            self.vcx
                .update(|window, cx| test(window, cx, heard.read(cx)))
        }

        fn saves(&mut self) -> Vec<OverlayConfig> {
            let heard = self.heard.clone();
            self.vcx.update(|_window, cx| heard.read(cx).saves.clone())
        }

        fn looks(&mut self) -> Vec<(String, OverlayConfig)> {
            let heard = self.heard.clone();
            self.vcx.update(|_window, cx| heard.read(cx).looks.clone())
        }
    }

    fn picker_focused(window: &Window, cx: &App, _: &Heard) -> bool {
        window.focused(cx).is_some()
    }

    fn predating(entries: &[(&str, Variant)]) -> OverlayConfig {
        let mut stored = config(entries);
        stored.insert(MIGRATION_ACKNOWLEDGED.to_owned(), Variant::Bool(false));
        stored
    }

    #[gpui::test]
    fn got_it_on_the_sizing_notice_saves_the_acknowledgement(cx: &mut gpui::TestAppContext) {
        let mut shown = show(cx, predating(&[]));

        shown.click_first_until(|_, _, heard| !heard.saves.is_empty());

        assert_eq!(
            shown.saves(),
            vec![config(&[(MIGRATION_ACKNOWLEDGED, Variant::Bool(true))])]
        );
    }

    #[gpui::test]
    fn the_sizing_notice_is_gone_once_acknowledged(cx: &mut gpui::TestAppContext) {
        let mut shown = show(cx, predating(&[]));
        let acknowledged_at = shown
            .click_first_until(|_, _, heard| !heard.saves.is_empty())
            .expect("the notice offers a button that acknowledges it");

        shown
            .vcx
            .simulate_click(acknowledged_at, gpui::Modifiers::none());

        assert_eq!(
            shown.saves().len(),
            1,
            "the acknowledged notice still answered a click where its button was"
        );
    }

    #[gpui::test]
    fn a_look_picked_from_the_keyboard_carries_the_source_box_into_the_change(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut shown = show(
            cx,
            config(&[
                (DESIGN_WIDTH, Variant::Int(HELD_WIDTH)),
                (MARGIN_TOP, Variant::Int(30)),
            ]),
        );
        shown.click_first_until(picker_focused);

        shown.vcx.simulate_keystrokes("down enter");

        let defaults = FrameOverlayKind.default_design_size();
        let looks = shown.looks();
        let carried: Vec<_> = looks
            .iter()
            .map(|(kind, config)| {
                (
                    kind.as_str(),
                    [
                        DESIGN_WIDTH,
                        DESIGN_HEIGHT,
                        MARGIN_TOP,
                        MARGIN_RIGHT,
                        MARGIN_BOTTOM,
                        MARGIN_LEFT,
                    ]
                    .map(|key| int_at(config, key)),
                )
            })
            .collect();
        assert_eq!(
            carried,
            vec![(
                GoalOverlayKind.id(),
                [
                    Some(HELD_WIDTH),
                    Some(i64::from(defaults.height)),
                    Some(30),
                    Some(0),
                    Some(0),
                    Some(0)
                ]
            )]
        );
    }

    #[gpui::test]
    fn a_look_row_clicked_in_the_open_picker_changes_the_look(cx: &mut gpui::TestAppContext) {
        let mut shown = show(cx, OverlayConfig::new());
        let trigger = shown
            .click_first_until(picker_focused)
            .expect("the look field opens the picker");

        let mut y = f32::from(trigger.y);
        while shown.looks().is_empty() && y < PANEL_H {
            if !shown.reached(&picker_focused) {
                shown.vcx.simulate_click(trigger, gpui::Modifiers::none());
            }
            y += SCAN_STEP;
            shown
                .vcx
                .simulate_click(gpui::point(PANE_W / 2.0, px(y)), gpui::Modifiers::none());
        }

        assert_eq!(
            shown
                .looks()
                .into_iter()
                .map(|(kind, _)| kind)
                .collect::<Vec<_>>(),
            vec![GoalOverlayKind.id().to_owned()]
        );
    }
}
