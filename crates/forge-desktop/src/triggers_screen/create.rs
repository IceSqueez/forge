use super::config_form::{ConfigField, fold_config_field, overlay_field_values, sparse_overrides};
use super::{TriggerInstanceRow, TriggersRegistryView, load_rows, platform_dot_color};
use crate::presentation::ActivePresentation;
use forge_components::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_XXS, ForgePalette,
    GridPicker, GridPickerConfig, GridPickerEvent, GridPickerGroup, GridPickerItem,
    GridPickerItemState, GridPickerSubtitle, Icon, InputEvent, ModalSize, OverlayPosition, Radius,
    Spacing, TextInput, ghost_button_with_icon, modal, overlay, primary_button, radius,
    secondary_button, spacing, toggle, tr,
};
use forge_registry::{TriggerCategory, TriggerKindDescriptor, TriggerRegistry};
use forge_types::{PlatformScope, TriggerInstance, TriggerInstanceId};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, SharedString, Subscription, Window, div, prelude::*,
    px,
};
use std::collections::HashMap;
use std::sync::Arc;

const FILL_KEY_W: gpui::Pixels = px(110.0);
const FILL_KEY_FS: gpui::Pixels = px(11.0);
const FILL_VAL_FS: gpui::Pixels = px(11.5);
const FILL_ROW_PAD_V: gpui::Pixels = px(8.0);
const FILL_ROW_PAD_H: gpui::Pixels = px(12.0);

pub(super) enum CreateStage {
    PickKind(KindPickerForm),
    Fill(CreateFillForm),
}

pub(super) struct KindPickerForm {
    picker: Entity<GridPicker>,
    /// Card id → `kind_id`.
    picks: HashMap<SharedString, String>,
    _sub: Subscription,
}

pub(super) struct CreateFillForm {
    kind_id: String,
    kind_label: String,
    name_field: Entity<TextInput>,
    fields: Vec<ConfigField>,
    saving: bool,
    _name_sub: Subscription,
}

