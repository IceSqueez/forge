use std::sync::Arc;

use forge_storage::CredentialId;
use forge_tts_cloud::azure::AzureEngineFactory;
use forge_tts_cloud::credentials::{
    AZURE_CREDENTIAL_ID, AzureCredentials, ELEVENLABS_CREDENTIAL_ID, ElevenLabsCredentials,
    OPENAI_CREDENTIAL_ID, OpenAiCredentials, POLLY_CREDENTIAL_ID, PollyCredentials,
};
use forge_tts_cloud::elevenlabs::ElevenLabsEngineFactory;
use forge_tts_cloud::openai::OpenAiEngineFactory;
use forge_tts_cloud::polly::PollyEngineFactory;
use forge_tts_core::TtsEngineFactory;
use forge_widgets::tokens::{
    BORDER_THIN, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, sp, spf,
};
use forge_widgets::{ForgePalette, ToastKind};
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Background, Border, Color, Element, Length, Task};

use crate::Message;
use crate::message::{CloudTtsEnginesMsg, ToastMsg, TtsMsg};
use crate::runtime_view::RuntimeView;

pub use crate::message::CloudEngineKind;

#[derive(Debug, Clone, PartialEq)]
pub enum TestStatus {
    Idle,
    Testing,
    Ok,
    Err(String),
}

pub struct AzureForm {
    pub api_key: String,
    pub region: String,
    pub is_dirty: bool,
    pub test_status: TestStatus,
}

pub struct ElevenLabsForm {
    pub api_key: String,
    pub is_dirty: bool,
    pub test_status: TestStatus,
}

pub struct OpenAiForm {
    pub api_key: String,
    pub is_dirty: bool,
    pub test_status: TestStatus,
}

pub struct PollyForm {
    pub access_key: String,
    pub secret_key: String,
    pub region: String,
    pub is_dirty: bool,
    pub test_status: TestStatus,
}

pub struct CloudTtsEnginesState {
    pub azure: AzureForm,
    pub elevenlabs: ElevenLabsForm,
    pub openai: OpenAiForm,
    pub polly: PollyForm,
}

impl Default for CloudTtsEnginesState {
    fn default() -> Self {
        Self {
            azure: AzureForm {
                api_key: String::new(),
                region: String::new(),
                is_dirty: false,
                test_status: TestStatus::Idle,
            },
            elevenlabs: ElevenLabsForm {
                api_key: String::new(),
                is_dirty: false,
                test_status: TestStatus::Idle,
            },
            openai: OpenAiForm {
                api_key: String::new(),
                is_dirty: false,
                test_status: TestStatus::Idle,
            },
            polly: PollyForm {
                access_key: String::new(),
                secret_key: String::new(),
                region: String::new(),
                is_dirty: false,
                test_status: TestStatus::Idle,
            },
        }
    }
}

