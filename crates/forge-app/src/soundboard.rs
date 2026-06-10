use std::path::PathBuf;
use std::sync::Arc;

use forge_audio::list_output_devices;
use forge_soundboard::SoundboardPlayer;
use forge_storage::{SoundboardClipsRepo, StoredClip};
use forge_types::{ClipId, OutputDevice};
use forge_widgets::icons::{Icon, tabler_icon};
use forge_widgets::tokens::{
    BORDER_THIN, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, spf,
};
use forge_widgets::{ClipCardData, DeviceLabel, ForgePalette, clip_card, output_device_picker};
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Background, Border, Element, Length, Task};

use crate::Message;
use crate::message::SoundboardMsg;
use crate::runtime_view::RuntimeView;

pub struct SoundboardState {
    pub clips: Vec<StoredClip>,
    pub card_data: Vec<ClipCardData>,
    pub loading: bool,
    pub error: Option<String>,
    pub add_modal: Option<AddClipModal>,
    pub play_error: Option<String>,
}

pub struct AddClipModal {
    pub editing_id: Option<ClipId>,
    pub file_path: Option<PathBuf>,
    pub name: String,
    pub hotkey: String,
    pub device_choice_idx: usize,
    pub volume: f32,
    pub saving: bool,
    pub error: Option<String>,
    pub devices: Vec<DeviceLabel>,
    pub devices_loading: bool,
}

impl AddClipModal {
    fn new(editing_id: Option<ClipId>) -> Self {
        Self {
            editing_id,
            file_path: None,
            name: String::new(),
            hotkey: String::new(),
            device_choice_idx: 0,
            volume: 1.0,
            saving: false,
            error: None,
            devices: Vec::new(),
            devices_loading: true,
        }
    }

    pub fn is_valid(&self) -> bool {
        !self.name.is_empty() && self.file_path.is_some()
    }
}

impl SoundboardState {
    pub fn new() -> Self {
        Self {
            clips: Vec::new(),
            card_data: Vec::new(),
            loading: false,
            error: None,
            add_modal: None,
            play_error: None,
        }
    }

    fn rebuild_card_data(&mut self) {
        self.card_data = self
            .clips
            .iter()
            .map(|c| ClipCardData {
                name: c.name.clone(),
                duration_label: "\u{2014}".to_string(),
                hotkey_label: c.hotkey.clone(),
                device_label: device_display_label(&c.output_device),
                volume_pct: (c.volume * 100.0).round() as u8,
            })
            .collect();
    }
}

impl Default for SoundboardState {
    fn default() -> Self {
        Self::new()
    }
}

fn device_display_label(dev: &OutputDevice) -> String {
    match dev {
        OutputDevice::Default => "default".to_string(),
        OutputDevice::ByName { name } => name.clone(),
        OutputDevice::ById { id } => id.clone(),
    }
}

fn forge_audio_to_widget_label(d: forge_audio::DeviceInfo) -> DeviceLabel {
    DeviceLabel {
        id: d.id.as_str().to_string(),
        name: d.name.clone(),
        is_default: d.is_default,
    }
}

async fn load_clips(repo: Arc<dyn SoundboardClipsRepo>) -> Result<Vec<StoredClip>, String> {
    repo.list().await.map_err(|e| e.to_string())
}

