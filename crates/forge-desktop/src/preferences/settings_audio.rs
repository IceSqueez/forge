use std::sync::Arc;

use forge_audio::{AudioSink, DeviceInfo, DeviceSink, stored_output_device};
use forge_components::{
    ActivePresentation, Density, FONT_SM, ForgePalette, Icon, Spacing, drive_overlay_focus,
    ghost_button_with_icon, icon_button, setting_block, setting_choice_trigger, setting_groups,
    setting_row, setting_rows, spacing, tr,
};
use forge_speech_types::PcmBuffer;
use forge_storage::settings::core_settings;
use forge_storage::{DataProvider, SettingsRepo, read_setting, write_setting};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, FocusHandle, Window, div, prelude::*, px,
};

use crate::app::async_bridge::{self, Loadable};
use crate::app::navigation::SettingsSection;
use crate::audio::device_choice::{
    DeviceDirection, DevicePickerHandlers, DevicePickerIds, device_id_at, device_index,
    device_picker_overlay, enumerate_devices, selected_device_label,
};
use crate::preferences::settings::SettingsAudioLaunch;
use crate::preferences::settings_audio_routing::SettingsAudioRoutingView;
use crate::preferences::settings_pane_header::SettingsPaneHeader;
use crate::preferences::settings_voice_gate::SettingsVoiceGateView;
use crate::shared::load_notice::{LoadNotice, NoticeWidth, load_notice};
use crate::shared::repo_failure::RepoFailure;

const TEST_TONE_SAMPLE_RATE: u32 = 22_050;

const TEST_TONE_DURATION_MS: u32 = 200;

const TEST_TONE_FREQ_HZ: f32 = 440.0;

const TEST_TONE_AMPLITUDE: f32 = 0.35;

const TEST_TONE_CHANNELS: u16 = 1;

const MILLIS_PER_SEC: u32 = 1_000;

pub(crate) struct SettingsAudioView {
    backend: Arc<dyn DataProvider>,
    rt_handle: tokio::runtime::Handle,
    speech_output: Arc<DeviceSink>,
    devices: Vec<DeviceInfo>,
    selected_idx: usize,
    selected_id: Option<String>,
    devices_load: Loadable<()>,
    persist_error: Option<String>,
    test_playing: bool,
    test_error: Option<String>,
    picker_open: bool,
    overlay_focus: FocusHandle,
    focus_restore: Option<FocusHandle>,
    devices_gen: async_bridge::Generation,
    routing: Entity<SettingsAudioRoutingView>,
    voice_gate: Entity<SettingsVoiceGateView>,
}

impl SettingsAudioView {
    pub(crate) fn new(
        launch: SettingsAudioLaunch,
        initial_active: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let SettingsAudioLaunch {
            backend,
            rt_handle,
            speech_output,
            overlays,
            audio_router,
            server_available,
            voice_gate,
        } = launch;
        let routing = cx.new(|cx| {
            SettingsAudioRoutingView::new(
                backend.settings_repo(),
                backend.overlay_repo(),
                overlays,
                rt_handle.clone(),
                audio_router,
                server_available,
                cx,
            )
        });
        let voice_gate = cx.new(|cx| {
            SettingsVoiceGateView::new(
                Arc::clone(&backend),
                rt_handle.clone(),
                voice_gate,
                initial_active,
                cx,
            )
        });
        let mut view = Self {
            backend,
            rt_handle,
            speech_output,
            devices: Vec::new(),
            selected_idx: 0,
            selected_id: None,
            devices_load: Loadable::Loading,
            persist_error: None,
            test_playing: false,
            test_error: None,
            picker_open: false,
            overlay_focus: cx.focus_handle(),
            focus_restore: None,
            devices_gen: async_bridge::Generation::default(),
            routing,
            voice_gate,
        };
        view.load_devices(false, cx);
        view
    }

