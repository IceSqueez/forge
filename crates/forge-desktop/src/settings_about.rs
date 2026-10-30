use std::sync::Arc;

use forge_audio::{OutputDevice, stored_output_device};
use forge_components::{
    ActivePresentation, FONT_XS, ForgePalette, Icon, SectionLabel, Spacing, body_family, card,
    field_hint, ghost_button_with_icon, setting_rows, setting_value_row, spacing, spinner, tr,
};
use forge_storage::settings::core_settings;
use forge_storage::{DataProvider, SettingsRepo, read_setting};
use gpui::{ClickEvent, Context, Entity, Window, div, prelude::*};
use tokio::runtime::Handle;

use crate::async_bridge;
use crate::problem_report::{ProblemReport, ReportSubject};
use crate::system_info::{HostFacts, InfoField, SystemInfo, WindowFacts};
use crate::toasts::copy_to_clipboard;

enum AboutState {
    NotStarted,
    Collecting,
    Ready(Box<SystemInfo>),
}

pub struct SettingsAboutView {
    backend: Arc<dyn DataProvider>,
    rt_handle: Handle,
    problem_report: Entity<ProblemReport>,
    state: AboutState,
}

impl SettingsAboutView {
    pub fn new(
        backend: Arc<dyn DataProvider>,
        rt_handle: Handle,
        problem_report: Entity<ProblemReport>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&problem_report, |_, _, cx| cx.notify()).detach();
        Self {
            backend,
            rt_handle,
            problem_report,
            state: AboutState::NotStarted,
        }
    }

    fn report_problem(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.problem_report.update(cx, |report, cx| {
            report.start(ReportSubject::Problem, window, cx)
        });
    }

    fn start_collection(&mut self, window: &Window, cx: &mut Context<Self>) {
        if !matches!(self.state, AboutState::NotStarted) {
            return;
        }
        self.state = AboutState::Collecting;
        let window_facts = WindowFacts::read(window, cx);
        let settings = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                match read_setting(settings.as_ref(), &core_settings::AUDIO_OUTPUT_DEVICE_ID).await
                {
                    Ok(stored_id) => Some(stored_output_device(stored_id)),
                    Err(e) => {
                        tracing::warn!(error = %e, "failed to read audio output device for system info");
                        None
                    }
                }
            },
            move |this, speech_preference, cx| {
                this.collect_host_facts(window_facts, speech_preference, cx)
            },
            cx,
        );
    }

    fn collect_host_facts(
        &mut self,
        window_facts: WindowFacts,
        speech_preference: Option<OutputDevice>,
        cx: &mut Context<Self>,
    ) {
        let data_dir = forge_paths::data_dir();
        async_bridge::run_blocking(
            &self.rt_handle,
            move || HostFacts::collect(&data_dir, speech_preference),
            move |this, host, cx| this.finish_collection(window_facts, host, cx),
            cx,
        );
    }

    fn finish_collection(
        &mut self,
        window_facts: WindowFacts,
        host: HostFacts,
        cx: &mut Context<Self>,
    ) {
        let info = SystemInfo::assemble(window_facts, host);
        crate::crash_hook::remember_system_info(info.to_report_text());
        self.state = AboutState::Ready(Box::new(info));
        cx.notify();
    }

    fn copy_report(&mut self, cx: &mut Context<Self>) {
        if let AboutState::Ready(info) = &self.state {
            copy_to_clipboard(info.to_report_text(), cx);
        }
    }

    fn render_rows(&self, palette: &ForgePalette, cx: &Context<Self>) -> gpui::AnyElement {
        let density = cx.density();
        let AboutState::Ready(info) = &self.state else {
            return div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xs, density))
                .py(spacing(Spacing::Xs, density))
                .child(spinner(
                    "settings-about-collecting",
                    Icon::Refresh,
                    FONT_XS,
                    palette.text_muted,
                ))
                .child(
                    div()
                        .font_family(body_family())
                        .text_size(FONT_XS)
                        .text_color(palette.text_muted)
                        .child(tr!("settings_about_collecting")),
                )
                .into_any_element();
        };
        InfoField::ALL
            .iter()
            .fold(setting_rows(palette), |rows, field| {
                let value = info
                    .value(*field)
                    .unwrap_or_else(|| tr!("settings_about_unknown"));
                rows.row(setting_value_row(
                    field_label(*field),
                    value,
                    palette,
                    density,
                ))
            })
            .into_any_element()
    }
}

fn field_label(field: InfoField) -> String {
    match field {
        InfoField::Forge => tr!("settings_about_field_forge"),
        InfoField::Build => tr!("settings_about_field_build"),
        InfoField::Os => tr!("settings_about_field_os"),
        InfoField::Kernel => tr!("settings_about_field_kernel"),
        InfoField::Cpu => tr!("settings_about_field_cpu"),
        InfoField::Memory => tr!("settings_about_field_memory"),
        InfoField::Gpu => tr!("settings_about_field_gpu"),
        InfoField::GpuBackend => tr!("settings_about_field_gpu_backend"),
        InfoField::Displays => tr!("settings_about_field_displays"),
        InfoField::AudioOutputs => tr!("settings_about_field_audio_outputs"),
        InfoField::DataDir => tr!("settings_about_field_data_dir"),
    }
}

impl Render for SettingsAboutView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if matches!(self.state, AboutState::NotStarted) {
            cx.defer_in(window, |this, window, cx| this.start_collection(window, cx));
        }
        let palette = cx.palette();
        let density = cx.density();
        let ready = matches!(self.state, AboutState::Ready(_));

        let copy = ghost_button_with_icon(Icon::Copy, tr!("common_copy"), &palette)
            .disabled(!ready)
            .on_click(
                "settings-about-copy",
                cx.listener(|this, _: &ClickEvent, _, cx| this.copy_report(cx)),
            );
        let report =
            ghost_button_with_icon(Icon::Bug, tr!("settings_about_report_problem"), &palette)
                .busy(self.problem_report.read(cx).is_busy())
                .on_click(
                    "settings-about-report-problem",
                    cx.listener(|this, _: &ClickEvent, window, cx| this.report_problem(window, cx)),
                );
        let actions = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(report)
            .child(copy);
        let header = SectionLabel::new(tr!("settings_about_system_title"), &palette)
            .row()
            .trailing(Some(actions.into_any_element()));

        let body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(header)
            .child(field_hint(tr!("settings_about_system_hint"), &palette))
            .child(self.render_rows(&palette, cx));

        card(body, &palette).full_width()
    }
}
