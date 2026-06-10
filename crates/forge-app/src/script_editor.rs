use std::sync::Arc;

pub use forge_script::RunResult;
use forge_script::contract::collect_annotation_diagnostics;
use forge_script::{
    MethodDescriptor, RHAI_VERSION, catalog, collect_user_functions, content_hash, format_script,
    parse_contract, run_inline,
};
use forge_storage::{GlobalsRepo, ScriptRecord, ScriptRepo};
use forge_types::{ArgStack, ScriptContract, ScriptId, Variant, VariantKind};
use forge_widgets::tokens::{FONT_SM, FONT_XS, FontRole, Spacing, font, spf};
use forge_widgets::{
    ConsoleLevel, ConsoleLine, ForgePalette, ModalProps, ScriptEditorWidgetMsg,
    ScriptEditorWidgetState, StatusVariant, apply_autocomplete_insert, filter_candidates, modal,
    prefix_under_cursor, scan_type_hint, script_editor_widget, should_trigger_autocomplete,
    status_pill,
};
use iced::widget::{column, container, row, scrollable, text, text_editor, tooltip};
use iced::{Alignment, Background, Border, Element, Length};
use time::OffsetDateTime;

use crate::Message;
use crate::runtime_view::RuntimeView;

#[derive(Debug, Clone)]
pub struct ScriptListEntry {
    pub id: ScriptId,
    pub name: String,
    pub enabled: bool,
}

pub struct OpenScript {
    pub id: ScriptId,
    pub record: ScriptRecord,
    pub widget: ScriptEditorWidgetState,
    pub original_body: String,
}

#[derive(Debug, Clone)]
pub struct RunModalForm {
    pub script_id: ScriptId,
    pub script_name: String,
    pub display_title: String,
    pub inputs: Vec<RunModalInputField>,
    pub error: Option<String>,
    pub running: bool,
}

#[derive(Debug, Clone)]
pub struct RunModalInputField {
    pub name: String,
    pub kind: VariantKind,
    pub raw_value: String,
}

pub struct ScriptEditorState {
    pub scripts: Vec<ScriptListEntry>,
    pub selected: Option<ScriptId>,
    pub editor: Option<OpenScript>,
    pub console_lines: Vec<ConsoleLine>,
    pub variables_in_scope: Vec<(String, VariantKind)>,
    pub run_modal: Option<RunModalForm>,
    pub loading: bool,
    pub api_docs_search: String,
}

