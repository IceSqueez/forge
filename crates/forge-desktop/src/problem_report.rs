use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use forge_audio::stored_output_device;
use forge_components::{
    ActivePresentation, Density, FONT_SM, FONT_XS, ForgePalette, Icon, ModalSize, OverlayPosition,
    PushToast, Spacing, ToastAction, ToastKind, body_family, card, ghost_button, modal,
    mono_family, overlay, primary_button, spacing, tr,
};
use forge_storage::settings::core_settings::AUDIO_OUTPUT_DEVICE_ID;
use forge_storage::{
    DEFAULT_DIAGNOSTIC_LOG_LEVEL, DataProvider, SettingsRepo, diagnostic_log_level, read_setting,
};
use forge_types::LogLevel;
use gpui::{
    AnyElement, App, AsyncApp, ClickEvent, Context, Entity, Pixels, Window, div, prelude::*, px,
};
use time::OffsetDateTime;
use tokio::runtime::Handle;

use crate::async_bridge::{self, detached, open_path};
use crate::bundle_preview::{export_statement, statement_row};
use crate::crash_record::{self, CrashRecord, PendingCrash};
use crate::diagnostic_bundle::{self, BUNDLE_FILE_NAME, Bundle};
use crate::integrations::BuiltinRegistry;
use crate::issue_link::{
    CRASH_REPORT_FILE_NAME, ISSUE_URL_MAX_BYTES, IssueForm, SYSTEM_INFO_FILE_NAME,
};
use crate::repo_failure::RepoFailure;
use crate::system_info::{HostFacts, InfoField, SystemInfo, WindowFacts};
use crate::update_check::REPO_URL;

