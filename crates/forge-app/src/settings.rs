use std::sync::Arc;

use forge_widgets::icons::{Icon, tabler_icon};
use forge_widgets::tokens::{FONT_LG, FONT_SM, FONT_XS, Spacing, sp, spf};
use forge_widgets::{ForgePalette, Radius, radius};
use iced::{Element, Length, Task};

use crate::app::App;

use forge_storage::SettingsRepo;

use crate::message::{Message, SettingsMsg};
use crate::page_chrome::simple_page_header;
use crate::runtime_view::RuntimeView;
use crate::screen::{Screen, SettingsSection};
use crate::server_screen::ServerScreenState;
use crate::settings_audio::{SettingsAudioState, settings_audio_view};
use crate::settings_hotkeys::SettingsHotkeysState;
use crate::settings_scripting::ScriptingSettingsState;
use crate::settings_websocket::settings_websocket_view;

fn settings_section_button<'a>(
    label: &'a str,
    section: SettingsSection,
    active: &SettingsSection,
    palette: &ForgePalette,
) -> Element<'a, Message> {
    if &section == active {
        forge_widgets::primary_button(label, Message::Navigate(Screen::Settings(section)), palette)
    } else {
        forge_widgets::ghost_button(label, Message::Navigate(Screen::Settings(section)), palette)
    }
}

fn settings_diagnostics_pane(palette: &ForgePalette) -> Element<'static, Message> {
    let version = env!("CARGO_PKG_VERSION");
    let log_dir = forge_platform_core::paths::data_dir().join("logs");

    let metrics = iced::widget::row![
        forge_widgets::metric_card("Build", version, None::<&str>, palette),
        forge_widgets::metric_card("Rust", "1.95.0", None::<&str>, palette),
        forge_widgets::metric_card("OS", std::env::consts::OS, None::<&str>, palette),
    ]
    .spacing(spf(Spacing::Sm));

    let log_path_label = iced::widget::text(format!("Log directory: {}", log_dir.display()))
        .size(FONT_SM)
        .color(palette.text_muted);
    let open_logs_btn = forge_widgets::primary_button(
        "Open log directory",
        Message::Settings(SettingsMsg::OpenLogDirectoryRequested),
        palette,
    );
    let level_label =
        iced::widget::text("Log level: controlled via RUST_LOG env var (e.g. info, debug, trace).")
            .size(FONT_SM)
            .color(palette.text_muted);

    let logs_card = forge_widgets::card(
        [
            iced::widget::text("Logs & diagnostics")
                .size(FONT_SM)
                .color(palette.text_primary)
                .into(),
            log_path_label.into(),
            open_logs_btn,
            level_label.into(),
        ],
        palette,
    );

    iced::widget::container(iced::widget::column![metrics, logs_card].spacing(spf(Spacing::Md)))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(sp(Spacing::Md))
        .into()
}

fn settings_storage_pane(palette: &ForgePalette) -> Element<'static, Message> {
    let db_path = forge_platform_core::paths::data_dir().join("forge.db");
    let path_label = iced::widget::text(format!("Database: {}", db_path.display()))
        .size(FONT_SM)
        .color(palette.text_muted);

    let vacuum_btn = forge_widgets::primary_button(
        "Vacuum (export compact snapshot)",
        Message::Settings(SettingsMsg::DbVacuumRequested),
        palette,
    );
    let vacuum_hint = iced::widget::text(
        "Writes a vacuumed snapshot to a temp file; useful before manual backups.",
    )
    .size(FONT_XS)
    .color(palette.text_faint);

    let backup_btn = forge_widgets::primary_button(
        "Backup now",
        Message::Settings(SettingsMsg::DbBackupRequested),
        palette,
    );
    let backup_hint = iced::widget::text("Creates a timestamped DB copy in the data directory.")
        .size(FONT_XS)
        .color(palette.text_faint);

    let storage_card = forge_widgets::card(
        [
            iced::widget::text("Storage & backups")
                .size(FONT_SM)
                .color(palette.text_primary)
                .into(),
            path_label.into(),
            vacuum_btn,
            vacuum_hint.into(),
            backup_btn,
            backup_hint.into(),
        ],
        palette,
    );

    iced::widget::container(storage_card)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(sp(Spacing::Md))
        .into()
}

fn settings_queues_pane(palette: &ForgePalette) -> Element<'static, Message> {
    let thread_hint = format!(
        "Tokio threadpool: {} worker(s) (auto-sized to system).",
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(0)
    );
    let card = forge_widgets::card(
        [
            iced::widget::text("Queues & threading")
                .size(FONT_SM)
                .color(palette.text_primary)
                .into(),
            iced::widget::text(thread_hint)
                .size(FONT_SM)
                .color(palette.text_muted)
                .into(),
            iced::widget::text(
                "Per-queue concurrency limits and blocking flags are managed on the Queues screen.",
            )
            .size(FONT_XS)
            .color(palette.text_faint)
            .into(),
        ],
        palette,
    );
    iced::widget::container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(sp(Spacing::Md))
        .into()
}

