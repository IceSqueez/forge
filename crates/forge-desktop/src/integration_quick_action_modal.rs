use std::collections::BTreeMap;
use std::sync::Arc;

use forge_components::{
    BORDER_THIN, FONT_XS, FONT_XXS, ForgePalette, Icon, InputEvent, OverlayPosition, Picker,
    PickerEvent, PickerItem, PickerLabels, Radius, TextArea, TextInput, body_family,
    destructive_button_with_icon, field_label, icon, modal, mono_family, overlay,
    primary_button_with_icon, radius, secondary_button, spinner, toggle, tr,
};
use forge_obs::{ObsClient, ObsSource};
use forge_platform_core::{
    PickerKind, QuickAction, QuickActionChoiceOption, QuickActionChoiceSource, QuickActionField,
    QuickActionFieldKind, QuickActionFieldValue,
};
use forge_types::{SubActionStep, Variant};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, Rgba, SharedString, Subscription,
    Window, div, prelude::*, px,
};
use tokio::runtime::Handle;

use crate::async_bridge;
use crate::integration_quick_actions::accent_color;
use crate::presentation::ActivePresentation;

const FIELD_FONT: gpui::Pixels = px(13.0);
const ROW_PAD_X: gpui::Pixels = px(12.0);
const ROW_PAD_Y: gpui::Pixels = px(8.0);

pub enum QuickActionModalEvent {
    Run { step: SubActionStep, label: String },
    Cancel,
}

enum ChoiceState {
    Loading,
    Ready(Vec<PickerItem>),
    Failed(String),
}

struct ChoiceControl {
    dynamic: Option<PickerKind>,
    state: ChoiceState,
    selected: Option<SharedString>,
    scene: Option<String>,
}

enum FieldControl {
    Text(Entity<TextInput>),
    Multiline(Entity<TextArea>),
    Toggle(bool),
    Choice(ChoiceControl),
}

struct ModalField {
    key: String,
    label: String,
    hint: Option<String>,
    required: bool,
    control: FieldControl,
}

struct OpenChoice {
    picker: Entity<Picker>,
    _sub: Subscription,
}

