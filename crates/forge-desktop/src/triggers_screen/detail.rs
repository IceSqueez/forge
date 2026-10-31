use super::permission_help::{gated_platforms, rung_meanings};
use super::*;
use crate::async_bridge::{self, WriteOutcome};
use crate::config_field_label::{config_field_labels, row_label};
use crate::config_form::INTEGER_PLACEHOLDER;
use crate::config_form::{
    ChoiceDropdown, ChoiceSupport, CollectionChoices, ConfigFieldHandlers, FoldContext,
    collect_field_values, config_label_cell, fold_config_field, render_config_control,
    set_picked_value, sparse_overrides,
};
use crate::integration_switch::{category_inactive_badge, inactive_badge};
use crate::repo_failure::RepoFailure;
use crate::trigger_form_shape::{trigger_form_defaults, trigger_form_specs};
use forge_components::ActivePresentation;
use forge_components::FONT_LABEL;
use forge_components::FONT_META;
use forge_components::FONT_SM;
use forge_components::FONT_XS;
use forge_components::ROW_OFF_OPACITY;
use forge_components::{
    Density, FONT_XXS, Icon, InputEvent, Picker, PickerEvent, Radius, ResizeEdge, ResizeRange,
    Spacing, TextInput, body_family, ghost_button_with_icon, icon, install_resize, mono_family,
    primary_button, radius, row_card, scaled, segment, segmented, spacing, status_dot, toggle, tr,
};
use forge_registry::effective_config;
use gpui::{AnyElement, ClickEvent, FontWeight, SharedString};
use std::collections::HashMap;

pub(super) const DETAIL_SHEET_W: Pixels = px(420.0);
const DETAIL_SHEET_MIN: Pixels = px(320.0);
const DETAIL_SHEET_MAX: Pixels = px(640.0);
const TILE: Pixels = px(30.0);
const TILE_RADIUS: Pixels = px(8.0);
const TILE_GLYPH: Pixels = px(16.0);
const KIND_DOT: Pixels = px(5.0);
const ROW_GLYPH: Pixels = px(12.0);
const NAME_FS: Pixels = FONT_SM;
const KIND_FS: Pixels = FONT_XXS;
const CFG_KEY_FS: Pixels = FONT_META;
const CFG_VAL_FS: Pixels = FONT_LABEL;
const USED_FS: Pixels = FONT_XS;
const CFG_KEY_W: Pixels = px(110.0);
const REVERT_W: Pixels = px(22.0);
struct TriggerDetailResizeDrag;

const HEADER_PAD_V: Pixels = px(12.0);
const BODY_PAD_V: Pixels = px(14.0);
const CFG_ROW_PAD_V: Pixels = px(8.0);
const CFG_ROW_PAD_H: Pixels = px(12.0);
const USED_ROW_PAD_V: Pixels = px(7.0);
const USED_ROW_PAD_H: Pixels = px(10.0);
const PLACEHOLDER_PAD_V: Pixels = px(14.0);
const PLACEHOLDER_PAD_H: Pixels = px(12.0);

#[derive(Clone, Copy, PartialEq, Eq)]
enum DetailAfterSave {
    Keep,
    Rebuild,
}

