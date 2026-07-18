use super::config_form::{fold_config_field, overlay_field_values, sparse_overrides};
use super::*;
use crate::presentation::ActivePresentation;
use forge_components::{
    DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_XXS, Icon, InputEvent, Radius, Spacing,
    TextInput, ghost_button_with_icon, icon, radius, spacing, status_dot, toggle, tr,
};
use forge_registry::effective_config;
use gpui::{AnyElement, ClickEvent, FontWeight, SharedString};
use std::collections::HashMap;

const SHEET_W: Pixels = px(420.0);
const HALF_BORDER: Pixels = px(0.5);
const TILE: Pixels = px(30.0);
const TILE_RADIUS: Pixels = px(8.0);
const TILE_GLYPH: Pixels = px(16.0);
const KIND_DOT: Pixels = px(5.0);
const ROW_GLYPH: Pixels = px(12.0);
const NAME_FS: Pixels = px(14.0);
const KIND_FS: Pixels = px(10.5);
const CFG_KEY_FS: Pixels = px(11.0);
const CFG_VAL_FS: Pixels = px(11.5);
const USED_FS: Pixels = px(12.0);
const CFG_KEY_W: Pixels = px(110.0);
const REVERT_W: Pixels = px(22.0);
const HEADER_PAD_V: Pixels = px(12.0);
const HEADER_PAD_H: Pixels = px(16.0);
const BODY_PAD_V: Pixels = px(14.0);
const BODY_PAD_H: Pixels = px(16.0);
const CFG_ROW_PAD_V: Pixels = px(8.0);
const CFG_ROW_PAD_H: Pixels = px(12.0);
const USED_ROW_PAD_V: Pixels = px(7.0);
const USED_ROW_PAD_H: Pixels = px(10.0);
const FOOTER_PAD_V: Pixels = px(10.0);
const FOOTER_PAD_H: Pixels = px(16.0);
const PLACEHOLDER_PAD_V: Pixels = px(14.0);
const PLACEHOLDER_PAD_H: Pixels = px(12.0);

