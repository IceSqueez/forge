use super::*;
use crate::async_bridge;
use crate::config_field_label::{config_field_labels, row_label};
use crate::config_form::{
    ChoiceDropdown, ChoiceSupport, CollectionChoices, ConfigField, ConfigFieldHandlers,
    FILL_VAL_FS, FoldContext, collect_field_values, fold_config_field, render_config_row,
    set_picked_value, set_slide_field, sparse_overrides, toggle_bool_field,
};
use crate::option_choices::ChoiceOptions;
use crate::repo_failure::RepoFailure;
use crate::trigger_form_shape::{trigger_form_defaults, trigger_form_specs};
use crate::triggers_screen::platform_dot_color;
use forge_components::ActivePresentation;
use forge_components::{
    BORDER_THIN, FONT_XXS, ForgePalette, Icon, InputEvent, ModalSize, OverlayPosition, Picker,
    PickerEvent, Radius, Spacing, TextInput, body_family, ghost_button_with_icon, modal,
    mono_family, overlay, primary_button, radius, secondary_button, spacing, tr,
};
use forge_types::TriggerInstance;
use gpui::{AnyElement, ClickEvent, Context, Entity, SharedString, WeakEntity, Window, div};

pub(super) struct TriggerFillDeps {
    pub(super) editor: WeakEntity<ActionEditor>,
    pub(super) trigger_registry: Arc<TriggerRegistry>,
    pub(super) actions_service: Arc<forge_runtime::ActionsService>,
    pub(super) rt_handle: tokio::runtime::Handle,
}

pub(super) enum TriggerFillEvent {
    Cancelled,
    SaveRequested(ActionId),
    Created,
}

pub(super) struct TriggerFillModal {
    deps: TriggerFillDeps,
    action_id: ActionId,
    kind_id: String,
    kind_label: String,
    name_field: Entity<TextInput>,
    fields: Vec<ConfigField>,
    labels: Vec<SharedString>,
    choices: CollectionChoices,
    saving: bool,
    _name_sub: Subscription,
}

impl EventEmitter<TriggerFillEvent> for TriggerFillModal {}

impl Render for TriggerFillModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        self.render_fill(&palette, cx)
    }
}

impl TriggerFillModal {
    pub(super) fn new(
        deps: TriggerFillDeps,
        action_id: ActionId,
        kind_id: String,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let descriptor = deps.trigger_registry.get(&kind_id);
        let kind_label = descriptor
            .map(crate::kind_copy::trigger_name)
            .unwrap_or_else(|| kind_id.clone());
        let default = trigger_form_defaults(descriptor);
        let specs = trigger_form_specs(descriptor);

        let fold = FoldContext {
            kind_id: &kind_id,
            config: &default,
            defaults: &default,
            palette: &palette,
            choices: ChoiceSupport::Text,
            on_committed: Self::on_trigger_config_committed,
        };
        let mut fields: Vec<ConfigField> = Vec::new();
        let mut labels: Vec<SharedString> = Vec::new();
        for spec in &specs {
            fold_config_field(spec, None, &fold, &mut fields, cx);
            config_field_labels(&kind_id, spec, &mut labels);
        }

        let name_field = cx.new(|cx| {
            TextInput::new(tr!("triggers_create_name_placeholder"), cx)
                .with_palette(palette)
                .static_chrome(palette.brand, Radius::Sm)
        });
        let name_sub = cx.subscribe(&name_field, Self::on_trigger_name_event);

        Self {
            deps,
            action_id,
            kind_id,
            kind_label,
            name_field,
            fields,
            labels,
            choices: CollectionChoices::for_specs(&specs),
            saving: false,
            _name_sub: name_sub,
        }
    }

