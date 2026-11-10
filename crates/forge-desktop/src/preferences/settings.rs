mod launch;
mod panes;

use launch::SettingsServices;
pub(crate) use launch::{SettingsAudioLaunch, SettingsLaunch};

use forge_components::FONT_MICRO;
use forge_components::FONT_ROW;
use forge_components::FONT_XXL;
use std::sync::Arc;

use forge_components::OverflowRevealingFocus;
use forge_components::{
    AccentColor, BORDER_THIN, BreadcrumbCrumb, Density, FONT_MD, FONT_SM, FONT_XS, FONT_XXS,
    ForgePalette, Icon, KeyActivated, Picker, PickerEvent, PickerItem, PickerLabels, Radius,
    Spacing, ThemeId, badge, card, dropdown, field_hint, field_label, ghost_button_with_icon, icon,
    page_frame, radius, scaled, setting_block, setting_groups, setting_row, setting_rows,
    setting_sections, spacing, toggle, tr, with_alpha,
};
use forge_runtime::ChatHistoryRetentionHandle;
use forge_storage::settings::core_settings;
use forge_storage::{DataProvider, Language, write_setting};
use forge_transfer::{ScheduledBackupHandle, TransferExclusivity};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, FontWeight, Pixels, Rgba, SharedString,
    Subscription, Window, div, prelude::*, px,
};

use crate::app::async_bridge::{self, ErrorSink, KeyedSnapshotWriters, WriteOutcome};
use crate::app::awake_state::AwakeState;
use crate::app::navigation::SettingsSection;
use crate::app::presentation::{LanguageSinks, LocaleState, apply_language};
use crate::app::window_chrome::{
    ActiveWindowChrome, INTEGRATED_TITLEBAR, WindowChrome, can_switch_live, set_integrated_titlebar,
};
use crate::data_flows::backup_schedule::BackupScheduleView;
use crate::data_flows::problem_report::ProblemReport;
use crate::data_flows::safety_copies::SafetyCopiesView;
use crate::data_flows::update_check::{CheckResult, run_update_check};
use crate::data_flows::update_flow::UpdateFlow;
use crate::preferences::settings_about::SettingsAboutView;
use crate::preferences::settings_audio::SettingsAudioView;
use crate::preferences::settings_catalog::DENSITY;
use crate::preferences::settings_diagnostics::SettingsDiagnosticsView;
use crate::preferences::settings_notifications::SettingsNotificationsView;
use crate::preferences::settings_pane_header::SettingsPaneHeader;
use crate::preferences::settings_region::SettingsRegionView;
use crate::preferences::settings_scripting::SettingsScriptingView;
use crate::preferences::settings_shortcuts::SettingsShortcutsView;
use crate::preferences::settings_storage::SettingsStorageView;
use crate::preferences::settings_update_status::{UpdateCheckStatus, status_line};
use crate::preferences::settings_websocket::SettingsWebSocketView;
use forge_components::{ActivePresentation, Presentation};

const FONT_FIELD_PAD_V: Pixels = px(7.0);

const RELEASES_URL: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/releases");

pub(crate) const NAV_WIDTH: Pixels = px(210.0);
pub(crate) const PANE_MIN_WIDTH: Pixels = px(360.0);

const NAV_PAD_V: Pixels = px(12.0);
const NAV_PAD_H: Pixels = px(8.0);
const NAV_GROUP_LABEL_FONT: Pixels = FONT_MICRO;
const NAV_ROW_PAD_V: Pixels = px(7.0);
const NAV_ROW_MB: Pixels = px(2.0);
const NAV_ROW_FONT: Pixels = FONT_ROW;
const NAV_ROW_ICON: Pixels = px(14.0);
const PANE_PAD_V: Pixels = px(20.0);
const CHOICE_LABEL_GAP: Pixels = px(2.0);
const CHOICE_PAD_V: Pixels = px(8.0);
const CHOICE_ACTIVE_FILL: f32 = 0.12;
const CHOICE_ACTIVE_EDGE: f32 = 0.5;
const ACCENT_SWATCH: Pixels = px(20.0);
const ACCENT_SWATCH_SELECTED: Pixels = px(24.0);
const ACCENT_SWATCH_RING: Pixels = px(2.0);

