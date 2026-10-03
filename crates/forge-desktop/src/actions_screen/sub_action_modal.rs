use super::editor::{step_glyph, sub_category_color};
use super::*;
use crate::async_bridge;
use crate::config_field_label::localized_label;
use crate::config_form::dependent_options;
use crate::presentation::ActivePresentation;
use forge_components::highlight::Language;
use forge_components::{
    BORDER_THIN, CodeEditor, DateTimePicker, DateTimePickerEvent, DateTimePickerLabels, FONT_SM,
    FONT_XS, FONT_XXS, InputEvent, Picker, PickerEvent, PickerItem, PickerLabels, Radius, Spacing,
    anchored_popover, body_family, drive_overlay_focus, dropdown, field_label,
    ghost_button_with_icon, modal, mono_family, primary_button, radius, secondary_button, spacing,
    toggle,
};
use forge_registry::{
    CodeLanguage, FormField, FormRefinement, FormSchemaSource, SubActionCategory, refined_fields,
};
use forge_types::{SubActionConfig, Variant, normalize_var_name};
use gpui::{FocusHandle, FontWeight, Rgba};

#[derive(Clone, Copy)]
pub(super) enum SubFormTarget {
    Edit(usize),
    Add,
}

pub(super) struct SelectPickerForm {
    key: String,
    picker: Entity<Picker>,
    _sub: Subscription,
}

pub(super) struct DateTimePickerForm {
    picker: Entity<DateTimePicker>,
    target_input: Entity<TextInput>,
    pos: Point<Pixels>,
    _sub: Subscription,
}

enum SubFormField {
    Input {
        key: String,
        label: String,
        integer: bool,
        browse: bool,
        datetime: bool,
        gate: Option<String>,
        input: Entity<TextInput>,
        _sub: Option<Subscription>,
    },
    Area {
        key: String,
        label: String,
        gate: Option<String>,
        area: Entity<TextArea>,
    },
    Code {
        key: String,
        label: String,
        gate: Option<String>,
        language: CodeLanguage,
        editor: Entity<CodeEditor>,
    },
    Bool {
        key: String,
        label: String,
        gate: Option<String>,
        value: bool,
    },
    Select {
        key: String,
        label: String,
        options_key: Option<String>,
        options: Vec<(String, String)>,
        gate: Option<String>,
        selected: String,
        dependency: Option<SelectDependency>,
    },
    Hint {
        label: String,
    },
}

struct SelectDependency {
    options_prefix: String,
    depends_on: String,
}

pub(super) struct SubFormLaunch {
    pub kind_id: String,
    pub target: SubFormTarget,
    pub specs: Vec<FormField>,
    pub config: SubActionConfig,
    pub name_value: String,
    pub condition_value: String,
    pub continue_on_error: bool,
    pub kind_label: String,
    pub icon_name: String,
    pub category: Option<SubActionCategory>,
    pub chain_len: usize,
    pub options_seed: HashMap<String, Vec<(String, String)>>,
    pub refinement: Option<FormRefinement>,
    pub schema: Arc<dyn FormSchemaSource>,
}

#[derive(Clone)]
pub(super) struct SubFormCommit {
    pub target: SubFormTarget,
    pub kind_id: String,
    pub overrides: Vec<(String, Variant)>,
    pub continue_on_error: bool,
    pub condition: Option<String>,
    pub label: Option<String>,
}

pub(super) enum SubFormEvent {
    Commit(SubFormCommit),
    Cancel,
}

pub(super) struct EditSubActionForm {
    kind_id: String,
    kind_label: String,
    icon_name: String,
    category: Option<SubActionCategory>,
    target: SubFormTarget,
    chain_len: usize,
    fields: Vec<SubFormField>,
    base_field_count: usize,
    launch_config: SubActionConfig,
    options: HashMap<String, Vec<(String, String)>>,
    refinement: Option<FormRefinement>,
    schema: Arc<dyn FormSchemaSource>,
    name_input: Entity<TextInput>,
    condition_input: Entity<TextInput>,
    continue_on_error: bool,
    select_picker: Option<SelectPickerForm>,
    datetime_picker: Option<DateTimePickerForm>,
    datetime_focus: FocusHandle,
    datetime_focus_restore: Option<FocusHandle>,
    rt_handle: tokio::runtime::Handle,
}

impl EventEmitter<SubFormEvent> for EditSubActionForm {}

impl EditSubActionForm {
    pub(super) fn new(
        launch: SubFormLaunch,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let SubFormLaunch {
            kind_id,
            target,
            specs,
            config,
            name_value,
            condition_value,
            continue_on_error,
            kind_label,
            icon_name,
            category,
            chain_len,
            options_seed,
            refinement,
            schema,
        } = launch;

        let palette = cx.palette();
        let fields = build_form_fields(&specs, &config, palette, &options_seed, cx);
        let base_field_count = fields.len();
        let (name_input, condition_input) =
            build_step_meta_inputs(&kind_label, &name_value, &condition_value, cx);

        let mut form = Self {
            kind_id,
            kind_label,
            icon_name,
            category,
            target,
            chain_len,
            fields,
            base_field_count,
            launch_config: config,
            options: options_seed,
            refinement,
            schema,
            name_input,
            condition_input,
            continue_on_error,
            select_picker: None,
            datetime_picker: None,
            datetime_focus: cx.focus_handle(),
            datetime_focus_restore: None,
            rt_handle,
        };
        form.rebuild_refined(cx);
        form
    }

    pub(super) fn set_schema(&mut self, schema: Arc<dyn FormSchemaSource>, cx: &mut Context<Self>) {
        self.schema = schema;
        self.rebuild_refined(cx);
        cx.notify();
    }