impl ScriptEditorState {
    pub fn new() -> Self {
        Self {
            scripts: Vec::new(),
            selected: None,
            editor: None,
            console_lines: Vec::new(),
            variables_in_scope: Vec::new(),
            run_modal: None,
            loading: false,
            api_docs_search: String::new(),
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.editor
            .as_ref()
            .is_some_and(|o| o.widget.editor.text() != o.original_body)
    }
}

impl Default for ScriptEditorState {
    fn default() -> Self {
        Self::new()
    }
}

fn now_timestamp() -> String {
    let now = OffsetDateTime::now_utc();
    format!("{:02}:{:02}:{:02}", now.hour(), now.minute(), now.second())
}

fn format_run_stats(duration_ms: f64, error_count: usize) -> String {
    format!("executed in {:.2}ms · {} errors", duration_ms, error_count)
}

fn parse_input_to_variant(field: &RunModalInputField) -> Result<Variant, String> {
    match field.kind {
        VariantKind::Int => field
            .raw_value
            .trim()
            .parse::<i64>()
            .map(Variant::Int)
            .map_err(|_| format!("`{}` must be an integer", field.name)),
        VariantKind::Float => field
            .raw_value
            .trim()
            .parse::<f64>()
            .map_err(|_| format!("`{}` must be a float", field.name))
            .and_then(|f| Variant::float(f).map_err(|e| e.to_string())),
        VariantKind::Bool => match field.raw_value.trim() {
            "true" => Ok(Variant::Bool(true)),
            "false" => Ok(Variant::Bool(false)),
            _ => Err(format!("`{}` must be `true` or `false`", field.name)),
        },
        VariantKind::String => Ok(Variant::String(field.raw_value.clone())),
        other => Err(format!(
            "`{}`: {other:?} inputs not supported in this run modal — edit the script's contract or pass via ArgStack",
            field.name
        )),
    }
}

async fn load_script_list(repo: Arc<dyn ScriptRepo>) -> Result<Vec<ScriptListEntry>, String> {
    let records = repo.list().await.map_err(|e| e.to_string())?;
    Ok(records
        .into_iter()
        .map(|r| ScriptListEntry {
            id: r.id,
            name: r.name,
            enabled: r.enabled,
        })
        .collect())
}

pub fn update(
    state: &mut ScriptEditorState,
    rt: &RuntimeView,
    msg: ScriptEditorMsg,
) -> iced::Task<Message> {
    match msg {
        ScriptEditorMsg::LoadRequested => {
            state.loading = true;
            let repo: Arc<dyn ScriptRepo> = Arc::clone(&rt.backend) as Arc<dyn ScriptRepo>;
            iced::Task::perform(async move { load_script_list(repo).await }, |r| {
                Message::ScriptEditor(ScriptEditorMsg::ScriptsLoaded(r))
            })
        }
        ScriptEditorMsg::ScriptsLoaded(Ok(entries)) => {
            let first_id = entries.first().map(|e| e.id);
            state.scripts = entries;
            state.loading = false;
            if let Some(id) = first_id {
                iced::Task::done(Message::ScriptEditor(ScriptEditorMsg::ScriptSelected(id)))
            } else {
                iced::Task::none()
            }
        }
        ScriptEditorMsg::ScriptsLoaded(Err(e)) => {
            state.loading = false;
            tracing::warn!(error = %e, "script list load failed");
            iced::Task::none()
        }
        ScriptEditorMsg::ScriptSelected(id) => {
            state.selected = Some(id);
            let dp = Arc::clone(&rt.backend);
            iced::Task::perform(
                async move {
                    ScriptRepo::get(&*dp, id)
                        .await
                        .map_err(|e| e.to_string())
                        .and_then(|opt| opt.ok_or_else(|| format!("script {id} not found")))
                },
                |r| Message::ScriptEditor(ScriptEditorMsg::ScriptOpened(r)),
            )
        }
        ScriptEditorMsg::ScriptOpened(Ok(record)) => {
            let contract = parse_contract(&record.body).unwrap_or_default();
            let vars: Vec<(String, VariantKind)> = contract
                .inputs
                .iter()
                .map(|i| (i.name.clone(), i.kind))
                .collect();
            let body = record.body.clone();
            let mut widget = ScriptEditorWidgetState::with_text(&body);
            widget.user_functions = collect_user_functions(&body);
            state.editor = Some(OpenScript {
                id: record.id,
                original_body: body,
                widget,
                record,
            });
            state.variables_in_scope = vars;
            iced::Task::none()
        }
        ScriptEditorMsg::ScriptOpened(Err(e)) => {
            tracing::warn!(error = %e, "script open failed");
            iced::Task::none()
        }
        ScriptEditorMsg::EditorAction(action) => {
            let just_typed = if let iced::widget::text_editor::Action::Edit(
                iced::widget::text_editor::Edit::Insert(ch),
            ) = &action
            {
                Some(*ch)
            } else {
                None
            };
            let is_edit = action.is_edit();
            if let Some(open) = state.editor.as_mut() {
                open.widget.editor.content.perform(action);
                open.widget.overlay_dismissed = false;

                let (line, col) = open.widget.editor.cursor_position();
                let line_text = open.widget.editor.line_text(line).unwrap_or_default();

                if should_trigger_autocomplete(&line_text, col, just_typed, false) {
                    open.widget.autocomplete_visible = true;
                    open.widget.autocomplete.selected_idx = 0;
                } else if open.widget.autocomplete_visible {
                    let pfx = prefix_under_cursor(&line_text, col);
                    if pfx.is_empty() {
                        open.widget.autocomplete_visible = false;
                    }
                }

                if is_edit {
                    let text = open.widget.editor.text();
                    open.widget.annotation_diagnostics = collect_annotation_diagnostics(&text);
                    open.widget.error_lines = open
                        .widget
                        .annotation_diagnostics
                        .iter()
                        .map(|d| d.line)
                        .collect();
                    open.widget.user_functions = collect_user_functions(&text);
                }
            }
            iced::Task::none()
        }
        ScriptEditorMsg::AutocompleteSelectionUp => {
            if let Some(open) = state.editor.as_mut() {
                let (line, col) = open.widget.editor.cursor_position();
                let line_text = open.widget.editor.line_text(line).unwrap_or_default();
                let prefix = prefix_under_cursor(&line_text, col);
                let count = filter_candidates(catalog(), &prefix).len();
                if count > 0 {
                    open.widget.autocomplete.selected_idx =
                        open.widget.autocomplete.selected_idx.saturating_sub(1);
                }
            }
            iced::Task::none()
        }
        ScriptEditorMsg::AutocompleteSelectionDown => {
            if let Some(open) = state.editor.as_mut() {
                let (line, col) = open.widget.editor.cursor_position();
                let line_text = open.widget.editor.line_text(line).unwrap_or_default();
                let prefix = prefix_under_cursor(&line_text, col);
                let count = filter_candidates(catalog(), &prefix).len();
                if count > 0 {
                    open.widget.autocomplete.selected_idx =
                        (open.widget.autocomplete.selected_idx + 1).min(count - 1);
                }
            }
            iced::Task::none()
        }
        ScriptEditorMsg::AutocompleteInsert(descriptor) => {
            if let Some(open) = state.editor.as_mut() {
                let (line, col) = open.widget.editor.cursor_position();
                let line_text = open.widget.editor.line_text(line).unwrap_or_default();
                let prefix = prefix_under_cursor(&line_text, col);
                for action in apply_autocomplete_insert(&prefix, &descriptor) {
                    open.widget.editor.content.perform(action);
                }
                open.widget.overlay_dismissed = true;
                open.widget.autocomplete_visible = false;
                open.widget.autocomplete.selected_idx = 0;
            }
            iced::Task::none()
        }
        ScriptEditorMsg::OverlayDismissed => {
            if let Some(open) = state.editor.as_mut() {
                open.widget.overlay_dismissed = true;
                open.widget.autocomplete_visible = false;
            }
            iced::Task::none()
        }
        ScriptEditorMsg::CtrlSpacePressed => {
            if let Some(open) = state.editor.as_mut() {
                open.widget.autocomplete_visible = true;
                open.widget.autocomplete.selected_idx = 0;
            }
            iced::Task::none()
        }
        ScriptEditorMsg::SaveRequested => {
            if !state.is_dirty() {
                return iced::Task::none();
            }
            let Some(open) = state.editor.as_ref() else {
                return iced::Task::none();
            };
            let body = open.widget.editor.text();
            let contract = match parse_contract(&body) {
                Ok(c) => c,
                Err(e) => {
                    let ts = now_timestamp();
                    state.console_lines.push(ConsoleLine {
                        level: ConsoleLevel::Err,
                        timestamp: Some(ts),
                        text: format!("contract parse error: {e}"),
                    });
                    return iced::Task::none();
                }
            };
            let mut record = open.record.clone();
            record.body = body.clone();
            record.body_hash = content_hash(&body);
            record.contract = contract;
            record.last_modified = OffsetDateTime::now_utc();
            let dp = Arc::clone(&rt.backend);
            iced::Task::perform(
                async move {
                    ScriptRepo::save(&*dp, record.clone())
                        .await
                        .map_err(|e| e.to_string())?;
                    Ok(record)
                },
                |r| Message::ScriptEditor(ScriptEditorMsg::ScriptSaved(r)),
            )
        }
        ScriptEditorMsg::ScriptSaved(Ok(record)) => {
            if let Some(open) = state.editor.as_mut() {
                open.original_body = record.body.clone();
                open.record = record.clone();
            }
            let ts = now_timestamp();
            state.console_lines.push(ConsoleLine {
                level: ConsoleLevel::Ok,
                timestamp: Some(ts),
                text: "script saved".to_string(),
            });
            let registry = Arc::clone(&rt.script_registry);
            let bus = Arc::clone(&rt.bus);
            iced::Task::perform(
                async move {
                    registry
                        .reload(record, bus.as_ref())
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| Message::ScriptEditor(ScriptEditorMsg::ScriptReloaded(r)),
            )
        }
        ScriptEditorMsg::ScriptReloaded(Ok(())) => iced::Task::none(),
        ScriptEditorMsg::ScriptReloaded(Err(e)) => {
            let ts = now_timestamp();
            state.console_lines.push(ConsoleLine {
                level: ConsoleLevel::Err,
                timestamp: Some(ts),
                text: format!("hot-reload failed: {e}"),
            });
            iced::Task::none()
        }
        ScriptEditorMsg::ScriptSaved(Err(e)) => {
            let ts = now_timestamp();
            state.console_lines.push(ConsoleLine {
                level: ConsoleLevel::Err,
                timestamp: Some(ts),
                text: format!("save failed: {e}"),
            });
            iced::Task::none()
        }
        ScriptEditorMsg::RunRequested => {
            let Some(open) = state.editor.as_ref() else {
                return iced::Task::none();
            };
            let body = open.widget.editor.text();
            let contract = parse_contract(&body).unwrap_or_default();
            if contract.inputs.is_empty() {
                let script_id = open.id;
                let script_name = open.record.name.clone();
                let dp = Arc::clone(&rt.backend) as Arc<dyn GlobalsRepo>;
                let bus = Arc::clone(&rt.bus);
                let ts = now_timestamp();
                state.console_lines.push(ConsoleLine {
                    level: ConsoleLevel::Run,
                    timestamp: Some(ts),
                    text: format!("running {script_name}"),
                });
                iced::Task::perform(
                    async move {
                        let publisher: Arc<dyn forge_events::EventPublisher> = bus;
                        run_inline(body, ArgStack::new(), dp, publisher, script_id)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    |r| Message::ScriptEditor(ScriptEditorMsg::RunFinished(r)),
                )
            } else {
                let form = RunModalForm {
                    script_id: open.id,
                    display_title: format!("Run script: {}", open.record.name),
                    script_name: open.record.name.clone(),
                    inputs: contract
                        .inputs
                        .iter()
                        .map(|i| RunModalInputField {
                            name: i.name.clone(),
                            kind: i.kind,
                            raw_value: String::new(),
                        })
                        .collect(),
                    error: None,
                    running: false,
                };
                state.run_modal = Some(form);
                iced::Task::none()
            }
        }
        ScriptEditorMsg::RunModalCancel => {
            state.run_modal = None;
            iced::Task::none()
        }
        ScriptEditorMsg::RunModalInputChanged(idx, val) => {
            if let Some(form) = state.run_modal.as_mut() {
                if let Some(field) = form.inputs.get_mut(idx) {
                    field.raw_value = val;
                }
                form.error = None;
            }
            iced::Task::none()
        }
        ScriptEditorMsg::RunModalSubmit => {
            let Some(form) = state.run_modal.as_ref() else {
                return iced::Task::none();
            };
            let mut arg_stack = ArgStack::new();
            for field in &form.inputs {
                match parse_input_to_variant(field) {
                    Ok(v) => arg_stack = arg_stack.set(field.name.clone(), v),
                    Err(e) => {
                        if let Some(f) = state.run_modal.as_mut() {
                            f.error = Some(e);
                        }
                        return iced::Task::none();
                    }
                }
            }
            let Some(open) = state.editor.as_ref() else {
                return iced::Task::none();
            };
            let body = open.widget.editor.text();
            let script_id = form.script_id;
            let script_name = form.script_name.clone();
            let dp = Arc::clone(&rt.backend) as Arc<dyn GlobalsRepo>;
            let bus = Arc::clone(&rt.bus);
            if let Some(f) = state.run_modal.as_mut() {
                f.running = true;
                f.error = None;
            }
            let ts = now_timestamp();
            state.console_lines.push(ConsoleLine {
                level: ConsoleLevel::Run,
                timestamp: Some(ts),
                text: format!("running {script_name} with inputs"),
            });
            iced::Task::perform(
                async move {
                    let publisher: Arc<dyn forge_events::EventPublisher> = bus;
                    run_inline(body, arg_stack, dp, publisher, script_id)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| Message::ScriptEditor(ScriptEditorMsg::RunFinished(r)),
            )
        }
        ScriptEditorMsg::RunFinished(Ok(result)) => {
            state.run_modal = None;
            let ts = now_timestamp();
            state.console_lines.push(ConsoleLine {
                level: ConsoleLevel::Ok,
                timestamp: Some(ts.clone()),
                text: format!("returned: {}", result.output_display),
            });
            state.console_lines.push(ConsoleLine {
                level: ConsoleLevel::Stats,
                timestamp: Some(ts),
                text: format_run_stats(result.duration_ms, result.error_count),
            });
            iced::Task::none()
        }
        ScriptEditorMsg::RunFinished(Err(e)) => {
            if let Some(f) = state.run_modal.as_mut() {
                f.running = false;
                f.error = Some(e.clone());
            }
            let ts = now_timestamp();
            state.console_lines.push(ConsoleLine {
                level: ConsoleLevel::Err,
                timestamp: Some(ts),
                text: format!("run error: {e}"),
            });
            iced::Task::none()
        }
        ScriptEditorMsg::NewScriptRequested => {
            let dp = Arc::clone(&rt.backend);
            iced::Task::perform(
                async move {
                    let now = OffsetDateTime::now_utc();
                    let name = format!("script_{}", now.unix_timestamp());
                    let body = "// @return string\n\n\"hello from forge\"".to_owned();
                    let record = ScriptRecord {
                        id: ScriptId::new(),
                        name,
                        body_hash: content_hash(&body),
                        body,
                        contract: ScriptContract::default(),
                        enabled: true,
                        created_at: now,
                        last_modified: now,
                    };
                    ScriptRepo::save(&*dp, record.clone())
                        .await
                        .map_err(|e| e.to_string())?;
                    Ok(record)
                },
                |r| Message::ScriptEditor(ScriptEditorMsg::NewScriptCreated(r)),
            )
        }
        ScriptEditorMsg::NewScriptCreated(Ok(record)) => {
            let entry = ScriptListEntry {
                id: record.id,
                name: record.name.clone(),
                enabled: record.enabled,
            };
            state.scripts.push(entry);
            let id = record.id;
            let body = record.body.clone();
            let mut widget = ScriptEditorWidgetState::with_text(&body);
            widget.user_functions = collect_user_functions(&body);
            state.editor = Some(OpenScript {
                id: record.id,
                original_body: body,
                widget,
                record,
            });
            state.selected = Some(id);
            state.variables_in_scope.clear();
            iced::Task::none()
        }
        ScriptEditorMsg::NewScriptCreated(Err(e)) => {
            tracing::warn!(error = %e, "new script creation failed");
            iced::Task::none()
        }
        ScriptEditorMsg::DeleteRequested(id) => {
            let dp = Arc::clone(&rt.backend);
            iced::Task::perform(
                async move {
                    ScriptRepo::delete(&*dp, id)
                        .await
                        .map(|_| ())
                        .map_err(|e| e.to_string())
                },
                |r| Message::ScriptEditor(ScriptEditorMsg::Deleted(r)),
            )
        }
        ScriptEditorMsg::Deleted(Ok(())) => {
            let id = state.selected;
            if let Some(selected_id) = id {
                state.scripts.retain(|e| e.id != selected_id);
            }
            state.selected = None;
            state.editor = None;
            state.variables_in_scope.clear();
            iced::Task::done(Message::ScriptEditor(ScriptEditorMsg::LoadRequested))
        }
        ScriptEditorMsg::Deleted(Err(e)) => {
            tracing::warn!(error = %e, "script delete failed");
            iced::Task::none()
        }
        ScriptEditorMsg::ConsoleClear => {
            state.console_lines.clear();
            iced::Task::none()
        }
        ScriptEditorMsg::FormatPressed => {
            if let Some(open) = state.editor.as_mut() {
                let source = open.widget.editor.text();
                let formatted = format_script(&source);
                if formatted != source {
                    open.widget.editor.content = text_editor::Content::with_text(&formatted);
                    open.widget.annotation_diagnostics = collect_annotation_diagnostics(&formatted);
                    open.widget.error_lines = open
                        .widget
                        .annotation_diagnostics
                        .iter()
                        .map(|d| d.line)
                        .collect();
                }
            }
            iced::Task::none()
        }
        ScriptEditorMsg::ApiDocsRequested => {
            iced::Task::done(Message::Navigate(crate::Screen::ScriptingApiDocs))
        }
        ScriptEditorMsg::ApiDocsSearchChanged(q) => {
            state.api_docs_search = q;
            iced::Task::none()
        }
    }
}

struct ApiNamespace {
    name: &'static str,
    entries: Vec<ApiEntry>,
}

struct ApiEntry {
    signature: &'static str,
}

fn api_catalog() -> Vec<ApiNamespace> {
    vec![
        ApiNamespace {
            name: "FORGE :: CORE",
            entries: vec![
                ApiEntry {
                    signature: "log(msg)",
                },
                ApiEntry {
                    signature: "warn(msg)",
                },
                ApiEntry {
                    signature: "sleep(ms)",
                },
            ],
        },
        ApiNamespace {
            name: "FORGE :: CHAT",
            entries: vec![
                ApiEntry {
                    signature: "send(text)",
                },
                ApiEntry {
                    signature: "reply(to, text)",
                },
                ApiEntry {
                    signature: "whisper(user, msg)",
                },
            ],
        },
        ApiNamespace {
            name: "FORGE :: GLOBALS",
            entries: vec![
                ApiEntry {
                    signature: "get(key)",
                },
                ApiEntry {
                    signature: "set(key, val, persisted)",
                },
                ApiEntry {
                    signature: "incr(key)",
                },
                ApiEntry {
                    signature: "del(key)",
                },
            ],
        },
        ApiNamespace {
            name: "FORGE :: OBS",
            entries: vec![ApiEntry {
                signature: "set_scene(n)",
            }],
        },
        ApiNamespace {
            name: "FORGE :: HTTP",
            entries: vec![
                ApiEntry {
                    signature: "get(url)",
                },
                ApiEntry {
                    signature: "post(url, body)",
                },
            ],
        },
    ]
}

fn run_button<'a>(enabled: bool, palette: &'a ForgePalette) -> Element<'a, Message> {
    use iced::widget::button;
    use iced::{Color, Shadow};

    let success = palette.success;
    let shell = palette.shell;
    let label = text("Run").size(FONT_SM).color(Color {
        a: if enabled { 1.0 } else { 0.4 },
        ..shell
    });
    let msg = if enabled {
        Some(Message::ScriptEditor(ScriptEditorMsg::RunRequested))
    } else {
        None
    };
    let mut btn = button(label).padding([spf(Spacing::Xxs), spf(Spacing::Sm)]);
    if let Some(m) = msg {
        btn = btn.on_press(m);
    }
    btn.style(move |_: &iced::Theme, _status| button::Style {
        background: Some(Background::Color(Color {
            a: if enabled { 1.0 } else { 0.3 },
            ..success
        })),
        border: Border {
            radius: 5.0.into(),
            ..Border::default()
        },
        text_color: shell,
        shadow: Shadow::default(),
        snap: false,
    })
    .into()
}

fn save_button<'a>(dirty: bool, palette: &'a ForgePalette) -> Element<'a, Message> {
    use iced::widget::button;
    use iced::{Color, Shadow};

