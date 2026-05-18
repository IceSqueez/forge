use std::path::PathBuf;
use std::sync::Arc;

use forge_storage::{GlobalEntry, GlobalsExport, GlobalsRepo, StorageError};
use forge_storage_sqlite::SqliteBackend;
use forge_types::Variant;
use forge_widgets::tokens::{FONT_BODY_MD, FONT_BODY_SM, FONT_CAPS_SM};
use forge_widgets::{
    BannerKind, FontRole, FooterProps, ForgePalette, ModalProps, ToggleProps, VariantKind,
    category_chip, data_screen_footer, data_table, empty_state, font, live_status_banner, modal,
    persistence_toggle_inline, primary_button_small, search_input, secondary_button,
    section_header, toggle, type_pill, value_preview,
};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Shadow,
    widget::{Space, button, column, container, row, rule, scrollable, text},
};
use time::OffsetDateTime;

use crate::Message;
use crate::app::App;
use crate::message::{EditorMode, GlobalsFilter, GlobalsLoadData, GlobalsMsg, VariantEditorMsg};

#[derive(Debug, thiserror::Error)]
enum ExportError {
    #[error("user cancelled")]
    Cancelled,
    #[error("storage: {0}")]
    Storage(#[from] StorageError),
    #[error("serialize: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

async fn export_globals_to_chosen_file(dp: Arc<SqliteBackend>) -> Result<PathBuf, ExportError> {
    let entries = dp.export_all().await?;
    let envelope = GlobalsExport::new(entries);
    let json = serde_json::to_string_pretty(&envelope)?;
    let default_name = format!(
        "forge-globals-{}.json",
        time::OffsetDateTime::now_utc().unix_timestamp()
    );
    let handle = rfd::AsyncFileDialog::new()
        .add_filter("JSON", &["json"])
        .set_file_name(&default_name)
        .save_file()
        .await
        .ok_or(ExportError::Cancelled)?;
    let path = handle.path().to_path_buf();
    tokio::fs::write(&path, json).await?;
    Ok(path)
}

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

pub struct GlobalsState {
    pub entries: Vec<GlobalEntry>,
    pub filter: GlobalsFilter,
    pub search: String,
    pub storage_bytes: u64,
    pub last_save: Option<OffsetDateTime>,
    pub loading: bool,
    pub position_display: String,
    pub storage_display: String,
    pub save_display: String,
    pub editor: Option<VariantEditorForm>,
}

impl GlobalsState {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            filter: GlobalsFilter::All,
            search: String::new(),
            storage_bytes: 0,
            last_save: None,
            loading: false,
            position_display: "Showing 0 of 0 · sorted by name asc".to_owned(),
            storage_display: "Storage: 0 B".to_owned(),
            save_display: "Not yet saved".to_owned(),
            editor: None,
        }
    }

    pub fn filtered_entries<'a>(&'a self) -> impl Iterator<Item = &'a GlobalEntry> + 'a {
        let filter = self.filter;
        let search_lower = self.search.to_lowercase();
        self.entries
            .iter()
            .filter(move |e| match filter {
                GlobalsFilter::All => true,
                GlobalsFilter::Persisted => e.persisted,
                GlobalsFilter::Session => !e.persisted,
            })
            .filter(move |e| {
                search_lower.is_empty() || e.name.to_lowercase().contains(&search_lower)
            })
    }

    fn refresh_displays(&mut self) {
        let visible = {
            let filter = self.filter;
            let search_lower = self.search.to_lowercase();
            self.entries
                .iter()
                .filter(|e| match filter {
                    GlobalsFilter::All => true,
                    GlobalsFilter::Persisted => e.persisted,
                    GlobalsFilter::Session => !e.persisted,
                })
                .filter(|e| {
                    search_lower.is_empty() || e.name.to_lowercase().contains(&search_lower)
                })
                .count()
        };
        let total = self.entries.len();
        self.position_display = format!("Showing {visible} of {total} · sorted by name asc");
        self.storage_display = format_storage_bytes(self.storage_bytes);
        self.save_display = format_save_ago(self.last_save);
    }
}