    fn rebuild_refined(&mut self, cx: &mut Context<Self>) {
        let Some(refinement) = self.refinement else {
            return;
        };
        let seed = self.current_values(cx);
        let specs = refined_fields(refinement, &seed, self.schema.as_ref());
        self.fields.truncate(self.base_field_count);
        let palette = cx.palette();
        let options = self.options.clone();
        let mut refined = Vec::new();
        for spec in &specs {
            push_form_field(spec, None, &seed, palette, &options, &mut refined, cx);
        }
        self.fields.append(&mut refined);
        if let Some(key) = self.select_picker.as_ref().map(|form| form.key.clone())
            && !self
                .fields
                .iter()
                .any(|field| matches!(field, SubFormField::Select { key: k, .. } if *k == key))
        {
            self.select_picker = None;
        }
    }

    fn resolve_dependent_selects(&mut self, cx: &App) {
        let values = self.current_values(cx);
        let Self {
            fields, options, ..
        } = self;
        for field in fields.iter_mut() {
            if let SubFormField::Select {
                options: opts,
                dependency: Some(dependency),
                ..
            } = field
            {
                let sibling = values
                    .get(&dependency.depends_on)
                    .map(forge_types::display_scalar)
                    .unwrap_or_default();
                *opts = dependent_options(options, &dependency.options_prefix, &sibling);
            }
        }
    }

    fn current_values(&self, cx: &App) -> SubActionConfig {
        let mut values = self.launch_config.clone();
        for field in &self.fields {
            if let Some((key, value)) = field_value(field, cx) {
                values.insert(key, value);
            }
        }
        values
    }

    pub(super) fn apply_options(
        &mut self,
        map: &HashMap<String, Vec<(String, String)>>,
        cx: &mut Context<Self>,
    ) {
        self.options = map.clone();
        for field in &mut self.fields {
            if let SubFormField::Select {
                options_key: Some(ok),
                options,
                ..
            } = field
                && let Some(opts) = map.get(ok)
            {
                *options = opts.clone();
            }
        }
        self.resolve_dependent_selects(cx);
        if let Some(picker_form) = self.select_picker.as_ref() {
            let key = picker_form.key.clone();
            if let Some(SubFormField::Select { options, .. }) = self
                .fields
                .iter()
                .find(|field| matches!(field, SubFormField::Select { key: k, .. } if *k == key))
            {
                let items = select_picker_items(options);
                picker_form
                    .picker
                    .update(cx, |picker, cx| picker.set_items(items, cx));
            }
        }
        cx.notify();
    }

    fn toggle_sub_continue_on_error(&mut self, cx: &mut Context<Self>) {
        self.continue_on_error = !self.continue_on_error;
        cx.notify();
    }

    fn toggle_sub_field(&mut self, key: String, cx: &mut Context<Self>) {
        for field in &mut self.fields {
            if let SubFormField::Bool { key: k, value, .. } = field
                && *k == key
            {
                *value = !*value;
            }
        }
        cx.notify();
    }

    fn open_select_picker(&mut self, key: String, window: &mut Window, cx: &mut Context<Self>) {
        let already_open = self
            .select_picker
            .as_ref()
            .is_some_and(|picker_form| picker_form.key == key);
        if already_open {
            self.close_select_picker(cx);
            return;
        }
        let Some(SubFormField::Select {
            options,
            selected,
            options_key,
            ..
        }) = self
            .fields
            .iter()
            .find(|field| matches!(field, SubFormField::Select { key: k, .. } if *k == key))
        else {
            return;
        };
        let accepts_typed_value = options_key.is_some();
        let palette = cx.palette();
        let picker_labels = PickerLabels {
            placeholder: tr!("widget_picker_search_placeholder").into(),
            empty: tr!("actions_sub_select_empty").into(),
            loading: tr!("widget_picker_loading").into(),
        };
        let items = select_picker_items(options);
        let current = Some(SharedString::from(selected.clone()));
        let picker = cx.new(|cx| {
            let picker = Picker::new(picker_labels, items, palette, cx).with_current(current);
            if accepts_typed_value {
                picker.with_custom_entry(tr!("config_form_choice_custom").into())
            } else {
                picker
            }
        });
        let sub = cx.subscribe(&picker, Self::on_select_picker_event);
        picker.update(cx, |f, cx| f.focus(window, cx));
        self.select_picker = Some(SelectPickerForm {
            key,
            picker,
            _sub: sub,
        });
        cx.notify();
    }

