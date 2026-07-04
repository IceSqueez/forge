use std::sync::Arc;

use forge_speak_queue::{Priority, RequestId, SpeakCommand, SpeakRequest};
use forge_storage::VoiceAliasRepo;
use forge_voice::{AliasId, AliasState, AssignmentStrategy, EngineId, VoiceAlias, VoiceId};
use forge_widgets::ForgePalette;
use forge_widgets::tokens::{
    BORDER_THIN, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, sp, spf,
};
use iced::widget::{
    Space, button, column, container, pick_list, row, scrollable, text, text_input,
};
use iced::{Alignment, Background, Border, Color, Element, Length, Task};

use crate::Message;
use crate::message::{TtsMsg, VoiceAliasesMsg};
use crate::runtime_view::RuntimeView;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssignmentStrategyChoice {
    DeterministicByName,
    Random,
    SingleVoice,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewerRole {
    Mod,
    Vip,
    Sub,
}

#[derive(Debug, Clone)]
pub struct VoiceAliasRow {
    pub id: AliasId,
    pub viewer_id: String,
    pub viewer_name: String,
    pub engine_id: String,
    pub voice_id: String,
    pub engine_label: String,
    pub voice_label: String,
    pub pitch_semitones: Option<f32>,
    pub rate_multiplier: Option<f32>,
    pub blocked: bool,
    pub role: Option<ViewerRole>,
}

#[derive(Debug, Clone, Default)]
pub struct AliasForm {
    pub editing: Option<AliasId>,
    pub viewer: String,
    pub engine: String,
    pub voice: String,
    pub pitch: String,
    pub rate: String,
    pub saving: bool,
}

pub struct VoiceAliasesState {
    pub strategy: AssignmentStrategyChoice,
    pub search: String,
    pub aliases: Vec<VoiceAliasRow>,
    pub total_count: usize,
    pub form: Option<AliasForm>,
    pub pending_delete: Option<usize>,
}

impl VoiceAliasesState {
    pub fn new() -> Self {
        Self {
            strategy: AssignmentStrategyChoice::DeterministicByName,
            search: String::new(),
            aliases: Vec::new(),
            total_count: 0,
            form: None,
            pending_delete: None,
        }
    }
}

impl Default for VoiceAliasesState {
    fn default() -> Self {
        Self::new()
    }
}

fn reload(rt: &RuntimeView) -> Task<Message> {
    let repo = rt.backend.voice_alias_repo();
    let engine_ids: Vec<EngineId> = rt.tts_engine_ids.clone();
    let aliases_task = Task::perform(load_aliases(repo.clone(), engine_ids), |r| {
        Message::Tts(TtsMsg::Aliases(VoiceAliasesMsg::AliasesLoaded(r)))
    });
    let strategy_task = Task::perform(
        async move { repo.get_strategy().await.map_err(|e| e.to_string()) },
        |r| match r {
            Ok(strategy) => Message::Tts(TtsMsg::Aliases(VoiceAliasesMsg::StrategyLoaded(
                choice_from_strategy(&strategy),
            ))),
            Err(e) => {
                tracing::warn!(error = %e, "voice strategy load failed");
                Message::Noop
            }
        },
    );
    Task::batch([aliases_task, strategy_task])
}

fn choice_from_strategy(strategy: &AssignmentStrategy) -> AssignmentStrategyChoice {
    match strategy {
        AssignmentStrategy::DeterministicByName => AssignmentStrategyChoice::DeterministicByName,
        AssignmentStrategy::Random => AssignmentStrategyChoice::Random,
        AssignmentStrategy::Single { .. } => AssignmentStrategyChoice::SingleVoice,
    }
}

/// `SingleVoice` needs a concrete voice; absent a dedicated picker it binds to the
/// first voice in the live catalog, so it returns `None` when no engine is running.
fn strategy_from_choice(
    choice: &AssignmentStrategyChoice,
    rt: &RuntimeView,
) -> Option<AssignmentStrategy> {
    match choice {
        AssignmentStrategyChoice::DeterministicByName => {
            Some(AssignmentStrategy::DeterministicByName)
        }
        AssignmentStrategyChoice::Random => Some(AssignmentStrategy::Random),
        AssignmentStrategyChoice::SingleVoice => {
            let voices = rt.speak_queue.as_ref()?.available_voices();
            let first = voices.first()?;
            Some(AssignmentStrategy::Single {
                voice_id: first.id.clone(),
                engine_id: first.engine_id.clone(),
            })
        }
    }
}

pub async fn load_aliases(
    repo: Arc<dyn VoiceAliasRepo>,
    engine_ids: Vec<EngineId>,
) -> Result<Vec<VoiceAliasRow>, String> {
    let aliases = repo.list().await.map_err(|e| e.to_string())?;
    Ok(aliases
        .into_iter()
        .map(|a| row_from_alias(a, &engine_ids))
        .collect())
}

fn row_from_alias(a: VoiceAlias, engine_ids: &[EngineId]) -> VoiceAliasRow {
    let engine_known = engine_ids.iter().any(|e| e == &a.engine_id);
    let engine_label = if engine_known {
        engine_display_label(&a.engine_id.0)
    } else {
        a.engine_id.0.clone()
    };
    VoiceAliasRow {
        id: a.id,
        viewer_id: a.viewer_id,
        viewer_name: a.viewer_name,
        engine_id: a.engine_id.0,
        voice_id: a.voice_id.0.clone(),
        engine_label,
        voice_label: a.voice_id.0,
        pitch_semitones: a.pitch_semitones,
        rate_multiplier: a.rate_multiplier,
        blocked: matches!(a.state, AliasState::Blocked),
        role: None,
    }
}

fn engine_display_label(engine_id: &str) -> String {
    match engine_id {
        "piper" => "Piper".to_owned(),
        "espeak" | "espeak-ng" => "eSpeak-NG".to_owned(),
        "sapi" => "SAPI 5".to_owned(),
        "avfoundation" => "AVFoundation".to_owned(),
        other => other.to_owned(),
    }
}

fn form_to_alias(form: &AliasForm) -> VoiceAlias {
    let pitch = form.pitch.trim().parse::<f32>().ok();
    let rate = form.rate.trim().parse::<f32>().ok();
    VoiceAlias {
        id: form.editing.clone().unwrap_or_default(),
        viewer_id: form.viewer.trim().to_owned(),
        viewer_name: form.viewer.trim().to_owned(),
        engine_id: EngineId(form.engine.trim().to_owned()),
        voice_id: VoiceId(form.voice.trim().to_owned()),
        pitch_semitones: pitch,
        rate_multiplier: rate,
        state: AliasState::Active,
    }
}

fn fmt_opt(v: Option<f32>) -> String {
    v.map(|n| n.to_string()).unwrap_or_default()
}

pub fn update(
    state: &mut VoiceAliasesState,
    rt: &RuntimeView,
    msg: VoiceAliasesMsg,
) -> Task<Message> {
    match msg {
        VoiceAliasesMsg::SearchChanged(s) => {
            state.search = s;
            Task::none()
        }
        VoiceAliasesMsg::StrategyChanged(choice) => {
            state.strategy = choice.clone();
            let Some(strategy) = strategy_from_choice(&choice, rt) else {
                return Task::none();
            };
            let repo = rt.backend.voice_alias_repo();
            let persist = {
                let strategy = strategy.clone();
                Task::perform(
                    async move {
                        repo.set_strategy(&strategy)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    |r| {
                        if let Err(e) = r {
                            tracing::warn!(error = %e, "voice strategy persist failed");
                        }
                        Message::Noop
                    },
                )
            };
            let push = match rt.speak_queue.clone() {
                Some(handle) => Task::perform(
                    async move {
                        handle
                            .send(SpeakCommand::SetStrategy(strategy))
                            .await
                            .map_err(|e| e.to_string())
                    },
                    |r| {
                        if let Err(e) = r {
                            tracing::warn!(error = %e, "voice strategy hot-reload failed");
                        }
                        Message::Noop
                    },
                ),
                None => Task::none(),
            };
            Task::batch([persist, push])
        }
        VoiceAliasesMsg::StrategyLoaded(choice) => {
            state.strategy = choice;
            Task::none()
        }
        VoiceAliasesMsg::LoadRequested => reload(rt),
        VoiceAliasesMsg::AliasesLoaded(Ok(rows)) => {
            state.total_count = rows.len();
            state.aliases = rows;
            Task::none()
        }
        VoiceAliasesMsg::AliasesLoaded(Err(e)) => {
            tracing::warn!(error = %e, "voice aliases load failed");
            Task::none()
        }
        VoiceAliasesMsg::PlayPreview(index) => {
            let Some(alias) = state.aliases.get(index) else {
                return Task::none();
            };
            if alias.blocked {
                return Task::none();
            }
            let Some(handle) = rt.speak_queue.clone() else {
                return Task::none();
            };
            let viewer_id = alias.viewer_id.clone();
            let viewer_name = alias.viewer_name.clone();
            Task::perform(
                async move {
                    let request = SpeakRequest {
                        request_id: RequestId::new(),
                        viewer_id,
                        viewer_name,
                        text: forge_widgets::tr!("tts_aliases_preview_text"),
                        priority: Priority::Normal,
                        alias_override: None,
                        engine_override: None,
                        voice_override: None,
                        source_event_id: forge_types::EventId::new(),
                    };
                    handle
                        .send(SpeakCommand::Enqueue(request))
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| {
                    if let Err(e) = r {
                        tracing::warn!(error = %e, "voice alias preview failed");
                    }
                    Message::Noop
                },
            )
        }
        VoiceAliasesMsg::Assign => {
            state.form = Some(AliasForm::default());
            Task::none()
        }
        VoiceAliasesMsg::Edit(index) => {
            if let Some(alias) = state.aliases.get(index) {
                state.form = Some(AliasForm {
                    editing: Some(alias.id.clone()),
                    viewer: alias.viewer_name.clone(),
                    engine: alias.engine_id.clone(),
                    voice: alias.voice_id.clone(),
                    pitch: fmt_opt(alias.pitch_semitones),
                    rate: fmt_opt(alias.rate_multiplier),
                    saving: false,
                });
            }
            Task::none()
        }
        VoiceAliasesMsg::FormViewerChanged(v) => {
            if let Some(form) = state.form.as_mut() {
                form.viewer = v;
            }
            Task::none()
        }
        VoiceAliasesMsg::FormEngineChanged(v) => {
            if let Some(form) = state.form.as_mut() {
                form.engine = v;
            }
            Task::none()
        }
        VoiceAliasesMsg::FormVoiceChanged(v) => {
            if let Some(form) = state.form.as_mut() {
                form.voice = v;
            }
            Task::none()
        }
        VoiceAliasesMsg::FormPitchChanged(v) => {
            if let Some(form) = state.form.as_mut() {
                form.pitch = v;
            }
            Task::none()
        }
        VoiceAliasesMsg::FormRateChanged(v) => {
            if let Some(form) = state.form.as_mut() {
                form.rate = v;
            }
            Task::none()
        }
        VoiceAliasesMsg::FormCancel => {
            state.form = None;
            Task::none()
        }
        VoiceAliasesMsg::FormSubmit => {
            let Some(form) = state.form.as_mut() else {
                return Task::none();
            };
            if form.viewer.trim().is_empty() {
                return Task::none();
            }
            form.saving = true;
            let alias = form_to_alias(form);
            let repo = rt.backend.voice_alias_repo();
            let handle = rt.speak_queue.clone();
            Task::perform(
                async move {
                    repo.upsert(&alias).await.map_err(|e| e.to_string())?;
                    if let Some(handle) = handle
                        && let Err(e) = handle.send(SpeakCommand::SetAlias(alias)).await
                    {
                        tracing::warn!(error = %e, "voice alias hot-reload failed");
                    }
                    Ok::<(), String>(())
                },
                |r| Message::Tts(TtsMsg::Aliases(VoiceAliasesMsg::FormSubmitResult(r))),
            )
        }
        VoiceAliasesMsg::FormSubmitResult(Ok(())) => {
            state.form = None;
            reload(rt)
        }
        VoiceAliasesMsg::FormSubmitResult(Err(e)) => {
            if let Some(form) = state.form.as_mut() {
                form.saving = false;
            }
            tracing::warn!(error = %e, "voice alias upsert failed");
            Task::none()
        }
        VoiceAliasesMsg::DeleteRequested(index) => {
            state.pending_delete = Some(index);
            Task::none()
        }
        VoiceAliasesMsg::DeleteCancel => {
            state.pending_delete = None;
            Task::none()
        }
        VoiceAliasesMsg::DeleteConfirm => {
            let Some(index) = state.pending_delete.take() else {
                return Task::none();
            };
            let Some(alias) = state.aliases.get(index) else {
                return Task::none();
            };
            let id = alias.id.clone();
            let repo = rt.backend.voice_alias_repo();
            let handle = rt.speak_queue.clone();
            Task::perform(
                async move {
                    repo.delete(&id).await.map_err(|e| e.to_string())?;
                    if let Some(handle) = handle
                        && let Err(e) = handle.send(SpeakCommand::RemoveAlias(id)).await
                    {
                        tracing::warn!(error = %e, "voice alias hot-reload (remove) failed");
                    }
                    Ok::<(), String>(())
                },
                |r| Message::Tts(TtsMsg::Aliases(VoiceAliasesMsg::DeleteResult(r))),
            )
        }
        VoiceAliasesMsg::DeleteResult(Ok(())) => reload(rt),
        VoiceAliasesMsg::DeleteResult(Err(e)) => {
            tracing::warn!(error = %e, "voice alias delete failed");
            Task::none()
        }
    }
}

pub fn voice_aliases_view<'a>(
    state: &'a VoiceAliasesState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let gap_sm = spf(Spacing::Xs);
    let gap_md = spf(Spacing::Sm);
    let gap_lg = spf(Spacing::Sm);

    let strategy_banner = strategy_banner_view(state, palette, gap_sm, gap_md);
    let toolbar = toolbar_view(state, palette, gap_sm);
    let table = aliases_table_view(state, palette, gap_sm);

    let page: Element<'a, Message> = column![strategy_banner, toolbar, table]
        .spacing(gap_lg)
        .padding(iced::Padding {
            top: 0.0,
            right: 0.0,
            bottom: spf(Spacing::Md),
            left: 0.0,
        })
        .into();

    if let Some(form) = &state.form {
        iced::widget::stack![page, alias_form_modal(form, &state.aliases, palette)].into()
    } else if let Some(index) = state.pending_delete {
        let name = state
            .aliases
            .get(index)
            .map(|a| a.viewer_name.clone())
            .unwrap_or_default();
        iced::widget::stack![page, delete_confirm_modal(name, palette)].into()
    } else {
        page
    }
}

fn modal_backdrop<'a>(on_press: Message) -> Element<'a, Message> {
    button(Space::new().width(Length::Fill).height(Length::Fill))
        .on_press(on_press)
        .padding(0)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_, _| button::Style {
            background: Some(Background::Color(Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.5,
            })),
            border: Border::default(),
            text_color: Color::TRANSPARENT,
            shadow: iced::Shadow::default(),
            snap: false,
        })
        .into()
}

