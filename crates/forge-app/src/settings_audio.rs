use std::sync::Arc;

use forge_audio::{list_output_devices, refresh_output_devices};
use forge_storage_sqlite::SqliteBackend;
use forge_widgets::icons::{Icon, tabler_icon};
use forge_widgets::tokens::{
    Density, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, spacing,
};
use forge_widgets::{DeviceLabel, ForgePalette, output_device_picker, section_header};
use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Background, Border, Element, Length, Shadow, Task};

use crate::Message;
use crate::message::SettingsAudioMsg;

pub struct SettingsAudioState {
    pub devices: Vec<DeviceLabel>,
    pub selected_device_idx: usize,
    pub devices_loading: bool,
    pub devices_error: Option<String>,
    pub test_tone_playing: bool,
    pub test_tone_error: Option<String>,
}

impl SettingsAudioState {
    pub fn new() -> Self {
        Self {
            devices: Vec::new(),
            selected_device_idx: 0,
            devices_loading: false,
            devices_error: None,
            test_tone_playing: false,
            test_tone_error: None,
        }
    }
}

impl Default for SettingsAudioState {
    fn default() -> Self {
        Self::new()
    }
}

fn forge_audio_to_widget_label(d: forge_audio::DeviceInfo) -> DeviceLabel {
    DeviceLabel {
        id: d.id.as_str().to_string(),
        name: d.name.clone(),
        is_default: d.is_default,
    }
}