    pub(crate) fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        self.voice_gate
            .update(cx, |view, cx| view.set_active(active, cx));
    }

    fn load_devices(&mut self, uncached: bool, cx: &mut Context<Self>) {
        self.devices_load = Loadable::Loading;
        let ticket = self.devices_gen.next();
        let settings = self.backend.settings_repo();
        async_bridge::run_async(
            &self.rt_handle,
            enumerate_devices_and_preference(settings, uncached),
            move |this, result, cx| {
                if this.devices_gen.is_current(ticket) {
                    this.apply_devices_loaded(result, cx);
                }
            },
            cx,
        );
        cx.notify();
    }

    fn apply_devices_loaded(
        &mut self,
        result: Result<(Vec<DeviceInfo>, Option<String>), RepoFailure>,
        cx: &mut Context<Self>,
    ) {
        self.devices_load = Loadable::settled(result.as_ref().map(|_| ()).map_err(Clone::clone));
        match result {
            Ok((devices, persisted)) => {
                self.devices = devices;
                let want = self.selected_id.clone().or(persisted);
                self.selected_idx = resolve_selected_idx(&self.devices, want.as_deref());
                self.selected_id = device_id_at(&self.devices, self.selected_idx);
            }
            Err(message) => {
                tracing::warn!(error = %message, "failed to enumerate audio output devices");
            }
        }
        cx.notify();
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.load_devices(true, cx);
    }

    fn select_device(&mut self, idx: usize, cx: &mut Context<Self>) {
        self.selected_idx = idx;
        self.selected_id = device_id_at(&self.devices, idx);
        self.persist_error = None;
        self.picker_open = false;
        self.speech_output
            .device_handle()
            .swap(stored_output_device(self.selected_id.clone()));
        let settings = self.backend.settings_repo();
        let device_id = self.selected_id.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                write_setting(
                    settings.as_ref(),
                    &core_settings::AUDIO_OUTPUT_DEVICE_ID,
                    &device_id,
                )
                .await
                .map_err(RepoFailure::from)
            },
            |this, result, cx| this.apply_persist_result(result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_persist_result(&mut self, result: Result<(), RepoFailure>, cx: &mut Context<Self>) {
        if let Err(message) = result {
            tracing::warn!(error = %message, "failed to persist audio output device");
            self.persist_error = Some(message.text());
            cx.notify();
        }
    }

    fn test_tone(&mut self, cx: &mut Context<Self>) {
        if self.test_playing {
            return;
        }
        self.test_playing = true;
        self.test_error = None;
        async_bridge::run_async(
            &self.rt_handle,
            play_test_tone(Arc::clone(&self.speech_output)),
            |this, result, cx| this.apply_test_result(result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_test_result(&mut self, result: Result<(), RepoFailure>, cx: &mut Context<Self>) {
        self.test_playing = false;
        if let Err(message) = result {
            tracing::warn!(error = %message, "audio test tone playback failed");
            self.test_error = Some(message.text());
        }
        cx.notify();
    }

    fn toggle_picker(&mut self, cx: &mut Context<Self>) {
        self.picker_open = !self.picker_open;
        cx.notify();
    }

    fn close_picker(&mut self, cx: &mut Context<Self>) {
        if self.picker_open {
            self.picker_open = false;
            cx.notify();
        }
    }

    fn device_section(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let notice = load_notice(
            &self.devices_load,
            LoadNotice {
                id: "settings-audio-devices-load",
                loading: tr!("settings_audio_scanning"),
                width: NoticeWidth::Pane,
            },
            cx.listener(|this, _: &ClickEvent, _, cx| this.load_devices(true, cx)),
            palette,
            density,
            cx,
        );
        if let Some(notice) = notice {
            return notice;
        }
        self.picker_row(palette, density, cx)
    }

    fn picker_row(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let trigger = setting_choice_trigger(
            "settings-audio-device-trigger",
            selected_device_label(&self.devices, self.selected_idx),
            palette,
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_picker(cx)),
            density,
            cx,
        );

        let mut field = div().relative().flex_1().min_w(px(0.0)).child(trigger);
        if self.picker_open {
            field = field.child(self.picker_overlay(palette, cx));
        }

        let refresh_btn = icon_button(Icon::Refresh, palette).on_click(
            "settings-audio-refresh",
            cx.listener(|this, _: &ClickEvent, _, cx| this.refresh(cx)),
        );

        let test_label = if self.test_playing {
            tr!("settings_audio_test_playing")
        } else {
            tr!("widget_device_test")
        };
        let test_btn = ghost_button_with_icon(Icon::PlayerPlay, test_label, palette)
            .disabled(self.test_playing)
            .on_click(
                "settings-audio-test-inline",
                cx.listener(|this, _: &ClickEvent, _, cx| this.test_tone(cx)),
            );

        div()
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(field)
            .child(refresh_btn)
            .child(test_btn)
            .into_any_element()
    }

    fn picker_overlay(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        device_picker_overlay(
            DevicePickerIds {
                list: "settings-audio-device-list",
                row: "settings-audio-device",
            },
            &self.devices,
            self.selected_idx,
            DevicePickerHandlers {
                select: Self::select_device,
                close: Self::close_picker,
            },
            &self.overlay_focus,
            palette,
            cx,
        )
    }

    fn error_text(
        &self,
        key: &'static str,
        message: &str,
        palette: &ForgePalette,
        cx: &App,
    ) -> impl IntoElement {
        div()
            .font_family(cx.body_family())
            .text_size(FONT_SM)
            .text_color(palette.danger)
            .child(tr!(key, error = message))
    }
}

impl Render for SettingsAudioView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        drive_overlay_focus(
            self.picker_open,
            &self.overlay_focus,
            &mut self.focus_restore,
            window,
            cx,
        );

        let palette = cx.palette();
        let density = cx.density();

        let output_rows = setting_rows(&palette)
            .row(
                setting_row(
                    tr!("settings_audio_output_devices"),
                    None,
                    self.device_section(&palette, density, cx),
                    &palette,
                    density,
                )
                .stacked(),
            )
            .in_card(&palette);

        let mut output = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(output_rows);
        if let Some(message) = &self.test_error {
            output =
                output.child(self.error_text("settings_audio_test_error", message, &palette, cx));
        }
        if let Some(message) = &self.persist_error {
            output = output.child(self.error_text(
                "settings_audio_persist_error",
                message,
                &palette,
                cx,
            ));
        }

        div()
            .flex()
            .flex_col()
            .child(SettingsPaneHeader::new(SettingsSection::Audio))
            .child(setting_groups(
                [
                    setting_block(
                        tr!("settings_audio_title"),
                        None,
                        output,
                        &palette,
                        density,
                        cx,
                    )
                    .into_any_element(),
                    self.routing.clone().into_any_element(),
                    self.voice_gate.clone().into_any_element(),
                ],
                density,
            ))
    }
}