pub struct QuickActionModal {
    label: String,
    glyph: Icon,
    accent: Rgba,
    destructive: bool,
    action: QuickAction,
    fields: Vec<ModalField>,
    obs_source: Option<Arc<ObsClient>>,
    rt_handle: Handle,
    open_choice: Option<OpenChoice>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<QuickActionModalEvent> for QuickActionModal {}

impl QuickActionModal {
    pub fn new(
        action: QuickAction,
        obs_source: Option<Arc<ObsClient>>,
        rt_handle: Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let accent = accent_color(action.accent, &palette);
        let glyph = Icon::from_name(action.icon.as_str());
        let label = action.label.clone();
        let destructive = action.destructive;

        let specs = build_specs(&action);
        let mut merge_action = action.clone();
        merge_action.fields = specs.iter().map(spec_to_field).collect();
        let mut fields = Vec::with_capacity(specs.len());
        let mut subs = Vec::new();
        let mut pending_dynamic: Vec<(usize, PickerKind)> = Vec::new();

        for (i, spec) in specs.into_iter().enumerate() {
            let control = match &spec.kind {
                QuickActionFieldKind::Text => {
                    let content = default_text(&spec.default);
                    let placeholder = spec.placeholder.clone().unwrap_or_default();
                    let blank = spec.required && content.trim().is_empty();
                    let input = cx.new(|cx| {
                        let mut ti = TextInput::new(placeholder, cx).with_palette(palette);
                        if !content.is_empty() {
                            ti.set_content(content, cx);
                        }
                        ti.set_invalid(blank, cx);
                        ti
                    });
                    subs.push(cx.subscribe(&input, move |this, _, event, cx| {
                        this.on_field_event(i, event, cx)
                    }));
                    FieldControl::Text(input)
                }
                QuickActionFieldKind::Multiline | QuickActionFieldKind::MultilineList => {
                    let content = default_text(&spec.default);
                    let placeholder = spec.placeholder.clone().unwrap_or_default();
                    let blank = spec.required && content.trim().is_empty();
                    let area = cx.new(|cx| {
                        let mut ta = TextArea::new(placeholder, cx)
                            .with_palette(palette)
                            .with_height(px(80.0));
                        if !content.is_empty() {
                            ta.set_content(content, cx);
                        }
                        ta.set_invalid(blank, cx);
                        ta
                    });
                    subs.push(cx.subscribe(&area, move |this, _, event, cx| {
                        this.on_field_event(i, event, cx)
                    }));
                    FieldControl::Multiline(area)
                }
                QuickActionFieldKind::Toggle => FieldControl::Toggle(default_toggle(&spec.default)),
                QuickActionFieldKind::Choice(source) => match source {
                    QuickActionChoiceSource::Static(opts) => {
                        let items = opts.iter().map(static_item).collect();
                        FieldControl::Choice(ChoiceControl {
                            dynamic: None,
                            state: ChoiceState::Ready(items),
                            selected: choice_default(&spec.default, opts),
                            scene: None,
                        })
                    }
                    QuickActionChoiceSource::Dynamic(pk) => {
                        if obs_source.is_some() {
                            pending_dynamic.push((i, *pk));
                            FieldControl::Choice(ChoiceControl {
                                dynamic: Some(*pk),
                                state: ChoiceState::Loading,
                                selected: None,
                                scene: None,
                            })
                        } else {
                            FieldControl::Choice(ChoiceControl {
                                dynamic: Some(*pk),
                                state: ChoiceState::Failed(tr!("integration_qa_field_unavailable")),
                                selected: None,
                                scene: None,
                            })
                        }
                    }
                },
            };
            fields.push(ModalField {
                key: spec.key,
                label: spec.label,
                hint: spec.hint,
                required: spec.required,
                control,
            });
        }

        for (i, pk) in pending_dynamic {
            if let Some(client) = &obs_source {
                let client = Arc::clone(client);
                async_bridge::run_async(
                    &rt_handle,
                    fetch_picker_items(client, pk, fetch_labels()),
                    move |this, result, cx| this.apply_dynamic(i, result, cx),
                    cx,
                );
            }
        }

        Self {
            label,
            glyph,
            accent,
            destructive,
            action: merge_action,
            fields,
            obs_source,
            rt_handle,
            open_choice: None,
            _subs: subs,
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        for field in &self.fields {
            match &field.control {
                FieldControl::Text(input) => {
                    input.update(cx, |f, cx| f.focus(window, cx));
                    return;
                }
                FieldControl::Multiline(area) => {
                    area.update(cx, |f, cx| f.focus(window, cx));
                    return;
                }
                _ => {}
            }
        }
    }

    fn on_field_event(&mut self, index: usize, event: &InputEvent, cx: &mut Context<Self>) {
        match event {
            InputEvent::Cancelled => self.cancel(cx),
            InputEvent::Changed(text) => {
                let blank = text.trim().is_empty();
                self.mark_blank(index, blank, cx);
                cx.notify();
            }
            InputEvent::Submitted(_) => {}
        }
    }

    fn mark_blank(&mut self, index: usize, blank: bool, cx: &mut Context<Self>) {
        let Some(field) = self.fields.get(index) else {
            return;
        };
        if !field.required {
            return;
        }
        match &field.control {
            FieldControl::Text(input) => {
                let input = input.clone();
                input.update(cx, |input, cx| input.set_invalid(blank, cx));
            }
            FieldControl::Multiline(area) => {
                let area = area.clone();
                area.update(cx, |area, cx| area.set_invalid(blank, cx));
            }
            _ => {}
        }
    }

    fn apply_dynamic(
        &mut self,
        index: usize,
        result: Result<PickerFetch, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(field) = self.fields.get_mut(index) else {
            return;
        };
        match result {
            Ok(fetch) => {
                field.hint = fetch
                    .hint_scene
                    .map(|scene| tr!("integration_qa_scene_current_hint", scene = scene));
                let FieldControl::Choice(choice) = &mut field.control else {
                    return;
                };
                choice.selected = fetch
                    .preselect
                    .filter(|id| fetch.items.iter().any(|item| &item.id == id))
                    .or_else(|| fetch.items.first().map(|item| item.id.clone()));
                choice.scene = fetch.scene;
                choice.state = ChoiceState::Ready(fetch.items);
            }
            Err(reason) => {
                field.hint = None;
                let FieldControl::Choice(choice) = &mut field.control else {
                    return;
                };
                choice.selected = None;
                choice.state = ChoiceState::Failed(reason);
            }
        }
        cx.notify();
    }

    fn retry_dynamic(&mut self, index: usize, cx: &mut Context<Self>) {
        let pk = match self.fields.get(index).map(|f| &f.control) {
            Some(FieldControl::Choice(choice)) => choice.dynamic,
            _ => None,
        };
        let Some(pk) = pk else {
            return;
        };
        let Some(client) = self.obs_source.clone() else {
            if let Some(FieldControl::Choice(choice)) =
                self.fields.get_mut(index).map(|f| &mut f.control)
            {
                choice.state = ChoiceState::Failed(tr!("integration_qa_field_unavailable"));
            }
            cx.notify();
            return;
        };
        if let Some(FieldControl::Choice(choice)) =
            self.fields.get_mut(index).map(|f| &mut f.control)
        {
            choice.selected = None;
            choice.state = ChoiceState::Loading;
        }
        cx.notify();
        async_bridge::run_async(
            &self.rt_handle,
            fetch_picker_items(client, pk, fetch_labels()),
            move |this, result, cx| this.apply_dynamic(index, result, cx),
            cx,
        );
    }

    fn toggle_field(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(FieldControl::Toggle(value)) =
            self.fields.get_mut(index).map(|f| &mut f.control)
        {
            *value = !*value;
            cx.notify();
        }
    }

    fn open_choice(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let (items, title) = match self.fields.get(index) {
            Some(field) => match &field.control {
                FieldControl::Choice(choice) => match &choice.state {
                    ChoiceState::Ready(items) => (items.clone(), choice_title(field)),
                    _ => return,
                },
                _ => return,
            },
            None => return,
        };
        let palette = cx.palette();
        let labels = PickerLabels {
            title: title.into(),
            placeholder: tr!("widget_picker_search_placeholder").into(),
            empty: tr!("widget_picker_no_results").into(),
            loading: tr!("widget_picker_loading").into(),
            cancel: tr!("common_cancel").into(),
        };
        let picker = cx.new(|cx| Picker::new(labels, items, palette, cx));
        let sub = cx.subscribe(&picker, move |this, _picker, event: &PickerEvent, cx| {
            this.on_choice_picker(index, event, cx);
        });
        picker.update(cx, |f, cx| f.focus(window, cx));
        self.open_choice = Some(OpenChoice { picker, _sub: sub });
        cx.notify();
    }

    fn on_choice_picker(&mut self, index: usize, event: &PickerEvent, cx: &mut Context<Self>) {
        match event {
            PickerEvent::Selected(id) => {
                if let Some(FieldControl::Choice(choice)) =
                    self.fields.get_mut(index).map(|f| &mut f.control)
                {
                    choice.selected = Some(id.clone());
                }
                self.open_choice = None;
                cx.notify();
            }
            PickerEvent::Cancelled => self.close_choice(cx),
        }
    }

    fn close_choice(&mut self, cx: &mut Context<Self>) {
        self.open_choice = None;
        cx.notify();
    }

    fn can_run(&self, cx: &Context<Self>) -> bool {
        self.fields.iter().all(|field| match &field.control {
            FieldControl::Choice(choice) => choice.selected.is_some(),
            FieldControl::Text(input) => {
                !field.required || !input.read(cx).content().trim().is_empty()
            }
            FieldControl::Multiline(area) => {
                !field.required || !area.read(cx).content().trim().is_empty()
            }
            FieldControl::Toggle(_) => true,
        })
    }

    fn build_step(&self, cx: &Context<Self>) -> SubActionStep {
        let values: BTreeMap<String, QuickActionFieldValue> = self
            .fields
            .iter()
            .map(|field| (field.key.clone(), field.current_value(cx)))
            .collect();
        let mut step = self.action.merge_config(&values);
        for field in &self.fields {
            if let FieldControl::Choice(choice) = &field.control
                && matches!(choice.dynamic, Some(PickerKind::Source))
                && let Some(scene) = &choice.scene
            {
                step.config
                    .insert("scene".to_owned(), Variant::String(scene.clone()));
            }
        }
        step
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        if !self.can_run(cx) {
            return;
        }
        let step = self.build_step(cx);
        cx.emit(QuickActionModalEvent::Run {
            step,
            label: self.label.clone(),
        });
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(QuickActionModalEvent::Cancel);
    }

    fn render_field(
        &self,
        index: usize,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(field) = self.fields.get(index) else {
            return div().into_any_element();
        };
        let control = match &field.control {
            FieldControl::Text(input) => input.clone().into_any_element(),
            FieldControl::Multiline(area) => area.clone().into_any_element(),
            FieldControl::Toggle(value) => self.render_toggle(index, *value, palette, cx),
            FieldControl::Choice(choice) => self.render_choice(index, choice, palette, cx),
        };
        let mut column = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(field_label(palette, field.label.to_uppercase(), control));
        if let Some(hint) = &field.hint {
            column = column.child(
                div()
                    .font_family(body_family())
                    .text_size(px(11.0))
                    .text_color(palette.text_faint)
                    .child(hint.clone()),
            );
        }
        column.into_any_element()
    }

    fn render_toggle(
        &self,
        index: usize,
        value: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (state_label, state_color) = if value {
            (tr!("integration_qa_toggle_on"), palette.success)
        } else {
            (tr!("integration_qa_toggle_off"), palette.random)
        };
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .px(ROW_PAD_X)
            .py(ROW_PAD_Y)
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_input)
            .bg(palette.shell)
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FIELD_FONT)
                    .text_color(state_color)
                    .child(state_label),
            )
            .child(toggle(value, palette).on_color(palette.success).on_click(
                ("qa-toggle", index),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_field(index, cx)),
            ))
            .into_any_element()
    }

    fn render_choice(
        &self,
        index: usize,
        choice: &ChoiceControl,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match &choice.state {
            ChoiceState::Loading => div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(ROW_PAD_X)
                .py(ROW_PAD_Y)
                .rounded(radius(Radius::Sm))
                .border(BORDER_THIN)
                .border_color(palette.border_input)
                .bg(palette.shell)
                .child(spinner(
                    ("qa-choice-spin", index),
                    Icon::Refresh,
                    FONT_XS,
                    palette.text_muted,
                ))
                .child(
                    div()
                        .font_family(body_family())
                        .text_size(FIELD_FONT)
                        .text_color(palette.text_muted)
                        .child(tr!("integration_qa_field_loading")),
                )
                .into_any_element(),
            ChoiceState::Failed(reason) => div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(icon(Icon::AlertCircle, FONT_XS, palette.random))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .font_family(body_family())
                                .text_size(px(11.5))
                                .text_color(palette.random)
                                .child(reason.clone()),
                        ),
                )
                .child(
                    div()
                        .id(("qa-retry", index))
                        .flex()
                        .items_center()
                        .gap(px(5.0))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.retry_dynamic(index, cx)
                        }))
                        .child(icon(Icon::Refresh, FONT_XXS, palette.info))
                        .child(
                            div()
                                .font_family(body_family())
                                .text_size(px(11.5))
                                .text_color(palette.info)
                                .child(tr!("integration_qa_field_retry")),
                        ),
                )
                .into_any_element(),
            ChoiceState::Ready(items) => {
                let selected = selected_label(items, &choice.selected);
                let has_selection = choice.selected.is_some();
                let text_color = if has_selection {
                    palette.text_primary
                } else {
                    palette.text_muted
                };
                let border_active = palette.border_active;
                div()
                    .id(("qa-choice", index))
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .px(ROW_PAD_X)
                    .py(ROW_PAD_Y)
                    .rounded(radius(Radius::Sm))
                    .border(BORDER_THIN)
                    .border_color(palette.border_input)
                    .bg(palette.shell)
                    .cursor_pointer()
                    .hover(move |s| s.border_color(border_active))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.open_choice(index, window, cx)
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .font_family(body_family())
                            .text_size(FIELD_FONT)
                            .text_color(text_color)
                            .child(selected),
                    )
                    .child(icon(Icon::ChevronDown, FIELD_FONT, palette.text_faint))
                    .into_any_element()
            }
        }
    }
}