impl TriggersRegistryView {
    pub(super) fn open_create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let palette = cx.palette();
        let (groups, picks) = build_kind_groups(&self.registry, &palette);
        let count: usize = groups.iter().map(|g| g.items.len()).sum();
        let config = GridPickerConfig {
            accent: palette.brand,
            header_icon: Icon::Bolt,
            title: tr!("triggers_new_trigger").into(),
            subtitle: GridPickerSubtitle::Plain(
                tr!("triggers_create_type_count", count = count as i64).into(),
            ),
            footer_hint: tr!("triggers_create_footer_hint").into(),
            search_placeholder: tr!("triggers_create_search_types", count = count as i64).into(),
            scope_cap: Some(8),
        };
        let picker = cx.new(|cx| GridPicker::new(config, groups, palette, cx));
        let sub = cx.subscribe(&picker, Self::on_create_picker_event);
        picker.read(cx).focus(window, cx);
        self.menu_open = None;
        self.create = Some(CreateStage::PickKind(KindPickerForm {
            picker,
            picks,
            _sub: sub,
        }));
        cx.notify();
    }

    fn on_create_picker_event(
        &mut self,
        _picker: Entity<GridPicker>,
        event: &GridPickerEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            GridPickerEvent::Picked(id) => {
                let kind_id = match self.create.as_ref() {
                    Some(CreateStage::PickKind(form)) => form.picks.get(id).cloned(),
                    _ => None,
                };
                if let Some(kind_id) = kind_id {
                    self.enter_fill(kind_id, cx);
                }
            }
            GridPickerEvent::Dismissed => self.cancel_create(cx),
        }
    }

    fn enter_fill(&mut self, kind_id: String, cx: &mut Context<Self>) {
        let palette = cx.palette();
        let descriptor = self.registry.get(&kind_id);
        let kind_label = descriptor
            .map(|d| d.label().to_owned())
            .unwrap_or_else(|| kind_id.clone());
        let default = descriptor.map(|d| d.default_config()).unwrap_or_default();
        let specs = descriptor.map(|d| d.config_fields()).unwrap_or_default();

        let mut fields: Vec<ConfigField> = Vec::new();
        for spec in &specs {
            fold_config_field(
                spec,
                None,
                &default,
                &palette,
                Self::on_create_config_committed,
                &mut fields,
                cx,
            );
        }

        let name_field = cx.new(|cx| {
            TextInput::new(tr!("triggers_create_name_placeholder"), cx)
                .with_palette(palette)
                .static_chrome(palette.brand, Radius::Sm)
        });
        let name_sub = cx.subscribe(&name_field, Self::on_create_name_event);

        self.create = Some(CreateStage::Fill(CreateFillForm {
            kind_id,
            kind_label,
            name_field,
            fields,
            saving: false,
            _name_sub: name_sub,
        }));
        cx.notify();
    }

    fn on_create_name_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Submitted(_) => self.submit_create(cx),
            InputEvent::Cancelled => self.cancel_create(cx),
            InputEvent::Changed(_) => cx.notify(),
        }
    }

    fn on_create_config_committed(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if let InputEvent::Submitted(_) = event {
            self.submit_create(cx);
        }
    }

    fn toggle_create_config_field(&mut self, key: String, cx: &mut Context<Self>) {
        if let Some(CreateStage::Fill(form)) = self.create.as_mut() {
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

    fn back_to_kind_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_create(window, cx);
    }

    pub(super) fn cancel_create(&mut self, cx: &mut Context<Self>) {
        self.create = None;
        cx.notify();
    }

    fn submit_create(&mut self, cx: &mut Context<Self>) {
        let Some(CreateStage::Fill(form)) = self.create.as_ref() else {
            return;
        };
        if form.saving {
            return;
        }
        let name = form.name_field.read(cx).content().trim().to_owned();
        if name.is_empty() {
            return;
        }
        let kind_id = form.kind_id.clone();
        let default = self
            .registry
            .get(&kind_id)
            .map(|d| d.default_config())
            .unwrap_or_default();
        let mut buffer = default.clone();
        overlay_field_values(&form.fields, &mut buffer, cx);
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
        };

        if let Some(CreateStage::Fill(form)) = self.create.as_mut() {
            form.saving = true;
        }
        cx.notify();

        let repo = Arc::clone(&self.repo);
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<Vec<TriggerInstanceRow>, String>>();
        self.rt_handle.spawn(async move {
            let outcome = async {
                repo.save(&instance).await.map_err(|e| e.to_string())?;
                load_rows(&*repo).await
            }
            .await;
            let _ = tx.send(outcome);
        });
        cx.spawn(async move |this, cx| match rx.await {
            Ok(Ok(rows)) => {
                let _ = this.update(cx, |this, cx| {
                    this.create = None;
                    this.apply_rows(rows, cx);
                    this.selected = Some(new_id);
                    this.detail = None;
                    this.load_detail(new_id, cx);
                    cx.notify();
                });
            }
            Ok(Err(message)) => {
                let _ = this.update(cx, |this, cx| {
                    if let Some(CreateStage::Fill(form)) = this.create.as_mut() {
                        form.saving = false;
                    }
                    this.on_repo_error(&message, cx);
                });
            }
            Err(_) => {}
        })
        .detach();
    }

    pub(super) fn render_create(
        &self,
        stage: &CreateStage,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match stage {
            CreateStage::PickKind(form) => {
                let view = cx.entity();
                overlay(form.picker.clone(), palette)
                    .position(OverlayPosition::Center)
                    .on_dismiss("triggers-create-kind-scrim", move |_window, cx| {
                        view.update(cx, |this, cx| this.cancel_create(cx));
                    })
                    .into_any_element()
            }
            CreateStage::Fill(form) => self.render_fill_form(form, palette, cx),
        }
    }

    fn render_fill_form(
        &self,
        form: &CreateFillForm,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let dot_color = platform_dot_color(&form.kind_id, palette);
        let glyph = self
            .registry
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
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FILL_VAL_FS)
                .text_color(palette.text_faint)
                .child(tr!("triggers_sheet_no_config"))
                .into_any_element()
        } else {
            let last = form.fields.len().saturating_sub(1);
            let mut col = div().flex().flex_col();
            for (i, field) in form.fields.iter().enumerate() {
                col = col.child(self.render_fill_config_row(field, i == last, palette, cx));
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
            .child(config_card);

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, Density::Cozy))
            .child(name_section)
            .child(config_section);

        let can_create = !form.name_field.read(cx).content().trim().is_empty() && !form.saving;

        let back = ghost_button_with_icon(Icon::ArrowBackUp, tr!("triggers_create_back"), palette)
            .on_click(
                "triggers-create-back",
                cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.back_to_kind_picker(window, cx)
                }),
            );
        let cancel = secondary_button(tr!("triggers_create_cancel"), palette).on_click(
            "triggers-create-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_create(cx)),
        );
        let create = primary_button(tr!("triggers_create_btn"), palette)
            .disabled(!can_create)
            .on_click(
                "triggers-create-submit",
                cx.listener(|this, _: &ClickEvent, _, cx| this.submit_create(cx)),
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
        .kbd_hint(tr!("triggers_create_kbd_hint"))
        .on_close(
            "triggers-create-close",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_create(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("triggers-create-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_create(cx));
            })
            .into_any_element()
    }

    fn fill_section_label(
        &self,
        label: impl Into<SharedString>,
        palette: &ForgePalette,
    ) -> AnyElement {
        div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XXS)
            .text_color(palette.text_muted)
            .child(label.into())
            .into_any_element()
    }

    fn render_fill_config_row(
        &self,
        field: &ConfigField,
        last: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = match field {
            ConfigField::Input { key, .. }
            | ConfigField::Bool { key, .. }
            | ConfigField::Hint { key } => key.clone(),
        };

        let label = div()
            .w(FILL_KEY_W)
            .flex_none()
            .overflow_hidden()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FILL_KEY_FS)
            .text_color(palette.text_muted)
            .child(key.clone());

        let value: AnyElement = match field {
            ConfigField::Input { input, .. } => div().child(input.clone()).into_any_element(),
            ConfigField::Bool { key, value, .. } => {
                let toggle_key = key.clone();
                toggle(*value, palette)
                    .on_click(
                        SharedString::from(format!("triggers-create-toggle-{key}")),
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.toggle_create_config_field(toggle_key.clone(), cx)
                        }),
                    )
                    .into_any_element()
            }
            ConfigField::Hint { .. } => div()
                .italic()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FILL_VAL_FS)
                .text_color(palette.text_faint)
                .child(tr!("triggers_sheet_config_authored"))
                .into_any_element(),
        };

        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .py(FILL_ROW_PAD_V)
            .px(FILL_ROW_PAD_H)
            .when(!last, |row| {
                row.border_b(BORDER_THIN)
                    .border_color(palette.border_regular)
            })
            .child(label)
            .child(div().flex_1().min_w(px(0.0)).child(value))
            .into_any_element()
    }
}

