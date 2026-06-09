use std::sync::Arc;

use forge_script::{ScriptHttpConfig, load_script_http_config};
use forge_storage::{SettingsRepo, reserved_keys};
use forge_widgets::{
    ForgePalette, TagListInputMessage, TagListInputState, ToggleProps,
    icons::{Icon, tabler_icon},
    tag_list_input, toggle,
    tokens::{
        BORDER_THIN, FONT_LG, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, spf,
    },
};
use iced::{
    Background, Border, Color, Element, Length, Task,
    widget::{Space, column, container, row, rule, scrollable, text, text_input},
};

use crate::message::{Message, SettingsMsg};
use crate::runtime_view::RuntimeView;

const DEFAULT_OP_LIMIT: u32 = 100_000;
const DEFAULT_ENGINE_TIMEOUT_MS: u32 = 500;

#[derive(Debug, Clone)]
pub struct ScriptingSettingsSnapshot {
    pub allowed_domains: Vec<String>,
    pub max_calls_per_script: u32,
    pub http_timeout_ms: u32,
    pub allow_local: bool,
    pub max_response_bytes: u32,
    pub op_limit: u32,
    pub engine_timeout_ms: u32,
}

#[derive(Debug, Clone)]
pub enum ScriptingSettingsMsg {
    LoadRequested,
    LoadResult(Result<ScriptingSettingsSnapshot, String>),
    TagInput(TagListInputMessage),
    MaxCallsChanged(String),
    HttpTimeoutChanged(String),
    AllowLocalToggled,
    MaxResponseKibChanged(String),
    OpLimitChanged(String),
    EngineTimeoutChanged(String),
    SavePressed,
    SaveResult(Result<(), String>),
}

pub struct ScriptingSettingsState {
    pub tag_input: TagListInputState,
    pub allowed_domains: Vec<String>,
    pub max_calls_buf: String,
    pub http_timeout_buf: String,
    pub allow_local: bool,
    pub max_response_kib_buf: String,
    pub op_limit_buf: String,
    pub engine_timeout_buf: String,
    pub loading: bool,
    pub saving: bool,
    pub save_error: Option<String>,
    pub all_changes_saved: bool,
}

impl Default for ScriptingSettingsState {
    fn default() -> Self {
        let http = ScriptHttpConfig::default();
        Self {
            tag_input: TagListInputState::default(),
            allowed_domains: http.allowed_domains,
            max_calls_buf: http.max_calls_per_script.to_string(),
            http_timeout_buf: http.timeout_ms.to_string(),
            allow_local: http.allow_local,
            max_response_kib_buf: (http.max_response_bytes / 1024).to_string(),
            op_limit_buf: DEFAULT_OP_LIMIT.to_string(),
            engine_timeout_buf: DEFAULT_ENGINE_TIMEOUT_MS.to_string(),
            loading: false,
            saving: false,
            save_error: None,
            all_changes_saved: true,
        }
    }
}

pub async fn load_scripting_settings(
    repo: Arc<dyn SettingsRepo>,
) -> Result<ScriptingSettingsSnapshot, String> {
    let http = load_script_http_config(repo.as_ref()).await;
    let op_limit = repo
        .get_string(reserved_keys::SCRIPT_OP_LIMIT_KEY)
        .await
        .ok()
        .flatten()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(DEFAULT_OP_LIMIT);
    let engine_timeout_ms = repo
        .get_string(reserved_keys::SCRIPT_TIMEOUT_MS_KEY)
        .await
        .ok()
        .flatten()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(DEFAULT_ENGINE_TIMEOUT_MS);
    Ok(ScriptingSettingsSnapshot {
        allowed_domains: http.allowed_domains,
        max_calls_per_script: http.max_calls_per_script,
        http_timeout_ms: http.timeout_ms,
        allow_local: http.allow_local,
        max_response_bytes: http.max_response_bytes,
        op_limit,
        engine_timeout_ms,
    })
}

struct SavePayload {
    domains_csv: String,
    max_calls: u32,
    http_timeout_ms: u32,
    allow_local: bool,
    max_response_bytes: u32,
    op_limit: u32,
    engine_timeout_ms: u32,
}

