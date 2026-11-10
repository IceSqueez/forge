use std::rc::Rc;
use std::sync::Arc;

use forge_components::{
    ActivePresentation, BORDER_THIN, FONT_XXS, ForgePalette, Icon, InputEvent, ModalSize,
    OverlayPosition, Picker, PickerEvent, Radius, Spacing, TextInput, ghost_button_with_icon,
    modal, overlay, primary_button, radius, secondary_button, spacing, tr,
};
use forge_registry::TriggerRegistry;
use forge_types::TriggerInstance;
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, EventEmitter, SharedString, Subscription, Window,
    div, prelude::*,
};

use crate::automation::trigger_form_shape::{trigger_form_defaults, trigger_form_specs};
use crate::automation::triggers_screen::{
    platform_dot_color, platform_kind_line, refuse_invalid_config, timer_field_hints,
};
use crate::shared::config_field_label::{config_field_labels, row_label};
use crate::shared::config_form::{
    ChoiceDropdown, ChoiceSupport, CollectionChoices, ConfigField, ConfigFieldHandlers,
    FILL_VAL_FS, FieldCapabilities, FoldContext, collect_field_values, fold_config_field,
    render_config_row, set_picked_value, set_slide_field, sparse_overrides, toggle_bool_field,
};
use crate::shared::option_choices::ChoiceOptions;

type BackHandler = Rc<dyn Fn(&mut Window, &mut App)>;

pub(crate) enum TriggerFillFormEvent {
    Cancelled,
    SubmitRequested,
}

#[derive(Clone, Copy)]
pub(crate) struct TriggerFillIds {
    pub(crate) field: &'static str,
    pub(crate) back: &'static str,
    pub(crate) cancel: &'static str,
    pub(crate) submit: &'static str,
    pub(crate) close: &'static str,
    pub(crate) scrim: &'static str,
}

pub(crate) struct TriggerFillSeed {
    pub(crate) registry: Arc<TriggerRegistry>,
    pub(crate) kind_id: String,
    pub(crate) ids: TriggerFillIds,
    pub(crate) on_back: BackHandler,
}

pub(crate) struct TriggerFillForm {
    registry: Arc<TriggerRegistry>,
    kind_id: String,
    kind_line: String,
    ids: TriggerFillIds,
    on_back: BackHandler,
    pub(crate) name_field: Entity<TextInput>,
    pub(crate) fields: Vec<ConfigField>,
    labels: Vec<SharedString>,
    choices: CollectionChoices,
    pub(crate) saving: bool,
    _name_sub: Subscription,
}

impl EventEmitter<TriggerFillFormEvent> for TriggerFillForm {}

impl TriggerFillForm {
    pub(crate) fn new(seed: TriggerFillSeed, cx: &mut Context<Self>) -> Self {
        let TriggerFillSeed {
            registry,
            kind_id,
            ids,
            on_back,
        } = seed;
        let palette = cx.palette();
        let descriptor = registry.get(&kind_id);
        let kind_line = platform_kind_line(&kind_id, &registry);
        let default = trigger_form_defaults(descriptor);
        let specs = trigger_form_specs(descriptor);

        let fold = FoldContext {
            kind_id: &kind_id,
            config: &default,
            defaults: &default,
            palette: &palette,
            capabilities: FieldCapabilities::inline(ChoiceSupport::Text),
            on_committed: Self::on_config_committed,
        };
        let mut fields: Vec<ConfigField> = Vec::new();
        let mut labels: Vec<SharedString> = Vec::new();
        for spec in &specs {
            fold_config_field(spec, None, &fold, &mut fields, cx);
            config_field_labels(&kind_id, spec, &mut labels);
        }

        let name_field = cx.new(|cx| {
            TextInput::new(tr!("triggers_create_name_placeholder"), cx)
                .static_chrome(|palette| palette.brand, Radius::Sm)
        });
        let name_sub = cx.subscribe(&name_field, Self::on_name_event);

        Self {
            registry,
            kind_id,
            kind_line,
            ids,
            on_back,
            name_field,
            fields,
            labels,
            choices: CollectionChoices::for_specs(&specs),
            saving: false,
            _name_sub: name_sub,
        }
    }

    pub(crate) fn set_saving(&mut self, saving: bool, cx: &mut Context<Self>) {
        self.saving = saving;
        cx.notify();
    }

    pub(crate) fn option_keys(&self) -> Vec<String> {
        self.choices.option_keys()
    }

    pub(crate) fn refresh_choices(
        &mut self,
        options: &ChoiceOptions,
        blank: &str,
        cx: &mut Context<Self>,
    ) {
        self.choices.refresh(&mut self.fields, options, Some(blank));
        cx.notify();
    }

    fn can_submit(&self, cx: &App) -> bool {
        !self.saving && !self.name_field.read(cx).content().trim().is_empty()
    }

    pub(crate) fn request_submit(&mut self, cx: &mut Context<Self>) {
        if self.can_submit(cx) {
            cx.emit(TriggerFillFormEvent::SubmitRequested);
        }
    }

    pub(crate) fn instance_to_save(&self, cx: &mut Context<Self>) -> Option<TriggerInstance> {
        if !self.can_submit(cx) {
            return None;
        }
        let default = trigger_form_defaults(self.registry.get(&self.kind_id));
        let mut buffer = default.clone();
        collect_field_values(&self.fields, &mut buffer, cx);
        if refuse_invalid_config(&self.registry, &self.kind_id, &buffer, cx) {
            return None;
        }
        Some(TriggerInstance::new_user_defined(
            self.kind_id.clone(),
            self.name_field.read(cx).content().trim().to_owned(),
            sparse_overrides(&default, &buffer),
        ))
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(TriggerFillFormEvent::Cancelled);
    }