impl Default for GlobalsState {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn load_globals_data(dp: Arc<SqliteBackend>) -> Result<GlobalsLoadData, String> {
    let mut entries = dp.list().await.map_err(|e| e.to_string())?;
    entries.sort_unstable_by(|a, b| a.name.cmp(&b.name));
    let storage_bytes = dp.storage_bytes().await.map_err(|e| e.to_string())?;
    let last_save = dp.last_save_at().await.map_err(|e| e.to_string())?;
    Ok(GlobalsLoadData {
        entries,
        storage_bytes,
        last_save,
    })
}

pub fn handle_globals_msg(app: &mut App, sub: GlobalsMsg) -> iced::Task<Message> {
    match sub {
        GlobalsMsg::LoadRequested => {
            app.globals.loading = true;
            let dp = Arc::clone(&app.backend);
            iced::Task::perform(
                async move { load_globals_data(dp).await.map_err(|e| e.to_string()) },
                |r| Message::Globals(GlobalsMsg::EntriesLoaded(r)),
            )
        }

        GlobalsMsg::EntriesLoaded(Ok(data)) => {
            app.globals.entries = data.entries;
            app.globals.storage_bytes = data.storage_bytes;
            app.globals.last_save = data.last_save;
            app.globals.loading = false;
            app.globals.refresh_displays();
            iced::Task::none()
        }

        GlobalsMsg::EntriesLoaded(Err(e)) => {
            tracing::warn!(error = %e, "globals load failed");
            app.globals.loading = false;
            iced::Task::none()
        }

        GlobalsMsg::FilterSelected(f) => {
            app.globals.filter = f;
            app.globals.refresh_displays();
            iced::Task::none()
        }

        GlobalsMsg::SearchChanged(s) => {
            app.globals.search = s;
            app.globals.refresh_displays();
            iced::Task::none()
        }

        GlobalsMsg::TogglePersistence(name, new_persisted) => {
            let Some(entry) = app.globals.entries.iter().find(|e| e.name == name).cloned() else {
                return iced::Task::none();
            };
            if let Some(e) = app.globals.entries.iter_mut().find(|e| e.name == name) {
                e.persisted = new_persisted;
            }
            app.globals.refresh_displays();
            let dp = Arc::clone(&app.backend);
            iced::Task::perform(
                async move {
                    dp.set(&name, entry.value, new_persisted)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| Message::Globals(GlobalsMsg::PersistenceToggled(r)),
            )
        }

        GlobalsMsg::PersistenceToggled(Err(e)) => {
            tracing::warn!(error = %e, "failed to toggle global persistence");
            iced::Task::none()
        }

        GlobalsMsg::PersistenceToggled(Ok(())) => iced::Task::none(),

        GlobalsMsg::DeleteRequested(name) => {
            let dp = Arc::clone(&app.backend);
            iced::Task::perform(
                async move {
                    dp.delete(&name)
                        .await
                        .map(|_| ())
                        .map_err(|e| e.to_string())
                },
                |r| Message::Globals(GlobalsMsg::Deleted(r)),
            )
        }

        GlobalsMsg::Deleted(Ok(())) => {
            let dp = Arc::clone(&app.backend);
            iced::Task::perform(
                async move { load_globals_data(dp).await.map_err(|e| e.to_string()) },
                |r| Message::Globals(GlobalsMsg::EntriesLoaded(r)),
            )
        }

        GlobalsMsg::Deleted(Err(e)) => {
            tracing::warn!(error = %e, "failed to delete global");
            iced::Task::none()
        }

        GlobalsMsg::OpenCreateModal => {
            iced::Task::done(Message::VariantEditor(VariantEditorMsg::OpenCreate))
        }

        GlobalsMsg::OpenEditModal(name) => {
            if let Some(entry) = app.globals.entries.iter().find(|e| e.name == name).cloned() {
                iced::Task::done(Message::VariantEditor(VariantEditorMsg::OpenEdit(
                    name, entry,
                )))
            } else {
                iced::Task::none()
            }
        }

        GlobalsMsg::ExportRequested => {
            let dp = Arc::clone(&app.backend);
            iced::Task::perform(
                async move { export_globals_to_chosen_file(dp).await.map_err(|e| e.to_string()) },
                |r| Message::Globals(GlobalsMsg::Exported(r)),
            )
        }

        GlobalsMsg::Exported(Ok(path)) => {
            tracing::info!(path = %path.display(), "globals exported");
            iced::Task::none()
        }

        GlobalsMsg::Exported(Err(reason)) => {
            tracing::warn!(error = %reason, "globals export failed or cancelled");
            iced::Task::none()
        }
    }
}

pub fn handle_variant_editor_msg(app: &mut App, sub: VariantEditorMsg) -> iced::Task<Message> {
    match sub {
        VariantEditorMsg::OpenCreate => {
            app.globals.editor = Some(VariantEditorForm::for_create());
            iced::Task::none()
        }

        VariantEditorMsg::OpenEdit(_name, entry) => {
            app.globals.editor = Some(VariantEditorForm::for_edit(&entry));
            iced::Task::none()
        }

        VariantEditorMsg::Cancel => {
            app.globals.editor = None;
            iced::Task::none()
        }

        VariantEditorMsg::NameChanged(v) => {
            if let Some(f) = app.globals.editor.as_mut() {
                f.name = v;
            }
            iced::Task::none()
        }

        VariantEditorMsg::KindSelected(kind) => {
            if let Some(f) = app.globals.editor.as_mut() {
                f.kind = kind;
                f.error = None;
            }
            iced::Task::none()
        }

        VariantEditorMsg::PersistenceToggled(v) => {
            if let Some(f) = app.globals.editor.as_mut() {
                f.persisted = v;
            }
            iced::Task::none()
        }

        VariantEditorMsg::IntInputChanged(v) => {
            if let Some(f) = app.globals.editor.as_mut() {
                f.fields.int_input = v;
            }
            iced::Task::none()
        }

        VariantEditorMsg::FloatInputChanged(v) => {
            if let Some(f) = app.globals.editor.as_mut() {
                f.fields.float_input = v;
            }
            iced::Task::none()
        }

        VariantEditorMsg::BoolValueChanged(v) => {
            if let Some(f) = app.globals.editor.as_mut() {
                f.fields.bool_value = v;
            }
            iced::Task::none()
        }

        VariantEditorMsg::StringInputChanged(v) => {
            if let Some(f) = app.globals.editor.as_mut() {
                f.fields.string_input = v;
            }
            iced::Task::none()
        }

        VariantEditorMsg::DatetimeInputChanged(v) => {
            if let Some(f) = app.globals.editor.as_mut() {
                f.fields.datetime_input = v;
            }
            iced::Task::none()
        }

        VariantEditorMsg::ArrayJsonChanged(v) => {
            if let Some(f) = app.globals.editor.as_mut() {
                f.fields.array_json = v;
            }
            iced::Task::none()
        }

        VariantEditorMsg::ObjectJsonChanged(v) => {
            if let Some(f) = app.globals.editor.as_mut() {
                f.fields.object_json = v;
            }
            iced::Task::none()
        }

        VariantEditorMsg::Submit => {
            let Some(form) = app.globals.editor.as_ref() else {
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
            if let Some(f) = app.globals.editor.as_mut() {
                f.saving = true;
            }
            let dp = Arc::clone(&app.backend);
            iced::Task::perform(
                async move {
                    if let Some(old) = old_name {
                        dp.delete(&old)
                            .await
                            .map_err(|e| e.to_string())
                            .map(|_| ())?;
                    }
                    dp.set(&name, variant, persisted)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| Message::VariantEditor(VariantEditorMsg::Saved(r)),
            )
        }

        VariantEditorMsg::Saved(Ok(())) => {
            app.globals.editor = None;
            iced::Task::done(Message::Globals(GlobalsMsg::LoadRequested))
        }

        VariantEditorMsg::Saved(Err(e)) => {
            if let Some(f) = app.globals.editor.as_mut() {
                f.error = Some(e);
                f.saving = false;
            }
            iced::Task::none()
        }
    }
}

pub fn globals_view<'a>(app: &'a App, palette: &'a ForgePalette) -> Element<'a, Message> {
    let main = globals_main_view(app, palette);
    if let Some(form) = app.globals.editor.as_ref() {
        let modal_el = variant_editor_modal_view(form, palette);
        iced::widget::stack![main, modal_el].into()
    } else {
        main
    }
}

fn globals_main_view<'a>(app: &'a App, palette: &'a ForgePalette) -> Element<'a, Message> {
    let border_color = palette.border_regular;
    let rule_style = move |_: &iced::Theme| rule::Style {
        color: border_color,
        radius: 0.0.into(),
        fill_mode: rule::FillMode::Full,
        snap: true,
    };

    let total = app.globals.entries.len();
    let persisted_count = app.globals.entries.iter().filter(|e| e.persisted).count();
    let session_count = total - persisted_count;

    let table_content: Element<'_, Message> = if app.globals.loading {
        container(
            text("Loading...")
                .size(FONT_BODY_SM)
                .color(palette.text_muted),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
    } else {
        let visible_entries: Vec<&GlobalEntry> = app.globals.filtered_entries().collect();

        if visible_entries.is_empty() {
            container(empty_state(
                "No globals here",
                "Adjust the filter or search, or create one with + New variable.",
                None::<(&str, Message)>,
                palette,
            ))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        } else {
            let headers = vec![
                "",
                "NAME",
                "TYPE",
                "VALUE",
                "LAST MODIFIED",
                "READS · WRITES",
                "PERSIST",
            ];
            let widths = [
                Length::Fixed(24.0),
                Length::FillPortion(8),
                Length::Fixed(80.0),
                Length::FillPortion(7),
                Length::Fixed(120.0),
                Length::Fixed(100.0),
                Length::Fixed(70.0),
            ];

            let rows: Vec<Vec<Element<'_, Message>>> = visible_entries
                .iter()
                .map(|entry| build_entry_row(entry, palette))
                .collect();

            data_table(palette, headers, &widths, rows)
        }
    };

    let footer = data_screen_footer(
        palette,
        FooterProps {
            position_info: &app.globals.position_display,
            storage_info: Some(&app.globals.storage_display),
            save_info: Some(&app.globals.save_display),
            live_indicator: true,
        },
    );

    column![
        globals_stats_header(total, persisted_count, session_count, palette),
        rule::horizontal(1.0).style(rule_style),
        globals_toolbar(app, palette),
        rule::horizontal(1.0).style(rule_style),
        scrollable(table_content).height(Length::Fill),
        footer,
    ]
    .width(Length::Fill)
    .into()
}

fn globals_stats_header<'a>(
    total: usize,
    persisted: usize,
    session: usize,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let shell_bg = palette.shell;
    let muted = palette.text_muted;
    let faint = palette.text_faint;
    let primary = palette.text_primary;
    let success = palette.success;
    let warning = palette.warning;

    let label = text("Global variables").size(FONT_BODY_MD).color(primary);

    let total_part = row![
        text(total.to_string()).size(FONT_BODY_MD).color(primary),
        text(" total").size(FONT_BODY_MD).color(muted),
    ]
    .spacing(0);

    let persisted_part = row![
        text(persisted.to_string())
            .size(FONT_BODY_MD)
            .color(success),
        text(" persisted").size(FONT_BODY_MD).color(muted),
    ]
    .spacing(0);

    let session_part = row![
        text(session.to_string()).size(FONT_BODY_MD).color(warning),
        text(" in-memory").size(FONT_BODY_MD).color(muted),
    ]
    .spacing(0);

    let stats = row![
        total_part,
        text(" · ").size(FONT_BODY_MD).color(faint),
        persisted_part,
        text(" · ").size(FONT_BODY_MD).color(faint),
        session_part,
    ]
    .spacing(0)
    .align_y(Alignment::Center);

    container(row![label, Space::new().width(Length::Fill), stats].align_y(Alignment::Center))
        .padding([10.0_f32, 14.0_f32])
        .width(Length::Fill)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(shell_bg)),
            ..container::Style::default()
        })
        .into()
}