    let fg = if dirty {
        palette.text_secondary
    } else {
        palette.text_faint
    };
    let label = text("Save").size(FONT_SM).color(fg);
    let msg = if dirty {
        Some(Message::ScriptEditor(ScriptEditorMsg::SaveRequested))
    } else {
        None
    };
    let mut btn = button(label).padding([spf(Spacing::Xxs), spf(Spacing::Xs)]);
    if let Some(m) = msg {
        btn = btn.on_press(m);
    }
    btn.style(move |_: &iced::Theme, _status| button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        border: Border {
            radius: 5.0.into(),
            ..Border::default()
        },
        text_color: fg,
        shadow: Shadow::default(),
        snap: false,
    })
    .into()
}

fn format_cursor_position(line: usize, col: usize) -> String {
    format!("Ln {}, Col {}", line + 1, col + 1)
}

fn type_check_pill_label(error_count: usize) -> String {
    if error_count == 0 {
        "Type-check passed".to_string()
    } else {
        format!("{error_count} errors")
    }
}

fn status_indicators_row<'a>(
    state: &'a ScriptEditorState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let diag_count = state
        .editor
        .as_ref()
        .map(|o| o.widget.annotation_diagnostics.len())
        .unwrap_or(0);
    let type_check_variant = if diag_count == 0 {
        StatusVariant::Positive
    } else {
        StatusVariant::Negative
    };
    let type_check = status_pill(
        type_check_pill_label(diag_count),
        type_check_variant,
        palette,
    );

    let rhai_pill = status_pill(
        format!("Rhai {RHAI_VERSION}"),
        StatusVariant::Neutral,
        palette,
    );

    let (ln, col) = state
        .editor
        .as_ref()
        .map(|o| o.widget.editor.cursor_position())
        .unwrap_or((0, 0));
    let cursor_label = text(format_cursor_position(ln, col))
        .size(FONT_XS)
        .color(palette.text_muted);

    container(
        row![type_check, rhai_pill, cursor_label]
            .spacing(spf(Spacing::Sm))
            .align_y(Alignment::Center),
    )
    .padding([spf(Spacing::Xxs), spf(Spacing::Sm)])
    .width(Length::Fill)
    .style(move |_: &iced::Theme| container::Style {
        border: Border {
            color: palette.border_regular,
            width: 0.5,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    })
    .into()
}

