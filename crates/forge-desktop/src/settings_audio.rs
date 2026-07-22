use std::sync::Arc;

use forge_audio::{DeviceId, DeviceInfo, list_output_devices, refresh_output_devices};
use forge_components::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, Density, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon,
    Radius, Spacing, anchored_popover_below, drive_overlay_focus, icon, radius, spacing, tr,
    with_alpha,
};
use forge_storage::{DataProvider, SettingsRepo};
use gpui::{AnyElement, ClickEvent, Context, FocusHandle, Pixels, Window, div, prelude::*, px};

use crate::async_bridge;
use crate::presentation::ActivePresentation;

const PANEL_WIDTH: Pixels = px(360.0);

const TRIGGER_HEIGHT: Pixels = px(34.0);

#[derive(Clone)]
struct DeviceRow {
    id: String,
    name: String,
    is_default: bool,
}

impl DeviceRow {
    fn from_info(info: DeviceInfo) -> Self {
        Self {
            id: info.id.as_str().to_string(),
            name: info.name,
            is_default: info.is_default,
        }
    }
}

pub struct SettingsAudioView {
    backend: Arc<dyn DataProvider>,
    rt_handle: tokio::runtime::Handle,
    devices: Vec<DeviceRow>,
    selected_idx: usize,
    selected_id: Option<String>,
    loading: bool,
    devices_error: Option<String>,
    persist_error: Option<String>,
    test_playing: bool,
    test_error: Option<String>,
    picker_open: bool,
    overlay_focus: FocusHandle,
    focus_restore: Option<FocusHandle>,
    devices_gen: async_bridge::Generation,
}

impl SettingsAudioView {
    pub fn new(
        backend: Arc<dyn DataProvider>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut view = Self {
            backend,
            rt_handle,
            devices: Vec::new(),
            selected_idx: 0,
            selected_id: None,
            loading: false,
            devices_error: None,
            persist_error: None,
            test_playing: false,
            test_error: None,
            picker_open: false,
            overlay_focus: cx.focus_handle(),
            focus_restore: None,
            devices_gen: async_bridge::Generation::default(),
        };
        view.load_devices(false, cx);
        view
    }