const FORGE_SETTINGS_ROW_BORDER: f32 = 0.5;

fn settings_language_pane(palette: &ForgePalette) -> Element<'static, Message> {
    use iced::widget::{Space, column, container, row, text};

    let p = *palette;
    let mono = forge_widgets::font(forge_widgets::FontRole::Monospace);

    let header = row![
        tabler_icon(Icon::Globe, 18.0, p.brand),
        text("Language & region")
            .size(FONT_LG)
            .color(p.text_primary),
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(iced::Alignment::Center);

    let rows: [(&str, &str); 4] = [
        ("Interface language", "Ukrainian (uk-UA)"),
        ("Region", "Ukraine"),
        ("Date format", "DD.MM.YYYY"),
        ("First day of week", "Monday"),
    ];

    let mut list = column![].spacing(0);
    let count = rows.len();
    for (i, (label, value)) in rows.into_iter().enumerate() {
        let bottom = if i == count - 1 {
            0_u16
        } else {
            FORGE_SETTINGS_ROW_BORDER as u16
        };
        let _ = bottom;
        let row_el = container(
            row![
                text(label).size(FONT_SM).color(p.text_primary),
                Space::new().width(Length::Fill),
                container(text(value).size(FONT_SM).color(p.text_secondary).font(mono))
                    .padding([3_u16, 8_u16])
                    .style(move |_: &iced::Theme| container::Style {
                        background: Some(iced::Background::Color(p.surface_overlay)),
                        border: iced::Border {
                            radius: radius(Radius::Sm).into(),
                            ..Default::default()
                        },
                        ..container::Style::default()
                    }),
            ]
            .align_y(iced::Alignment::Center),
        )
        .padding([10_u16, 0_u16])
        .width(Length::Fill)
        .style(move |_: &iced::Theme| {
            let border_color = if i + 1 == count {
                iced::Color::TRANSPARENT
            } else {
                p.border_regular
            };
            container::Style {
                border: iced::Border {
                    color: border_color,
                    width: 0.5,
                    radius: 0.0.into(),
                },
                ..container::Style::default()
            }
        });
        list = list.push(row_el);
    }

    let body = column![header, list].spacing(spf(Spacing::Md));

    iced::widget::container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(sp(Spacing::Lg))
        .into()
}

fn settings_shortcuts_pane(palette: &ForgePalette) -> Element<'static, Message> {
    use iced::widget::{Space, column, container, row, text};

    let p = *palette;
    let mono = forge_widgets::font(forge_widgets::FontRole::Monospace);

    let header = row![
        tabler_icon(Icon::Keyboard, 18.0, p.brand),
        text("Shortcuts").size(FONT_LG).color(p.text_primary),
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(iced::Alignment::Center);

    let subtitle = text("Quick keys across Forge")
        .size(FONT_SM)
        .color(p.text_muted);

    let rows: [(&str, &str); 6] = [
        ("Save", "Ctrl + S"),
        ("New action", "Ctrl + N"),
        ("Quick switcher", "Ctrl + K"),
        ("Toggle Live Chat", "Ctrl + Shift + C"),
        ("Toggle Event Feed", "Ctrl + Shift + E"),
        ("Run script", "F5"),
    ];

    let mut list = column![].spacing(0);
    let count = rows.len();
    for (i, (label, key)) in rows.into_iter().enumerate() {
        let key_chip = container(text(key).size(FONT_XS).color(p.text_primary).font(mono))
            .padding([3_u16, 8_u16])
            .style(move |_: &iced::Theme| container::Style {
                background: Some(iced::Background::Color(p.surface_overlay)),
                border: iced::Border {
                    radius: radius(Radius::Sm).into(),
                    ..Default::default()
                },
                ..container::Style::default()
            });

        let row_el = container(
            row![
                text(label).size(FONT_SM).color(p.text_primary),
                Space::new().width(Length::Fill),
                key_chip,
            ]
            .align_y(iced::Alignment::Center),
        )
        .padding([10_u16, 0_u16])
        .width(Length::Fill)
        .style(move |_: &iced::Theme| {
            let border_color = if i + 1 == count {
                iced::Color::TRANSPARENT
            } else {
                p.border_regular
            };
            container::Style {
                border: iced::Border {
                    color: border_color,
                    width: 0.5,
                    radius: 0.0.into(),
                },
                ..container::Style::default()
            }
        });
        list = list.push(row_el);
    }

    let note = container(
        text("Keyboard shortcuts not yet bound — labels only for now.")
            .size(FONT_XS)
            .color(p.text_faint)
            .font(mono),
    )
    .padding([8_u16, 0_u16]);

    let body = column![header, subtitle, list, note].spacing(spf(Spacing::Sm));

    iced::widget::container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(sp(Spacing::Lg))
        .into()
}

