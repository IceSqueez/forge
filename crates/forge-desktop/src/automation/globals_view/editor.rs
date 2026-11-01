use crate::automation::globals::{Global, Globals, variant_kind_color};
use forge_components::{
    ActivePresentation, BORDER_THIN, ChipGlyph, CodeEditor, Density, FONT_XS, FONT_XXS,
    ForgePalette, Icon, InputEvent, Language, ModalSize, NAME_FIELD_LIMIT, OverlayPosition, Radius,
    Spacing, TINT_BORDER_ALPHA, TINT_FILL_ALPHA, TextInput, chip, icon, modal, overlay,
    primary_button, radius, secondary_button, spacing, toggle, tr, with_alpha,
};
use forge_types::{Variant, VariantKind};
use gpui::Div;
use gpui::prelude::*;
use gpui::{
    App, ClickEvent, Context, Entity, EventEmitter, SharedString, Subscription, Window, div,
};

const VALUE_AREA_ROWS: std::ops::RangeInclusive<usize> = 3..=12;

#[derive(Clone)]
pub(super) enum EditorMode {
    Create,
    Edit(SharedString),
}

pub(super) struct GlobalDraft {
    pub(super) name: String,
    pub(super) variant: Variant,
    pub(super) persisted: bool,
    pub(super) rename_from: Option<String>,
}

pub(super) enum GlobalEditorEvent {
    Submit(GlobalDraft),
    Cancel,
}

pub(super) struct GlobalEditor {
    pub(super) mode: EditorMode,
    pub(super) kind: VariantKind,
    pub(super) persisted: bool,
    bool_value: bool,
    name_input: Entity<TextInput>,
    value_input: Entity<TextInput>,
    value_area: Entity<CodeEditor>,
    pub(super) error: Option<SharedString>,
    pub(super) saving: bool,
    pub(super) globals: Entity<Globals>,
    _name_sub: Subscription,
    _value_sub: Subscription,
    _area_sub: Subscription,
}

impl EventEmitter<GlobalEditorEvent> for GlobalEditor {}

impl GlobalEditor {
    pub(super) fn new(
        mode: EditorMode,
        kind: VariantKind,
        persisted: bool,
        prefill: Option<&Global>,
        globals: Entity<Globals>,
        cx: &mut Context<Self>,
    ) -> Self {
        let name_seed = prefill.map(|g| g.name.to_string()).unwrap_or_default();
        let name_input = cx.new(|cx| {
            let mut ti = TextInput::new(tr!("globals_editor_name_placeholder"), cx);
            ti.set_content(name_seed, cx);
            ti
        });

        let single_seed = prefill.and_then(|g| single_line_seed(&g.value));
        let value_input = cx.new(|cx| {
            let mut ti = TextInput::new(single_line_placeholder(kind), cx);
            if let Some(seed) = single_seed {
                ti.set_content(seed, cx);
            }
            ti
        });

        let area_seed = prefill.and_then(|g| json_seed(&g.value));
        let value_area = cx.new(|cx| {
            let mut ta = CodeEditor::new(Language::Json, json_placeholder(kind), cx)
                .with_field_rows(VALUE_AREA_ROWS);
            if let Some(seed) = area_seed {
                ta.set_content(seed, cx);
            }
            ta
        });

        let bool_value = matches!(prefill.map(|g| &g.value), Some(Variant::Bool(true)));

        let name_sub = cx.subscribe(
            &name_input,
            |this, _f, event: &InputEvent, cx| match event {
                InputEvent::Submitted(_) => this.submit(cx),
                InputEvent::Cancelled => this.cancel(cx),
                InputEvent::Changed(_) => cx.notify(),
                InputEvent::Blurred(_) => {}
            },
        );
        let value_sub = cx.subscribe(
            &value_input,
            |this, _f, event: &InputEvent, cx| match event {
                InputEvent::Submitted(_) => this.submit(cx),
                InputEvent::Cancelled => this.cancel(cx),
                InputEvent::Changed(_) => cx.notify(),
                InputEvent::Blurred(_) => {}
            },
        );
        let area_sub = cx.subscribe(&value_area, |_this, _f, _event: &InputEvent, cx| {
            cx.notify()
        });

        GlobalEditor {
            mode,
            kind,
            persisted,
            bool_value,
            name_input,
            value_input,
            value_area,
            error: None,
            saving: false,
            globals,
            _name_sub: name_sub,
            _value_sub: value_sub,
            _area_sub: area_sub,
        }
    }

