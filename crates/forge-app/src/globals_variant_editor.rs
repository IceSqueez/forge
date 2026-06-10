use std::sync::Arc;

use forge_storage::{GlobalEntry, GlobalsRepo};
use forge_types::Variant;
use forge_widgets::tokens::{FONT_XS, Spacing, spf};
use forge_widgets::{
    BannerKind, FontRole, ForgePalette, ModalProps, ToggleProps, VariantKind, category_chip, font,
    live_status_banner, modal, primary_button_small, secondary_button, section_header, toggle,
    variant_kind_color,
};
use iced::{
    Alignment, Element, Length,
    widget::{Space, column, row, text},
};

use crate::Message;
use crate::message::{EditorMode, GlobalsMsg, VariantEditorMsg};
use crate::runtime_view::RuntimeView;

#[derive(Debug, Clone, Default)]
pub struct VariantEditorFields {
    pub int_input: String,
    pub float_input: String,
    pub bool_value: bool,
    pub string_input: String,
    pub datetime_input: String,
    pub array_json: String,
    pub object_json: String,
}

#[derive(Debug, Clone)]
pub struct VariantEditorForm {
    pub mode: EditorMode,
    pub name: String,
    pub kind: VariantKind,
    pub persisted: bool,
    pub fields: VariantEditorFields,
    pub error: Option<String>,
    pub saving: bool,
}

impl VariantEditorForm {
    pub fn for_create() -> Self {
        Self {
            mode: EditorMode::Create,
            name: String::new(),
            kind: VariantKind::Int,
            persisted: false,
            fields: VariantEditorFields::default(),
            error: None,
            saving: false,
        }
    }

    pub fn for_edit(entry: &GlobalEntry) -> Self {
        let kind = VariantKind::from_variant(&entry.value);
        let mut fields = VariantEditorFields::default();
        match &entry.value {
            Variant::Int(n) => fields.int_input = n.to_string(),
            Variant::Float(f) => fields.float_input = f.to_string(),
            Variant::Bool(b) => fields.bool_value = *b,
            Variant::String(s) => fields.string_input = s.clone(),
            Variant::Datetime(dt) => {
                fields.datetime_input = dt
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default();
            }
            Variant::Array(_) => {
                if let Ok(json) =
                    serde_json::to_string_pretty(&variant_to_display_json(&entry.value))
                {
                    fields.array_json = json;
                }
            }
            Variant::Object(_) => {
                if let Ok(json) =
                    serde_json::to_string_pretty(&variant_to_display_json(&entry.value))
                {
                    fields.object_json = json;
                }
            }
        }
        Self {
            mode: EditorMode::Edit(entry.name.clone()),
            name: entry.name.clone(),
            kind,
            persisted: entry.persisted,
            fields,
            error: None,
            saving: false,
        }
    }

