use std::sync::{Arc, LazyLock};

use forge_storage::Language;
use forge_widgets::icons::{Icon, tabler_icon};
use forge_widgets::tokens::{FONT_LG, FONT_MD, FONT_SM, FONT_XS, FONT_XXS, Spacing, sp, spf};
use forge_widgets::{ForgePalette, Radius, radius};
use iced::{Element, Length, Task};

use crate::app::App;

use forge_storage::SettingsRepo;

use crate::message::{Message, SettingsMsg};
use crate::runtime_view::RuntimeView;
use crate::screen::{Screen, SettingsSection};
use crate::server_screen::ServerScreenState;
use crate::settings_audio::{SettingsAudioState, settings_audio_view};
use crate::settings_hotkeys::SettingsHotkeysState;
use crate::settings_scripting::ScriptingSettingsState;
use crate::settings_websocket::settings_websocket_view;

fn section_icon(section: &SettingsSection) -> Icon {
    match section {
        SettingsSection::Appearance => Icon::Photo,
        SettingsSection::Language => Icon::Globe,
        SettingsSection::Shortcuts => Icon::Keyboard,
        SettingsSection::Notifications => Icon::InfoCircle,
        SettingsSection::Audio => Icon::Volume,
        SettingsSection::Scripting => Icon::Terminal,
        SettingsSection::Queues => Icon::Notebook,
        SettingsSection::Storage => Icon::Folder,
        SettingsSection::WebSocket => Icon::Server,
        SettingsSection::Hotkeys => Icon::Bolt,
        SettingsSection::Version => Icon::Diamond,
        SettingsSection::Diagnostics => Icon::Activity,
    }
}

fn section_label(section: &SettingsSection) -> String {
    match section {
        SettingsSection::Appearance => forge_widgets::tr!("settings_nav_appearance"),
        SettingsSection::Language => forge_widgets::tr!("settings_nav_language"),
        SettingsSection::Shortcuts => forge_widgets::tr!("settings_nav_shortcuts"),
        SettingsSection::Notifications => forge_widgets::tr!("settings_nav_notifications"),
        SettingsSection::Audio => forge_widgets::tr!("settings_nav_audio"),
        SettingsSection::Scripting => forge_widgets::tr!("settings_nav_scripting"),
        SettingsSection::Queues => forge_widgets::tr!("settings_nav_queues"),
        SettingsSection::Storage => forge_widgets::tr!("settings_nav_storage"),
        SettingsSection::WebSocket => forge_widgets::tr!("settings_nav_websocket"),
        SettingsSection::Hotkeys => forge_widgets::tr!("settings_nav_hotkeys"),
        SettingsSection::Version => forge_widgets::tr!("settings_nav_version"),
        SettingsSection::Diagnostics => forge_widgets::tr!("settings_nav_diagnostics"),
    }
}

fn settings_section_button(
    label: &str,
    section: SettingsSection,
    active: &SettingsSection,
    palette: &ForgePalette,
) -> Element<'static, Message> {
    let icon = section_icon(&section);
    if &section == active {
        forge_widgets::primary_button_with_icon(
            icon,
            label.to_owned(),
            Message::Navigate(Screen::Settings(section)),
            palette,
        )
    } else {
        forge_widgets::ghost_button_with_icon(
            icon,
            label.to_owned(),
            Message::Navigate(Screen::Settings(section)),
            palette,
        )
    }
}

/// One parsed release header from the embedded changelog.
struct ReleaseEntry {
    version: String,
    date: String,
    summary: String,
}

/// Embedded changelog, parsed once into the most recent release headers.
/// Kept behind `LazyLock` so the ~36 KB markdown is scanned a single time
/// rather than on every `view` frame.
static RECENT_RELEASES: LazyLock<Vec<ReleaseEntry>> =
    LazyLock::new(|| parse_release_headers(include_str!("../../../CHANGELOG.md"), 4));