fn globals_toolbar<'a>(app: &'a App, palette: &'a ForgePalette) -> Element<'a, Message> {
    let elevated_bg = palette.elevated;

    let search = container(search_input(
        "Search variables...",
        &app.globals.search,
        |s| Message::Globals(GlobalsMsg::SearchChanged(s)),
        palette,
    ))
    .width(220.0);

    let chips = row![
        filter_chip(
            "All",
            palette.brand,
            app.globals.filter == GlobalsFilter::All,
            Message::Globals(GlobalsMsg::FilterSelected(GlobalsFilter::All)),
            palette,
        ),
        filter_chip(
            "Persisted",
            palette.success,
            app.globals.filter == GlobalsFilter::Persisted,
            Message::Globals(GlobalsMsg::FilterSelected(GlobalsFilter::Persisted)),
            palette,
        ),
        filter_chip(
            "Session",
            palette.warning,
            app.globals.filter == GlobalsFilter::Session,
            Message::Globals(GlobalsMsg::FilterSelected(GlobalsFilter::Session)),
            palette,
        ),
    ]
    .spacing(4);

    let export_btn = secondary_button(
        "Export JSON",
        Message::Globals(GlobalsMsg::ExportRequested),
        palette,
    );
    let new_btn = primary_button_small(
        "+ New variable",
        Message::Globals(GlobalsMsg::OpenCreateModal),
        palette,
    );

    let left = row![search, chips].spacing(10).align_y(Alignment::Center);
    let right = row![export_btn, new_btn]
        .spacing(6)
        .align_y(Alignment::Center);

    container(row![left, Space::new().width(Length::Fill), right].align_y(Alignment::Center))
        .padding([8.0_f32, 14.0_f32])
        .width(Length::Fill)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(elevated_bg)),
            ..container::Style::default()
        })
        .into()
}