const NAV_GROUPS: [(&str, &[SettingsSection]); 3] = [
    (
        "settings_nav_group_preferences",
        &[
            SettingsSection::General,
            SettingsSection::Appearance,
            SettingsSection::Language,
            SettingsSection::Shortcuts,
            SettingsSection::Notifications,
        ],
    ),
    (
        "settings_nav_group_engine",
        &[
            SettingsSection::Audio,
            SettingsSection::Scripting,
            SettingsSection::Storage,
            SettingsSection::WebSocket,
        ],
    ),
    (
        "settings_nav_group_about",
        &[SettingsSection::About, SettingsSection::Diagnostics],
    ),
];

impl SettingsSection {
    pub(crate) fn label(self) -> String {
        match self {
            SettingsSection::General => tr!("settings_nav_general"),
            SettingsSection::Appearance => tr!("settings_nav_appearance"),
            SettingsSection::Language => tr!("settings_nav_language_region"),
            SettingsSection::Shortcuts => tr!("settings_nav_shortcuts"),
            SettingsSection::Notifications => tr!("settings_nav_notifications"),
            SettingsSection::Audio => tr!("settings_nav_audio"),
            SettingsSection::Scripting => tr!("settings_scripting_title"),
            SettingsSection::Storage => tr!("settings_storage_section_title"),
            SettingsSection::WebSocket => tr!("settings_ws_title"),
            SettingsSection::About => tr!("settings_about_title"),
            SettingsSection::Diagnostics => tr!("settings_diagnostics_section_title"),
        }
    }

    pub(crate) fn icon(self) -> Icon {
        match self {
            SettingsSection::General => Icon::Settings,
            SettingsSection::Appearance => Icon::Palette,
            SettingsSection::Language => Icon::Language,
            SettingsSection::Shortcuts => Icon::Keyboard,
            SettingsSection::Notifications => Icon::Bell,
            SettingsSection::Audio => Icon::Volume,
            SettingsSection::Scripting => Icon::Code,
            SettingsSection::Storage => Icon::Database,
            SettingsSection::WebSocket => Icon::Network,
            SettingsSection::About => Icon::InfoCircle,
            SettingsSection::Diagnostics => Icon::Bug,
        }
    }
}

fn theme_meta(theme: ThemeId) -> (String, String) {
    match theme {
        ThemeId::ForgeDefault => (
            "Forge Default".to_owned(),
            tr!("settings_theme_desc_violet_dark"),
        ),
        ThemeId::TokyoNight => ("Tokyo Night".to_owned(), tr!("settings_theme_desc_storm")),
        ThemeId::Latte => (
            "Catppuccin Latte".to_owned(),
            tr!("settings_theme_desc_light_mode"),
        ),
    }
}

fn density_meta(density: Density) -> (String, String) {
    match density {
        Density::Compact => (
            tr!("settings_appearance_density_compact"),
            tr!("settings_appearance_density_compact_hint"),
        ),
        Density::Cozy => (
            tr!("settings_appearance_density_cozy"),
            tr!("settings_appearance_density_cozy_hint"),
        ),
        Density::Spacious => (
            tr!("settings_appearance_density_spacious"),
            tr!("settings_appearance_density_spacious_hint"),
        ),
    }
}