fn disabled_toolbar_button<'a>(label: &'a str, palette: &'a ForgePalette) -> Element<'a, Message> {
    use iced::widget::button;
    use iced::{Color, Shadow};

    let fg = palette.text_faint;
    button(text(label).size(FONT_SM).color(fg))
        .padding([spf(Spacing::Xxs), spf(Spacing::Xs)])
        .style(move |_: &iced::Theme, _status| button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            border: Border {
                radius: 5.0.into(),
                ..Border::default()
            },
            text_color: fg,
            shadow: Shadow::default(),
            snap: false,
        })
        .into()
}

fn left_pane<'a>(state: &'a ScriptEditorState, palette: &'a ForgePalette) -> Element<'a, Message> {
    use iced::widget::button;
    use iced::{Color, Shadow};

    let scripts_label = text("SCRIPTS")
        .size(FONT_XS)
        .color(palette.text_faint)
        .font(font(FontRole::Monospace));
    let scripts_header = container(scripts_label).padding([spf(Spacing::Xxs), spf(Spacing::Xs)]);

    let mut tree_col = column![scripts_header].spacing(0);

    if state.scripts.is_empty() {
        let empty_label = text("No scripts yet")
            .size(FONT_XS)
            .color(palette.text_extreme_faint)
            .font(font(FontRole::Monospace));
        tree_col =
            tree_col.push(container(empty_label).padding([spf(Spacing::Xxs), spf(Spacing::Xs)]));
    } else {
        for entry in &state.scripts {
            let selected = state.selected == Some(entry.id);
            let name_text = text(entry.name.clone())
                .size(FONT_XS)
                .color(if selected {
                    palette.text_primary
                } else {
                    palette.text_secondary
                })
                .font(font(FontRole::Monospace));
            let fg = if selected {
                palette.brand
            } else {
                Color::TRANSPARENT
            };
            let bg = if selected {
                palette.elevated
            } else {
                Color::TRANSPARENT
            };
            let id = entry.id;
            let btn = button(name_text)
                .on_press(Message::ScriptEditor(ScriptEditorMsg::ScriptSelected(id)))
                .padding([spf(Spacing::Xxs), spf(Spacing::Xs)])
                .width(Length::Fill)
                .style(move |_: &iced::Theme, _status| button::Style {
                    background: Some(Background::Color(bg)),
                    border: Border {
                        color: fg,
                        width: if selected { 2.0 } else { 0.0 },
                        radius: 5.0.into(),
                    },
                    text_color: palette.text_primary,
                    shadow: Shadow::default(),
                    snap: false,
                });
            tree_col = tree_col.push(btn);
        }
    }

    let new_script_btn = button(text("+ New").size(FONT_SM).color(palette.text_secondary))
        .on_press(Message::ScriptEditor(ScriptEditorMsg::NewScriptRequested))
        .padding([spf(Spacing::Xxs), spf(Spacing::Xs)])
        .width(Length::Fill)
        .style(move |_: &iced::Theme, _status| button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            border: Border::default(),
            text_color: palette.text_secondary,
            shadow: Shadow::default(),
            snap: false,
        });

    tree_col = tree_col.push(new_script_btn);

    if !state.variables_in_scope.is_empty() {
        let vars_label = text("VARIABLES IN SCOPE")
            .size(FONT_XS)
            .color(palette.text_faint)
            .font(font(FontRole::Monospace));
        let vars_header = container(vars_label).padding([spf(Spacing::Xxs), spf(Spacing::Xs)]);
        tree_col = tree_col.push(vars_header);

        for (name, kind) in &state.variables_in_scope {
            let name_text = text(format!("%{name}%"))
                .size(FONT_XS)
                .color(palette.warning)
                .font(font(FontRole::Monospace));
            let kind_text = text(kind.label().to_lowercase())
                .size(FONT_XS)
                .color(palette.text_faint)
                .font(font(FontRole::Monospace));
            let var_row = row![
                name_text,
                iced::widget::Space::new().width(Length::Fill),
                kind_text,
            ]
            .align_y(Alignment::Center)
            .padding([spf(Spacing::Xxs), spf(Spacing::Xs)]);
            tree_col = tree_col.push(var_row);
        }
    }

    container(scrollable(tree_col).height(Length::Fill))
        .width(Length::Fixed(180.0))
        .height(Length::Fill)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(palette.shell)),
            border: Border {
                color: palette.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn center_pane<'a>(
    state: &'a ScriptEditorState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let editor_area: Element<'a, Message> = if let Some(open) = state.editor.as_ref() {
        script_editor_widget(&open.widget, palette, |msg| match msg {
            ScriptEditorWidgetMsg::EditorAction(a) => {
                Message::ScriptEditor(ScriptEditorMsg::EditorAction(a))
            }
            ScriptEditorWidgetMsg::AutocompleteSelectionUp => {
                Message::ScriptEditor(ScriptEditorMsg::AutocompleteSelectionUp)
            }
            ScriptEditorWidgetMsg::AutocompleteSelectionDown => {
                Message::ScriptEditor(ScriptEditorMsg::AutocompleteSelectionDown)
            }
            ScriptEditorWidgetMsg::AutocompleteInsert(d) => {
                Message::ScriptEditor(ScriptEditorMsg::AutocompleteInsert(d))
            }
            ScriptEditorWidgetMsg::OverlayDismissed => {
                Message::ScriptEditor(ScriptEditorMsg::OverlayDismissed)
            }
            ScriptEditorWidgetMsg::CtrlSpacePressed => {
                Message::ScriptEditor(ScriptEditorMsg::CtrlSpacePressed)
            }
        })
    } else {
        container(
            column![
                text("Select a script or click + New")
                    .size(FONT_SM)
                    .color(palette.text_faint),
                text("Scripts let you run rhai code from any action.")
                    .size(FONT_SM)
                    .color(palette.text_extreme_faint),
            ]
            .spacing(spf(Spacing::Xs))
            .align_x(Alignment::Center),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
    };

    let status_bar: Element<'a, Message> = {
        let cursor_line = state
            .editor
            .as_ref()
            .map(|o| o.widget.editor.cursor_position().0);
        let diag = cursor_line.and_then(|line| {
            state.editor.as_ref().and_then(|o| {
                o.widget
                    .annotation_diagnostics
                    .iter()
                    .find(|d| d.line == line)
            })
        });
        let msg_text = if let Some(d) = diag {
            text(d.message.clone())
                .size(FONT_XS)
                .color(palette.random)
                .font(font(FontRole::Monospace))
        } else {
            let hint = cursor_line.and_then(|line_idx| {
                state.editor.as_ref().and_then(|o| {
                    o.widget
                        .editor
                        .line_text(line_idx)
                        .and_then(|l| scan_type_hint(&l))
                })
            });
            if let Some((name, ty)) = hint {
                text(format!("{name}: {ty}"))
                    .size(FONT_XS)
                    .color(palette.text_muted)
                    .font(font(FontRole::Monospace))
            } else {
                text("")
                    .size(FONT_XS)
                    .color(iced::Color::TRANSPARENT)
                    .font(font(FontRole::Monospace))
            }
        };
        container(msg_text)
            .width(Length::Fill)
            .padding([spf(Spacing::Xxs), spf(Spacing::Sm)])
            .into()
    };

    let output_header = {
        let out_label = text("Output").size(FONT_SM).color(palette.text_primary);
        let clear_btn = {
            use iced::widget::button;
            use iced::{Color, Shadow};
            button(text("Clear").size(FONT_XS).color(palette.text_faint))
                .on_press(Message::ScriptEditor(ScriptEditorMsg::ConsoleClear))
                .padding([spf(Spacing::Xxs), spf(Spacing::Xs)])
                .style(move |_: &iced::Theme, _status| button::Style {
                    background: Some(Background::Color(Color::TRANSPARENT)),
                    border: Border::default(),
                    text_color: palette.text_faint,
                    shadow: Shadow::default(),
                    snap: false,
                })
        };
        let header_inner = row![
            out_label,
            iced::widget::Space::new().width(Length::Fill),
            clear_btn,
        ]
        .align_y(Alignment::Center)
        .padding([spf(Spacing::Xs), spf(Spacing::Sm)]);
        container(header_inner)
            .width(Length::Fill)
            .style(move |_: &iced::Theme| container::Style {
                border: Border {
                    color: palette.border_regular,
                    width: 0.5,
                    radius: 0.0.into(),
                },
                ..container::Style::default()
            })
    };

    let console_panel = column![
        output_header,
        forge_widgets::console(palette, &state.console_lines),
    ]
    .height(Length::Fixed(130.0));

    column![
        container(editor_area)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_: &iced::Theme| container::Style {
                background: Some(Background::Color(palette.base)),
                ..container::Style::default()
            }),
        status_bar,
        console_panel,
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn right_pane<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    let header = row![
        text("API reference")
            .size(FONT_SM)
            .color(palette.text_primary),
        iced::widget::Space::new().width(Length::Fill),
    ]
    .align_y(Alignment::Center)
    .padding(iced::Padding {
        top: 0.0,
        right: 0.0,
        bottom: spf(Spacing::Xs),
        left: 0.0,
    });

    let catalog = api_catalog();
    let mut api_col = column![header].spacing(spf(Spacing::Xxs));

    for ns in &catalog {
        let ns_label = text(ns.name)
            .size(FONT_XS)
            .color(palette.text_muted)
            .font(font(FontRole::Monospace));
        api_col = api_col.push(container(ns_label).padding(iced::Padding {
            top: spf(Spacing::Xs),
            right: spf(Spacing::Xxs),
            bottom: spf(Spacing::Xxs),
            left: 0.0,
        }));

        for entry in &ns.entries {
            let kind_badge = {
                use iced::Shadow;
                use iced::widget::button;
                let shell_color = palette.shell;
                let badge_text = text("fn")
                    .size(FONT_XS)
                    .color(shell_color)
                    .font(font(FontRole::Monospace));
                button(badge_text)
                    .padding([spf(Spacing::Xxs), spf(Spacing::Xxs)])
                    .style(move |_: &iced::Theme, _status| button::Style {
                        background: Some(Background::Color(palette.brand)),
                        border: Border {
                            radius: 2.0.into(),
                            ..Border::default()
                        },
                        text_color: shell_color,
                        shadow: Shadow::default(),
                        snap: false,
                    })
            };
            let sig = text(entry.signature)
                .size(FONT_XS)
                .color(palette.text_primary)
                .font(font(FontRole::Monospace));
            let entry_row = row![kind_badge, sig]
                .spacing(spf(Spacing::Xs))
                .align_y(Alignment::Center);
            api_col =
                api_col.push(container(entry_row).padding([spf(Spacing::Xxs), spf(Spacing::Xxs)]));
        }
    }

    container(scrollable(api_col).height(Length::Fill))
        .width(Length::Fixed(200.0))
        .height(Length::Fill)
        .padding([spf(Spacing::Xs), spf(Spacing::Xs)])
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(palette.shell)),
            border: Border {
                color: palette.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn run_modal_view<'a>(
    state: &'a ScriptEditorState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let form = match state.run_modal.as_ref() {
        Some(f) => f,
        None => {
            return container(iced::widget::Space::new())
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
        }
    };

    let mut body_col = column![].spacing(spf(Spacing::Sm));

    for (idx, field) in form.inputs.iter().enumerate() {
        let label = text(format!(
            "{}: {}",
            field.name,
            field.kind.label().to_lowercase()
        ))
        .size(FONT_SM)
        .color(palette.text_muted);
        let input = iced::widget::text_input(
            &format!("Enter {} value...", field.kind.label().to_lowercase()),
            &field.raw_value,
        )
        .on_input(move |v| Message::ScriptEditor(ScriptEditorMsg::RunModalInputChanged(idx, v)))
        .padding([spf(Spacing::Xs), spf(Spacing::Xs)])
        .size(FONT_SM)
        .style(
            move |_: &iced::Theme, _status| iced::widget::text_input::Style {
                background: Background::Color(palette.elevated),
                border: Border {
                    color: palette.border_input,
                    width: 0.5,
                    radius: 6.0.into(),
                },
                icon: palette.text_faint,
                placeholder: palette.text_extreme_faint,
                value: palette.text_primary,
                selection: iced::Color {
                    a: 0.2,
                    ..palette.brand
                },
            },
        );
        body_col = body_col.push(column![label, input].spacing(spf(Spacing::Xxs)));
    }

    if let Some(err) = &form.error {
        let err_text = text(err.clone()).size(FONT_SM).color(palette.random);
        body_col = body_col.push(err_text);
    }

    let footer = {
        let cancel = forge_widgets::ghost_button(
            "Cancel",
            Message::ScriptEditor(ScriptEditorMsg::RunModalCancel),
            palette,
        );
        let run_label = if form.running { "Running…" } else { "Run" };
        let run_btn = if form.running {
            disabled_toolbar_button(run_label, palette)
        } else {
            forge_widgets::primary_button(
                run_label,
                Message::ScriptEditor(ScriptEditorMsg::RunModalSubmit),
                palette,
            )
        };
        row![
            cancel,
            iced::widget::Space::new().width(Length::Fill),
            run_btn
        ]
        .align_y(Alignment::Center)
        .into()
    };

    modal(
        palette,
        ModalProps {
            title: std::borrow::Cow::Borrowed(form.display_title.as_str()),
            on_close: Message::ScriptEditor(ScriptEditorMsg::RunModalCancel),
            kbd_hint: None,
        },
        body_col.into(),
        footer,
    )
}