impl Render for QuickActionModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();

        let mut body = div().w_full().flex().flex_col().gap(px(12.0));
        if self.fields.is_empty() {
            body = body.child(
                div()
                    .font_family(body_family())
                    .text_size(px(12.5))
                    .text_color(palette.text_muted)
                    .child(tr!(
                        "integration_qa_modal_confirm_body",
                        label = self.label.clone()
                    )),
            );
        } else {
            for index in 0..self.fields.len() {
                body = body.child(self.render_field(index, &palette, cx));
            }
        }

        let hint = if self.destructive {
            tr!("integration_qa_modal_footer_destructive")
        } else {
            tr!("integration_qa_modal_footer_immediate")
        };
        let run_button = if self.destructive {
            destructive_button_with_icon(
                Icon::AlertTriangle,
                tr!("integration_qa_modal_confirm"),
                &palette,
            )
        } else {
            primary_button_with_icon(
                Icon::PlayerPlayFilled,
                tr!("integration_qa_modal_run"),
                &palette,
            )
        }
        .disabled(!self.can_run(cx))
        .on_click(
            "qa-modal-run",
            cx.listener(|this, _: &ClickEvent, _, cx| this.run(cx)),
        );
        let cancel_button = secondary_button(tr!("common_cancel"), &palette).on_click(
            "qa-modal-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
        );
        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(px(11.0))
                    .text_color(palette.text_faint)
                    .child(hint),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(cancel_button)
                    .child(run_button),
            );

        let card = modal(self.label.clone(), body, &palette)
            .header_icon(self.glyph, self.accent)
            .subtitle(tr!("integration_qa_modal_subtitle"))
            .width(px(440.0))
            .footer(footer)
            .on_close(
                "qa-modal-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            );

        let modal_view = cx.entity();
        let modal_overlay = overlay(card, &palette)
            .position(OverlayPosition::Center)
            .on_dismiss("qa-modal-scrim", move |_window, cx| {
                modal_view.update(cx, |this, cx| this.cancel(cx));
            })
            .into_any_element();

        let choice_overlay = self.open_choice.as_ref().map(|open| {
            let choice_view = cx.entity();
            overlay(open.picker.clone(), &palette)
                .position(OverlayPosition::Center)
                .on_dismiss("qa-choice-scrim", move |_window, cx| {
                    choice_view.update(cx, |this, cx| this.close_choice(cx));
                })
                .into_any_element()
        });

        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(modal_overlay)
            .children(choice_overlay)
    }
}