pub(crate) struct SettingsView {
    section: SettingsSection,
    services: SettingsServices,
    awake: Entity<AwakeState>,
    stay_awake_pending: Option<bool>,
    integrated_titlebar_pending: Option<bool>,
    update_flow: Entity<UpdateFlow>,
    update_check: UpdateCheckStatus,
    language: Language,
    about: Entity<SettingsAboutView>,
    audio: Entity<SettingsAudioView>,
    scripting: Entity<SettingsScriptingView>,
    websocket: Entity<SettingsWebSocketView>,
    shortcuts: Entity<SettingsShortcutsView>,
    notifications: Entity<SettingsNotificationsView>,
    storage: Entity<SettingsStorageView>,
    diagnostics: Entity<SettingsDiagnosticsView>,
    appearance_writers: KeyedSnapshotWriters<&'static str>,
    region: Entity<SettingsRegionView>,
    font_picker: Option<FontPicker>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FontTarget {
    Body,
    Mono,
}

struct FontPicker {
    picker: Entity<Picker>,
    target: FontTarget,
    _sub: Subscription,
}

const FONT_DEFAULT_ID: &str = "__forge_default_font__";

impl SettingsView {
    fn follow_awake(awake: &Entity<AwakeState>, cx: &mut Context<Self>) {
        cx.observe(awake, |this: &mut Self, awake, cx| {
            if this.stay_awake_pending == awake.read(cx).enabled() {
                this.stay_awake_pending = None;
            }
            cx.notify();
        })
        .detach();
    }

    fn storage_view(
        backend: &Arc<dyn DataProvider>,
        rt_handle: &tokio::runtime::Handle,
        chat_history_retention: ChatHistoryRetentionHandle,
        scheduled_backups: &ScheduledBackupHandle,
        transfer_exclusivity: &TransferExclusivity,
        cx: &mut Context<Self>,
    ) -> Entity<SettingsStorageView> {
        let schedule = cx.new(|cx| {
            BackupScheduleView::new(
                Arc::clone(backend),
                rt_handle.clone(),
                scheduled_backups.clone(),
                transfer_exclusivity.clone(),
                crate::app::app_paths::app_paths().data_dir(),
                cx,
            )
        });
        let safety_copies = cx.new(|cx| {
            SafetyCopiesView::new(
                Arc::clone(backend),
                rt_handle.clone(),
                transfer_exclusivity.clone(),
                scheduled_backups.clone(),
                crate::app::app_paths::app_paths().data_dir(),
                cx,
            )
        });
        cx.new(|cx| {
            let mut storage = SettingsStorageView::new(
                Arc::clone(backend),
                rt_handle.clone(),
                chat_history_retention,
                cx,
            );
            storage.attach_schedule(schedule, cx);
            storage.attach_safety_copies(safety_copies, cx);
            storage.follow_transfer_lock(transfer_exclusivity, cx);
            storage
        })
    }