    fn load_devices(&mut self, uncached: bool, cx: &mut Context<Self>) {
        self.loading = true;
        self.devices_error = None;
        let ticket = self.devices_gen.next();
        let settings = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let _ = tx.send(enumerate_devices_and_preference(settings, uncached).await);
        });
        cx.spawn(async move |this, cx| {
            if let Ok(result) = rx.await {
                let _ = this.update(cx, |this, cx| {
                    if this.devices_gen.is_current(ticket) {
                        this.apply_devices_loaded(result, cx);
                    }
                });
            }
        })
        .detach();
        cx.notify();
    }

    fn apply_devices_loaded(
        &mut self,
        result: Result<(Vec<DeviceRow>, Option<String>), String>,
        cx: &mut Context<Self>,
    ) {
        self.loading = false;
        match result {
            Ok((devices, persisted)) => {
                self.devices = devices;
                let want = self.selected_id.clone().or(persisted);
                self.selected_idx = resolve_selected_idx(&self.devices, want.as_deref());
                self.selected_id = self.devices.get(self.selected_idx).map(|d| d.id.clone());
            }
            Err(message) => {
                tracing::warn!(error = %message, "failed to enumerate audio output devices");
                self.devices_error = Some(message);
            }
        }
        cx.notify();
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.load_devices(true, cx);
    }

    fn select_device(&mut self, idx: usize, cx: &mut Context<Self>) {
        self.selected_idx = idx;
        self.selected_id = self.devices.get(idx).map(|d| d.id.clone());
        self.persist_error = None;
        self.picker_open = false;
        let settings = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        let device_id = self.selected_id.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let outcome = settings
                .set_audio_output_device_id(device_id)
                .await
                .map_err(|e| e.to_string());
            let _ = tx.send(outcome);
        });
        cx.spawn(async move |this, cx| {
            if let Ok(result) = rx.await {
                let _ = this.update(cx, |this, cx| this.apply_persist_result(result, cx));
            }
        })
        .detach();
        cx.notify();
    }

    fn apply_persist_result(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        if let Err(message) = result {
            tracing::warn!(error = %message, "failed to persist audio output device");
            self.persist_error = Some(message);
            cx.notify();
        }
    }

    fn test_tone(&mut self, cx: &mut Context<Self>) {
        if self.test_playing {
            return;
        }
        self.test_playing = true;
        self.test_error = None;
        let device_id = self.devices.get(self.selected_idx).map(|d| d.id.clone());
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let _ = tx.send(play_test_tone(device_id).await);
        });
        cx.spawn(async move |this, cx| {
            if let Ok(result) = rx.await {
                let _ = this.update(cx, |this, cx| this.apply_test_result(result, cx));
            }
        })
        .detach();
        cx.notify();
    }

    fn apply_test_result(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        self.test_playing = false;
        if let Err(message) = result {
            tracing::warn!(error = %message, "audio test tone playback failed");
            self.test_error = Some(message);
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

    fn display_name(&self, device: &DeviceRow) -> String {
        if device.is_default {
            format!("{} {}", device.name, tr!("widget_device_default_suffix"))
        } else {
            device.name.clone()
        }
    }

    fn screen_header(&self, palette: &ForgePalette, density: Density) -> impl IntoElement {
        let tile = div()
            .size(px(30.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(radius(Radius::Lg))
            .bg(with_alpha(palette.info, 0.12))
            .child(icon(Icon::Volume, px(16.0), palette.info));
        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Md, density))
            .child(tile)
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(tr!("settings_audio_title")),
            )
    }

    fn section_label(&self, key: &'static str, palette: &ForgePalette) -> impl IntoElement {
        div()
            .py(spacing(Spacing::Xs, Density::Cozy))
            .px(spacing(Spacing::Md, Density::Cozy))
            .font_family(forge_components::DEFAULT_MONO_FAMILY)
            .text_size(FONT_XXS)
            .text_color(palette.text_muted)
            .child(tr!(key))
    }

    fn device_section(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.loading {
            return div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child(tr!("settings_audio_scanning"))
                .into_any_element();
        }
        if let Some(message) = &self.devices_error {
            return div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.random)
                .child(message.clone())
                .into_any_element();
        }
        self.picker_row(palette, density, cx)
    }

    fn picker_row(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected_name = self
            .devices
            .get(self.selected_idx)
            .map(|d| self.display_name(d))
            .unwrap_or_else(|| "-".to_string());

        let trigger = div()
            .id("settings-audio-device-trigger")
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .px(spacing(Spacing::Sm, Density::Cozy))
            .py(px(7.0))
            .rounded(radius(Radius::Sm))
            .bg(palette.base)
            .border(BORDER_THIN)
            .border_color(palette.border_input)
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_picker(cx)))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(selected_name),
            )
            .child(icon(Icon::ChevronDown, FONT_XS, palette.text_faint));

        let mut field = div().relative().flex_1().child(trigger);
        if self.picker_open {
            field = field.child(self.picker_overlay(palette, cx));
        }

        let refresh_btn = div()
            .id("settings-audio-refresh")
            .flex()
            .items_center()
            .justify_center()
            .px(spacing(Spacing::Xs, Density::Cozy))
            .py(spacing(Spacing::Xxs, Density::Cozy))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .cursor_pointer()
            .hover(|style| style.bg(with_alpha(palette.border_regular, 0.08)))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.refresh(cx)))
            .child(icon(Icon::Refresh, px(12.0), palette.text_secondary));

        let test_btn = div()
            .id("settings-audio-test-inline")
            .flex()
            .items_center()
            .gap(px(4.0))
            .px(spacing(Spacing::Sm, Density::Cozy))
            .py(spacing(Spacing::Xxs, Density::Cozy))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .cursor_pointer()
            .hover(|style| style.bg(with_alpha(palette.border_regular, 0.08)))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.test_tone(cx)))
            .child(icon(Icon::PlayerPlay, px(11.0), palette.text_secondary))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_secondary)
                    .child(tr!("widget_device_test")),
            );

        div()
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
        let mut panel = div()
            .flex()
            .flex_col()
            .w(PANEL_WIDTH)
            .py(spacing(Spacing::Xs, Density::Cozy))
            .bg(palette.elevated)
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_input)
            .occlude();

        for (idx, device) in self.devices.iter().enumerate() {
            let selected = idx == self.selected_idx;
            let mut item = div()
                .id(("settings-audio-device", idx))
                .flex()
                .items_center()
                .w_full()
                .gap(spacing(Spacing::Sm, Density::Cozy))
                .px(spacing(Spacing::Sm, Density::Cozy))
                .py(spacing(Spacing::Xs, Density::Cozy))
                .rounded(radius(Radius::Sm))
                .cursor_pointer()
                .hover(|style| style.bg(palette.surface_overlay))
                .on_click(
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.select_device(idx, cx)),
                )
                .child(
                    div()
                        .flex_1()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .text_size(FONT_SM)
                        .text_color(palette.text_primary)
                        .child(self.display_name(device)),
                );
            if selected {
                item = item.child(icon(Icon::CircleCheck, FONT_SM, palette.brand));
            }
            panel = panel.child(item);
        }

        let view = cx.entity();
        anchored_popover_below(TRIGGER_HEIGHT, panel)
            .dismiss_on_escape(&self.overlay_focus)
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_picker(cx));
            })
            .into_any_element()
    }

    fn standalone_test(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = if self.test_playing {
            tr!("settings_audio_test_playing")
        } else {
            tr!("settings_audio_test_tone")
        };
        let mut button = div()
            .id("settings-audio-test")
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Sm, Density::Cozy))
            .py(spacing(Spacing::Xxs, Density::Cozy))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(icon(Icon::Volume, px(12.0), palette.info))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.info)
                    .child(label),
            );
        if !self.test_playing {
            let hover = with_alpha(palette.info, 0.08);
            button = button
                .cursor_pointer()
                .hover(move |style| style.bg(hover))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.test_tone(cx)));
        }
        button.into_any_element()
    }

    fn error_text(
        &self,
        key: &'static str,
        message: &str,
        palette: &ForgePalette,
    ) -> impl IntoElement {
        div()
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FONT_SM)
            .text_color(palette.random)
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

        let mut content = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(self.section_label("settings_audio_output_devices", &palette))
            .child(self.device_section(&palette, density, cx))
            .child(self.section_label("settings_audio_test_section", &palette))
            .child(self.standalone_test(&palette, density, cx));

        if let Some(message) = &self.test_error {
            content =
                content.child(self.error_text("settings_audio_test_error", message, &palette));
        }
        if let Some(message) = &self.persist_error {
            content =
                content.child(self.error_text("settings_audio_persist_error", message, &palette));
        }

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .p(spacing(Spacing::Md, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.elevated)
            .child(self.screen_header(&palette, density))
            .child(content)
    }
}

