use super::*;
use crate::async_bridge;
use crate::config_field_label::{config_field_labels, row_label};
use crate::config_form::{
    ChoiceDropdown, ChoiceSupport, CollectionChoices, ConfigField, ConfigFieldHandlers,
    FILL_VAL_FS, FoldContext, collect_field_values, fold_config_field, render_config_row,
    set_picked_value, sparse_overrides,
};
use crate::presentation::ActivePresentation;
use crate::triggers_screen::platform_dot_color;
use forge_components::{
    BORDER_THIN, Density, FONT_XXS, ForgePalette, Icon, InputEvent, ModalSize, OverlayPosition,
    Picker, PickerEvent, Radius, Spacing, TextInput, body_family, ghost_button_with_icon, modal,
    mono_family, overlay, primary_button, radius, secondary_button, spacing, tr,
};
use forge_types::{PermissionRung, PlatformScope, TriggerInstance, TriggerInstanceId};
use gpui::{AnyElement, ClickEvent, Context, Entity, SharedString, Window, div};

impl ScreenActionsView {
    pub(super) fn enter_trigger_fill(
        &mut self,
        action_id: ActionId,
        kind_id: String,
        cx: &mut Context<Self>,
    ) {
        let palette = cx.palette();
        let descriptor = self.trigger_registry.get(&kind_id);
        let kind_label = descriptor
            .map(|d| d.label().to_owned())
            .unwrap_or_else(|| kind_id.clone());
        let default = descriptor.map(|d| d.default_config()).unwrap_or_default();
        let specs = descriptor.map(|d| d.config_fields()).unwrap_or_default();

        let fold = FoldContext {
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
            config_field_labels(spec, &mut labels);
        }

        let name_field = cx.new(|cx| {
            TextInput::new(tr!("triggers_create_name_placeholder"), cx)
                .with_palette(palette)
                .static_chrome(palette.brand, Radius::Sm)
        });
        let name_sub = cx.subscribe(&name_field, Self::on_trigger_name_event);

        self.add_trigger = Some(AddTriggerStage::Fill(AddTriggerFill {
            action_id,
            kind_id,
            kind_label,
            name_field,
            fields,
            labels,
            choices: CollectionChoices::for_specs(&specs),
            saving: false,
            _name_sub: name_sub,
        }));
        self.start_collection_options(cx);
        cx.notify();
    }