fn modal_field<'a>(
    label: String,
    value: &'a str,
    placeholder: String,
    on_change: impl Fn(String) -> Message + 'a,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let input = text_input(&placeholder, value)
        .on_input(on_change)
        .size(FONT_SM)
        .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
        .style(move |_, _| text_input::Style {
            background: Background::Color(palette.shell),
            border: Border {
                color: palette.border_regular,
                width: BORDER_THIN,
                radius: radius(Radius::Sm).into(),
            },
            icon: palette.text_muted,
            placeholder: palette.text_muted,
            value: palette.text_primary,
            selection: palette.brand,
        });
    column![
        text(label)
            .size(FONT_XS)
            .color(palette.text_muted)
            .font(font(FontRole::Monospace)),
        input,
    ]
    .spacing(spf(Spacing::Xxs))
    .into()
}

fn alias_form_modal<'a>(
    form: &'a AliasForm,
    rows: &'a [VoiceAliasRow],
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;
    let backdrop = modal_backdrop(Message::Tts(TtsMsg::Aliases(VoiceAliasesMsg::FormCancel)));

    let title_key = if form.editing.is_some() {
        "tts_aliases_form_title_edit"
    } else {
        "tts_aliases_form_title_assign"
    };
    let title = text(forge_widgets::tr!(title_key))
        .size(FONT_SM)
        .color(p.text_primary)
        .font(font(FontRole::Body));

    let engines: Vec<String> = {
        let mut seen: Vec<String> = rows
            .iter()
            .map(|r| r.engine_id.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        if !form.engine.is_empty() && !seen.contains(&form.engine) {
            seen.push(form.engine.clone());
        }
        seen
    };
    let selected_engine = if form.engine.is_empty() {
        None
    } else {
        Some(form.engine.clone())
    };
    let engine_picker = pick_list(engines, selected_engine, |v| {
        Message::Tts(TtsMsg::Aliases(VoiceAliasesMsg::FormEngineChanged(v)))
    })
    .placeholder(forge_widgets::tr!("tts_aliases_form_engine_placeholder"))
    .text_size(FONT_SM)
    .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
    .width(Length::Fill);
    let engine_section = column![
        text(forge_widgets::tr!("tts_aliases_form_engine_label"))
            .size(FONT_XS)
            .color(p.text_muted)
            .font(font(FontRole::Monospace)),
        engine_picker,
    ]
    .spacing(spf(Spacing::Xxs));

    let viewer_field = modal_field(
        forge_widgets::tr!("tts_aliases_form_viewer_label"),
        form.viewer.as_str(),
        forge_widgets::tr!("tts_aliases_form_viewer_placeholder"),
        |s| Message::Tts(TtsMsg::Aliases(VoiceAliasesMsg::FormViewerChanged(s))),
        palette,
    );
    let voice_field = modal_field(
        forge_widgets::tr!("tts_aliases_form_voice_label"),
        form.voice.as_str(),
        forge_widgets::tr!("tts_aliases_form_voice_placeholder"),
        |s| Message::Tts(TtsMsg::Aliases(VoiceAliasesMsg::FormVoiceChanged(s))),
        palette,
    );
    let pitch_field = modal_field(
        forge_widgets::tr!("tts_aliases_form_pitch_label"),
        form.pitch.as_str(),
        forge_widgets::tr!("tts_aliases_form_pitch_placeholder"),
        |s| Message::Tts(TtsMsg::Aliases(VoiceAliasesMsg::FormPitchChanged(s))),
        palette,
    );
    let rate_field = modal_field(
        forge_widgets::tr!("tts_aliases_form_rate_label"),
        form.rate.as_str(),
        forge_widgets::tr!("tts_aliases_form_rate_placeholder"),
        |s| Message::Tts(TtsMsg::Aliases(VoiceAliasesMsg::FormRateChanged(s))),
        palette,
    );

    let can_save = !form.viewer.trim().is_empty() && !form.saving;
    let save_key = if form.editing.is_some() {
        "common.save"
    } else {
        "tts_aliases_form_create"
    };
    let save_btn = {
        let b = button(
            text(forge_widgets::tr!(save_key))
                .size(FONT_SM)
                .color(if can_save { p.shell } else { p.text_muted }),
        )
        .padding([sp(Spacing::Xs), sp(Spacing::Md)])
        .style(move |_, _| button::Style {
            background: Some(Background::Color(if can_save {
                p.brand
            } else {
                p.surface_overlay
            })),
            border: Border {
                radius: radius(Radius::Sm).into(),
                ..Border::default()
            },
            text_color: if can_save { p.shell } else { p.text_muted },
            ..button::Style::default()
        });
        if can_save {
            b.on_press(Message::Tts(TtsMsg::Aliases(VoiceAliasesMsg::FormSubmit)))
        } else {
            b
        }
    };
    let cancel_btn = button(
        text(forge_widgets::tr!("common.cancel"))
            .size(FONT_SM)
            .color(p.text_secondary),
    )
    .on_press(Message::Tts(TtsMsg::Aliases(VoiceAliasesMsg::FormCancel)))
    .padding([sp(Spacing::Xs), sp(Spacing::Md)])
    .style(move |_, _| button::Style {
        background: None,
        border: Border {
            color: p.border_regular,
            width: BORDER_THIN,
            radius: radius(Radius::Sm).into(),
        },
        text_color: p.text_secondary,
        ..button::Style::default()
    });

    let footer =
        row![cancel_btn, Space::new().width(Length::Fill), save_btn].align_y(Alignment::Center);

    let inner = column![
        title,
        viewer_field,
        engine_section,
        voice_field,
        row![pitch_field, rate_field].spacing(spf(Spacing::Sm)),
        footer,
    ]
    .spacing(spf(Spacing::Sm))
    .padding(sp(Spacing::Md));

    let card = container(inner)
        .max_width(440)
        .style(move |_| container::Style {
            background: Some(Background::Color(p.elevated)),
            border: Border {
                color: p.border_regular,
                width: BORDER_THIN,
                radius: radius(Radius::Lg).into(),
            },
            ..container::Style::default()
        });

    let centered = container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center);

    iced::widget::stack![backdrop, centered].into()
}

