use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use forge_components::{
    BORDER_THIN, FONT_XXS, ForgePalette, Picker, PickerEvent, PickerItem, PickerLabels, TextInput,
    anchored_popover, body_family, field_label, section_label, tr,
};
use forge_overlay::config::RETIRED_KEYS;
use forge_overlay::{ConfigSection, SectionedField};
use forge_registry::FormField;
use forge_runtime::OverlayServiceHandle;
use forge_storage::{OverlayConfig, OverlayId, OverlayRepo};
use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, Pixels, Point, SharedString, Subscription,
    Window, div, prelude::*, px,
};

use super::store_config;
use crate::async_bridge;
use crate::config_form::{
    ChoiceSupport, ConfigField, ConfigFieldHandlers, FoldContext, collect_field_values,
    fold_config_field, render_config_control, resolve_dependent_choices, sparse_overrides,
};
use crate::presentation::ActivePresentation;

const PANE_W: Pixels = px(246.0);
const PANE_PAD: Pixels = px(14.0);

const SECTION_GAP: Pixels = px(10.0);
const SECTION_TOP_GAP: Pixels = px(16.0);
const FIELD_GAP: Pixels = px(12.0);

const NOTICE_PAD: Pixels = px(9.0);
const NOTICE_RADIUS: Pixels = px(6.0);
const NOTICE_LINE_H: Pixels = px(15.0);

/// A slider fires while the pointer moves; a save per step would rewrite the page and reload the
/// browser source dozens of times per drag.
const SLIDE_SETTLE: Duration = Duration::from_millis(400);

const RELEASE_PERSIST_CONTEXT: &str = "overlay property panel teardown";

const SECTION_ORDER: [ConfigSection; 3] = [
    ConfigSection::Content,
    ConfigSection::Style,
    ConfigSection::Behavior,
];

fn section_heading(section: ConfigSection) -> String {
    match section {
        ConfigSection::Content => tr!("overlays_panel_section_content"),
        ConfigSection::Style => tr!("overlays_panel_section_style"),
        ConfigSection::Behavior => tr!("overlays_panel_section_behavior"),
    }
}

pub(super) enum PropertyPanelEvent {
    Save(OverlayConfig),
}

pub(super) struct PanelLaunch {
    pub(super) overlay_id: OverlayId,
    pub(super) specs: Vec<SectionedField>,
    pub(super) defaults: OverlayConfig,
    pub(super) stored: OverlayConfig,
    pub(super) effective: OverlayConfig,
    pub(super) choices: HashMap<String, Vec<(String, String)>>,
    pub(super) overridden_files: Vec<String>,
    pub(super) repo: Arc<dyn OverlayRepo>,
    pub(super) service: OverlayServiceHandle,
    pub(super) rt_handle: tokio::runtime::Handle,
}

struct ChoicePicker {
    key: String,
    picker: Entity<Picker>,
    position: Point<Pixels>,
    _sub: Subscription,
}

pub(super) struct OverlayPropertyPanel {
    overlay_id: OverlayId,
    defaults: OverlayConfig,
    stored: OverlayConfig,
    labels: HashMap<String, String>,
    sections: HashMap<String, ConfigSection>,
    fields: Vec<ConfigField>,
    choices: HashMap<String, Vec<(String, String)>>,
    overridden_files: Vec<String>,
    picker: Option<ChoicePicker>,
    settle_epoch: u64,
    repo: Arc<dyn OverlayRepo>,
    service: OverlayServiceHandle,
    rt_handle: tokio::runtime::Handle,
    _release: Subscription,
}