    fn on_trigger_name_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Submitted(_) => self.submit_trigger_fill(cx),
            InputEvent::Cancelled => self.cancel_trigger_picker(cx),
            InputEvent::Changed(_) => cx.notify(),
            InputEvent::Blurred(_) => {}
        }
    }

    fn on_trigger_config_committed(&mut self, event: &InputEvent, cx: &mut Context<Self>) {
        if let InputEvent::Submitted(_) = event {
            self.submit_trigger_fill(cx);
        }
    }

    fn toggle_trigger_config_field(&mut self, key: String, cx: &mut Context<Self>) {
        if let Some(AddTriggerStage::Fill(form)) = self.add_trigger.as_mut() {
            for field in &mut form.fields {
                if let ConfigField::Bool { key: k, value, .. } = field
                    && *k == key
                {
                    *value = !*value;
                }
            }
        }
        cx.notify();
    }

    fn slide_trigger_config_field(&mut self, key: String, next: i64, cx: &mut Context<Self>) {
        if let Some(AddTriggerStage::Fill(form)) = self.add_trigger.as_mut() {
            for field in &mut form.fields {
                if let ConfigField::Slide { key: k, value, .. } = field
                    && *k == key
                {
                    *value = next;
                }
            }
        }
        cx.notify();
    }

    fn pick_trigger_config_field(&mut self, key: String, choice: String, cx: &mut Context<Self>) {
        if let Some(AddTriggerStage::Fill(form)) = self.add_trigger.as_mut() {
            set_picked_value(&mut form.fields, &key, &choice, cx);
        }
        cx.notify();
    }

    fn open_trigger_choice(&mut self, key: String, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(AddTriggerStage::Fill(form)) = self.add_trigger.as_mut() {
            form.choices
                .toggle(&form.fields, key, Self::on_trigger_choice_event, window, cx);
        }
        cx.notify();
    }

    fn close_trigger_choice(&mut self, cx: &mut Context<Self>) {
        if let Some(AddTriggerStage::Fill(form)) = self.add_trigger.as_mut() {
            form.choices.close();
        }
        cx.notify();
    }

    fn on_trigger_choice_event(
        &mut self,
        _picker: Entity<Picker>,
        event: &PickerEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(AddTriggerStage::Fill(form)) = self.add_trigger.as_mut() else {
            return;
        };
        match event {
            PickerEvent::Selected(value) => {
                if let Some(key) = form.choices.take_open_key() {
                    set_picked_value(&mut form.fields, &key, value, cx);
                }
            }
            PickerEvent::Cancelled => form.choices.close(),
        }
        cx.notify();
    }

    fn trigger_config_handlers(form: &AddTriggerFill) -> ConfigFieldHandlers<Self> {
        ConfigFieldHandlers {
            toggle: Self::toggle_trigger_config_field,
            slide: Self::slide_trigger_config_field,
            pick: Self::pick_trigger_config_field,
            choice: Some(ChoiceDropdown {
                open: Self::open_trigger_choice,
                close: Self::close_trigger_choice,
                active: form.choices.active(),
            }),
        }
    }

    fn back_to_trigger_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_trigger_picker(window, cx);
    }

    fn submit_trigger_fill(&mut self, cx: &mut Context<Self>) {
        let Some(AddTriggerStage::Fill(form)) = self.add_trigger.as_ref() else {
            return;
        };
        if form.saving {
            return;
        }
        let name = form.name_field.read(cx).content().trim().to_owned();
        if name.is_empty() {
            return;
        }
        let action_id = form.action_id;
        if self.selected != Some(action_id) {
            self.add_trigger = None;
            cx.notify();
            return;
        }
        let kind_id = form.kind_id.clone();
        let default = self
            .trigger_registry
            .get(&kind_id)
            .map(|d| d.default_config())
            .unwrap_or_default();
        let mut buffer = default.clone();
        collect_field_values(&form.fields, &mut buffer, cx);
        let overrides = sparse_overrides(&default, &buffer);

        let new_id = TriggerInstanceId::new();
        let instance = TriggerInstance {
            id: new_id,
            kind_id,
            name,
            overrides,
            enabled: true,
            user_defined: true,
            platform_scope: PlatformScope::Any,
            cooldown_secs: 0,
            cooldown_global: true,
            permission_rung: PermissionRung::Everyone,
        };

        if let Some(AddTriggerStage::Fill(form)) = self.add_trigger.as_mut() {
            form.saving = true;
        }
        cx.notify();

        let repo = Arc::clone(&self.trigger_instance_repo);
        let service = Arc::clone(&self.actions_service);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                repo.save(&instance).await.map_err(|e| e.to_string())?;
                service
                    .link_trigger_instance(action_id, new_id)
                    .await
                    .map_err(|e| e.to_string())
            },
            |this, result: Result<(), String>, cx| match result {
                Ok(()) => {
                    this.add_trigger = None;
                    this.reload_detail(cx);
                    cx.notify();
                }
                Err(message) => {
                    if let Some(AddTriggerStage::Fill(form)) = this.add_trigger.as_mut() {
                        form.saving = false;
                    }
                    this.on_repo_error(&message, cx);
                }
            },
            cx,
        );
    }

    pub(super) fn render_trigger_fill(
        &self,
        form: &AddTriggerFill,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let dot_color = platform_dot_color(&form.kind_id, palette);
        let glyph = self
            .trigger_registry
            .get(&form.kind_id)
            .map(|d| Icon::from_name(d.icon_name()))
            .unwrap_or(Icon::Bolt);

        let name_section = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(self.fill_section_label(tr!("triggers_create_section_name"), palette))
            .child(div().child(form.name_field.clone()));

        let config_card: AnyElement = if form.fields.is_empty() {
            div()
                .py(spacing(Spacing::Sm, Density::Cozy))
                .px(spacing(Spacing::Sm, Density::Cozy))
                .italic()
                .font_family(body_family())
                .text_size(FILL_VAL_FS)
                .text_color(palette.text_faint)
                .child(tr!("triggers_sheet_no_config"))
                .into_any_element()
        } else {
            let last = form.fields.len().saturating_sub(1);
            let view = cx.entity();
            let handlers = Self::trigger_config_handlers(form);
            let mut col = div().flex().flex_col();
            for (i, field) in form.fields.iter().enumerate() {
                col = col.child(render_config_row(
                    field,
                    row_label(&form.labels, i, field),
                    i == last,
                    palette,
                    "actions-trigger-field",
                    &view,
                    &handlers,
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
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(self.fill_section_label(tr!("triggers_create_section_config"), palette))
            .child(config_card)
            .children(crate::triggers_screen::timer_field_hints(
                &form.kind_id,
                palette,
            ));

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, Density::Cozy))
            .child(name_section)
            .child(config_section);

        let can_create = !form.name_field.read(cx).content().trim().is_empty() && !form.saving;

        let back = ghost_button_with_icon(Icon::ArrowBackUp, tr!("triggers_create_back"), palette)
            .on_click(
                "actions-trigger-fill-back",
                cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.back_to_trigger_picker(window, cx)
                }),
            );
        let cancel = secondary_button(tr!("triggers_create_cancel"), palette).on_click(
            "actions-trigger-fill-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_trigger_picker(cx)),
        );
        let create = primary_button(tr!("triggers_create_btn"), palette)
            .disabled(!can_create)
            .on_click(
                "actions-trigger-fill-submit",
                cx.listener(|this, _: &ClickEvent, _, cx| this.submit_trigger_fill(cx)),
            );
        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(back)
            .child(div().flex_1())
            .child(cancel)
            .child(create);

        let card = modal(
            tr!(
                "triggers_create_new_instance",
                kind = form.kind_label.as_str()
            ),
            body,
            palette,
        )
        .header_icon(glyph, dot_color)
        .subtitle(form.kind_id.clone())
        .size(ModalSize::Md)
        .footer(footer)
        .on_close(
            "actions-trigger-fill-close",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_trigger_picker(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .busy(form.saving)
            .on_dismiss("actions-trigger-fill-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_trigger_picker(cx));
            })
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