fn delete_confirm_modal<'a>(
    viewer_name: String,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;
    let backdrop = modal_backdrop(Message::Tts(TtsMsg::Aliases(VoiceAliasesMsg::DeleteCancel)));

    let title = text(forge_widgets::tr!("tts_aliases_delete_title"))
        .size(FONT_SM)
        .color(p.text_primary)
        .font(font(FontRole::Body));
    let body = text(forge_widgets::tr!(
        "tts_aliases_delete_body",
        viewer = viewer_name
    ))
    .size(FONT_SM)
    .color(p.text_secondary);

    let cancel_btn = button(
        text(forge_widgets::tr!("common.cancel"))
            .size(FONT_SM)
            .color(p.text_secondary),
    )
    .on_press(Message::Tts(TtsMsg::Aliases(VoiceAliasesMsg::DeleteCancel)))
    .padding([sp(Spacing::Xs), sp(Spacing::Md)])
    .style(move |_, _| button::Style {
        background: None,
        border: Border {
            color: p.border_regular,
            width: BORDER_THIN,
            radius: radius(Radius::Sm).into(),
        },
        text_color: p.text_secondary,
        ..button::Style::default()
    });
    let delete_btn = button(
        text(forge_widgets::tr!("common.delete"))
            .size(FONT_SM)
            .color(p.shell),
    )
    .on_press(Message::Tts(TtsMsg::Aliases(
        VoiceAliasesMsg::DeleteConfirm,
    )))
    .padding([sp(Spacing::Xs), sp(Spacing::Md)])
    .style(move |_, _| button::Style {
        background: Some(Background::Color(p.random)),
        border: Border {
            radius: radius(Radius::Sm).into(),
            ..Border::default()
        },
        text_color: p.shell,
        ..button::Style::default()
    });

    let footer =
        row![cancel_btn, Space::new().width(Length::Fill), delete_btn].align_y(Alignment::Center);

    let inner = column![title, body, footer]
        .spacing(spf(Spacing::Md))
        .padding(sp(Spacing::Md));

    let card = container(inner)
        .max_width(380)
        .style(move |_| container::Style {
            background: Some(Background::Color(p.elevated)),
            border: Border {
                color: p.border_regular,
                width: BORDER_THIN,
                radius: radius(Radius::Lg).into(),
            },
            ..container::Style::default()
        });

    let centered = container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center);

    iced::widget::stack![backdrop, centered].into()
}