    pub(super) fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.name_input.update(cx, |f, cx| f.focus(window, cx));
    }

    fn select_kind(&mut self, kind: VariantKind, cx: &mut Context<Self>) {
        if !matches!(self.mode, EditorMode::Create) {
            return;
        }
        let value_input = cx.new(|cx| TextInput::new(single_line_placeholder(kind), cx));
        let value_sub = cx.subscribe(
            &value_input,
            |this, _f, event: &InputEvent, cx| match event {
                InputEvent::Submitted(_) => this.submit(cx),
                InputEvent::Cancelled => this.cancel(cx),
                InputEvent::Changed(_) => cx.notify(),
                InputEvent::Blurred(_) => {}
            },
        );
        let value_area = cx.new(|cx| {
            CodeEditor::new(Language::Json, json_placeholder(kind), cx)
                .with_field_rows(VALUE_AREA_ROWS)
        });
        let area_sub = cx.subscribe(&value_area, |_this, _f, _event: &InputEvent, cx| {
            cx.notify()
        });
        self.kind = kind;
        self.error = None;
        self.value_input = value_input;
        self._value_sub = value_sub;
        self.value_area = value_area;
        self._area_sub = area_sub;
        cx.notify();
    }

    pub(super) fn toggle_persist(&mut self, cx: &mut Context<Self>) {
        self.persisted = !self.persisted;
        cx.notify();
    }

    fn toggle_bool_value(&mut self, cx: &mut Context<Self>) {
        self.bool_value = !self.bool_value;
        cx.notify();
    }

    fn set_error(&mut self, message: SharedString, cx: &mut Context<Self>) {
        self.error = Some(message);
        cx.notify();
    }

    fn is_saveable(&self, cx: &App) -> bool {
        let name = self.name(cx);
        if name.is_empty() {
            return false;
        }
        if self.globals.read(cx).contains(&name) && self.original_name() != Some(name.as_str()) {
            return false;
        }
        self.build_variant(cx).is_ok()
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        let name = self.name(cx);
        let original = self.original_name().map(str::to_owned);
        let build = self.build_variant(cx);

        if name.is_empty() {
            self.set_error(tr!("globals_error_name_required").into(), cx);
            return;
        }
        if name.chars().count() > NAME_FIELD_LIMIT {
            self.set_error(
                tr!(
                    "globals_error_name_too_long",
                    limit = i64::try_from(NAME_FIELD_LIMIT).unwrap_or(i64::MAX)
                )
                .into(),
                cx,
            );
            return;
        }
        let collides =
            self.globals.read(cx).contains(&name) && original.as_deref() != Some(name.as_str());
        if collides {
            self.set_error(tr!("globals_error_name_taken").into(), cx);
            return;
        }
        let variant = match build {
            Ok(v) => v,
            Err(reason) => {
                self.set_error(reason, cx);
                return;
            }
        };
        let persisted = self.persisted;
        let rename_from = match &original {
            Some(old) if old.as_str() != name.as_str() => Some(old.clone()),
            _ => None,
        };
        self.error = None;
        cx.emit(GlobalEditorEvent::Submit(GlobalDraft {
            name,
            variant,
            persisted,
            rename_from,
        }));
    }

    pub(super) fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(GlobalEditorEvent::Cancel);
    }

    fn build_variant(&self, cx: &gpui::App) -> Result<Variant, SharedString> {
        match self.kind {
            VariantKind::Int => self
                .value_input
                .read(cx)
                .content()
                .trim()
                .parse::<i64>()
                .map(Variant::Int)
                .map_err(|_| tr!("globals_error_invalid_int").into()),
            VariantKind::Float => {
                let raw = self.value_input.read(cx).content().trim().to_owned();
                let parsed = raw
                    .parse::<f64>()
                    .map_err(|_| tr!("globals_error_invalid_float"))?;
                Variant::float(parsed).map_err(|_| tr!("globals_error_invalid_float").into())
            }
            VariantKind::Bool => Ok(Variant::Bool(self.bool_value)),
            VariantKind::String => Ok(Variant::String(
                self.value_input.read(cx).content().to_owned(),
            )),
            VariantKind::Datetime => {
                forge_types::datetime_from_text(self.value_input.read(cx).content())
                    .map(Variant::Datetime)
                    .ok_or_else(|| tr!("globals_error_invalid_datetime").into())
            }
            VariantKind::Array => parse_json_variant(
                self.value_area.read(cx).content(),
                true,
                tr!("globals_error_invalid_json_array").into(),
            ),
            VariantKind::Object => parse_json_variant(
                self.value_area.read(cx).content(),
                false,
                tr!("globals_error_invalid_json_object").into(),
            ),
        }
    }

    pub(super) fn name(&self, cx: &gpui::App) -> String {
        self.name_input.read(cx).content().trim().to_owned()
    }

    fn original_name(&self) -> Option<&str> {
        match &self.mode {
            EditorMode::Create => None,
            EditorMode::Edit(original) => Some(original.as_ref()),
        }
    }
}

