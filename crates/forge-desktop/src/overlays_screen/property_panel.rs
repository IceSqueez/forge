use std::collections::HashMap;
use std::time::Duration;

use forge_components::{
    BORDER_THIN, FONT_XXS, ForgePalette, Picker, PickerEvent, PickerItem, PickerLabels, TextInput,
    anchored_popover, body_family, field_label, mono_family, section_label, tr,
};
use forge_overlay::config::{ACCENT, ANIMATION, DURATION, EVENT, FONT, POSITION, SOUND};
use forge_overlay::sample_payload;
use forge_registry::FormField;
use forge_storage::{OverlayConfig, OverlayId};
use forge_types::Variant;
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, Pixels, Point, SharedString,
    Subscription, Window, div, prelude::*, px,
};

use crate::config_form::{
    ChoiceSupport, ConfigField, ConfigFieldHandlers, FoldContext, collect_field_values,
    fold_config_field, render_config_control, sparse_overrides,
};
use crate::presentation::ActivePresentation;
use crate::toasts::copy_to_clipboard;

const PANE_W: Pixels = px(246.0);
const PANE_PAD: Pixels = px(14.0);

const SECTION_GAP: Pixels = px(10.0);
const SECTION_TOP_GAP: Pixels = px(16.0);
const FIELD_GAP: Pixels = px(12.0);

const CHIP_GAP: Pixels = px(5.0);
const CHIP_PAD_V: Pixels = px(2.0);
const CHIP_PAD_H: Pixels = px(6.0);
const CHIP_RADIUS: Pixels = px(4.0);
const CHIP_FS: Pixels = px(10.0);

const NOTICE_PAD: Pixels = px(9.0);
const NOTICE_RADIUS: Pixels = px(6.0);
const NOTICE_LINE_H: Pixels = px(15.0);

/// A slider fires while the pointer moves; a save per step would rewrite the page and reload the
/// browser source dozens of times per drag.
const SLIDE_SETTLE: Duration = Duration::from_millis(400);

#[derive(Clone, Copy, PartialEq, Eq)]
enum PanelSection {
    Content,
    Style,
    Behavior,
}

impl PanelSection {
    const ORDER: [Self; 3] = [Self::Content, Self::Style, Self::Behavior];

    fn label(self) -> String {
        match self {
            Self::Content => tr!("overlays_panel_section_content"),
            Self::Style => tr!("overlays_panel_section_style"),
            Self::Behavior => tr!("overlays_panel_section_behavior"),
        }
    }
}

/// Keys outside the shared overlay vocabulary belong to a kind that names its own properties, so
/// they land in the section that describes what an overlay says rather than being dropped.
fn section_of(key: &str) -> PanelSection {
    match key {
        ACCENT | FONT | POSITION => PanelSection::Style,
        ANIMATION | DURATION | SOUND => PanelSection::Behavior,
        _ => PanelSection::Content,
    }
}

pub(super) enum PropertyPanelEvent {
    Save(OverlayConfig),
}

pub(super) struct PanelLaunch {
    pub(super) overlay_id: OverlayId,
    pub(super) specs: Vec<FormField>,
    pub(super) defaults: OverlayConfig,
    pub(super) stored: OverlayConfig,
    pub(super) effective: OverlayConfig,
    pub(super) choices: HashMap<String, Vec<(String, String)>>,
    pub(super) overridden_files: Vec<String>,
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
    fields: Vec<ConfigField>,
    overridden_files: Vec<String>,
    picker: Option<ChoicePicker>,
    settle_epoch: u64,
}

impl EventEmitter<PropertyPanelEvent> for OverlayPropertyPanel {}

impl OverlayPropertyPanel {
    pub(super) fn new(launch: PanelLaunch, cx: &mut Context<Self>) -> Self {
        let palette = cx.palette();
        let fold = FoldContext {
            config: &launch.effective,
            palette: &palette,
            choices: ChoiceSupport::Picker(&launch.choices),
            on_committed: Self::on_field_committed,
        };
        let mut fields: Vec<ConfigField> = Vec::new();
        for spec in &launch.specs {
            fold_config_field(spec, None, &fold, &mut fields, cx);
        }

        Self {
            overlay_id: launch.overlay_id,
            defaults: launch.defaults,
            stored: launch.stored,
            labels: field_labels(&launch.specs),
            fields,
            overridden_files: launch.overridden_files,
            picker: None,
            settle_epoch: 0,
        }
    }

    pub(super) fn overlay_id(&self) -> &OverlayId {
        &self.overlay_id
    }