    fn on_select_picker_event(
        &mut self,
        _picker: Entity<Picker>,
        event: &PickerEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            PickerEvent::Selected(id) => self.pick_select_option(id.to_string(), cx),
            PickerEvent::Cancelled => self.close_select_picker(cx),
        }
    }

    fn close_select_picker(&mut self, cx: &mut Context<Self>) {
        self.select_picker = None;
        cx.notify();
    }

    fn pick_select_option(&mut self, value: String, cx: &mut Context<Self>) {
        let picked = self.select_picker.as_ref().map(|p| p.key.clone());
        if let Some(key) = &picked {
            for field in &mut self.fields {
                if let SubFormField::Select {
                    key: k, selected, ..
                } = field
                    && k == key
                {
                    *selected = value.clone();
                }
            }
        }
        self.select_picker = None;
        let selector_changed = self
            .refinement
            .zip(picked.as_ref())
            .is_some_and(|(refinement, key)| refinement.selector_key == key);
        if selector_changed {
            self.rebuild_refined(cx);
        }
        self.resolve_dependent_selects(cx);
        cx.notify();
    }

    fn browse_sub_field(&mut self, input: Entity<TextInput>, cx: &mut Context<Self>) {
        async_bridge::spawn_dialog(
            &self.rt_handle,
            async_bridge::pick_file(None),
            move |_this, result, cx| {
                if let Ok(path) = result {
                    input.update(cx, |input, cx| {
                        input.set_content(path.to_string_lossy().into_owned(), cx);
                        cx.notify();
                    });
                }
            },
            cx,
        );
    }

    fn open_datetime_picker(
        &mut self,
        target_input: Entity<TextInput>,
        pos: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let palette = cx.palette();
        let initial = target_input.read(cx).content().to_owned();
        let labels = DateTimePickerLabels {
            now: tr!("actions_sub_datetime_now").into(),
            set: tr!("actions_sub_datetime_set").into(),
            cancel: tr!("common_cancel").into(),
        };
        let picker = cx.new(|cx| DateTimePicker::new(Some(initial.as_str()), labels, palette, cx));
        let sub = cx.subscribe(&picker, Self::on_datetime_event);
        self.datetime_picker = Some(DateTimePickerForm {
            picker,
            target_input,
            pos,
            _sub: sub,
        });
        cx.notify();
    }

    fn on_datetime_event(
        &mut self,
        _picker: Entity<DateTimePicker>,
        event: &DateTimePickerEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            DateTimePickerEvent::Picked(value) => {
                if let Some(form) = self.datetime_picker.take() {
                    let value = value.to_string();
                    form.target_input.update(cx, |input, cx| {
                        input.set_content(value, cx);
                        cx.notify();
                    });
                }
                cx.notify();
            }
            DateTimePickerEvent::Dismissed => self.close_datetime_picker(cx),
        }
    }

    fn close_datetime_picker(&mut self, cx: &mut Context<Self>) {
        self.datetime_picker = None;
        cx.notify();
    }

    fn on_var_input_event(
        &mut self,
        field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if let InputEvent::Changed(text) = event {
            let invalid = !text.trim().is_empty() && normalize_var_name(text).is_none();
            field.update(cx, |input, cx| input.set_invalid(invalid, cx));
        }
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(SubFormEvent::Cancel);
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        let mut has_invalid = false;
        for field in &self.fields {
            if let SubFormField::Input { key, input, .. } = field
                && is_var_key(key)
            {
                let text = input.read(cx).content().to_owned();
                let invalid = !text.trim().is_empty() && normalize_var_name(&text).is_none();
                input.update(cx, |input, cx| input.set_invalid(invalid, cx));
                has_invalid |= invalid;
            }
        }
        if has_invalid {
            return;
        }
        let target = self.target;
        let kind_id = self.kind_id.clone();
        let continue_on_error = self.continue_on_error;
        let label = resolve_step_label(self.name_input.read(cx).content(), &self.kind_label);
        let condition = normalize_condition(self.condition_input.read(cx).content());
        let bool_vals: HashMap<String, bool> = self
            .fields
            .iter()
            .filter_map(|f| match f {
                SubFormField::Bool { key, value, .. } => Some((key.clone(), *value)),
                _ => None,
            })
            .collect();
        let gate_on = |gate: Option<&String>| {
            gate.map(|g| bool_vals.get(g).copied().unwrap_or(false))
                .unwrap_or(true)
        };
        let overrides: Vec<(String, Variant)> = self
            .fields
            .iter()
            .filter(|field| gate_on(field_gate(field)))
            .filter_map(|field| field_value(field, cx))
            .collect();

        cx.emit(SubFormEvent::Commit(SubFormCommit {
            target,
            kind_id,
            overrides,
            continue_on_error,
            condition,
            label,
        }));
    }

    #[allow(clippy::too_many_arguments)]
    fn render_select_field(
        &self,
        key: &str,
        label: &str,
        options: &[(String, String)],
        selected: &str,
        open_picker: Option<&SelectPickerForm>,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected_label = options
            .iter()
            .find(|(value, _)| value == selected)
            .map(|(_, label)| label.clone());
        let (display, display_color): (String, Rgba) = match selected_label {
            Some(label) => (label, palette.text_primary),
            None if !selected.is_empty() => (selected.to_owned(), palette.text_primary),
            None => (tr!("actions_sub_select_placeholder"), palette.text_faint),
        };

        let key_open = key.to_owned();
        let border_color = if open_picker.is_some() {
            palette.brand
        } else {
            palette.border_input
        };
        let hover_border = palette.brand;
        let trigger = div()
            .id(SharedString::from(format!("actions-sub-select-{key}")))
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .py(px(6.0))
            .px(px(10.0))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(border_color)
            .bg(palette.shell)
            .cursor_pointer()
            .hover(move |s| s.border_color(hover_border))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.open_select_picker(key_open.clone(), window, cx)
            }))
            .child(
                div()
                    .flex_1()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(display_color)
                    .child(display),
            )
            .child(icon(Icon::ChevronDown, FONT_SM, palette.text_faint));

        let popover = open_picker.map(|form| {
            let view = cx.entity();
            dropdown(form.picker.clone()).on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_select_picker(cx));
            })
        });

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(label.to_owned()),
            )
            .child(div().relative().w_full().child(trigger).children(popover))
            .into_any_element()
    }

    fn render_modal(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let (header_glyph, header_color) = step_glyph(
            &self.kind_id,
            &self.icon_name,
            self.category.map(|c| sub_category_color(c, palette)),
            palette,
        );
        let (step_index, step_total) = match self.target {
            SubFormTarget::Edit(i) => (i + 1, self.chain_len),
            SubFormTarget::Add => (self.chain_len + 1, self.chain_len + 1),
        };
        let bool_vals: HashMap<&str, bool> = self
            .fields
            .iter()
            .filter_map(|f| match f {
                SubFormField::Bool { key, value, .. } => Some((key.as_str(), *value)),
                _ => None,
            })
            .collect();
        let gate_on = |gate: &Option<String>| {
            gate.as_ref()
                .map(|g| bool_vals.get(g.as_str()).copied().unwrap_or(false))
                .unwrap_or(true)
        };

        let mut grid_items: Vec<(bool, AnyElement)> = Vec::new();
        for field in &self.fields {
            match field {
                SubFormField::Input {
                    key,
                    label,
                    browse,
                    datetime,
                    gate,
                    input,
                    ..
                } => {
                    if !gate_on(gate) {
                        continue;
                    }
                    let control: AnyElement = if *browse {
                        let target_input = input.clone();
                        div()
                            .flex()
                            .items_center()
                            .gap(spacing(Spacing::Xs, Density::Cozy))
                            .child(div().flex_1().child(input.clone()))
                            .child(
                                ghost_button_with_icon(
                                    Icon::Folder,
                                    tr!("actions_sub_file_browse"),
                                    palette,
                                )
                                .on_click(
                                    SharedString::from(format!("actions-sub-browse-{key}")),
                                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        this.browse_sub_field(target_input.clone(), cx)
                                    }),
                                ),
                            )
                            .into_any_element()
                    } else if *datetime {
                        let target_input = input.clone();
                        div()
                            .flex()
                            .items_center()
                            .gap(spacing(Spacing::Xs, Density::Cozy))
                            .child(div().flex_1().child(input.clone()))
                            .child(
                                ghost_button_with_icon(
                                    Icon::Calendar,
                                    tr!("actions_sub_datetime_pick"),
                                    palette,
                                )
                                .on_click(
                                    SharedString::from(format!("actions-sub-datetime-{key}")),
                                    cx.listener(move |this, ev: &ClickEvent, _, cx| {
                                        this.open_datetime_picker(
                                            target_input.clone(),
                                            ev.position(),
                                            cx,
                                        )
                                    }),
                                ),
                            )
                            .into_any_element()
                    } else {
                        input.clone().into_any_element()
                    };
                    grid_items.push((
                        sub_field_is_half(field),
                        field_wrap(label, control, palette),
                    ));
                }
                SubFormField::Area {
                    label, gate, area, ..
                } => {
                    if !gate_on(gate) {
                        continue;
                    }
                    grid_items.push((
                        false,
                        multiline_field(label, None, area.clone().into_any_element(), palette),
                    ));
                }
                SubFormField::Code {
                    label,
                    gate,
                    language,
                    editor,
                    ..
                } => {
                    if !gate_on(gate) {
                        continue;
                    }
                    let tag = match language {
                        CodeLanguage::Rhai => "rhai",
                        CodeLanguage::Json => "json",
                    };
                    grid_items.push((
                        false,
                        multiline_field(
                            label,
                            Some(tag),
                            editor.clone().into_any_element(),
                            palette,
                        ),
                    ));
                }
                SubFormField::Bool {
                    key,
                    label,
                    gate,
                    value,
                } => {
                    if !gate_on(gate) {
                        continue;
                    }
                    let toggle_key = key.clone();
                    grid_items.push((
                        false,
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(spacing(Spacing::Sm, Density::Cozy))
                            .child(
                                div()
                                    .font_family(body_family())
                                    .text_size(FONT_XS)
                                    .text_color(palette.text_primary)
                                    .child(label.clone()),
                            )
                            .child(toggle(*value, palette).on_click(
                                SharedString::from(format!("actions-sub-toggle-{key}")),
                                cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    this.toggle_sub_field(toggle_key.clone(), cx)
                                }),
                            ))
                            .into_any_element(),
                    ));
                }
                SubFormField::Select {
                    key,
                    label,
                    options,
                    gate,
                    selected,
                    ..
                } => {
                    if !gate_on(gate) {
                        continue;
                    }
                    let open_picker = self
                        .select_picker
                        .as_ref()
                        .filter(|picker_form| picker_form.key == *key);
                    grid_items.push((
                        true,
                        self.render_select_field(
                            key,
                            label,
                            options,
                            selected,
                            open_picker,
                            palette,
                            cx,
                        ),
                    ));
                }
                SubFormField::Hint { label } => {
                    grid_items.push((
                        false,
                        div()
                            .flex()
                            .flex_col()
                            .gap(spacing(Spacing::Xxs, Density::Cozy))
                            .child(
                                div()
                                    .font_family(mono_family())
                                    .text_size(FONT_XXS)
                                    .text_color(palette.text_muted)
                                    .child(label.clone()),
                            )
                            .child(
                                div()
                                    .font_family(body_family())
                                    .text_size(FONT_XS)
                                    .text_color(palette.text_faint)
                                    .child(tr!("action_editor_branch_modal_hint")),
                            )
                            .into_any_element(),
                    ));
                }
            }
        }

        let mut grid = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, Density::Cozy));
        if grid_items.is_empty() {
            grid = grid.child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_muted)
                    .child(tr!("actions_sub_no_config")),
            );
        } else {
            let mut it = grid_items.into_iter().peekable();
            while let Some((half, element)) = it.next() {
                if half {
                    let second = if it.peek().map(|(h, _)| *h).unwrap_or(false) {
                        it.next().map(|(_, e)| e)
                    } else {
                        None
                    };
                    let right = match second {
                        Some(e) => div().flex_1().child(e),
                        None => div().flex_1(),
                    };
                    grid = grid.child(
                        div()
                            .flex()
                            .gap(GRID_COL_GAP)
                            .child(div().flex_1().child(element))
                            .child(right),
                    );
                } else {
                    grid = grid.child(element);
                }
            }
        }

        let continue_row = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, Density::Cozy))
                    .child(icon(Icon::AlertTriangle, CARD_GLYPH, palette.warning))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(spacing(Spacing::Xxs, Density::Cozy))
                            .child(
                                div()
                                    .font_family(body_family())
                                    .text_size(FONT_XS)
                                    .text_color(palette.text_primary)
                                    .child(tr!("actions_step_continue_on_error")),
                            )
                            .child(
                                div()
                                    .font_family(body_family())
                                    .text_size(FONT_XXS)
                                    .text_color(palette.text_faint)
                                    .child(tr!("actions_step_continue_on_error_hint")),
                            ),
                    ),
            )
            .child(toggle(self.continue_on_error, palette).on_click(
                "actions-sub-continue-on-error",
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_sub_continue_on_error(cx)),
            ));

        let condition_field = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(tr!("actions_step_condition_label")),
            )
            .child(self.condition_input.clone())
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(tr!("actions_step_condition_hint")),
            );

        let advanced = div()
            .w_full()
            .mt(spacing(Spacing::Xs, Density::Cozy))
            .pt(spacing(Spacing::Sm, Density::Cozy))
            .border_t(HALF_BORDER)
            .border_color(palette.border_regular)
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(tr!("actions_step_advanced")),
            )
            .child(condition_field)
            .child(continue_row);

        let body = div()
            .id("actions-sub-scroll")
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .px(spacing(Spacing::Md, Density::Cozy))
            .py(spacing(Spacing::Sm, Density::Cozy))
            .max_h(SUB_MODAL_MAX_H)
            .overflow_y_scroll()
            .child(grid)
            .child(advanced);

        let title_slot = div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .overflow_hidden()
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.name_input.clone()),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(tr!(
                        "actions_step_subtitle",
                        index = step_index as i64,
                        total = step_total as i64
                    )),
            );

        let cancel = secondary_button(tr!("common_cancel"), palette).on_click(
            "actions-sub-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
        );
        let save = primary_button(tr!("actions_modal_save_btn"), palette).on_click(
            "actions-sub-submit",
            cx.listener(|this, _: &ClickEvent, _, cx| this.submit(cx)),
        );
        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .justify_end()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(cancel)
            .child(save);

        let card = modal("", body, palette)
            .width(STEP_MODAL_W)
            .header_icon(header_glyph, header_color)
            .header_tile_size(STEP_TILE, STEP_TILE_GLYPH)
            .title_slot(title_slot)
            .flush_body()
            .footer(footer)
            .on_close(
                "actions-sub-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("actions-sub-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel(cx));
            })
            .into_any_element()
    }

    fn render_datetime_popover(
        &self,
        form: &DateTimePickerForm,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        anchored_popover(form.pos, form.picker.clone())
            .dismiss_on_escape(&self.datetime_focus)
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_datetime_picker(cx));
            })
            .into_any_element()
    }
}