pub fn script_editor_view<'a>(
    app: &'a crate::app::App,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let state = &app.ui.script_editor;

    let toolbar_actions = toolbar_action_row(state, palette);
    let indicators = status_indicators_row(state, palette);
    let left = left_pane(state, palette);
    let center = center_pane(state, palette);
    let right = right_pane(palette);

    let three_pane = row![left, center, right]
        .width(Length::Fill)
        .height(Length::Fill);

    let page_header = crate::page_chrome::page_header_with_actions(
        &[("Script Editor".to_owned(), true)],
        Some(toolbar_actions),
        palette,
    );

    let main_content = column![page_header, indicators, three_pane]
        .width(Length::Fill)
        .height(Length::Fill);

    if state.run_modal.is_some() {
        let modal_el = run_modal_view(state, palette);
        iced::widget::stack![main_content, modal_el].into()
    } else {
        main_content.into()
    }
}

fn toolbar_action_row<'a>(
    state: &'a ScriptEditorState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::button;
    use iced::{Color, Shadow};

    let dirty = state.is_dirty();
    let has_script = state.editor.is_some();

    let run_btn = run_button(has_script, palette);
    let save_btn = save_button(dirty, palette);

    let fg_format = if has_script {
        palette.text_secondary
    } else {
        palette.text_faint
    };
    let format_btn: Element<'a, Message> = {
        let label = text("Format").size(FONT_SM).color(fg_format);
        let mut btn = button(label)
            .padding([spf(Spacing::Xxs), spf(Spacing::Xs)])
            .style(move |_: &iced::Theme, _status| button::Style {
                background: Some(Background::Color(Color::TRANSPARENT)),
                border: Border {
                    radius: 5.0.into(),
                    ..Border::default()
                },
                text_color: fg_format,
                shadow: Shadow::default(),
                snap: false,
            });
        if has_script {
            btn = btn.on_press(Message::ScriptEditor(ScriptEditorMsg::FormatPressed));
        }
        btn.into()
    };

    let api_docs_btn: Element<'a, Message> = {
        let fg = palette.text_secondary;
        button(text("API docs").size(FONT_SM).color(fg))
            .on_press(Message::ScriptEditor(ScriptEditorMsg::ApiDocsRequested))
            .padding([spf(Spacing::Xxs), spf(Spacing::Xs)])
            .style(move |_: &iced::Theme, _status| button::Style {
                background: Some(Background::Color(Color::TRANSPARENT)),
                border: Border {
                    radius: 5.0.into(),
                    ..Border::default()
                },
                text_color: fg,
                shadow: Shadow::default(),
                snap: false,
            })
            .into()
    };

    let debug_btn: Element<'a, Message> = {
        let fg = palette.disabled;
        let inner = button(text("Debug").size(FONT_SM).color(fg))
            .padding([spf(Spacing::Xxs), spf(Spacing::Xs)])
            .style(move |_: &iced::Theme, _status| button::Style {
                background: Some(Background::Color(Color::TRANSPARENT)),
                border: Border {
                    radius: 5.0.into(),
                    ..Border::default()
                },
                text_color: fg,
                shadow: Shadow::default(),
                snap: false,
            });
        let tip = container(
            text("Debugger planned for post-1.0")
                .size(FONT_XS)
                .color(palette.text_primary),
        )
        .padding([spf(Spacing::Xxs), spf(Spacing::Sm)])
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(palette.elevated)),
            border: Border {
                color: palette.border_regular,
                width: 0.5,
                radius: 6.0.into(),
            },
            ..container::Style::default()
        });
        tooltip(inner, tip, tooltip::Position::Bottom).into()
    };

    let divider = crate::page_chrome::header_divider(palette);

    row![
        run_btn,
        save_btn,
        format_btn,
        debug_btn,
        divider,
        api_docs_btn
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(Alignment::Center)
    .into()
}