impl TriggersRegistryView {
    pub(super) fn load_detail(&self, id: TriggerInstanceId, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.repo);
        let action_repo = Arc::clone(&self.action_repo);
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let _ = tx.send(load_detail_data(&*repo, &*action_repo, id).await);
        });
        cx.spawn(async move |this, cx| match rx.await {
            Ok(Ok(Some(data))) => {
                let _ = this.update(cx, |this, cx| this.apply_detail(id, data, cx));
            }
            Ok(Ok(None)) => {
                let _ = this.update(cx, |this, cx| {
                    if this.selected == Some(id) {
                        this.detail = None;
                        cx.notify();
                    }
                });
            }
            Ok(Err(message)) => {
                let _ = this.update(cx, |this, cx| this.on_repo_error(&message, cx));
            }
            Err(_) => {}
        })
        .detach();
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
        let default = descriptor.map(|d| d.default_config()).unwrap_or_default();
        let specs = descriptor.map(|d| d.config_fields()).unwrap_or_default();
        let effective = effective_config(&default, &data.instance.overrides);

        let mut fields: Vec<ConfigField> = Vec::new();
        for spec in &specs {
            fold_config_field(
                spec,
                None,
                &effective,
                &palette,
                Self::on_config_committed,
                &mut fields,
                cx,
            );
        }

        let cooldown_per_user = data.instance.user_cooldown_secs > 0;
        let cooldown_seed = if cooldown_per_user {
            data.instance.user_cooldown_secs
        } else {
            data.instance.global_cooldown_secs
        };
        let cooldown_input = self.build_cooldown_input(cooldown_seed, palette, cx);
        let cooldown_sub = cx.subscribe(&cooldown_input, Self::on_cooldown_committed);

        self.detail = Some(TriggerDetail {
            instance: data.instance,
            fields,
            used_in: data.used_in,
            cooldown_input,
            cooldown_per_user,
            _cooldown_sub: cooldown_sub,
        });
        cx.notify();
    }

    pub(super) fn close_detail(&mut self, cx: &mut Context<Self>) {
        self.selected = None;
        self.detail = None;
        cx.notify();
    }

    fn on_config_committed(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if let InputEvent::Submitted(_) = event {
            self.commit_config(cx);
        }
    }

    fn on_cooldown_committed(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if let InputEvent::Submitted(_) = event {
            self.commit_config(cx);
        }
    }

    fn build_cooldown_input(
        &self,
        seed: u32,
        palette: ForgePalette,
        cx: &mut Context<Self>,
    ) -> Entity<TextInput> {
        cx.new(|cx| {
            let mut input = TextInput::new("0", cx).with_palette(palette);
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

    fn toggle_cooldown_scope(&mut self, cx: &mut Context<Self>) {
        if let Some(detail) = self.detail.as_mut() {
            detail.cooldown_per_user = !detail.cooldown_per_user;
        }
        self.commit_config(cx);
    }

    fn commit_config(&mut self, cx: &mut Context<Self>) {
        let Some(detail) = self.detail.as_ref() else {
            return;
        };
        let kind_id = detail.instance.kind_id.clone();
        let default = self
            .registry
            .get(&kind_id)
            .map(|d| d.default_config())
            .unwrap_or_default();
        let mut buffer = effective_config(&default, &detail.instance.overrides);
        overlay_field_values(&detail.fields, &mut buffer, cx);

        let sparse = sparse_overrides(&default, &buffer);
        let cooldown_secs = parse_cooldown(detail.cooldown_input.read(cx).content());
        let (global_cooldown_secs, user_cooldown_secs) = if detail.cooldown_per_user {
            (0, cooldown_secs)
        } else {
            (cooldown_secs, 0)
        };
        let mut instance = detail.instance.clone();
        instance.overrides = sparse;
        instance.global_cooldown_secs = global_cooldown_secs;
        instance.user_cooldown_secs = user_cooldown_secs;
        let repo = Arc::clone(&self.repo);
        self.spawn_reload(
            async move {
                repo.save(&instance).await.map_err(|e| e.to_string())?;
                load_rows(&*repo).await
            },
            cx,
        );
    }

    fn revert_config_field(&mut self, key: String, cx: &mut Context<Self>) {
        let Some(detail) = self.detail.as_ref() else {
            return;
        };
        let mut instance = detail.instance.clone();
        instance.overrides.remove(&key);
        let repo = Arc::clone(&self.repo);
        self.spawn_reload(
            async move {
                repo.save(&instance).await.map_err(|e| e.to_string())?;
                load_rows(&*repo).await
            },
            cx,
        );
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
                    this.apply_rows(rows, cx);
                    this.selected = Some(new_id);
                    this.detail = None;
                    this.load_detail(new_id, cx);
                    cx.notify();
                });
            }
            Ok(Err(message)) => {
                let _ = this.update(cx, |this, cx| this.on_repo_error(&message, cx));
            }
            Err(_) => {}
        })
        .detach();
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
            None => self.render_detail_loading(palette),
        }
    }

    fn detail_shell(&self, palette: &ForgePalette) -> gpui::Div {
        div()
            .w(SHEET_W)
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .min_h(px(0.0))
            .bg(palette.base)
            .border_l(HALF_BORDER)
            .border_color(palette.border_regular)
    }

    fn render_detail_loading(&self, palette: &ForgePalette) -> AnyElement {
        self.detail_shell(palette)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .font_family(DEFAULT_BODY_FAMILY)
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
        self.detail_shell(palette)
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
            .font_family(DEFAULT_BODY_FAMILY)
            .font_weight(FontWeight::SEMIBOLD)
            .text_size(NAME_FS)
            .text_color(palette.text_primary)
            .child(instance.name.clone())
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.start_rename(id, window, cx)
            }));
        let kind_line = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(status_dot(dot_color, KIND_DOT))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(KIND_FS)
                    .text_color(palette.text_faint)
                    .child(instance.kind_id.clone()),
            );
        let title = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(2.0))
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
            .p(spacing(Spacing::Xxs, Density::Cozy))
            .child(icon(Icon::X, px(15.0), palette.text_faint))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.close_detail(cx)));

        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .py(HEADER_PAD_V)
            .px(HEADER_PAD_H)
            .border_b(HALF_BORDER)
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
        div()
            .id("triggers-detail-body")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .py(BODY_PAD_V)
            .px(BODY_PAD_H)
            .flex()
            .flex_col()
            .child(self.render_config_section(detail, palette, cx))
            .child(self.render_cooldown_section(detail, palette, cx))
            .child(self.render_used_in_section(detail, palette))
            .into_any_element()
    }

    fn section_label(
        &self,
        label: impl Into<SharedString>,
        right: Option<AnyElement>,
        palette: &ForgePalette,
    ) -> AnyElement {
        let label = label.into();
        div()
            .w_full()
            .flex()
            .items_center()
            .py(spacing(Spacing::Xs, Density::Cozy))
            .child(
                div()
                    .flex_1()
                    .font_family(DEFAULT_MONO_FAMILY)
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
        let override_count = detail.instance.overrides.len();
        let right = if override_count > 0 {
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XXS)
                .text_color(palette.bits)
                .child(tr!(
                    "triggers_sheet_config_overridden",
                    count = override_count as i64
                ))
                .into_any_element()
        } else {
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(tr!("triggers_sheet_config_all_defaults"))
                .into_any_element()
        };

        let card: AnyElement = if detail.fields.is_empty() {
            div()
                .py(PLACEHOLDER_PAD_V)
                .px(PLACEHOLDER_PAD_H)
                .italic()
                .font_family(DEFAULT_BODY_FAMILY)
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
                col = col.child(self.render_config_row(field, &overridden, i == last, palette, cx));
            }
            col.into_any_element()
        };

        let framed = div()
            .w_full()
            .rounded(radius(Radius::Md))
            .border(HALF_BORDER)
            .border_color(palette.border_regular)
            .bg(palette.shell)
            .child(card);

        div()
            .flex()
            .flex_col()
            .pb(spacing(Spacing::Md, Density::Cozy))
            .child(self.section_label(
                tr!("triggers_sheet_section_configuration"),
                Some(right),
                palette,
            ))
            .child(framed)
            .into_any_element()
    }

    fn render_config_row(
        &self,
        field: &ConfigField,
        overridden: &HashMap<&str, bool>,
        last: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = match field {
            ConfigField::Input { key, .. }
            | ConfigField::Bool { key, .. }
            | ConfigField::Hint { key } => key.clone(),
        };
        let is_overridden = overridden.contains_key(key.as_str());
        let key_color = if is_overridden {
            palette.bits
        } else {
            palette.text_muted
        };

        let label = div()
            .w(CFG_KEY_W)
            .flex_none()
            .overflow_hidden()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(CFG_KEY_FS)
            .text_color(key_color)
            .child(key.clone());

        let value: AnyElement = match field {
            ConfigField::Input { input, .. } => div().child(input.clone()).into_any_element(),
            ConfigField::Bool { key, value, .. } => {
                let toggle_key = key.clone();
                toggle(*value, palette)
                    .on_click(
                        SharedString::from(format!("triggers-cfg-toggle-{key}")),
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.toggle_config_field(toggle_key.clone(), cx)
                        }),
                    )
                    .into_any_element()
            }
            ConfigField::Hint { .. } => div()
                .italic()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(CFG_VAL_FS)
                .text_color(palette.text_faint)
                .child(tr!("triggers_sheet_config_authored"))
                .into_any_element(),
        };

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
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .py(CFG_ROW_PAD_V)
            .px(CFG_ROW_PAD_H)
            .when(!last, |row| {
                row.border_b(HALF_BORDER)
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
        let caption = div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(tr!("triggers_sheet_cooldown_caption"))
            .into_any_element();

        let label_cell = |label: SharedString| {
            div()
                .w(CFG_KEY_W)
                .flex_none()
                .overflow_hidden()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(CFG_KEY_FS)
                .text_color(palette.text_muted)
                .child(label)
        };

        let value_row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .py(CFG_ROW_PAD_V)
            .px(CFG_ROW_PAD_H)
            .border_b(HALF_BORDER)
            .border_color(palette.border_regular)
            .child(label_cell(tr!("triggers_sheet_cooldown_value").into()))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(detail.cooldown_input.clone()),
            );

        let scope_toggle = toggle(detail.cooldown_per_user, palette).on_click(
            "triggers-detail-cooldown-scope",
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_cooldown_scope(cx)),
        );
        let scope_row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .py(CFG_ROW_PAD_V)
            .px(CFG_ROW_PAD_H)
            .child(label_cell(tr!("triggers_sheet_cooldown_scope").into()))
            .child(div().flex_1().min_w(px(0.0)).child(scope_toggle));

        let card = div().flex().flex_col().child(value_row).child(scope_row);

        let framed = div()
            .w_full()
            .rounded(radius(Radius::Md))
            .border(HALF_BORDER)
            .border_color(palette.border_regular)
            .bg(palette.shell)
            .child(card);

        div()
            .flex()
            .flex_col()
            .pb(spacing(Spacing::Md, Density::Cozy))
            .child(self.section_label(
                tr!("triggers_sheet_section_cooldown"),
                Some(caption),
                palette,
            ))
            .child(framed)
            .into_any_element()
    }

    fn render_used_in_section(&self, detail: &TriggerDetail, palette: &ForgePalette) -> AnyElement {
        let count = detail.used_in.len();
        let label = if count > 0 {
            tr!("triggers_sheet_section_used_in_count", count = count as i64)
        } else {
            tr!("triggers_sheet_section_used_in")
        };

        let inner: AnyElement = if detail.used_in.is_empty() {
            div()
                .w_full()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(2.0))
                .py(PLACEHOLDER_PAD_V)
                .px(PLACEHOLDER_PAD_H)
                .rounded(radius(Radius::Md))
                .border(HALF_BORDER)
                .border_color(palette.border_input)
                .text_center()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(CFG_VAL_FS)
                .text_color(palette.text_faint)
                .child(tr!("triggers_sheet_used_in_empty_title"))
                .child(tr!("triggers_sheet_used_in_empty_hint"))
                .into_any_element()
        } else {
            let mut col = div().flex().flex_col().gap(px(3.0));
            for (_, name) in &detail.used_in {
                col = col.child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap(spacing(Spacing::Xs, Density::Cozy))
                        .py(USED_ROW_PAD_V)
                        .px(USED_ROW_PAD_H)
                        .rounded(radius(Radius::Sm))
                        .bg(palette.shell)
                        .child(icon(Icon::Bolt, ROW_GLYPH, palette.brand))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .overflow_hidden()
                                .font_family(DEFAULT_MONO_FAMILY)
                                .text_size(USED_FS)
                                .text_color(palette.text_primary)
                                .child(name.clone()),
                        ),
                );
            }
            col.into_any_element()
        };

        div()
            .flex()
            .flex_col()
            .child(self.section_label(label, None, palette))
            .child(inner)
            .into_any_element()
    }

    fn render_detail_footer(
        &self,
        detail: &TriggerDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = detail.instance.id;
        let used_count = self.find(id).map(|r| r.used_in_count).unwrap_or(0);
        let can_delete = used_count == 0;

        let template = ghost_button_with_icon(Icon::Copy, tr!("triggers_menu_template"), palette)
            .on_click(
                "triggers-detail-template",
                cx.listener(|this, _: &ClickEvent, _, cx| this.use_as_template(cx)),
            );

        let delete_color = if can_delete {
            palette.random
        } else {
            palette.disabled
        };
        let delete_base = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .py(spacing(Spacing::Xs, Density::Cozy))
            .px(spacing(Spacing::Sm, Density::Cozy))
            .rounded(radius(Radius::Sm))
            .border(HALF_BORDER)
            .border_color(palette.border_input)
            .child(icon(Icon::Eraser, px(11.0), delete_color))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
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
            delete_base
                .opacity(super::DISABLED_OPACITY)
                .into_any_element()
        };

        div()
            .flex_none()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .py(FOOTER_PAD_V)
            .px(FOOTER_PAD_H)
            .bg(palette.shell)
            .border_t(HALF_BORDER)
            .border_color(palette.border_regular)
            .child(template)
            .child(div().flex_1())
            .child(delete)
            .into_any_element()
    }
}

fn parse_cooldown(text: &str) -> u32 {
    text.trim().parse::<u32>().unwrap_or(0)
}

async fn load_detail_data(
    repo: &dyn TriggerInstanceRepo,
    action_repo: &dyn ActionRepo,
    id: TriggerInstanceId,
) -> Result<Option<TriggerDetailData>, String> {
    let Some(instance) = repo.get(id).await.map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let action_ids = repo.actions_using(id).await.map_err(|e| e.to_string())?;
    let mut used_in = Vec::with_capacity(action_ids.len());
    for action_id in action_ids {
        let name = action_repo
            .get(action_id)
            .await
            .map_err(|e| e.to_string())?
            .map(|a| a.name)
            .unwrap_or_else(|| action_id.to_string());
        used_in.push((action_id, name));
    }
    Ok(Some(TriggerDetailData { instance, used_in }))
}
