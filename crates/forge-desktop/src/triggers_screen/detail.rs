//! Triggers registry — detail side-sheet: the async detail pull, the per-field
//! configuration editor folded from the kind's `config_fields` (mirroring the
//! sub-action editor's `FormField` rendering), the used-in list, and the sheet's
//! rename / toggle / delete / use-as-template controls. Every config write
//! reconciles by a full re-pull.

use super::*;
use crate::presentation::ActivePresentation;
use forge_components::{
    DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_XXS, Icon, InputEvent, Radius, Spacing,
    TextInput, ghost_button_with_icon, icon, radius, spacing, status_dot, toggle,
};
use forge_registry::{FormField, effective_config};
use forge_types::TriggerConfig;
use gpui::{AnyElement, ClickEvent, FontWeight, SharedString};
use std::collections::HashMap;

/// Detail side-sheet width. The design pins the docked inspector at a fixed 420px.
const SHEET_W: Pixels = px(420.0);
/// Hairline width the design uses for the sheet's leading edge and the intra-card
/// field dividers (0.5px, below the `BORDER_THIN` token).
const HALF_BORDER: Pixels = px(0.5);
/// Header icon tile side and corner (fixed 30px / 8px) and its centred glyph (16px).
const TILE: Pixels = px(30.0);
const TILE_RADIUS: Pixels = px(8.0);
const TILE_GLYPH: Pixels = px(16.0);
/// Leading status-dot diameter on the kind line (5px) and on a config/used-in row
/// glyph (12px), off the `FONT_*`/`Spacing` scales.
const KIND_DOT: Pixels = px(5.0);
const ROW_GLYPH: Pixels = px(12.0);
/// Off-scale detail font sizes pinned to the design: name 14, kind line 10.5, config
/// key/value 11 / 11.5, used-in name 12.
const NAME_FS: Pixels = px(14.0);
const KIND_FS: Pixels = px(10.5);
const CFG_KEY_FS: Pixels = px(11.0);
const CFG_VAL_FS: Pixels = px(11.5);
const USED_FS: Pixels = px(12.0);
/// The config-key column width in a field row (fixed 110px in the design).
const CFG_KEY_W: Pixels = px(110.0);
/// The trailing revert-affordance slot width (fixed 22px), reserved on every row so
/// overridden and default rows stay column-aligned.
const REVERT_W: Pixels = px(22.0);
/// Header / body / footer / row insets pinned to the design.
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
/// Empty used-in / no-fields placeholder inset.
const PLACEHOLDER_PAD_V: Pixels = px(14.0);
const PLACEHOLDER_PAD_H: Pixels = px(12.0);

/// Renders a `Variant` as the single-line string the field editor seeds and commits.
/// Composite values carry no inline text form.
fn variant_display(v: &Variant) -> String {
    match v {
        Variant::Int(n) => n.to_string(),
        Variant::Float(f) => f.to_string(),
        Variant::Bool(b) => b.to_string(),
        Variant::String(s) => s.clone(),
        Variant::Datetime(dt) => dt.to_string(),
        Variant::Array(_) | Variant::Object(_) => String::new(),
    }
}