/// Strips a leading `*(scope)*` tag and a `[**breaking**]` marker from a
/// changelog bullet, leaving the human-readable summary line.
fn clean_bullet(bullet: &str) -> String {
    let mut text = bullet.trim();
    if let Some(rest) = text.strip_prefix("*(")
        && let Some(idx) = rest.find(")* ")
    {
        text = rest[idx + 3..].trim_start();
    }
    text.replace("[**breaking**] ", "").trim().to_string()
}

/// Extracts up to `limit` release headers from git-cliff changelog markdown.
/// Each `## [version] - date` header yields a version tag, date, and a short
/// summary drawn from the release's first Features bullet (falling back to the
/// first bullet of any category). Never synthesizes entries not in the source.
fn parse_release_headers(raw: &str, limit: usize) -> Vec<ReleaseEntry> {
    let mut out: Vec<ReleaseEntry> = Vec::new();
    let mut version = String::new();
    let mut date = String::new();
    let mut first_any: Option<String> = None;
    let mut first_feature: Option<String> = None;
    let mut in_features = false;
    let mut open = false;

    let flush = |out: &mut Vec<ReleaseEntry>,
                 version: &str,
                 date: &str,
                 first_any: &Option<String>,
                 first_feature: &Option<String>| {
        let summary = first_feature
            .clone()
            .or_else(|| first_any.clone())
            .unwrap_or_default();
        out.push(ReleaseEntry {
            version: version.to_string(),
            date: date.to_string(),
            summary,
        });
    };

    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("## [") {
            if open {
                flush(&mut out, &version, &date, &first_any, &first_feature);
                if out.len() >= limit {
                    return out;
                }
            }
            let (v, d) = match rest.split_once("] - ") {
                Some((v, d)) => (v.trim().to_string(), d.trim().to_string()),
                None => (rest.trim_end_matches(']').trim().to_string(), String::new()),
            };
            version = v;
            date = d;
            first_any = None;
            first_feature = None;
            in_features = false;
            open = true;
            continue;
        }
        if !open {
            continue;
        }
        if trimmed.starts_with("### ") {
            in_features = trimmed.contains("Features");
            continue;
        }
        if let Some(bullet) = trimmed.strip_prefix("- ") {
            let cleaned = clean_bullet(bullet);
            if cleaned.is_empty() {
                continue;
            }
            if first_any.is_none() {
                first_any = Some(cleaned.clone());
            }
            if in_features && first_feature.is_none() {
                first_feature = Some(cleaned);
            }
        }
    }
    if open && out.len() < limit {
        flush(&mut out, &version, &date, &first_any, &first_feature);
    }
    out
}

/// GitHub releases page for the "Check for updates" affordance.
const RELEASES_URL: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/releases");

fn release_row<'a>(entry: &'a ReleaseEntry, palette: &ForgePalette) -> Element<'a, Message> {
    use iced::widget::{Space, row, text};

    let mono = forge_widgets::font(forge_widgets::FontRole::Monospace);

    row![
        iced::widget::container(
            text(entry.version.as_str())
                .size(FONT_XS)
                .font(mono)
                .color(palette.text_primary)
        )
        .width(Length::Fixed(130.0)),
        text(entry.summary.as_str())
            .size(FONT_XS)
            .color(palette.text_muted),
        Space::new().width(Length::Fill),
        text(entry.date.as_str())
            .size(FONT_XXS)
            .font(mono)
            .color(palette.text_faint),
    ]
    .spacing(spf(Spacing::Sm))
    .align_y(iced::Alignment::Center)
    .into()
}