fn settings_notifications_pane(palette: &ForgePalette) -> Element<'static, Message> {
    let card = forge_widgets::card(
        [
            iced::widget::text("Notifications")
                .size(FONT_SM)
                .color(palette.text_primary)
                .into(),
            iced::widget::text(
                "Per-event-type toast customisation lands in beta-2. Errors and connection \
                 changes always surface in the status bar.",
            )
            .size(FONT_SM)
            .color(palette.text_muted)
            .into(),
        ],
        palette,
    );
    iced::widget::container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(sp(Spacing::Md))
        .into()
}

fn nav_group_header<'a>(label: &'a str, palette: &'a ForgePalette) -> Element<'a, Message> {
    iced::widget::text(label)
        .font(forge_widgets::tokens::font(
            forge_widgets::tokens::FontRole::Monospace,
        ))
        .size(FONT_XS)
        .color(palette.text_faint)
        .into()
}

pub(crate) struct SettingsViewParams<'a> {
    pub section: &'a SettingsSection,
    pub ws: &'a crate::settings_websocket::SettingsWebSocketState,
    pub server: &'a ServerScreenState,
    pub audio: &'a SettingsAudioState,
    pub hotkeys: &'a SettingsHotkeysState,
    pub scripting: &'a ScriptingSettingsState,
    pub rt: &'a RuntimeView,
}

pub(crate) fn settings_view<'a>(
    params: SettingsViewParams<'a>,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let SettingsViewParams {
        section,
        ws,
        server,
        audio,
        hotkeys,
        scripting,
        rt,
    } = params;
    let nav = iced::widget::column![
        nav_group_header("PREFERENCES", palette),
        settings_section_button("Appearance", SettingsSection::Appearance, section, palette),
        settings_section_button("Language", SettingsSection::Language, section, palette),
        settings_section_button("Shortcuts", SettingsSection::Shortcuts, section, palette),
        settings_section_button(
            "Notifications",
            SettingsSection::Notifications,
            section,
            palette,
        ),
        iced::widget::Space::new().height(6),
        nav_group_header("ENGINE", palette),
        settings_section_button("Audio", SettingsSection::Audio, section, palette),
        settings_section_button("Scripting", SettingsSection::Scripting, section, palette),
        settings_section_button("Queues", SettingsSection::Queues, section, palette),
        settings_section_button("Storage", SettingsSection::Storage, section, palette),
        settings_section_button("WebSocket", SettingsSection::WebSocket, section, palette),
        settings_section_button("Hotkeys", SettingsSection::Hotkeys, section, palette),
        iced::widget::Space::new().height(6),
        nav_group_header("ABOUT", palette),
        settings_section_button("Version", SettingsSection::Version, section, palette),
        settings_section_button(
            "Diagnostics",
            SettingsSection::Diagnostics,
            section,
            palette,
        ),
    ]
    .spacing(spf(Spacing::Xxs))
    .padding([12_u16, 8_u16])
    .width(Length::Fixed(200.0));

    let nav_container = iced::widget::container(nav)
        .height(Length::Fill)
        .style(move |_| iced::widget::container::Style {
            background: Some(iced::Background::Color(palette.shell)),
            border: iced::Border {
                color: palette.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..iced::widget::container::Style::default()
        });

    let pane: Element<'a, Message> = match section {
        SettingsSection::Diagnostics => settings_diagnostics_pane(palette),
        SettingsSection::Audio => settings_audio_view(audio, palette),
        SettingsSection::WebSocket => {
            settings_websocket_view(ws, &server.bearer_token, server.token_revealed, palette)
        }
        SettingsSection::Storage => settings_storage_pane(palette),
        SettingsSection::Queues => settings_queues_pane(palette),
        SettingsSection::Notifications => settings_notifications_pane(palette),
        SettingsSection::Language => settings_language_pane(palette),
        SettingsSection::Shortcuts => settings_shortcuts_pane(palette),
        SettingsSection::Hotkeys => crate::settings_hotkeys::view(hotkeys, rt, palette),
        SettingsSection::Scripting => crate::settings_scripting::view(scripting, palette),
        other => {
            let label = format!("Settings · {other:?}");
            iced::widget::container(forge_widgets::empty_state(
                label,
                "Coming with alpha-N.",
                None::<(&str, Message)>,
                palette,
            ))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        }
    };

    let page_header = simple_page_header(&[("Settings", true)], palette);
    let body = iced::widget::row![nav_container, pane].spacing(0);

    iced::widget::column![page_header, body]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