impl EventEmitter<PropertyPanelEvent> for OverlayPropertyPanel {}

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

        Self {
            overlay_id: launch.overlay_id,
            defaults: launch.defaults,
            stored: launch.stored,
            labels: index.labels,
            sections: index.sections,
            fields,
            choices: launch.choices,
            overridden_files: launch.overridden_files,
            picker: None,
            settle_epoch: 0,
            repo: launch.repo,
            service: launch.service,
            rt_handle: launch.rt_handle,
            _release: release,
        }
    }

    pub(super) fn overlay_id(&self) -> &OverlayId {
        &self.overlay_id
    }

    /// Keys a retired build wrote are dropped instead of being carried forward, so a save is the
    /// moment a record stops mentioning them.
    fn pending_config(&self, cx: &App) -> OverlayConfig {
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

    /// Regenerating the files reloads every connected page, so a commit that changes nothing -
    /// a blur on an untouched field, a swatch clicked twice - never reaches the record.
    fn emit_save(&mut self, cx: &mut Context<Self>) {
        let sparse = self.pending_config(cx);
        if sparse == self.stored {
            return;
        }
        self.stored = sparse.clone();
        cx.emit(PropertyPanelEvent::Save(sparse));
    }

    /// The panel can go away in the same tick as the click that unfocused a field, so the value a
    /// blur would have committed is written straight through the repo instead of through the screen.
    fn persist_on_release(&mut self, cx: &mut App) {
        let sparse = self.pending_config(cx);
        if sparse == self.stored {
            return;
        }
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

    /// A later move supersedes an earlier one, so only the value the pointer came to rest on saves.
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

    fn on_field_committed(
        &mut self,
        _field: Entity<TextInput>,
        event: &forge_components::InputEvent,
        cx: &mut Context<Self>,
    ) {
        if matches!(
            event,
            forge_components::InputEvent::Submitted(_) | forge_components::InputEvent::Blurred(_)
        ) {
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

    fn open_choice(
        &mut self,
        key: String,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.picker.as_ref().is_some_and(|open| open.key == key) {
            self.close_choice(cx);
            return;
        }
        let Some(ConfigField::Choice { options, .. }) = self
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
                icon: forge_components::Icon::Circle,
            })
            .collect();
        let labels = PickerLabels {
            title: self.label_of(&key).into(),
            placeholder: tr!("widget_picker_search_placeholder").into(),
            empty: tr!("overlays_panel_choice_empty").into(),
            loading: tr!("widget_picker_loading").into(),
            cancel: tr!("common_cancel").into(),
        };
        let palette = cx.palette();
        let picker = cx.new(|cx| Picker::new(labels, items, palette, cx));
        let sub = cx.subscribe(&picker, Self::on_picker_event);
        picker.update(cx, |picker, cx| picker.focus(window, cx));
        self.picker = Some(ChoicePicker {
            key,
            picker,
            position,
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
        self.picker = None;
        self.emit_save(cx);
        cx.notify();
    }

    fn close_choice(&mut self, cx: &mut Context<Self>) {
        self.picker = None;
        cx.notify();
    }

    fn label_of(&self, key: &str) -> String {
        self.labels
            .get(key)
            .cloned()
            .unwrap_or_else(|| key.to_owned())
    }

    /// A control the fold produced for a key the kind never declared still gets a home rather than
    /// disappearing from the panel.
    fn section_of(&self, key: &str) -> ConfigSection {
        self.sections
            .get(key)
            .copied()
            .unwrap_or(ConfigSection::Content)
    }

    fn handlers() -> ConfigFieldHandlers<Self> {
        ConfigFieldHandlers {
            toggle: Self::toggle_field,
            slide: Self::slide_field,
            pick: Self::pick_swatch,
            open_choice: Some(Self::open_choice),
        }
    }

    fn render_section(
        &self,
        section: ConfigSection,
        first: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let members: Vec<&ConfigField> = self
            .fields
            .iter()
            .filter(|field| self.section_of(field.key()) == section)
            .collect();
        if members.is_empty() {
            return None;
        }

        let mut column = div()
            .flex()
            .flex_col()
            .when(!first, |col| col.pt(SECTION_TOP_GAP))
            .child(div().pb(SECTION_GAP).child(section_label(
                section_heading(section).to_uppercase(),
                palette,
            )));

        let view = cx.entity();
        let handlers = Self::handlers();
        for field in members {
            let control = render_config_control(field, palette, "overlays-panel", &view, &handlers);
            column = column.child(
                div().pb(FIELD_GAP).child(
                    field_label(palette, self.label_of(field.key()).to_uppercase(), control)
                        .tone(palette.text_faint),
                ),
            );
        }

        Some(column.into_any_element())
    }
}

/// Both editor modes read the record's own override list, so the panel never claims a file is
/// generated while the other mode calls it the user's.
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
    sections: HashMap<String, ConfigSection>,
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

/// A wrapped field shares the section of the field that wraps it, so a gate and the control it
/// gates never drift into separate headings.
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
            out.sections.insert((*key).to_owned(), section);
        }
        FormField::Optional { key, label, inner } => {
            out.labels.insert((*key).to_owned(), (*label).to_owned());
            out.sections.insert((*key).to_owned(), section);
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

        body = body.children(override_notice(&self.overridden_files, &palette));

        let mut rendered = 0usize;
        for section in SECTION_ORDER {
            if let Some(block) = self.render_section(section, rendered == 0, &palette, cx) {
                rendered += 1;
                body = body.child(block);
            }
        }

        if rendered == 0 {
            body = body.child(
                div()
                    .italic()
                    .font_family(body_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(tr!("overlays_panel_no_properties")),
            );
        }

        let popover = self.picker.as_ref().map(|open| {
            let view = cx.entity();
            anchored_popover(open.position, open.picker.clone())
                .on_dismiss(move |_window, cx| {
                    view.update(cx, |this, cx| this.close_choice(cx));
                })
                .into_any_element()
        });

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
            .children(popover)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Mutex;

    use forge_components::{Density, ThemeId};
    use forge_overlay::OverlayKindRegistry;
    use forge_overlay::config::{ELEMENT_WIDTH, HEADLINE};
    use forge_registry::FormField;
    use forge_runtime::EventBus;
    use forge_storage::{OverlayCredential, OverlayDefinition, StorageError};
    use forge_types::Variant;
    use time::OffsetDateTime;

    use super::*;
    use crate::presentation::Presentation;
    use crate::test_support::{StubEventLog, pump, runtime, test_backend};

    const OVERLAY: &str = "alert-one";
    const KIND: &str = "overlay.alert";
    const TOKEN: &str = "token";
    const DEFAULT_HEADLINE: &str = "hello";
    const EDITED_HEADLINE: &str = "goodbye";
    const ACCENT: &str = "accent";
    const CANVAS_WIDTH: &str = "canvas_width";
    const CANVAS_HEIGHT: &str = "canvas_height";

    struct RecordingOverlays {
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

    fn defaults() -> OverlayConfig {
        config(&[
            (HEADLINE, Variant::String(DEFAULT_HEADLINE.into())),
            (ACCENT, Variant::String("mauve".into())),
        ])
    }

    fn specs() -> Vec<SectionedField> {
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
        ]
    }

    struct Fixture {
        panel: Option<Entity<OverlayPropertyPanel>>,
        saves: Entity<Saves>,
        repo: Arc<RecordingOverlays>,
        rt: tokio::runtime::Runtime,
    }

    impl Fixture {
        fn new(cx: &mut gpui::TestAppContext, stored: OverlayConfig) -> Self {
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
            let defaults = defaults();
            let mut effective = defaults.clone();
            for (key, value) in &stored {
                effective.insert(key.clone(), value.clone());
            }
            let launch = PanelLaunch {
                overlay_id: OverlayId::new(OVERLAY),
                specs: specs(),
                defaults,
                stored,
                effective,
                choices: HashMap::new(),
                overridden_files: Vec::new(),
                repo: Arc::clone(&repo) as Arc<dyn OverlayRepo>,
                service,
                rt_handle: rt.handle().clone(),
            };
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

        /// Why: gpui defers the release callback to the next app update, so dropping the handle
        /// has to be followed by one before the tokio work it spawns exists to be pumped.
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
}