    fn region_view(
        backend: &Arc<dyn DataProvider>,
        rt_handle: &tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Entity<SettingsRegionView> {
        cx.new(|cx| {
            SettingsRegionView::new(
                backend.settings_repo(),
                rt_handle.clone(),
                cx.global::<LocaleState>().formats,
                cx,
            )
        })
    }

    fn about_view(
        backend: &Arc<dyn DataProvider>,
        rt_handle: &tokio::runtime::Handle,
        problem_report: Entity<ProblemReport>,
        section: SettingsSection,
        cx: &mut Context<Self>,
    ) -> Entity<SettingsAboutView> {
        cx.new(|cx| {
            let mut about =
                SettingsAboutView::new(Arc::clone(backend), rt_handle.clone(), problem_report, cx);
            if section == SettingsSection::About {
                about.begin_collection(cx);
            }
            about
        })
    }

    pub(crate) fn new(
        launch: SettingsLaunch,
        awake: Entity<AwakeState>,
        update_flow: Entity<UpdateFlow>,
        problem_report: Entity<ProblemReport>,
        preselect: Option<SettingsSection>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::follow_awake(&awake, cx);
        let SettingsLaunch {
            services,
            audio,
            log_tail,
            server,
            overlays,
            overlay_samples,
            hotkey_client,
            notification_policy,
            scheduled_backups,
            transfer_exclusivity,
            chat_history_retention,
            builtins,
            boot_warnings,
        } = launch;
        let backend = Arc::clone(&services.backend);
        let rt_handle = services.rt_handle.clone();
        let section = preselect.unwrap_or(NAV_GROUPS[0].1[0]);
        let audio =
            cx.new(|cx| SettingsAudioView::new(audio, section == SettingsSection::Audio, cx));
        let scripting =
            cx.new(|cx| SettingsScriptingView::new(Arc::clone(&backend), rt_handle.clone(), cx));
        let websocket = cx.new(|cx| {
            SettingsWebSocketView::new(
                Arc::clone(&backend),
                rt_handle.clone(),
                server,
                overlays,
                overlay_samples,
                cx,
            )
        });
        let shortcuts = cx.new(|cx| {
            SettingsShortcutsView::new(Arc::clone(&backend), rt_handle.clone(), hotkey_client, cx)
        });
        let notifications = cx.new(|cx| {
            SettingsNotificationsView::new(
                backend.settings_repo(),
                rt_handle.clone(),
                notification_policy,
            )
            .with_backup_failure_notice(cx)
        });
        let storage = Self::storage_view(
            &backend,
            &rt_handle,
            chat_history_retention,
            &scheduled_backups,
            &transfer_exclusivity,
            cx,
        );
        let about = Self::about_view(&backend, &rt_handle, problem_report, section, cx);
        let diagnostics = cx.new(|cx| {
            SettingsDiagnosticsView::new(
                log_tail,
                Arc::clone(&backend),
                builtins,
                rt_handle.clone(),
                section == SettingsSection::Diagnostics,
                cx,
            )
            .with_storage_changes(scheduled_backups)
            .with_boot_warnings(boot_warnings)
        });
        let region = Self::region_view(&backend, &rt_handle, cx);
        let appearance_writers = KeyedSnapshotWriters::new(&rt_handle);
        Self {
            section,
            services,
            awake,
            stay_awake_pending: None,
            integrated_titlebar_pending: None,
            update_flow,
            update_check: UpdateCheckStatus::Idle,
            language: cx.global::<LocaleState>().language,
            about,
            audio,
            scripting,
            websocket,
            shortcuts,
            notifications,
            storage,
            diagnostics,
            appearance_writers,
            region,
            font_picker: None,
        }
    }

    fn stay_awake_enabled(&self, cx: &App) -> bool {
        self.stay_awake_pending
            .or_else(|| self.awake.read(cx).enabled())
            .unwrap_or(crate::app::stay_awake::ENABLED_BY_DEFAULT)
    }

    fn integrated_titlebar_enabled(&self, cx: &App) -> bool {
        self.integrated_titlebar_pending
            .unwrap_or_else(|| cx.integrated_titlebar())
    }

    fn toggle_integrated_titlebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let enabled = !self.integrated_titlebar_enabled(cx);
        self.integrated_titlebar_pending = Some(enabled);
        if can_switch_live() {
            cx.set_global(WindowChrome {
                integrated_titlebar: enabled,
            });
            set_integrated_titlebar(window, enabled);
            self.integrated_titlebar_pending = None;
            cx.refresh_windows();
        }

        let backend = self.services.backend.settings_repo();
        self.appearance_writers.write_reporting(
            "integrated_titlebar",
            async move { write_setting(backend.as_ref(), &INTEGRATED_TITLEBAR, &enabled).await },
            tr!("settings_integrated_titlebar_persist_failed"),
            cx,
        );
        cx.notify();
    }

    fn toggle_stay_awake(&mut self, cx: &mut Context<Self>) {
        let enabled = !self.stay_awake_enabled(cx);
        self.stay_awake_pending = Some(enabled);
        let stay_awake = self.services.stay_awake.clone();
        stay_awake.set_enabled(
            enabled,
            |this: &mut Self, outcome, cx| this.stay_awake_settled(outcome, cx),
            cx,
        );
        cx.notify();
    }