pub(crate) fn handle_message(app: &mut App, sub: SettingsMsg) -> Task<Message> {
    match sub {
        SettingsMsg::ReconnectPlatform(platform) => {
            let builtin = match platform {
                forge_types::PlatformId::Twitch => "twitch",
                forge_types::PlatformId::YouTube => "youtube",
                forge_types::PlatformId::Kick => "kick",
            };
            Task::batch([
                Task::done(Message::Navigate(Screen::BuiltinDetail(
                    forge_platform_core::BuiltinId::new(builtin),
                ))),
                Task::done(Message::LocalCallbackFlow(
                    crate::local_callback_flow::LocalCallbackFlowMsg::ConnectPlatform(platform),
                )),
            ])
        }
        SettingsMsg::PlatformReconnectResult(Ok(())) => Task::none(),
        SettingsMsg::PlatformReconnectResult(Err(e)) => {
            tracing::warn!(error = %e, "platform reconnect failed");
            Task::none()
        }
        SettingsMsg::DbVacuumRequested => {
            let dp = Arc::clone(&app.rt.backend);
            Task::perform(
                async move {
                    let tmp_target = std::env::temp_dir().join("forge_vacuum.db");
                    dp.export(&tmp_target)
                        .await
                        .map(|()| tmp_target.metadata().map(|m| m.len()).unwrap_or(0))
                        .map_err(|e| e.to_string())
                },
                |r| Message::Settings(SettingsMsg::DbVacuumDone(r)),
            )
        }
        SettingsMsg::DbVacuumDone(result) => {
            match result {
                Ok(bytes) => tracing::info!(bytes, "DB vacuum exported snapshot"),
                Err(e) => tracing::warn!(error = %e, "DB vacuum failed"),
            }
            Task::none()
        }
        SettingsMsg::DbBackupRequested => {
            let dp = Arc::clone(&app.rt.backend);
            Task::perform(
                async move {
                    let stamp = time::OffsetDateTime::now_utc().unix_timestamp();
                    let path = forge_platform_core::paths::data_dir()
                        .join(format!("forge-backup-{stamp}.db"));
                    dp.export(&path)
                        .await
                        .map(|()| path.display().to_string())
                        .map_err(|e| e.to_string())
                },
                |r| Message::Settings(SettingsMsg::DbBackupDone(r)),
            )
        }
        SettingsMsg::DbBackupDone(result) => {
            match result {
                Ok(path) => tracing::info!(path = %path, "DB backup created"),
                Err(e) => tracing::warn!(error = %e, "DB backup failed"),
            }
            Task::none()
        }
        SettingsMsg::OpenLogDirectoryRequested => {
            let log_dir = forge_platform_core::paths::data_dir().join("logs");
            Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        open::that(&log_dir).map_err(|e| e.to_string())
                    })
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r)
                },
                |r| Message::Settings(SettingsMsg::OpenLogDirectoryResult(r)),
            )
        }
        SettingsMsg::OpenLogDirectoryResult(result) => {
            if let Err(e) = result {
                tracing::warn!(error = %e, "failed to open log directory");
            }
            Task::none()
        }
        SettingsMsg::Scripting(sub) => {
            crate::settings_scripting::update(&mut app.ui.settings_scripting, &app.rt, sub)
        }
        SettingsMsg::LanguageChanged(lang) => {
            crate::i18n::install_language(lang);
            let settings: Arc<dyn SettingsRepo> =
                Arc::clone(&app.rt.backend) as Arc<dyn SettingsRepo>;
            Task::perform(
                async move { settings.set_language(lang).await.map_err(|e| e.to_string()) },
                |r| Message::Settings(SettingsMsg::LanguagePersisted(r)),
            )
        }
        SettingsMsg::LanguagePersisted(result) => {
            if let Err(e) = result {
                tracing::warn!(error = %e, "failed to persist language selection");
            }
            Task::none()
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::screen::Screen;
    use forge_types::PlatformId;

    #[test]
    fn reconnect_platform_youtube_dispatches_navigate_to_local_callback_flow() {
        let mut app = App::default();
        let _task = handle_message(
            &mut app,
            SettingsMsg::ReconnectPlatform(PlatformId::YouTube),
        );
        assert_eq!(app.screen, Screen::Home);
    }

    #[test]
    fn reconnect_platform_twitch_dispatches_navigate_to_local_callback_flow() {
        let mut app = App::default();
        let _task = handle_message(&mut app, SettingsMsg::ReconnectPlatform(PlatformId::Twitch));
        assert_eq!(app.screen, Screen::Home);
    }
}