fn settings_version_pane(palette: &ForgePalette) -> Element<'static, Message> {
    use iced::widget::{Space, column, container, row, text};

    let p = *palette;
    let mono = forge_widgets::font(forge_widgets::FontRole::Monospace);
    let version = env!("CARGO_PKG_VERSION");

    let header = row![
        tabler_icon(Icon::InfoCircle, 18.0, p.brand),
        text(forge_widgets::tr!("settings_version_title"))
            .size(FONT_LG)
            .color(p.text_primary),
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(iced::Alignment::Center);

    // Identity card: mauve "F" tile + name/version + tagline + updates button.
    let tile = container(text("F").size(24.0).color(p.base))
        .center_x(Length::Fixed(48.0))
        .center_y(Length::Fixed(48.0))
        .style(move |_: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(p.brand)),
            border: iced::Border {
                radius: 11.0.into(),
                ..Default::default()
            },
            ..container::Style::default()
        });

    let name_line = row![
        text("Forge").size(FONT_MD).color(p.text_primary),
        text(format!("v{version}"))
            .size(FONT_XS)
            .font(mono)
            .color(p.text_muted),
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(iced::Alignment::Center);

    let tagline = text(forge_widgets::tr!("settings_version_tagline"))
        .size(FONT_XS)
        .color(p.text_muted);

    let identity = row![
        tile,
        column![name_line, tagline].spacing(4),
        Space::new().width(Length::Fill),
        forge_widgets::ghost_button_with_icon(
            Icon::Refresh,
            forge_widgets::tr!("settings_version_check_updates"),
            Message::Settings(SettingsMsg::CheckForUpdatesRequested),
            palette,
        ),
    ]
    .spacing(spf(Spacing::Md))
    .align_y(iced::Alignment::Center);

    let identity_card = forge_widgets::card(identity, palette);

    // Recent releases card, sourced from the embedded changelog.
    let releases_header = text(forge_widgets::tr!("settings_version_recent_releases"))
        .size(FONT_XXS)
        .font(mono)
        .color(p.text_muted);

    let mut releases_col = column![releases_header].spacing(spf(Spacing::Xs));
    if RECENT_RELEASES.is_empty() {
        releases_col = releases_col.push(
            text(forge_widgets::tr!("settings_version_changelog_empty"))
                .size(FONT_XS)
                .color(p.text_faint),
        );
    } else {
        for entry in RECENT_RELEASES.iter() {
            releases_col = releases_col.push(release_row(entry, palette));
        }
    }
    let releases_card = forge_widgets::card(releases_col, palette);

    container(column![header, identity_card, releases_card].spacing(spf(Spacing::Md)))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(sp(Spacing::Md))
        .into()
}