impl TriggersRegistryView {
    pub(super) fn load_detail(&self, id: TriggerInstanceId, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.repo);
        let action_repo = Arc::clone(&self.action_repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move { load_detail_data(&*repo, &*action_repo, id).await },
            move |this, result, cx| match result {
                Ok(Some(data)) => this.apply_detail(id, data, cx),
                Ok(None) => {
                    if this.selected == Some(id) {
                        this.detail = None;
                        cx.notify();
                    }
                }
                Err(failure) => this.on_repo_error(&failure, cx),
            },
            cx,
        );
    }

    fn apply_detail(
        &mut self,
        id: TriggerInstanceId,
        data: TriggerDetailData,
        cx: &mut Context<Self>,
    ) {
        if self.selected != Some(id) {
            return;
        }
        let palette = cx.palette();
        let descriptor = self.registry.get(&data.instance.kind_id);
        let default = trigger_form_defaults(descriptor);
        let specs = trigger_form_specs(descriptor);
        let effective = effective_config(&default, &data.instance.overrides);

        let fold = FoldContext {
            kind_id: &data.instance.kind_id,
            config: &effective,
            defaults: &default,
            palette: &palette,
            choices: ChoiceSupport::Text,
            on_committed: Self::on_config_committed,
        };
        let mut fields: Vec<ConfigField> = Vec::new();
        let mut labels: Vec<SharedString> = Vec::new();
        for spec in &specs {
            fold_config_field(spec, None, &fold, &mut fields, cx);
            config_field_labels(&data.instance.kind_id, spec, &mut labels);
        }

        let cooldown_per_user = !data.instance.cooldown_global;
        let cooldown_seed = data.instance.cooldown_secs;
        let cooldown_input = self.build_cooldown_input(cooldown_seed, palette, cx);
        let cooldown_sub = cx.subscribe(&cooldown_input, Self::on_cooldown_committed);
        let permission_rung = data.instance.permission_rung;

        self.detail = Some(TriggerDetail {
            instance: data.instance,
            fields,
            labels,
            used_in: data.used_in,
            cooldown_input,
            cooldown_per_user,
            permission_rung,
            choices: CollectionChoices::for_specs(&specs),
            _cooldown_sub: cooldown_sub,
        });
        self.start_collection_options(cx);
        cx.notify();
    }

    pub(super) fn close_detail(&mut self, cx: &mut Context<Self>) {
        self.selected = None;
        self.detail = None;
        cx.notify();
    }

    fn on_config_committed(&mut self, event: &InputEvent, cx: &mut Context<Self>) {
        if matches!(event, InputEvent::Submitted(_) | InputEvent::Blurred(_)) {
            self.commit_config(cx);
        }
    }

    fn on_cooldown_committed(
        &mut self,
        field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Submitted(_) | InputEvent::Blurred(_) => self.commit_config(cx),
            InputEvent::Cancelled => {
                field.update(cx, |input, cx| input.restore_committed(cx));
            }
            InputEvent::Changed(_) => {}
        }
    }

    fn build_cooldown_input(
        &self,
        seed: u32,
        palette: ForgePalette,
        cx: &mut Context<Self>,
    ) -> Entity<TextInput> {
        cx.new(|cx| {
            let mut input = TextInput::new(INTEGER_PLACEHOLDER, cx).with_palette(palette);
            if seed > 0 {
                input.set_content(seed.to_string(), cx);
            }
            input
        })
    }

    fn toggle_config_field(&mut self, key: String, cx: &mut Context<Self>) {
        if let Some(detail) = self.detail.as_mut() {
            for field in &mut detail.fields {
                if let ConfigField::Bool { key: k, value, .. } = field
                    && *k == key
                {
                    *value = !*value;
                }
            }
        }
        self.commit_config(cx);
    }

    fn slide_config_field(&mut self, key: String, next: i64, cx: &mut Context<Self>) {
        if let Some(detail) = self.detail.as_mut() {
            for field in &mut detail.fields {
                if let ConfigField::Slide { key: k, value, .. } = field
                    && *k == key
                {
                    *value = next;
                }
            }
        }
        self.commit_config(cx);
    }

    fn pick_config_field(&mut self, key: String, choice: String, cx: &mut Context<Self>) {
        if let Some(detail) = self.detail.as_mut() {
            set_picked_value(&mut detail.fields, &key, &choice, cx);
        }
        self.commit_config(cx);
    }

    fn open_detail_choice(&mut self, key: String, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(detail) = self.detail.as_mut() {
            detail.choices.toggle(
                &detail.fields,
                key,
                Self::on_detail_choice_event,
                window,
                cx,
            );
        }
        cx.notify();
    }

    fn close_detail_choice(&mut self, cx: &mut Context<Self>) {
        if let Some(detail) = self.detail.as_mut() {
            detail.choices.close();
        }
        cx.notify();
    }

    fn on_detail_choice_event(
        &mut self,
        _picker: Entity<Picker>,
        event: &PickerEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(detail) = self.detail.as_mut() else {
            return;
        };
        match event {
            PickerEvent::Selected(value) => {
                if let Some(key) = detail.choices.take_open_key() {
                    self.pick_config_field(key, value.to_string(), cx);
                }
            }
            PickerEvent::Cancelled => {
                detail.choices.close();
                cx.notify();
            }
        }
    }

    fn detail_config_handlers(&self) -> ConfigFieldHandlers<Self> {
        ConfigFieldHandlers {
            toggle: Self::toggle_config_field,
            slide: Self::slide_config_field,
            pick: Self::pick_config_field,
            choice: Some(ChoiceDropdown {
                open: Self::open_detail_choice,
                close: Self::close_detail_choice,
                active: self
                    .detail
                    .as_ref()
                    .and_then(|detail| detail.choices.active()),
            }),
        }
    }

    fn toggle_cooldown_scope(&mut self, cx: &mut Context<Self>) {
        if let Some(detail) = self.detail.as_mut() {
            detail.cooldown_per_user = !detail.cooldown_per_user;
        }
        self.commit_config(cx);
    }

    fn set_permission_rung(&mut self, rung: PermissionRung, cx: &mut Context<Self>) {
        let Some(detail) = self.detail.as_mut() else {
            return;
        };
        if detail.permission_rung == rung {
            return;
        }
        detail.permission_rung = rung;
        self.commit_config(cx);
        cx.notify();
    }

    fn pending_instance(&self, cx: &App) -> Option<TriggerInstance> {
        let detail = self.detail.as_ref()?;
        let default = trigger_form_defaults(self.registry.get(&detail.instance.kind_id));
        let mut buffer = effective_config(&default, &detail.instance.overrides);
        collect_field_values(&detail.fields, &mut buffer, cx);

        let mut instance = detail.instance.clone();
        instance.overrides = sparse_overrides(&default, &buffer);
        if let Some(cooldown) = parse_cooldown(detail.cooldown_input.read(cx).content()) {
            instance.cooldown_secs = cooldown;
        }
        instance.cooldown_global = !detail.cooldown_per_user;
        instance.permission_rung = detail.permission_rung;
        Some(instance)
    }

    fn flag_cooldown_entry(&self, cx: &mut Context<Self>) {
        if let Some(detail) = self.detail.as_ref() {
            let input = detail.cooldown_input.clone();
            let invalid = parse_cooldown(input.read(cx).content()).is_none();
            input.update(cx, |input, cx| input.set_invalid(invalid, cx));
        }
    }

    pub(super) fn commit_config(&mut self, cx: &mut Context<Self>) {
        self.flag_cooldown_entry(cx);
        let Some(instance) = self.pending_instance(cx) else {
            return;
        };
        let Some(detail) = self.detail.as_ref() else {
            return;
        };
        if detail.instance == instance {
            return;
        }
        if self.refuses_instance(&instance, cx) {
            return;
        }
        if let Some(detail) = self.detail.as_mut() {
            detail.instance = instance.clone();
        }
        self.save_detail(instance, DetailAfterSave::Keep, cx);
    }

    fn instance_config(&self, instance: &TriggerInstance) -> TriggerConfig {
        let default = trigger_form_defaults(self.registry.get(&instance.kind_id));
        effective_config(&default, &instance.overrides)
    }

    fn holds_valid_config(&self, instance: &TriggerInstance) -> bool {
        self.registry
            .get(&instance.kind_id)
            .is_none_or(|descriptor| {
                descriptor
                    .check_config(&self.instance_config(instance))
                    .is_ok()
            })
    }

    fn refuses_instance(&self, instance: &TriggerInstance, cx: &mut Context<Self>) -> bool {
        self.refuse_invalid_config(&instance.kind_id, &self.instance_config(instance), cx)
    }

    fn save_detail(
        &mut self,
        instance: TriggerInstance,
        after: DetailAfterSave,
        cx: &mut Context<Self>,
    ) {
        let id = instance.id;
        let repo = Arc::clone(&self.repo);
        self.detail_writers.writer(id).write(
            async move { repo.save(&instance).await },
            move |this, outcome, cx| this.detail_save_settled(id, after, outcome, cx),
            cx,
        );
    }

    fn detail_save_settled(
        &mut self,
        id: TriggerInstanceId,
        after: DetailAfterSave,
        outcome: WriteOutcome,
        cx: &mut Context<Self>,
    ) {
        let rebuild = match outcome {
            WriteOutcome::Written => {
                self.reload(cx);
                after == DetailAfterSave::Rebuild
            }
            WriteOutcome::Superseded => after == DetailAfterSave::Rebuild,
            WriteOutcome::Failed(reason) => {
                self.on_repo_error(&reason, cx);
                true
            }
        };
        if rebuild && self.selected == Some(id) {
            self.load_detail(id, cx);
        }
    }

    pub(super) fn persist_detail_on_release(&mut self, cx: &mut App) {
        let Some(instance) = self.pending_instance(cx) else {
            return;
        };
        if self
            .detail
            .as_ref()
            .is_some_and(|detail| detail.instance == instance)
            || !self.holds_valid_config(&instance)
        {
            return;
        }
        let repo = Arc::clone(&self.repo);
        let failure = tr!(
            "triggers_release_persist_failed",
            name = instance.name.as_str()
        );
        self.detail_writers.writer(instance.id).write_on_release(
            async move { repo.save(&instance).await },
            failure,
            cx,
        );
    }

    fn revert_config_field(&mut self, key: String, cx: &mut Context<Self>) {
        let Some(mut instance) = self.pending_instance(cx) else {
            return;
        };
        instance.overrides.remove(&key);
        if self.refuses_instance(&instance, cx) {
            return;
        }
        if let Some(detail) = self.detail.as_mut() {
            detail.instance = instance.clone();
        }
        self.save_detail(instance, DetailAfterSave::Rebuild, cx);
    }

    fn use_as_template(&mut self, cx: &mut Context<Self>) {
        let Some(detail) = self.detail.as_ref() else {
            return;
        };
        let src = detail.instance.clone();
        let new_id = TriggerInstanceId::new();
        let instance = TriggerInstance {
            id: new_id,
            name: tr!("triggers_template_copy_name", name = src.name.as_str()),
            user_defined: true,
            ..src
        };
        let repo = Arc::clone(&self.repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                repo.save(&instance).await.map_err(RepoFailure::from)?;
                load_rows(&*repo).await
            },
            move |this, result, cx| match result {
                Ok(rows) => {
                    this.apply_rows(rows, cx);
                    this.selected = Some(new_id);
                    this.list_cursor = Some(new_id);
                    this.detail = None;
                    this.load_detail(new_id, cx);
                    cx.notify();
                }
                Err(failure) => this.on_repo_error(&failure, cx),
            },
            cx,
        );
        cx.notify();
    }

    pub(super) fn render_detail_sheet(
        &self,
        id: TriggerInstanceId,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match self.detail.as_ref().filter(|d| d.instance.id == id) {
            Some(detail) => self.render_detail_panel(detail, palette, cx),
            None => self.render_detail_loading(palette, cx),
        }
    }

    fn detail_shell(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> gpui::Div {
        let panel = div()
            .w(self.detail_width)
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .min_h(px(0.0))
            .bg(palette.base)
            .border_l(BORDER_THIN)
            .border_color(palette.border_regular);

        install_resize(
            panel,
            TriggerDetailResizeDrag,
            "triggers-detail-resize",
            ResizeEdge::Left,
            ResizeRange {
                min: DETAIL_SHEET_MIN,
                max: DETAIL_SHEET_MAX,
            },
            palette,
            cx.listener(|this, width: &Pixels, _, cx| this.set_detail_width(*width, cx)),
        )
    }

    fn render_detail_loading(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        self.detail_shell(palette, cx)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .font_family(body_family())
                    .text_size(CFG_VAL_FS)
                    .text_color(palette.text_muted)
                    .child(tr!("triggers_detail_loading")),
            )
            .into_any_element()
    }

    fn render_detail_panel(
        &self,
        detail: &TriggerDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.detail_shell(palette, cx)
            .child(self.render_detail_header(detail, palette, cx))
            .child(self.render_detail_body(detail, palette, cx))
            .child(self.render_detail_footer(detail, palette, cx))
            .into_any_element()
    }

    fn render_detail_header(
        &self,
        detail: &TriggerDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let instance = &detail.instance;
        let id = instance.id;
        let dot_color = platform_dot_color(&instance.kind_id, palette);
        let glyph = self
            .registry
            .get(&instance.kind_id)
            .map(|d| Icon::from_name(d.icon_name()))
            .unwrap_or(Icon::Bolt);

        let tile = div()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(TILE)
            .rounded(TILE_RADIUS)
            .bg(palette.surface_overlay)
            .child(icon(glyph, TILE_GLYPH, dot_color));

        let name = div()
            .id("triggers-detail-rename")
            .cursor_pointer()
            .font_family(body_family())
            .font_weight(FontWeight::SEMIBOLD)
            .text_size(NAME_FS)
            .text_color(palette.text_primary)
            .child(instance.name.clone())
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.start_rename(id, window, cx)
            }));
        let kind_line = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(status_dot(dot_color, KIND_DOT))
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(KIND_FS)
                    .text_color(palette.text_faint)
                    .child(instance.kind_id.clone()),
            )
            .when_some(self.switched_off_owner(&instance.kind_id), |line, owner| {
                line.child(inactive_badge(&owner, palette, density))
            })
            .when_some(
                self.uncovered_category(&instance.kind_id),
                |line, category| line.child(category_inactive_badge(category, palette, density)),
            );
        let title = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(scaled(px(2.0), density))
            .child(name)
            .child(kind_line);

        let enable_toggle = toggle(instance.enabled, palette).on_click(
            SharedString::from(format!("triggers-detail-toggle-{id}")),
            cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_enable(id, cx)),
        );

        let close = div()
            .id("triggers-detail-close")
            .flex_none()
            .cursor_pointer()
            .p(spacing(Spacing::Xxs, density))
            .child(icon(Icon::X, px(15.0), palette.text_faint))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.close_detail(cx)));

        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(scaled(HEADER_PAD_V, density))
            .px(spacing(Spacing::Md, density))
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(tile)
            .child(title)
            .child(enable_toggle)
            .child(close)
            .into_any_element()
    }

    fn render_detail_body(
        &self,
        detail: &TriggerDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        div()
            .id("triggers-detail-body")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .py(scaled(BODY_PAD_V, density))
            .px(spacing(Spacing::Md, density))
            .flex()
            .flex_col()
            .child(self.render_config_section(detail, palette, cx))
            .when(
                cooldown_applies(self.registry.get(&detail.instance.kind_id)),
                |body| body.child(self.render_cooldown_section(detail, palette, cx)),
            )
            .children(self.render_permission_section(detail, palette, cx))
            .child(self.render_used_in_section(detail, palette, cx))
            .into_any_element()
    }

    fn section_label(
        &self,
        label: impl Into<SharedString>,
        right: Option<AnyElement>,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        let label = label.into();
        div()
            .w_full()
            .flex()
            .items_center()
            .py(spacing(Spacing::Xs, density))
            .child(
                div()
                    .flex_1()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(label),
            )
            .children(right)
            .into_any_element()
    }

    fn render_config_section(
        &self,
        detail: &TriggerDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let override_count = detail.instance.overrides.len();
        let right = if override_count > 0 {
            div()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.accent_peach)
                .child(tr!(
                    "triggers_sheet_config_overridden",
                    count = i64::try_from(override_count).unwrap_or(i64::MAX)
                ))
                .into_any_element()
        } else {
            div()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(tr!("triggers_sheet_config_all_defaults"))
                .into_any_element()
        };

        let card: AnyElement = if detail.fields.is_empty() {
            div()
                .py(scaled(PLACEHOLDER_PAD_V, density))
                .px(scaled(PLACEHOLDER_PAD_H, density))
                .italic()
                .font_family(body_family())
                .text_size(CFG_VAL_FS)
                .text_color(palette.text_faint)
                .child(tr!("triggers_sheet_no_config"))
                .into_any_element()
        } else {
            let overridden: HashMap<&str, bool> = detail
                .instance
                .overrides
                .keys()
                .map(|k| (k.as_str(), true))
                .collect();
            let last = detail.fields.len().saturating_sub(1);
            let mut col = div().flex().flex_col();
            for (i, field) in detail.fields.iter().enumerate() {
                let label = row_label(&detail.labels, i, field);
                col = col.child(self.render_config_row(
                    field,
                    label,
                    &overridden,
                    i == last,
                    palette,
                    cx,
                ));
            }
            col.into_any_element()
        };

        let framed = div()
            .w_full()
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.shell)
            .child(card);

        div()
            .flex()
            .flex_col()
            .pb(spacing(Spacing::Md, density))
            .child(self.section_label(
                tr!("triggers_sheet_section_configuration"),
                Some(right),
                palette,
                density,
            ))
            .child(framed)
            .children(
                timer_field_hints(&detail.instance.kind_id, palette, density)
                    .map(|hints| div().pt(spacing(Spacing::Xs, density)).child(hints)),
            )
            .into_any_element()
    }

    fn render_config_row(
        &self,
        field: &ConfigField,
        label: SharedString,
        overridden: &HashMap<&str, bool>,
        last: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let key = field.key().to_owned();
        let is_overridden = overridden.contains_key(key.as_str());
        let key_color = if is_overridden {
            palette.accent_peach
        } else {
            palette.text_muted
        };

        let label = config_label_cell(label, CFG_KEY_W, CFG_KEY_FS, key_color);

        let value = render_config_control(
            field,
            palette,
            "triggers-cfg",
            &cx.entity(),
            &self.detail_config_handlers(),
            density,
        );

        let revert: AnyElement = if is_overridden && !matches!(field, ConfigField::Hint { .. }) {
            let revert_key = key.clone();
            let hover = palette.surface_overlay;
            div()
                .id(SharedString::from(format!("triggers-cfg-revert-{key}")))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .size(REVERT_W)
                .rounded(radius(Radius::Sm))
                .cursor_pointer()
                .hover(move |s| s.bg(hover))
                .child(icon(Icon::X, px(11.0), palette.text_faint))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.revert_config_field(revert_key.clone(), cx)
                }))
                .into_any_element()
        } else {
            div().w(REVERT_W).flex_none().into_any_element()
        };

        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(scaled(CFG_ROW_PAD_V, density))
            .px(scaled(CFG_ROW_PAD_H, density))
            .when(!last, |row| {
                row.border_b(BORDER_THIN)
                    .border_color(palette.border_regular)
            })
            .child(label)
            .child(div().flex_1().min_w(px(0.0)).child(value))
            .child(revert)
            .into_any_element()
    }

    fn render_cooldown_section(
        &self,
        detail: &TriggerDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let caption = div()
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(tr!("triggers_sheet_cooldown_caption"))
            .into_any_element();

        let label_cell = |label: SharedString| {
            div()
                .w(CFG_KEY_W)
                .flex_none()
                .overflow_hidden()
                .font_family(mono_family())
                .text_size(CFG_KEY_FS)
                .text_color(palette.text_muted)
                .child(label)
        };

        let value_row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(scaled(CFG_ROW_PAD_V, density))
            .px(scaled(CFG_ROW_PAD_H, density))
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(label_cell(tr!("triggers_sheet_cooldown_value").into()))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(detail.cooldown_input.clone()),
            );

        let scope_toggle = toggle(!detail.cooldown_per_user, palette).on_click(
            "triggers-detail-cooldown-scope",
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_cooldown_scope(cx)),
        );
        let scope_row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(scaled(CFG_ROW_PAD_V, density))
            .px(scaled(CFG_ROW_PAD_H, density))
            .child(label_cell(tr!("triggers_sheet_cooldown_scope").into()))
            .child(div().flex_1().min_w(px(0.0)).child(scope_toggle));

        let card = div().flex().flex_col().child(value_row).child(scope_row);

        let framed = div()
            .w_full()
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.shell)
            .child(card);

        div()
            .flex()
            .flex_col()
            .pb(spacing(Spacing::Md, density))
            .child(self.section_label(
                tr!("triggers_sheet_section_cooldown"),
                Some(caption),
                palette,
                density,
            ))
            .child(framed)
            .into_any_element()
    }

    fn render_permission_section(
        &self,
        detail: &TriggerDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let density = cx.density();
        let descriptor = self.registry.get(&detail.instance.kind_id)?;
        descriptor.chat_trigger_family()?;
        let platforms = gated_platforms(
            descriptor.platform_contract(),
            &detail.instance.platform_scope,
        );
        let meanings = rung_meanings(detail.permission_rung, &platforms);

        let segments = PERMISSION_RUNGS
            .into_iter()
            .map(|rung| {
                segment(
                    SharedString::from(format!("triggers-detail-rung-{}", rung.as_str())),
                    permission_rung_label(rung),
                    detail.permission_rung == rung,
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.set_permission_rung(rung, cx)
                    }),
                )
            })
            .collect();
        let ladder = segmented(segments, palette).wrap(spacing(Spacing::Xxs, density));

        let rung_row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(scaled(CFG_ROW_PAD_V, density))
            .px(scaled(CFG_ROW_PAD_H, density))
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(
                div()
                    .w(CFG_KEY_W)
                    .flex_none()
                    .overflow_hidden()
                    .font_family(mono_family())
                    .text_size(CFG_KEY_FS)
                    .text_color(palette.text_muted)
                    .child(tr!("triggers_sheet_permission_value")),
            )
            .child(div().flex_1().min_w(px(0.0)).child(ladder));

        let helper_row = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .py(scaled(CFG_ROW_PAD_V, density))
            .px(scaled(CFG_ROW_PAD_H, density))
            .font_family(body_family())
            .text_size(CFG_VAL_FS)
            .text_color(palette.text_faint)
            .child(tr!("triggers_sheet_permission_helper"))
            .children(meanings.into_iter().map(|meaning| div().child(meaning)));

        let framed = div()
            .w_full()
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.shell)
            .child(div().flex().flex_col().child(rung_row).child(helper_row));

        Some(
            div()
                .flex()
                .flex_col()
                .pb(spacing(Spacing::Md, density))
                .child(self.section_label(
                    tr!("triggers_sheet_section_permission"),
                    None,
                    palette,
                    density,
                ))
                .child(framed)
                .into_any_element(),
        )
    }

    fn render_used_in_section(
        &self,
        detail: &TriggerDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let count = detail.used_in.len();
        let label = if count > 0 {
            tr!(
                "triggers_sheet_section_used_in_count",
                count = i64::try_from(count).unwrap_or(i64::MAX)
            )
        } else {
            tr!("triggers_sheet_section_used_in")
        };

        let inner: AnyElement = if detail.used_in.is_empty() {
            div()
                .w_full()
                .flex()
                .flex_col()
                .items_center()
                .gap(scaled(px(2.0), density))
                .py(scaled(PLACEHOLDER_PAD_V, density))
                .px(scaled(PLACEHOLDER_PAD_H, density))
                .rounded(radius(Radius::Md))
                .border(BORDER_THIN)
                .border_color(palette.border_input)
                .text_center()
                .font_family(body_family())
                .text_size(CFG_VAL_FS)
                .text_color(palette.text_faint)
                .child(tr!("triggers_sheet_used_in_empty_title"))
                .child(tr!("triggers_sheet_used_in_empty_hint"))
                .into_any_element()
        } else {
            let mut col = div().flex().flex_col().gap(scaled(px(3.0), density));
            for (action_id, name) in &detail.used_in {
                let action_id = *action_id;
                let title = div()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .font_family(mono_family())
                    .text_size(USED_FS)
                    .text_color(palette.text_primary)
                    .child(name.clone());
                col = col.child(
                    row_card(title, palette)
                        .leading(icon(Icon::Bolt, ROW_GLYPH, palette.brand))
                        .idle_background(palette.shell)
                        .bordered(palette.border_regular, BORDER_THIN, radius(Radius::Sm))
                        .padding_xy(scaled(USED_ROW_PAD_V, density), USED_ROW_PAD_H)
                        .on_click(
                            SharedString::from(format!("triggers-used-in-{action_id}")),
                            cx.listener(move |_this, _: &ClickEvent, _, cx| {
                                cx.emit(NavRequested(Screen::Actions(Some(action_id))))
                            }),
                        )
                        .into_any_element(),
                );
            }
            col.into_any_element()
        };

        div()
            .flex()
            .flex_col()
            .child(self.section_label(label, None, palette, density))
            .child(inner)
            .into_any_element()
    }

    fn render_detail_footer(
        &self,
        detail: &TriggerDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let id = detail.instance.id;
        let used_count = self.find(id).map(|r| r.used_in_count).unwrap_or(0);
        let can_delete = used_count == 0;

        let template = ghost_button_with_icon(Icon::Copy, tr!("triggers_menu_template"), palette)
            .on_click(
                "triggers-detail-template",
                cx.listener(|this, _: &ClickEvent, _, cx| this.use_as_template(cx)),
            );

        let delete_color = if can_delete {
            palette.danger
        } else {
            palette.disabled
        };
        let delete_base = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_input)
            .child(icon(Icon::Eraser, px(11.0), delete_color))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(CFG_VAL_FS)
                    .text_color(delete_color)
                    .child(tr!("triggers_sheet_delete_btn")),
            );
        let delete: AnyElement = if can_delete {
            delete_base
                .id("triggers-detail-delete")
                .cursor_pointer()
                .on_click(
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.request_delete(id, cx)),
                )
                .into_any_element()
        } else {
            delete_base.opacity(ROW_OFF_OPACITY).into_any_element()
        };

        let save = primary_button(tr!("triggers_sheet_save_btn"), palette).on_click(
            "triggers-detail-save",
            cx.listener(|this, _: &ClickEvent, _, cx| this.commit_config(cx)),
        );

        div()
            .flex_none()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Md, density))
            .bg(palette.shell)
            .border_t(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(template)
            .child(div().flex_1())
            .child(save)
            .child(delete)
            .into_any_element()
    }
}