    fn stay_awake_settled(&mut self, outcome: WriteOutcome, cx: &mut Context<Self>) {
        match outcome {
            WriteOutcome::Written => {
                if self.stay_awake_pending == self.awake.read(cx).enabled() {
                    self.stay_awake_pending = None;
                }
            }
            WriteOutcome::Failed(_) => {
                self.stay_awake_pending = None;
                ErrorSink::Toast.report(tr!("settings_stay_awake_persist_failed"), cx);
            }
            WriteOutcome::Superseded => {}
        }
    }

    fn select_section(&mut self, section: SettingsSection, cx: &mut Context<Self>) {
        self.section = section;
        if section == SettingsSection::About {
            self.about.update(cx, SettingsAboutView::begin_collection);
        }
        let diagnostics_open = section == SettingsSection::Diagnostics;
        self.diagnostics.update(cx, |view, cx| {
            view.set_active(diagnostics_open, cx);
        });
        let audio_open = section == SettingsSection::Audio;
        self.audio.update(cx, |view, cx| {
            view.set_active(audio_open, cx);
        });
        let shortcuts_open = section == SettingsSection::Shortcuts;
        self.shortcuts.update(cx, |view, cx| {
            view.set_active(shortcuts_open, cx);
        });
        cx.notify();
    }

    fn open_font_picker(
        &mut self,
        target: FontTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let default_label = match target {
            FontTarget::Body => tr!("settings_appearance_font_default_body"),
            FontTarget::Mono => tr!("settings_appearance_font_default_mono"),
        };
        let mut items = vec![PickerItem {
            id: FONT_DEFAULT_ID.into(),
            label: default_label.into(),
            sublabel: None,
            icon: None,
        }];
        items.extend(
            cx.text_system()
                .all_font_names()
                .into_iter()
                .map(|name| PickerItem {
                    id: name.clone().into(),
                    label: name.into(),
                    sublabel: None,
                    icon: None,
                }),
        );

        let labels = PickerLabels {
            placeholder: tr!("settings_appearance_font_search").into(),
            empty: tr!("widget_picker_no_results").into(),
            loading: tr!("widget_picker_loading").into(),
        };
        let current = match target {
            FontTarget::Body => cx.body_family(),
            FontTarget::Mono => cx.mono_family(),
        };

        let picker = cx.new(|cx| Picker::new(labels, items, cx).with_current(Some(current)));
        let sub = cx.subscribe(&picker, Self::on_font_picker_event);
        picker.update(cx, |f, cx| f.focus(window, cx));
        self.font_picker = Some(FontPicker {
            picker,
            target,
            _sub: sub,
        });
        cx.notify();
    }

    fn on_font_picker_event(
        &mut self,
        _picker: Entity<Picker>,
        event: &PickerEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            PickerEvent::Selected(id) => self.pick_font(id.clone(), cx),
            PickerEvent::Cancelled => self.close_font_picker(cx),
        }
    }

    fn pick_font(&mut self, id: SharedString, cx: &mut Context<Self>) {
        let Some(pending) = self.font_picker.take() else {
            return;
        };
        let family: Option<SharedString> = if id.as_ref() == FONT_DEFAULT_ID {
            None
        } else {
            Some(id)
        };
        cx.update_global::<Presentation, _>(|presentation, _| match pending.target {
            FontTarget::Body => presentation.fonts.set_body_override(family.clone()),
            FontTarget::Mono => presentation.fonts.set_mono_override(family.clone()),
        });

        let backend = self.services.backend.settings_repo();
        let persisted = family.map(|f| f.to_string());
        let target = pending.target;
        let writer_key = match target {
            FontTarget::Body => "font_body",
            FontTarget::Mono => "font_mono",
        };
        self.appearance_writers.write_reporting(
            writer_key,
            async move {
                let setting = match target {
                    FontTarget::Body => &core_settings::FONT_BODY,
                    FontTarget::Mono => &core_settings::FONT_MONO,
                };
                write_setting(backend.as_ref(), setting, &persisted).await
            },
            tr!("settings_appearance_font_persist_failed"),
            cx,
        );

        cx.refresh_windows();
        cx.notify();
    }