/// Keeps only the buffer entries diverging from `default`, so a saved config stores a
/// sparse diff the runtime re-merges over the current defaults rather than freezing
/// today's defaults into the row.
fn sparse_overrides(default: &TriggerConfig, buffer: &TriggerConfig) -> TriggerConfig {
    buffer
        .iter()
        .filter(|(k, v)| default.get(*k) != Some(*v))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

impl TriggersRegistryView {
    // --- detail: async pull -----------------------------------------------

    /// Re-pulls the open sheet's detail from the currently selected instance. A no-op
    /// when nothing is selected.
    pub(super) fn reload_detail(&self, cx: &mut Context<Self>) {
        if let Some(id) = self.selected {
            self.load_detail(id, cx);
        }
    }

    /// Pulls `id`'s fresh instance plus the names of the actions linking it off the
    /// storage provider, then folds the detail on the foreground executor — guarding
    /// on the selection not having moved on while the pull was in flight.
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

    /// Folds a freshly pulled instance into the detail sheet: seeds one editable field
    /// per `config_field` over the effective (default-merged) config, and stores the
    /// resolved used-in list. Discards a pull whose instance is no longer selected.
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
            fold_config_field(spec, None, &effective, &palette, &mut fields, cx);
        }

        self.detail = Some(TriggerDetail {
            instance: data.instance,
            fields,
            used_in: data.used_in,
        });
        cx.notify();
    }

    pub(super) fn close_detail(&mut self, cx: &mut Context<Self>) {
        self.selected = None;
        self.detail = None;
        cx.notify();
    }

    // --- config: per-field edit + revert ----------------------------------

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

    /// Overlays the field values onto the effective config, diffs to a sparse override
    /// set and persists it, then reconciles the roster and (via the roster re-pull) the
    /// sheet by a full re-pull.
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

        let bool_vals: HashMap<&str, bool> = detail
            .fields
            .iter()
            .filter_map(|f| match f {
                ConfigField::Bool { key, value, .. } => Some((key.as_str(), *value)),
                _ => None,
            })
            .collect();
        let gate_on = |gate: &Option<String>| {
            gate.as_ref()
                .map(|g| bool_vals.get(g.as_str()).copied().unwrap_or(false))
                .unwrap_or(true)
        };

        for field in &detail.fields {
            match field {
                ConfigField::Bool {
                    key, value, gate, ..
                } => {
                    if gate_on(gate) {
                        buffer.insert(key.clone(), Variant::Bool(*value));
                    }
                }
                ConfigField::Input {
                    key,
                    integer,
                    gate,
                    input,
                    ..
                } => {
                    if !gate_on(gate) {
                        continue;
                    }
                    let text = input.read(cx).content().to_owned();
                    if *integer {
                        if let Ok(n) = text.trim().parse::<i64>() {
                            buffer.insert(key.clone(), Variant::Int(n));
                        }
                    } else {
                        buffer.insert(key.clone(), Variant::String(text));
                    }
                }
                ConfigField::Hint { .. } => {}
            }
        }

        let sparse = sparse_overrides(&default, &buffer);
        let mut instance = detail.instance.clone();
        instance.overrides = sparse;
        let repo = Arc::clone(&self.repo);
        self.spawn_reload(
            async move {
                repo.save(&instance).await.map_err(|e| e.to_string())?;
                load_rows(&*repo).await
            },
            cx,
        );
    }

    /// Reverts one config key to its schema default by dropping it from the persisted
    /// overrides, then reconciles by a full re-pull.
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

    // --- footer: use as template ------------------------------------------

    /// Clones the selected instance into a new user-defined instance (fresh id, copied
    /// kind / overrides / scope), persists it, re-pulls the roster, then selects the
    /// clone so its detail opens.
    fn use_as_template(&mut self, cx: &mut Context<Self>) {
        let Some(detail) = self.detail.as_ref() else {
            return;
        };
        let src = detail.instance.clone();
        let new_id = TriggerInstanceId::new();
        let instance = TriggerInstance {
            id: new_id,
            name: format!("{} copy", src.name),
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

    // --- render: detail side-sheet ----------------------------------------

    /// The detail side-sheet as a docked pane sharing the body row with the list — a
    /// fixed-width inspector that opens on selection, not a modal scrim (the list stays
    /// live behind it, so selecting another row just swaps the sheet's contents).
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
                    .child("Loading trigger\u{2026}"),
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

        // The name reuses the roster rename verb (opens the rename modal), so a click
        // anywhere on the name row starts an inline rename.
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
                .child(format!("{override_count} overridden"))
                .into_any_element()
        } else {
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child("all defaults")
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
                .child("This trigger kind has no configurable fields.")
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
            .child(self.section_label("CONFIGURATION", Some(right), palette))
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
                .child("Authored on the step")
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

    fn render_used_in_section(&self, detail: &TriggerDetail, palette: &ForgePalette) -> AnyElement {
        let count = detail.used_in.len();
        let label = if count > 0 {
            format!("USED IN ({count})")
        } else {
            "USED IN".to_owned()
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
                .child("Not linked to any action yet.")
                .child("Open an action and add this trigger from the picker.")
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

        let template = ghost_button_with_icon(Icon::Copy, "Use as template", palette).on_click(
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
                    .child("Delete"),
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

/// Folds one `FormField` (recursing through `Optional`) into the flat config-editor
/// field list, seeding each input from the effective config. Select / DynamicSelect
/// degrade to a free-text input — the kit ships no value-picker primitive yet.
fn fold_config_field(
    spec: &FormField,
    gate: Option<String>,
    config: &TriggerConfig,
    palette: &ForgePalette,
    out: &mut Vec<ConfigField>,
    cx: &mut Context<TriggersRegistryView>,
) {
    match spec {
        FormField::Text {
            key, placeholder, ..
        } => out.push(build_config_input(
            key,
            placeholder,
            false,
            gate,
            config,
            palette,
            cx,
        )),
        FormField::TextArea { key, .. } => out.push(build_config_input(
            key, "", false, gate, config, palette, cx,
        )),
        FormField::Integer { key, .. } => out.push(build_config_input(
            key, "0", true, gate, config, palette, cx,
        )),
        FormField::Select { key, .. } | FormField::DynamicSelect { key, .. } => out.push(
            build_config_input(key, "", false, gate, config, palette, cx),
        ),
        FormField::Toggle { key, .. } => {
            let value = matches!(config.get(*key), Some(Variant::Bool(true)));
            out.push(ConfigField::Bool {
                key: (*key).to_owned(),
                gate,
                value,
            });
        }
        FormField::SubChain { key, .. } | FormField::CaseList { key, .. } => {
            out.push(ConfigField::Hint {
                key: (*key).to_owned(),
            });
        }
        FormField::Optional { key, inner, .. } => {
            let value = matches!(config.get(*key), Some(Variant::Bool(true)));
            out.push(ConfigField::Bool {
                key: (*key).to_owned(),
                gate: gate.clone(),
                value,
            });
            fold_config_field(inner, Some((*key).to_owned()), config, palette, out, cx);
        }
    }
}

fn build_config_input(
    key: &str,
    placeholder: &'static str,
    integer: bool,
    gate: Option<String>,
    config: &TriggerConfig,
    palette: &ForgePalette,
    cx: &mut Context<TriggersRegistryView>,
) -> ConfigField {
    let seed = config.get(key).map(variant_display).unwrap_or_default();
    let palette = *palette;
    let input = cx.new(|cx| {
        let mut input = TextInput::new(placeholder, cx).with_palette(palette);
        if !seed.is_empty() {
            input.set_content(seed, cx);
        }
        input
    });
    let sub = cx.subscribe(&input, TriggersRegistryView::on_config_committed);
    ConfigField::Input {
        key: key.to_owned(),
        integer,
        gate,
        input,
        _sub: sub,
    }
}

/// Pulls `id`'s fresh instance and resolves each linking action to its display name,
/// mirroring the count the roster computes. A missing instance yields `None` (the
/// sheet clears); a missing action degrades to its id string.
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