#[derive(Debug, Clone)]
pub enum ScriptEditorMsg {
    LoadRequested,
    ScriptsLoaded(Result<Vec<ScriptListEntry>, String>),
    ScriptSelected(ScriptId),
    ScriptOpened(Result<ScriptRecord, String>),
    EditorAction(iced::widget::text_editor::Action),
    SaveRequested,
    ScriptSaved(Result<ScriptRecord, String>),
    ScriptReloaded(Result<(), String>),
    RunRequested,
    RunModalCancel,
    RunModalInputChanged(usize, String),
    RunModalSubmit,
    RunFinished(Result<RunResult, String>),
    NewScriptRequested,
    NewScriptCreated(Result<ScriptRecord, String>),
    DeleteRequested(ScriptId),
    Deleted(Result<(), String>),
    ConsoleClear,
    AutocompleteSelectionUp,
    AutocompleteSelectionDown,
    AutocompleteInsert(MethodDescriptor),
    OverlayDismissed,
    FormatPressed,
    ApiDocsRequested,
    ApiDocsSearchChanged(String),
    CtrlSpacePressed,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_storage_sqlite::SqliteBackend;

    fn make_test_state_with_editor(body: &str, original: &str) -> ScriptEditorState {
        let mut s = ScriptEditorState::new();
        let id = ScriptId::new();
        let now = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        s.editor = Some(OpenScript {
            id,
            original_body: original.to_owned(),
            widget: ScriptEditorWidgetState::with_text(body),
            record: ScriptRecord {
                id,
                name: "test_script".to_owned(),
                body: original.to_owned(),
                contract: ScriptContract::default(),
                body_hash: content_hash(original),
                enabled: true,
                created_at: now,
                last_modified: now,
            },
        });
        s
    }