fn strategy_banner_view<'a>(
    state: &'a VoiceAliasesState,
    palette: &'a ForgePalette,
    gap_sm: f32,
    gap_md: f32,
) -> Element<'a, Message> {
    fn strategy_btn<'b>(
        label: &'static str,
        choice: AssignmentStrategyChoice,
        current: &AssignmentStrategyChoice,
        palette: &'b ForgePalette,
    ) -> Element<'b, Message> {
        let active = &choice == current;
        button(text(label).size(FONT_XS))
            .on_press(Message::Tts(TtsMsg::Aliases(
                VoiceAliasesMsg::StrategyChanged(choice),
            )))
            .style(move |_, _| {
                if active {
                    button::Style {
                        background: Some(Background::Color(palette.brand)),
                        border: Border {
                            radius: radius(Radius::Sm).into(),
                            ..Border::default()
                        },
                        text_color: palette.shell,
                        ..button::Style::default()
                    }
                } else {
                    button::Style {
                        background: None,
                        text_color: palette.text_secondary,
                        ..button::Style::default()
                    }
                }
            })
            .padding([sp(Spacing::Xxs), sp(Spacing::Sm)])
            .into()
    }

    let _ = gap_sm;

    let segmented = container(
        row![
            strategy_btn(
                Box::leak(
                    forge_widgets::tr!("tts_aliases_strategy_deterministic").into_boxed_str()
                ),
                AssignmentStrategyChoice::DeterministicByName,
                &state.strategy,
                palette,
            ),
            strategy_btn(
                Box::leak(forge_widgets::tr!("tts_aliases_strategy_random").into_boxed_str()),
                AssignmentStrategyChoice::Random,
                &state.strategy,
                palette
            ),
            strategy_btn(
                Box::leak(forge_widgets::tr!("tts_aliases_strategy_single").into_boxed_str()),
                AssignmentStrategyChoice::SingleVoice,
                &state.strategy,
                palette,
            ),
        ]
        .spacing(0),
    )
    .style(move |_| container::Style {
        background: Some(Background::Color(palette.shell)),
        border: Border {
            color: palette.border_regular,
            width: BORDER_THIN,
            radius: radius(Radius::Sm).into(),
        },
        ..container::Style::default()
    })
    .padding(sp(Spacing::Xxs));

    let inner = container(
        row![
            text(forge_widgets::tr!("tts_aliases_strategy_label"))
                .size(FONT_SM)
                .color(palette.text_primary)
                .width(Length::Fill),
            segmented,
        ]
        .align_y(Alignment::Center)
        .spacing(gap_md),
    )
    .style(move |_| container::Style {
        background: Some(Background::Color(palette.elevated)),
        border: Border {
            color: palette.border_regular,
            width: BORDER_THIN,
            radius: radius(Radius::Md).into(),
        },
        ..container::Style::default()
    })
    .padding([sp(Spacing::Sm), sp(Spacing::Sm)])
    .width(Length::Fill);

    container(inner)
        .padding([sp(Spacing::Sm), sp(Spacing::Md)])
        .width(Length::Fill)
        .into()
}