impl GlobalEditor {
    fn name_section(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let name_len = self.name_input.read(cx).content().chars().count();
        let name_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(div().flex_1().child(self.name_input.clone()))
            .child(
                div()
                    .font_family(cx.mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(format!("{name_len}/{NAME_FIELD_LIMIT}")),
            );
        section(
            palette,
            tr!("globals_editor_section_name"),
            name_row,
            density,
            cx,
        )
    }

    fn type_section(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let locked = matches!(self.mode, EditorMode::Edit(_));
        let mut chips = div().flex().flex_wrap().gap(spacing(Spacing::Xxs, density));
        for (i, &kind) in VariantKind::ALL.iter().enumerate() {
            let active = self.kind == kind;
            let mut c = chip(
                kind.contract_name(),
                ChipGlyph::Dot(variant_kind_color(kind, palette)),
                active,
                palette,
            );
            if !locked {
                c = c.on_click(
                    ("globals-kind", i),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.select_kind(kind, cx)),
                );
            }
            chips = chips.child(c);
        }
        let mut type_children = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(chips);
        if locked {
            type_children = type_children.child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(tr!("globals_editor_type_locked_hint")),
            );
        }
        section(
            palette,
            tr!("globals_editor_section_type"),
            type_children,
            density,
            cx,
        )
    }

    fn persist_section(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let persist_row = div()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .font_family(cx.body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_primary)
                            .child(tr!("globals_editor_persist_label")),
                    )
                    .child(
                        div()
                            .font_family(cx.body_family())
                            .text_size(FONT_XXS)
                            .text_color(palette.text_muted)
                            .child(tr!("globals_editor_persist_desc")),
                    ),
            )
            .child(toggle(self.persisted, palette).on_click(
                "globals-editor-persist",
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_persist(cx)),
            ));
        section(
            palette,
            tr!("globals_editor_section_persistence"),
            persist_row,
            density,
            cx,
        )
    }

    fn footer(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> Div {
        let saveable = self.is_saveable(cx) && !self.saving;
        let save_label: SharedString = if self.saving {
            tr!("globals_editor_saving").into()
        } else {
            tr!("globals_editor_save").into()
        };
        let cancel = secondary_button(tr!("globals_editor_cancel"), palette).on_click(
            "globals-editor-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
        );
        let save = primary_button(save_label, palette)
            .disabled(!saveable)
            .on_click(
                "globals-editor-save",
                cx.listener(|this, _: &ClickEvent, _, cx| this.submit(cx)),
            );
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(cancel)
            .child(save)
    }
}