async fn load_devices() -> Result<Vec<DeviceLabel>, String> {
    tokio::task::spawn_blocking(|| {
        list_output_devices()
            .map(|devs| devs.into_iter().map(forge_audio_to_widget_label).collect())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn save_clip(repo: Arc<dyn SoundboardClipsRepo>, clip: StoredClip) -> Result<(), String> {
    repo.save(&clip).await.map_err(|e| e.to_string())
}

async fn delete_clip(repo: Arc<dyn SoundboardClipsRepo>, id: ClipId) -> Result<(), String> {
    repo.delete(id).await.map(|_| ()).map_err(|e| e.to_string())
}

async fn play_clip(player: Arc<SoundboardPlayer>, id: ClipId) -> Result<(), String> {
    player.play(id, None).await.map_err(|e| e.to_string())
}

pub fn update(state: &mut SoundboardState, rt: &RuntimeView, msg: SoundboardMsg) -> Task<Message> {
    match msg {
        SoundboardMsg::LoadRequested => {
            state.loading = true;
            state.error = None;
            let repo = rt.backend.soundboard_clips_repo();
            Task::perform(load_clips(repo), |r| {
                Message::Soundboard(SoundboardMsg::ClipsLoaded(r))
            })
        }
        SoundboardMsg::ClipsLoaded(Ok(clips)) => {
            state.clips = clips;
            state.loading = false;
            state.rebuild_card_data();
            Task::none()
        }
        SoundboardMsg::ClipsLoaded(Err(e)) => {
            state.loading = false;
            state.error = Some(e);
            Task::none()
        }
        SoundboardMsg::OpenAddModal => {
            state.add_modal = Some(AddClipModal::new(None));
            Task::perform(load_devices(), |r| {
                Message::Soundboard(SoundboardMsg::ModalDevicesLoaded(r))
            })
        }
        SoundboardMsg::OpenEditModal(clip_id) => {
            let mut modal = AddClipModal::new(Some(clip_id));
            if let Some(clip) = state.clips.iter().find(|c| c.id == clip_id) {
                modal.name = clip.name.clone();
                modal.file_path = Some(clip.file_path.clone());
                modal.hotkey = clip.hotkey.clone().unwrap_or_default();
                modal.volume = clip.volume;
            }
            state.add_modal = Some(modal);
            Task::perform(load_devices(), |r| {
                Message::Soundboard(SoundboardMsg::ModalDevicesLoaded(r))
            })
        }
        SoundboardMsg::ModalDevicesLoaded(Ok(devices)) => {
            if let Some(modal) = state.add_modal.as_mut() {
                modal.devices = devices;
                modal.devices_loading = false;
            }
            Task::none()
        }
        SoundboardMsg::ModalDevicesLoaded(Err(e)) => {
            if let Some(modal) = state.add_modal.as_mut() {
                modal.devices_loading = false;
                modal.error = Some(format!("Device load failed: {e}"));
            }
            Task::none()
        }
        SoundboardMsg::ModalFilePickRequested => Task::perform(
            async {
                rfd::AsyncFileDialog::new()
                    .add_filter("Audio", &["mp3", "wav", "ogg", "flac", "aac", "m4a"])
                    .pick_file()
                    .await
                    .map(|h| h.path().to_path_buf())
            },
            |p| Message::Soundboard(SoundboardMsg::ModalFilePicked(p)),
        ),
        SoundboardMsg::ModalFilePicked(path) => {
            if let (Some(modal), Some(ref p)) = (state.add_modal.as_mut(), path) {
                if modal.name.is_empty()
                    && let Some(stem) = p.file_stem().and_then(|s| s.to_str())
                {
                    modal.name = stem.to_string();
                }
                modal.file_path = Some(p.clone());
            }
            Task::none()
        }
        SoundboardMsg::ModalNameChanged(s) => {
            if let Some(modal) = state.add_modal.as_mut() {
                modal.name = s;
            }
            Task::none()
        }
        SoundboardMsg::ModalHotkeyChanged(s) => {
            if let Some(modal) = state.add_modal.as_mut() {
                modal.hotkey = s;
            }
            Task::none()
        }
        SoundboardMsg::ModalDeviceSelected(idx) => {
            if let Some(modal) = state.add_modal.as_mut() {
                modal.device_choice_idx = idx;
            }
            Task::none()
        }
        SoundboardMsg::ModalVolumeChanged(v) => {
            if let Some(modal) = state.add_modal.as_mut() {
                modal.volume = v;
            }
            Task::none()
        }
        SoundboardMsg::ModalSave => {
            let Some(modal) = state.add_modal.as_mut() else {
                return Task::none();
            };
            if !modal.is_valid() {
                modal.error = Some("Name and audio file are required.".to_string());
                return Task::none();
            }
            modal.saving = true;
            modal.error = None;

            let clip_id = modal.editing_id.unwrap_or_else(ClipId::new);
            let file_path = modal.file_path.clone().unwrap_or_default();
            let name = modal.name.clone();
            let hotkey = if modal.hotkey.is_empty() {
                None
            } else {
                Some(modal.hotkey.clone())
            };
            let volume = modal.volume;
            let output_device = modal
                .devices
                .get(modal.device_choice_idx)
                .map(|d| OutputDevice::ById { id: d.id.clone() })
                .unwrap_or(OutputDevice::Default);

            let clip = StoredClip {
                id: clip_id,
                name,
                file_path,
                volume,
                output_device,
                hotkey,
                created_at: time::OffsetDateTime::now_utc(),
            };
            let repo = rt.backend.soundboard_clips_repo();
            Task::perform(save_clip(repo, clip), |r| {
                Message::Soundboard(SoundboardMsg::ModalSaved(r))
            })
        }
        SoundboardMsg::ModalSaved(Ok(())) => {
            state.add_modal = None;
            Task::done(Message::Soundboard(SoundboardMsg::LoadRequested))
        }
        SoundboardMsg::ModalSaved(Err(e)) => {
            if let Some(modal) = state.add_modal.as_mut() {
                modal.saving = false;
                modal.error = Some(e);
            }
            Task::none()
        }
        SoundboardMsg::ModalCancel => {
            state.add_modal = None;
            Task::none()
        }
        SoundboardMsg::PlayClip(clip_id) => {
            state.play_error = None;
            let Some(p) = rt.sound_player.clone() else {
                state.play_error =
                    Some("Audio player not initialised — check Settings → Audio.".to_string());
                return Task::none();
            };
            Task::perform(play_clip(p, clip_id), |r| {
                Message::Soundboard(SoundboardMsg::PlayResult(r))
            })
        }
        SoundboardMsg::PlayResult(Ok(())) => Task::none(),
        SoundboardMsg::PlayResult(Err(e)) => {
            state.play_error = Some(e);
            Task::none()
        }
        SoundboardMsg::DeleteClip(clip_id) => {
            let repo = rt.backend.soundboard_clips_repo();
            Task::perform(delete_clip(repo, clip_id), |r| {
                Message::Soundboard(SoundboardMsg::ClipDeleted(r))
            })
        }
        SoundboardMsg::ClipDeleted(Ok(())) => {
            Task::done(Message::Soundboard(SoundboardMsg::LoadRequested))
        }
        SoundboardMsg::ClipDeleted(Err(e)) => {
            state.error = Some(e);
            Task::none()
        }
        SoundboardMsg::HotkeyPressed(key) => {
            let Some(clip_id) = state
                .clips
                .iter()
                .find(|c| c.hotkey.as_deref() == Some(key.as_str()))
                .map(|c| c.id)
            else {
                return Task::none();
            };
            Task::done(Message::Soundboard(SoundboardMsg::PlayClip(clip_id)))
        }
    }
}

fn add_btn_style(palette: &ForgePalette, status: iced::widget::button::Status) -> button::Style {
    let bg = match status {
        iced::widget::button::Status::Hovered => palette.brand,
        _ => palette.surface_overlay,
    };
    let text_color = match status {
        iced::widget::button::Status::Hovered => palette.shell,
        _ => palette.brand,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            color: palette.brand,
            width: BORDER_THIN,
            radius: radius(Radius::Md).into(),
        },
        text_color,
        shadow: iced::Shadow::default(),
        snap: false,
    }
}

fn modal_backdrop_style(_theme: &iced::Theme) -> button::Style {
    button::Style {
        background: Some(Background::Color(iced::Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.55,
        })),
        border: Border::default(),
        text_color: iced::Color::TRANSPARENT,
        shadow: iced::Shadow::default(),
        snap: false,
    }
}

fn modal_card_style(palette: &ForgePalette) -> impl Fn(&iced::Theme) -> container::Style + '_ {
    let p = *palette;
    move |_| container::Style {
        background: Some(Background::Color(p.elevated)),
        border: Border {
            color: p.border_input,
            width: BORDER_THIN,
            radius: radius(Radius::Md).into(),
        },
        ..container::Style::default()
    }
}