fn toolbar_view<'a>(
    state: &'a VoiceAliasesState,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    let search_placeholder = forge_widgets::tr!("tts_aliases_search_placeholder");
    let search = text_input(&search_placeholder, &state.search)
        .on_input(|s| Message::Tts(TtsMsg::Aliases(VoiceAliasesMsg::SearchChanged(s))))
        .size(FONT_SM)
        .width(240)
        .style(move |_, _| text_input::Style {
            background: Background::Color(palette.elevated),
            border: Border {
                color: palette.border_regular,
                width: BORDER_THIN,
                radius: radius(Radius::Sm).into(),
            },
            icon: palette.text_muted,
            placeholder: palette.text_muted,
            value: palette.text_primary,
            selection: palette.brand,
        });

    let count_text = text(forge_widgets::tr!(
        "tts_aliases_count",
        count = state.total_count as i64
    ))
    .size(FONT_SM)
    .color(palette.text_muted);

    let assign_btn = button(text(forge_widgets::tr!("tts_aliases_assign_btn")).size(FONT_SM))
        .on_press(Message::Tts(TtsMsg::Aliases(VoiceAliasesMsg::Assign)))
        .style(move |_, _| button::Style {
            background: Some(Background::Color(palette.brand)),
            border: Border {
                radius: radius(Radius::Sm).into(),
                ..Border::default()
            },
            text_color: palette.shell,
            ..button::Style::default()
        })
        .padding([sp(Spacing::Xs), sp(Spacing::Sm)]);

    container(
        row![
            search,
            row![count_text, assign_btn]
                .align_y(Alignment::Center)
                .spacing(gap_sm),
        ]
        .align_y(Alignment::Center)
        .spacing(gap_sm)
        .width(Length::Fill),
    )
    .padding([0, sp(Spacing::Md)])
    .width(Length::Fill)
    .into()
}