fn parse_cooldown(text: &str) -> Option<u32> {
    let text = text.trim();
    if text.is_empty() {
        return Some(0);
    }
    text.parse::<u32>().ok()
}

async fn load_detail_data(
    repo: &dyn TriggerInstanceRepo,
    action_repo: &dyn ActionRepo,
    id: TriggerInstanceId,
) -> Result<Option<TriggerDetailData>, RepoFailure> {
    let Some(instance) = repo.get(id).await.map_err(RepoFailure::from)? else {
        return Ok(None);
    };
    let action_ids = repo.actions_using(id).await.map_err(RepoFailure::from)?;
    let mut used_in = Vec::with_capacity(action_ids.len());
    for action_id in action_ids {
        let name = action_repo
            .get(action_id)
            .await
            .map_err(RepoFailure::from)?
            .map(|a| a.name)
            .unwrap_or_else(|| action_id.to_string());
        used_in.push((action_id, name));
    }
    Ok(Some(TriggerDetailData { instance, used_in }))
}

#[cfg(test)]
pub(super) mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use forge_components::ThemeId;
    use forge_platform_core::CollectionFailure;
    use forge_registry::TriggerRegistry;
    use forge_registry::{FormField, TriggerCategory};
    use forge_storage::StorageError;
    use forge_types::{PlatformScope, TriggerConfig, TriggerInstance, TriggerInstanceId, Variant};

    use super::*;
    use crate::integrations::BuiltinRegistry;
    use crate::screen_shortcuts::ScreenShortcuts;
    use crate::test_support::{
        Declares, FakeCollections, StubActions, StubTrigger, error_toasts, owned_reward, pump,
        pump_until, runtime, test_backend, trigger_registry,
    };
    use forge_components::Presentation;

    pub(in crate::triggers_screen) const KIND: &str = "midi.input.note_on";
    const NAME: &str = "Note on";
    const SEEDED_COOLDOWN: u32 = 30;
    const SAVE_REFUSED: &str = "database is locked";
    pub(in crate::triggers_screen) const GREETING_FIELD: &str = "greeting";
    pub(in crate::triggers_screen) const REFUSED_GREETING: &str = "refused";

    pub(in crate::triggers_screen) struct RecordingTriggers {
        instance: Mutex<TriggerInstance>,
        saved: Mutex<Vec<u32>>,
        saved_overrides: Mutex<Vec<TriggerConfig>>,
        saves_fail: AtomicBool,
        row_lists: AtomicUsize,
    }

    impl RecordingTriggers {
        fn new(instance: TriggerInstance) -> Arc<Self> {
            Arc::new(Self {
                instance: Mutex::new(instance),
                saved: Mutex::new(Vec::new()),
                saved_overrides: Mutex::new(Vec::new()),
                saves_fail: AtomicBool::new(false),
                row_lists: AtomicUsize::new(0),
            })
        }

        fn stored(&self) -> TriggerInstance {
            self.instance.lock().unwrap().clone()
        }

        fn fail_saves(&self) {
            self.saves_fail.store(true, Ordering::SeqCst);
        }

        pub(in crate::triggers_screen) fn saved_cooldowns(&self) -> Vec<u32> {
            self.saved.lock().unwrap().clone()
        }

        pub(in crate::triggers_screen) fn last_saved_overrides(&self) -> Option<TriggerConfig> {
            self.saved_overrides.lock().unwrap().last().cloned()
        }
    }

    #[async_trait::async_trait]
    impl TriggerInstanceRepo for RecordingTriggers {
        async fn list_all(&self) -> Result<Vec<TriggerInstance>, StorageError> {
            Ok(vec![self.instance.lock().unwrap().clone()])
        }

        async fn list_user_defined(&self) -> Result<Vec<TriggerInstance>, StorageError> {
            self.row_lists.fetch_add(1, Ordering::SeqCst);
            Ok(vec![self.instance.lock().unwrap().clone()])
        }

        async fn list_for_action(&self, _: ActionId) -> Result<Vec<TriggerInstance>, StorageError> {
            Ok(Vec::new())
        }

        async fn actions_using(&self, _: TriggerInstanceId) -> Result<Vec<ActionId>, StorageError> {
            Ok(Vec::new())
        }

        async fn link_action(&self, _: ActionId, _: TriggerInstanceId) -> Result<(), StorageError> {
            unreachable!("the detail sheet never links an action")
        }

        async fn link_action_at(
            &self,
            _: ActionId,
            _: TriggerInstanceId,
            _: i64,
        ) -> Result<(), StorageError> {
            unreachable!("the detail sheet never links an action")
        }

        async fn unlink_action(
            &self,
            _: ActionId,
            _: TriggerInstanceId,
        ) -> Result<bool, StorageError> {
            unreachable!("the detail sheet never unlinks an action")
        }

        async fn get(&self, _: TriggerInstanceId) -> Result<Option<TriggerInstance>, StorageError> {
            Ok(Some(self.instance.lock().unwrap().clone()))
        }

        async fn save(&self, instance: &TriggerInstance) -> Result<(), StorageError> {
            if self.saves_fail.load(Ordering::SeqCst) {
                return Err(StorageError::Connection {
                    reason: SAVE_REFUSED.to_owned(),
                });
            }
            self.saved.lock().unwrap().push(instance.cooldown_secs);
            self.saved_overrides
                .lock()
                .unwrap()
                .push(instance.overrides.clone());
            *self.instance.lock().unwrap() = instance.clone();
            Ok(())
        }

        async fn delete(&self, _: TriggerInstanceId) -> Result<bool, StorageError> {
            unreachable!("the detail sheet never deletes an instance")
        }

        async fn upsert_default(
            &self,
            _: &str,
            _: &str,
        ) -> Result<TriggerInstanceId, StorageError> {
            unreachable!("the detail sheet never upserts a default")
        }

        async fn set_enabled(
            &self,
            _: TriggerInstanceId,
            enabled: bool,
        ) -> Result<(), StorageError> {
            self.instance.lock().unwrap().enabled = enabled;
            Ok(())
        }
    }

    pub(in crate::triggers_screen) struct Fixture {
        view: Option<Entity<TriggersRegistryView>>,
        pub(in crate::triggers_screen) repo: Arc<RecordingTriggers>,
        pub(in crate::triggers_screen) rt: tokio::runtime::Runtime,
    }

    impl Fixture {
        fn new(cx: &mut gpui::TestAppContext) -> Self {
            Self::build(
                cx,
                TriggerRegistry::new(),
                KIND,
                TriggerConfig::new(),
                BuiltinRegistry::default(),
            )
        }

        pub(in crate::triggers_screen) fn build(
            cx: &mut gpui::TestAppContext,
            registry: TriggerRegistry,
            kind: &str,
            overrides: TriggerConfig,
            builtins: BuiltinRegistry,
        ) -> Self {
            cx.update(|cx| {
                cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
            });
            let rt = runtime();
            let id = TriggerInstanceId::new();
            let instance = TriggerInstance {
                id,
                kind_id: kind.to_owned(),
                name: NAME.to_owned(),
                overrides,
                enabled: true,
                user_defined: false,
                platform_scope: PlatformScope::Any,
                cooldown_secs: SEEDED_COOLDOWN,
                cooldown_global: true,
                permission_rung: PermissionRung::Everyone,
            };
            let repo = RecordingTriggers::new(instance.clone());
            let (backend, _writes) = test_backend();
            let view = cx.update(|cx| {
                cx.new(|cx| {
                    TriggersRegistryView::new(
                        Arc::clone(&repo) as Arc<dyn TriggerInstanceRepo>,
                        Arc::new(StubActions),
                        Arc::new(registry),
                        backend as Arc<dyn forge_storage::SettingsRepo>,
                        rt.handle().clone(),
                        None,
                        cx,
                    )
                    .with_builtins(builtins)
                })
            });
            let mut fixture = Self {
                view: Some(view),
                repo,
                rt,
            };
            fixture.settle(cx);
            fixture.view().update(cx, |view, cx| {
                view.selected = Some(id);
                view.apply_detail(
                    id,
                    TriggerDetailData {
                        instance,
                        used_in: Vec::new(),
                    },
                    cx,
                );
            });
            fixture.settle(cx);
            fixture
        }

        pub(in crate::triggers_screen) fn view(&self) -> &Entity<TriggersRegistryView> {
            self.view.as_ref().unwrap()
        }

        pub(in crate::triggers_screen) fn settle(&mut self, cx: &mut gpui::TestAppContext) {
            pump(&self.rt);
            cx.update(|_cx| {});
            cx.run_until_parked();
            pump(&self.rt);
            cx.run_until_parked();
        }

        fn cooldown_input(&self, cx: &mut gpui::TestAppContext) -> Entity<TextInput> {
            self.view().read_with(cx, |view, _| {
                view.detail.as_ref().unwrap().cooldown_input.clone()
            })
        }

        fn cooldown_text(&self, cx: &mut gpui::TestAppContext) -> String {
            let input = self.cooldown_input(cx);
            input.read_with(cx, |field, _| field.content().to_owned())
        }

        fn detail_instance(&self, cx: &mut gpui::TestAppContext) -> TriggerInstance {
            self.view()
                .read_with(cx, |view, _| view.detail.as_ref().unwrap().instance.clone())
        }

        fn wait_for_detail(
            &self,
            cx: &mut gpui::TestAppContext,
            mut reached: impl FnMut(&TriggerInstance) -> bool,
        ) {
            pump_until(&self.rt, || {
                cx.run_until_parked();
                reached(&self.detail_instance(cx))
            });
        }

        fn type_cooldown(&self, cx: &mut gpui::TestAppContext, text: &str) {
            self.view().update(cx, |view, cx| {
                let input = view.detail.as_ref().unwrap().cooldown_input.clone();
                input.update(cx, |field, cx| field.set_content(text.to_owned(), cx));
            });
        }

        fn commit(&mut self, cx: &mut gpui::TestAppContext) {
            self.view().update(cx, |view, cx| view.commit_config(cx));
            self.settle(cx);
        }

        fn release(&mut self, cx: &mut gpui::TestAppContext) {
            self.view = None;
            cx.update(|_cx| {});
            cx.run_until_parked();
            pump(&self.rt);
        }

        fn type_greeting(&self, cx: &mut gpui::TestAppContext, text: &str) {
            let input = self.view().read_with(cx, |view, _| {
                view.detail
                    .as_ref()
                    .unwrap()
                    .fields
                    .iter()
                    .find_map(|field| match field {
                        ConfigField::Input { key, input, .. } if key == GREETING_FIELD => {
                            Some(input.clone())
                        }
                        _ => None,
                    })
                    .unwrap()
            });
            input.update(cx, |field, cx| field.set_content(text.to_owned(), cx));
        }

        fn detail_greeting(&self, cx: &mut gpui::TestAppContext) -> Option<Variant> {
            self.detail_instance(cx)
                .overrides
                .get(GREETING_FIELD)
                .cloned()
        }
    }

    #[gpui::test]
    fn a_sheet_torn_down_over_an_uncommitted_cooldown_writes_it_through_the_repo(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut fixture = Fixture::new(cx);
        fixture.type_cooldown(cx, "5");

        fixture.release(cx);

        assert_eq!(fixture.repo.saved_cooldowns(), [5]);
    }

    #[gpui::test]
    fn a_sheet_torn_down_over_an_untouched_form_writes_nothing(cx: &mut gpui::TestAppContext) {
        let mut fixture = Fixture::new(cx);

        fixture.release(cx);

        assert!(fixture.repo.saved_cooldowns().is_empty());
    }

    #[gpui::test]
    fn a_sheet_torn_down_after_its_commit_landed_writes_nothing_more(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut fixture = Fixture::new(cx);
        fixture.type_cooldown(cx, "5");
        fixture.commit(cx);
        fixture.commit(cx);

        fixture.release(cx);

        assert_eq!(fixture.repo.saved_cooldowns(), [5]);
    }

    #[gpui::test]
    fn an_unreadable_cooldown_keeps_the_stored_value_and_flags_the_field(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut fixture = Fixture::new(cx);
        fixture.type_cooldown(cx, "later");
        let flagged = Arc::new(AtomicUsize::new(0));
        let field_changes = Arc::clone(&flagged);
        let field = fixture.cooldown_input(cx);
        let _watch = cx.update(|cx| {
            cx.observe(&field, move |_, _| {
                field_changes.fetch_add(1, Ordering::SeqCst);
            })
        });

        fixture.commit(cx);
        let flagged_by_the_commit = flagged.load(Ordering::SeqCst);
        fixture.commit(cx);
        let stored_after_commits = fixture.detail_instance(cx).cooldown_secs;
        fixture.release(cx);

        assert_eq!(
            (
                flagged_by_the_commit,
                flagged.load(Ordering::SeqCst),
                stored_after_commits,
                fixture.repo.saved_cooldowns(),
            ),
            (1, 1, SEEDED_COOLDOWN, Vec::new())
        );
    }

    #[test]
    fn parse_cooldown_reads_a_blank_field_as_zero_and_refuses_what_is_not_a_u32() {
        for (text, expected) in [
            ("", Some(0)),
            ("   ", Some(0)),
            ("0", Some(0)),
            (" 45 ", Some(45)),
            ("4294967295", Some(u32::MAX)),
            ("4294967296", None),
            ("-1", None),
            ("1.5", None),
            ("later", None),
        ] {
            assert_eq!(parse_cooldown(text), expected, "{text:?}");
        }
    }

    const REWARD_KIND: &str = "twitch.channel_points.redemption";
    const REWARD_FIELD: &str = "reward_id";
    const LEGACY_REWARD_ID: &str = "0c7f3a52-legacy";

    fn reward_registry() -> TriggerRegistry {
        trigger_registry(vec![
            StubTrigger::new(
                REWARD_KIND,
                "Reward redeemed",
                TriggerCategory::ChannelPoints,
                Declares::Nothing,
            )
            .with_fields(vec![FormField::DynamicSelect {
                key: "reward_id",
                label: "Reward",
                options_key: "collections.twitch.rewards",
            }]),
        ])
    }

    fn reward_sheet(
        cx: &mut gpui::TestAppContext,
        fake: &Arc<FakeCollections>,
        stored: Option<&str>,
    ) -> Fixture {
        let overrides: TriggerConfig = stored
            .map(|id| (REWARD_FIELD.to_owned(), Variant::String(id.to_owned())))
            .into_iter()
            .collect();
        Fixture::build(
            cx,
            reward_registry(),
            REWARD_KIND,
            overrides,
            fake.installed_as("twitch"),
        )
    }

    fn reward_choice(fixture: &Fixture, cx: &mut gpui::TestAppContext) -> (String, Vec<String>) {
        fixture.view().read_with(cx, |view, _| {
            view.detail
                .as_ref()
                .unwrap()
                .fields
                .iter()
                .find_map(|field| match field {
                    ConfigField::Choice {
                        key,
                        selected,
                        options,
                        ..
                    } if key == REWARD_FIELD => Some((
                        selected.clone(),
                        options.iter().map(|(value, _)| value.clone()).collect(),
                    )),
                    _ => None,
                })
                .unwrap()
        })
    }

    #[gpui::test]
    fn a_reward_filter_offers_match_any_first_then_every_listed_reward(
        cx: &mut gpui::TestAppContext,
    ) {
        let fake = FakeCollections::listing(Ok(vec![
            owned_reward("r-1", "Hydrate"),
            owned_reward("r-2", "Stretch"),
        ]));

        let fixture = reward_sheet(cx, &fake, None);

        assert_eq!(reward_choice(&fixture, cx).1, ["", "r-1", "r-2"]);
    }

    #[gpui::test]
    fn a_stored_reward_id_the_listing_no_longer_has_still_shows_as_the_selection(
        cx: &mut gpui::TestAppContext,
    ) {
        let fake = FakeCollections::listing(Ok(vec![owned_reward("r-1", "Hydrate")]));

        let fixture = reward_sheet(cx, &fake, Some(LEGACY_REWARD_ID));

        assert_eq!(reward_choice(&fixture, cx).0, LEGACY_REWARD_ID);
    }

    #[gpui::test]
    fn a_revision_bump_reoffers_the_fresh_reward_listing(cx: &mut gpui::TestAppContext) {
        let fake = FakeCollections::listing(Ok(vec![owned_reward("r-1", "Hydrate")]));
        let mut fixture = reward_sheet(cx, &fake, None);
        fake.relist(Ok(vec![owned_reward("r-9", "Brand new")]));

        fake.bump();
        fixture.settle(cx);

        assert_eq!(reward_choice(&fixture, cx).1, ["", "r-9"]);
    }

    #[gpui::test]
    fn a_reward_listing_that_fails_after_a_disconnect_withdraws_the_stale_rewards(
        cx: &mut gpui::TestAppContext,
    ) {
        let fake = FakeCollections::listing(Ok(vec![owned_reward("r-1", "Hydrate")]));
        let mut fixture = reward_sheet(cx, &fake, None);
        fake.relist(Err(CollectionFailure::NotConnected));

        fake.bump();
        fixture.settle(cx);

        assert_eq!(reward_choice(&fixture, cx).1, [""]);
    }

    #[gpui::test]
    fn picking_a_reward_saves_its_id_as_the_filter(cx: &mut gpui::TestAppContext) {
        let fake = FakeCollections::listing(Ok(vec![owned_reward("r-1", "Hydrate")]));
        let mut fixture = reward_sheet(cx, &fake, Some(LEGACY_REWARD_ID));

        fixture.view().update(cx, |view, cx| {
            view.pick_config_field(REWARD_FIELD.to_owned(), "r-1".to_owned(), cx)
        });
        fixture.settle(cx);

        assert_eq!(
            fixture
                .repo
                .last_saved_overrides()
                .and_then(|overrides| overrides.get(REWARD_FIELD).cloned()),
            Some(Variant::String("r-1".to_owned()))
        );
    }

    #[gpui::test]
    fn the_save_shortcut_commits_the_open_sheet_unless_a_delete_prompt_is_showing(
        cx: &mut gpui::TestAppContext,
    ) {
        for (delete_prompt, expected, case) in [
            (false, vec![5], "sheet open"),
            (true, Vec::new(), "delete prompt showing"),
        ] {
            let mut fixture = Fixture::new(cx);
            fixture.type_cooldown(cx, "5");
            if delete_prompt {
                fixture.view().update(cx, |view, _| {
                    view.pending_delete.request(TriggerInstanceId::new())
                });
            }

            fixture.view().update(cx, |view, cx| view.save_shortcut(cx));
            fixture.settle(cx);

            assert_eq!(fixture.repo.saved_cooldowns(), expected, "{case}");
        }
    }

    #[gpui::test]
    fn a_landed_commit_keeps_the_field_being_typed_in_with_its_content(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut fixture = Fixture::new(cx);
        let input = fixture.cooldown_input(cx);
        fixture.type_cooldown(cx, "5");
        let lists_before = fixture.repo.row_lists.load(Ordering::SeqCst);

        fixture.commit(cx);
        pump_until(&fixture.rt, || {
            cx.run_until_parked();
            fixture.repo.row_lists.load(Ordering::SeqCst) > lists_before
        });
        fixture.settle(cx);

        assert_eq!(
            (
                fixture.cooldown_input(cx).entity_id(),
                fixture.cooldown_text(cx)
            ),
            (input.entity_id(), "5".to_owned())
        );
    }

    #[gpui::test]
    fn a_commit_after_a_toggle_keeps_the_toggled_state(cx: &mut gpui::TestAppContext) {
        let mut fixture = Fixture::new(cx);
        let id = fixture.detail_instance(cx).id;

        fixture
            .view()
            .update(cx, |view, cx| view.toggle_enable(id, cx));
        fixture.wait_for_detail(cx, |instance| !instance.enabled);
        fixture.type_cooldown(cx, "5");
        fixture.commit(cx);

        assert_eq!(
            (
                fixture.repo.saved_cooldowns(),
                fixture.repo.stored().enabled
            ),
            (vec![5], false)
        );
    }

    #[gpui::test]
    fn a_failed_save_rolls_the_sheet_back_to_the_stored_trigger_with_one_toast(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| cx.set_global(forge_components::Toasts::new()));
        let mut fixture = Fixture::new(cx);
        fixture.repo.fail_saves();
        fixture.type_cooldown(cx, "5");

        fixture.commit(cx);
        fixture.wait_for_detail(cx, |instance| instance.cooldown_secs == SEEDED_COOLDOWN);

        assert_eq!(
            (
                fixture.cooldown_text(cx),
                cx.update(|cx| error_toasts(cx)),
                fixture.repo.stored().cooldown_secs,
            ),
            (SEEDED_COOLDOWN.to_string(), 1, SEEDED_COOLDOWN)
        );
    }

    #[gpui::test]
    fn a_failed_write_at_teardown_tells_the_user_which_trigger_lost_its_edit(
        cx: &mut gpui::TestAppContext,
    ) {
        crate::i18n::install_language(forge_storage::Language::En);
        cx.update(|cx| cx.set_global(forge_components::Toasts::new()));
        let mut fixture = Fixture::new(cx);
        fixture.repo.fail_saves();
        fixture.type_cooldown(cx, "5");

        fixture.release(cx);
        pump_until(&fixture.rt, || {
            cx.run_until_parked();
            cx.update(|cx| error_toasts(cx)) == 1
        });

        let message = cx.update(|cx| {
            cx.global::<forge_components::Toasts>().items()[0]
                .message
                .to_string()
        });
        assert!(message.contains(NAME), "{message}");
    }

    fn greeting(text: &str) -> Variant {
        Variant::String(text.to_owned())
    }

    pub(in crate::triggers_screen) fn refusing_registry(
        defaults: TriggerConfig,
    ) -> TriggerRegistry {
        trigger_registry(vec![
            StubTrigger::new(KIND, NAME, TriggerCategory::Midi, Declares::Nothing)
                .with_fields(vec![FormField::Text {
                    key: "greeting",
                    label: "Greeting",
                    placeholder: "",
                }])
                .with_defaults(defaults)
                .refusing(GREETING_FIELD, REFUSED_GREETING),
        ])
    }

    fn refusing_sheet(cx: &mut gpui::TestAppContext, defaults: TriggerConfig) -> Fixture {
        crate::i18n::install_language(forge_storage::Language::En);
        cx.update(|cx| cx.set_global(forge_components::Toasts::new()));
        let overrides: TriggerConfig = [(GREETING_FIELD.to_owned(), greeting("hello"))]
            .into_iter()
            .collect();
        Fixture::build(
            cx,
            refusing_registry(defaults),
            KIND,
            overrides,
            BuiltinRegistry::default(),
        )
    }

    #[gpui::test]
    fn a_commit_the_kind_refuses_saves_nothing_and_keeps_the_sheet_on_the_stored_value(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut fixture = refusing_sheet(cx, TriggerConfig::new());
        fixture.type_greeting(cx, REFUSED_GREETING);

        fixture.commit(cx);

        assert_eq!(
            (
                fixture.repo.last_saved_overrides(),
                fixture.detail_greeting(cx),
            ),
            (None, Some(greeting("hello")))
        );
    }

    #[gpui::test]
    fn a_refused_commit_names_the_field_in_one_error_toast(cx: &mut gpui::TestAppContext) {
        let mut fixture = refusing_sheet(cx, TriggerConfig::new());
        fixture.type_greeting(cx, REFUSED_GREETING);

        fixture.commit(cx);

        let toast = cx.update(|cx| {
            (
                error_toasts(cx),
                cx.global::<forge_components::Toasts>().items()[0]
                    .message
                    .to_string(),
            )
        });
        assert_eq!(toast.0, 1);
        assert!(toast.1.contains("Greeting"), "{}", toast.1);
    }

    #[gpui::test]
    fn a_commit_after_correcting_a_refused_value_saves_the_correction(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut fixture = refusing_sheet(cx, TriggerConfig::new());
        fixture.type_greeting(cx, REFUSED_GREETING);
        fixture.commit(cx);
        fixture.type_greeting(cx, "corrected");

        fixture.commit(cx);
        pump_until(&fixture.rt, || {
            cx.run_until_parked();
            fixture.repo.last_saved_overrides().is_some()
        });

        assert_eq!(
            fixture
                .repo
                .last_saved_overrides()
                .and_then(|saved| saved.get(GREETING_FIELD).cloned()),
            Some(greeting("corrected"))
        );
    }

    #[gpui::test]
    fn reverting_a_field_to_a_default_the_kind_refuses_keeps_the_override(
        cx: &mut gpui::TestAppContext,
    ) {
        let defaults: TriggerConfig = [(GREETING_FIELD.to_owned(), greeting(REFUSED_GREETING))]
            .into_iter()
            .collect();
        let mut fixture = refusing_sheet(cx, defaults);

        fixture.view().update(cx, |view, cx| {
            view.revert_config_field(GREETING_FIELD.to_owned(), cx)
        });
        fixture.settle(cx);

        assert_eq!(
            (
                fixture.repo.last_saved_overrides(),
                fixture.detail_greeting(cx),
                cx.update(|cx| error_toasts(cx)),
            ),
            (None, Some(greeting("hello")), 1)
        );
    }

    #[gpui::test]
    fn a_sheet_torn_down_over_a_value_the_kind_refuses_writes_nothing(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut fixture = refusing_sheet(cx, TriggerConfig::new());
        fixture.type_greeting(cx, REFUSED_GREETING);
        fixture.type_cooldown(cx, "5");

        fixture.release(cx);

        assert!(fixture.repo.saved_cooldowns().is_empty());
    }

    fn greeting_sheet(cx: &mut gpui::TestAppContext) -> Fixture {
        let registry = trigger_registry(vec![
            StubTrigger::new(KIND, NAME, TriggerCategory::Midi, Declares::Nothing).with_fields(
                vec![FormField::Text {
                    key: "greeting",
                    label: "Greeting",
                    placeholder: "",
                }],
            ),
        ]);
        let overrides: TriggerConfig = [(
            GREETING_FIELD.to_owned(),
            Variant::String("hello".to_owned()),
        )]
        .into_iter()
        .collect();
        Fixture::build(cx, registry, KIND, overrides, BuiltinRegistry::default())
    }

    #[gpui::test]
    fn reverting_one_field_keeps_the_other_unsaved_edits(cx: &mut gpui::TestAppContext) {
        let mut fixture = greeting_sheet(cx);
        fixture.type_cooldown(cx, "5");

        fixture.view().update(cx, |view, cx| {
            view.revert_config_field(GREETING_FIELD.to_owned(), cx)
        });
        pump_until(&fixture.rt, || {
            cx.run_until_parked();
            !fixture.repo.saved_cooldowns().is_empty()
        });
        fixture.settle(cx);

        let stored = fixture.repo.stored();
        assert_eq!(
            (
                stored.cooldown_secs,
                stored.overrides.contains_key(GREETING_FIELD),
                fixture.cooldown_text(cx),
            ),
            (5, false, "5".to_owned())
        );
    }

    #[gpui::test]
    fn reverting_a_field_shows_its_default_again(cx: &mut gpui::TestAppContext) {
        let mut fixture = greeting_sheet(cx);
        let shown_greeting = |fixture: &Fixture, cx: &mut gpui::TestAppContext| {
            fixture.view().read_with(cx, |view, cx| {
                view.detail
                    .as_ref()
                    .unwrap()
                    .fields
                    .iter()
                    .find_map(|field| match field {
                        ConfigField::Input { key, input, .. } if key == GREETING_FIELD => {
                            Some(input.read(cx).content().to_owned())
                        }
                        _ => None,
                    })
                    .unwrap()
            })
        };
        assert_eq!(shown_greeting(&fixture, cx), "hello");

        fixture.view().update(cx, |view, cx| {
            view.revert_config_field(GREETING_FIELD.to_owned(), cx)
        });
        pump_until(&fixture.rt, || {
            cx.run_until_parked();
            shown_greeting(&fixture, cx).is_empty()
        });
        fixture.settle(cx);

        assert_eq!(shown_greeting(&fixture, cx), "");
    }

    #[gpui::test]
    fn a_trigger_used_as_a_template_takes_the_cursor_with_the_selection(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut fixture = Fixture::new(cx);
        let seeded = fixture.view().read_with(cx, |view, _| view.selected);

        fixture
            .view()
            .update(cx, |view, cx| view.use_as_template(cx));
        pump_until(&fixture.rt, || {
            cx.run_until_parked();
            fixture
                .view()
                .read_with(cx, |view, _| view.selected != seeded)
        });
        fixture.settle(cx);

        let (selected, cursor) = fixture
            .view()
            .read_with(cx, |view, _| (view.selected, view.list_cursor));
        assert_eq!(cursor, selected);
    }
}