fn add_clip_modal<'a>(modal: &'a AddClipModal, palette: &'a ForgePalette) -> Element<'a, Message> {
    let gap_md = spf(Spacing::Sm);
    let gap_lg = spf(Spacing::Sm);

    let title = text(if modal.editing_id.is_some() {
        "Edit clip"
    } else {
        "Add clip"
    })
    .size(FONT_SM)
    .color(palette.text_primary)
    .font(font(FontRole::Body));

    let file_label = text(
        modal
            .file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("No file selected"),
    )
    .size(FONT_SM)
    .color(if modal.file_path.is_some() {
        palette.text_secondary
    } else {
        palette.text_muted
    })
    .font(font(FontRole::Monospace))
    .width(Length::Fill);

    let p = *palette;
    let pick_btn = button(
        row![
            tabler_icon(Icon::FolderOpen, 12.0, p.info),
            text("Browse")
                .size(FONT_SM)
                .color(p.info)
                .font(font(FontRole::Body)),
        ]
        .spacing(gap_md)
        .align_y(Alignment::Center),
    )
    .on_press(Message::Soundboard(SoundboardMsg::ModalFilePickRequested))
    .padding([spf(Spacing::Xxs), spf(Spacing::Xs)])
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
            shadow: iced::Shadow::default(),
            snap: false,
        }
    });

    let file_row = row![file_label, pick_btn]
        .spacing(gap_md)
        .align_y(Alignment::Center);

    let name_input = text_input("Clip name", &modal.name)
        .on_input(|s| Message::Soundboard(SoundboardMsg::ModalNameChanged(s)))
        .size(FONT_SM)
        .font(font(FontRole::Body))
        .padding(forge_widgets::input_padding());

    let hotkey_input = text_input("e.g. Ctrl+1", &modal.hotkey)
        .on_input(|s| Message::Soundboard(SoundboardMsg::ModalHotkeyChanged(s)))
        .size(FONT_SM)
        .font(font(FontRole::Monospace))
        .padding(forge_widgets::input_padding());

    let volume_row = forge_widgets::volume_slider(
        modal.volume,
        |v| Message::Soundboard(SoundboardMsg::ModalVolumeChanged(v)),
        palette,
    );

    let device_section = if modal.devices_loading {
        text("Loading devices\u{2026}")
            .size(FONT_SM)
            .color(palette.text_muted)
            .into()
    } else {
        output_device_picker(
            &modal.devices,
            modal.device_choice_idx,
            |idx| Message::Soundboard(SoundboardMsg::ModalDeviceSelected(idx)),
            Message::Soundboard(SoundboardMsg::ModalDevicesLoaded(load_devices_placeholder())),
            Message::Noop,
            palette,
        )
    };

    let error_el: Option<Element<'a, Message>> = modal.error.as_ref().map(|e| {
        text(e.as_str())
            .size(FONT_SM)
            .color(palette.random)
            .font(font(FontRole::Body))
            .into()
    });

    let p2 = *palette;
    let save_btn = button(
        text(if modal.saving {
            "Saving\u{2026}"
        } else {
            "Save"
        })
        .size(FONT_SM)
        .color(p2.shell)
        .font(font(FontRole::Body)),
    )
    .on_press_maybe(if modal.is_valid() && !modal.saving {
        Some(Message::Soundboard(SoundboardMsg::ModalSave))
    } else {
        None
    })
    .padding([spf(Spacing::Xs), spf(Spacing::Md)])
    .style(move |_theme, _status| button::Style {
        background: Some(Background::Color(if modal.is_valid() && !modal.saving {
            p2.brand
        } else {
            p2.surface_overlay
        })),
        border: Border {
            color: p2.border_regular,
            width: 0.5,
            radius: radius(Radius::Md).into(),
        },
        text_color: p2.shell,
        shadow: iced::Shadow::default(),
        snap: false,
    });

    let p3 = *palette;
    let cancel_btn = button(
        text("Cancel")
            .size(FONT_SM)
            .color(p3.text_secondary)
            .font(font(FontRole::Body)),
    )
    .on_press(Message::Soundboard(SoundboardMsg::ModalCancel))
    .padding([spf(Spacing::Xs), spf(Spacing::Md)])
    .style(move |_theme, status| button::Style {
        background: if matches!(status, iced::widget::button::Status::Hovered) {
            Some(Background::Color(p3.surface_overlay))
        } else {
            None
        },
        border: Border {
            color: p3.border_regular,
            width: 0.5,
            radius: radius(Radius::Md).into(),
        },
        text_color: p3.text_secondary,
        shadow: iced::Shadow::default(),
        snap: false,
    });

    let footer = row![
        iced::widget::Space::new().width(Length::Fill),
        cancel_btn,
        save_btn,
    ]
    .spacing(gap_md)
    .align_y(Alignment::Center);

    let mut form_col = column![
        text("FILE")
            .size(FONT_XS)
            .color(palette.text_muted)
            .font(font(FontRole::Monospace)),
        file_row,
        text("NAME")
            .size(FONT_XS)
            .color(palette.text_muted)
            .font(font(FontRole::Monospace)),
        name_input,
        text("HOTKEY")
            .size(FONT_XS)
            .color(palette.text_muted)
            .font(font(FontRole::Monospace)),
        hotkey_input,
        text("OUTPUT DEVICE")
            .size(FONT_XS)
            .color(palette.text_muted)
            .font(font(FontRole::Monospace)),
        device_section,
        text("VOLUME")
            .size(FONT_XS)
            .color(palette.text_muted)
            .font(font(FontRole::Monospace)),
        volume_row,
    ]
    .spacing(gap_lg);

    if let Some(err) = error_el {
        form_col = form_col.push(err);
    }

    let card_content = column![
        row![title, iced::widget::Space::new().width(Length::Fill),].align_y(Alignment::Center),
        form_col,
        footer,
    ]
    .spacing(spf(Spacing::Md));

    let card = container(card_content)
        .width(480.0)
        .padding(spf(Spacing::Md))
        .style(modal_card_style(palette));

    let backdrop = button(iced::widget::Space::new())
        .width(Length::Fill)
        .height(Length::Fill)
        .on_press(Message::Soundboard(SoundboardMsg::ModalCancel))
        .style(|_theme, _status| modal_backdrop_style(_theme));

    let centered = container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center);

    iced::widget::stack![backdrop, centered]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn load_devices_placeholder() -> Result<Vec<DeviceLabel>, String> {
    Ok(Vec::new())
}