impl ModalField {
    fn current_value(&self, cx: &Context<QuickActionModal>) -> QuickActionFieldValue {
        match &self.control {
            FieldControl::Text(input) => {
                QuickActionFieldValue::Text(input.read(cx).content().to_owned())
            }
            FieldControl::Multiline(area) => {
                QuickActionFieldValue::Text(area.read(cx).content().to_owned())
            }
            FieldControl::Toggle(value) => QuickActionFieldValue::Toggle(*value),
            FieldControl::Choice(choice) => QuickActionFieldValue::Text(
                choice
                    .selected
                    .as_ref()
                    .map(|s| s.to_string())
                    .unwrap_or_default(),
            ),
        }
    }
}

struct FieldSpec {
    key: String,
    label: String,
    hint: Option<String>,
    kind: QuickActionFieldKind,
    default: Option<QuickActionFieldValue>,
    placeholder: Option<String>,
    required: bool,
}

fn build_specs(action: &QuickAction) -> Vec<FieldSpec> {
    if !action.fields.is_empty() {
        return action
            .fields
            .iter()
            .map(|field| FieldSpec {
                key: field.key.clone(),
                label: field.label.clone(),
                hint: field.hint.clone(),
                kind: field.kind.clone(),
                default: field.default.clone(),
                placeholder: field.placeholder.clone(),
                required: field.required,
            })
            .collect();
    }
    match action.picker {
        Some(pk) => vec![FieldSpec {
            key: picker_key(pk).to_owned(),
            label: picker_title(pk),
            hint: None,
            kind: QuickActionFieldKind::Choice(QuickActionChoiceSource::Dynamic(pk)),
            default: None,
            placeholder: None,
            required: true,
        }],
        None => Vec::new(),
    }
}