fn build_kind_groups(
    registry: &TriggerRegistry,
    palette: &ForgePalette,
) -> (Vec<GridPickerGroup>, HashMap<SharedString, String>) {
    let mut groups: Vec<GridPickerGroup> = Vec::new();
    let mut picks: HashMap<SharedString, String> = HashMap::new();

    for cat in CATEGORY_ORDER {
        let (label, slug, color) = category_meta(cat, palette);
        let mut descs: Vec<&dyn TriggerKindDescriptor> =
            registry.all().filter(|d| d.category() == cat).collect();
        if descs.is_empty() {
            continue;
        }
        descs.sort_by(|a, b| a.label().cmp(b.label()));

        let mut items = Vec::with_capacity(descs.len());
        for d in descs {
            let id = SharedString::from(format!("kind-{}", d.id()));
            picks.insert(id.clone(), d.id().to_owned());
            items.push(GridPickerItem {
                id,
                icon: Icon::from_name(d.icon_name()),
                icon_color: color,
                name: d.label().to_string().into(),
                desc: d.summary().to_string().into(),
                state: GridPickerItemState::Normal,
            });
        }
        groups.push(GridPickerGroup {
            label,
            dot_color: color,
            scope: SharedString::from(slug),
            items,
        });
    }

    (groups, picks)
}