pub fn update(
    state: &mut CloudTtsEnginesState,
    rt: &RuntimeView,
    msg: CloudTtsEnginesMsg,
) -> Task<Message> {
    match msg {
        CloudTtsEnginesMsg::ApiKeyChanged(kind, val) => {
            match kind {
                CloudEngineKind::Azure => {
                    state.azure.api_key = val;
                    state.azure.is_dirty = true;
                }
                CloudEngineKind::ElevenLabs => {
                    state.elevenlabs.api_key = val;
                    state.elevenlabs.is_dirty = true;
                }
                CloudEngineKind::OpenAI => {
                    state.openai.api_key = val;
                    state.openai.is_dirty = true;
                }
                CloudEngineKind::Polly => {
                    state.polly.access_key = val;
                    state.polly.is_dirty = true;
                }
            }
            Task::none()
        }

        CloudTtsEnginesMsg::RegionChanged(kind, val) => {
            match kind {
                CloudEngineKind::Azure => {
                    state.azure.region = val;
                    state.azure.is_dirty = true;
                }
                CloudEngineKind::Polly => {
                    state.polly.region = val;
                    state.polly.is_dirty = true;
                }
                _ => {}
            }
            Task::none()
        }

        CloudTtsEnginesMsg::PollySecretKeyChanged(val) => {
            state.polly.secret_key = val;
            state.polly.is_dirty = true;
            Task::none()
        }

        CloudTtsEnginesMsg::SavePressed(kind) => {
            let backend = Arc::clone(&rt.backend);
            match kind {
                CloudEngineKind::Azure => {
                    if state.azure.api_key.is_empty() || state.azure.region.is_empty() {
                        return Task::none();
                    }
                    let creds = AzureCredentials {
                        api_key: state.azure.api_key.clone(),
                        region: state.azure.region.clone(),
                        base_url: None,
                    };
                    Task::perform(
                        async move {
                            let json = serde_json::to_string(&creds).map_err(|e| e.to_string())?;
                            backend
                                .store(&CredentialId::new(AZURE_CREDENTIAL_ID), &json)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |r| {
                            Message::Tts(TtsMsg::CloudEngines(CloudTtsEnginesMsg::Saved(
                                CloudEngineKind::Azure,
                                r,
                            )))
                        },
                    )
                }
                CloudEngineKind::ElevenLabs => {
                    if state.elevenlabs.api_key.is_empty() {
                        return Task::none();
                    }
                    let creds = ElevenLabsCredentials {
                        api_key: state.elevenlabs.api_key.clone(),
                        base_url: None,
                    };
                    Task::perform(
                        async move {
                            let json = serde_json::to_string(&creds).map_err(|e| e.to_string())?;
                            backend
                                .store(&CredentialId::new(ELEVENLABS_CREDENTIAL_ID), &json)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |r| {
                            Message::Tts(TtsMsg::CloudEngines(CloudTtsEnginesMsg::Saved(
                                CloudEngineKind::ElevenLabs,
                                r,
                            )))
                        },
                    )
                }
                CloudEngineKind::OpenAI => {
                    if state.openai.api_key.is_empty() {
                        return Task::none();
                    }
                    let creds = OpenAiCredentials {
                        api_key: state.openai.api_key.clone(),
                        base_url: None,
                    };
                    Task::perform(
                        async move {
                            let json = serde_json::to_string(&creds).map_err(|e| e.to_string())?;
                            backend
                                .store(&CredentialId::new(OPENAI_CREDENTIAL_ID), &json)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |r| {
                            Message::Tts(TtsMsg::CloudEngines(CloudTtsEnginesMsg::Saved(
                                CloudEngineKind::OpenAI,
                                r,
                            )))
                        },
                    )
                }
                CloudEngineKind::Polly => {
                    if state.polly.access_key.is_empty()
                        || state.polly.secret_key.is_empty()
                        || state.polly.region.is_empty()
                    {
                        return Task::none();
                    }
                    let creds = PollyCredentials {
                        access_key_id: state.polly.access_key.clone(),
                        secret_access_key: state.polly.secret_key.clone(),
                        region: state.polly.region.clone(),
                        base_url: None,
                    };
                    Task::perform(
                        async move {
                            let json = serde_json::to_string(&creds).map_err(|e| e.to_string())?;
                            backend
                                .store(&CredentialId::new(POLLY_CREDENTIAL_ID), &json)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |r| {
                            Message::Tts(TtsMsg::CloudEngines(CloudTtsEnginesMsg::Saved(
                                CloudEngineKind::Polly,
                                r,
                            )))
                        },
                    )
                }
            }
        }

        CloudTtsEnginesMsg::Saved(kind, Ok(())) => {
            let name = kind.display_name();
            match kind {
                CloudEngineKind::Azure => state.azure.is_dirty = false,
                CloudEngineKind::ElevenLabs => state.elevenlabs.is_dirty = false,
                CloudEngineKind::OpenAI => state.openai.is_dirty = false,
                CloudEngineKind::Polly => state.polly.is_dirty = false,
            }
            Task::done(Message::Toast(ToastMsg::Fired {
                kind: ToastKind::Info,
                message: forge_widgets::tr!("tts_cloud_saved_toast", name = name),
                duration_ms: 6000,
            }))
        }

        CloudTtsEnginesMsg::Saved(kind, Err(e)) => {
            let name = kind.display_name();
            Task::done(Message::Toast(ToastMsg::Fired {
                kind: ToastKind::Error,
                message: forge_widgets::tr!(
                    "tts_cloud_save_failed_toast",
                    name = name,
                    error = e.as_str()
                ),
                duration_ms: 8000,
            }))
        }

        CloudTtsEnginesMsg::TestPressed(kind) => match kind {
            CloudEngineKind::Azure => {
                if state.azure.api_key.is_empty() || state.azure.region.is_empty() {
                    return Task::none();
                }
                state.azure.test_status = TestStatus::Testing;
                let creds = AzureCredentials {
                    api_key: state.azure.api_key.clone(),
                    region: state.azure.region.clone(),
                    base_url: None,
                };
                Task::perform(
                    async move {
                        AzureEngineFactory::new(creds)
                            .create()
                            .map_err(|e| e.to_string())?
                            .test_connection()
                            .await
                            .map_err(|e| truncate_err(e.to_string()))
                    },
                    |r| {
                        Message::Tts(TtsMsg::CloudEngines(CloudTtsEnginesMsg::Tested(
                            CloudEngineKind::Azure,
                            r,
                        )))
                    },
                )
            }
            CloudEngineKind::ElevenLabs => {
                if state.elevenlabs.api_key.is_empty() {
                    return Task::none();
                }
                state.elevenlabs.test_status = TestStatus::Testing;
                let creds = ElevenLabsCredentials {
                    api_key: state.elevenlabs.api_key.clone(),
                    base_url: None,
                };
                Task::perform(
                    async move {
                        ElevenLabsEngineFactory::new(creds)
                            .create()
                            .map_err(|e| e.to_string())?
                            .test_connection()
                            .await
                            .map_err(|e| truncate_err(e.to_string()))
                    },
                    |r| {
                        Message::Tts(TtsMsg::CloudEngines(CloudTtsEnginesMsg::Tested(
                            CloudEngineKind::ElevenLabs,
                            r,
                        )))
                    },
                )
            }
            CloudEngineKind::OpenAI => {
                if state.openai.api_key.is_empty() {
                    return Task::none();
                }
                state.openai.test_status = TestStatus::Testing;
                let creds = OpenAiCredentials {
                    api_key: state.openai.api_key.clone(),
                    base_url: None,
                };
                Task::perform(
                    async move {
                        OpenAiEngineFactory::new(creds)
                            .create()
                            .map_err(|e| e.to_string())?
                            .test_connection()
                            .await
                            .map_err(|e| truncate_err(e.to_string()))
                    },
                    |r| {
                        Message::Tts(TtsMsg::CloudEngines(CloudTtsEnginesMsg::Tested(
                            CloudEngineKind::OpenAI,
                            r,
                        )))
                    },
                )
            }
            CloudEngineKind::Polly => {
                if state.polly.access_key.is_empty()
                    || state.polly.secret_key.is_empty()
                    || state.polly.region.is_empty()
                {
                    return Task::none();
                }
                state.polly.test_status = TestStatus::Testing;
                let creds = PollyCredentials {
                    access_key_id: state.polly.access_key.clone(),
                    secret_access_key: state.polly.secret_key.clone(),
                    region: state.polly.region.clone(),
                    base_url: None,
                };
                Task::perform(
                    async move {
                        PollyEngineFactory::new(creds)
                            .create()
                            .map_err(|e| e.to_string())?
                            .test_connection()
                            .await
                            .map_err(|e| truncate_err(e.to_string()))
                    },
                    |r| {
                        Message::Tts(TtsMsg::CloudEngines(CloudTtsEnginesMsg::Tested(
                            CloudEngineKind::Polly,
                            r,
                        )))
                    },
                )
            }
        },

        CloudTtsEnginesMsg::Tested(kind, result) => {
            let status = match result {
                Ok(()) => TestStatus::Ok,
                Err(e) => TestStatus::Err(e),
            };
            match kind {
                CloudEngineKind::Azure => state.azure.test_status = status,
                CloudEngineKind::ElevenLabs => state.elevenlabs.test_status = status,
                CloudEngineKind::OpenAI => state.openai.test_status = status,
                CloudEngineKind::Polly => state.polly.test_status = status,
            }
            Task::none()
        }
    }
}

pub fn view<'a>(
    state: &'a CloudTtsEnginesState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let header = text(forge_widgets::tr!("tts_cloud_header"))
        .size(FONT_XS)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace));

    let cards = column![
        azure_card(&state.azure, palette),
        elevenlabs_card(&state.elevenlabs, palette),
        openai_card(&state.openai, palette),
        polly_card(&state.polly, palette),
    ]
    .spacing(spf(Spacing::Sm));

    scrollable(
        column![header, cards]
            .spacing(spf(Spacing::Sm))
            .padding([sp(Spacing::Md), sp(Spacing::Md)]),
    )
    .height(Length::Fill)
    .into()
}

fn azure_card<'a>(form: &'a AzureForm, palette: &'a ForgePalette) -> Element<'a, Message> {
    let can_save = form.is_dirty && !form.api_key.is_empty() && !form.region.is_empty();
    let can_test = !form.api_key.is_empty()
        && !form.region.is_empty()
        && form.test_status != TestStatus::Testing;

    let fields =
        column![
            labeled_field(
                forge_widgets::tr!("tts_cloud_field_api_key"),
                text_input(
                    &forge_widgets::tr!("tts_cloud_field_placeholder_subscription_key"),
                    &form.api_key
                )
                .secure(true)
                .on_input(|v| Message::Tts(TtsMsg::CloudEngines(
                    CloudTtsEnginesMsg::ApiKeyChanged(CloudEngineKind::Azure, v)
                )))
                .size(FONT_SM)
                .style(move |_, _| input_style(palette)),
                palette,
            ),
            labeled_field(
                forge_widgets::tr!("tts_cloud_field_region"),
                text_input("e.g. eastus", &form.region)
                    .on_input(|v| Message::Tts(TtsMsg::CloudEngines(
                        CloudTtsEnginesMsg::RegionChanged(CloudEngineKind::Azure, v)
                    )))
                    .size(FONT_SM)
                    .style(move |_, _| input_style(palette)),
                palette,
            ),
        ]
        .spacing(spf(Spacing::Xs));

    engine_card(
        "Azure Speech",
        palette.info,
        &form.test_status,
        &form.api_key,
        fields.into(),
        CloudEngineKind::Azure,
        can_save,
        can_test,
        palette,
    )
}

fn elevenlabs_card<'a>(
    form: &'a ElevenLabsForm,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let can_save = form.is_dirty && !form.api_key.is_empty();
    let can_test = !form.api_key.is_empty() && form.test_status != TestStatus::Testing;

    let fields =
        column![labeled_field(
            forge_widgets::tr!("tts_cloud_field_api_key"),
            text_input("xi-api-key", &form.api_key)
                .secure(true)
                .on_input(|v| Message::Tts(TtsMsg::CloudEngines(
                    CloudTtsEnginesMsg::ApiKeyChanged(CloudEngineKind::ElevenLabs, v)
                )))
                .size(FONT_SM)
                .style(move |_, _| input_style(palette)),
            palette,
        )]
        .spacing(spf(Spacing::Xs));

    engine_card(
        "ElevenLabs",
        palette.bits,
        &form.test_status,
        &form.api_key,
        fields.into(),
        CloudEngineKind::ElevenLabs,
        can_save,
        can_test,
        palette,
    )
}