const REPORT_FOLDER_PREFIX: &str = "report-";
const PREVIEW_WIDTH: Pixels = px(600.0);
const ISSUE_TEXT_MAX_HEIGHT: Pixels = px(220.0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportSubject {
    Crash(Box<PendingCrash>),
    Problem,
}

impl ReportSubject {
    fn crash(&self) -> Option<&CrashRecord> {
        match self {
            ReportSubject::Crash(pending) => Some(&pending.record),
            ReportSubject::Problem => None,
        }
    }
}

pub struct ReportDraft {
    pub subject: ReportSubject,
    pub bundle: Bundle,
    pub level: LogLevel,
    pub system_info: String,
    pub form: IssueForm,
}

impl ReportDraft {
    pub fn new(subject: ReportSubject, bundle: Bundle, level: LogLevel, info: &SystemInfo) -> Self {
        let system_info = info.to_report_text();
        let form = match &subject {
            ReportSubject::Crash(pending) => IssueForm::crash(&pending.record, &system_info),
            ReportSubject::Problem => IssueForm::problem(
                info.value(InfoField::Forge).unwrap_or_default(),
                &system_info,
            ),
        }
        .fitted(REPO_URL, ISSUE_URL_MAX_BYTES);
        Self {
            subject,
            bundle,
            level,
            system_info,
            form,
        }
    }
}

enum ReportStage {
    Idle,
    Preparing,
    Previewing {
        draft: Box<ReportDraft>,
        saving: bool,
    },
}

pub struct ProblemReport {
    backend: Arc<dyn DataProvider>,
    builtins: BuiltinRegistry,
    rt_handle: Handle,
    stage: ReportStage,
}

impl ProblemReport {
    pub fn new(
        backend: Arc<dyn DataProvider>,
        builtins: BuiltinRegistry,
        rt_handle: Handle,
    ) -> Self {
        Self {
            backend,
            builtins,
            rt_handle,
            stage: ReportStage::Idle,
        }
    }

    pub fn is_busy(&self) -> bool {
        !matches!(self.stage, ReportStage::Idle)
    }

    pub fn start(&mut self, subject: ReportSubject, window: &Window, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        self.stage = ReportStage::Preparing;
        let window_facts = WindowFacts::read(window, cx);
        let backend = Arc::clone(&self.backend);
        let integrations = diagnostic_bundle::integration_facts(&self.builtins);
        let env_overridden = crate::log_level::env_overridden();
        async_bridge::run_async(
            &self.rt_handle,
            gather(backend, integrations, env_overridden),
            move |this, gathered, cx| this.finish_preparing(subject, window_facts, gathered, cx),
            cx,
        );
        cx.notify();
    }

    fn finish_preparing(
        &mut self,
        subject: ReportSubject,
        window_facts: WindowFacts,
        gathered: Result<Gathered, RepoFailure>,
        cx: &mut Context<Self>,
    ) {
        match gathered {
            Ok(Gathered {
                bundle,
                host,
                level,
            }) => {
                let info = SystemInfo::assemble(window_facts, host);
                crate::crash_hook::remember_system_info(info.to_report_text());
                let draft = ReportDraft::new(subject, bundle, level, &info);
                self.stage = ReportStage::Previewing {
                    draft: Box::new(draft),
                    saving: false,
                };
            }
            Err(error) => {
                self.stage = ReportStage::Idle;
                cx.push_toast(
                    ToastKind::Error,
                    tr!("problem_report_prepare_failed", error = error.text()),
                );
                self.reoffer_crash(subject, cx);
            }
        }
        cx.notify();
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        let stage = std::mem::replace(&mut self.stage, ReportStage::Idle);
        match stage {
            ReportStage::Previewing {
                saving: true,
                draft,
            } => {
                self.stage = ReportStage::Previewing {
                    draft,
                    saving: true,
                };
                return;
            }
            ReportStage::Previewing { draft, .. } => self.reoffer_crash(draft.subject, cx),
            ReportStage::Idle | ReportStage::Preparing => {}
        }
        cx.notify();
    }

    fn reoffer_crash(&self, subject: ReportSubject, cx: &mut Context<Self>) {
        if let ReportSubject::Crash(pending) = subject {
            raise_crash_toast(*pending, cx.entity(), self.rt_handle.clone(), cx);
        }
    }

    fn confirm(&mut self, cx: &mut Context<Self>) {
        let ReportStage::Previewing { draft, saving } = &mut self.stage else {
            return;
        };
        if *saving {
            return;
        }
        *saving = true;
        let contents = ReportFolderContents {
            bundle_text: draft.bundle.text.clone(),
            system_info: draft.system_info.clone(),
            crash_text: draft.subject.crash().map(CrashRecord::to_text),
        };
        let reports_dir = crate::app_paths::app_paths().problem_reports_dir();
        let folder_name = report_folder_name(OffsetDateTime::now_utc());
        async_bridge::run_blocking(
            &self.rt_handle,
            move || write_report_folder(&reports_dir.join(folder_name), &contents),
            |this, written, cx| this.finish_saving(written, cx),
            cx,
        );
        cx.notify();
    }

    fn finish_saving(&mut self, written: io::Result<PathBuf>, cx: &mut Context<Self>) {
        let stage = std::mem::replace(&mut self.stage, ReportStage::Idle);
        let ReportStage::Previewing { draft, .. } = stage else {
            return;
        };
        match written {
            Ok(folder) => {
                let url = draft.form.url(REPO_URL);
                tracing::info!(bytes = url.len(), "opening the prefilled issue page");
                cx.reveal_path(&folder);
                detached(&self.rt_handle, "open the issue page", open_path(url));
                if draft.subject.crash().is_some() {
                    mark_crashes_handled(&self.rt_handle);
                }
                cx.push_toast_full(
                    ToastKind::Success,
                    tr!("problem_report_saved"),
                    None,
                    None,
                    Duration::ZERO,
                );
            }
            Err(error) => {
                tracing::warn!(error = %error, "problem report files could not be written");
                cx.push_toast(
                    ToastKind::Error,
                    tr!(
                        "problem_report_save_failed",
                        error = RepoFailure::from(error).text()
                    ),
                );
                self.reoffer_crash(draft.subject, cx);
            }
        }
        cx.notify();
    }

    fn preview(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let ReportStage::Previewing { draft, saving } = &self.stage else {
            return None;
        };
        let crash = draft.subject.crash().is_some();
        let (title, lead) = if crash {
            (
                tr!("problem_report_crash_title"),
                tr!("problem_report_crash_lead"),
            )
        } else {
            (tr!("problem_report_title"), tr!("problem_report_lead"))
        };

        let mut body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(paragraph(lead, palette.text_primary, FONT_SM))
            .child(paragraph(
                tr!("problem_report_files_note"),
                palette.text_secondary,
                FONT_XS,
            ));
        for note in export_statement(&draft.bundle, &draft.level) {
            body = body.child(statement_row(note, palette, density));
        }
        body = body
            .child(paragraph(
                tr!("problem_report_issue_note"),
                palette.text_secondary,
                FONT_XS,
            ))
            .child(
                card(
                    div()
                        .id("problem-report-issue-text")
                        .max_h(ISSUE_TEXT_MAX_HEIGHT)
                        .overflow_y_scroll()
                        .font_family(mono_family())
                        .text_size(FONT_XS)
                        .text_color(palette.text_secondary)
                        .child(draft.form.preview_text()),
                    palette,
                )
                .full_width(),
            );

        let footer = div()
            .flex()
            .items_center()
            .justify_end()
            .gap(spacing(Spacing::Xs, density))
            .child(
                ghost_button(tr!("common_cancel"), palette)
                    .density(density)
                    .disabled(*saving)
                    .on_click(
                        "problem-report-cancel",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
                    ),
            )
            .child(
                primary_button(tr!("problem_report_confirm"), palette)
                    .density(density)
                    .busy(*saving)
                    .on_click(
                        "problem-report-confirm",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.confirm(cx)),
                    ),
            );

        let card = modal(title, body, palette)
            .header_icon(
                Icon::Bug,
                if crash { palette.danger } else { palette.brand },
            )
            .size(ModalSize::Lg)
            .width(PREVIEW_WIDTH)
            .footer(footer)
            .on_close(
                "problem-report-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            );

        let weak = cx.entity().downgrade();
        Some(
            overlay(card, palette)
                .position(OverlayPosition::Center)
                .on_dismiss("problem-report-dismiss", move |_window, cx| {
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "the view may close before the result arrives"
                    )]
                    let _ = weak.update(cx, |this, cx| this.cancel(cx));
                })
                .into_any_element(),
        )
    }
}