fn settings_diagnostics_pane(palette: &ForgePalette) -> Element<'static, Message> {
    let version = env!("CARGO_PKG_VERSION");
    let log_dir = forge_platform_core::paths::data_dir().join("logs");

    let metrics = iced::widget::row![
        forge_widgets::metric_card(
            forge_widgets::tr!("settings_about_build_label"),
            version,
            None::<&str>,
            None,
            palette,
        ),
        forge_widgets::metric_card("Rust", "1.95.0", None::<&str>, None, palette),
        forge_widgets::metric_card("OS", std::env::consts::OS, None::<&str>, None, palette),
    ]
    .spacing(spf(Spacing::Sm));

    let log_dir_row = forge_widgets::settings_info_row(
        &forge_widgets::tr!("settings_diagnostics_log_dir_label"),
        &log_dir.display().to_string(),
        true,
        palette,
    );
    let open_logs_btn = forge_widgets::primary_button(
        forge_widgets::tr!("settings_diagnostics_open_log_dir"),
        Message::Settings(SettingsMsg::OpenLogDirectoryRequested),
        palette,
    );
    let level_label = iced::widget::text(forge_widgets::tr!("settings_diagnostics_log_level_hint"))
        .size(FONT_SM)
        .color(palette.text_muted);

    let logs_card = forge_widgets::card(
        iced::widget::column![
            iced::widget::text(forge_widgets::tr!("settings_diagnostics_section_title"))
                .size(FONT_SM)
                .color(palette.text_primary),
            log_dir_row,
            open_logs_btn,
            level_label,
        ]
        .spacing(spf(Spacing::Xs)),
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
    let path_row = forge_widgets::settings_info_row(
        &forge_widgets::tr!("settings_storage_db_path_label"),
        &db_path.display().to_string(),
        true,
        palette,
    );

    let backup_btn = forge_widgets::primary_button(
        forge_widgets::tr!("settings_storage_backup_btn"),
        Message::Settings(SettingsMsg::DbBackupRequested),
        palette,
    );
    let backup_hint = iced::widget::text(forge_widgets::tr!("settings_storage_backup_hint"))
        .size(FONT_XS)
        .color(palette.text_faint);

    let storage_card = forge_widgets::card(
        iced::widget::column![
            iced::widget::text(forge_widgets::tr!("settings_storage_section_title"))
                .size(FONT_SM)
                .color(palette.text_primary),
            path_row,
            backup_btn,
            backup_hint,
        ]
        .spacing(spf(Spacing::Xs)),
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
    let workers_row = forge_widgets::settings_info_row(
        &forge_widgets::tr!("settings_queues_workers_label"),
        &workers.to_string(),
        true,
        palette,
    );
    let card = forge_widgets::card(
        iced::widget::column![
            iced::widget::text(forge_widgets::tr!("settings_queues_section_title"))
                .size(FONT_SM)
                .color(palette.text_primary),
            workers_row,
            iced::widget::text(forge_widgets::tr!("settings_queues_managed_hint"))
                .size(FONT_XS)
                .color(palette.text_faint),
        ]
        .spacing(spf(Spacing::Xs)),
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

fn font_role_picker<'a>(
    role: forge_widgets::FontRole,
    fonts: &'a crate::ui_settings::FontSettings,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{checkbox, column, row, text};

    let p = *palette;
    let (label, default_family) = match role {
        forge_widgets::FontRole::Body => (
            forge_widgets::tr!("settings_appearance_font_body_label"),
            forge_widgets::DEFAULT_BODY_FAMILY,
        ),
        forge_widgets::FontRole::Monospace => (
            forge_widgets::tr!("settings_appearance_font_mono_label"),
            forge_widgets::DEFAULT_MONO_FAMILY,
        ),
    };

    let selected = fonts.stored(role).map(str::to_owned);
    let options: Vec<String> = fonts
        .catalog
        .iter()
        .filter(|f| {
            !matches!(role, forge_widgets::FontRole::Monospace)
                || fonts.mono_show_all
                || f.monospaced
                || Some(f.name.as_str()) == fonts.stored(role)
        })
        .map(|f| f.name.clone())
        .collect();

    let picker = forge_widgets::select_owned(
        options,
        selected.clone(),
        forge_widgets::tr!(
            "settings_appearance_font_default_placeholder",
            family = default_family
        ),
        move |name| Message::Settings(SettingsMsg::FontChanged(role, Some(name))),
        palette,
    );

    let mut controls = row![picker]
        .spacing(spf(Spacing::Xs))
        .align_y(iced::Alignment::Center);
    if selected.is_some() {
        controls = controls.push(forge_widgets::ghost_button(
            forge_widgets::tr!("settings_appearance_font_reset"),
            Message::Settings(SettingsMsg::FontChanged(role, None)),
            palette,
        ));
    }

    let mut section = column![text(label).size(FONT_SM).color(p.text_primary), controls]
        .spacing(spf(Spacing::Xxs));

    if let Some(name) = fonts.missing(role) {
        section = section.push(
            text(forge_widgets::tr!(
                "settings_appearance_font_missing",
                family = name.to_owned()
            ))
            .size(FONT_XS)
            .color(p.warning),
        );
    }

    section = section.push(
        text(forge_widgets::tr!("settings_appearance_font_preview"))
            .size(FONT_SM)
            .font(forge_widgets::font(role))
            .color(p.text_muted),
    );

    if matches!(role, forge_widgets::FontRole::Monospace) {
        section = section.push(
            checkbox(fonts.mono_show_all)
                .label(forge_widgets::tr!("settings_appearance_font_show_all"))
                .size(FONT_SM)
                .on_toggle(|b| Message::Settings(SettingsMsg::FontMonoShowAllToggled(b))),
        );
    }

    section.into()
}

fn settings_fonts_section<'a>(
    fonts: &'a crate::ui_settings::FontSettings,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{column, text};

    let p = *palette;
    let label = text(forge_widgets::tr!("settings_appearance_fonts_label"))
        .size(FONT_SM)
        .color(p.text_primary);
    let subtitle = text(forge_widgets::tr!("settings_appearance_fonts_subtitle"))
        .size(FONT_XS)
        .color(p.text_muted);

    let content: Element<'a, Message> = if fonts.catalog_loaded {
        column![
            font_role_picker(forge_widgets::FontRole::Body, fonts, palette),
            font_role_picker(forge_widgets::FontRole::Monospace, fonts, palette),
        ]
        .spacing(spf(Spacing::Sm))
        .into()
    } else {
        text(forge_widgets::tr!("settings_appearance_fonts_scanning"))
            .size(FONT_SM)
            .color(p.text_muted)
            .into()
    };

    column![column![label, subtitle].spacing(2), content]
        .spacing(spf(Spacing::Sm))
        .into()
}