fn openai_card<'a>(form: &'a OpenAiForm, palette: &'a ForgePalette) -> Element<'a, Message> {
    let can_save = form.is_dirty && !form.api_key.is_empty();
    let can_test = !form.api_key.is_empty() && form.test_status != TestStatus::Testing;

    let fields =
        column![labeled_field(
            forge_widgets::tr!("tts_cloud_field_api_key"),
            text_input("sk-...", &form.api_key)
                .secure(true)
                .on_input(|v| Message::Tts(TtsMsg::CloudEngines(
                    CloudTtsEnginesMsg::ApiKeyChanged(CloudEngineKind::OpenAI, v)
                )))
                .size(FONT_SM)
                .style(move |_, _| input_style(palette)),
            palette,
        )]
        .spacing(spf(Spacing::Xs));

    engine_card(
        "OpenAI TTS",
        palette.success,
        &form.test_status,
        &form.api_key,
        fields.into(),
        CloudEngineKind::OpenAI,
        can_save,
        can_test,
        palette,
    )
}

fn polly_card<'a>(form: &'a PollyForm, palette: &'a ForgePalette) -> Element<'a, Message> {
    let can_save = form.is_dirty
        && !form.access_key.is_empty()
        && !form.secret_key.is_empty()
        && !form.region.is_empty();
    let can_test = !form.access_key.is_empty()
        && !form.secret_key.is_empty()
        && !form.region.is_empty()
        && form.test_status != TestStatus::Testing;

    let fields = column![
        labeled_field(
            forge_widgets::tr!("tts_cloud_field_access_key_id"),
            text_input("AKIA...", &form.access_key)
                .on_input(|v| Message::Tts(TtsMsg::CloudEngines(
                    CloudTtsEnginesMsg::ApiKeyChanged(CloudEngineKind::Polly, v)
                )))
                .size(FONT_SM)
                .style(move |_, _| input_style(palette)),
            palette,
        ),
        labeled_field(
            forge_widgets::tr!("tts_cloud_field_secret_key"),
            text_input("secret access key", &form.secret_key)
                .secure(true)
                .on_input(|v| Message::Tts(TtsMsg::CloudEngines(
                    CloudTtsEnginesMsg::PollySecretKeyChanged(v)
                )))
                .size(FONT_SM)
                .style(move |_, _| input_style(palette)),
            palette,
        ),
        labeled_field(
            forge_widgets::tr!("tts_cloud_field_region"),
            text_input("e.g. us-east-1", &form.region)
                .on_input(|v| Message::Tts(TtsMsg::CloudEngines(
                    CloudTtsEnginesMsg::RegionChanged(CloudEngineKind::Polly, v)
                )))
                .size(FONT_SM)
                .style(move |_, _| input_style(palette)),
            palette,
        ),
    ]
    .spacing(spf(Spacing::Xs));

    engine_card(
        "Amazon Polly",
        palette.warning,
        &form.test_status,
        &form.access_key,
        fields.into(),
        CloudEngineKind::Polly,
        can_save,
        can_test,
        palette,
    )
}