    fn on_trigger_name_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Submitted(_) => self.request_save(cx),
            InputEvent::Cancelled => self.cancel(cx),
            InputEvent::Changed(_) => cx.notify(),
            InputEvent::Blurred(_) => {}
        }
    }

    fn on_trigger_config_committed(&mut self, event: &InputEvent, cx: &mut Context<Self>) {
        if let InputEvent::Submitted(_) = event {
            self.request_save(cx);
        }
    }

    fn toggle_trigger_config_field(&mut self, key: String, cx: &mut Context<Self>) {
        toggle_bool_field(&mut self.fields, &key);
        cx.notify();
    }

    fn slide_trigger_config_field(&mut self, key: String, next: i64, cx: &mut Context<Self>) {
        set_slide_field(&mut self.fields, &key, next);
        cx.notify();
    }

    fn pick_trigger_config_field(&mut self, key: String, choice: String, cx: &mut Context<Self>) {
        set_picked_value(&mut self.fields, &key, &choice, cx);
        cx.notify();
    }

    fn open_trigger_choice(&mut self, key: String, window: &mut Window, cx: &mut Context<Self>) {
        self.choices
            .toggle(&self.fields, key, Self::on_trigger_choice_event, window, cx);
        cx.notify();
    }

    fn close_trigger_choice(&mut self, cx: &mut Context<Self>) {
        self.choices.close();
        cx.notify();
    }

    fn on_trigger_choice_event(
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

    fn trigger_config_handlers(&self) -> ConfigFieldHandlers<Self> {
        ConfigFieldHandlers {
            toggle: Self::toggle_trigger_config_field,
            slide: Self::slide_trigger_config_field,
            pick: Self::pick_trigger_config_field,
            choice: Some(ChoiceDropdown {
                open: Self::open_trigger_choice,
                close: Self::close_trigger_choice,
                active: self.choices.active(),
            }),
        }
    }

    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(closed) = self
            .deps
            .editor
            .update(cx, |editor, cx| editor.open_trigger_picker(window, cx))
        {
            tracing::debug!(error = %closed, "the action editor closed under its trigger form");
        }
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(TriggerFillEvent::Cancelled);
    }

    pub(super) fn request_save(&mut self, cx: &mut Context<Self>) {
        if !self.saving && !self.name_field.read(cx).content().trim().is_empty() {
            cx.emit(TriggerFillEvent::SaveRequested(self.action_id));
        }
    }

    pub(super) fn save(&mut self, cx: &mut Context<Self>) {
        let name = self.name_field.read(cx).content().trim().to_owned();
        let kind_id = self.kind_id.clone();
        let default = trigger_form_defaults(self.deps.trigger_registry.get(&kind_id));
        let mut buffer = default.clone();
        collect_field_values(&self.fields, &mut buffer, cx);
        let overrides = sparse_overrides(&default, &buffer);

        let instance = TriggerInstance::new_user_defined(kind_id, name, overrides);

        self.saving = true;
        cx.notify();

        let action_id = self.action_id;
        let service = Arc::clone(&self.deps.actions_service);
        async_bridge::run_async(
            &self.deps.rt_handle,
            async move {
                service
                    .create_linked_trigger(action_id, instance)
                    .await
                    .map_err(RepoFailure::from)
            },
            |this, result: Result<(), RepoFailure>, cx| match result {
                Ok(()) => cx.emit(TriggerFillEvent::Created),
                Err(message) => {
                    this.saving = false;
                    on_repo_error(&message, cx);
                }
            },
            cx,
        );
    }

    pub(super) fn option_keys(&self) -> Vec<String> {
        self.choices.option_keys()
    }

    pub(super) fn refresh_choices(&mut self, options: &ChoiceOptions, cx: &mut Context<Self>) {
        let blank = tr!("config_form_choice_any");
        self.choices
            .refresh(&mut self.fields, options, Some(&blank));
        cx.notify();
    }

    fn render_fill(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let dot_color = platform_dot_color(&self.kind_id, &self.deps.trigger_registry, palette);
        let glyph = self
            .deps
            .trigger_registry
            .get(&self.kind_id)
            .map(|d| Icon::from_name(d.icon_name()))
            .unwrap_or(Icon::Bolt);

        let name_section = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(self.fill_section_label(tr!("triggers_create_section_name"), palette))
            .child(div().child(self.name_field.clone()));

        let config_card: AnyElement = if self.fields.is_empty() {
            div()
                .py(spacing(Spacing::Sm, density))
                .px(spacing(Spacing::Sm, density))
                .italic()
                .font_family(body_family())
                .text_size(FILL_VAL_FS)
                .text_color(palette.text_faint)
                .child(tr!("triggers_sheet_no_config"))
                .into_any_element()
        } else {
            let last = self.fields.len().saturating_sub(1);
            let view = cx.entity();
            let handlers = self.trigger_config_handlers();
            let mut col = div().flex().flex_col();
            for (i, field) in self.fields.iter().enumerate() {
                col = col.child(render_config_row(
                    field,
                    row_label(&self.labels, i, field),
                    i == last,
                    palette,
                    "actions-trigger-field",
                    &view,
                    &handlers,
                    density,
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
        };

        let config_section = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(self.fill_section_label(tr!("triggers_create_section_config"), palette))
            .child(config_card)
            .children(crate::triggers_screen::timer_field_hints(
                self.deps.trigger_registry.get(&self.kind_id),
                palette,
                density,
            ));

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(name_section)
            .child(config_section);

        let can_create = !self.name_field.read(cx).content().trim().is_empty() && !self.saving;

        let back = ghost_button_with_icon(Icon::ArrowBackUp, tr!("triggers_create_back"), palette)
            .on_click(
                "actions-trigger-fill-back",
                cx.listener(|this, _: &ClickEvent, window, cx| this.back(window, cx)),
            );
        let cancel = secondary_button(tr!("triggers_create_cancel"), palette).on_click(
            "actions-trigger-fill-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
        );
        let create = primary_button(tr!("triggers_create_btn"), palette)
            .disabled(!can_create)
            .on_click(
                "actions-trigger-fill-submit",
                cx.listener(|this, _: &ClickEvent, _, cx| this.request_save(cx)),
            );
        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(back)
            .child(div().flex_1())
            .child(cancel)
            .child(create);

        let card = modal(
            tr!(
                "triggers_create_new_instance",
                kind = self.kind_label.as_str()
            ),
            body,
            palette,
        )
        .header_icon(glyph, dot_color)
        .subtitle(self.kind_id.clone())
        .size(ModalSize::Md)
        .footer(footer)
        .on_close(
            "actions-trigger-fill-close",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
        );

        let view = cx.entity();
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(
                overlay(card, palette)
                    .position(OverlayPosition::Center)
                    .busy(self.saving)
                    .on_dismiss("actions-trigger-fill-scrim", move |_window, cx| {
                        view.update(cx, |this, cx| this.cancel(cx));
                    }),
            )
            .into_any_element()
    }

    fn fill_section_label(
        &self,
        label: impl Into<SharedString>,
        palette: &ForgePalette,
    ) -> AnyElement {
        div()
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_muted)
            .child(label.into())
            .into_any_element()
    }
}