impl Render for ProblemReport {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        match self.preview(&palette, density, cx) {
            Some(preview) => div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .child(preview)
                .into_any_element(),
            None => div().into_any_element(),
        }
    }
}

fn paragraph(text: String, color: gpui::Rgba, size: Pixels) -> impl IntoElement {
    div()
        .font_family(body_family())
        .text_size(size)
        .text_color(color)
        .child(text)
}

struct Gathered {
    bundle: Bundle,
    host: HostFacts,
    level: LogLevel,
}

async fn gather(
    backend: Arc<dyn DataProvider>,
    integrations: Vec<diagnostic_bundle::IntegrationFacts>,
    env_overridden: bool,
) -> Result<Gathered, RepoFailure> {
    let settings = Arc::clone(&backend) as Arc<dyn SettingsRepo>;
    let level = diagnostic_log_level(settings.as_ref())
        .await
        .unwrap_or(DEFAULT_DIAGNOSTIC_LOG_LEVEL);
    let speech_preference = read_setting(settings.as_ref(), &AUDIO_OUTPUT_DEVICE_ID)
        .await
        .ok()
        .map(stored_output_device);
    let input =
        diagnostic_bundle::gather_input(backend, integrations, level.clone(), env_overridden)
            .await?;
    let data_dir = crate::app_paths::app_paths().data_dir();
    tokio::task::spawn_blocking(move || {
        let host = HostFacts::collect(&data_dir, speech_preference);
        let mut bundle = diagnostic_bundle::assemble(&input)?;
        bundle.text = host.identity.scrub(&bundle.text);
        Ok(Gathered {
            bundle,
            host,
            level,
        })
    })
    .await
    .map_err(RepoFailure::from)?
}