fn aliases_table_view<'a>(
    state: &'a VoiceAliasesState,
    palette: &'a ForgePalette,
    gap_sm: f32,
) -> Element<'a, Message> {
    let mono = font(FontRole::Monospace);

    let header = container(
        row![
            text(forge_widgets::tr!("tts_aliases_col_viewer"))
                .size(FONT_XS)
                .color(palette.text_muted)
                .font(mono)
                .width(Length::FillPortion(14)),
            text(forge_widgets::tr!("tts_aliases_col_voice"))
                .size(FONT_XS)
                .color(palette.text_muted)
                .font(mono)
                .width(Length::FillPortion(16)),
            text(forge_widgets::tr!("tts_aliases_col_pitch"))
                .size(FONT_XS)
                .color(palette.text_muted)
                .font(mono)
                .width(Length::FillPortion(8)),
            text(forge_widgets::tr!("tts_aliases_col_speed"))
                .size(FONT_XS)
                .color(palette.text_muted)
                .font(mono)
                .width(Length::FillPortion(8)),
            text(forge_widgets::tr!("tts_aliases_col_actions"))
                .size(FONT_XS)
                .color(palette.text_muted)
                .font(mono)
                .width(90),
        ]
        .align_y(Alignment::Center)
        .spacing(0),
    )
    .style(move |_| container::Style {
        background: Some(Background::Color(palette.shell)),
        border: Border {
            color: palette.border_regular,
            width: BORDER_THIN,
            radius: iced::border::Radius {
                top_left: 8.0,
                top_right: 8.0,
                bottom_left: 0.0,
                bottom_right: 0.0,
            },
        },
        ..container::Style::default()
    })
    .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
    .width(Length::Fill);

    let needle = state.search.to_lowercase();
    let visible: Vec<(usize, &VoiceAliasRow)> = state
        .aliases
        .iter()
        .enumerate()
        .filter(|(_, a)| state.search.is_empty() || a.viewer_name.to_lowercase().contains(&needle))
        .collect();

    let rows: Element<'a, Message> = if visible.is_empty() {
        container(
            text(forge_widgets::tr!("tts_aliases_empty"))
                .size(FONT_SM)
                .color(palette.text_muted),
        )
        .padding([sp(Spacing::Lg), sp(Spacing::Sm)])
        .width(Length::Fill)
        .into()
    } else {
        let total = visible.len();
        let row_els: Vec<Element<'a, Message>> = visible
            .iter()
            .enumerate()
            .map(|(pos, (orig_index, alias))| {
                alias_row(pos, *orig_index, alias, palette, gap_sm, total)
            })
            .collect();
        scrollable(column(row_els)).height(Length::Fill).into()
    };

    let body = container(rows)
        .style(move |_| container::Style {
            border: Border {
                color: palette.border_regular,
                width: BORDER_THIN,
                radius: iced::border::Radius {
                    top_left: 0.0,
                    top_right: 0.0,
                    bottom_left: 8.0,
                    bottom_right: 8.0,
                },
            },
            ..container::Style::default()
        })
        .width(Length::Fill);

    container(column![header, body].width(Length::Fill))
        .padding([0, sp(Spacing::Md)])
        .width(Length::Fill)
        .into()
}

fn alias_row<'a>(
    index: usize,
    orig_index: usize,
    alias: &'a VoiceAliasRow,
    palette: &'a ForgePalette,
    gap_sm: f32,
    total: usize,
) -> Element<'a, Message> {
    let muted = alias.blocked;
    let mono = font(FontRole::Monospace);
    let text_color = if muted {
        palette.text_muted
    } else {
        palette.text_primary
    };

    let initial = alias
        .viewer_name
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .next()
        .unwrap_or('?');

    let avatar_color = if muted {
        palette.surface_overlay
    } else {
        avatar_color_for(&alias.viewer_name, palette)
    };

    let avatar = container(
        text(initial.to_string())
            .size(FONT_XS)
            .color(if muted {
                palette.text_muted
            } else {
                palette.shell
            })
            .font(mono),
    )
    .style(move |_| container::Style {
        background: Some(Background::Color(avatar_color)),
        border: Border {
            radius: radius(Radius::Sm).into(),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .width(22)
    .height(22)
    .align_x(iced::alignment::Horizontal::Center)
    .align_y(iced::alignment::Vertical::Center);

    let role_badge: Element<'a, Message> = match &alias.role {
        Some(ViewerRole::Mod) => role_badge_el(
            forge_widgets::tr!("tts_aliases_role_mod"),
            palette.warning,
            palette,
        ),
        Some(ViewerRole::Vip) => role_badge_el(
            forge_widgets::tr!("tts_aliases_role_vip"),
            palette.brand,
            palette,
        ),
        Some(ViewerRole::Sub) => role_badge_el(
            forge_widgets::tr!("tts_aliases_role_sub"),
            palette.success,
            palette,
        ),
        None if muted => role_badge_el(
            forge_widgets::tr!("tts_aliases_role_blocked"),
            palette.random,
            palette,
        ),
        None => Space::new().into(),
    };

    let viewer_col = row![
        avatar,
        text(&alias.viewer_name).size(FONT_SM).color(text_color),
        role_badge,
    ]
    .align_y(Alignment::Center)
    .spacing(gap_sm)
    .width(Length::FillPortion(14));

    let voice_col: Element<'a, Message> = if muted {
        text(forge_widgets::tr!("tts_aliases_never_speak"))
            .size(FONT_SM)
            .color(palette.random)
            .font(mono)
            .width(Length::FillPortion(16))
            .into()
    } else {
        text(format!("{} · {}", alias.engine_label, alias.voice_label))
            .size(FONT_SM)
            .color(palette.text_primary)
            .font(mono)
            .width(Length::FillPortion(16))
            .into()
    };

    let pitch_str = if muted {
        "—".to_string()
    } else {
        alias
            .pitch_semitones
            .map(|p| {
                if p >= 0.0 {
                    format!("+{p:.0} st")
                } else {
                    format!("{p:.0} st")
                }
            })
            .unwrap_or_else(|| "0 st".to_string())
    };

    let speed_str = if muted {
        "—".to_string()
    } else {
        alias
            .rate_multiplier
            .map(|r| format!("{r:.1}x"))
            .unwrap_or_else(|| "1.0x".to_string())
    };

    let faint = palette.surface_overlay;

    let pitch_col = text(pitch_str)
        .size(FONT_SM)
        .color(if muted { faint } else { palette.text_muted })
        .font(mono)
        .width(Length::FillPortion(8));

    let speed_col = text(speed_str)
        .size(FONT_SM)
        .color(if muted { faint } else { palette.text_muted })
        .font(mono)
        .width(Length::FillPortion(8));

    let play_color = if muted {
        palette.surface_overlay
    } else {
        palette.success
    };
    let play_btn = {
        let b = button(text("▶").size(FONT_SM).color(play_color))
            .style(|_, _| button::Style::default())
            .padding(0);
        if muted {
            b
        } else {
            b.on_press(Message::Tts(TtsMsg::Aliases(VoiceAliasesMsg::PlayPreview(
                orig_index,
            ))))
        }
    };
    let actions = row![
        play_btn,
        button(text("✎").size(FONT_SM).color(palette.text_muted))
            .on_press(Message::Tts(TtsMsg::Aliases(VoiceAliasesMsg::Edit(
                orig_index
            ))))
            .style(|_, _| button::Style::default())
            .padding(0),
        button(text("✕").size(FONT_SM).color(palette.text_muted))
            .on_press(Message::Tts(TtsMsg::Aliases(
                VoiceAliasesMsg::DeleteRequested(orig_index)
            )))
            .style(|_, _| button::Style::default())
            .padding(0),
    ]
    .align_y(Alignment::Center)
    .spacing(gap_sm)
    .width(90);

    let is_last = index + 1 == total;
    let bg = if index.is_multiple_of(2) {
        palette.elevated
    } else {
        palette.shell
    };

    container(
        row![viewer_col, voice_col, pitch_col, speed_col, actions]
            .align_y(Alignment::Center)
            .spacing(0)
            .width(Length::Fill),
    )
    .style(move |_| container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            color: palette.border_regular,
            width: if is_last { 0.0 } else { BORDER_THIN },
            radius: iced::border::Radius::default(),
        },
        ..container::Style::default()
    })
    .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
    .width(Length::Fill)
    .into()
}

