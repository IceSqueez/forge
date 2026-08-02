use std::sync::Arc;
use std::time::Duration;

use forge_audio::{
    DeviceId, DeviceInfo, VoiceGateConfig, VoiceGateState, list_input_devices,
    pick_default_input_device, refresh_input_devices,
};
use forge_components::{
    BORDER_THIN, Density, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon, InputEvent, Radius,
    Spacing, TextInput, anchored_popover_below, body_family, drive_overlay_focus, icon,
    mono_family, radius, setting_row, spacing, toggle, tr, with_alpha,
};
use forge_storage::{DataProvider, SettingsRepo, VoiceGateSettings};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, FocusHandle, Pixels, Subscription, Window, div,
    prelude::*, px, relative,
};

use crate::async_bridge;
use crate::presentation::ActivePresentation;
use crate::settings_audio::DeviceRow;
use crate::voice_gate::VoiceGateOwner;

const PANEL_WIDTH: Pixels = px(360.0);
const TRIGGER_HEIGHT: Pixels = px(34.0);
const METER_WIDTH: Pixels = px(72.0);
const METER_HEIGHT: Pixels = px(6.0);
const SLIDER_WIDTH: Pixels = px(140.0);
const READOUT_WIDTH: Pixels = px(38.0);
const HOLD_INPUT_WIDTH: Pixels = px(96.0);

const LEVEL_TICK: Duration = Duration::from_millis(100);

const HOLD_MIN_MS: u32 = 0;
const HOLD_MAX_MS: u32 = 10_000;

pub struct SettingsVoiceGateView {
    backend: Arc<dyn DataProvider>,
    rt_handle: tokio::runtime::Handle,
    owner: Arc<VoiceGateOwner>,
    enabled: bool,
    devices: Vec<DeviceRow>,
    selected_idx: usize,
    selected_id: Option<String>,
    threshold: f32,
    hold_ms: u32,
    hold_input: Entity<TextInput>,
    hold_invalid: bool,
    level: f32,
    gate_state: Option<VoiceGateState>,
    loading: bool,
    devices_error: Option<String>,
    persist_error: Option<String>,
    picker_open: bool,
    overlay_focus: FocusHandle,
    focus_restore: Option<FocusHandle>,
    active: bool,
    ticking: bool,
    devices_gen: async_bridge::Generation,
    threshold_debounce: async_bridge::Debounced,
    hold_debounce: async_bridge::Debounced,
    _subs: Vec<Subscription>,
}

impl SettingsVoiceGateView {
    pub fn new(
        backend: Arc<dyn DataProvider>,
        rt_handle: tokio::runtime::Handle,
        owner: Arc<VoiceGateOwner>,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let defaults = VoiceGateSettings::default();
        let hold_input = cx.new(|cx| {
            let mut input = TextInput::new(tr!("settings_voice_gate_hold_placeholder"), cx)
                .with_palette(palette)
                .with_font_size(FONT_SM);
            input.set_content(defaults.hold_ms.to_string(), cx);
            input
        });
        let subs = vec![cx.subscribe(
            &hold_input,
            |this, _input, event: &InputEvent, cx| match event {
                InputEvent::Changed(text) => this.commit_hold(text.as_ref(), cx),
                InputEvent::Submitted(text) => this.commit_hold(text.as_ref(), cx),
                InputEvent::Cancelled => {}
            },
        )];

        let mut view = Self {
            backend,
            rt_handle,
            owner,
            enabled: defaults.enabled,
            devices: Vec::new(),
            selected_idx: 0,
            selected_id: defaults.input_device_id,
            threshold: defaults.threshold,
            hold_ms: defaults.hold_ms,
            hold_input,
            hold_invalid: false,
            level: 0.0,
            gate_state: None,
            loading: false,
            devices_error: None,
            persist_error: None,
            picker_open: false,
            overlay_focus: cx.focus_handle(),
            focus_restore: None,
            active: false,
            ticking: false,
            devices_gen: async_bridge::Generation::default(),
            threshold_debounce: async_bridge::Debounced::new(async_bridge::SLIDER_PERSIST_DEBOUNCE),
            hold_debounce: async_bridge::Debounced::new(async_bridge::SLIDER_PERSIST_DEBOUNCE),
            _subs: subs,
        };
        view.load(false, cx);
        view
    }