    fn close_font_picker(&mut self, cx: &mut Context<Self>) {
        self.font_picker = None;
        cx.notify();
    }

    fn font_field(
        &self,
        label: SharedString,
        target: FontTarget,
        current: SharedString,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let density = cx.density();
        let element_id = match target {
            FontTarget::Body => "font-field-body",
            FontTarget::Mono => "font-field-mono",
        };
        let preview_family = current.clone();
        let value = div()
            .id(element_id)
            .cursor_pointer()
            .flex()
            .items_center()
            .justify_between()
            .px(spacing(Spacing::Sm, density))
            .py(scaled(FONT_FIELD_PAD_V, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.base)
            .border(BORDER_THIN)
            .border_color(palette.border_input)
            .hover(|s| s.border_color(palette.border_active))
            .child(
                div()
                    .font_family(preview_family)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(current),
            )
            .child(icon(Icon::ChevronDown, FONT_XS, palette.text_faint))
            .key_activated(palette.text_primary)
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.open_font_picker(target, window, cx)
            }));
        let popover = self
            .font_picker
            .as_ref()
            .filter(|pending| pending.target == target)
            .map(|pending| {
                let view = cx.entity();
                dropdown(pending.picker.clone()).on_dismiss(move |_window, cx| {
                    view.update(cx, |this, cx| this.close_font_picker(cx));
                })
            });
        let field = div().relative().w_full().child(value).children(popover);
        div()
            .flex_1()
            .min_w(px(0.0))
            .child(field_label(palette, label, field))
    }

    fn select_theme(&mut self, theme: ThemeId, cx: &mut Context<Self>) {
        let density = cx.density();
        let accent = cx.accent();
        let restyled = cx.global::<Presentation>().restyled(theme, accent, density);
        cx.set_global(restyled);

        let backend = self.services.backend.settings_repo();
        let key = theme.storage_key().to_owned();
        self.appearance_writers.write_reporting(
            "theme",
            async move { write_setting(backend.as_ref(), &core_settings::THEME, &Some(key)).await },
            tr!("settings_theme_persist_failed"),
            cx,
        );
        cx.refresh_windows();
        cx.notify();
    }

    fn select_accent(&mut self, accent: AccentColor, cx: &mut Context<Self>) {
        if cx.accent() == accent {
            return;
        }
        let theme = cx.theme();
        let density = cx.density();
        let restyled = cx.global::<Presentation>().restyled(theme, accent, density);
        cx.set_global(restyled);

        let backend = self.services.backend.settings_repo();
        let key = accent.storage_key().to_owned();
        self.appearance_writers.write_reporting(
            "accent",
            async move { write_setting(backend.as_ref(), &core_settings::ACCENT, &Some(key)).await },
            tr!("settings_accent_persist_failed"),
            cx,
        );
        cx.refresh_windows();
        cx.notify();
    }

    fn select_language(&mut self, lang: Language, cx: &mut Context<Self>) {
        if self.language == lang {
            return;
        }
        self.language = lang;
        apply_language(
            lang,
            LanguageSinks {
                anonymous_donor: &self.services.anonymous_donor,
                date_names: &self.services.date_names,
            },
            cx,
        );

        let backend = self.services.backend.settings_repo();
        self.appearance_writers.write_reporting(
            "language",
            async move {
                write_setting(
                    backend.as_ref(),
                    &core_settings::LANGUAGE,
                    &lang.to_string(),
                )
                .await
            },
            tr!("settings_language_persist_failed"),
            cx,
        );

        cx.refresh_windows();
        cx.notify();
    }

    fn select_density(&mut self, density: Density, cx: &mut Context<Self>) {
        let theme = cx.theme();
        let accent = cx.accent();
        let restyled = cx.global::<Presentation>().restyled(theme, accent, density);
        cx.set_global(restyled);

        let backend = self.services.backend.settings_repo();
        let key = density.storage_key().to_owned();
        self.appearance_writers.write_reporting(
            "density",
            async move { write_setting(backend.as_ref(), &DENSITY, &Some(key)).await },
            tr!("settings_density_persist_failed"),
            cx,
        );
        cx.refresh_windows();
        cx.notify();
    }

    fn check_for_updates(&mut self, cx: &mut Context<Self>) {
        if self.update_check.is_checking() {
            return;
        }
        self.update_check = UpdateCheckStatus::Checking;
        let backend = Arc::clone(&self.services.backend);
        let rt_handle = self.services.rt_handle.clone();
        let flow = self.update_flow.clone();
        cx.spawn(async move |this, cx| {
            let result = run_update_check(cx, backend, &rt_handle, None, &flow).await;
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
            let _ = this.update(cx, |view, cx| view.finish_update_check(result, cx));
        })
        .detach();
        cx.notify();
    }

    fn finish_update_check(&mut self, result: CheckResult, cx: &mut Context<Self>) {
        self.update_check = UpdateCheckStatus::from_result(result);
        cx.notify();
    }

    fn open_releases_page(&mut self, cx: &mut Context<Self>) {
        async_bridge::open_external(
            &self.services.rt_handle,
            RELEASES_URL,
            ErrorSink::Toast,
            tr!("settings_check_updates_failed"),
            cx,
        );
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let nav = self.render_nav(&palette, cx);
        let pane = self.render_pane(&palette, density, cx);

        let body = div()
            .w_full()
            .flex_1()
            .flex()
            .flex_row()
            .overflow_hidden()
            .child(nav)
            .child(pane);

        let frame = page_frame(
            vec![
                BreadcrumbCrumb::leaf(tr!("settings_page_title")),
                BreadcrumbCrumb::leaf(self.section.label()),
            ],
            &palette,
        )
        .body(body);

        div().size_full().flex().flex_col().child(frame)
    }
}