pub fn soundboard_view<'a>(
    state: &'a SoundboardState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let gap_lg = spf(Spacing::Sm);
    let gap_xl = spf(Spacing::Md);

    let p = *palette;

    let add_btn = button(
        row![
            tabler_icon(Icon::Plus, 12.0, p.brand),
            text("Add clip")
                .size(FONT_SM)
                .color(p.brand)
                .font(font(FontRole::Body)),
        ]
        .spacing(gap_lg)
        .align_y(Alignment::Center),
    )
    .on_press(Message::Soundboard(SoundboardMsg::OpenAddModal))
    .padding([spf(Spacing::Xxs), spf(Spacing::Sm)])
    .style(move |_theme, status| add_btn_style(&p, status));

    let _ = gap_xl;

    let body: Element<'a, Message> = if state.loading {
        container(
            text("Loading clips\u{2026}")
                .size(FONT_SM)
                .color(palette.text_muted),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
    } else if let Some(ref e) = state.error {
        container(text(e.as_str()).size(FONT_SM).color(palette.random))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into()
    } else if state.clips.is_empty() {
        container(
            column![
                tabler_icon(Icon::Music, 24.0, palette.text_faint),
                text("No clips yet").size(FONT_SM).color(palette.text_muted),
                text("Click \u{201c}Add clip\u{201d} to add your first sound.")
                    .size(FONT_SM)
                    .color(palette.text_faint),
            ]
            .spacing(gap_lg)
            .align_x(Alignment::Center),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
    } else {
        let mut grid_rows: Vec<Element<'a, Message>> = Vec::new();
        let mut current_row: Vec<Element<'a, Message>> = Vec::new();

        for (i, (clip, card_data)) in state.clips.iter().zip(state.card_data.iter()).enumerate() {
            let clip_id = clip.id;
            let card_el = clip_card(
                card_data,
                Message::Soundboard(SoundboardMsg::PlayClip(clip_id)),
                Message::Soundboard(SoundboardMsg::OpenEditModal(clip_id)),
                Message::Soundboard(SoundboardMsg::DeleteClip(clip_id)),
                palette,
            );
            current_row.push(card_el);

            if current_row.len() == 3 || i == state.clips.len() - 1 {
                while current_row.len() < 3 {
                    current_row.push(
                        container(iced::widget::Space::new())
                            .width(Length::Fill)
                            .into(),
                    );
                }
                let row_el = row(current_row).spacing(gap_xl).into();
                grid_rows.push(row_el);
                current_row = Vec::new();
            }
        }

        let grid = column(grid_rows).spacing(gap_xl).width(Length::Fill);

        scrollable(
            container(grid)
                .padding(spf(Spacing::Md))
                .width(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    };

    let play_error_banner: Option<Element<'a, Message>> = state.play_error.as_ref().map(|e| {
        container(
            text(format!("Playback error: {e}"))
                .size(FONT_SM)
                .color(palette.random),
        )
        .width(Length::Fill)
        .padding([spf(Spacing::Xs), spf(Spacing::Md)])
        .style(move |_| container::Style {
            background: Some(Background::Color(iced::Color {
                a: 0.08,
                ..p.random
            })),
            ..container::Style::default()
        })
        .into()
    });

    let page_header = crate::page_chrome::page_header_with_actions(
        &[
            ("Builtin".to_owned(), false),
            ("Soundboard".to_owned(), true),
        ],
        Some(add_btn.into()),
        palette,
    );

    let mut main_col = column![page_header];
    if let Some(banner) = play_error_banner {
        main_col = main_col.push(banner);
    }
    main_col = main_col.push(body);

    let main_view = container(main_col)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();

    if let Some(modal) = state.add_modal.as_ref() {
        let modal_el = add_clip_modal(modal, palette);
        iced::widget::stack![main_view, modal_el]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        main_view
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soundboard_state_new_is_empty() {
        let s = SoundboardState::new();
        assert!(s.clips.is_empty());
        assert!(s.card_data.is_empty());
        assert!(!s.loading);
        assert!(s.error.is_none());
    }

    #[test]
    fn add_clip_modal_new_create_mode() {
        let m = AddClipModal::new(None);
        assert!(m.editing_id.is_none());
        assert!(!m.is_valid());
    }

    #[test]
    fn add_clip_modal_valid_when_name_and_path_set() {
        let mut m = AddClipModal::new(None);
        m.name = "horn".to_string();
        m.file_path = Some(PathBuf::from("/tmp/horn.wav"));
        assert!(m.is_valid());
    }

    #[test]
    fn device_display_label_variants() {
        assert_eq!(device_display_label(&OutputDevice::Default), "default");
        assert_eq!(
            device_display_label(&OutputDevice::ByName {
                name: "Speakers".into()
            }),
            "Speakers"
        );
        assert_eq!(
            device_display_label(&OutputDevice::ById {
                id: "dev-42".into()
            }),
            "dev-42"
        );
    }
}