pub struct ReportFolderContents {
    pub bundle_text: String,
    pub system_info: String,
    pub crash_text: Option<String>,
}

pub fn report_folder_name(now: OffsetDateTime) -> String {
    format!(
        "{REPORT_FOLDER_PREFIX}{:04}{:02}{:02}-{:02}{:02}{:02}",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

pub fn write_report_folder(folder: &Path, contents: &ReportFolderContents) -> io::Result<PathBuf> {
    fs::create_dir_all(folder)?;
    fs::write(folder.join(BUNDLE_FILE_NAME), &contents.bundle_text)?;
    fs::write(folder.join(SYSTEM_INFO_FILE_NAME), &contents.system_info)?;
    if let Some(crash_text) = &contents.crash_text {
        fs::write(folder.join(CRASH_REPORT_FILE_NAME), crash_text)?;
    }
    Ok(folder.to_path_buf())
}

fn mark_crashes_handled(rt_handle: &Handle) {
    let crashes_dir = crate::app_paths::app_paths().crashes_dir();
    let process_id = std::process::id();
    rt_handle.spawn_blocking(move || {
        if let Err(error) = crash_record::mark_handled(&crashes_dir, process_id) {
            tracing::warn!(error = %error, "crash files could not be marked handled");
        }
    });
}

pub fn start_crash_notice(cx: &mut AsyncApp, report: Entity<ProblemReport>, rt_handle: Handle) {
    let crashes_dir = crate::app_paths::app_paths().crashes_dir();
    let process_id = std::process::id();
    let pending =
        rt_handle.spawn_blocking(move || crash_record::latest_pending(&crashes_dir, process_id));
    cx.spawn(async move |cx| {
        let Ok(Some(pending)) = pending.await else {
            return;
        };
        tracing::info!(crashed_at = %pending.record.occurred_at_utc, "unreported crash found");
        cx.update(|cx| raise_crash_toast(pending, report, rt_handle, cx));
    })
    .detach();
}

fn raise_crash_toast(
    pending: PendingCrash,
    report: Entity<ProblemReport>,
    rt_handle: Handle,
    cx: &mut App,
) {
    let report_action =
        ToastAction::new(tr!("crash_notice_report"), move |window, cx: &mut App| {
            let subject = ReportSubject::Crash(Box::new(pending.clone()));
            report.update(cx, |report, cx| report.start(subject, window, cx));
        });
    let dismiss_action = ToastAction::new(
        tr!("crash_notice_dismiss"),
        move |_window, _cx: &mut App| {
            mark_crashes_handled(&rt_handle);
        },
    );
    cx.push_toast_with_secondary(
        ToastKind::Error,
        tr!("crash_notice"),
        Some(Icon::Bug),
        report_action,
        dismiss_action,
        Duration::ZERO,
    );
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use forge_types::LogLevel;
    use time::{Date, Month, OffsetDateTime, Time};

    use super::{
        ReportDraft, ReportFolderContents, ReportSubject, SYSTEM_INFO_FILE_NAME,
        report_folder_name, write_report_folder,
    };
    use crate::crash_record::{CrashRecord, PendingCrash};
    use crate::diagnostic_bundle::{BUNDLE_FILE_NAME, Bundle};
    use crate::issue_link::{CRASH_REPORT_FILE_NAME, FIELD_VERSION, ISSUE_URL_MAX_BYTES};
    use crate::system_info::{HostFacts, IdentityScrubber, SystemInfo, WindowFacts};
    use crate::update_check::REPO_URL;

    fn contents(crash_text: Option<&str>) -> ReportFolderContents {
        ReportFolderContents {
            bundle_text: "bundle".to_owned(),
            system_info: "OS: Arch Linux".to_owned(),
            crash_text: crash_text.map(str::to_owned),
        }
    }

    fn system_info() -> SystemInfo {
        SystemInfo {
            forge_version: "0.6.2".to_owned(),
            build_commit: "abc1234".to_owned(),
            release_build: true,
            window: WindowFacts {
                gpu: None,
                displays: Vec::new(),
                scale_factor: None,
            },
            host: HostFacts {
                install: None,
                os_name: None,
                os_version: None,
                kernel: None,
                cpu_model: None,
                logical_cores: None,
                total_memory_bytes: None,
                audio_outputs: None,
                data_dir_bytes: None,
                identity: IdentityScrubber::default(),
            },
        }
    }

    fn bundle() -> Bundle {
        Bundle {
            text: "bundle".to_owned(),
            script_lines: 0,
            command_lines: 0,
            elided_bytes: 0,
        }
    }

    fn sorted_names(folder: &std::path::Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(folder)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn report_folder_name_is_a_zero_padded_utc_timestamp() {
        let now = OffsetDateTime::new_utc(
            Date::from_calendar_date(2026, Month::January, 2).unwrap(),
            Time::from_hms(3, 4, 5).unwrap(),
        );
        assert_eq!(report_folder_name(now), "report-20260102-030405");
    }

    #[test]
    fn crash_report_folder_holds_bundle_system_info_and_crash_text() {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("reports").join("report-1");
        let written = write_report_folder(&folder, &contents(Some("crash text"))).unwrap();
        assert_eq!(written, folder);
        assert_eq!(
            fs::read_to_string(folder.join(CRASH_REPORT_FILE_NAME)).unwrap(),
            "crash text"
        );
        let mut expected = vec![
            BUNDLE_FILE_NAME.to_owned(),
            CRASH_REPORT_FILE_NAME.to_owned(),
            SYSTEM_INFO_FILE_NAME.to_owned(),
        ];
        expected.sort();
        assert_eq!(sorted_names(&folder), expected);
    }

    #[test]
    fn problem_report_folder_has_no_crash_text() {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("report-1");
        write_report_folder(&folder, &contents(None)).unwrap();
        let mut expected = vec![
            BUNDLE_FILE_NAME.to_owned(),
            SYSTEM_INFO_FILE_NAME.to_owned(),
        ];
        expected.sort();
        assert_eq!(sorted_names(&folder), expected);
    }

    #[test]
    fn write_report_folder_fails_when_the_folder_path_is_a_file() {
        let temp = tempfile::tempdir().unwrap();
        let blocked: PathBuf = temp.path().join("report-1");
        fs::write(&blocked, "not a folder").unwrap();
        assert!(write_report_folder(&blocked, &contents(None)).is_err());
    }

    #[test]
    fn crash_draft_fits_an_oversized_backtrace_into_the_issue_url() {
        let backtrace = (0..2000)
            .map(|frame| format!("{frame:>4}: forge::module_{frame}::function_{frame}"))
            .collect::<Vec<_>>()
            .join("\n");
        let pending = PendingCrash {
            path: PathBuf::from("crash.json"),
            record: CrashRecord {
                message: "boom".to_owned(),
                location: None,
                thread: None,
                backtrace,
                forge_version: "0.6.2".to_owned(),
                build_commit: "abc1234".to_owned(),
                occurred_at_utc: "2026-10-16T10:00:00Z".to_owned(),
                process_id: 7,
                system_info: None,
            },
        };
        let draft = ReportDraft::new(
            ReportSubject::Crash(Box::new(pending)),
            bundle(),
            LogLevel::Info,
            &system_info(),
        );
        assert!(draft.form.url(REPO_URL).len() <= ISSUE_URL_MAX_BYTES);
    }

    #[test]
    fn problem_draft_prefills_the_running_forge_version() {
        let draft = ReportDraft::new(
            ReportSubject::Problem,
            bundle(),
            LogLevel::Info,
            &system_info(),
        );
        let version = draft
            .form
            .fields
            .iter()
            .find(|field| field.id == FIELD_VERSION)
            .unwrap();
        assert_eq!(version.value, "v0.6.2 (abc1234)");
    }
}