impl Render for EditSubActionForm {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();

        drive_overlay_focus(
            self.datetime_picker.is_some(),
            &self.datetime_focus,
            &mut self.datetime_focus_restore,
            window,
            cx,
        );

        let modal = self.render_modal(&palette, cx);
        let datetime_popover = self
            .datetime_picker
            .as_ref()
            .map(|form| self.render_datetime_popover(form, cx));

        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(modal)
            .children(datetime_popover)
    }
}

fn field_gate(field: &SubFormField) -> Option<&String> {
    match field {
        SubFormField::Input { gate, .. }
        | SubFormField::Area { gate, .. }
        | SubFormField::Code { gate, .. }
        | SubFormField::Bool { gate, .. }
        | SubFormField::Select { gate, .. } => gate.as_ref(),
        SubFormField::Hint { .. } => None,
    }
}

fn field_value(field: &SubFormField, cx: &App) -> Option<(String, Variant)> {
    match field {
        SubFormField::Input {
            key,
            integer,
            input,
            ..
        } => {
            let text = input.read(cx).content().to_owned();
            if *integer {
                let parsed = text.trim().parse::<i64>().ok()?;
                Some((key.clone(), Variant::Int(parsed)))
            } else if is_var_key(key) {
                let name = normalize_var_name(&text).unwrap_or_default();
                Some((key.clone(), Variant::String(name)))
            } else {
                Some((key.clone(), Variant::String(text)))
            }
        }
        SubFormField::Area { key, area, .. } => Some((
            key.clone(),
            Variant::String(area.read(cx).content().to_owned()),
        )),
        SubFormField::Code { key, editor, .. } => Some((
            key.clone(),
            Variant::String(editor.read(cx).content().to_owned()),
        )),
        SubFormField::Bool { key, value, .. } => Some((key.clone(), Variant::Bool(*value))),
        SubFormField::Select { key, selected, .. } => {
            Some((key.clone(), Variant::String(selected.clone())))
        }
        SubFormField::Hint { .. } => None,
    }
}