    pub fn is_valid(&self) -> Option<&'static str> {
        if self.name.trim().is_empty() {
            return Some("Name is required");
        }
        match self.kind {
            VariantKind::Int => {
                if self.fields.int_input.parse::<i64>().is_err() {
                    Some("Invalid integer")
                } else {
                    None
                }
            }
            VariantKind::Float => {
                if self.fields.float_input.parse::<f64>().is_err() {
                    Some("Invalid float")
                } else {
                    None
                }
            }
            VariantKind::Bool | VariantKind::String => None,
            VariantKind::Datetime => {
                if time::OffsetDateTime::parse(
                    &self.fields.datetime_input,
                    &time::format_description::well_known::Rfc3339,
                )
                .is_err()
                {
                    Some("Invalid ISO 8601 datetime (e.g. 2026-05-18T14:23:00Z)")
                } else {
                    None
                }
            }
            VariantKind::Array => {
                match serde_json::from_str::<serde_json::Value>(&self.fields.array_json) {
                    Err(_) => Some("Invalid JSON array"),
                    Ok(v) => {
                        if v.is_array() && Variant::from_json(v).is_ok() {
                            None
                        } else {
                            Some("Invalid JSON array")
                        }
                    }
                }
            }
            VariantKind::Object => {
                match serde_json::from_str::<serde_json::Value>(&self.fields.object_json) {
                    Err(_) => Some("Invalid JSON object"),
                    Ok(v) => {
                        if v.is_object() && Variant::from_json(v).is_ok() {
                            None
                        } else {
                            Some("Invalid JSON object")
                        }
                    }
                }
            }
        }
    }

    pub fn build_variant(&self) -> Option<Variant> {
        match self.kind {
            VariantKind::Int => self.fields.int_input.parse::<i64>().ok().map(Variant::Int),
            VariantKind::Float => self
                .fields
                .float_input
                .parse::<f64>()
                .ok()
                .and_then(|f| Variant::float(f).ok()),
            VariantKind::Bool => Some(Variant::Bool(self.fields.bool_value)),
            VariantKind::String => Some(Variant::String(self.fields.string_input.clone())),
            VariantKind::Datetime => time::OffsetDateTime::parse(
                &self.fields.datetime_input,
                &time::format_description::well_known::Rfc3339,
            )
            .ok()
            .map(Variant::Datetime),
            VariantKind::Array => {
                serde_json::from_str::<serde_json::Value>(&self.fields.array_json)
                    .ok()
                    .and_then(|v| Variant::from_json(v).ok())
            }
            VariantKind::Object => {
                serde_json::from_str::<serde_json::Value>(&self.fields.object_json)
                    .ok()
                    .and_then(|v| Variant::from_json(v).ok())
            }
        }
    }
}

fn variant_to_display_json(v: &Variant) -> serde_json::Value {
    match v {
        Variant::Int(n) => serde_json::Value::Number((*n).into()),
        Variant::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Variant::Bool(b) => serde_json::Value::Bool(*b),
        Variant::String(s) => serde_json::Value::String(s.clone()),
        Variant::Datetime(dt) => serde_json::Value::String(
            dt.format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_else(|_| String::new()),
        ),
        Variant::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(variant_to_display_json).collect())
        }
        Variant::Object(map) => serde_json::Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), variant_to_display_json(v)))
                .collect(),
        ),
    }
}