async fn do_save(repo: Arc<dyn SettingsRepo>, p: SavePayload) -> Result<(), String> {
    repo.set_string(
        reserved_keys::SCRIPT_HTTP_ALLOWED_DOMAINS_KEY,
        &p.domains_csv,
    )
    .await
    .map_err(|e| e.to_string())?;
    repo.set_string(
        reserved_keys::SCRIPT_HTTP_MAX_CALLS_KEY,
        &p.max_calls.to_string(),
    )
    .await
    .map_err(|e| e.to_string())?;
    repo.set_string(
        reserved_keys::SCRIPT_HTTP_TIMEOUT_MS_KEY,
        &p.http_timeout_ms.to_string(),
    )
    .await
    .map_err(|e| e.to_string())?;
    repo.set_string(
        reserved_keys::SCRIPT_HTTP_ALLOW_LOCAL_KEY,
        if p.allow_local { "true" } else { "false" },
    )
    .await
    .map_err(|e| e.to_string())?;
    repo.set_string(
        reserved_keys::SCRIPT_HTTP_MAX_RESPONSE_BYTES_KEY,
        &p.max_response_bytes.to_string(),
    )
    .await
    .map_err(|e| e.to_string())?;
    repo.set_string(reserved_keys::SCRIPT_OP_LIMIT_KEY, &p.op_limit.to_string())
        .await
        .map_err(|e| e.to_string())?;
    repo.set_string(
        reserved_keys::SCRIPT_TIMEOUT_MS_KEY,
        &p.engine_timeout_ms.to_string(),
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn update(
    state: &mut ScriptingSettingsState,
    rt: &RuntimeView,
    msg: ScriptingSettingsMsg,
) -> Task<Message> {
    match msg {
        ScriptingSettingsMsg::LoadRequested => {
            state.loading = true;
            let repo: Arc<dyn SettingsRepo> = Arc::clone(&rt.backend) as Arc<dyn SettingsRepo>;
            Task::perform(async move { load_scripting_settings(repo).await }, |r| {
                Message::Settings(SettingsMsg::Scripting(ScriptingSettingsMsg::LoadResult(r)))
            })
        }
        ScriptingSettingsMsg::LoadResult(Ok(snap)) => {
            state.loading = false;
            state.allowed_domains = snap.allowed_domains;
            state.max_calls_buf = snap.max_calls_per_script.to_string();
            state.http_timeout_buf = snap.http_timeout_ms.to_string();
            state.allow_local = snap.allow_local;
            state.max_response_kib_buf = (snap.max_response_bytes / 1024).to_string();
            state.op_limit_buf = snap.op_limit.to_string();
            state.engine_timeout_buf = snap.engine_timeout_ms.to_string();
            state.all_changes_saved = true;
            state.save_error = None;
            Task::none()
        }
        ScriptingSettingsMsg::LoadResult(Err(e)) => {
            state.loading = false;
            state.save_error = Some(e);
            Task::none()
        }
        ScriptingSettingsMsg::TagInput(TagListInputMessage::DraftChanged(s)) => {
            state.tag_input.draft = s;
            Task::none()
        }
        ScriptingSettingsMsg::TagInput(TagListInputMessage::AddPressed) => {
            let draft = state.tag_input.draft.trim().to_string();
            if !draft.is_empty() && !state.allowed_domains.contains(&draft) {
                state.allowed_domains.push(draft);
                state.tag_input.draft.clear();
                state.all_changes_saved = false;
            }
            Task::none()
        }
        ScriptingSettingsMsg::TagInput(TagListInputMessage::RemoveTag(i)) => {
            if i < state.allowed_domains.len() {
                state.allowed_domains.remove(i);
                state.all_changes_saved = false;
            }
            Task::none()
        }
        ScriptingSettingsMsg::MaxCallsChanged(s) => {
            state.max_calls_buf = s;
            state.all_changes_saved = false;
            Task::none()
        }
        ScriptingSettingsMsg::HttpTimeoutChanged(s) => {
            state.http_timeout_buf = s;
            state.all_changes_saved = false;
            Task::none()
        }
        ScriptingSettingsMsg::AllowLocalToggled => {
            state.allow_local = !state.allow_local;
            state.all_changes_saved = false;
            Task::none()
        }
        ScriptingSettingsMsg::MaxResponseKibChanged(s) => {
            state.max_response_kib_buf = s;
            state.all_changes_saved = false;
            Task::none()
        }
        ScriptingSettingsMsg::OpLimitChanged(s) => {
            state.op_limit_buf = s;
            state.all_changes_saved = false;
            Task::none()
        }
        ScriptingSettingsMsg::EngineTimeoutChanged(s) => {
            state.engine_timeout_buf = s;
            state.all_changes_saved = false;
            Task::none()
        }
        ScriptingSettingsMsg::SavePressed => {
            let max_calls = state
                .max_calls_buf
                .parse::<u32>()
                .ok()
                .filter(|v| (1..=100).contains(v))
                .unwrap_or(10);
            let http_timeout_ms = state
                .http_timeout_buf
                .parse::<u32>()
                .ok()
                .filter(|v| (100..=30_000).contains(v))
                .unwrap_or(5_000);
            let max_response_bytes = state
                .max_response_kib_buf
                .parse::<u32>()
                .ok()
                .map(|kib| kib.saturating_mul(1024))
                .filter(|v| (1024..=10_485_760).contains(v))
                .unwrap_or(1_048_576);
            let op_limit = state
                .op_limit_buf
                .parse::<u32>()
                .ok()
                .filter(|v| (1_000..=10_000_000).contains(v))
                .unwrap_or(100_000);
            let engine_timeout_ms = state
                .engine_timeout_buf
                .parse::<u32>()
                .ok()
                .filter(|v| (50..=10_000).contains(v))
                .unwrap_or(500);
            let payload = SavePayload {
                domains_csv: state.allowed_domains.join(","),
                max_calls,
                http_timeout_ms,
                allow_local: state.allow_local,
                max_response_bytes,
                op_limit,
                engine_timeout_ms,
            };
            state.saving = true;
            state.all_changes_saved = false;
            let repo: Arc<dyn SettingsRepo> = Arc::clone(&rt.backend) as Arc<dyn SettingsRepo>;
            Task::perform(async move { do_save(repo, payload).await }, |r| {
                Message::Settings(SettingsMsg::Scripting(ScriptingSettingsMsg::SaveResult(r)))
            })
        }
        ScriptingSettingsMsg::SaveResult(Ok(())) => {
            state.saving = false;
            state.all_changes_saved = true;
            state.save_error = None;
            Task::none()
        }
        ScriptingSettingsMsg::SaveResult(Err(e)) => {
            state.saving = false;
            state.save_error = Some(e);
            Task::none()
        }
    }
}

fn section_rule<'a>(border_color: Color) -> Element<'a, Message> {
    rule::horizontal(0.5_f32)
        .style(move |_: &iced::Theme| rule::Style {
            color: border_color,
            radius: 0.0.into(),
            fill_mode: rule::FillMode::Full,
            snap: true,
        })
        .into()
}

fn section_header<'a>(label: &'a str, palette: &'a ForgePalette) -> Element<'a, Message> {
    text(label)
        .size(FONT_SM)
        .color(palette.text_primary)
        .font(iced::Font {
            weight: iced::font::Weight::Medium,
            ..font(FontRole::Body)
        })
        .into()
}

fn field_input_style(
    p: ForgePalette,
) -> impl Fn(&iced::Theme, iced::widget::text_input::Status) -> iced::widget::text_input::Style {
    move |_theme, _status| iced::widget::text_input::Style {
        background: Background::Color(p.shell),
        border: Border {
            color: p.border_input,
            width: BORDER_THIN,
            radius: radius(Radius::Md).into(),
        },
        icon: p.text_muted,
        placeholder: p.text_muted,
        value: p.text_primary,
        selection: Color { a: 0.25, ..p.brand },
    }
}

fn labeled_row<'a>(
    label: &'a str,
    hint: &'a str,
    input: Element<'a, Message>,
    palette: &ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;
    row![
        column![
            text(label).size(FONT_SM).color(p.text_primary),
            text(hint).size(FONT_XS).color(p.text_muted),
        ]
        .spacing(spf(Spacing::Xxs))
        .width(Length::FillPortion(4)),
        container(input).width(Length::FillPortion(3)),
    ]
    .spacing(spf(Spacing::Md))
    .align_y(iced::Alignment::Center)
    .into()
}

pub fn view<'a>(
    state: &'a ScriptingSettingsState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let p = palette;

    let save_indicator: Element<'a, Message> = if let Some(ref err) = state.save_error {
        text(format!("Save failed: {err}"))
            .size(FONT_SM)
            .color(p.random)
            .into()
    } else if state.saving {
        text("Saving…").size(FONT_SM).color(p.text_faint).into()
    } else if state.all_changes_saved {
        row![
            tabler_icon(Icon::CircleCheck, 13.0, p.success),
            text("All changes saved").size(FONT_SM).color(p.success),
        ]
        .spacing(spf(Spacing::Xs))
        .align_y(iced::Alignment::Center)
        .into()
    } else {
        text("Unsaved changes")
            .size(FONT_SM)
            .color(p.warning)
            .into()
    };

    let header_row = row![
        tabler_icon(Icon::FileCode, 20.0, p.brand),
        text("Scripting (Rhai)")
            .size(FONT_LG)
            .color(p.text_primary)
            .font(iced::Font {
                weight: iced::font::Weight::Medium,
                ..font(FontRole::Body)
            }),
        Space::new().width(Length::Fill),
        save_indicator,
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(iced::Alignment::Center);

    let op_input = text_input("100000", &state.op_limit_buf)
        .on_input(|s| {
            Message::Settings(SettingsMsg::Scripting(
                ScriptingSettingsMsg::OpLimitChanged(s),
            ))
        })
        .font(font(FontRole::Monospace))
        .size(FONT_SM)
        .padding([7_u16, 12_u16])
        .width(Length::Fill)
        .style(field_input_style(*p));

    let engine_timeout_input = text_input("500", &state.engine_timeout_buf)
        .on_input(|s| {
            Message::Settings(SettingsMsg::Scripting(
                ScriptingSettingsMsg::EngineTimeoutChanged(s),
            ))
        })
        .font(font(FontRole::Monospace))
        .size(FONT_SM)
        .padding([7_u16, 12_u16])
        .width(Length::Fill)
        .style(field_input_style(*p));

    let engine_section = column![
        section_header("Engine Limits", p),
        section_rule(p.border_regular),
        labeled_row(
            "Op-count limit",
            "Range 1 000 – 10 000 000 (default 100 000)",
            op_input.into(),
            p,
        ),
        labeled_row(
            "Timeout (ms)",
            "Range 50 – 10 000 (default 500)",
            engine_timeout_input.into(),
            p,
        ),
    ]
    .spacing(spf(Spacing::Sm));

    let domain_list = tag_list_input(
        &state.tag_input,
        &state.allowed_domains,
        "e.g. api.example.com",
        |m| Message::Settings(SettingsMsg::Scripting(ScriptingSettingsMsg::TagInput(m))),
        p,
    );

    let domains_col = column![
        text("Allowed domains")
            .size(FONT_SM)
            .color(p.text_primary)
            .font(iced::Font {
                weight: iced::font::Weight::Medium,
                ..font(FontRole::Body)
            }),
        text("Requests to unlisted domains are blocked. Wildcards: *.example.com")
            .size(FONT_XS)
            .color(p.text_muted),
        domain_list,
    ]
    .spacing(spf(Spacing::Xs));

    let max_calls_input = text_input("10", &state.max_calls_buf)
        .on_input(|s| {
            Message::Settings(SettingsMsg::Scripting(
                ScriptingSettingsMsg::MaxCallsChanged(s),
            ))
        })
        .font(font(FontRole::Monospace))
        .size(FONT_SM)
        .padding([7_u16, 12_u16])
        .width(Length::Fill)
        .style(field_input_style(*p));

    let http_timeout_input = text_input("5000", &state.http_timeout_buf)
        .on_input(|s| {
            Message::Settings(SettingsMsg::Scripting(
                ScriptingSettingsMsg::HttpTimeoutChanged(s),
            ))
        })
        .font(font(FontRole::Monospace))
        .size(FONT_SM)
        .padding([7_u16, 12_u16])
        .width(Length::Fill)
        .style(field_input_style(*p));

    let max_response_input = text_input("1024", &state.max_response_kib_buf)
        .on_input(|s| {
            Message::Settings(SettingsMsg::Scripting(
                ScriptingSettingsMsg::MaxResponseKibChanged(s),
            ))
        })
        .font(font(FontRole::Monospace))
        .size(FONT_SM)
        .padding([7_u16, 12_u16])
        .width(Length::Fill)
        .style(field_input_style(*p));

    let allow_local_toggle = toggle(
        p,
        ToggleProps {
            label: "Allow localhost / private IPs",
            description: "Disables SSRF protections. Only enable for local development.",
            value: state.allow_local,
            on_toggle: Message::Settings(SettingsMsg::Scripting(
                ScriptingSettingsMsg::AllowLocalToggled,
            )),
        },
    );

    let ssrf_warning: Element<'a, Message> = if state.allow_local {
        container(
            text("WARNING — disables SSRF protections. Only enable for local development.")
                .size(FONT_XS)
                .color(p.random),
        )
        .padding([4_u16, 0_u16])
        .into()
    } else {
        Space::new().height(0).into()
    };

    let http_section = column![
        section_header("HTTP Sandbox", p),
        section_rule(p.border_regular),
        domains_col,
        labeled_row(
            "Max calls per script",
            "Range 1 – 100 (default 10)",
            max_calls_input.into(),
            p,
        ),
        labeled_row(
            "Request timeout (ms)",
            "Range 100 – 30 000 (default 5 000)",
            http_timeout_input.into(),
            p,
        ),
        labeled_row(
            "Max response size (KiB)",
            "Range 1 – 10 240 (default 1 024 KiB = 1 MiB)",
            max_response_input.into(),
            p,
        ),
        allow_local_toggle,
        ssrf_warning,
    ]
    .spacing(spf(Spacing::Sm));

    let save_btn = forge_widgets::primary_button(
        "Save",
        Message::Settings(SettingsMsg::Scripting(ScriptingSettingsMsg::SavePressed)),
        p,
    );

    scrollable(
        column![header_row, engine_section, http_section, save_btn,]
            .spacing(spf(Spacing::Lg))
            .padding([20_u16, 24_u16]),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use forge_runtime::{EventBus, NullEventLogRepo, ScriptRegistry};
    use forge_storage::{CredentialsRepo, DataProvider};
    use forge_storage_sqlite::SqliteBackend;
    use std::sync::Arc;

    use crate::server_subsystem::ServerSubsystem;

    fn test_rt() -> RuntimeView {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let backend: Arc<dyn DataProvider> = Arc::new(
            tokio_rt
                .block_on(SqliteBackend::open_with_key("sqlite::memory:", [0xab; 32]))
                .unwrap(),
        );
        let server_subsystem = Arc::new(ServerSubsystem::new(
            Arc::clone(&backend) as Arc<dyn CredentialsRepo>
        ));
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
    fn csv_serialize_and_split_roundtrip_for_allowed_domains() {
        let domains = vec!["a.com".to_string(), "b.com".to_string()];
        let csv = domains.join(",");
        let parsed: Vec<String> = csv
            .split(',')
            .map(str::trim)
            .map(String::from)
            .filter(|s| !s.is_empty())
            .collect();
        assert_eq!(parsed, domains);
    }

    #[tokio::test]
    async fn save_scripting_settings_writes_all_seven_keys() {
        let backend = Arc::new(
            SqliteBackend::open_with_key("sqlite::memory:", [0xab; 32])
                .await
                .unwrap(),
        );
        let repo: Arc<dyn SettingsRepo> = backend.clone() as Arc<dyn SettingsRepo>;
        do_save(
            repo.clone(),
            SavePayload {
                domains_csv: "api.example.com".to_string(),
                max_calls: 5,
                http_timeout_ms: 3_000,
                allow_local: false,
                max_response_bytes: 524_288,
                op_limit: 50_000,
                engine_timeout_ms: 250,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            repo.get_string(reserved_keys::SCRIPT_HTTP_ALLOWED_DOMAINS_KEY)
                .await
                .unwrap(),
            Some("api.example.com".to_string())
        );
        assert_eq!(
            repo.get_string(reserved_keys::SCRIPT_HTTP_MAX_CALLS_KEY)
                .await
                .unwrap(),
            Some("5".to_string())
        );
        assert_eq!(
            repo.get_string(reserved_keys::SCRIPT_HTTP_TIMEOUT_MS_KEY)
                .await
                .unwrap(),
            Some("3000".to_string())
        );
        assert_eq!(
            repo.get_string(reserved_keys::SCRIPT_HTTP_ALLOW_LOCAL_KEY)
                .await
                .unwrap(),
            Some("false".to_string())
        );
        assert_eq!(
            repo.get_string(reserved_keys::SCRIPT_HTTP_MAX_RESPONSE_BYTES_KEY)
                .await
                .unwrap(),
            Some("524288".to_string())
        );
        assert_eq!(
            repo.get_string(reserved_keys::SCRIPT_OP_LIMIT_KEY)
                .await
                .unwrap(),
            Some("50000".to_string())
        );
        assert_eq!(
            repo.get_string(reserved_keys::SCRIPT_TIMEOUT_MS_KEY)
                .await
                .unwrap(),
            Some("250".to_string())
        );
    }

    #[test]
    fn domain_add_appends_and_deduplicates() {
        let rt = test_rt();
        let mut state = ScriptingSettingsState::default();
        let _ = update(
            &mut state,
            &rt,
            ScriptingSettingsMsg::TagInput(TagListInputMessage::DraftChanged(
                "api.example.com".to_string(),
            )),
        );
        let _ = update(
            &mut state,
            &rt,
            ScriptingSettingsMsg::TagInput(TagListInputMessage::AddPressed),
        );
        assert_eq!(state.allowed_domains, vec!["api.example.com".to_string()]);
        assert!(state.tag_input.draft.is_empty());
        let _ = update(
            &mut state,
            &rt,
            ScriptingSettingsMsg::TagInput(TagListInputMessage::DraftChanged(
                "api.example.com".to_string(),
            )),
        );
        let _ = update(
            &mut state,
            &rt,
            ScriptingSettingsMsg::TagInput(TagListInputMessage::AddPressed),
        );
        assert_eq!(state.allowed_domains.len(), 1);
    }

    #[test]
    fn domain_remove_removes_by_index() {
        let rt = test_rt();
        let mut state = ScriptingSettingsState {
            allowed_domains: vec!["a.com".to_string(), "b.com".to_string()],
            ..ScriptingSettingsState::default()
        };
        let _ = update(
            &mut state,
            &rt,
            ScriptingSettingsMsg::TagInput(TagListInputMessage::RemoveTag(0)),
        );
        assert_eq!(state.allowed_domains, vec!["b.com".to_string()]);
    }

    #[test]
    fn allow_local_toggle_flips_flag() {
        let rt = test_rt();
        let mut state = ScriptingSettingsState::default();
        assert!(!state.allow_local);
        let _ = update(&mut state, &rt, ScriptingSettingsMsg::AllowLocalToggled);
        assert!(state.allow_local);
        let _ = update(&mut state, &rt, ScriptingSettingsMsg::AllowLocalToggled);
        assert!(!state.allow_local);
    }

    #[test]
    fn load_result_ok_populates_all_fields() {
        let rt = test_rt();
        let mut state = ScriptingSettingsState::default();
        let snap = ScriptingSettingsSnapshot {
            allowed_domains: vec!["cdn.example.com".to_string()],
            max_calls_per_script: 20,
            http_timeout_ms: 8_000,
            allow_local: true,
            max_response_bytes: 2_097_152,
            op_limit: 200_000,
            engine_timeout_ms: 1_000,
        };
        let _ = update(&mut state, &rt, ScriptingSettingsMsg::LoadResult(Ok(snap)));
        assert_eq!(state.allowed_domains, vec!["cdn.example.com".to_string()]);
        assert_eq!(state.max_calls_buf, "20");
        assert_eq!(state.http_timeout_buf, "8000");
        assert!(state.allow_local);
        assert_eq!(state.max_response_kib_buf, "2048");
        assert_eq!(state.op_limit_buf, "200000");
        assert_eq!(state.engine_timeout_buf, "1000");
        assert!(state.all_changes_saved);
    }

    #[test]
    fn save_pressed_sets_saving_flag() {
        let rt = test_rt();
        let mut state = ScriptingSettingsState::default();
        let _ = update(&mut state, &rt, ScriptingSettingsMsg::SavePressed);
        assert!(state.saving);
    }

    #[test]
    fn save_result_ok_clears_saving_and_sets_saved() {
        let rt = test_rt();
        let mut state = ScriptingSettingsState {
            saving: true,
            all_changes_saved: false,
            ..ScriptingSettingsState::default()
        };
        let _ = update(&mut state, &rt, ScriptingSettingsMsg::SaveResult(Ok(())));
        assert!(!state.saving);
        assert!(state.all_changes_saved);
        assert!(state.save_error.is_none());
    }

    #[test]
    fn save_result_err_records_error() {
        let rt = test_rt();
        let mut state = ScriptingSettingsState {
            saving: true,
            ..ScriptingSettingsState::default()
        };
        let _ = update(
            &mut state,
            &rt,
            ScriptingSettingsMsg::SaveResult(Err("disk full".to_string())),
        );
        assert!(!state.saving);
        assert_eq!(state.save_error.as_deref(), Some("disk full"));
    }
}
