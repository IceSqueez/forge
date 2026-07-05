use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;

use forge_storage::{GlobalEntry, GlobalsExport, GlobalsRepo, StorageError};
use forge_widgets::tokens::{FONT_SM, FONT_XS, Spacing, sp, spf};
use forge_widgets::{
    ConfirmKind, ConfirmModalParams, ConfirmTone, FontRole, FooterProps, ForgePalette, Icon,
    RowAction, ToastAction, ToastKind, VariantKind, confirm_modal, data_screen_footer, data_table,
    empty_state, font, persistence_toggle_inline, primary_button_small, row_actions, search_input,
    secondary_button, type_pill, value_preview,
};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Shadow,
    widget::{Space, button, column, container, row, rule, scrollable, stack, text},
};
use time::OffsetDateTime;

use crate::Message;
use crate::app::App;
use crate::message::{GlobalsFilter, GlobalsLoadData, GlobalsMsg, ToastMsg, VariantEditorMsg};
use crate::runtime_view::RuntimeView;

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

async fn export_globals_to_chosen_file(repo: Arc<dyn GlobalsRepo>) -> Result<PathBuf, ExportError> {
    let entries = repo.export_all().await?;
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

pub use crate::globals_variant_editor::{
    VariantEditorFields, VariantEditorForm, update_variant_editor, variant_editor_modal_view,
};

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
    /// Two-phase delete gate — armed by the row delete control, rendered by
    /// the shared `confirm_modal`. `None` = no confirm dialog showing.
    pub pending_delete: Option<String>,
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
            pending_delete: None,
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

pub async fn load_globals_data(repo: Arc<dyn GlobalsRepo>) -> Result<GlobalsLoadData, String> {
    let mut entries = repo.list().await.map_err(|e| e.to_string())?;
    entries.sort_unstable_by(|a, b| a.name.cmp(&b.name));
    let storage_bytes = repo.storage_bytes().await.map_err(|e| e.to_string())?;
    let last_save = repo.last_save_at().await.map_err(|e| e.to_string())?;
    Ok(GlobalsLoadData {
        entries,
        storage_bytes,
        last_save,
    })
}

pub fn update(state: &mut GlobalsState, rt: &RuntimeView, msg: GlobalsMsg) -> iced::Task<Message> {
    match msg {
        GlobalsMsg::LoadRequested => {
            state.loading = true;
            let repo: Arc<dyn GlobalsRepo> = Arc::clone(&rt.backend) as Arc<dyn GlobalsRepo>;
            iced::Task::perform(
                async move { load_globals_data(repo).await.map_err(|e| e.to_string()) },
                |r| Message::Globals(GlobalsMsg::EntriesLoaded(r)),
            )
        }

        GlobalsMsg::EntriesLoaded(Ok(data)) => {
            state.entries = data.entries;
            state.storage_bytes = data.storage_bytes;
            state.last_save = data.last_save;
            state.loading = false;
            state.refresh_displays();
            iced::Task::none()
        }

        GlobalsMsg::EntriesLoaded(Err(e)) => {
            tracing::warn!(error = %e, "globals load failed");
            state.loading = false;
            iced::Task::none()
        }

        GlobalsMsg::FilterSelected(f) => {
            state.filter = f;
            state.refresh_displays();
            iced::Task::none()
        }

        GlobalsMsg::SearchChanged(s) => {
            state.search = s;
            state.refresh_displays();
            iced::Task::none()
        }

        GlobalsMsg::TogglePersistence(name, new_persisted) => {
            if !state.entries.iter().any(|e| e.name == name) {
                return iced::Task::none();
            }
            if let Some(e) = state.entries.iter_mut().find(|e| e.name == name) {
                e.persisted = new_persisted;
            }
            state.refresh_displays();
            let repo: Arc<dyn GlobalsRepo> = Arc::clone(&rt.backend) as Arc<dyn GlobalsRepo>;
            let name_for_msg = name.clone();
            iced::Task::perform(
                async move {
                    repo.set_persisted(&name, new_persisted)
                        .await
                        .map(|_| ())
                        .map_err(|e| e.to_string())
                },
                move |result| {
                    Message::Globals(GlobalsMsg::PersistenceToggled {
                        name: name_for_msg.clone(),
                        attempted: new_persisted,
                        result,
                    })
                },
            )
        }

        GlobalsMsg::PersistenceToggled { result: Ok(()), .. } => iced::Task::none(),

        GlobalsMsg::PersistenceToggled {
            name,
            attempted,
            result: Err(e),
        } => {
            if let Some(entry) = state.entries.iter_mut().find(|entry| entry.name == name) {
                entry.persisted = !attempted;
            }
            state.refresh_displays();
            iced::Task::done(Message::Toast(ToastMsg::Fired {
                kind: ToastKind::Error,
                message: format!("Could not change persistence for '{name}': {e}"),
                duration_ms: 5000,
                action: None,
            }))
        }

        GlobalsMsg::DeleteRequested(name) => {
            // Arms the confirm gate only — the row delete control no longer
            // deletes directly (DT-06-F13/OV-04-F15: was fully wired but
            // unreachable AND unconfirmed).
            state.pending_delete = Some(name);
            iced::Task::none()
        }

        GlobalsMsg::DeleteConfirmDismissed => {
            state.pending_delete = None;
            iced::Task::none()
        }

        GlobalsMsg::DeleteConfirmAccepted(name) => {
            state.pending_delete = None;
            // Capture the full entry BEFORE deleting — this is the undo
            // payload (name/value/persisted), not just a reload trigger.
            let Some(entry) = state.entries.iter().find(|e| e.name == name).cloned() else {
                return iced::Task::none();
            };
            let repo: Arc<dyn GlobalsRepo> = Arc::clone(&rt.backend) as Arc<dyn GlobalsRepo>;
            iced::Task::perform(
                async move {
                    let result = repo
                        .delete(&entry.name)
                        .await
                        .map(|_| ())
                        .map_err(|e| e.to_string());
                    (entry, result)
                },
                |(entry, result)| Message::Globals(GlobalsMsg::Deleted(entry, result)),
            )
        }

        GlobalsMsg::Deleted(entry, Ok(())) => {
            let repo: Arc<dyn GlobalsRepo> = Arc::clone(&rt.backend) as Arc<dyn GlobalsRepo>;
            let reload = iced::Task::perform(
                async move { load_globals_data(repo).await.map_err(|e| e.to_string()) },
                |r| Message::Globals(GlobalsMsg::EntriesLoaded(r)),
            );
            let undo_toast = iced::Task::done(Message::Toast(ToastMsg::Fired {
                kind: ToastKind::Undo,
                message: forge_widgets::tr!("globals_deleted_toast", name = entry.name.as_str()),
                duration_ms: 6000,
                action: Some(Box::new(ToastAction {
                    label: forge_widgets::tr!("common_undo"),
                    on_press: Message::Globals(GlobalsMsg::UndoDelete(entry)),
                })),
            }));
            iced::Task::batch([reload, undo_toast])
        }

        GlobalsMsg::Deleted(entry, Err(e)) => {
            tracing::warn!(error = %e, name = %entry.name, "failed to delete global");
            iced::Task::done(Message::Toast(ToastMsg::Fired {
                kind: ToastKind::Error,
                message: format!("Could not delete '{}': {e}", entry.name),
                duration_ms: 5000,
                action: None,
            }))
        }

        GlobalsMsg::UndoDelete(entry) => {
            let repo: Arc<dyn GlobalsRepo> = Arc::clone(&rt.backend) as Arc<dyn GlobalsRepo>;
            iced::Task::perform(
                async move {
                    repo.set(&entry.name, entry.value, entry.persisted)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| Message::Globals(GlobalsMsg::UndoDeleteResult(r)),
            )
        }

        GlobalsMsg::UndoDeleteResult(Ok(())) => {
            let repo: Arc<dyn GlobalsRepo> = Arc::clone(&rt.backend) as Arc<dyn GlobalsRepo>;
            iced::Task::perform(
                async move { load_globals_data(repo).await.map_err(|e| e.to_string()) },
                |r| Message::Globals(GlobalsMsg::EntriesLoaded(r)),
            )
        }

        GlobalsMsg::UndoDeleteResult(Err(e)) => {
            tracing::warn!(error = %e, "undo delete failed");
            iced::Task::done(Message::Toast(ToastMsg::Fired {
                kind: ToastKind::Error,
                message: format!("Could not restore global: {e}"),
                duration_ms: 5000,
                action: None,
            }))
        }

        GlobalsMsg::OpenCreateModal => iced::Task::done(Message::Globals(
            GlobalsMsg::VariantEditor(VariantEditorMsg::OpenCreate),
        )),

        GlobalsMsg::OpenEditModal(name) => {
            if let Some(entry) = state.entries.iter().find(|e| e.name == name).cloned() {
                iced::Task::done(Message::Globals(GlobalsMsg::VariantEditor(
                    VariantEditorMsg::OpenEdit(name, entry),
                )))
            } else {
                iced::Task::none()
            }
        }

        GlobalsMsg::ExportRequested => {
            let repo: Arc<dyn GlobalsRepo> = Arc::clone(&rt.backend) as Arc<dyn GlobalsRepo>;
            iced::Task::perform(
                async move {
                    export_globals_to_chosen_file(repo)
                        .await
                        .map_err(|e| e.to_string())
                },
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

        GlobalsMsg::VariantEditor(sub) => {
            update_variant_editor(&mut state.editor, rt, sub, &state.entries)
        }
    }
}

pub fn globals_view<'a>(app: &'a App, palette: &'a ForgePalette) -> Element<'a, Message> {
    let main = globals_main_view(app, palette);

    let with_pending_delete: Element<'_, Message> = match app.ui.globals.pending_delete.as_ref() {
        Some(name) => {
            let modal = confirm_modal(
                ConfirmModalParams {
                    kind: ConfirmKind::Global,
                    item_name: Cow::Borrowed(name.as_str()),
                    cascade_hint: None,
                    tone: ConfirmTone::Destructive,
                },
                Message::Globals(GlobalsMsg::DeleteConfirmAccepted(name.clone())),
                Message::Globals(GlobalsMsg::DeleteConfirmDismissed),
                palette,
            );
            stack![main, modal].into()
        }
        None => main,
    };

    if let Some(form) = app.ui.globals.editor.as_ref() {
        let modal_el = variant_editor_modal_view(form, &app.ui.globals.entries, palette);
        stack![with_pending_delete, modal_el].into()
    } else {
        with_pending_delete
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

    let total = app.ui.globals.entries.len();
    let persisted_count = app
        .ui
        .globals
        .entries
        .iter()
        .filter(|e| e.persisted)
        .count();
    let session_count = total - persisted_count;

    let table_content: Element<'_, Message> = if app.ui.globals.loading {
        container(
            text(forge_widgets::tr!("globals_loading"))
                .size(FONT_SM)
                .color(palette.text_muted),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
    } else {
        let visible_entries: Vec<&GlobalEntry> = app.ui.globals.filtered_entries().collect();

        if visible_entries.is_empty() {
            container(empty_state(
                forge_widgets::tr!("globals_empty_title"),
                forge_widgets::tr!("globals_empty_desc"),
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
                "",
            ];
            let widths = [
                Length::Fixed(24.0),
                Length::FillPortion(8),
                Length::Fixed(80.0),
                Length::FillPortion(7),
                Length::Fixed(120.0),
                Length::Fixed(100.0),
                Length::Fixed(70.0),
                Length::Fixed(36.0),
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
            position_info: &app.ui.globals.position_display,
            storage_info: Some(&app.ui.globals.storage_display),
            save_info: Some(&app.ui.globals.save_display),
            live_indicator: true,
        },
    );

    let _ = (persisted_count, session_count, total);

    column![
        globals_page_header(app, palette),
        rule::horizontal(1.0).style(rule_style),
        scrollable(table_content).height(Length::Fill),
        footer,
    ]
    .width(Length::Fill)
    .into()
}

fn globals_page_header<'a>(app: &'a App, palette: &'a ForgePalette) -> Element<'a, Message> {
    let p = *palette;

    let crumb_bar = forge_widgets::breadcrumb(
        vec![
            forge_widgets::BreadcrumbCrumb::leaf(forge_widgets::tr!(
                "globals_breadcrumb_automation"
            )),
            forge_widgets::BreadcrumbCrumb::leaf(forge_widgets::tr!("globals_breadcrumb_globals")),
        ],
        None,
        palette,
    );

    let chip_all = filter_chip(
        forge_widgets::tr!("globals_filter_all"),
        p.brand,
        app.ui.globals.filter == GlobalsFilter::All,
        Message::Globals(GlobalsMsg::FilterSelected(GlobalsFilter::All)),
        palette,
    );
    let chip_persisted = filter_chip(
        forge_widgets::tr!("globals_filter_persisted"),
        p.success,
        app.ui.globals.filter == GlobalsFilter::Persisted,
        Message::Globals(GlobalsMsg::FilterSelected(GlobalsFilter::Persisted)),
        palette,
    );
    let chip_session = filter_chip(
        forge_widgets::tr!("globals_filter_session"),
        p.warning,
        app.ui.globals.filter == GlobalsFilter::Session,
        Message::Globals(GlobalsMsg::FilterSelected(GlobalsFilter::Session)),
        palette,
    );
    let chips = row![chip_all, chip_persisted, chip_session].spacing(spf(Spacing::Xxs));

    let divider = container(Space::new().width(0.5).height(16.0))
        .width(0.5)
        .height(16.0)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(p.border_regular)),
            ..container::Style::default()
        });

    let search = container(search_input(
        forge_widgets::tr!("globals_search_placeholder"),
        &app.ui.globals.search,
        |s| Message::Globals(GlobalsMsg::SearchChanged(s)),
        palette,
    ))
    .width(Length::Fixed(180.0));

    let export_btn = secondary_button(
        forge_widgets::tr!("globals_export_btn"),
        Message::Globals(GlobalsMsg::ExportRequested),
        palette,
    );
    let new_btn = primary_button_small(
        forge_widgets::tr!("globals_new_btn"),
        Message::Globals(GlobalsMsg::OpenCreateModal),
        palette,
    );

    let filter_row = row![
        chips,
        divider,
        search,
        Space::new().width(Length::Fill),
        export_btn,
        new_btn
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(Alignment::Center);

    let filter_bar = container(filter_row)
        .width(Length::Fill)
        .padding([sp(Spacing::Xs), sp(Spacing::Md)])
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(p.shell)),
            border: Border {
                color: p.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        });

    column![crumb_bar, filter_bar].into()
}

fn filter_chip<'a>(
    label: impl Into<String>,
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

    let inner = row![dot, text(label.into()).size(FONT_XS).color(text_color)]
        .spacing(spf(Spacing::Xxs))
        .align_y(Alignment::Center);

    button(inner)
        .on_press(on_press)
        .padding([spf(Spacing::Xxs), spf(Spacing::Xs)])
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
            .size(FONT_SM)
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
        .size(FONT_SM)
        .color(muted);

    let rw_cell = text(format!("{} · {}", entry.reads, entry.writes))
        .size(FONT_SM)
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

    // `row_actions` is a hover-reveal primitive (dims to `text_faint` when
    // `hovered` is false); Globals' `data_table` has no per-row hover-state
    // tracking today, so this always renders in its "hovered" (full-opacity)
    // style — the delete control being reachable matters more here than the
    // dim-until-hover polish. Revisit once a row-hover primitive exists.
    let delete_cell = row_actions(
        vec![RowAction {
            icon: Icon::X,
            label: forge_widgets::tr!("globals_delete_action"),
            on_press: Message::Globals(GlobalsMsg::DeleteRequested(entry.name.clone())),
            color: Some(palette.random),
        }],
        true,
        palette,
    );

    vec![
        status_dot.into(),
        name_cell.into(),
        type_cell,
        value_cell,
        modified_cell.into(),
        rw_cell.into(),
        toggle_cell.into(),
        delete_cell,
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
    use crate::message::EditorMode;

    use forge_storage::GlobalEntry;
    use forge_storage_sqlite::SqliteBackend;
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
        assert!(app.ui.globals.loading);
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
        assert_eq!(app.ui.globals.entries.len(), 2);
        assert!(!app.ui.globals.loading);
        assert_eq!(app.ui.globals.storage_bytes, 2048);
    }

    #[test]
    fn entries_loaded_err_clears_loading() {
        let mut app = App::default();
        app.ui.globals.loading = true;
        let _ = update(
            &mut app,
            Message::Globals(GlobalsMsg::EntriesLoaded(Err("db error".to_owned()))),
        );
        assert!(!app.ui.globals.loading);
        assert!(app.ui.globals.entries.is_empty());
    }

    #[test]
    fn position_display_updates_on_filter_change() {
        let mut app = App::default();
        app.ui.globals.entries = vec![make_entry("alpha", true), make_entry("beta", false)];
        app.ui.globals.refresh_displays();
        assert!(app.ui.globals.position_display.contains("2 of 2"));

        let _ = update(
            &mut app,
            Message::Globals(GlobalsMsg::FilterSelected(GlobalsFilter::Persisted)),
        );
        assert!(app.ui.globals.position_display.contains("1 of 2"));
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
        assert!(app.ui.globals.editor.is_none());
        let _ = update(&mut app, Message::Globals(GlobalsMsg::OpenCreateModal));
        let _ = update(
            &mut app,
            Message::Globals(GlobalsMsg::VariantEditor(VariantEditorMsg::OpenCreate)),
        );
        assert!(app.ui.globals.editor.is_some());
        assert!(matches!(
            app.ui.globals.editor.as_ref().unwrap().mode,
            EditorMode::Create
        ));
    }

    #[test]
    fn cancel_clears_editor() {
        let mut app = App::default();
        app.ui.globals.editor = Some(VariantEditorForm::for_create());
        let _ = update(
            &mut app,
            Message::Globals(GlobalsMsg::VariantEditor(VariantEditorMsg::Cancel)),
        );
        assert!(app.ui.globals.editor.is_none());
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
            Message::Globals(GlobalsMsg::VariantEditor(VariantEditorMsg::OpenEdit(
                "myvar".to_owned(),
                entry,
            ))),
        );
        let form = app.ui.globals.editor.as_ref().unwrap();
        assert_eq!(form.name, "myvar");
        assert_eq!(form.kind, VariantKind::Int);
        assert!(form.persisted);
        assert_eq!(form.fields.int_input, "99");
    }

    #[test]
    fn submit_with_invalid_form_is_noop() {
        let mut app = App::default();
        app.ui.globals.editor = Some(int_form("", "not_a_number"));
        let _ = update(
            &mut app,
            Message::Globals(GlobalsMsg::VariantEditor(VariantEditorMsg::Submit)),
        );
        assert!(app.ui.globals.editor.is_some());
        assert!(!app.ui.globals.editor.as_ref().unwrap().saving);
    }

    #[test]
    fn submit_with_valid_form_sets_saving() {
        let mut app = App::default();
        app.ui.globals.editor = Some(int_form("counter", "42"));
        let _ = update(
            &mut app,
            Message::Globals(GlobalsMsg::VariantEditor(VariantEditorMsg::Submit)),
        );
        assert!(app.ui.globals.editor.as_ref().is_some_and(|f| f.saving));
    }

    #[test]
    fn saved_ok_closes_modal() {
        let mut app = App::default();
        app.ui.globals.editor = Some(int_form("counter", "42"));
        let _ = update(
            &mut app,
            Message::Globals(GlobalsMsg::VariantEditor(VariantEditorMsg::Saved(Ok(())))),
        );
        assert!(app.ui.globals.editor.is_none());
    }

    #[test]
    fn saved_err_keeps_modal_with_error() {
        let mut app = App::default();
        app.ui.globals.editor = Some(VariantEditorForm {
            saving: true,
            ..int_form("counter", "42")
        });
        let _ = update(
            &mut app,
            Message::Globals(GlobalsMsg::VariantEditor(VariantEditorMsg::Saved(Err(
                "db write failed".to_owned(),
            )))),
        );
        let form = app.ui.globals.editor.as_ref().unwrap();
        assert!(!form.saving);
        assert_eq!(form.error.as_deref(), Some("db write failed"));
    }

    #[test]
    fn persistence_toggled_err_reverts_optimistic_flip() {
        // STORAGE-4 regression: the optimistic flip already set entry.persisted
        // to `attempted`; a failed write must roll it back to `!attempted`.
        for attempted in [true, false] {
            let mut app = App::default();
            // Simulate the state right after the optimistic flip.
            app.ui.globals.entries = vec![make_entry("flag", attempted)];
            let _ = update(
                &mut app,
                Message::Globals(GlobalsMsg::PersistenceToggled {
                    name: "flag".to_owned(),
                    attempted,
                    result: Err("db down".to_owned()),
                }),
            );
            let entry = app
                .ui
                .globals
                .entries
                .iter()
                .find(|e| e.name == "flag")
                .expect("entry");
            assert_eq!(
                entry.persisted, !attempted,
                "failed write must revert persisted to !attempted ({attempted})"
            );
        }
    }

    #[test]
    fn persistence_toggled_ok_leaves_optimistic_flip_intact() {
        let mut app = App::default();
        app.ui.globals.entries = vec![make_entry("flag", true)];
        let _ = update(
            &mut app,
            Message::Globals(GlobalsMsg::PersistenceToggled {
                name: "flag".to_owned(),
                attempted: true,
                result: Ok(()),
            }),
        );
        let entry = app.ui.globals.entries.iter().find(|e| e.name == "flag");
        assert!(entry.is_some_and(|e| e.persisted), "success keeps the flip");
    }

    #[test]
    fn persistence_toggled_err_for_unknown_name_does_not_panic() {
        let mut app = App::default();
        app.ui.globals.entries = vec![make_entry("present", true)];
        let _ = update(
            &mut app,
            Message::Globals(GlobalsMsg::PersistenceToggled {
                name: "absent".to_owned(),
                attempted: true,
                result: Err("db down".to_owned()),
            }),
        );
        // Untouched entry stays as-is; the missing target is a no-op on state.
        let entry = app.ui.globals.entries.iter().find(|e| e.name == "present");
        assert!(entry.is_some_and(|e| e.persisted));
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