fn spec_to_field(spec: &FieldSpec) -> QuickActionField {
    QuickActionField {
        key: spec.key.clone(),
        label: spec.label.clone(),
        kind: spec.kind.clone(),
        default: spec.default.clone(),
        placeholder: spec.placeholder.clone(),
        hint: spec.hint.clone(),
        required: spec.required,
    }
}

fn picker_key(pk: PickerKind) -> &'static str {
    match pk {
        PickerKind::Scene => "scene",
        PickerKind::Source | PickerKind::Input => "source",
        PickerKind::Hotkey => "hotkey",
        PickerKind::Expression => "expression",
        PickerKind::MidiPort => "port",
        PickerKind::Transition => "transition",
        PickerKind::Profile => "profile",
        PickerKind::SceneCollection => "collection",
    }
}

fn picker_title(pk: PickerKind) -> String {
    match pk {
        PickerKind::Scene => tr!("builtin_picker_scene"),
        PickerKind::Source => tr!("builtin_picker_source"),
        PickerKind::Input => tr!("builtin_picker_audio_input"),
        PickerKind::Hotkey => tr!("builtin_picker_hotkey"),
        PickerKind::Expression => tr!("builtin_picker_expression"),
        PickerKind::MidiPort => tr!("builtin_picker_midi_port"),
        PickerKind::Transition => tr!("builtin_picker_transition"),
        PickerKind::Profile => tr!("builtin_picker_profile"),
        PickerKind::SceneCollection => tr!("builtin_picker_scene_collection"),
    }
}