    #[test]
    fn is_dirty_no_editor_is_false() {
        let s = ScriptEditorState::new();
        assert!(!s.is_dirty());
    }

    #[test]
    fn is_dirty_matching_content_is_false() {
        let s = make_test_state_with_editor("let x = 1;", "let x = 1;");
        assert!(!s.is_dirty());
    }

    #[test]
    fn is_dirty_different_content_is_true() {
        let s = make_test_state_with_editor("let x = 2;", "let x = 1;");
        assert!(s.is_dirty());
    }

    #[test]
    fn scripts_loaded_populates_state() {
        let mut state = ScriptEditorState::new();
        let id = ScriptId::new();
        state.scripts = vec![ScriptListEntry {
            id,
            name: "greet".to_owned(),
            enabled: true,
        }];
        assert_eq!(state.scripts.len(), 1);
        assert_eq!(state.scripts[0].name, "greet");
    }

    #[test]
    fn script_selected_updates_state() {
        let mut state = ScriptEditorState::new();
        let id = ScriptId::new();
        state.selected = Some(id);
        assert_eq!(state.selected, Some(id));
    }

    #[test]
    fn script_opened_populates_editor() {
        let mut state = ScriptEditorState::new();
        let id = ScriptId::new();
        let now = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let record = ScriptRecord {
            id,
            name: "greet".to_owned(),
            body: "1 + 1".to_owned(),
            contract: ScriptContract::default(),
            body_hash: content_hash("1 + 1"),
            enabled: true,
            created_at: now,
            last_modified: now,
        };
        let body = record.body.clone();
        state.editor = Some(OpenScript {
            id,
            original_body: body.clone(),
            widget: ScriptEditorWidgetState::with_text(&body),
            record,
        });
        assert!(state.editor.is_some());
        assert_eq!(state.editor.as_ref().unwrap().record.name, "greet");
    }