const CATEGORY_ORDER: [TriggerCategory; 23] = [
    TriggerCategory::Chat,
    TriggerCategory::Subscriptions,
    TriggerCategory::Bits,
    TriggerCategory::Raids,
    TriggerCategory::ChannelPoints,
    TriggerCategory::Polls,
    TriggerCategory::Predictions,
    TriggerCategory::Hype,
    TriggerCategory::Charity,
    TriggerCategory::Goals,
    TriggerCategory::Clips,
    TriggerCategory::Streams,
    TriggerCategory::Users,
    TriggerCategory::Moderation,
    TriggerCategory::Obs,
    TriggerCategory::VTube,
    TriggerCategory::Discord,
    TriggerCategory::Midi,
    TriggerCategory::Hotkey,
    TriggerCategory::Timer,
    TriggerCategory::Server,
    TriggerCategory::Core,
    TriggerCategory::Ungrouped,
];

fn category_meta(
    cat: TriggerCategory,
    palette: &ForgePalette,
) -> (SharedString, &'static str, gpui::Rgba) {
    match cat {
        TriggerCategory::Chat => (tr!("trigger_cat_chat").into(), "chat", palette.info),
        TriggerCategory::Subscriptions => (
            tr!("trigger_cat_subscriptions").into(),
            "subs",
            palette.brand,
        ),
        TriggerCategory::Bits => (tr!("trigger_cat_bits").into(), "bits", palette.bits),
        TriggerCategory::Raids => (tr!("trigger_cat_raids").into(), "raids", palette.brand),
        TriggerCategory::Moderation => (
            tr!("trigger_cat_moderation").into(),
            "moderation",
            palette.random,
        ),
        TriggerCategory::ChannelPoints => (
            tr!("trigger_cat_channel_points").into(),
            "points",
            palette.accent_pink_light,
        ),
        TriggerCategory::Polls => (tr!("trigger_cat_polls").into(), "polls", palette.warning),
        TriggerCategory::Predictions => (
            tr!("trigger_cat_predictions").into(),
            "predictions",
            palette.warning,
        ),
        TriggerCategory::Hype => (tr!("trigger_cat_hype").into(), "hype", palette.brand),
        TriggerCategory::Charity => (
            tr!("trigger_cat_charity").into(),
            "charity",
            palette.success,
        ),
        TriggerCategory::Goals => (tr!("trigger_cat_goals").into(), "goals", palette.success),
        TriggerCategory::Clips => (tr!("trigger_cat_clips").into(), "clips", palette.info),
        TriggerCategory::Streams => (tr!("trigger_cat_streams").into(), "streams", palette.info),
        TriggerCategory::Users => (tr!("trigger_cat_users").into(), "users", palette.info),
        TriggerCategory::Obs => ("OBS".into(), "obs", palette.accent_teal),
        TriggerCategory::VTube => ("VTube Studio".into(), "vtube", palette.accent_teal),
        TriggerCategory::Discord => ("Discord".into(), "discord", palette.info),
        TriggerCategory::Midi => ("MIDI".into(), "midi", palette.random),
        TriggerCategory::Hotkey => (
            tr!("triggers_filter_hotkey").into(),
            "hotkey",
            palette.warning,
        ),
        TriggerCategory::Core => (tr!("trigger_cat_core").into(), "core", palette.info),
        TriggerCategory::Server => (
            tr!("triggers_create_cat_server").into(),
            "server",
            palette.info,
        ),
        TriggerCategory::Timer => (
            tr!("triggers_create_cat_timer").into(),
            "timer",
            palette.warning,
        ),
        TriggerCategory::Ungrouped => {
            (tr!("trigger_cat_other").into(), "other", palette.text_muted)
        }
    }
}