    pub fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if self.active == active {
            return;
        }
        self.active = active;
        if active {
            self.read_gate(cx);
            self.ensure_ticker(cx);
        } else {
            self.level = 0.0;
            cx.notify();
        }
    }

    fn load(&mut self, uncached: bool, cx: &mut Context<Self>) {
        self.loading = true;
        self.devices_error = None;
        let ticket = self.devices_gen.next();
        let settings = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        async_bridge::run_async(
            &self.rt_handle,
            load_devices_and_settings(settings, uncached),
            move |this, result, cx| {
                if this.devices_gen.is_current(ticket) {
                    this.apply_loaded(result, cx);
                }
            },
            cx,
        );
        cx.notify();
    }

    fn apply_loaded(
        &mut self,
        result: Result<(Vec<DeviceRow>, VoiceGateSettings), String>,
        cx: &mut Context<Self>,
    ) {
        self.loading = false;
        match result {
            Ok((devices, settings)) => {
                self.devices = devices;
                self.enabled = settings.enabled;
                self.threshold = settings.threshold;
                self.hold_ms = settings.hold_ms;
                self.hold_invalid = false;
                self.hold_input.update(cx, |input, cx| {
                    input.set_invalid(false, cx);
                    input.set_content(settings.hold_ms.to_string(), cx);
                });
                let want = self.selected_id.clone().or(settings.input_device_id);
                self.selected_idx = resolve_input_idx(&self.devices, want.as_deref());
                self.selected_id = self.devices.get(self.selected_idx).map(|d| d.id.clone());
            }
            Err(message) => {
                tracing::warn!(error = %message, "failed to load voice gate settings");
                self.devices_error = Some(message);
            }
        }
        self.read_gate(cx);
        self.ensure_ticker(cx);
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.load(true, cx);
    }

    fn config(&self) -> VoiceGateConfig {
        VoiceGateConfig {
            device: self.selected_id.clone().map(DeviceId::new),
            threshold: self.threshold,
            hold: Duration::from_millis(u64::from(self.hold_ms)),
        }
    }

    fn read_gate(&mut self, cx: &mut Context<Self>) {
        self.gate_state = self.owner.state();
        self.level = if self.active { self.owner.level() } else { 0.0 };
        cx.notify();
    }

    fn ensure_ticker(&mut self, cx: &mut Context<Self>) {
        if self.ticking || !self.active || !self.owner.is_running() {
            return;
        }
        self.ticking = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(LEVEL_TICK).await;
                let keep_going = this.update(cx, |this, cx| {
                    if this.active && this.owner.is_running() {
                        this.read_gate(cx);
                        true
                    } else {
                        this.ticking = false;
                        this.read_gate(cx);
                        false
                    }
                });
                match keep_going {
                    Ok(true) => continue,
                    _ => break,
                }
            }
        })
        .detach();
    }

    fn toggle_enabled(&mut self, cx: &mut Context<Self>) {
        self.enabled = !self.enabled;
        self.persist_error = None;
        if self.enabled {
            self.owner.start(self.config());
        } else {
            self.owner.stop();
        }
        let settings = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        let enabled = self.enabled;
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                forge_storage::set_voice_gate_enabled(settings.as_ref(), enabled)
                    .await
                    .map_err(|e| e.to_string())
            },
            |this, result, cx| this.apply_persist_result(result, cx),
            cx,
        );
        self.read_gate(cx);
        self.ensure_ticker(cx);
    }

    fn select_device(&mut self, idx: usize, cx: &mut Context<Self>) {
        self.selected_idx = idx;
        self.selected_id = self.devices.get(idx).map(|d| d.id.clone());
        self.persist_error = None;
        self.picker_open = false;
        self.owner.reconfigure(self.config());
        let settings = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        let device_id = self.selected_id.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                forge_storage::set_voice_gate_input_device_id(settings.as_ref(), device_id)
                    .await
                    .map_err(|e| e.to_string())
            },
            |this, result, cx| this.apply_persist_result(result, cx),
            cx,
        );
        self.read_gate(cx);
    }

    fn set_threshold(&mut self, threshold: f32, cx: &mut Context<Self>) {
        self.threshold = threshold.clamp(0.0, 1.0);
        self.owner.reconfigure(self.config());
        let settings = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        let value = self.threshold;
        self.threshold_debounce
            .schedule(&self.rt_handle, "voice gate threshold", async move {
                forge_storage::set_voice_gate_threshold(settings.as_ref(), value).await
            });
        cx.notify();
    }

    fn commit_hold(&mut self, text: &str, cx: &mut Context<Self>) {
        let parsed = text
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|v| (HOLD_MIN_MS..=HOLD_MAX_MS).contains(v));
        self.hold_invalid = parsed.is_none();
        let invalid = self.hold_invalid;
        self.hold_input
            .update(cx, |input, cx| input.set_invalid(invalid, cx));
        if let Some(hold_ms) = parsed {
            self.hold_ms = hold_ms;
            self.owner.reconfigure(self.config());
            let settings = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
            self.hold_debounce
                .schedule(&self.rt_handle, "voice gate hold", async move {
                    forge_storage::set_voice_gate_hold_ms(settings.as_ref(), hold_ms).await
                });
        }
        cx.notify();
    }

    fn apply_persist_result(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        if let Err(message) = result {
            tracing::warn!(error = %message, "failed to persist voice gate settings");
            self.persist_error = Some(message);
            cx.notify();
        }
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

    fn card_header(&self, palette: &ForgePalette, density: Density) -> impl IntoElement {
        let tile = div()
            .size(px(30.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(radius(Radius::Lg))
            .bg(with_alpha(palette.brand, 0.12))
            .child(icon(Icon::Microphone2, px(16.0), palette.brand));
        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Md, density))
            .child(tile)
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(tr!("settings_voice_gate_title")),
            )
    }

    fn section_label(&self, key: &'static str, palette: &ForgePalette) -> impl IntoElement {
        div()
            .py(spacing(Spacing::Xs, Density::Cozy))
            .px(spacing(Spacing::Md, Density::Cozy))
            .font_family(mono_family())
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
                .font_family(body_family())
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child(tr!("settings_voice_gate_scanning"))
                .into_any_element();
        }
        if let Some(message) = &self.devices_error {
            return div()
                .font_family(body_family())
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
            .id("settings-voice-gate-device-trigger")
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
                    .font_family(body_family())
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
            .id("settings-voice-gate-refresh")
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

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(field)
            .child(refresh_btn)
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
                .id(("settings-voice-gate-device", idx))
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
                        .font_family(body_family())
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

    fn threshold_row(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let control = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(div().w(SLIDER_WIDTH).child(
                forge_components::slider(self.threshold, 0.0, 1.0, palette).on_change(
                    "settings-voice-gate-threshold",
                    cx.listener(|this, value: &f32, _, cx| this.set_threshold(*value, cx)),
                ),
            ))
            .child(
                div()
                    .w(READOUT_WIDTH)
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(format!("{}%", (self.threshold * 100.0).round() as u32)),
            )
            .child(self.level_meter(palette));

        setting_row(
            tr!("settings_voice_gate_threshold_label"),
            Some(tr!("settings_voice_gate_threshold_hint").into()),
            control,
            palette,
            density,
        )
    }

    fn level_meter(&self, palette: &ForgePalette) -> impl IntoElement {
        let level = self.level.clamp(0.0, 1.0);
        let fill = if level >= self.threshold {
            palette.success
        } else {
            palette.info
        };
        div()
            .w(METER_WIDTH)
            .h(METER_HEIGHT)
            .rounded(radius(Radius::Pill))
            .bg(palette.surface_overlay)
            .child(
                div()
                    .h_full()
                    .w(relative(level))
                    .rounded(radius(Radius::Pill))
                    .bg(fill),
            )
    }

    fn hold_row(&self, palette: &ForgePalette, density: Density) -> impl IntoElement {
        let hint = if self.hold_invalid {
            tr!(
                "settings_voice_gate_hold_range",
                min = HOLD_MIN_MS.to_string(),
                max = HOLD_MAX_MS.to_string()
            )
        } else {
            tr!("settings_voice_gate_hold_hint")
        };
        setting_row(
            tr!("settings_voice_gate_hold_label"),
            Some(hint.into()),
            div().w(HOLD_INPUT_WIDTH).child(self.hold_input.clone()),
            palette,
            density,
        )
    }

    fn state_line(&self, palette: &ForgePalette) -> impl IntoElement {
        let (text, color) = match &self.gate_state {
            None => (tr!("settings_voice_gate_state_off"), palette.text_muted),
            Some(VoiceGateState::Inactive) => (
                tr!("settings_voice_gate_state_inactive"),
                palette.text_secondary,
            ),
            Some(VoiceGateState::Active) => {
                (tr!("settings_voice_gate_state_active"), palette.success)
            }
            Some(VoiceGateState::Unavailable(message)) => (
                tr!("settings_voice_gate_state_unavailable", error = message),
                palette.random,
            ),
        };
        div()
            .px(spacing(Spacing::Md, Density::Cozy))
            .font_family(body_family())
            .text_size(FONT_XS)
            .text_color(color)
            .child(text)
    }
}