    fn on_name_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Submitted(_) => self.request_submit(cx),
            InputEvent::Cancelled => self.cancel(cx),
            InputEvent::Changed(_) => cx.notify(),
            InputEvent::Blurred(_) => {}
        }
    }

    fn on_config_committed(&mut self, event: &InputEvent, cx: &mut Context<Self>) {
        if let InputEvent::Submitted(_) = event {
            self.request_submit(cx);
        }
    }

    fn toggle_config_field(&mut self, key: String, cx: &mut Context<Self>) {
        toggle_bool_field(&mut self.fields, &key);
        cx.notify();
    }

    fn slide_config_field(&mut self, key: String, next: i64, cx: &mut Context<Self>) {
        set_slide_field(&mut self.fields, &key, next);
        cx.notify();
    }

    fn pick_config_field(&mut self, key: String, choice: String, cx: &mut Context<Self>) {
        set_picked_value(&mut self.fields, &key, &choice, cx);
        cx.notify();
    }

    fn open_choice(&mut self, key: String, window: &mut Window, cx: &mut Context<Self>) {
        self.choices
            .toggle(&self.fields, key, Self::on_choice_event, window, cx);
        cx.notify();
    }

    fn close_choice(&mut self, cx: &mut Context<Self>) {
        self.choices.close();
        cx.notify();
    }

    fn on_choice_event(
        &mut self,
        _picker: Entity<Picker>,
        event: &PickerEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            PickerEvent::Selected(value) => {
                if let Some(key) = self.choices.take_open_key() {
                    set_picked_value(&mut self.fields, &key, value, cx);
                }
            }
            PickerEvent::Cancelled => self.choices.close(),
        }
        cx.notify();
    }

    fn config_handlers(&self) -> ConfigFieldHandlers<Self> {
        ConfigFieldHandlers {
            toggle: Self::toggle_config_field,
            slide: Self::slide_config_field,
            pick: Self::pick_config_field,
            choice: Some(ChoiceDropdown {
                open: Self::open_choice,
                close: Self::close_choice,
                active: self.choices.active(),
            }),
        }
    }

    fn config_card(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        if self.fields.is_empty() {
            return div()
                .py(spacing(Spacing::Sm, density))
                .px(spacing(Spacing::Sm, density))
                .italic()
                .font_family(cx.body_family())
                .text_size(FILL_VAL_FS)
                .text_color(palette.text_faint)
                .child(tr!("triggers_sheet_no_config"))
                .into_any_element();
        }
        let last = self.fields.len().saturating_sub(1);
        let view = cx.entity();
        let handlers = self.config_handlers();
        let mut col = div().flex().flex_col();
        for (i, field) in self.fields.iter().enumerate() {
            col = col.child(render_config_row(
                field,
                row_label(&self.labels, i, field),
                i == last,
                palette,
                self.ids.field,
                &view,
                &handlers,
                density,
                cx,
            ));
        }
        div()
            .w_full()
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.shell)
            .child(col)
            .into_any_element()
    }

    fn footer(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let on_back = Rc::clone(&self.on_back);
        let back = ghost_button_with_icon(Icon::ArrowBackUp, tr!("triggers_create_back"), palette)
            .on_click(
                self.ids.back,
                move |_: &ClickEvent, window: &mut Window, cx: &mut App| on_back(window, cx),
            );
        let cancel = secondary_button(tr!("triggers_create_cancel"), palette).on_click(
            self.ids.cancel,
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
        );
        let create = primary_button(tr!("triggers_create_btn"), palette)
            .disabled(!self.can_submit(cx))
            .on_click(
                self.ids.submit,
                cx.listener(|this, _: &ClickEvent, _, cx| this.request_submit(cx)),
            );
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(back)
            .child(div().flex_1())
            .child(cancel)
            .child(create)
            .into_any_element()
    }
}

fn section_label(label: SharedString, palette: &ForgePalette, cx: &App) -> AnyElement {
    div()
        .font_family(cx.mono_family())
        .text_size(FONT_XXS)
        .text_color(palette.text_muted)
        .child(label)
        .into_any_element()
}

impl Render for TriggerFillForm {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        let descriptor = self.registry.get(&self.kind_id);
        let dot_color = platform_dot_color(&self.kind_id, &self.registry, &palette);
        let glyph = descriptor
            .map(|d| Icon::from_name(d.icon_name()))
            .unwrap_or(Icon::Bolt);

        let name_section = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(section_label(
                tr!("triggers_create_section_name").into(),
                &palette,
                cx,
            ))
            .child(div().child(self.name_field.clone()));

        let config_section = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(section_label(
                tr!("triggers_create_section_config").into(),
                &palette,
                cx,
            ))
            .child(self.config_card(&palette, cx))
            .children(timer_field_hints(descriptor, &palette, density, cx));

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(name_section)
            .child(config_section);

        let card = modal(tr!("triggers_new_trigger"), body, &palette)
            .header_icon(glyph, dot_color)
            .subtitle(self.kind_line.clone())
            .size(ModalSize::Md)
            .footer(self.footer(&palette, cx))
            .on_close(
                self.ids.close,
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            );

        let view = cx.entity();
        div().absolute().top_0().left_0().size_full().child(
            overlay(card, &palette)
                .position(OverlayPosition::Center)
                .busy(self.saving)
                .on_dismiss(self.ids.scrim, move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel(cx));
                }),
        )
    }
}