fn filter_chip<'a>(
    label: &'a str,
    dot_color: Color,
    active: bool,
    on_press: Message,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let bg = if active {
        Some(Background::Color(palette.surface_overlay))
    } else {
        None
    };
    let text_color = if active {
        palette.text_primary
    } else {
        palette.text_secondary
    };

    let dot = container(Space::new())
        .width(5.0)
        .height(5.0)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(dot_color)),
            border: Border {
                radius: 2.5.into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            ..container::Style::default()
        });

    let inner = row![dot, text(label).size(FONT_CAPS_SM).color(text_color)]
        .spacing(5)
        .align_y(Alignment::Center);

    button(inner)
        .on_press(on_press)
        .padding([4.0_f32, 10.0_f32])
        .style(move |_: &iced::Theme, _: button::Status| button::Style {
            background: bg,
            text_color,
            border: Border {
                radius: 11.0.into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            shadow: Shadow::default(),
            snap: false,
        })
        .into()
}

fn build_entry_row<'a>(
    entry: &'a GlobalEntry,
    palette: &'a ForgePalette,
) -> Vec<Element<'a, Message>> {
    let mono = font(FontRole::Monospace);
    let muted = palette.text_muted;

    let dot_color = if entry.persisted {
        palette.brand
    } else {
        palette.warning
    };
    let status_dot = container(container(Space::new()).width(6.0).height(6.0).style(
        move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(dot_color)),
            border: Border {
                radius: 3.0.into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            ..container::Style::default()
        },
    ))
    .width(Length::Fill)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center);

    let name_cell = button(
        text(&entry.name)
            .size(FONT_BODY_SM)
            .font(mono)
            .color(palette.text_primary),
    )
    .on_press(Message::Globals(GlobalsMsg::OpenEditModal(
        entry.name.clone(),
    )))
    .padding(0)
    .style(|_: &iced::Theme, _: button::Status| button::Style {
        background: None,
        text_color: Color::TRANSPARENT,
        border: Border::default(),
        shadow: Shadow::default(),
        snap: false,
    });

    let type_cell = type_pill(palette, VariantKind::from_variant(&entry.value));

    let value_cell = value_preview(palette, &entry.value);

    let modified_cell = text(format_time_ago(entry.last_modified))
        .size(FONT_BODY_SM)
        .color(muted);

    let rw_cell = text(format!("{} · {}", entry.reads, entry.writes))
        .size(FONT_BODY_SM)
        .font(mono)
        .color(muted);

    let toggle_cell = row![
        Space::new().width(Length::Fill),
        persistence_toggle_inline(
            palette,
            entry.persisted,
            Message::Globals(GlobalsMsg::TogglePersistence(
                entry.name.clone(),
                !entry.persisted,
            )),
        ),
    ];

    vec![
        status_dot.into(),
        name_cell.into(),
        type_cell,
        value_cell,
        modified_cell.into(),
        rw_cell.into(),
        toggle_cell.into(),
    ]
}