impl Render for SettingsVoiceGateView {
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
            .child(setting_row(
                tr!("settings_voice_gate_enable_label"),
                Some(tr!("settings_voice_gate_enable_hint").into()),
                toggle(self.enabled, &palette).on_click(
                    "settings-voice-gate-enable",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_enabled(cx)),
                ),
                &palette,
                density,
            ))
            .child(self.section_label("settings_voice_gate_input_devices", &palette))
            .child(self.device_section(&palette, density, cx))
            .child(self.threshold_row(&palette, density, cx))
            .child(self.hold_row(&palette, density))
            .child(self.state_line(&palette));

        if let Some(message) = &self.persist_error {
            content = content.child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.random)
                    .child(tr!("settings_voice_gate_persist_error", error = message)),
            );
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
            .child(self.card_header(&palette, density))
            .child(content)
    }
}

fn resolve_input_idx(devices: &[DeviceRow], want_id: Option<&str>) -> usize {
    if let Some(want) = want_id
        && let Some(idx) = devices.iter().position(|d| d.id == want)
    {
        return idx;
    }
    let infos: Vec<DeviceInfo> = devices
        .iter()
        .map(|d| DeviceInfo {
            id: DeviceId::new(d.id.clone()),
            name: d.name.clone(),
            is_default: d.is_default,
        })
        .collect();
    pick_default_input_device(&infos)
        .and_then(|id| devices.iter().position(|d| d.id == id.as_str()))
        .unwrap_or(0)
}

async fn enumerate_inputs(uncached: bool) -> Result<Vec<DeviceRow>, String> {
    tokio::task::spawn_blocking(move || {
        let listed = if uncached {
            refresh_input_devices()
        } else {
            list_input_devices()
        };
        listed
            .map(|devs| devs.into_iter().map(DeviceRow::from_info).collect())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn load_devices_and_settings(
    settings: Arc<dyn SettingsRepo>,
    uncached: bool,
) -> Result<(Vec<DeviceRow>, VoiceGateSettings), String> {
    let devices = enumerate_inputs(uncached).await?;
    let stored = forge_storage::voice_gate_settings(settings.as_ref())
        .await
        .map_err(|e| e.to_string())?;
    Ok((devices, stored))
}