#[allow(clippy::too_many_arguments)]
fn engine_card<'a>(
    name: &'static str,
    dot_color: Color,
    test_status: &'a TestStatus,
    primary_key: &str,
    fields: Element<'a, Message>,
    kind: CloudEngineKind,
    can_save: bool,
    can_test: bool,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let status_dot = container(text(""))
        .style(move |_| container::Style {
            background: Some(Background::Color(dot_color)),
            border: Border {
                radius: radius(Radius::Pill).into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .width(7)
        .height(7);

    let status_badge = config_status_badge(test_status, primary_key, palette);

    let header = row![
        status_dot,
        text(name)
            .size(FONT_SM)
            .color(palette.text_primary)
            .width(Length::Fill),
        status_badge,
    ]
    .align_y(Alignment::Center)
    .spacing(spf(Spacing::Xs));

    let test_result_row: Option<Element<'a, Message>> = match test_status {
        TestStatus::Ok => Some(
            row![
                container(text(""))
                    .style(move |_| container::Style {
                        background: Some(Background::Color(palette.success)),
                        border: Border {
                            radius: radius(Radius::Pill).into(),
                            ..Border::default()
                        },
                        ..container::Style::default()
                    })
                    .width(6)
                    .height(6),
                text(forge_widgets::tr!("tts_cloud_connection_verified"))
                    .size(FONT_XS)
                    .color(palette.success),
            ]
            .align_y(Alignment::Center)
            .spacing(spf(Spacing::Xs))
            .into(),
        ),
        TestStatus::Err(msg) => Some(
            row![
                container(text(""))
                    .style(move |_| container::Style {
                        background: Some(Background::Color(palette.random)),
                        border: Border {
                            radius: radius(Radius::Pill).into(),
                            ..Border::default()
                        },
                        ..container::Style::default()
                    })
                    .width(6)
                    .height(6),
                text(msg.as_str()).size(FONT_XS).color(palette.random),
            ]
            .align_y(Alignment::Center)
            .spacing(spf(Spacing::Xs))
            .into(),
        ),
        _ => None,
    };

    let test_label = if test_status == &TestStatus::Testing {
        forge_widgets::tr!("tts_cloud_testing_btn")
    } else {
        forge_widgets::tr!("tts_cloud_test_connection_btn")
    };

    let test_btn = if can_test {
        button(
            text(test_label.clone())
                .size(FONT_SM)
                .color(palette.text_muted),
        )
        .on_press(Message::Tts(TtsMsg::CloudEngines(
            CloudTtsEnginesMsg::TestPressed(kind),
        )))
        .style(move |_, _| button::Style {
            background: None,
            border: Border {
                color: palette.border_regular,
                width: BORDER_THIN,
                radius: radius(Radius::Md).into(),
            },
            text_color: palette.text_muted,
            ..button::Style::default()
        })
        .padding([sp(Spacing::Xxs), sp(Spacing::Sm)])
    } else {
        button(
            text(test_label.clone())
                .size(FONT_SM)
                .color(palette.disabled),
        )
        .style(move |_, _| button::Style {
            background: None,
            border: Border {
                color: palette.disabled,
                width: BORDER_THIN,
                radius: radius(Radius::Md).into(),
            },
            text_color: palette.disabled,
            ..button::Style::default()
        })
        .padding([sp(Spacing::Xxs), sp(Spacing::Sm)])
    };

    let save_btn = if can_save {
        button(
            text(forge_widgets::tr!("tts_cloud_save_credentials_btn"))
                .size(FONT_SM)
                .color(palette.text_primary),
        )
        .on_press(Message::Tts(TtsMsg::CloudEngines(
            CloudTtsEnginesMsg::SavePressed(kind),
        )))
        .style(move |_, _| button::Style {
            background: Some(Background::Color(palette.brand)),
            border: Border {
                radius: radius(Radius::Md).into(),
                ..Border::default()
            },
            text_color: palette.shell,
            ..button::Style::default()
        })
        .padding([sp(Spacing::Xxs), sp(Spacing::Sm)])
    } else {
        button(
            text(forge_widgets::tr!("tts_cloud_save_credentials_btn"))
                .size(FONT_SM)
                .color(palette.disabled),
        )
        .style(move |_, _| button::Style {
            background: Some(Background::Color(palette.surface_overlay)),
            border: Border {
                color: palette.disabled,
                width: BORDER_THIN,
                radius: radius(Radius::Md).into(),
            },
            text_color: palette.disabled,
            ..button::Style::default()
        })
        .padding([sp(Spacing::Xxs), sp(Spacing::Sm)])
    };

    let test_el: Element<'a, Message> = test_btn.into();
    let save_el: Element<'a, Message> = save_btn.into();
    let action_row: Element<'a, Message> = row![test_el, iced::widget::Space::new(), save_el,]
        .align_y(Alignment::Center)
        .into();

    let mut body = column![header, fields, action_row].spacing(spf(Spacing::Sm));

    if let Some(result_row) = test_result_row {
        body = body.push(result_row);
    }

    container(body)
        .style(move |_| container::Style {
            background: Some(Background::Color(palette.elevated)),
            border: Border {
                color: palette.border_regular,
                width: BORDER_THIN,
                radius: radius(Radius::Lg).into(),
            },
            ..container::Style::default()
        })
        .padding([sp(Spacing::Sm), sp(Spacing::Md)])
        .width(Length::Fill)
        .into()
}

fn config_status_badge<'a>(
    test_status: &TestStatus,
    primary_key: &str,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let (label, color) = if primary_key.is_empty() {
        (
            forge_widgets::tr!("tts_cloud_not_configured"),
            palette.text_muted,
        )
    } else if matches!(test_status, TestStatus::Err(_)) {
        (
            forge_widgets::tr!("tts_cloud_connection_failed"),
            palette.random,
        )
    } else {
        (forge_widgets::tr!("tts_cloud_configured"), palette.success)
    };

    container(
        text(label)
            .size(FONT_XS)
            .color(color)
            .font(font(FontRole::Monospace)),
    )
    .style(move |_| container::Style {
        background: Some(Background::Color(palette.surface_overlay)),
        border: Border {
            color: palette.border_regular,
            width: BORDER_THIN,
            radius: radius(Radius::Pill).into(),
        },
        ..container::Style::default()
    })
    .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
    .into()
}