fn choice_row(
    id: SharedString,
    label: SharedString,
    detail: AnyElement,
    active: bool,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let labels = div()
        .flex()
        .flex_col()
        .gap(scaled(CHOICE_LABEL_GAP, density))
        .child(
            div()
                .font_family(cx.body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_primary)
                .child(label),
        )
        .child(detail);

    let row = div()
        .id(id)
        .flex()
        .items_center()
        .justify_between()
        .px(spacing(Spacing::Sm, density))
        .py(scaled(CHOICE_PAD_V, density))
        .rounded(radius(Radius::Sm))
        .border(BORDER_THIN)
        .cursor_pointer()
        .child(labels);
    if active {
        row.bg(with_alpha(palette.brand, CHOICE_ACTIVE_FILL))
            .border_color(with_alpha(palette.brand, CHOICE_ACTIVE_EDGE))
            .child(icon(Icon::CircleCheck, FONT_SM, palette.brand))
    } else {
        let hover = palette.elevated;
        row.border_color(gpui::transparent_black())
            .hover(move |style| style.bg(hover))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::navigation::Screen;

    #[test]
    fn every_settings_nav_entry_opens_from_its_route_key() {
        for section in NAV_GROUPS.iter().flat_map(|(_, sections)| sections.iter()) {
            assert_eq!(
                Screen::parse_cli(&format!("settings:{}", section.key())),
                Ok(Screen::Settings(Some(*section))),
                "{section:?}"
            );
        }
    }

    #[test]
    fn the_settings_sidebar_lists_every_section_in_route_order() {
        let sidebar: Vec<SettingsSection> = NAV_GROUPS
            .iter()
            .flat_map(|(_, sections)| sections.iter().copied())
            .collect();

        assert_eq!(sidebar, SettingsSection::ALL.to_vec());
    }
}