fn resolve_selected_idx(devices: &[DeviceInfo], want_id: Option<&str>) -> usize {
    let resolved =
        forge_audio::resolve_device(&stored_output_device(want_id.map(str::to_string)), devices);
    device_index(devices, resolved.as_ref())
}

async fn enumerate_devices_and_preference(
    settings: Arc<dyn SettingsRepo>,
    uncached: bool,
) -> Result<(Vec<DeviceInfo>, Option<String>), RepoFailure> {
    let devices = enumerate_devices(DeviceDirection::Output, uncached).await?;
    let preference = read_setting(settings.as_ref(), &core_settings::AUDIO_OUTPUT_DEVICE_ID)
        .await
        .map_err(RepoFailure::from)?;
    Ok((devices, preference))
}

pub(crate) fn test_tone() -> PcmBuffer {
    let num_samples = (TEST_TONE_SAMPLE_RATE * TEST_TONE_DURATION_MS / MILLIS_PER_SEC) as usize;
    let samples: Vec<i16> = (0..num_samples)
        .map(|i| {
            let t = i as f32 / TEST_TONE_SAMPLE_RATE as f32;
            let s =
                (2.0 * std::f32::consts::PI * TEST_TONE_FREQ_HZ * t).sin() * TEST_TONE_AMPLITUDE;
            (s * i16::MAX as f32) as i16
        })
        .collect();
    PcmBuffer::new(samples, TEST_TONE_SAMPLE_RATE, TEST_TONE_CHANNELS)
}

async fn play_test_tone(sink: Arc<DeviceSink>) -> Result<(), RepoFailure> {
    sink.play(test_tone()).await.map_err(RepoFailure::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_audio::DeviceId;

    fn row(id: &str, is_default: bool) -> DeviceInfo {
        DeviceInfo {
            id: DeviceId::new(id.to_owned()),
            name: format!("Device {id}"),
            is_default,
        }
    }

    #[test]
    fn the_selected_row_is_the_wanted_device_then_the_default_then_the_first() {
        let devices = [row("a", false), row("b", true), row("c", false)];
        let no_default = [row("a", false), row("c", false)];
        let cases: [(&[DeviceInfo], Option<&str>, usize); 5] = [
            (&devices, Some("c"), 2),
            (&devices, Some("unplugged"), 1),
            (&devices, None, 1),
            (&no_default, None, 0),
            (&[], Some("a"), 0),
        ];

        for (rows, wanted, expected) in cases {
            assert_eq!(resolve_selected_idx(rows, wanted), expected, "{wanted:?}");
        }
    }
}