fn is_var_key(key: &str) -> bool {
    matches!(key, "target_var" | "into_var" | "into_arg")
}

fn normalize_condition(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let trimmed = trimmed.strip_prefix("if ").unwrap_or(trimmed).trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn resolve_step_label(raw: &str, kind_label: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == kind_label {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn sub_field_is_half(field: &SubFormField) -> bool {
    match field {
        SubFormField::Select { .. } => true,
        SubFormField::Input {
            key,
            integer,
            browse,
            datetime,
            ..
        } => !*browse && !*datetime && (*integer || is_var_key(key)),
        _ => false,
    }
}

fn field_wrap(label: &str, control: AnyElement, palette: &ForgePalette) -> AnyElement {
    field_label(palette, label.to_owned(), control)
        .tone(palette.text_muted)
        .into_any_element()
}

fn select_picker_items(options: &[(String, String)]) -> Vec<PickerItem> {
    options
        .iter()
        .map(|(value, label)| PickerItem {
            id: SharedString::from(value.clone()),
            label: SharedString::from(label.clone()),
            sublabel: None,
            icon: None,
        })
        .collect()
}

fn build_form_fields(
    specs: &[FormField],
    config: &SubActionConfig,
    palette: ForgePalette,
    options_map: &HashMap<String, Vec<(String, String)>>,
    cx: &mut Context<EditSubActionForm>,
) -> Vec<SubFormField> {
    let mut fields: Vec<SubFormField> = Vec::new();
    for spec in specs {
        push_form_field(spec, None, config, palette, options_map, &mut fields, cx);
    }
    fields
}

fn build_step_meta_inputs(
    kind_label: &str,
    name_value: &str,
    condition_value: &str,
    cx: &mut Context<EditSubActionForm>,
) -> (Entity<TextInput>, Entity<TextInput>) {
    let palette = cx.palette();
    let placeholder = kind_label.to_owned();
    let name_value = name_value.to_owned();
    let condition_value = condition_value.to_owned();
    let name_input = cx.new(|cx| {
        let mut input = TextInput::new(placeholder, cx)
            .with_palette(palette)
            .plain()
            .with_font_size(FONT_SM);
        if !name_value.is_empty() {
            input.set_content(name_value, cx);
        }
        input
    });
    let condition_input = cx.new(|cx| {
        let mut input = TextInput::new("%user.isMod% == true", cx)
            .with_palette(palette)
            .mono()
            .prefix("if");
        if !condition_value.is_empty() {
            input.set_content(condition_value, cx);
        }
        input
    });
    (name_input, condition_input)
}

#[allow(clippy::too_many_arguments)]
fn build_input_field(
    key: &str,
    label: &str,
    placeholder: &'static str,
    integer: bool,
    browse: bool,
    datetime: bool,
    gate: Option<String>,
    config: &SubActionConfig,
    palette: ForgePalette,
    cx: &mut Context<EditSubActionForm>,
) -> SubFormField {
    let seed = config
        .get(key)
        .map(forge_types::display_scalar)
        .unwrap_or_default();
    let is_var = is_var_key(key);
    let invalid_seed = is_var && !seed.trim().is_empty() && normalize_var_name(&seed).is_none();
    let input = cx.new(|cx| {
        let ph = if is_var { "%result%" } else { placeholder };
        let mut input = TextInput::new(ph, cx).with_palette(palette);
        if is_var {
            input = input
                .mono()
                .leading_icon(Icon::Variable, palette.warning)
                .accent(palette.warning);
        }
        if !seed.is_empty() {
            input.set_content(seed, cx);
        }
        input
    });
    if invalid_seed {
        input.update(cx, |input, cx| input.set_invalid(true, cx));
    }
    let sub = is_var.then(|| cx.subscribe(&input, EditSubActionForm::on_var_input_event));
    SubFormField::Input {
        key: key.to_owned(),
        label: label.to_owned(),
        integer,
        browse,
        datetime,
        gate,
        input,
        _sub: sub,
    }
}

fn build_area_field(
    key: &str,
    label: &str,
    gate: Option<String>,
    config: &SubActionConfig,
    palette: ForgePalette,
    cx: &mut Context<EditSubActionForm>,
) -> SubFormField {
    let seed = config_seed(config, key);
    let area = cx.new(|cx| {
        let mut area = TextArea::new("", cx)
            .with_palette(palette)
            .with_height(SUB_AREA_FIELD_H);
        if !seed.is_empty() {
            area.set_content(seed, cx);
        }
        area
    });
    SubFormField::Area {
        key: key.to_owned(),
        label: label.to_owned(),
        gate,
        area,
    }
}

fn build_code_field(
    key: &str,
    label: &str,
    gate: Option<String>,
    language: CodeLanguage,
    config: &SubActionConfig,
    palette: ForgePalette,
    cx: &mut Context<EditSubActionForm>,
) -> SubFormField {
    let seed = config_seed(config, key);
    let editor = cx.new(|cx| {
        let mut editor = CodeEditor::new(highlight_language(language), "", cx)
            .with_palette(palette)
            .with_field_height(SUB_AREA_FIELD_H);
        if !seed.is_empty() {
            editor.set_content(seed, cx);
        }
        editor
    });
    SubFormField::Code {
        key: key.to_owned(),
        label: label.to_owned(),
        gate,
        language,
        editor,
    }
}

fn config_seed(config: &SubActionConfig, key: &str) -> String {
    config
        .get(key)
        .map(forge_types::display_scalar)
        .unwrap_or_default()
}

fn highlight_language(language: CodeLanguage) -> Language {
    match language {
        CodeLanguage::Rhai => Language::Rhai,
        CodeLanguage::Json => Language::Json,
    }
}

fn multiline_field(
    label: &str,
    tag: Option<&'static str>,
    control: AnyElement,
    palette: &ForgePalette,
) -> AnyElement {
    let caption = |text: SharedString| {
        div()
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_muted)
            .child(text)
    };
    let header = div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, Density::Cozy))
        .child(caption(label.to_owned().into()))
        .children(tag.map(|tag| caption(tag.into())));
    div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, Density::Cozy))
        .child(header)
        .child(control)
        .into_any_element()
}