fn format_time_ago(dt: OffsetDateTime) -> String {
    let now = OffsetDateTime::now_utc();
    let diff = now - dt;
    let secs = diff.whole_seconds().max(0);
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        let mins = secs / 60;
        format!("{mins} min ago")
    } else {
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        if mins == 0 {
            format!("{hours}h ago")
        } else {
            format!("{hours}h {mins}m ago")
        }
    }
}

fn format_save_ago(dt: Option<OffsetDateTime>) -> String {
    match dt {
        None => "Not yet saved".to_owned(),
        Some(t) => format!("Auto-saved {}", format_time_ago(t)),
    }
}

fn variant_editor_modal_view<'a>(
    form: &'a VariantEditorForm,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let title = match &form.mode {
        EditorMode::Create => "New variable",
        EditorMode::Edit(_) => "Edit variable",
    };

    let name_count = format!("{}/64", form.name.len().min(64));
    let name_counter = text(name_count)
        .size(10.0)
        .color(palette.text_faint)
        .font(font(FontRole::Monospace));
    let name_input = forge_widgets::text_input_field(
        "my_variable",
        &form.name,
        |v| Message::VariantEditor(VariantEditorMsg::NameChanged(v)),
        palette,
    );
    let name_row = row![name_input, name_counter]
        .spacing(8)
        .align_y(Alignment::Center);
    let name_block = column![section_header("NAME", None, palette), name_row].spacing(4);

    let kinds = [
        VariantKind::Int,
        VariantKind::Float,
        VariantKind::Bool,
        VariantKind::String,
        VariantKind::Datetime,
        VariantKind::Array,
        VariantKind::Object,
    ];
    let chips_row = kinds.iter().fold(row![].spacing(4), |acc, &k| {
        acc.push(category_chip(
            palette,
            k.label(),
            k.color(palette),
            form.kind == k,
            Message::VariantEditor(VariantEditorMsg::KindSelected(k)),
        ))
    });
    let type_block = column![section_header("TYPE", None, palette), chips_row].spacing(4);

    let persist_toggle = toggle(
        palette,
        ToggleProps {
            label: "Save across restarts",
            description: "Persisted globals survive app close; session-only reset on launch",
            value: form.persisted,
            on_toggle: Message::VariantEditor(VariantEditorMsg::PersistenceToggled(
                !form.persisted,
            )),
        },
    );
    let persist_block =
        column![section_header("PERSISTENCE", None, palette), persist_toggle].spacing(4);

    let value_editor: Element<'_, Message> = match form.kind {
        VariantKind::Int => forge_widgets::text_input_field(
            "0",
            &form.fields.int_input,
            |v| Message::VariantEditor(VariantEditorMsg::IntInputChanged(v)),
            palette,
        ),
        VariantKind::Float => forge_widgets::text_input_field(
            "0.0",
            &form.fields.float_input,
            |v| Message::VariantEditor(VariantEditorMsg::FloatInputChanged(v)),
            palette,
        ),
        VariantKind::Bool => toggle(
            palette,
            ToggleProps {
                label: "Value",
                description: "",
                value: form.fields.bool_value,
                on_toggle: Message::VariantEditor(VariantEditorMsg::BoolValueChanged(
                    !form.fields.bool_value,
                )),
            },
        ),
        VariantKind::String => forge_widgets::text_input_field(
            "",
            &form.fields.string_input,
            |v| Message::VariantEditor(VariantEditorMsg::StringInputChanged(v)),
            palette,
        ),
        VariantKind::Datetime => forge_widgets::text_input_field(
            "2026-05-18T14:23:00Z",
            &form.fields.datetime_input,
            |v| Message::VariantEditor(VariantEditorMsg::DatetimeInputChanged(v)),
            palette,
        ),
        VariantKind::Array => forge_widgets::text_input_field(
            "[1, 2, 3]",
            &form.fields.array_json,
            |v| Message::VariantEditor(VariantEditorMsg::ArrayJsonChanged(v)),
            palette,
        ),
        VariantKind::Object => forge_widgets::text_input_field(
            r#"{"key": "value"}"#,
            &form.fields.object_json,
            |v| Message::VariantEditor(VariantEditorMsg::ObjectJsonChanged(v)),
            palette,
        ),
    };
    let value_block = column![section_header("VALUE", None, palette), value_editor].spacing(4);

    let mut body_col = column![name_block, type_block, persist_block, value_block].spacing(12);
    if let Some(err) = form.error.as_deref() {
        body_col = body_col.push(live_status_banner(BannerKind::Error, err, None, palette));
    }
    let body: Element<'_, Message> = body_col.into();

    let cancel_btn = secondary_button(
        "Cancel",
        Message::VariantEditor(VariantEditorMsg::Cancel),
        palette,
    );
    let is_saveable = form.is_valid().is_none() && !form.saving;
    let save_label = if form.saving { "Saving..." } else { "Save" };
    let save_btn: Element<'_, Message> = if is_saveable {
        primary_button_small(
            save_label,
            Message::VariantEditor(VariantEditorMsg::Submit),
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
            title,
            on_close: Message::VariantEditor(VariantEditorMsg::Cancel),
            kbd_hint: Some("ESC to cancel"),
        },
        body,
        footer,
    )
}