fn error_banner(message: SharedString, palette: &ForgePalette, density: Density, cx: &App) -> Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, density))
        .p(spacing(Spacing::Xs, density))
        .rounded(radius(Radius::Sm))
        .bg(with_alpha(palette.danger, TINT_FILL_ALPHA))
        .border(BORDER_THIN)
        .border_color(with_alpha(palette.danger, TINT_BORDER_ALPHA))
        .child(icon(Icon::AlertTriangle, FONT_XS, palette.danger))
        .child(
            div()
                .font_family(cx.body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_primary)
                .child(message),
        )
}

impl Render for GlobalEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let density = cx.density();
        let palette = cx.palette();
        let palette = &palette;
        let ed = &*self;
        let title = match ed.mode {
            EditorMode::Create => tr!("globals_editor_title_create"),
            EditorMode::Edit(_) => tr!("globals_editor_title_edit"),
        };

        let value_control = self.editor_value_control(palette, cx);
        let value_section = section(
            palette,
            tr!("globals_editor_section_value"),
            value_control,
            density,
            cx,
        )
        .into_any_element();

        let mut body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(self.name_section(palette, density, cx))
            .child(self.type_section(palette, density, cx))
            .child(self.persist_section(palette, density, cx))
            .child(value_section);
        if let Some(err) = ed.error.clone() {
            body = body.child(error_banner(err, palette, density, cx));
        }

        let card = modal(title, body, palette)
            .header_icon(Icon::Variable, palette.warning)
            .size(ModalSize::Md)
            .footer(self.footer(palette, cx))
            .on_close(
                "globals-editor-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .busy(self.saving)
            .on_dismiss("globals-editor-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel(cx));
            })
    }
}

impl GlobalEditor {
    fn editor_value_control(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let ed = self;
        match ed.kind {
            VariantKind::Bool => {
                let (label, hue) = if ed.bool_value {
                    ("true", palette.success)
                } else {
                    ("false", palette.danger)
                };
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(toggle(ed.bool_value, palette).on_click(
                        "globals-editor-bool",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_bool_value(cx)),
                    ))
                    .child(
                        div()
                            .font_family(cx.mono_family())
                            .text_size(FONT_XS)
                            .text_color(hue)
                            .child(label),
                    )
                    .into_any_element()
            }
            VariantKind::Array | VariantKind::Object => {
                div().child(ed.value_area.clone()).into_any_element()
            }
            _ => div().child(ed.value_input.clone()).into_any_element(),
        }
    }
}

fn section(
    palette: &ForgePalette,
    label: impl Into<SharedString>,
    control: impl IntoElement,
    density: Density,
    cx: &App,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, density))
        .child(
            div()
                .font_family(cx.mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(label.into()),
        )
        .child(control)
}

fn single_line_placeholder(kind: VariantKind) -> &'static str {
    match kind {
        VariantKind::Int => "0",
        VariantKind::Float => "0.0",
        VariantKind::Datetime => "2026-05-18T14:23:00Z",
        _ => "",
    }
}

fn json_placeholder(kind: VariantKind) -> &'static str {
    match kind {
        VariantKind::Object => r#"{"key": "value"}"#,
        _ => "[1, 2, 3]",
    }
}

pub(super) fn single_line_seed(value: &Variant) -> Option<String> {
    match value {
        Variant::Int(n) => Some(n.to_string()),
        Variant::Float(f) => Some(f.to_string()),
        Variant::String(s) => Some(s.clone()),
        Variant::Datetime(dt) => Some(forge_types::datetime_to_text(dt)),
        _ => None,
    }
}

pub(super) fn json_seed(value: &Variant) -> Option<String> {
    match value {
        Variant::Array(_) | Variant::Object(_) => {
            serde_json::to_string_pretty(&value.to_plain_json()).ok()
        }
        _ => None,
    }
}

pub(super) fn parse_json_variant(
    text: &str,
    want_array: bool,
    reason: SharedString,
) -> Result<Variant, SharedString> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|_| reason.clone())?;
    let shape_ok = if want_array {
        value.is_array()
    } else {
        value.is_object()
    };
    if !shape_ok {
        return Err(reason);
    }
    Variant::from_json(value).map_err(|_| reason)
}