fn resolve_selected_idx(devices: &[DeviceRow], want_id: Option<&str>) -> usize {
    let infos: Vec<DeviceInfo> = devices
        .iter()
        .map(|d| DeviceInfo {
            id: DeviceId::new(d.id.clone()),
            name: d.name.clone(),
            is_default: d.is_default,
        })
        .collect();
    let resolved = forge_audio::resolve_device(want_id.map(str::to_string), &infos);
    resolved
        .and_then(|id| devices.iter().position(|d| d.id == id.as_str()))
        .unwrap_or(0)
}

async fn enumerate_devices(uncached: bool) -> Result<Vec<DeviceRow>, String> {
    tokio::task::spawn_blocking(move || {
        let listed = if uncached {
            refresh_output_devices()
        } else {
            list_output_devices()
        };
        listed
            .map(|devs| devs.into_iter().map(DeviceRow::from_info).collect())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn enumerate_devices_and_preference(
    settings: Arc<dyn SettingsRepo>,
    uncached: bool,
) -> Result<(Vec<DeviceRow>, Option<String>), String> {
    let devices = enumerate_devices(uncached).await?;
    let preference = settings
        .audio_output_device_id()
        .await
        .map_err(|e| e.to_string())?;
    Ok((devices, preference))
}

async fn play_test_tone(device_id: Option<String>) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        use forge_audio::{AudioSink, CpalSink, NullAudioEventSink, PcmBuffer};

        const SAMPLE_RATE: u32 = 22_050;
        const DURATION_MS: u32 = 200;
        const FREQ_HZ: f32 = 440.0;
        let num_samples = (SAMPLE_RATE * DURATION_MS / 1000) as usize;
        let samples: Vec<i16> = (0..num_samples)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE as f32;
                let s = (2.0 * std::f32::consts::PI * FREQ_HZ * t).sin() * 0.35;
                (s * i16::MAX as f32) as i16
            })
            .collect();

        let buf = PcmBuffer::new(samples, SAMPLE_RATE, 1);
        let devices = forge_audio::list_output_devices().map_err(|e| e.to_string())?;
        let id = forge_audio::resolve_device(device_id, &devices)
            .ok_or_else(|| "no output device available".to_string())?;
        let sink = CpalSink::new(id, Some(SAMPLE_RATE), Some(1), Arc::new(NullAudioEventSink));
        let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
        rt.block_on(sink.play(buf)).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