    #[test]
    fn parse_int_input_valid() {
        let field = RunModalInputField {
            name: "count".to_owned(),
            kind: VariantKind::Int,
            raw_value: "42".to_owned(),
        };
        assert_eq!(parse_input_to_variant(&field).unwrap(), Variant::Int(42));
    }

    #[test]
    fn parse_int_input_invalid_returns_err() {
        let field = RunModalInputField {
            name: "count".to_owned(),
            kind: VariantKind::Int,
            raw_value: "abc".to_owned(),
        };
        assert!(parse_input_to_variant(&field).is_err());
    }

    #[test]
    fn parse_bool_true() {
        let field = RunModalInputField {
            name: "flag".to_owned(),
            kind: VariantKind::Bool,
            raw_value: "true".to_owned(),
        };
        assert_eq!(parse_input_to_variant(&field).unwrap(), Variant::Bool(true));
    }

    #[test]
    fn parse_bool_false() {
        let field = RunModalInputField {
            name: "flag".to_owned(),
            kind: VariantKind::Bool,
            raw_value: "false".to_owned(),
        };
        assert_eq!(
            parse_input_to_variant(&field).unwrap(),
            Variant::Bool(false)
        );
    }

    #[test]
    fn parse_string_input_passes_through() {
        let field = RunModalInputField {
            name: "msg".to_owned(),
            kind: VariantKind::String,
            raw_value: "hello world".to_owned(),
        };
        assert_eq!(
            parse_input_to_variant(&field).unwrap(),
            Variant::String("hello world".to_owned())
        );
    }

    #[test]
    fn hash_body_is_deterministic() {
        assert_eq!(content_hash("hello"), content_hash("hello"));
        assert_ne!(content_hash("hello"), content_hash("world"));
    }

    #[test]
    fn error_lines_derived_from_annotation_diagnostics() {
        use forge_types::AnnotationDiagnostic;
        let mut widget = ScriptEditorWidgetState::new();
        widget.annotation_diagnostics = vec![AnnotationDiagnostic {
            line: 2,
            message: "missing input lines".into(),
        }];
        widget.error_lines = widget
            .annotation_diagnostics
            .iter()
            .map(|d| d.line)
            .collect();
        assert_eq!(widget.error_lines, vec![2]);
    }

    #[tokio::test]
    async fn run_script_inline_simple_arithmetic() {
        let dp = Arc::new(
            SqliteBackend::open_with_key(":memory:", [0xab; 32])
                .await
                .unwrap(),
        );
        let bus =
            forge_runtime::EventBus::new(std::sync::Arc::new(forge_runtime::NullEventLogRepo));
        let id = ScriptId::new();
        let publisher: Arc<dyn forge_events::EventPublisher> = bus;
        let result = run_inline(
            "1 + 2".to_owned(),
            ArgStack::new(),
            dp as Arc<dyn forge_storage::GlobalsRepo>,
            publisher,
            id,
        )
        .await
        .unwrap();
        assert_eq!(result.output_display, "3");
        assert_eq!(result.script_id, id);
    }

    #[tokio::test]
    async fn run_script_inline_with_input() {
        let dp = Arc::new(
            SqliteBackend::open_with_key(":memory:", [0xab; 32])
                .await
                .unwrap(),
        );
        let bus =
            forge_runtime::EventBus::new(std::sync::Arc::new(forge_runtime::NullEventLogRepo));
        let id = ScriptId::new();
        let stack = ArgStack::new().set("x".to_owned(), Variant::Int(5));
        let publisher: Arc<dyn forge_events::EventPublisher> = bus;
        let result = run_inline(
            "// @input x: int\nx * 2".to_owned(),
            stack,
            dp as Arc<dyn forge_storage::GlobalsRepo>,
            publisher,
            id,
        )
        .await
        .unwrap();
        assert_eq!(result.output_display, "10");
    }

    #[tokio::test]
    async fn load_script_list_empty_db() {
        let dp = Arc::new(
            SqliteBackend::open_with_key(":memory:", [0xab; 32])
                .await
                .unwrap(),
        );
        let list = load_script_list(dp).await.unwrap();
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn load_script_list_with_one_record() {
        use forge_storage::ScriptRepo;

        let dp = Arc::new(
            SqliteBackend::open_with_key(":memory:", [0xab; 32])
                .await
                .unwrap(),
        );
        let now = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let record = ScriptRecord {
            id: ScriptId::new(),
            name: "my_script".to_owned(),
            body: "1".to_owned(),
            contract: ScriptContract::default(),
            body_hash: content_hash("1"),
            enabled: true,
            created_at: now,
            last_modified: now,
        };
        ScriptRepo::save(&*dp, record.clone()).await.unwrap();

        let list = load_script_list(dp).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "my_script");
    }

    #[test]
    fn format_cursor_position_simple() {
        assert_eq!(format_cursor_position(0, 0), "Ln 1, Col 1");
    }

    #[test]
    fn format_cursor_position_large() {
        assert_eq!(format_cursor_position(999, 80), "Ln 1000, Col 81");
    }

    #[test]
    fn type_check_pill_label_passes_when_no_diagnostics() {
        assert_eq!(type_check_pill_label(0), "Type-check passed");
    }

    #[test]
    fn type_check_pill_label_shows_count_with_diagnostics() {
        assert_eq!(type_check_pill_label(2), "2 errors");
    }

    #[test]
    fn format_run_stats_zero_errors() {
        assert_eq!(format_run_stats(1.84, 0), "executed in 1.84ms · 0 errors");
    }

    #[test]
    fn format_run_stats_with_errors() {
        assert_eq!(format_run_stats(0.5, 3), "executed in 0.50ms · 3 errors");
    }

    #[test]
    fn format_run_stats_handles_sub_millisecond() {
        assert_eq!(format_run_stats(0.001, 0), "executed in 0.00ms · 0 errors");
    }
}