async fn enumerate_devices() -> Result<Vec<DeviceLabel>, String> {
    tokio::task::spawn_blocking(|| {
        list_output_devices()
            .map(|devs| devs.into_iter().map(forge_audio_to_widget_label).collect())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn enumerate_devices_uncached() -> Result<Vec<DeviceLabel>, String> {
    tokio::task::spawn_blocking(|| {
        refresh_output_devices()
            .map(|devs| devs.into_iter().map(forge_audio_to_widget_label).collect())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn play_test_tone(device_id: Option<String>) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        use forge_audio::{AudioSink, CpalSink, DeviceId, NullAudioEventSink, PcmBuffer};

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
        let id = device_id
            .map(DeviceId::new)
            .unwrap_or_else(|| DeviceId::new("default".to_string()));
        let sink = CpalSink::new(id, Some(SAMPLE_RATE), Some(1), Arc::new(NullAudioEventSink));
        let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
        rt.block_on(sink.play(buf)).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

pub fn handle_settings_audio_msg(
    state: &mut SettingsAudioState,
    _backend: Arc<SqliteBackend>,
    msg: SettingsAudioMsg,
) -> Task<Message> {
    match msg {
        SettingsAudioMsg::LoadRequested => {
            state.devices_loading = true;
            state.devices_error = None;
            Task::perform(enumerate_devices(), |r| {
                Message::SettingsAudio(SettingsAudioMsg::DevicesLoaded(r))
            })
        }
        SettingsAudioMsg::DevicesLoaded(Ok(devices)) => {
            state.devices = devices;
            state.devices_loading = false;
            Task::none()
        }
        SettingsAudioMsg::DevicesLoaded(Err(e)) => {
            state.devices_loading = false;
            state.devices_error = Some(e);
            Task::none()
        }
        SettingsAudioMsg::RefreshDevices => {
            state.devices_loading = true;
            state.devices_error = None;
            Task::perform(enumerate_devices_uncached(), |r| {
                Message::SettingsAudio(SettingsAudioMsg::DevicesLoaded(r))
            })
        }
        SettingsAudioMsg::DeviceSelected(idx) => {
            state.selected_device_idx = idx;
            Task::none()
        }
        SettingsAudioMsg::TestToneRequested => {
            state.test_tone_playing = true;
            state.test_tone_error = None;
            let device_id = state
                .devices
                .get(state.selected_device_idx)
                .map(|d| d.id.clone());
            Task::perform(play_test_tone(device_id), |r| {
                Message::SettingsAudio(SettingsAudioMsg::TestToneResult(r))
            })
        }
        SettingsAudioMsg::TestToneResult(Ok(())) => {
            state.test_tone_playing = false;
            Task::none()
        }
        SettingsAudioMsg::TestToneResult(Err(e)) => {
            state.test_tone_playing = false;
            state.test_tone_error = Some(e);
            Task::none()
        }
    }
}

pub fn settings_audio_view<'a>(
    state: &'a SettingsAudioState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let gap_lg = f32::from(spacing(Spacing::Sm, Density::Cozy));
    let gap_xl = f32::from(spacing(Spacing::Md, Density::Cozy));
    let gap_xxl = f32::from(spacing(Spacing::Md, Density::Cozy));

    let header = section_header("OUTPUT DEVICES", None, palette);

    let device_section: Element<'a, Message> = if state.devices_loading {
        text("Scanning devices\u{2026}")
            .size(FONT_SM)
            .color(palette.text_muted)
            .into()
    } else if let Some(ref e) = state.devices_error {
        text(e.as_str()).size(FONT_SM).color(palette.random).into()
    } else {
        output_device_picker(
            &state.devices,
            state.selected_device_idx,
            |idx| Message::SettingsAudio(SettingsAudioMsg::DeviceSelected(idx)),
            Message::SettingsAudio(SettingsAudioMsg::RefreshDevices),
            Message::SettingsAudio(SettingsAudioMsg::TestToneRequested),
            palette,
        )
    };

    let test_error_el: Option<Element<'a, Message>> = state.test_tone_error.as_ref().map(|e| {
        text(format!("Test tone error: {e}"))
            .size(FONT_SM)
            .color(palette.random)
            .font(font(FontRole::Body))
            .into()
    });

    let p = *palette;
    let test_btn_label = if state.test_tone_playing {
        "Playing\u{2026}"
    } else {
        "Play 440 Hz test tone"
    };

    let test_standalone_btn = button(
        row![
            tabler_icon(Icon::Volume, 12.0, p.info),
            text(test_btn_label)
                .size(FONT_SM)
                .color(p.info)
                .font(font(FontRole::Body)),
        ]
        .spacing(gap_lg)
        .align_y(Alignment::Center),
    )
    .on_press_maybe(if state.test_tone_playing {
        None
    } else {
        Some(Message::SettingsAudio(SettingsAudioMsg::TestToneRequested))
    })
    .padding([5.0, 12.0])
    .style(move |_theme, status| {
        let bg_a = if matches!(status, iced::widget::button::Status::Hovered) {
            0.08
        } else {
            0.0
        };
        button::Style {
            background: Some(Background::Color(iced::Color { a: bg_a, ..p.info })),
            border: Border {
                color: p.border_regular,
                width: 0.5,
                radius: radius(Radius::Md).into(),
            },
            text_color: p.info,
            shadow: Shadow::default(),
            snap: false,
        }
    });

    let mut content_col = column![
        header,
        device_section,
        text("TEST")
            .size(FONT_XS)
            .color(palette.text_muted)
            .font(font(FontRole::Monospace)),
        test_standalone_btn,
    ]
    .spacing(gap_xxl);

    if let Some(err) = test_error_el {
        content_col = content_col.push(err);
    }

    let p2 = *palette;
    let icon_el = container(tabler_icon(Icon::Volume, 16.0, p2.info))
        .width(30.0)
        .height(30.0)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_| container::Style {
            background: Some(Background::Color(iced::Color { a: 0.12, ..p2.info })),
            border: Border {
                radius: radius(Radius::Lg).into(),
                color: iced::Color::TRANSPARENT,
                width: 0.0,
            },
            ..container::Style::default()
        });

    let screen_header = row![
        icon_el,
        text("Audio")
            .size(FONT_SM)
            .color(palette.text_primary)
            .font(font(FontRole::Body)),
    ]
    .spacing(gap_xl)
    .align_y(Alignment::Center);

    let card = container(column![screen_header, content_col].spacing(gap_xxl))
        .padding(f32::from(spacing(Spacing::Md, Density::Cozy)))
        .style(move |_| {
            let p3 = p2;
            container::Style {
                background: Some(Background::Color(p3.elevated)),
                border: Border {
                    color: p3.border_regular,
                    width: 0.5,
                    radius: radius(Radius::Md).into(),
                },
                ..container::Style::default()
            }
        });

    container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(f32::from(spacing(Spacing::Md, Density::Cozy)))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_widgets::palette::CATPPUCCIN_MOCHA;

    #[test]
    fn settings_audio_state_new() {
        let s = SettingsAudioState::new();
        assert!(s.devices.is_empty());
        assert!(!s.devices_loading);
        assert!(!s.test_tone_playing);
    }

    #[test]
    fn settings_audio_view_idle_constructs() {
        let state = SettingsAudioState::new();
        let _ = settings_audio_view(&state, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn settings_audio_view_loading_constructs() {
        let mut state = SettingsAudioState::new();
        state.devices_loading = true;
        let _ = settings_audio_view(&state, &CATPPUCCIN_MOCHA);
    }
}