fn choice_title(field: &ModalField) -> String {
    match &field.control {
        FieldControl::Choice(ChoiceControl {
            dynamic: Some(pk), ..
        }) => picker_title(*pk),
        _ => field.label.clone(),
    }
}

fn static_item(opt: &QuickActionChoiceOption) -> PickerItem {
    PickerItem {
        id: opt.value.clone().into(),
        label: opt.label.clone().into(),
        sublabel: None,
        icon: Icon::from_name("circle"),
    }
}

fn choice_default(
    default: &Option<QuickActionFieldValue>,
    opts: &[QuickActionChoiceOption],
) -> Option<SharedString> {
    match default {
        Some(QuickActionFieldValue::Text(value)) => Some(value.clone().into()),
        _ => opts.first().map(|opt| opt.value.clone().into()),
    }
}

fn selected_label(items: &[PickerItem], selected: &Option<SharedString>) -> SharedString {
    match selected {
        Some(id) => items
            .iter()
            .find(|item| &item.id == id)
            .map(|item| item.label.clone())
            .unwrap_or_else(|| id.clone()),
        None => tr!("integration_qa_field_select").into(),
    }
}

fn default_text(default: &Option<QuickActionFieldValue>) -> String {
    match default {
        Some(QuickActionFieldValue::Text(value)) => value.clone(),
        _ => String::new(),
    }
}

fn default_toggle(default: &Option<QuickActionFieldValue>) -> bool {
    matches!(default, Some(QuickActionFieldValue::Toggle(true)))
}

struct PickerFetch {
    items: Vec<PickerItem>,
    scene: Option<String>,
    preselect: Option<SharedString>,
    hint_scene: Option<String>,
}

fn no_context(items: Vec<PickerItem>) -> PickerFetch {
    PickerFetch {
        items,
        scene: None,
        preselect: None,
        hint_scene: None,
    }
}

struct FetchLabels {
    no_scene: String,
    source_visible: String,
    source_hidden: String,
    unavailable: String,
}

fn fetch_labels() -> FetchLabels {
    FetchLabels {
        no_scene: tr!("integration_qa_field_no_scene"),
        source_visible: tr!("integration_qa_source_visible"),
        source_hidden: tr!("integration_qa_source_hidden"),
        unavailable: tr!("integration_qa_field_unavailable"),
    }
}