fn theme_option<'a>(
    theme_id: forge_widgets::ThemeId,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let (title, subtitle) = match theme_id {
        forge_widgets::ThemeId::CatppuccinMocha => (
            "Catppuccin Mocha",
            forge_widgets::tr!("settings_theme_mocha_desc"),
        ),
        forge_widgets::ThemeId::TokyoNight => (
            "Tokyo Night",
            forge_widgets::tr!("settings_theme_tokyo_desc"),
        ),
        forge_widgets::ThemeId::Latte => (
            "Catppuccin Latte",
            forge_widgets::tr!("settings_theme_latte_desc"),
        ),
    };
    let selected = *palette == *forge_widgets::palette_for_theme(theme_id);
    forge_widgets::theme_card(
        forge_widgets::ThemeCardParams {
            theme_id,
            title: title.to_owned(),
            subtitle,
            active_label: forge_widgets::tr!("settings_theme_active"),
            selected,
        },
        Message::ThemeChanged(theme_id),
        palette,
    )
}

fn settings_appearance_pane<'a>(
    current: forge_storage::settings::Density,
    fonts: &'a crate::ui_settings::FontSettings,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{column, container, row, scrollable, text};

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

    let theme_label = text(forge_widgets::tr!("settings_appearance_theme_label"))
        .size(FONT_SM)
        .color(p.text_primary);
    let theme_row = row![
        theme_option(forge_widgets::ThemeId::CatppuccinMocha, palette),
        theme_option(forge_widgets::ThemeId::TokyoNight, palette),
        theme_option(forge_widgets::ThemeId::Latte, palette),
    ]
    .spacing(spf(Spacing::Sm));

    let body = column![
        header,
        theme_label,
        theme_row,
        column![density_label, density_subtitle].spacing(2),
        options,
        settings_fonts_section(fonts, palette),
    ]
    .spacing(spf(Spacing::Md))
    .padding(sp(Spacing::Lg));

    container(scrollable(body))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn settings_notifications_pane(palette: &ForgePalette) -> Element<'static, Message> {
    let card = forge_widgets::card(
        iced::widget::column![
            iced::widget::text(forge_widgets::tr!("settings_notifications_section_title"))
                .size(FONT_SM)
                .color(palette.text_primary),
            iced::widget::text(forge_widgets::tr!("settings_notifications_hint"))
                .size(FONT_SM)
                .color(palette.text_muted),
        ]
        .spacing(spf(Spacing::Xs)),
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
    pub shortcuts: &'a crate::settings_shortcuts::ShortcutsState,
    pub rt: &'a RuntimeView,
    pub current_language: forge_storage::Language,
    pub current_density: forge_storage::settings::Density,
    pub fonts: &'a crate::ui_settings::FontSettings,
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
        shortcuts,
        rt,
        current_language,
        current_density,
        fonts,
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
        SettingsSection::Version => settings_version_pane(palette),
        SettingsSection::Storage => settings_storage_pane(palette),
        SettingsSection::Queues => settings_queues_pane(palette),
        SettingsSection::Notifications => settings_notifications_pane(palette),
        SettingsSection::Language => settings_language_pane(current_language, palette),
        SettingsSection::Appearance => settings_appearance_pane(current_density, fonts, palette),
        SettingsSection::Shortcuts => crate::settings_shortcuts::view(shortcuts, palette),
        SettingsSection::Hotkeys => crate::settings_hotkeys::view(hotkeys, rt, palette),
        SettingsSection::Scripting => crate::settings_scripting::view(scripting, palette),
    };

    let page_header = forge_widgets::breadcrumb(
        vec![
            forge_widgets::BreadcrumbCrumb::leaf(forge_widgets::tr!("settings_page_title")),
            forge_widgets::BreadcrumbCrumb::leaf(section_label(section)),
        ],
        None,
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
        SettingsMsg::CheckForUpdatesRequested => Task::perform(
            async move {
                tokio::task::spawn_blocking(|| open::that(RELEASES_URL).map_err(|e| e.to_string()))
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r)
            },
            |r| Message::Settings(SettingsMsg::CheckForUpdatesResult(r)),
        ),
        SettingsMsg::CheckForUpdatesResult(result) => {
            if let Err(e) = result {
                tracing::warn!(error = %e, "failed to open releases page");
            }
            Task::none()
        }
        SettingsMsg::Scripting(sub) => {
            crate::settings_scripting::update(&mut app.ui.settings_scripting, &app.rt, sub)
        }
        SettingsMsg::Shortcuts(sub) => {
            crate::settings_shortcuts::update(&mut app.ui.settings_shortcuts, &app.rt, sub)
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
        SettingsMsg::ThemePersisted(result) => {
            if let Err(e) = result {
                tracing::warn!(error = %e, "failed to persist theme selection");
            }
            Task::none()
        }
        SettingsMsg::FontCatalogLoaded(catalog) => {
            app.fonts.catalog = catalog;
            app.fonts.catalog_loaded = true;
            for role in [
                forge_widgets::FontRole::Body,
                forge_widgets::FontRole::Monospace,
            ] {
                if let Some(name) = app.fonts.missing(role) {
                    tracing::warn!(
                        family = name,
                        "stored font family not installed; rendering bundled default"
                    );
                    forge_widgets::install_font_override(role, None);
                }
            }
            Task::none()
        }
        SettingsMsg::FontChanged(role, family) => {
            forge_widgets::install_font_override(role, family.as_deref());
            app.fonts.set_stored(role, family.clone());
            let settings: Arc<dyn SettingsRepo> =
                Arc::clone(&app.rt.backend) as Arc<dyn SettingsRepo>;
            Task::perform(
                async move {
                    match role {
                        forge_widgets::FontRole::Body => settings.set_font_body(family).await,
                        forge_widgets::FontRole::Monospace => settings.set_font_mono(family).await,
                    }
                    .map_err(|e| e.to_string())
                },
                |r| Message::Settings(SettingsMsg::FontPersisted(r)),
            )
        }
        SettingsMsg::FontPersisted(result) => {
            if let Err(e) = result {
                tracing::warn!(error = %e, "failed to persist font selection");
            }
            Task::none()
        }
        SettingsMsg::FontMonoShowAllToggled(show_all) => {
            app.fonts.mono_show_all = show_all;
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