    /// The kind's own event field decides which sample payload the token row offers.
    fn bound_event(&self, cx: &Context<Self>) -> String {
        let mut buffer = self.defaults.clone();
        collect_field_values(&self.fields, &mut buffer, cx);
        buffer
            .get(EVENT)
            .and_then(Variant::as_str)
            .unwrap_or_default()
            .to_owned()
    }

    /// Only top-level scalar keys: the page's expander reads own properties of the payload, so a
    /// nested entity or a null would render a placeholder instead of a value.
    fn template_tokens(&self, cx: &Context<Self>) -> Vec<String> {
        let event_kind = self.bound_event(cx);
        if event_kind.is_empty() {
            return Vec::new();
        }
        let Some(fields) = sample_payload(&event_kind).as_object().cloned() else {
            return Vec::new();
        };
        fields
            .into_iter()
            .filter(|(_, value)| value.is_string() || value.is_number() || value.is_boolean())
            .map(|(name, _)| name)
            .collect()
    }

    fn emit_save(&mut self, cx: &mut Context<Self>) {
        let mut buffer = self.defaults.clone();
        for (key, value) in &self.stored {
            buffer.insert(key.clone(), value.clone());
        }
        collect_field_values(&self.fields, &mut buffer, cx);

        let sparse = sparse_overrides(&self.defaults, &buffer);
        self.stored = sparse.clone();
        cx.emit(PropertyPanelEvent::Save(sparse));
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
        if let forge_components::InputEvent::Submitted(_) = event {
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
        section: PanelSection,
        first: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let members: Vec<&ConfigField> = self
            .fields
            .iter()
            .filter(|field| section_of(field.key()) == section)
            .collect();
        if members.is_empty() {
            return None;
        }

        let mut column = div()
            .flex()
            .flex_col()
            .when(!first, |col| col.pt(SECTION_TOP_GAP))
            .child(
                div()
                    .pb(SECTION_GAP)
                    .child(section_label(section.label().to_uppercase(), palette)),
            );

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

        if section == PanelSection::Content {
            column = column.child(self.render_tokens(palette, cx));
        }

        Some(column.into_any_element())
    }

    fn render_tokens(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let tokens = self.template_tokens(cx);
        let body: AnyElement = if tokens.is_empty() {
            div()
                .font_family(body_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(tr!("overlays_panel_tokens_none"))
                .into_any_element()
        } else {
            let mut row = div().flex().flex_row().flex_wrap().gap(CHIP_GAP);
            for name in tokens {
                let token = format!("%{name}%");
                let copied = token.clone();
                row = row.child(
                    div()
                        .id(SharedString::from(format!("overlays-token-{name}")))
                        .py(CHIP_PAD_V)
                        .px(CHIP_PAD_H)
                        .rounded(CHIP_RADIUS)
                        .border(BORDER_THIN)
                        .border_color(palette.border_regular)
                        .bg(palette.base)
                        .cursor_pointer()
                        .font_family(mono_family())
                        .text_size(CHIP_FS)
                        .text_color(palette.warning)
                        .on_click(move |_: &ClickEvent, _window, cx| {
                            copy_to_clipboard(copied.clone(), cx);
                        })
                        .child(token),
                );
            }
            row.into_any_element()
        };

        div()
            .pb(FIELD_GAP)
            .flex()
            .flex_col()
            .gap(CHIP_GAP)
            .child(section_label(
                tr!("overlays_panel_tokens_label").to_uppercase(),
                palette,
            ))
            .child(body)
            .into_any_element()
    }

    fn render_override_notice(&self, palette: &ForgePalette) -> Option<AnyElement> {
        if self.overridden_files.is_empty() {
            return None;
        }
        let files = self.overridden_files.join(", ");
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
                    files = files.as_str()
                ))
                .into_any_element(),
        )
    }
}

fn field_labels(specs: &[FormField]) -> HashMap<String, String> {
    let mut labels = HashMap::new();
    for spec in specs {
        collect_labels(spec, &mut labels);
    }
    labels
}

fn collect_labels(spec: &FormField, out: &mut HashMap<String, String>) {
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
        | FormField::Swatch { key, label, .. }
        | FormField::SubChain { key, label }
        | FormField::CaseList { key, label } => {
            out.insert((*key).to_owned(), (*label).to_owned());
        }
        FormField::Optional { key, label, inner } => {
            out.insert((*key).to_owned(), (*label).to_owned());
            collect_labels(inner, out);
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

        body = body.children(self.render_override_notice(&palette));

        let mut rendered = 0usize;
        for section in PanelSection::ORDER {
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