fn labeled_field<'a>(
    label: String,
    input: iced::widget::TextInput<'a, Message>,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    row![
        text(label)
            .size(FONT_SM)
            .color(palette.text_muted)
            .width(120),
        input.width(Length::Fill),
    ]
    .align_y(Alignment::Center)
    .spacing(spf(Spacing::Sm))
    .into()
}

fn input_style(palette: &ForgePalette) -> iced::widget::text_input::Style {
    iced::widget::text_input::Style {
        background: Background::Color(palette.shell),
        border: Border {
            color: palette.border_input,
            width: BORDER_THIN,
            radius: radius(Radius::Md).into(),
        },
        icon: palette.text_muted,
        placeholder: palette.text_muted,
        value: palette.text_primary,
        selection: Color {
            a: 0.25,
            ..palette.brand
        },
    }
}

fn truncate_err(s: String) -> String {
    const MAX: usize = 120;
    if s.chars().count() <= MAX {
        s
    } else {
        s.chars().take(MAX).collect::<String>() + "\u{2026}"
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Arc;

    use forge_runtime::{EventBus, NullEventLogRepo, ScriptRegistry};
    use forge_storage::CredentialsRepo;
    use forge_storage_sqlite::SqliteBackend;

    use super::*;
    use crate::runtime_view::RuntimeView;
    use crate::server_subsystem::ServerSubsystem;

    fn rt() -> RuntimeView {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let backend = Arc::new(
            tokio_rt
                .block_on(SqliteBackend::open_with_key("sqlite::memory:", [0xab; 32]))
                .unwrap(),
        );
        let server_subsystem = Arc::new(ServerSubsystem::new(
            Arc::clone(&backend) as Arc<dyn CredentialsRepo>
        ));
        let backend: Arc<dyn forge_storage::DataProvider> = backend;
        RuntimeView {
            actions: Arc::new(forge_runtime::actions::ActionsService::new(
                backend.action_repo(),
                backend.queue_repo(),
                backend.history_repo(),
                backend.trigger_instance_repo(),
                backend.soundboard_clips_repo(),
            )),
            backend,
            bus: EventBus::new(Arc::new(NullEventLogRepo)),
            script_registry: Arc::new(ScriptRegistry::new()),
            server_subsystem,
            action_engine: None,
            scheduler: None,
            obs_client: None,
            vtube_client: None,
            vtube_sink: forge_vtube::SwitchableVTubeSink::new(),
            discord_client: None,
            midi_client: None,
            hotkey_client: None,
            speak_queue: None,
            sound_player: None,
            twitch_chat_handle: None,
            chat_send_bridge: None,
            twitch_flow: None,
            youtube_flow: None,
            kick_flow: None,
            tts_engine_ids: Vec::new(),
            twitch_login: None,
            twitch_token_expires: None,
            twitch_reauth_required: false,
            sub_action_registry: Arc::new(forge_registry::SubActionRegistry::new()),
            trigger_registry: Arc::new(forge_registry::TriggerRegistry::new()),
        }
    }

    #[test]
    fn api_key_changed_sets_is_dirty() {
        let mut state = CloudTtsEnginesState::default();
        let _task = update(
            &mut state,
            &rt(),
            CloudTtsEnginesMsg::ApiKeyChanged(CloudEngineKind::Azure, "test-key".into()),
        );
        assert!(state.azure.is_dirty);
        assert_eq!(state.azure.api_key, "test-key");
    }

    #[test]
    fn elevenlabs_api_key_changed_sets_is_dirty() {
        let mut state = CloudTtsEnginesState::default();
        let _task = update(
            &mut state,
            &rt(),
            CloudTtsEnginesMsg::ApiKeyChanged(CloudEngineKind::ElevenLabs, "xi-key".into()),
        );
        assert!(state.elevenlabs.is_dirty);
        assert_eq!(state.elevenlabs.api_key, "xi-key");
    }

    #[test]
    fn save_with_empty_api_key_is_noop() {
        let mut state = CloudTtsEnginesState::default();
        let _task = update(
            &mut state,
            &rt(),
            CloudTtsEnginesMsg::SavePressed(CloudEngineKind::Azure),
        );
        assert!(!state.azure.is_dirty);
    }

    #[test]
    fn save_with_missing_region_is_noop() {
        let mut state = CloudTtsEnginesState::default();
        state.azure.api_key = "key".into();
        let _task = update(
            &mut state,
            &rt(),
            CloudTtsEnginesMsg::SavePressed(CloudEngineKind::Azure),
        );
        assert!(!state.azure.is_dirty);
    }

    #[test]
    fn test_pressed_sets_testing_status() {
        let mut state = CloudTtsEnginesState::default();
        state.azure.api_key = "key".into();
        state.azure.region = "eastus".into();
        let _task = update(
            &mut state,
            &rt(),
            CloudTtsEnginesMsg::TestPressed(CloudEngineKind::Azure),
        );
        assert_eq!(state.azure.test_status, TestStatus::Testing);
    }

    #[test]
    fn tested_ok_sets_ok_status() {
        let mut state = CloudTtsEnginesState::default();
        state.azure.test_status = TestStatus::Testing;
        let _task = update(
            &mut state,
            &rt(),
            CloudTtsEnginesMsg::Tested(CloudEngineKind::Azure, Ok(())),
        );
        assert_eq!(state.azure.test_status, TestStatus::Ok);
    }

    #[test]
    fn tested_err_sets_error_status() {
        let mut state = CloudTtsEnginesState::default();
        state.azure.test_status = TestStatus::Testing;
        let _task = update(
            &mut state,
            &rt(),
            CloudTtsEnginesMsg::Tested(CloudEngineKind::Azure, Err("auth failed".into())),
        );
        assert!(matches!(state.azure.test_status, TestStatus::Err(_)));
    }

    #[test]
    fn saved_ok_clears_dirty_flag() {
        let mut state = CloudTtsEnginesState::default();
        state.azure.is_dirty = true;
        let _task = update(
            &mut state,
            &rt(),
            CloudTtsEnginesMsg::Saved(CloudEngineKind::Azure, Ok(())),
        );
        assert!(!state.azure.is_dirty);
    }

    #[test]
    fn polly_secret_key_changed_sets_dirty() {
        let mut state = CloudTtsEnginesState::default();
        let _task = update(
            &mut state,
            &rt(),
            CloudTtsEnginesMsg::PollySecretKeyChanged("secret".into()),
        );
        assert!(state.polly.is_dirty);
        assert_eq!(state.polly.secret_key, "secret");
    }

    #[test]
    fn region_changed_sets_dirty_for_azure() {
        let mut state = CloudTtsEnginesState::default();
        let _task = update(
            &mut state,
            &rt(),
            CloudTtsEnginesMsg::RegionChanged(CloudEngineKind::Azure, "westus".into()),
        );
        assert!(state.azure.is_dirty);
        assert_eq!(state.azure.region, "westus");
    }

    #[test]
    fn polly_save_blocked_when_missing_fields() {
        let mut state = CloudTtsEnginesState::default();
        state.polly.access_key = "AKID".into();
        let _task = update(
            &mut state,
            &rt(),
            CloudTtsEnginesMsg::SavePressed(CloudEngineKind::Polly),
        );
        assert!(!state.polly.is_dirty);
    }
}