pub fn update_variant_editor(
    editor: &mut Option<VariantEditorForm>,
    rt: &RuntimeView,
    sub: VariantEditorMsg,
) -> iced::Task<Message> {
    match sub {
        VariantEditorMsg::OpenCreate => {
            *editor = Some(VariantEditorForm::for_create());
            iced::Task::none()
        }

        VariantEditorMsg::OpenEdit(_name, entry) => {
            *editor = Some(VariantEditorForm::for_edit(&entry));
            iced::Task::none()
        }

        VariantEditorMsg::Cancel => {
            *editor = None;
            iced::Task::none()
        }

        VariantEditorMsg::NameChanged(v) => {
            if let Some(f) = editor.as_mut() {
                f.name = v;
            }
            iced::Task::none()
        }

        VariantEditorMsg::KindSelected(kind) => {
            if let Some(f) = editor.as_mut() {
                f.kind = kind;
                f.error = None;
            }
            iced::Task::none()
        }

        VariantEditorMsg::PersistenceToggled(v) => {
            if let Some(f) = editor.as_mut() {
                f.persisted = v;
            }
            iced::Task::none()
        }

        VariantEditorMsg::IntInputChanged(v) => {
            if let Some(f) = editor.as_mut() {
                f.fields.int_input = v;
            }
            iced::Task::none()
        }

        VariantEditorMsg::FloatInputChanged(v) => {
            if let Some(f) = editor.as_mut() {
                f.fields.float_input = v;
            }
            iced::Task::none()
        }

        VariantEditorMsg::BoolValueChanged(v) => {
            if let Some(f) = editor.as_mut() {
                f.fields.bool_value = v;
            }
            iced::Task::none()
        }

        VariantEditorMsg::StringInputChanged(v) => {
            if let Some(f) = editor.as_mut() {
                f.fields.string_input = v;
            }
            iced::Task::none()
        }

        VariantEditorMsg::DatetimeInputChanged(v) => {
            if let Some(f) = editor.as_mut() {
                f.fields.datetime_input = v;
            }
            iced::Task::none()
        }

        VariantEditorMsg::ArrayJsonChanged(v) => {
            if let Some(f) = editor.as_mut() {
                f.fields.array_json = v;
            }
            iced::Task::none()
        }

        VariantEditorMsg::ObjectJsonChanged(v) => {
            if let Some(f) = editor.as_mut() {
                f.fields.object_json = v;
            }
            iced::Task::none()
        }

        VariantEditorMsg::Submit => {
            let Some(form) = editor.as_ref() else {
                return iced::Task::none();
            };
            if form.is_valid().is_some() {
                return iced::Task::none();
            }
            let Some(variant) = form.build_variant() else {
                return iced::Task::none();
            };
            let name = form.name.trim().to_owned();
            let persisted = form.persisted;
            let old_name = match &form.mode {
                EditorMode::Create => None,
                EditorMode::Edit(original) if original.as_str() != name.as_str() => {
                    Some(original.clone())
                }
                EditorMode::Edit(_) => None,
            };
            if let Some(f) = editor.as_mut() {
                f.saving = true;
            }
            let dp = Arc::clone(&rt.backend);
            iced::Task::perform(
                async move {
                    if let Some(old) = old_name {
                        let g: &dyn GlobalsRepo = &*dp;
                        g.delete(&old)
                            .await
                            .map_err(|e| e.to_string())
                            .map(|_| ())?;
                    }
                    let g: &dyn GlobalsRepo = &*dp;
                    g.set(&name, variant, persisted)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| Message::Globals(GlobalsMsg::VariantEditor(VariantEditorMsg::Saved(r))),
            )
        }

        VariantEditorMsg::Saved(Ok(())) => {
            *editor = None;
            iced::Task::done(Message::Globals(GlobalsMsg::LoadRequested))
        }

        VariantEditorMsg::Saved(Err(e)) => {
            if let Some(f) = editor.as_mut() {
                f.error = Some(e);
                f.saving = false;
            }
            iced::Task::none()
        }
    }
}

pub fn variant_editor_modal_view<'a>(
    form: &'a VariantEditorForm,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let title = match &form.mode {
        EditorMode::Create => "New variable",
        EditorMode::Edit(_) => "Edit variable",
    };

    let name_count = format!("{}/64", form.name.len().min(64));
    let name_counter = text(name_count)
        .size(FONT_XS)
        .color(palette.text_faint)
        .font(font(FontRole::Monospace));
    let name_input = forge_widgets::text_input_field(
        "my_variable",
        &form.name,
        |v| Message::Globals(GlobalsMsg::VariantEditor(VariantEditorMsg::NameChanged(v))),
        palette,
    );
    let name_row = row![name_input, name_counter]
        .spacing(spf(Spacing::Xs))
        .align_y(Alignment::Center);
    let name_block =
        column![section_header("NAME", None, palette), name_row].spacing(spf(Spacing::Xxs));

    let kinds = [
        VariantKind::Int,
        VariantKind::Float,
        VariantKind::Bool,
        VariantKind::String,
        VariantKind::Datetime,
        VariantKind::Array,
        VariantKind::Object,
    ];
    let chips_row = kinds
        .iter()
        .fold(row![].spacing(spf(Spacing::Xxs)), |acc, &k| {
            acc.push(category_chip(
                palette,
                k.label(),
                variant_kind_color(k, palette),
                form.kind == k,
                Message::Globals(GlobalsMsg::VariantEditor(VariantEditorMsg::KindSelected(k))),
            ))
        });
    let type_block =
        column![section_header("TYPE", None, palette), chips_row].spacing(spf(Spacing::Xxs));

    let persist_toggle = toggle(
        palette,
        ToggleProps {
            label: "Save across restarts".to_owned(),
            description: "Persisted globals survive app close; session-only reset on launch"
                .to_owned(),
            value: form.persisted,
            on_toggle: Message::Globals(GlobalsMsg::VariantEditor(
                VariantEditorMsg::PersistenceToggled(!form.persisted),
            )),
        },
    );
    let persist_block = column![section_header("PERSISTENCE", None, palette), persist_toggle]
        .spacing(spf(Spacing::Xxs));

    let value_editor: Element<'_, Message> = match form.kind {
        VariantKind::Int => forge_widgets::text_input_field(
            "0",
            &form.fields.int_input,
            |v| {
                Message::Globals(GlobalsMsg::VariantEditor(
                    VariantEditorMsg::IntInputChanged(v),
                ))
            },
            palette,
        ),
        VariantKind::Float => forge_widgets::text_input_field(
            "0.0",
            &form.fields.float_input,
            |v| {
                Message::Globals(GlobalsMsg::VariantEditor(
                    VariantEditorMsg::FloatInputChanged(v),
                ))
            },
            palette,
        ),
        VariantKind::Bool => toggle(
            palette,
            ToggleProps {
                label: "Value".to_owned(),
                description: String::new(),
                value: form.fields.bool_value,
                on_toggle: Message::Globals(GlobalsMsg::VariantEditor(
                    VariantEditorMsg::BoolValueChanged(!form.fields.bool_value),
                )),
            },
        ),
        VariantKind::String => forge_widgets::text_input_field(
            "",
            &form.fields.string_input,
            |v| {
                Message::Globals(GlobalsMsg::VariantEditor(
                    VariantEditorMsg::StringInputChanged(v),
                ))
            },
            palette,
        ),
        VariantKind::Datetime => forge_widgets::text_input_field(
            "2026-05-18T14:23:00Z",
            &form.fields.datetime_input,
            |v| {
                Message::Globals(GlobalsMsg::VariantEditor(
                    VariantEditorMsg::DatetimeInputChanged(v),
                ))
            },
            palette,
        ),
        VariantKind::Array => forge_widgets::text_input_field(
            "[1, 2, 3]",
            &form.fields.array_json,
            |v| {
                Message::Globals(GlobalsMsg::VariantEditor(
                    VariantEditorMsg::ArrayJsonChanged(v),
                ))
            },
            palette,
        ),
        VariantKind::Object => forge_widgets::text_input_field(
            r#"{"key": "value"}"#,
            &form.fields.object_json,
            |v| {
                Message::Globals(GlobalsMsg::VariantEditor(
                    VariantEditorMsg::ObjectJsonChanged(v),
                ))
            },
            palette,
        ),
    };
    let value_block =
        column![section_header("VALUE", None, palette), value_editor].spacing(spf(Spacing::Xxs));

    let mut body_col =
        column![name_block, type_block, persist_block, value_block].spacing(spf(Spacing::Sm));
    if let Some(err) = form.error.as_deref() {
        body_col = body_col.push(live_status_banner(BannerKind::Error, err, None, palette));
    }
    let body: Element<'_, Message> = body_col.into();

    let cancel_btn = secondary_button(
        "Cancel",
        Message::Globals(GlobalsMsg::VariantEditor(VariantEditorMsg::Cancel)),
        palette,
    );
    let is_saveable = form.is_valid().is_none() && !form.saving;
    let save_label = if form.saving { "Saving..." } else { "Save" };
    let save_btn: Element<'_, Message> = if is_saveable {
        primary_button_small(
            save_label,
            Message::Globals(GlobalsMsg::VariantEditor(VariantEditorMsg::Submit)),
            palette,
        )
    } else {
        secondary_button(save_label, Message::Noop, palette)
    };
    let footer: Element<'_, Message> =
        row![cancel_btn, Space::new().width(Length::Fill), save_btn,]
            .align_y(Alignment::Center)
            .into();

    modal(
        palette,
        ModalProps {
            title: std::borrow::Cow::Borrowed(title),
            on_close: Message::Globals(GlobalsMsg::VariantEditor(VariantEditorMsg::Cancel)),
            kbd_hint: Some("ESC to cancel"),
        },
        body,
        footer,
    )
}