fn push_form_field(
    spec: &FormField,
    gate: Option<String>,
    config: &SubActionConfig,
    palette: ForgePalette,
    options_map: &HashMap<String, Vec<(String, String)>>,
    out: &mut Vec<SubFormField>,
    cx: &mut Context<EditSubActionForm>,
) {
    match spec {
        FormField::Text {
            key,
            label,
            placeholder,
        } => out.push(build_input_field(
            key,
            label,
            placeholder,
            false,
            false,
            false,
            gate,
            config,
            palette,
            cx,
        )),
        FormField::TextArea { key, label } => {
            out.push(build_area_field(key, label, gate, config, palette, cx))
        }
        FormField::Code {
            key,
            label,
            language,
        } => out.push(build_code_field(
            key, label, gate, *language, config, palette, cx,
        )),
        FormField::Integer { key, label, .. } | FormField::Slider { key, label, .. } => {
            out.push(build_input_field(
                key, label, "0", true, false, false, gate, config, palette, cx,
            ))
        }
        FormField::FilePicker { key, label } => out.push(build_input_field(
            key, label, "", false, true, false, gate, config, palette, cx,
        )),
        FormField::DateTime { key, label } => out.push(build_input_field(
            key, label, "", false, false, true, gate, config, palette, cx,
        )),
        FormField::Select {
            key,
            label,
            options,
        }
        | FormField::Swatch {
            key,
            label,
            options,
        } => {
            let selected = config
                .get(*key)
                .map(forge_types::display_scalar)
                .unwrap_or_default();
            let options = options
                .iter()
                .map(|opt| ((*opt).to_owned(), (*opt).to_owned()))
                .collect();
            out.push(SubFormField::Select {
                key: (*key).to_owned(),
                label: (*label).to_owned(),
                options_key: None,
                options,
                gate,
                selected,
                dependency: None,
            });
        }
        FormField::DynamicSelect {
            key,
            label,
            options_key,
        } => {
            let selected = config
                .get(*key)
                .map(forge_types::display_scalar)
                .unwrap_or_default();
            let options = options_map.get(*options_key).cloned().unwrap_or_default();
            out.push(SubFormField::Select {
                key: (*key).to_owned(),
                label: localized_label(label),
                options_key: Some((*options_key).to_owned()),
                options,
                gate,
                selected,
                dependency: None,
            });
        }
        FormField::DependentSelect {
            key,
            label,
            options_prefix,
            depends_on,
        } => {
            let selected = config
                .get(*key)
                .map(forge_types::display_scalar)
                .unwrap_or_default();
            let sibling = config
                .get(*depends_on)
                .map(forge_types::display_scalar)
                .unwrap_or_default();
            out.push(SubFormField::Select {
                key: (*key).to_owned(),
                label: (*label).to_owned(),
                options_key: None,
                options: dependent_options(options_map, options_prefix, &sibling),
                gate,
                selected,
                dependency: Some(SelectDependency {
                    options_prefix: (*options_prefix).to_owned(),
                    depends_on: (*depends_on).to_owned(),
                }),
            });
        }
        FormField::Toggle { key, label } => {
            let value = matches!(config.get(*key), Some(Variant::Bool(true)));
            out.push(SubFormField::Bool {
                key: (*key).to_owned(),
                label: (*label).to_owned(),
                gate,
                value,
            });
        }
        FormField::SubChain { label, .. } | FormField::CaseList { label, .. } => {
            out.push(SubFormField::Hint {
                label: (*label).to_owned(),
            });
        }
        FormField::Optional { key, label, inner } => {
            let value = matches!(config.get(*key), Some(Variant::Bool(true)));
            out.push(SubFormField::Bool {
                key: (*key).to_owned(),
                label: (*label).to_owned(),
                gate: gate.clone(),
                value,
            });
            push_form_field(
                inner,
                Some((*key).to_owned()),
                config,
                palette,
                options_map,
                out,
                cx,
            );
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use forge_components::{ThemeId, bind_picker_keys, bind_text_input_keys};
    use gpui::{TestAppContext, VisualTestContext};

    use super::*;
    use crate::actions_screen::overlay_schema::OverlayContentSchema;
    use crate::presentation::Presentation;
    use crate::test_support::runtime;

    const REWARD_FIELD: &str = "reward_id";
    const OWNED_REWARDS: &str = "collections.twitch.rewards.manageable";
    const MODE_FIELD: &str = "mode";

    fn specs() -> Vec<FormField> {
        vec![
            FormField::DynamicSelect {
                key: REWARD_FIELD,
                label: "Reward",
                options_key: OWNED_REWARDS,
            },
            FormField::Select {
                key: MODE_FIELD,
                label: "Mode",
                options: &["on", "off"],
            },
        ]
    }

    fn launch(stored_reward: Option<&str>) -> SubFormLaunch {
        let config: SubActionConfig = stored_reward
            .map(|id| (REWARD_FIELD.to_owned(), Variant::String(id.to_owned())))
            .into_iter()
            .collect();
        launch_with(specs(), config)
    }

    fn launch_with(specs: Vec<FormField>, config: SubActionConfig) -> SubFormLaunch {
        SubFormLaunch {
            kind_id: "twitch.channel_points.enable_reward".to_owned(),
            target: SubFormTarget::Add,
            specs,
            config,
            name_value: String::new(),
            condition_value: String::new(),
            continue_on_error: false,
            kind_label: "Enable reward".to_owned(),
            icon_name: "gift".to_owned(),
            category: None,
            chain_len: 0,
            options_seed: HashMap::new(),
            refinement: None,
            schema: Arc::new(OverlayContentSchema::new(Arc::new(
                OverlayKindRegistry::new(),
            ))) as Arc<dyn FormSchemaSource>,
        }
    }

    fn open<'a>(
        cx: &'a mut TestAppContext,
        stored_reward: Option<&str>,
    ) -> (
        Entity<EditSubActionForm>,
        &'a mut VisualTestContext,
        tokio::runtime::Runtime,
    ) {
        open_launch(cx, launch(stored_reward))
    }

    fn open_launch(
        cx: &mut TestAppContext,
        launch: SubFormLaunch,
    ) -> (
        Entity<EditSubActionForm>,
        &mut VisualTestContext,
        tokio::runtime::Runtime,
    ) {
        cx.update(|cx| {
            cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
            bind_picker_keys(cx);
            bind_text_input_keys(cx);
        });
        let rt = runtime();
        let handle = rt.handle().clone();
        let (form, vcx) =
            cx.add_window_view(move |_window, cx| EditSubActionForm::new(launch, handle, cx));
        vcx.run_until_parked();
        (form, vcx, rt)
    }

    fn select(
        form: &Entity<EditSubActionForm>,
        vcx: &mut VisualTestContext,
        key: &str,
    ) -> (String, Vec<String>) {
        vcx.update(|_window, cx| {
            form.read(cx)
                .fields
                .iter()
                .find_map(|field| match field {
                    SubFormField::Select {
                        key: k,
                        selected,
                        options,
                        ..
                    } if k == key => Some((
                        selected.clone(),
                        options.iter().map(|(value, _)| value.clone()).collect(),
                    )),
                    _ => None,
                })
                .unwrap()
        })
    }

    fn deliver_rewards(form: &Entity<EditSubActionForm>, vcx: &mut VisualTestContext) {
        let map = HashMap::from([(
            OWNED_REWARDS.to_owned(),
            vec![("r-1".to_owned(), "Hydrate".to_owned())],
        )]);
        vcx.update(|_window, cx| form.update(cx, |form, cx| form.apply_options(&map, cx)));
    }

    fn type_into_picker(
        form: &Entity<EditSubActionForm>,
        vcx: &mut VisualTestContext,
        key: &str,
        typed: &str,
    ) {
        vcx.update(|window, cx| {
            form.update(cx, |form, cx| {
                form.open_select_picker(key.to_owned(), window, cx)
            })
        });
        vcx.run_until_parked();
        vcx.simulate_input(typed);
        vcx.simulate_keystrokes("enter");
        vcx.run_until_parked();
    }

    #[gpui::test]
    fn arriving_rewards_fill_the_reward_select(cx: &mut TestAppContext) {
        let (form, vcx, _rt) = open(cx, None);

        deliver_rewards(&form, vcx);

        assert_eq!(select(&form, vcx, REWARD_FIELD).1, ["r-1"]);
    }

    #[gpui::test]
    fn a_stored_raw_reward_id_stays_selected_after_the_rewards_arrive(cx: &mut TestAppContext) {
        let (form, vcx, _rt) = open(cx, Some("%reward.id%"));

        deliver_rewards(&form, vcx);

        assert_eq!(select(&form, vcx, REWARD_FIELD).0, "%reward.id%");
    }

    #[gpui::test]
    fn the_reward_select_accepts_a_typed_variable(cx: &mut TestAppContext) {
        let (form, vcx, _rt) = open(cx, None);
        deliver_rewards(&form, vcx);

        type_into_picker(&form, vcx, REWARD_FIELD, "%reward.id%");

        assert_eq!(select(&form, vcx, REWARD_FIELD).0, "%reward.id%");
    }

    #[gpui::test]
    fn a_fixed_option_select_does_not_accept_typed_text(cx: &mut TestAppContext) {
        let (form, vcx, _rt) = open(cx, None);

        type_into_picker(&form, vcx, MODE_FIELD, "sideways");

        assert_ne!(select(&form, vcx, MODE_FIELD).0, "sideways");
    }

    const CODE_FIELD: &str = "body";
    const GATE_FIELD: &str = "use_body";
    const RHAI_BODY: &str = "let n = 1;\r\nif n > 0 {\n\tprint(\"привіт 🚀\");\n}\n";
    const JSON_BODY: &str = "{\n  \"items\": [1, 2.5, null],\n  \"name\": \"ü\"\n}";

    fn code_spec(language: CodeLanguage) -> FormField {
        FormField::Code {
            key: CODE_FIELD,
            label: "Body",
            language,
        }
    }

    fn committed_overrides(
        specs: Vec<FormField>,
        config: SubActionConfig,
        cx: &mut TestAppContext,
    ) -> Vec<(String, Variant)> {
        let (form, vcx, _rt) = open_launch(cx, launch_with(specs, config));
        let heard = std::rc::Rc::new(std::cell::RefCell::new(None));
        let sink = std::rc::Rc::clone(&heard);
        let _subscription = vcx.update(|_window, cx| {
            cx.subscribe(&form, move |_, event: &SubFormEvent, _| {
                if let SubFormEvent::Commit(commit) = event {
                    *sink.borrow_mut() = Some(commit.overrides.clone());
                }
            })
        });
        vcx.update(|_window, cx| form.update(cx, |form, cx| form.submit(cx)));
        heard.borrow_mut().take().unwrap()
    }

    fn override_for<'a>(overrides: &'a [(String, Variant)], key: &str) -> Option<&'a Variant> {
        overrides.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    #[gpui::test]
    fn code_field_commits_the_stored_body_verbatim(cx: &mut TestAppContext) {
        for (language, stored, expected) in [
            (CodeLanguage::Rhai, Some(RHAI_BODY), RHAI_BODY),
            (CodeLanguage::Json, Some(JSON_BODY), JSON_BODY),
            (CodeLanguage::Rhai, None, ""),
            (CodeLanguage::Json, Some(""), ""),
        ] {
            let config: SubActionConfig = stored
                .map(|body| (CODE_FIELD.to_owned(), Variant::String(body.to_owned())))
                .into_iter()
                .collect();
            let overrides = committed_overrides(vec![code_spec(language)], config, cx);
            assert_eq!(
                override_for(&overrides, CODE_FIELD),
                Some(&Variant::String(expected.to_owned())),
                "{language:?} stored {stored:?}"
            );
        }
    }

    #[gpui::test]
    fn gated_code_field_commits_only_while_its_toggle_is_on(cx: &mut TestAppContext) {
        for (toggle, expected) in [
            (true, Some(Variant::String(RHAI_BODY.to_owned()))),
            (false, None),
        ] {
            let spec = FormField::Optional {
                key: GATE_FIELD,
                label: "Use body",
                inner: Box::new(code_spec(CodeLanguage::Rhai)),
            };
            let config: SubActionConfig = [
                (GATE_FIELD.to_owned(), Variant::Bool(toggle)),
                (CODE_FIELD.to_owned(), Variant::String(RHAI_BODY.to_owned())),
            ]
            .into_iter()
            .collect();
            let overrides = committed_overrides(vec![spec], config, cx);
            assert_eq!(
                override_for(&overrides, CODE_FIELD),
                expected.as_ref(),
                "toggle {toggle}"
            );
        }
    }
}