async fn fetch_picker_items(
    client: Arc<ObsClient>,
    kind: PickerKind,
    labels: FetchLabels,
) -> Result<PickerFetch, String> {
    match kind {
        PickerKind::Scene => {
            let current = client.current_scene().await.map_err(|e| e.to_string())?;
            let scenes = client.scenes().await.map_err(|e| e.to_string())?;
            let items = scenes
                .into_iter()
                .map(|name| PickerItem {
                    id: name.clone().into(),
                    label: name.into(),
                    sublabel: None,
                    icon: Icon::from_name("layout"),
                })
                .collect();
            Ok(PickerFetch {
                items,
                scene: None,
                preselect: current.clone().map(SharedString::from),
                hint_scene: current,
            })
        }
        PickerKind::Source => {
            let scene = client
                .current_scene()
                .await
                .map_err(|e| e.to_string())?
                .ok_or(labels.no_scene)?;
            let sources = client.sources(&scene).await.map_err(|e| e.to_string())?;
            let items = sources
                .into_iter()
                .map(|source| PickerItem {
                    id: source.name.clone().into(),
                    label: source.name.into(),
                    sublabel: Some(
                        if source.visible {
                            labels.source_visible.clone()
                        } else {
                            labels.source_hidden.clone()
                        }
                        .into(),
                    ),
                    icon: Icon::from_name("device-desktop"),
                })
                .collect();
            Ok(PickerFetch {
                items,
                scene: Some(scene),
                preselect: None,
                hint_scene: None,
            })
        }
        PickerKind::Input => {
            let inputs = client.audio_inputs().await.map_err(|e| e.to_string())?;
            let items = inputs
                .into_iter()
                .map(|name| PickerItem {
                    id: name.clone().into(),
                    label: name.into(),
                    sublabel: None,
                    icon: Icon::from_name("volume"),
                })
                .collect();
            Ok(no_context(items))
        }
        PickerKind::Transition => {
            let transitions = client.transitions().await.map_err(|e| e.to_string())?;
            let items = transitions
                .into_iter()
                .map(|name| PickerItem {
                    id: name.clone().into(),
                    label: name.into(),
                    sublabel: None,
                    icon: Icon::from_name("transition-right"),
                })
                .collect();
            Ok(no_context(items))
        }
        PickerKind::Profile => {
            let profiles = client.profiles().await.map_err(|e| e.to_string())?;
            let items = profiles
                .into_iter()
                .map(|name| PickerItem {
                    id: name.clone().into(),
                    label: name.into(),
                    sublabel: None,
                    icon: Icon::from_name("user-cog"),
                })
                .collect();
            Ok(no_context(items))
        }
        PickerKind::SceneCollection => {
            let collections = client
                .scene_collections()
                .await
                .map_err(|e| e.to_string())?;
            let items = collections
                .into_iter()
                .map(|name| PickerItem {
                    id: name.clone().into(),
                    label: name.into(),
                    sublabel: None,
                    icon: Icon::from_name("layout-2"),
                })
                .collect();
            Ok(no_context(items))
        }
        PickerKind::Hotkey | PickerKind::Expression | PickerKind::MidiPort => {
            Err(labels.unavailable)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_platform_core::{QuickActionAccent, SectionIcon};

    use super::*;

    fn picker_only_action(picker: PickerKind) -> QuickAction {
        QuickAction {
            label: "Switch scene".to_owned(),
            icon: SectionIcon::new("arrows-shuffle"),
            enabled: true,
            locked_reason: None,
            group: None,
            group_icon: None,
            group_accent: None,
            destructive: false,
            accent: QuickActionAccent::Brand,
            subaction_template: SubActionStep {
                kind_id: "obs.scenes.switch_current".to_owned(),
                config: BTreeMap::from([("scene".to_owned(), Variant::String(String::new()))]),
                enabled: true,
                continue_on_error: false,
                condition: None,
                label: None,
            },
            picker: Some(picker),
            fields: Vec::new(),
        }
    }

    // Why: merge_config only walks an action's own fields, so a picker-only action depends on
    // the synthesized picker spec being folded back in - otherwise the modal runs the untouched
    // template and the picked value is silently dropped.
    #[test]
    fn a_picker_only_action_merges_the_picked_value_into_the_step_config() {
        let action = picker_only_action(PickerKind::Scene);
        let mut merge_action = action.clone();
        merge_action.fields = build_specs(&action).iter().map(spec_to_field).collect();
        let values = BTreeMap::from([(
            "scene".to_owned(),
            QuickActionFieldValue::Text("Gameplay".to_owned()),
        )]);

        let step = merge_action.merge_config(&values);

        assert_eq!(
            step.config.get("scene"),
            Some(&Variant::String("Gameplay".to_owned())),
        );
    }

    #[test]
    fn declared_fields_take_precedence_over_the_action_picker() {
        let mut action = picker_only_action(PickerKind::Scene);
        action.fields = vec![QuickActionField {
            key: "scene".to_owned(),
            label: "Scene".to_owned(),
            kind: QuickActionFieldKind::Text,
            default: Some(QuickActionFieldValue::Text("BRB".to_owned())),
            placeholder: None,
            hint: None,
            required: false,
        }];

        let specs = build_specs(&action);

        assert_eq!(specs.len(), 1);
        assert!(matches!(specs[0].kind, QuickActionFieldKind::Text));
    }
}
