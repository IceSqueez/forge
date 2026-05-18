use std::sync::Arc;

use forge_storage::{GlobalEntry, GlobalsRepo};
use forge_storage_sqlite::SqliteBackend;
use forge_widgets::tokens::{FONT_BODY_MD, FONT_BODY_SM, FONT_CAPS_SM};
use forge_widgets::{
    FontRole, FooterProps, ForgePalette, VariantKind, data_screen_footer, data_table, empty_state,
    font, persistence_toggle_inline, primary_button_small, search_input, secondary_button,
    type_pill, value_preview,
};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Shadow,
    widget::{Space, button, column, container, row, rule, scrollable, text},
};
use time::OffsetDateTime;

use crate::Message;
use crate::app::App;
use crate::message::{GlobalsFilter, GlobalsLoadData, GlobalsMsg};

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

        GlobalsMsg::OpenCreateModal
        | GlobalsMsg::OpenEditModal(_)
        | GlobalsMsg::ExportRequested => iced::Task::none(),
    }
}

pub fn globals_view<'a>(app: &'a App, palette: &'a ForgePalette) -> Element<'a, Message> {
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

fn format_storage_bytes(bytes: u64) -> String {
    if bytes == 0 {
        "Storage: 0 B".to_owned()
    } else {
        let kb = bytes as f64 / 1024.0;
        format!("Storage: {kb:.1} KB")
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::Message;
    use crate::app::{App, update};
    use crate::screen::Screen;
    use forge_storage::GlobalEntry;
    use forge_types::Variant;
    use time::OffsetDateTime;

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
}