fn format_storage_bytes(bytes: u64) -> String {
    if bytes == 0 {
        "Storage: 0 B".to_owned()
    } else {
        let kb = bytes as f64 / 1024.0;
        format!("Storage: {kb:.1} KB")
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::Message;
    use crate::app::{App, update};
    use crate::screen::Screen;
    use forge_storage::GlobalEntry;
    use forge_types::Variant;
    use time::OffsetDateTime;

    const TEST_KEY: [u8; 32] = [0xab; 32];

    fn make_entry(name: &str, persisted: bool) -> GlobalEntry {
        GlobalEntry {
            name: name.to_owned(),
            value: Variant::Int(0),
            persisted,
            reads: 0,
            writes: 0,
            created_at: OffsetDateTime::UNIX_EPOCH,
            last_modified: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn state_with(entries: Vec<GlobalEntry>, filter: GlobalsFilter, search: &str) -> GlobalsState {
        let mut s = GlobalsState::new();
        s.entries = entries;
        s.filter = filter;
        s.search = search.to_owned();
        s.refresh_displays();
        s
    }

    #[test]
    fn filter_all_returns_all_entries() {
        let s = state_with(
            vec![
                make_entry("counter", true),
                make_entry("name", true),
                make_entry("session_var", false),
            ],
            GlobalsFilter::All,
            "",
        );
        assert_eq!(s.filtered_entries().count(), 3);
    }

    #[test]
    fn filter_persisted_returns_only_persisted() {
        let s = state_with(
            vec![
                make_entry("counter", true),
                make_entry("name", true),
                make_entry("session_var", false),
            ],
            GlobalsFilter::Persisted,
            "",
        );
        assert_eq!(s.filtered_entries().count(), 2);
        assert!(s.filtered_entries().all(|e| e.persisted));
    }

    #[test]
    fn filter_session_returns_only_session() {
        let s = state_with(
            vec![
                make_entry("counter", true),
                make_entry("session_var", false),
            ],
            GlobalsFilter::Session,
            "",
        );
        assert_eq!(s.filtered_entries().count(), 1);
        assert!(!s.filtered_entries().next().unwrap().persisted);
    }

    #[test]
    fn search_filters_entries_by_name() {
        let s = state_with(
            vec![
                make_entry("quoteCounter", true),
                make_entry("streamLive", true),
                make_entry("counterReset", false),
            ],
            GlobalsFilter::All,
            "counter",
        );
        let results: Vec<_> = s.filtered_entries().collect();
        assert_eq!(results.len(), 2);
        assert!(
            results
                .iter()
                .all(|e| e.name.to_lowercase().contains("counter"))
        );
    }

    #[test]
    fn load_requested_sets_loading_flag() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Globals(GlobalsMsg::LoadRequested));
        assert!(app.globals.loading);
    }

    #[test]
    fn entries_loaded_ok_populates_state() {
        let mut app = App::default();
        let data = GlobalsLoadData {
            entries: vec![make_entry("test", true), make_entry("other", false)],
            storage_bytes: 2048,
            last_save: None,
        };
        let _ = update(
            &mut app,
            Message::Globals(GlobalsMsg::EntriesLoaded(Ok(data))),
        );
        assert_eq!(app.globals.entries.len(), 2);
        assert!(!app.globals.loading);
        assert_eq!(app.globals.storage_bytes, 2048);
    }

    #[test]
    fn entries_loaded_err_clears_loading() {
        let mut app = App::default();
        app.globals.loading = true;
        let _ = update(
            &mut app,
            Message::Globals(GlobalsMsg::EntriesLoaded(Err("db error".to_owned()))),
        );
        assert!(!app.globals.loading);
        assert!(app.globals.entries.is_empty());
    }

    #[test]
    fn globals_view_smoke_empty_state() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Globals));
        let palette = app.palette;
        let _ = globals_view(&app, &palette);
    }

    #[test]
    fn globals_view_smoke_with_entries() {
        let mut app = App::default();
        app.globals.entries = vec![
            make_entry("quoteCounter", true),
            make_entry("streamLive", false),
        ];
        app.globals.loading = false;
        app.globals.refresh_displays();
        let palette = app.palette;
        let _ = globals_view(&app, &palette);
    }

    #[test]
    fn position_display_updates_on_filter_change() {
        let mut app = App::default();
        app.globals.entries = vec![make_entry("alpha", true), make_entry("beta", false)];
        app.globals.refresh_displays();
        assert!(app.globals.position_display.contains("2 of 2"));

        let _ = update(
            &mut app,
            Message::Globals(GlobalsMsg::FilterSelected(GlobalsFilter::Persisted)),
        );
        assert!(app.globals.position_display.contains("1 of 2"));
    }

    fn int_form(name: &str, value: &str) -> VariantEditorForm {
        VariantEditorForm {
            mode: EditorMode::Create,
            name: name.to_owned(),
            kind: VariantKind::Int,
            persisted: false,
            fields: VariantEditorFields {
                int_input: value.to_owned(),
                ..VariantEditorFields::default()
            },
            error: None,
            saving: false,
        }
    }

    #[test]
    fn is_valid_empty_name_is_required() {
        let form = int_form("", "42");
        assert_eq!(form.is_valid(), Some("Name is required"));
    }

    #[test]
    fn is_valid_whitespace_name_is_required() {
        let form = int_form("   ", "42");
        assert_eq!(form.is_valid(), Some("Name is required"));
    }

    #[test]
    fn is_valid_int_with_valid_number_is_none() {
        let form = int_form("counter", "42");
        assert_eq!(form.is_valid(), None);
    }

    #[test]
    fn is_valid_int_with_non_number_is_invalid() {
        let form = int_form("counter", "not a number");
        assert_eq!(form.is_valid(), Some("Invalid integer"));
    }

    #[test]
    fn is_valid_float_with_valid_value_is_none() {
        let form = VariantEditorForm {
            mode: EditorMode::Create,
            name: "ratio".to_owned(),
            kind: VariantKind::Float,
            persisted: false,
            fields: VariantEditorFields {
                float_input: "3.14".to_owned(),
                ..VariantEditorFields::default()
            },
            error: None,
            saving: false,
        };
        assert_eq!(form.is_valid(), None);
    }

    #[test]
    fn is_valid_datetime_with_rfc3339_is_none() {
        let form = VariantEditorForm {
            mode: EditorMode::Create,
            name: "ts".to_owned(),
            kind: VariantKind::Datetime,
            persisted: false,
            fields: VariantEditorFields {
                datetime_input: "2026-05-18T14:23:00Z".to_owned(),
                ..VariantEditorFields::default()
            },
            error: None,
            saving: false,
        };
        assert_eq!(form.is_valid(), None);
    }

    #[test]
    fn is_valid_array_with_plain_json_is_none() {
        let form = VariantEditorForm {
            mode: EditorMode::Create,
            name: "nums".to_owned(),
            kind: VariantKind::Array,
            persisted: false,
            fields: VariantEditorFields {
                array_json: "[1, 2, 3]".to_owned(),
                ..VariantEditorFields::default()
            },
            error: None,
            saving: false,
        };
        assert_eq!(form.is_valid(), None);
    }

    #[test]
    fn is_valid_array_with_invalid_json_is_error() {
        let form = VariantEditorForm {
            mode: EditorMode::Create,
            name: "nums".to_owned(),
            kind: VariantKind::Array,
            persisted: false,
            fields: VariantEditorFields {
                array_json: "not json".to_owned(),
                ..VariantEditorFields::default()
            },
            error: None,
            saving: false,
        };
        assert_eq!(form.is_valid(), Some("Invalid JSON array"));
    }

    #[test]
    fn build_variant_int_produces_int() {
        let form = int_form("counter", "42");
        assert!(matches!(form.build_variant(), Some(Variant::Int(42))));
    }

    #[test]
    fn build_variant_bool_true_produces_bool() {
        let form = VariantEditorForm {
            mode: EditorMode::Create,
            name: "flag".to_owned(),
            kind: VariantKind::Bool,
            persisted: false,
            fields: VariantEditorFields {
                bool_value: true,
                ..VariantEditorFields::default()
            },
            error: None,
            saving: false,
        };
        assert!(matches!(form.build_variant(), Some(Variant::Bool(true))));
    }

    #[test]
    fn open_create_modal_sets_editor() {
        let mut app = App::default();
        assert!(app.globals.editor.is_none());
        let _ = update(&mut app, Message::Globals(GlobalsMsg::OpenCreateModal));
        let _ = update(
            &mut app,
            Message::VariantEditor(VariantEditorMsg::OpenCreate),
        );
        assert!(app.globals.editor.is_some());
        assert!(matches!(
            app.globals.editor.as_ref().unwrap().mode,
            EditorMode::Create
        ));
    }

    #[test]
    fn cancel_clears_editor() {
        let mut app = App::default();
        app.globals.editor = Some(VariantEditorForm::for_create());
        let _ = update(&mut app, Message::VariantEditor(VariantEditorMsg::Cancel));
        assert!(app.globals.editor.is_none());
    }

    #[test]
    fn open_edit_modal_prefills_entry() {
        let mut app = App::default();
        let entry = GlobalEntry {
            name: "myvar".to_owned(),
            value: Variant::Int(99),
            persisted: true,
            reads: 0,
            writes: 0,
            created_at: OffsetDateTime::UNIX_EPOCH,
            last_modified: OffsetDateTime::UNIX_EPOCH,
        };
        let _ = update(
            &mut app,
            Message::VariantEditor(VariantEditorMsg::OpenEdit("myvar".to_owned(), entry)),
        );
        let form = app.globals.editor.as_ref().unwrap();
        assert_eq!(form.name, "myvar");
        assert_eq!(form.kind, VariantKind::Int);
        assert!(form.persisted);
        assert_eq!(form.fields.int_input, "99");
    }

    #[test]
    fn submit_with_invalid_form_is_noop() {
        let mut app = App::default();
        app.globals.editor = Some(int_form("", "not_a_number"));
        let _ = update(&mut app, Message::VariantEditor(VariantEditorMsg::Submit));
        assert!(app.globals.editor.is_some());
        assert!(!app.globals.editor.as_ref().unwrap().saving);
    }

    #[test]
    fn submit_with_valid_form_sets_saving() {
        let mut app = App::default();
        app.globals.editor = Some(int_form("counter", "42"));
        let _ = update(&mut app, Message::VariantEditor(VariantEditorMsg::Submit));
        assert!(app.globals.editor.as_ref().is_some_and(|f| f.saving));
    }

    #[test]
    fn saved_ok_closes_modal() {
        let mut app = App::default();
        app.globals.editor = Some(int_form("counter", "42"));
        let _ = update(
            &mut app,
            Message::VariantEditor(VariantEditorMsg::Saved(Ok(()))),
        );
        assert!(app.globals.editor.is_none());
    }

    #[test]
    fn saved_err_keeps_modal_with_error() {
        let mut app = App::default();
        app.globals.editor = Some(VariantEditorForm {
            saving: true,
            ..int_form("counter", "42")
        });
        let _ = update(
            &mut app,
            Message::VariantEditor(VariantEditorMsg::Saved(Err("db write failed".to_owned()))),
        );
        let form = app.globals.editor.as_ref().unwrap();
        assert!(!form.saving);
        assert_eq!(form.error.as_deref(), Some("db write failed"));
    }

    #[test]
    fn globals_view_smoke_with_modal_open() {
        let mut app = App::default();
        app.globals.editor = Some(int_form("x", "1"));
        let palette = app.palette;
        let _ = globals_view(&app, &palette);
    }

    #[test]
    fn submit_create_saves_to_backend() {
        let rt = tokio::runtime::Runtime::new().expect("tokio");
        let backend = Arc::new(
            rt.block_on(SqliteBackend::open_with_key("sqlite::memory:", TEST_KEY))
                .expect("in-memory db"),
        );
        rt.block_on(async {
            backend
                .set("counter", Variant::Int(42), false)
                .await
                .expect("set ok");
            let entries = backend.list().await.expect("list ok");
            assert!(
                entries
                    .iter()
                    .any(|e| e.name == "counter" && matches!(e.value, Variant::Int(42)))
            );
        });
    }

    #[test]
    fn submit_edit_updates_backend_entry() {
        let rt = tokio::runtime::Runtime::new().expect("tokio");
        let backend = Arc::new(
            rt.block_on(SqliteBackend::open_with_key("sqlite::memory:", TEST_KEY))
                .expect("in-memory db"),
        );
        rt.block_on(async {
            backend
                .set("counter", Variant::Int(1), false)
                .await
                .expect("initial set");
            backend
                .set("counter", Variant::Int(99), false)
                .await
                .expect("edit set");
            let entries = backend.list().await.expect("list ok");
            let entry = entries.iter().find(|e| e.name == "counter").unwrap();
            assert!(matches!(entry.value, Variant::Int(99)));
        });
    }
}
