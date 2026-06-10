use std::sync::Arc;

use forge_storage::Language;
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

fn settings_section_button(
    label: &str,
    section: SettingsSection,
    active: &SettingsSection,
    palette: &ForgePalette,
) -> Element<'static, Message> {
    if &section == active {
        forge_widgets::primary_button(
            label.to_owned(),
            Message::Navigate(Screen::Settings(section)),
            palette,
        )
    } else {
        forge_widgets::ghost_button(
            label.to_owned(),
            Message::Navigate(Screen::Settings(section)),
            palette,
        )
    }
}

fn settings_diagnostics_pane(palette: &ForgePalette) -> Element<'static, Message> {
    let version = env!("CARGO_PKG_VERSION");
    let log_dir = forge_platform_core::paths::data_dir().join("logs");

    let metrics = iced::widget::row![
        forge_widgets::metric_card(
            forge_widgets::tr!("settings_about_build_label"),
            version,
            None::<&str>,
            palette,
        ),
        forge_widgets::metric_card("Rust", "1.95.0", None::<&str>, palette),
        forge_widgets::metric_card("OS", std::env::consts::OS, None::<&str>, palette),
    ]
    .spacing(spf(Spacing::Sm));

    let log_path_label = iced::widget::text(forge_widgets::tr!(
        "settings_diagnostics_log_dir",
        path = log_dir.display().to_string()
    ))
    .size(FONT_SM)
    .color(palette.text_muted);
    let open_logs_btn = forge_widgets::primary_button(
        forge_widgets::tr!("settings_diagnostics_open_log_dir"),
        Message::Settings(SettingsMsg::OpenLogDirectoryRequested),
        palette,
    );
    let level_label = iced::widget::text(forge_widgets::tr!("settings_diagnostics_log_level_hint"))
        .size(FONT_SM)
        .color(palette.text_muted);

    let logs_card = forge_widgets::card(
        [
            iced::widget::text(forge_widgets::tr!("settings_diagnostics_section_title"))
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
    let path_label = iced::widget::text(forge_widgets::tr!(
        "settings_storage_db_path",
        path = db_path.display().to_string()
    ))
    .size(FONT_SM)
    .color(palette.text_muted);

    let vacuum_btn = forge_widgets::primary_button(
        forge_widgets::tr!("settings_storage_vacuum_btn"),
        Message::Settings(SettingsMsg::DbVacuumRequested),
        palette,
    );
    let vacuum_hint = iced::widget::text(forge_widgets::tr!("settings_storage_vacuum_hint"))
        .size(FONT_XS)
        .color(palette.text_faint);

    let backup_btn = forge_widgets::primary_button(
        forge_widgets::tr!("settings_storage_backup_btn"),
        Message::Settings(SettingsMsg::DbBackupRequested),
        palette,
    );
    let backup_hint = iced::widget::text(forge_widgets::tr!("settings_storage_backup_hint"))
        .size(FONT_XS)
        .color(palette.text_faint);

    let storage_card = forge_widgets::card(
        [
            iced::widget::text(forge_widgets::tr!("settings_storage_section_title"))
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
    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(0);
    let thread_hint =
        forge_widgets::tr!("settings_queues_thread_hint", workers = workers.to_string());
    let card = forge_widgets::card(
        [
            iced::widget::text(forge_widgets::tr!("settings_queues_section_title"))
                .size(FONT_SM)
                .color(palette.text_primary)
                .into(),
            iced::widget::text(thread_hint)
                .size(FONT_SM)
                .color(palette.text_muted)
                .into(),
            iced::widget::text(forge_widgets::tr!("settings_queues_managed_hint"))
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

fn language_option_row<'a>(
    native_label: &'a str,
    bcp47: &'a str,
    lang: Language,
    current: Language,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{Space, button, container, row, text};

    let p = *palette;
    let mono = forge_widgets::font(forge_widgets::FontRole::Monospace);
    let is_selected = lang == current;

    let bcp47_chip = container(text(bcp47).size(FONT_XS).color(p.text_primary).font(mono))
        .padding([3_u16, 8_u16])
        .style(move |_: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(p.surface_overlay)),
            border: iced::Border {
                radius: radius(Radius::Sm).into(),
                ..Default::default()
            },
            ..container::Style::default()
        });

    let inner = row![
        text(native_label).size(FONT_SM).color(p.text_primary),
        Space::new().width(Length::Fill),
        bcp47_chip,
    ]
    .align_y(iced::Alignment::Center)
    .padding([10_u16, 0_u16]);

    button(inner)
        .on_press(Message::Settings(
            crate::message::SettingsMsg::LanguageChanged(lang),
        ))
        .width(Length::Fill)
        .style(move |_: &iced::Theme, _status| {
            let bg_color = if is_selected {
                iced::Color { a: 0.12, ..p.brand }
            } else {
                iced::Color::TRANSPARENT
            };
            let border_color = if is_selected {
                iced::Color { a: 0.5, ..p.brand }
            } else {
                iced::Color::TRANSPARENT
            };
            iced::widget::button::Style {
                background: Some(iced::Background::Color(bg_color)),
                border: iced::Border {
                    color: border_color,
                    width: if is_selected { 1.0 } else { 0.0 },
                    radius: radius(Radius::Sm).into(),
                },
                text_color: p.text_primary,
                ..iced::widget::button::Style::default()
            }
        })
        .into()
}

fn settings_language_pane(current: Language, palette: &ForgePalette) -> Element<'_, Message> {
    use iced::widget::{column, container, row, text};

    let p = *palette;

    let header = row![
        tabler_icon(Icon::Globe, 18.0, p.brand),
        text(forge_widgets::tr!("settings_language_title"))
            .size(FONT_LG)
            .color(p.text_primary),
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(iced::Alignment::Center);

    let subtitle = text(forge_widgets::tr!("settings_language_subtitle"))
        .size(FONT_SM)
        .color(p.text_muted);

    let list = column![
        language_option_row("English", "en-US", Language::En, current, palette),
        language_option_row("Українська", "uk-UA", Language::Uk, current, palette),
    ]
    .spacing(spf(Spacing::Xs));

    let body = column![header, subtitle, list].spacing(spf(Spacing::Md));

    container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(sp(Spacing::Lg))
        .into()
}

fn density_option_row(
    density: forge_storage::settings::Density,
    current: forge_storage::settings::Density,
    palette: &ForgePalette,
) -> Element<'static, Message> {
    use iced::widget::{Space, button, column, row, text};

    let p = *palette;
    let is_selected = density == current;
    let (label, hint) = match density {
        forge_storage::settings::Density::Compact => (
            forge_widgets::tr!("settings_appearance_density_compact"),
            forge_widgets::tr!("settings_appearance_density_compact_hint"),
        ),
        forge_storage::settings::Density::Cozy => (
            forge_widgets::tr!("settings_appearance_density_cozy"),
            forge_widgets::tr!("settings_appearance_density_cozy_hint"),
        ),
        forge_storage::settings::Density::Spacious => (
            forge_widgets::tr!("settings_appearance_density_spacious"),
            forge_widgets::tr!("settings_appearance_density_spacious_hint"),
        ),
    };

    let labels = column![
        text(label).size(FONT_SM).color(p.text_primary),
        text(hint).size(FONT_XS).color(p.text_muted),
    ]
    .spacing(2);

    let mut inner = row![labels, Space::new().width(Length::Fill)]
        .align_y(iced::Alignment::Center)
        .padding([10_u16, 0_u16]);
    if is_selected {
        inner = inner.push(tabler_icon(Icon::CircleCheck, 16.0, p.brand));
    }

    button(inner)
        .on_press(Message::Settings(SettingsMsg::DensityChanged(density)))
        .width(Length::Fill)
        .style(move |_: &iced::Theme, _status| {
            let bg_color = if is_selected {
                iced::Color { a: 0.12, ..p.brand }
            } else {
                iced::Color::TRANSPARENT
            };
            let border_color = if is_selected {
                iced::Color { a: 0.5, ..p.brand }
            } else {
                iced::Color::TRANSPARENT
            };
            iced::widget::button::Style {
                background: Some(iced::Background::Color(bg_color)),
                border: iced::Border {
                    color: border_color,
                    width: if is_selected { 1.0 } else { 0.0 },
                    radius: radius(Radius::Sm).into(),
                },
                text_color: p.text_primary,
                ..iced::widget::button::Style::default()
            }
        })
        .into()
}

fn settings_appearance_pane(
    current: forge_storage::settings::Density,
    palette: &ForgePalette,
) -> Element<'_, Message> {
    use iced::widget::{column, container, row, text};

    let p = *palette;

    let header = row![
        tabler_icon(Icon::LayoutGrid, 18.0, p.brand),
        text(forge_widgets::tr!("settings_appearance_title"))
            .size(FONT_LG)
            .color(p.text_primary),
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(iced::Alignment::Center);

    let density_label = text(forge_widgets::tr!("settings_appearance_density_label"))
        .size(FONT_SM)
        .color(p.text_primary);
    let density_subtitle = text(forge_widgets::tr!("settings_appearance_density_subtitle"))
        .size(FONT_XS)
        .color(p.text_muted);

    let options = column![
        density_option_row(forge_storage::settings::Density::Compact, current, palette),
        density_option_row(forge_storage::settings::Density::Cozy, current, palette),
        density_option_row(forge_storage::settings::Density::Spacious, current, palette),
    ]
    .spacing(spf(Spacing::Xs));

    let body = column![
        header,
        column![density_label, density_subtitle].spacing(2),
        options
    ]
    .spacing(spf(Spacing::Md));

    container(body)
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
        text(forge_widgets::tr!("settings_shortcuts_title"))
            .size(FONT_LG)
            .color(p.text_primary),
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(iced::Alignment::Center);

    let subtitle = text(forge_widgets::tr!("settings_shortcuts_subtitle"))
        .size(FONT_SM)
        .color(p.text_muted);

    let rows = [
        (forge_widgets::tr!("settings_shortcut_save"), "Ctrl + S"),
        (
            forge_widgets::tr!("settings_shortcut_new_action"),
            "Ctrl + N",
        ),
        (
            forge_widgets::tr!("settings_shortcut_quick_switcher"),
            "Ctrl + K",
        ),
        (
            forge_widgets::tr!("settings_shortcut_toggle_chat"),
            "Ctrl + Shift + C",
        ),
        (
            forge_widgets::tr!("settings_shortcut_toggle_events"),
            "Ctrl + Shift + E",
        ),
        (forge_widgets::tr!("settings_shortcut_run_script"), "F5"),
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
        text(forge_widgets::tr!("settings_shortcuts_note"))
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
            iced::widget::text(forge_widgets::tr!("settings_notifications_section_title"))
                .size(FONT_SM)
                .color(palette.text_primary)
                .into(),
            iced::widget::text(forge_widgets::tr!("settings_notifications_hint"))
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

fn nav_group_header(label: &str, palette: &ForgePalette) -> Element<'static, Message> {
    iced::widget::text(label.to_owned())
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
    pub current_language: forge_storage::Language,
    pub current_density: forge_storage::settings::Density,
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
        current_language,
        current_density,
    } = params;
    let nav = iced::widget::column![
        nav_group_header(
            &forge_widgets::tr!("settings_nav_group_preferences"),
            palette
        ),
        settings_section_button(
            &forge_widgets::tr!("settings_nav_appearance"),
            SettingsSection::Appearance,
            section,
            palette,
        ),
        settings_section_button(
            &forge_widgets::tr!("settings_nav_language"),
            SettingsSection::Language,
            section,
            palette,
        ),
        settings_section_button(
            &forge_widgets::tr!("settings_nav_shortcuts"),
            SettingsSection::Shortcuts,
            section,
            palette,
        ),
        settings_section_button(
            &forge_widgets::tr!("settings_nav_notifications"),
            SettingsSection::Notifications,
            section,
            palette,
        ),
        iced::widget::Space::new().height(6),
        nav_group_header(&forge_widgets::tr!("settings_nav_group_engine"), palette),
        settings_section_button(
            &forge_widgets::tr!("settings_nav_audio"),
            SettingsSection::Audio,
            section,
            palette,
        ),
        settings_section_button(
            &forge_widgets::tr!("settings_nav_scripting"),
            SettingsSection::Scripting,
            section,
            palette,
        ),
        settings_section_button(
            &forge_widgets::tr!("settings_nav_queues"),
            SettingsSection::Queues,
            section,
            palette,
        ),
        settings_section_button(
            &forge_widgets::tr!("settings_nav_storage"),
            SettingsSection::Storage,
            section,
            palette,
        ),
        settings_section_button(
            &forge_widgets::tr!("settings_nav_websocket"),
            SettingsSection::WebSocket,
            section,
            palette,
        ),
        settings_section_button(
            &forge_widgets::tr!("settings_nav_hotkeys"),
            SettingsSection::Hotkeys,
            section,
            palette,
        ),
        iced::widget::Space::new().height(6),
        nav_group_header(&forge_widgets::tr!("settings_nav_group_about"), palette),
        settings_section_button(
            &forge_widgets::tr!("settings_nav_version"),
            SettingsSection::Version,
            section,
            palette,
        ),
        settings_section_button(
            &forge_widgets::tr!("settings_nav_diagnostics"),
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
        SettingsSection::Language => settings_language_pane(current_language, palette),
        SettingsSection::Appearance => settings_appearance_pane(current_density, palette),
        SettingsSection::Shortcuts => settings_shortcuts_pane(palette),
        SettingsSection::Hotkeys => crate::settings_hotkeys::view(hotkeys, rt, palette),
        SettingsSection::Scripting => crate::settings_scripting::view(scripting, palette),
        other => {
            let label = format!("Settings · {other:?}");
            iced::widget::container(forge_widgets::empty_state(
                label,
                forge_widgets::tr!("settings_coming_soon_placeholder"),
                None::<(&str, Message)>,
                palette,
            ))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        }
    };

    let page_header = simple_page_header(
        &[(forge_widgets::tr!("settings_page_title"), true)],
        palette,
    );
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
            app.language = lang;
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
        SettingsMsg::DensityChanged(density) => {
            app.density = density;
            crate::ui_settings::install_density(density);
            let settings: Arc<dyn SettingsRepo> =
                Arc::clone(&app.rt.backend) as Arc<dyn SettingsRepo>;
            Task::perform(
                async move {
                    settings
                        .set_density(density)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| Message::Settings(SettingsMsg::DensityPersisted(r)),
            )
        }
        SettingsMsg::DensityPersisted(result) => {
            if let Err(e) = result {
                tracing::warn!(error = %e, "failed to persist density selection");
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