fn role_badge_el<'a>(
    label: String,
    color: iced::Color,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    container(
        text(label)
            .size(8.5)
            .color(color)
            .font(font(FontRole::Monospace)),
    )
    .style(move |_| container::Style {
        background: Some(Background::Color(palette.surface_overlay)),
        border: Border {
            radius: radius(Radius::Sm).into(),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .padding([sp(Spacing::Xxs), sp(Spacing::Xxs)])
    .into()
}

fn avatar_color_for(name: &str, palette: &ForgePalette) -> iced::Color {
    let hash: u32 = name
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    let colors = [
        palette.brand,
        palette.success,
        palette.warning,
        palette.info,
        palette.random,
        palette.bits,
    ];
    colors[(hash as usize) % colors.len()]
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use forge_runtime::{EventBus, NullEventLogRepo, ScriptRegistry};
    use forge_storage::CredentialsRepo;
    use forge_storage_sqlite::SqliteBackend;

    use crate::server_subsystem::ServerSubsystem;

    fn test_rt() -> RuntimeView {
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
            obs_sink: forge_obs::SwitchableObsSink::new(),
            discord_client: None,
            midi_client: None,
            hotkey_client: None,
            speak_queue: None,
            pipeline_config: None,
            tts_trigger_settings: None,
            sound_player: None,
            twitch_builtin: None,
            kick_builtin: None,
            platform_connection: std::collections::BTreeMap::new(),
            twitch_flow: None,
            youtube_flow: None,
            kick_flow: None,
            tts_engine_ids: Vec::new(),
            tts_registry: None,
            twitch_login: None,
            twitch_token_expires: None,
            twitch_reauth_required: false,
            sub_action_registry: Arc::new(forge_registry::SubActionRegistry::new()),
            trigger_registry: Arc::new(forge_registry::TriggerRegistry::new()),
        }
    }

    fn sample_row(viewer: &str) -> VoiceAliasRow {
        VoiceAliasRow {
            id: AliasId("alias-1".to_owned()),
            viewer_id: format!("{viewer}-id"),
            viewer_name: viewer.to_owned(),
            engine_id: "piper".to_owned(),
            voice_id: "en_US-amy".to_owned(),
            engine_label: "Piper".to_owned(),
            voice_label: "en_US-amy".to_owned(),
            pitch_semitones: Some(2.0),
            rate_multiplier: Some(1.5),
            blocked: false,
            role: None,
        }
    }

    fn dispatch(state: &mut VoiceAliasesState, rt: &RuntimeView, msg: VoiceAliasesMsg) {
        let _ = update(state, rt, msg);
    }

    #[test]
    fn assign_opens_an_empty_create_form() {
        let rt = test_rt();
        let mut state = VoiceAliasesState::new();
        dispatch(&mut state, &rt, VoiceAliasesMsg::Assign);
        let form = state.form.expect("assign should open a form");
        assert!(form.editing.is_none(), "create form must not target an id");
        assert!(form.viewer.is_empty());
        assert!(form.engine.is_empty());
        assert!(form.voice.is_empty());
    }

    #[test]
    fn edit_prefills_form_from_the_selected_row() {
        let rt = test_rt();
        let mut state = VoiceAliasesState::new();
        state.aliases = vec![sample_row("Nora")];
        dispatch(&mut state, &rt, VoiceAliasesMsg::Edit(0));
        let form = state.form.expect("edit should open a form");
        assert_eq!(form.editing, Some(AliasId("alias-1".to_owned())));
        assert_eq!(form.viewer, "Nora");
        assert_eq!(form.engine, "piper");
        assert_eq!(form.voice, "en_US-amy");
        assert_eq!(form.pitch, "2");
        assert_eq!(form.rate, "1.5");
    }

    #[test]
    fn edit_with_out_of_range_index_opens_no_form() {
        let rt = test_rt();
        let mut state = VoiceAliasesState::new();
        dispatch(&mut state, &rt, VoiceAliasesMsg::Edit(7));
        assert!(state.form.is_none());
    }

    #[test]
    fn form_field_messages_write_into_their_own_fields() {
        let rt = test_rt();
        let mut state = VoiceAliasesState::new();
        state.form = Some(AliasForm::default());

        dispatch(
            &mut state,
            &rt,
            VoiceAliasesMsg::FormViewerChanged("Zed".to_owned()),
        );
        dispatch(
            &mut state,
            &rt,
            VoiceAliasesMsg::FormEngineChanged("espeak".to_owned()),
        );
        dispatch(
            &mut state,
            &rt,
            VoiceAliasesMsg::FormVoiceChanged("en-uk".to_owned()),
        );
        dispatch(
            &mut state,
            &rt,
            VoiceAliasesMsg::FormPitchChanged("-3".to_owned()),
        );
        dispatch(
            &mut state,
            &rt,
            VoiceAliasesMsg::FormRateChanged("0.9".to_owned()),
        );

        let form = state.form.expect("form stays open");
        assert_eq!(form.viewer, "Zed");
        assert_eq!(form.engine, "espeak");
        assert_eq!(form.voice, "en-uk");
        assert_eq!(form.pitch, "-3");
        assert_eq!(form.rate, "0.9");
    }

    #[test]
    fn form_field_messages_with_no_open_form_are_noops() {
        let rt = test_rt();
        let mut state = VoiceAliasesState::new();
        dispatch(
            &mut state,
            &rt,
            VoiceAliasesMsg::FormViewerChanged("dropped".to_owned()),
        );
        assert!(state.form.is_none());
    }

    #[test]
    fn form_cancel_closes_the_form() {
        let rt = test_rt();
        let mut state = VoiceAliasesState::new();
        state.form = Some(AliasForm {
            viewer: "half typed".to_owned(),
            ..AliasForm::default()
        });
        dispatch(&mut state, &rt, VoiceAliasesMsg::FormCancel);
        assert!(state.form.is_none());
    }

    #[test]
    fn submit_with_blank_viewer_keeps_form_open_without_saving() {
        let rt = test_rt();
        for blank in ["", "   ", "\t\n"] {
            let mut state = VoiceAliasesState::new();
            state.form = Some(AliasForm {
                viewer: blank.to_owned(),
                ..AliasForm::default()
            });
            dispatch(&mut state, &rt, VoiceAliasesMsg::FormSubmit);
            let form = state
                .form
                .as_ref()
                .unwrap_or_else(|| panic!("form should stay open for {blank:?}"));
            assert!(!form.saving, "blank {blank:?} must not enter saving");
        }
    }

    #[test]
    fn submit_with_nonblank_viewer_enters_saving() {
        let rt = test_rt();
        let mut state = VoiceAliasesState::new();
        state.form = Some(AliasForm {
            viewer: "Mara".to_owned(),
            ..AliasForm::default()
        });
        dispatch(&mut state, &rt, VoiceAliasesMsg::FormSubmit);
        let form = state.form.expect("form stays open while saving");
        assert!(form.saving);
    }

    #[test]
    fn submit_result_ok_closes_the_form() {
        let rt = test_rt();
        let mut state = VoiceAliasesState::new();
        state.form = Some(AliasForm {
            viewer: "Mara".to_owned(),
            saving: true,
            ..AliasForm::default()
        });
        dispatch(&mut state, &rt, VoiceAliasesMsg::FormSubmitResult(Ok(())));
        assert!(state.form.is_none());
    }

    #[test]
    fn submit_result_err_clears_saving_for_retry() {
        let rt = test_rt();
        let mut state = VoiceAliasesState::new();
        state.form = Some(AliasForm {
            viewer: "Mara".to_owned(),
            saving: true,
            ..AliasForm::default()
        });
        dispatch(
            &mut state,
            &rt,
            VoiceAliasesMsg::FormSubmitResult(Err("db down".to_owned())),
        );
        let form = state.form.expect("form stays open after error");
        assert!(!form.saving);
    }

    #[test]
    fn delete_requested_arms_the_confirm_gate_without_deleting() {
        let rt = test_rt();
        let mut state = VoiceAliasesState::new();
        state.aliases = vec![sample_row("Nora")];
        dispatch(&mut state, &rt, VoiceAliasesMsg::DeleteRequested(0));
        assert_eq!(state.pending_delete, Some(0));
        // The row is still present: arming the gate must not remove anything.
        assert_eq!(state.aliases.len(), 1);
    }

    #[test]
    fn delete_cancel_clears_the_pending_gate() {
        let rt = test_rt();
        let mut state = VoiceAliasesState::new();
        state.pending_delete = Some(2);
        dispatch(&mut state, &rt, VoiceAliasesMsg::DeleteCancel);
        assert!(state.pending_delete.is_none());
    }

    #[test]
    fn delete_confirm_consumes_the_pending_index() {
        let rt = test_rt();
        let mut state = VoiceAliasesState::new();
        state.aliases = vec![sample_row("Nora")];
        state.pending_delete = Some(0);
        dispatch(&mut state, &rt, VoiceAliasesMsg::DeleteConfirm);
        assert!(
            state.pending_delete.is_none(),
            "confirm must take the pending index so the gate cannot re-fire"
        );
    }

    #[test]
    fn play_preview_on_blocked_alias_is_a_noop() {
        let rt = test_rt();
        let mut state = VoiceAliasesState::new();
        let mut blocked = sample_row("Nora");
        blocked.blocked = true;
        state.aliases = vec![blocked];
        // No speak queue, blocked alias: must return without enqueue and without panic.
        dispatch(&mut state, &rt, VoiceAliasesMsg::PlayPreview(0));
        assert!(state.form.is_none());
    }

    #[test]
    fn play_preview_without_speak_queue_is_graceful() {
        let rt = test_rt();
        assert!(rt.speak_queue.is_none(), "fixture has no speak queue");
        let mut state = VoiceAliasesState::new();
        state.aliases = vec![sample_row("Nora")];
        // Valid alias but no queue handle: must not panic.
        dispatch(&mut state, &rt, VoiceAliasesMsg::PlayPreview(0));
    }

    #[test]
    fn play_preview_with_out_of_range_index_is_a_noop() {
        let rt = test_rt();
        let mut state = VoiceAliasesState::new();
        dispatch(&mut state, &rt, VoiceAliasesMsg::PlayPreview(9));
    }
}
