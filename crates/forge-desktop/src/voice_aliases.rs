use crate::platform_visuals::platform_color;
use crate::repo_failure::RepoFailure;
use forge_components::FONT_LABEL;
use forge_components::FONT_META;
use forge_components::FONT_ROLE_BADGE;
use forge_components::FONT_ROW;
use forge_components::SEARCH_FIELD_WIDTH;
use forge_components::focus_inside_modal;
use std::future::Future;
use std::sync::Arc;

use forge_components::{
    BORDER_THIN, ColumnWidth, Confirm, ConfirmTone, DataRow, Density, FONT_XS, FONT_XXS,
    ForgePalette, Icon, OverlayPosition, Picker, PickerEvent, PickerItem, PickerLabels, Radius,
    SearchState, Spacing, ToastKind, avatar_tile, badge, body_family, card, column, confirm_modal,
    data_table, dropdown, empty_state, error_row, error_row_with_retry, hash_accent, icon,
    mono_family, overlay, primary_button_with_icon, radius, scaled, segment, segmented, spacing,
    status_dot, toolbar_row, tr, virtual_table, with_alpha,
};
use forge_speak_queue::{
    AliasKey, EmoteTokenSet, Priority, RequestId, SpeakCommand, SpeakQueueHandle, SpeakRequest,
};
use forge_speech_types::AliasState;
use forge_speech_types::{AliasId, AssignmentStrategy, EngineId, TtsVoice, VoiceAlias, VoiceId};
use forge_storage::{StorageError, ViewerRepo, VoiceAliasRepo};
use forge_tts_core::engine_display_name;
use gpui::{
    AnyElement, ClickEvent, Context, Entity, FontWeight, Pixels, Rgba, SharedString, Subscription,
    UniformListScrollHandle, Window, div, prelude::*, px,
};

use crate::async_bridge::{self, Loadable, SnapshotWriter, WriteOutcome};
use crate::voice_alias_form::{
    AliasForm, AliasFormEvent, AliasIdentity, AliasScope, AliasValues, alias_key_scope, fmt_field,
};
use crate::voice_alias_store::save_alias;
use forge_components::ActivePresentation;
use forge_components::PushToast;

const ACTIONS_W: Pixels = px(90.0);
const AVATAR: Pixels = px(22.0);
const TABLE_RADIUS: Pixels = px(8.0);
const ROLE_BADGE_FS: Pixels = FONT_ROLE_BADGE;
const ENGINE_GLYPH: Pixels = px(12.0);
const ACTION_GLYPH: Pixels = px(13.0);
const BANNER_ICON: Pixels = px(18.0);
const ROW_PAD_V: Pixels = px(9.0);
const ROW_PAD_H: Pixels = px(12.0);
const ROW_HOVER_ALPHA: f32 = 0.08;
const HEADER_PAD_V: Pixels = px(7.0);
const HEADER_PAD_H: Pixels = px(12.0);
const FOOTER_PAD_TOP: Pixels = px(8.0);
const VOICE_FS: Pixels = FONT_LABEL;
const META_FS: Pixels = FONT_META;
const PAGE_PAD_H: Pixels = px(18.0);
const PAGE_GAP_V: Pixels = px(12.0);
const BANNER_PAD_V: Pixels = px(12.0);
const BANNER_PAD_H: Pixels = px(14.0);
const BANNER_GAP: Pixels = px(14.0);
const BANNER_TITLE_FS: Pixels = FONT_ROW;
const BANNER_TITLE_MB: Pixels = px(1.0);
const PLATFORM_DOT: Pixels = px(6.0);
const PLATFORM_BADGE_PAD_V: Pixels = px(1.0);

const VIEWER_GROW: f32 = 1.4;
const VOICE_GROW: f32 = 1.6;
const PITCH_GROW: f32 = 0.8;
const SPEED_GROW: f32 = 0.8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StrategyChoice {
    DeterministicByName,
    Random,
    SingleVoice,
}

struct SingleVoice {
    voice_id: VoiceId,
    engine_id: EngineId,
}

struct VoicePicker {
    picker: Entity<Picker>,
    voices: Arc<Vec<TtsVoice>>,
    _sub: Subscription,
}

impl StrategyChoice {
    const ALL: [StrategyChoice; 3] = [
        StrategyChoice::DeterministicByName,
        StrategyChoice::Random,
        StrategyChoice::SingleVoice,
    ];

    fn label(self) -> String {
        match self {
            StrategyChoice::DeterministicByName => tr!("tts_aliases_strategy_deterministic"),
            StrategyChoice::Random => tr!("tts_aliases_strategy_random"),
            StrategyChoice::SingleVoice => tr!("tts_aliases_strategy_single"),
        }
    }

    fn key(self) -> &'static str {
        match self {
            StrategyChoice::DeterministicByName => "deterministic",
            StrategyChoice::Random => "random",
            StrategyChoice::SingleVoice => "single",
        }
    }
}

struct AliasRow {
    id: AliasId,
    viewer_id: String,
    viewer_name: String,
    platform: AliasScope,
    engine_id: String,
    engine_label: String,
    voice_id: String,
    pitch_semitones: Option<f32>,
    rate_multiplier: Option<f32>,
    blocked: bool,
}

pub(crate) struct VoiceAliasesView {
    repo: Arc<dyn VoiceAliasRepo>,
    viewer_repo: Arc<dyn ViewerRepo>,
    speak: Option<SpeakQueueHandle>,
    rt_handle: tokio::runtime::Handle,
    loading: bool,
    load_failed: bool,
    strategy: Loadable<StrategyChoice>,
    strategy_writer: SnapshotWriter,
    strategy_error: Option<String>,
    single_voice: Option<SingleVoice>,
    voice_picker: Option<VoicePicker>,
    aliases: Vec<AliasRow>,
    visible: Vec<usize>,
    total_count: usize,
    viewer_count: usize,
    search: SearchState,
    table_scroll: UniformListScrollHandle,
    form: Option<Entity<AliasForm>>,
    _form_sub: Option<Subscription>,
    pending_delete: Confirm<AliasId>,
    _search_sub: Subscription,
}

impl VoiceAliasesView {
    pub(crate) fn new(
        repo: Arc<dyn VoiceAliasRepo>,
        viewer_repo: Arc<dyn ViewerRepo>,
        speak: Option<SpeakQueueHandle>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let search = SearchState::new(cx, palette, tr!("tts_aliases_search_placeholder"));
        let search_sub = cx.subscribe(search.field(), |this: &mut Self, _input, event, cx| {
            if this.search.on_changed(event) {
                this.rebuild_visible();
                cx.notify();
            }
        });

        let strategy_writer = SnapshotWriter::new(&rt_handle);
        let mut view = Self {
            repo,
            viewer_repo,
            speak,
            rt_handle,
            loading: true,
            load_failed: false,
            strategy: Loadable::Loading,
            strategy_writer,
            strategy_error: None,
            single_voice: None,
            voice_picker: None,
            aliases: Vec::new(),
            visible: Vec::new(),
            total_count: 0,
            viewer_count: 0,
            search,
            table_scroll: UniformListScrollHandle::new(),
            form: None,
            _form_sub: None,
            pending_delete: Confirm::default(),
            _search_sub: search_sub,
        };
        view.reload(cx);
        view.load_strategy(cx);
        view
    }

    fn load_strategy(&mut self, cx: &mut Context<Self>) {
        self.strategy = Loadable::Loading;
        let repo = Arc::clone(&self.repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move { repo.get_strategy().await },
            |this, result: Result<AssignmentStrategy, StorageError>, cx| {
                if let Err(error) = &result {
                    tracing::warn!(%error, "failed to load the voice assignment strategy");
                }
                if let Ok(strategy) = &result {
                    this.single_voice = single_voice_of(strategy);
                }
                this.strategy = Loadable::settled(result.map(|s| choice_from_strategy(&s)));
                cx.notify();
            },
            cx,
        );
        cx.notify();
    }

    fn retry_strategy(&mut self, cx: &mut Context<Self>) {
        self.strategy_error = None;
        self.load_strategy(cx);
    }

    fn strategy_write_settled(&mut self, outcome: WriteOutcome, cx: &mut Context<Self>) {
        match outcome {
            WriteOutcome::Written => self.strategy_error = None,
            WriteOutcome::Superseded => {}
            WriteOutcome::Failed(error) => {
                self.strategy_error = Some(error.text());
                self.load_strategy(cx);
            }
        }
    }

    fn retry_load(&mut self, cx: &mut Context<Self>) {
        self.load_failed = false;
        self.loading = true;
        self.reload(cx);
        cx.notify();
    }

    fn reload(&self, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.repo);
        let viewer_repo = Arc::clone(&self.viewer_repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let aliases = repo.list().await.map_err(RepoFailure::from)?;
                let viewers = viewer_repo.count().await.map_err(RepoFailure::from)?;
                Ok::<_, RepoFailure>((aliases, viewers))
            },
            |this, result, cx| match result {
                Ok((aliases, viewers)) => {
                    this.load_failed = false;
                    this.apply_loaded(aliases, viewers, cx);
                }
                Err(failure) => {
                    this.load_failed = true;
                    this.on_repo_error(&failure, cx);
                }
            },
            cx,
        );
    }

    fn spawn_write(
        &self,
        work: impl Future<Output = Result<Vec<VoiceAlias>, RepoFailure>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        async_bridge::run_async(
            &self.rt_handle,
            work,
            |this, result, cx| match result {
                Ok(aliases) => this.apply_aliases(aliases, cx),
                Err(failure) => this.on_repo_error(&failure, cx),
            },
            cx,
        );
    }

    fn apply_loaded(&mut self, aliases: Vec<VoiceAlias>, viewers: u64, cx: &mut Context<Self>) {
        self.viewer_count = usize::try_from(viewers).unwrap_or(usize::MAX);
        self.set_roster(aliases);
        cx.notify();
    }

    fn apply_aliases(&mut self, aliases: Vec<VoiceAlias>, cx: &mut Context<Self>) {
        self.set_roster(aliases);
        cx.notify();
    }

    fn set_roster(&mut self, aliases: Vec<VoiceAlias>) {
        self.total_count = aliases.len();
        self.aliases = aliases.into_iter().map(row_from_alias).collect();
        self.rebuild_visible();
        self.loading = false;
    }

    fn row_by_id(&self, id: &AliasId) -> Option<&AliasRow> {
        self.aliases.iter().find(|row| row.id == *id)
    }

    fn rebuild_visible(&mut self) {
        self.visible = self
            .aliases
            .iter()
            .enumerate()
            .filter(|(_, a)| self.search.matches(&a.viewer_name))
            .map(|(index, _)| index)
            .collect();
    }

    fn on_repo_error(&mut self, failure: &RepoFailure, cx: &mut Context<Self>) {
        tracing::warn!(error = %failure, "voice aliases operation failed");
        let message = failure.text();
        self.loading = false;
        cx.push_toast(
            ToastKind::Error,
            tr!("tts_aliases_operation_failed", error = message.as_str()),
        );
        cx.notify();
    }

    fn choose_strategy(
        &mut self,
        choice: StrategyChoice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match choice {
            StrategyChoice::SingleVoice => self.open_voice_picker(window, cx),
            other => self.set_strategy(other, cx),
        }
    }

    fn set_strategy(&mut self, choice: StrategyChoice, cx: &mut Context<Self>) {
        let strategy = match choice {
            StrategyChoice::DeterministicByName => AssignmentStrategy::DeterministicByName,
            StrategyChoice::Random => AssignmentStrategy::Random,
            StrategyChoice::SingleVoice => return,
        };
        self.persist_strategy(strategy, cx);
    }

    fn open_voice_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.strategy.is_loaded() {
            return;
        }
        let voices = self.available_voices();
        if voices.is_empty() {
            self.strategy_error = Some(tr!("tts_aliases_strategy_no_voices"));
            cx.notify();
            return;
        }
        let items = voices
            .iter()
            .enumerate()
            .map(|(index, voice)| PickerItem {
                id: index.to_string().into(),
                label: voice.name.clone().into(),
                sublabel: Some(engine_display_name(&voice.engine_id.0).into()),
                icon: None,
            })
            .collect();
        let labels = PickerLabels {
            placeholder: tr!("tts_aliases_strategy_voice_search").into(),
            empty: tr!("widget_picker_no_results").into(),
            loading: tr!("widget_picker_loading").into(),
        };
        let palette = cx.palette();
        let picker = cx.new(|cx| Picker::new(labels, items, palette, cx));
        let sub = cx.subscribe(&picker, Self::on_voice_picker_event);
        picker.update(cx, |p, cx| p.focus(window, cx));
        self.strategy_error = None;
        self.voice_picker = Some(VoicePicker {
            picker,
            voices,
            _sub: sub,
        });
        cx.notify();
    }

    fn available_voices(&self) -> Arc<Vec<TtsVoice>> {
        self.speak
            .as_ref()
            .map(|speak| speak.available_voices())
            .unwrap_or_default()
    }

    fn on_voice_picker_event(
        &mut self,
        _picker: Entity<Picker>,
        event: &PickerEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(pending) = self.voice_picker.take() else {
            return;
        };
        if let PickerEvent::Selected(id) = event
            && let Some(voice) = id
                .parse::<usize>()
                .ok()
                .and_then(|ix| pending.voices.get(ix))
        {
            self.persist_strategy(
                AssignmentStrategy::Single {
                    voice_id: voice.id.clone(),
                    engine_id: voice.engine_id.clone(),
                },
                cx,
            );
        }
        cx.notify();
    }

    fn close_voice_picker(&mut self, cx: &mut Context<Self>) {
        self.voice_picker = None;
        cx.notify();
    }

    fn persist_strategy(&mut self, strategy: AssignmentStrategy, cx: &mut Context<Self>) {
        if !self.strategy.is_loaded() {
            return;
        }
        if let Some(current) = self.strategy.loaded_mut() {
            *current = choice_from_strategy(&strategy);
        }
        self.single_voice = single_voice_of(&strategy);
        let repo = Arc::clone(&self.repo);
        let speak = self.speak.clone();
        self.strategy_writer.write(
            async move {
                repo.set_strategy(&strategy).await?;
                if let Some(handle) = speak
                    && let Err(error) = handle.send(SpeakCommand::SetStrategy(strategy)).await
                {
                    tracing::warn!(%error, "voice strategy hot-reload failed");
                }
                Ok::<(), StorageError>(())
            },
            Self::strategy_write_settled,
            cx,
        );
        cx.notify();
    }

    fn open_assign(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let form = cx.new(|cx| AliasForm::new(None, None, false, AliasValues::default(), cx));
        self.mount_form(form, window, cx);
    }

    fn open_edit(&mut self, id: &AliasId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.row_by_id(id) else {
            return;
        };
        let id_keyed =
            row.viewer_id != AliasKey::new(row.platform.kind(), &row.viewer_name).to_stored();
        let identity = AliasIdentity {
            key: row.viewer_id.clone(),
            name: row.viewer_name.clone(),
            platform: row.platform,
        };
        let values = AliasValues {
            engine: Some(row.engine_id.clone()),
            voice: row.voice_id.clone(),
            pitch: fmt_field(row.pitch_semitones),
            rate: fmt_field(row.rate_multiplier),
            blocked: row.blocked,
        };
        let id = row.id.clone();
        let form = cx.new(|cx| AliasForm::new(Some(id), Some(identity), id_keyed, values, cx));
        self.mount_form(form, window, cx);
    }

    fn mount_form(&mut self, form: Entity<AliasForm>, window: &mut Window, cx: &mut Context<Self>) {
        focus_inside_modal(window, cx, |window, cx| {
            form.update(cx, |f, cx| f.focus(window, cx))
        });
        self._form_sub = Some(cx.subscribe(&form, Self::on_form_event));
        self.form = Some(form);
        cx.notify();
    }

    fn on_form_event(
        &mut self,
        _form: Entity<AliasForm>,
        event: &AliasFormEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            AliasFormEvent::Submit(alias) => self.persist(alias.clone(), cx),
            AliasFormEvent::Cancel => self.close_form(cx),
        }
    }

    fn close_form(&mut self, cx: &mut Context<Self>) {
        self.form = None;
        self._form_sub = None;
        cx.notify();
    }

    fn persist(&mut self, alias: VoiceAlias, cx: &mut Context<Self>) {
        if let Some(form) = self.form.as_ref() {
            form.update(cx, |f, cx| f.set_saving(true, cx));
        }
        cx.notify();

        let repo = Arc::clone(&self.repo);
        let speak = self.speak.clone();
        let replaces_existing = self
            .form
            .as_ref()
            .is_some_and(|form| form.read(cx).is_editing());
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                save_alias(repo.as_ref(), speak.as_ref(), &alias, replaces_existing).await?;
                repo.list().await
            },
            |this, result, cx| match result {
                Ok(aliases) => {
                    this.apply_aliases(aliases, cx);
                    this.close_form(cx);
                }
                Err(error) => {
                    if let Some(form) = this.form.as_ref() {
                        form.update(cx, |f, cx| f.set_saving(false, cx));
                    }
                    if matches!(error, StorageError::AliasViewerTaken) {
                        tracing::warn!(error = %error, "voice alias viewer already taken");
                        this.loading = false;
                        cx.push_toast(ToastKind::Error, tr!("tts_aliases_viewer_taken"));
                        cx.notify();
                    } else {
                        this.on_repo_error(&error.into(), cx);
                    }
                }
            },
            cx,
        );
    }

    fn preview(&self, id: &AliasId, cx: &mut Context<Self>) {
        let Some(row) = self.row_by_id(id) else {
            return;
        };
        if row.blocked {
            return;
        }
        let Some(handle) = self.speak.clone() else {
            return;
        };
        let viewer_id = row.viewer_id.clone();
        let viewer_name = row.viewer_name.clone();
        let text = tr!("tts_aliases_preview_text");
        async_bridge::report_failure(
            &self.rt_handle,
            async move {
                let request = SpeakRequest {
                    request_id: RequestId::new(),
                    viewer_id,
                    viewer_name,
                    text,
                    priority: Priority::Normal,
                    engine_override: None,
                    voice_override: None,
                    source_event_id: None,
                    is_reward: false,
                    target: None,
                    message_emotes: EmoteTokenSet::default(),
                };
                handle.send(SpeakCommand::Enqueue(request)).await
            },
            async_bridge::ErrorSink::Toast,
            tr!("tts_aliases_preview_failed"),
            cx,
        );
    }

    fn request_delete(&mut self, id: AliasId, cx: &mut Context<Self>) {
        self.pending_delete.request(id);
        cx.notify();
    }

    fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.pending_delete.cancel();
        cx.notify();
    }

    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.pending_delete.take() else {
            return;
        };
        let Some(row) = self.row_by_id(&pending) else {
            cx.notify();
            return;
        };
        let id = row.id.clone();
        cx.notify();

        let repo = Arc::clone(&self.repo);
        let speak = self.speak.clone();
        self.spawn_write(
            async move {
                repo.delete(&id).await.map_err(RepoFailure::from)?;
                if let Some(handle) = speak
                    && let Err(e) = handle.send(SpeakCommand::RemoveAlias(id)).await
                {
                    tracing::warn!(error = %e, "voice alias hot-reload (remove) failed");
                }
                repo.list().await.map_err(RepoFailure::from)
            },
            cx,
        );
    }

    fn voice_label(&self, voice: &SingleVoice) -> String {
        self.available_voices()
            .iter()
            .find(|known| known.id == voice.voice_id && known.engine_id == voice.engine_id)
            .map_or_else(|| voice.voice_id.0.clone(), |known| known.name.clone())
    }

    fn strategy_banner(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let current = self.strategy.loaded().copied();
        let segments = StrategyChoice::ALL
            .into_iter()
            .map(|choice| {
                segment(
                    SharedString::from(format!("va-strat-{}", choice.key())),
                    choice.label(),
                    current == Some(choice),
                    cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.choose_strategy(choice, window, cx)
                    }),
                )
                .disabled(current.is_none())
            })
            .collect();
        let voice_popover = self.voice_picker.as_ref().map(|pending| {
            let view = cx.entity();
            dropdown(pending.picker.clone()).on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_voice_picker(cx));
            })
        });
        let strategy_segments = div()
            .relative()
            .child(segmented(segments, palette))
            .children(voice_popover);
        let sublabel = match (current, &self.single_voice) {
            (Some(StrategyChoice::SingleVoice), Some(voice)) => tr!(
                "tts_aliases_strategy_single_current",
                voice = self.voice_label(voice)
            ),
            _ => tr!("tts_aliases_strategy_sublabel"),
        };

        let heading = div()
            .flex_1()
            .flex()
            .flex_col()
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(BANNER_TITLE_FS)
                    .mb(BANNER_TITLE_MB)
                    .text_color(palette.text_primary)
                    .child(tr!("tts_aliases_strategy_label")),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(META_FS)
                    .text_color(palette.text_muted)
                    .child(sublabel),
            );

        let row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(scaled(BANNER_GAP, density))
            .child(icon(Icon::Wand, BANNER_ICON, palette.brand))
            .child(heading)
            .child(strategy_segments);
        let notice: Option<AnyElement> = if let Some(reason) = self.strategy.failure() {
            Some(
                error_row_with_retry(
                    "va-strategy-retry",
                    tr!("tts_aliases_strategy_load_failed", reason = reason),
                    palette,
                    cx.listener(|this, _: &ClickEvent, _, cx| this.retry_strategy(cx)),
                    density,
                )
                .into_any_element(),
            )
        } else {
            self.strategy_error.as_ref().map(|reason| {
                error_row(
                    tr!("tts_aliases_strategy_save_failed", reason = reason.as_str()),
                    palette,
                    density,
                )
                .into_any_element()
            })
        };
        let body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(scaled(BANNER_GAP, density))
            .child(row)
            .children(notice);

        div()
            .w_full()
            .px(scaled(PAGE_PAD_H, density))
            .py(scaled(PAGE_GAP_V, density))
            .child(
                card(body, palette)
                    .padding_xy(scaled(BANNER_PAD_V, density), scaled(BANNER_PAD_H, density))
                    .full_width(),
            )
            .into_any_element()
    }

    fn toolbar(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let count = div()
            .font_family(body_family())
            .text_size(META_FS)
            .text_color(palette.text_muted)
            .child(tr!(
                "tts_aliases_count",
                count = i64::try_from(self.total_count).unwrap_or(i64::MAX)
            ));

        let assign = primary_button_with_icon(Icon::Plus, tr!("tts_aliases_assign_btn"), palette)
            .on_click(
                "va-assign",
                cx.listener(|this, _: &ClickEvent, window, cx| this.open_assign(window, cx)),
            );

        let right = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(count)
            .child(assign);

        toolbar_row(
            div()
                .w(SEARCH_FIELD_WIDTH)
                .child(self.search.field().clone()),
            right,
        )
        .density(density)
        .px(scaled(PAGE_PAD_H, density))
        .pb(scaled(PAGE_GAP_V, density))
        .into_any_element()
    }

    fn table(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let columns = vec![
            column(
                tr!("tts_aliases_col_viewer"),
                ColumnWidth::Flex(VIEWER_GROW),
            ),
            column(tr!("tts_aliases_col_voice"), ColumnWidth::Flex(VOICE_GROW)),
            column(tr!("tts_aliases_col_pitch"), ColumnWidth::Flex(PITCH_GROW)),
            column(tr!("tts_aliases_col_speed"), ColumnWidth::Flex(SPEED_GROW)),
            column(
                tr!("tts_aliases_col_actions"),
                ColumnWidth::Fixed(ACTIONS_W),
            )
            .align_end(),
        ];

        let hover_bg = with_alpha(palette.border_regular, ROW_HOVER_ALPHA);
        let mut frame = div()
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .rounded(TABLE_RADIUS)
            .overflow_hidden();

        let shown = self.visible.len();
        if self.visible.is_empty() {
            let caption = if self.loading {
                tr!("tts_aliases_loading")
            } else {
                tr!("tts_aliases_empty")
            };
            let state = if self.load_failed {
                div()
                    .p(spacing(Spacing::Md, density))
                    .child(error_row_with_retry(
                        "va-load-retry",
                        tr!("tts_aliases_load_failed"),
                        palette,
                        cx.listener(|this, _: &ClickEvent, _, cx| this.retry_load(cx)),
                        density,
                    ))
                    .into_any_element()
            } else {
                let mut state = empty_state(caption, palette).density(density);
                if self.loading {
                    state = state.loading("voice-aliases-loading");
                }
                state.into_any_element()
            };
            let header_only = data_table(palette, columns, Vec::new())
                .density(density)
                .header_padding(scaled(HEADER_PAD_V, density), scaled(HEADER_PAD_H, density))
                .trailing_rule(false);
            frame = frame
                .child(header_only)
                .child(div().flex_1().min_h(px(0.0)).child(state));
        } else {
            let pal = *palette;
            let body = virtual_table(
                "va-table-scroll",
                palette,
                columns,
                shown,
                &self.table_scroll,
                density,
            )
            .header_padding(scaled(HEADER_PAD_V, density), scaled(HEADER_PAD_H, density))
            .row_padding(scaled(ROW_PAD_V, density), scaled(ROW_PAD_H, density))
            .row_hover(hover_bg)
            .trailing_rule(false)
            .build(
                move |this, ix, _window, cx| {
                    let index = this.visible[ix];
                    match this.aliases.get(index) {
                        Some(row) => this.alias_row(row, &pal, density, cx),
                        None => DataRow::new(Vec::new()),
                    }
                },
                cx,
            );
            frame = frame.child(body);
        }

        let auto = self.viewer_count.saturating_sub(self.total_count);
        let footer = div()
            .w_full()
            .pt(scaled(FOOTER_PAD_TOP, density))
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(tr!(
                "tts_aliases_footer_caption",
                shown = i64::try_from(shown).unwrap_or(i64::MAX),
                total = i64::try_from(self.total_count).unwrap_or(i64::MAX),
                auto = i64::try_from(auto).unwrap_or(i64::MAX)
            ));

        div()
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .px(scaled(PAGE_PAD_H, density))
            .pb(spacing(Spacing::Md, density))
            .child(frame)
            .child(footer)
            .into_any_element()
    }

    fn alias_row(
        &self,
        row: &AliasRow,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> DataRow {
        let muted = row.blocked;
        let row_key: SharedString = row.id.0.clone().into();
        let name_color = if muted {
            palette.text_muted
        } else {
            palette.text_primary
        };

        let initial = row
            .viewer_name
            .chars()
            .next()
            .unwrap_or('?')
            .to_uppercase()
            .next()
            .unwrap_or('?');
        let avatar_bg = if muted {
            palette.text_extreme_faint
        } else {
            hash_accent(&row.viewer_name, palette)
        };
        let avatar = avatar_tile(initial.to_string(), avatar_bg, palette)
            .size(AVATAR)
            .font(FONT_XXS)
            .mono();
        let mut viewer_inner = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(avatar)
            .child(
                div()
                    .min_w(px(0.0))
                    .truncate()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_XS)
                    .text_color(name_color)
                    .child(row.viewer_name.clone()),
            );
        if let Some(kind) = row.platform.kind() {
            viewer_inner = viewer_inner.child(platform_badge(
                row.platform.label(),
                platform_color(kind, palette),
                palette,
                density,
            ));
        }
        if muted {
            viewer_inner = viewer_inner.child(role_badge(
                tr!("tts_aliases_role_blocked"),
                palette.danger,
                palette,
            ));
        }

        let voice_inner: AnyElement = if muted {
            div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xxs, density))
                .child(icon(Icon::VolumeOff, ENGINE_GLYPH, palette.danger))
                .child(
                    div()
                        .font_family(mono_family())
                        .text_size(VOICE_FS)
                        .text_color(palette.text_faint)
                        .child(tr!("tts_aliases_never_speak")),
                )
                .into_any_element()
        } else {
            let (glyph, glyph_color) = engine_visual(&row.engine_id, palette);
            div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xxs, density))
                .child(icon(glyph, ENGINE_GLYPH, glyph_color))
                .child(
                    div()
                        .min_w(px(0.0))
                        .truncate()
                        .font_family(mono_family())
                        .text_size(VOICE_FS)
                        .text_color(palette.text_primary)
                        .child(format!("{} · {}", row.engine_label, row.voice_id)),
                )
                .into_any_element()
        };

        let (pitch_color, speed_color) = if muted {
            (palette.text_extreme_faint, palette.text_extreme_faint)
        } else {
            (palette.text_muted, palette.text_muted)
        };
        let pitch_cell = mono_cell(fmt_pitch(row.pitch_semitones, muted), pitch_color);
        let speed_cell = mono_cell(fmt_rate(row.rate_multiplier, muted), speed_color);

        let preview_color = if muted {
            palette.text_extreme_faint
        } else {
            palette.success
        };
        let preview_alias = row.id.clone();
        let edit_alias = row.id.clone();
        let delete_alias = row.id.clone();
        let mut preview = div()
            .id((gpui::ElementId::from("va-preview"), row_key.clone()))
            .flex()
            .child(icon(Icon::PlayerPlay, ACTION_GLYPH, preview_color));
        if !muted {
            preview = preview.cursor_pointer().on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.preview(&preview_alias, cx)),
            );
        }
        let edit = div()
            .id((gpui::ElementId::from("va-edit"), row_key.clone()))
            .flex()
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.open_edit(&edit_alias, window, cx)
            }))
            .child(icon(Icon::Pencil, ACTION_GLYPH, palette.text_faint));
        let delete = div()
            .id((gpui::ElementId::from("va-delete"), row_key.clone()))
            .flex()
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.request_delete(delete_alias.clone(), cx)
            }))
            .child(icon(Icon::Trash, ACTION_GLYPH, palette.text_faint));
        let actions = div()
            .w_full()
            .flex()
            .items_center()
            .justify_end()
            .gap(spacing(Spacing::Sm, density))
            .child(preview)
            .child(edit)
            .child(delete);

        DataRow::new(vec![
            viewer_inner.into_any_element(),
            voice_inner,
            pitch_cell.into_any_element(),
            speed_cell.into_any_element(),
            actions.into_any_element(),
        ])
    }

    fn active_overlay(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> Option<AnyElement> {
        if let Some(form) = self.form.as_ref() {
            Some(form.clone().into_any_element())
        } else {
            self.pending_delete
                .get()
                .map(|id| self.delete_confirm(id, palette, cx))
        }
    }

    fn delete_confirm(
        &self,
        id: &AliasId,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let viewer = self
            .row_by_id(id)
            .map(|a| a.viewer_name.clone())
            .unwrap_or_default();
        let message = tr!("tts_aliases_delete_body", viewer = viewer.as_str());

        let card = confirm_modal(
            tr!("tts_aliases_delete_title"),
            message,
            ConfirmTone::Destructive,
            palette,
        )
        .esc_hint(tr!("widget_confirm_esc_to_cancel"))
        .on_cancel(
            "va-delete-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_delete(cx)),
        )
        .on_confirm(
            "va-delete-confirm",
            tr!("common_delete"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_delete(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("va-delete-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_delete(cx));
            })
            .into_any_element()
    }
}

impl Render for VoiceAliasesView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let banner = self.strategy_banner(&palette, cx);
        let toolbar = self.toolbar(&palette, density, cx);
        let table = self.table(&palette, density, cx);
        let overlay = self.active_overlay(&palette, cx);

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(banner)
            .child(toolbar)
            .child(table)
            .children(overlay)
    }
}

fn mono_cell(value: String, color: Rgba) -> impl IntoElement {
    div()
        .font_family(mono_family())
        .text_size(META_FS)
        .text_color(color)
        .child(value)
}

fn role_badge(
    label: impl Into<SharedString>,
    color: Rgba,
    palette: &ForgePalette,
) -> impl IntoElement {
    badge(palette.surface_overlay, color, label, true, ROLE_BADGE_FS)
}

fn platform_badge(
    label: String,
    dot: Rgba,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xxs, density))
        .py(scaled(PLATFORM_BADGE_PAD_V, density))
        .px(spacing(Spacing::Xs, density))
        .rounded(radius(Radius::Pill))
        .bg(palette.surface_overlay)
        .child(status_dot(dot, PLATFORM_DOT))
        .child(
            div()
                .font_family(mono_family())
                .text_size(ROLE_BADGE_FS)
                .text_color(palette.text_muted)
                .child(label),
        )
}

fn row_from_alias(a: VoiceAlias) -> AliasRow {
    let engine = a.engine_id.0;
    let engine_label = engine_display_name(&engine).to_owned();
    let voice = a.voice_id.0;
    let key = AliasKey::parse(&a.viewer_id);
    let platform = alias_key_scope(&key);
    let named = AliasKey::parse(&a.viewer_name);
    let viewer_name = match named.platform {
        Some(_) if named.platform == key.platform => named.handle,
        _ => a.viewer_name,
    };
    AliasRow {
        id: a.id,
        viewer_id: a.viewer_id,
        viewer_name,
        platform,
        engine_id: engine,
        engine_label,
        voice_id: voice,
        pitch_semitones: a.pitch_semitones,
        rate_multiplier: a.rate_multiplier,
        blocked: matches!(a.state, AliasState::Blocked),
    }
}

fn single_voice_of(strategy: &AssignmentStrategy) -> Option<SingleVoice> {
    match strategy {
        AssignmentStrategy::Single {
            voice_id,
            engine_id,
        } => Some(SingleVoice {
            voice_id: voice_id.clone(),
            engine_id: engine_id.clone(),
        }),
        AssignmentStrategy::DeterministicByName | AssignmentStrategy::Random => None,
    }
}

fn choice_from_strategy(strategy: &AssignmentStrategy) -> StrategyChoice {
    match strategy {
        AssignmentStrategy::DeterministicByName => StrategyChoice::DeterministicByName,
        AssignmentStrategy::Random => StrategyChoice::Random,
        AssignmentStrategy::Single { .. } => StrategyChoice::SingleVoice,
    }
}

fn engine_visual(engine_id: &str, palette: &ForgePalette) -> (Icon, Rgba) {
    match engine_id {
        forge_tts_core::ENGINE_ID_PIPER => (Icon::Cpu, palette.success),
        forge_tts_core::ENGINE_ID_ESPEAK_NG
        | forge_tts_core::ENGINE_ID_SAPI
        | forge_tts_core::ENGINE_ID_NSSPEECH => (Icon::Terminal, palette.success),
        forge_tts_core::ENGINE_ID_ELEVENLABS => (Icon::Microphone2, palette.theme_brand),
        forge_tts_core::ENGINE_ID_POLLY => (Icon::BrandAws, palette.accent_peach),
        forge_tts_core::ENGINE_ID_AZURE => (Icon::Cloud, palette.info),
        forge_tts_core::ENGINE_ID_OPENAI => (Icon::Bolt, palette.accent_teal),
        _ => (Icon::Cloud, palette.text_muted),
    }
}

fn fmt_pitch_amount(semitones: f32) -> String {
    format!("{semitones:.2}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

fn fmt_pitch(value: Option<f32>, blocked: bool) -> String {
    if blocked {
        return "-".to_owned();
    }
    match value {
        Some(p) if p >= 0.0 => format!("+{} st", fmt_pitch_amount(p)),
        Some(p) => format!("{} st", fmt_pitch_amount(p)),
        None => "0 st".to_owned(),
    }
}

fn fmt_rate(value: Option<f32>, blocked: bool) -> String {
    if blocked {
        return "-".to_owned();
    }
    value
        .map(|r| format!("{r:.1}x"))
        .unwrap_or_else(|| "1.0x".to_owned())
}

#[cfg(test)]
mod tests {

    use forge_components::ThemeId;
    use forge_speech_types::{EngineId, VoiceId};
    use forge_storage::{DataProvider, Language};
    use gpui::{TestAppContext, VisualTestContext};

    use super::*;
    use crate::i18n::install_language;
    use crate::test_support::{
        LiveAliases, Sandboxed, alias_shape, error_toasts, live_alias, runtime, sandboxed_backend,
        settle_until, spawn_speak_queue,
    };
    use forge_components::Presentation;
    use forge_components::Toasts;

    const TEST_KEY: [u8; 32] = [0x5a; 32];

    struct Fixture {
        backend: Sandboxed<forge_storage_sqlite::SqliteBackend>,
        resolver: LiveAliases,
        view: Entity<VoiceAliasesView>,
    }

    struct Services {
        backend: Sandboxed<forge_storage_sqlite::SqliteBackend>,
        speak: forge_speak_queue::SpeakQueueHandle,
        resolver: LiveAliases,
    }

    impl Services {
        fn view(
            &self,
            rt: &tokio::runtime::Runtime,
            cx: &mut Context<VoiceAliasesView>,
        ) -> VoiceAliasesView {
            VoiceAliasesView::new(
                self.backend.voice_alias_repo(),
                self.backend.viewer_repo(),
                Some(self.speak.clone()),
                rt.handle().clone(),
                cx,
            )
        }

        fn into_fixture(self, view: Entity<VoiceAliasesView>) -> Fixture {
            Fixture {
                backend: self.backend,
                resolver: self.resolver,
                view,
            }
        }
    }

    fn keyed(id: &str, viewer_id: &str, voice: &str) -> VoiceAlias {
        VoiceAlias {
            id: AliasId(id.to_owned()),
            voice_id: VoiceId(voice.to_owned()),
            ..stored_alias(viewer_id, viewer_id)
        }
    }

    fn services(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        seeded: Vec<VoiceAlias>,
    ) -> Services {
        install_language(Language::En);
        cx.update(|cx| {
            cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
            cx.set_global(Toasts::new());
        });
        let backend = rt.block_on(sandboxed_backend("sqlite::memory:", TEST_KEY));
        let repo = backend.voice_alias_repo();
        for alias in &seeded {
            rt.block_on(repo.upsert(alias)).expect("seed alias");
        }
        let (speak, _events, resolver) = rt.block_on(async { spawn_speak_queue(seeded) });
        Services {
            backend,
            speak,
            resolver,
        }
    }

    fn mount(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        seeded: Vec<VoiceAlias>,
    ) -> Fixture {
        let services = services(cx, rt, seeded);
        let view = cx.update(|cx| cx.new(|cx| services.view(rt, cx)));
        services.into_fixture(view)
    }

    fn mount_in_window<'a>(
        cx: &'a mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        seeded: Vec<VoiceAlias>,
    ) -> (Fixture, &'a mut VisualTestContext) {
        let services = services(cx, rt, seeded);
        let (view, vcx) = cx.add_window_view(|_window, cx| services.view(rt, cx));
        vcx.update(|window, cx| {
            window.activate_window();
            forge_components::bind_text_input_keys(cx);
        });
        (services.into_fixture(view), vcx)
    }

    fn live_alias_for(fixture: &Fixture, viewer_id: &str) -> Option<VoiceAlias> {
        live_alias(&fixture.resolver, viewer_id)
    }

    #[gpui::test]
    fn saving_under_a_fresh_id_hot_reloads_the_alias_with_the_stored_id(cx: &mut TestAppContext) {
        let rt = runtime();
        let fixture = mount(cx, &rt, vec![keyed("stored", "twitch:alice", "old-voice")]);

        fixture.view.update(cx, |view, cx| {
            view.persist(keyed("fresh", "twitch:alice", "new-voice"), cx)
        });
        settle_until(cx, &rt, |_| {
            live_alias_for(&fixture, "twitch:alice")
                .is_some_and(|alias| alias.voice_id.0 == "new-voice")
        });

        let live = live_alias_for(&fixture, "twitch:alice").expect("live alias");
        assert_eq!(live.id.0, "stored");
    }

    #[gpui::test]
    fn saving_an_alias_onto_a_viewer_held_by_another_alias_shows_an_error_toast(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let fixture = mount(
            cx,
            &rt,
            vec![
                keyed("a", "twitch:alice", "voice-a"),
                keyed("b", "twitch:bob", "voice-b"),
            ],
        );

        fixture.view.update(cx, |view, cx| {
            view.persist(keyed("a", "twitch:bob", "voice-edited"), cx)
        });
        settle_until(cx, &rt, |cx| cx.update(|cx| error_toasts(cx)) > 0);

        let messages: Vec<String> = cx.update(|cx| {
            cx.global::<Toasts>()
                .items()
                .iter()
                .map(|toast| toast.message.to_string())
                .collect()
        });
        assert_eq!(messages, [tr!("tts_aliases_viewer_taken")]);
    }

    fn stored_ids(fixture: &Fixture, rt: &tokio::runtime::Runtime) -> Vec<String> {
        let mut ids = rt
            .block_on(fixture.backend.voice_alias_repo().list())
            .expect("list")
            .into_iter()
            .map(|alias| alias.id.0)
            .collect::<Vec<_>>();
        ids.sort();
        ids
    }

    fn mount_loaded(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        seeded: Vec<VoiceAlias>,
    ) -> Fixture {
        let fixture = mount(cx, rt, seeded);
        let view = fixture.view.clone();
        settle_until(cx, rt, |cx| {
            view.read_with(cx, |view, _| !view.aliases.is_empty())
        });
        fixture
    }

    #[gpui::test]
    fn confirming_a_delete_after_the_list_was_reordered_deletes_the_chosen_alias(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let alice = keyed("a", "twitch:alice", "voice-a");
        let bob = keyed("b", "twitch:bob", "voice-b");
        let fixture = mount_loaded(cx, &rt, vec![alice.clone(), bob.clone()]);

        fixture.view.update(cx, |view, cx| {
            view.request_delete(AliasId("b".to_owned()), cx);
            view.apply_aliases(vec![bob, alice], cx);
            view.confirm_delete(cx);
        });
        settle_until(cx, &rt, |_| stored_ids(&fixture, &rt).len() == 1);

        assert_eq!(stored_ids(&fixture, &rt), ["a"]);
    }

    #[gpui::test]
    fn confirming_a_delete_of_an_alias_gone_from_the_list_deletes_nothing(cx: &mut TestAppContext) {
        let rt = runtime();
        let alice = keyed("a", "twitch:alice", "voice-a");
        let bob = keyed("b", "twitch:bob", "voice-b");
        let fixture = mount_loaded(cx, &rt, vec![alice.clone(), bob]);

        fixture.view.update(cx, |view, cx| {
            view.request_delete(AliasId("b".to_owned()), cx);
            view.apply_aliases(vec![alice], cx);
            view.confirm_delete(cx);
        });
        cx.run_until_parked();

        assert_eq!(stored_ids(&fixture, &rt), ["a", "b"]);
    }

    fn stored_alias(viewer_id: &str, viewer_name: &str) -> VoiceAlias {
        VoiceAlias {
            id: AliasId::new(),
            viewer_id: viewer_id.to_owned(),
            viewer_name: viewer_name.to_owned(),
            engine_id: EngineId("piper".to_owned()),
            voice_id: VoiceId("amy".to_owned()),
            pitch_semitones: None,
            rate_multiplier: None,
            state: AliasState::Active,
        }
    }

    #[gpui::test]
    fn editing_a_row_locks_the_viewer_only_when_the_alias_is_keyed_by_viewer_id(
        cx: &mut TestAppContext,
    ) {
        for (viewer_id, viewer_name, saved_under) in [
            ("twitch:141981764", "alice", "twitch:141981764"),
            ("twitch:alice", "alice", "twitch:alicex"),
        ] {
            let rt = runtime();
            let seeded = VoiceAlias {
                id: AliasId("a1".to_owned()),
                ..stored_alias(viewer_id, viewer_name)
            };
            let (fixture, vcx) = mount_in_window(cx, &rt, vec![seeded]);
            let view = fixture.view.clone();
            settle_until(vcx, &rt, |cx| {
                view.read_with(cx, |view, _| !view.aliases.is_empty())
            });

            vcx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.open_edit(&AliasId("a1".to_owned()), window, cx)
                })
            });
            vcx.run_until_parked();
            vcx.simulate_input("x");
            vcx.simulate_keystrokes("enter");
            let repo = fixture.backend.voice_alias_repo();
            let saved = || {
                rt.block_on(repo.list())
                    .expect("list")
                    .into_iter()
                    .map(|alias| (alias.viewer_id, alias.voice_id.0))
                    .collect::<Vec<_>>()
            };
            settle_until(vcx, &rt, |_| {
                saved()
                    .iter()
                    .any(|(id, voice)| id != viewer_id || voice != "amy")
            });

            assert_eq!(
                saved().into_iter().map(|(id, _)| id).collect::<Vec<_>>(),
                [saved_under],
                "{viewer_id:?}"
            );
        }
    }

    #[gpui::test]
    fn editing_a_blocked_row_saves_it_still_blocked_with_its_engine_voice_pitch_and_rate(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let seeded = VoiceAlias {
            id: AliasId("a1".to_owned()),
            engine_id: EngineId("elevenlabs".to_owned()),
            pitch_semitones: Some(-2.5),
            rate_multiplier: Some(1.25),
            state: AliasState::Blocked,
            ..stored_alias("twitch:alice", "alice")
        };
        let (fixture, vcx) = mount_in_window(cx, &rt, vec![seeded.clone()]);
        let view = fixture.view.clone();
        settle_until(vcx, &rt, |cx| {
            view.read_with(cx, |view, _| !view.aliases.is_empty())
        });

        vcx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.open_edit(&AliasId("a1".to_owned()), window, cx)
            })
        });
        vcx.run_until_parked();
        vcx.simulate_input("x");
        vcx.simulate_keystrokes("enter");
        let repo = fixture.backend.voice_alias_repo();
        let saved = || rt.block_on(repo.list()).expect("list");
        settle_until(vcx, &rt, |_| {
            saved()
                .iter()
                .any(|alias| alias.viewer_id == "twitch:alicex")
        });

        let renamed = VoiceAlias {
            viewer_id: "twitch:alicex".to_owned(),
            viewer_name: "alicex".to_owned(),
            ..seeded
        };
        assert_eq!(
            saved().iter().map(alias_shape).collect::<Vec<_>>(),
            [alias_shape(&renamed)]
        );
    }

    #[test]
    fn row_shows_the_viewer_name_without_the_platform_prefix() {
        for (viewer_id, viewer_name, platform, shown) in [
            ("twitch:Alice", "Alice", AliasScope::Twitch, "Alice"),
            ("kick:Alice", "kick:Alice", AliasScope::Kick, "Alice"),
            ("Alice", "Alice", AliasScope::Any, "Alice"),
            (
                "youtube:Alice",
                "kick:Alice",
                AliasScope::YouTube,
                "kick:Alice",
            ),
            (
                "discord:Alice",
                "discord:Alice",
                AliasScope::Any,
                "discord:Alice",
            ),
            (
                "Twitch:Alice",
                "Twitch:Alice",
                AliasScope::Any,
                "Twitch:Alice",
            ),
        ] {
            let row = row_from_alias(stored_alias(viewer_id, viewer_name));

            assert_eq!(
                (row.platform, row.viewer_name.as_str()),
                (platform, shown),
                "{viewer_id:?} / {viewer_name:?}"
            );
        }
    }

    const UNDECODABLE_STRATEGY: &str = "strategy: not a known shape";

    #[derive(Default, Clone, Copy, Debug)]
    enum StrategyRead {
        #[default]
        Stored,
        Unreachable,
        Undecodable,
    }

    impl StrategyRead {
        fn failure(self) -> Result<(), StorageError> {
            match self {
                Self::Stored => Ok(()),
                Self::Unreachable => Err(StorageError::Connection {
                    reason: crate::test_support::SETTINGS_READ_FAILED.to_owned(),
                }),
                Self::Undecodable => Err(StorageError::Parse(UNDECODABLE_STRATEGY.to_owned())),
            }
        }
    }

    #[derive(Default)]
    struct StrategyStore {
        stored: AssignmentStrategy,
        read: StrategyRead,
        read_gate: Option<Arc<tokio::sync::Notify>>,
        writes_fail: bool,
        writes: Vec<AssignmentStrategy>,
    }

    #[derive(Default)]
    struct FakeStrategyRepo(std::sync::Mutex<StrategyStore>);

    impl FakeStrategyRepo {
        fn holding(stored: AssignmentStrategy) -> Arc<Self> {
            let repo = Self::default();
            repo.lock().stored = stored;
            Arc::new(repo)
        }

        fn lock(&self) -> std::sync::MutexGuard<'_, StrategyStore> {
            self.0.lock().unwrap_or_else(|p| p.into_inner())
        }

        fn writes(&self) -> Vec<AssignmentStrategy> {
            std::mem::take(&mut self.lock().writes)
        }
    }

    #[async_trait::async_trait]
    impl VoiceAliasRepo for FakeStrategyRepo {
        async fn list(&self) -> Result<Vec<VoiceAlias>, StorageError> {
            Ok(Vec::new())
        }

        async fn upsert(&self, alias: &VoiceAlias) -> Result<VoiceAlias, StorageError> {
            Ok(alias.clone())
        }

        async fn delete(&self, _id: &AliasId) -> Result<(), StorageError> {
            Ok(())
        }

        async fn find_by_viewer(
            &self,
            _viewer_id: &str,
        ) -> Result<Option<VoiceAlias>, StorageError> {
            Ok(None)
        }

        async fn get_strategy(&self) -> Result<AssignmentStrategy, StorageError> {
            let gate = self.lock().read_gate.clone();
            if let Some(gate) = gate {
                gate.notified().await;
            }
            let store = self.lock();
            store.read.failure()?;
            Ok(store.stored.clone())
        }

        async fn set_strategy(&self, strategy: &AssignmentStrategy) -> Result<(), StorageError> {
            let mut store = self.lock();
            if store.writes_fail {
                return Err(StorageError::Connection {
                    reason: crate::test_support::SETTINGS_WRITE_FAILED.to_owned(),
                });
            }
            store.stored = strategy.clone();
            store.writes.push(strategy.clone());
            Ok(())
        }

        async fn get_ignore_profile(
            &self,
        ) -> Result<forge_speech_types::IgnoreProfile, StorageError> {
            Ok(forge_speech_types::IgnoreProfile::default())
        }

        async fn set_ignore_profile(
            &self,
            _profile: &forge_speech_types::IgnoreProfile,
        ) -> Result<(), StorageError> {
            Ok(())
        }
    }

    struct StrategyHarness {
        rt: tokio::runtime::Runtime,
        repo: Arc<FakeStrategyRepo>,
    }

    impl StrategyHarness {
        fn new(stored: AssignmentStrategy, cx: &mut TestAppContext) -> Self {
            install_language(Language::En);
            cx.update(|cx| {
                cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
                cx.set_global(Toasts::new());
            });
            Self {
                rt: runtime(),
                repo: FakeStrategyRepo::holding(stored),
            }
        }

        fn open(&self, cx: &mut TestAppContext) -> Entity<VoiceAliasesView> {
            let mut viewers = forge_storage::MockViewerRepo::new();
            viewers.expect_count().returning(|| Ok(0));
            let repo = Arc::clone(&self.repo) as Arc<dyn VoiceAliasRepo>;
            let handle = self.rt.handle().clone();
            let view = cx.update(|cx| {
                cx.new(|cx| VoiceAliasesView::new(repo, Arc::new(viewers), None, handle, cx))
            });
            self.settle(cx);
            view
        }

        fn settle(&self, cx: &mut TestAppContext) {
            for _ in 0..3 {
                crate::test_support::pump(&self.rt);
                cx.run_until_parked();
            }
        }
    }

    fn shown_strategy(
        view: &Entity<VoiceAliasesView>,
        cx: &mut TestAppContext,
    ) -> Option<StrategyChoice> {
        view.read_with(cx, |view, _| view.strategy.loaded().copied())
    }

    const STRATEGY_EDITS: [StrategyChoice; 2] =
        [StrategyChoice::DeterministicByName, StrategyChoice::Random];

    #[gpui::test]
    fn no_strategy_pick_after_a_failed_load_writes_over_the_stored_strategy(
        cx: &mut TestAppContext,
    ) {
        for read in [StrategyRead::Unreachable, StrategyRead::Undecodable] {
            let reason = crate::test_support::shown_in_english(
                read.failure().expect_err("a faulty read fails"),
            );
            for choice in STRATEGY_EDITS {
                let harness = StrategyHarness::new(AssignmentStrategy::Random, cx);
                harness.repo.lock().read = read;
                let view = harness.open(cx);

                view.update(cx, |view, cx| view.set_strategy(choice, cx));
                harness.settle(cx);

                let failure_shown = view.read_with(cx, |view, _| view.strategy.failure());
                assert_eq!(
                    (harness.repo.writes(), failure_shown),
                    (Vec::new(), Some(reason.clone())),
                    "{read:?}: {choice:?}"
                );
            }
        }
    }

    #[gpui::test]
    fn no_strategy_pick_while_the_load_is_pending_writes_and_the_load_still_lands_intact(
        cx: &mut TestAppContext,
    ) {
        for choice in STRATEGY_EDITS {
            let harness = StrategyHarness::new(AssignmentStrategy::Random, cx);
            let gate = Arc::new(tokio::sync::Notify::new());
            harness.repo.lock().read_gate = Some(Arc::clone(&gate));
            let view = harness.open(cx);

            view.update(cx, |view, cx| view.set_strategy(choice, cx));
            harness.settle(cx);
            let while_pending = harness.repo.writes();
            harness.repo.lock().read_gate = None;
            gate.notify_one();
            harness.settle(cx);

            assert_eq!(
                (while_pending, shown_strategy(&view, cx)),
                (Vec::new(), Some(StrategyChoice::Random)),
                "{choice:?}"
            );
        }
    }

    #[gpui::test]
    fn retry_after_a_failed_strategy_load_restores_it_and_a_pick_persists(cx: &mut TestAppContext) {
        let harness = StrategyHarness::new(AssignmentStrategy::Random, cx);
        harness.repo.lock().read = StrategyRead::Unreachable;
        let view = harness.open(cx);
        harness.repo.lock().read = StrategyRead::Stored;

        view.update(cx, |view, cx| view.retry_strategy(cx));
        harness.settle(cx);
        let restored = shown_strategy(&view, cx);
        view.update(cx, |view, cx| {
            view.set_strategy(StrategyChoice::DeterministicByName, cx)
        });
        harness.settle(cx);

        assert_eq!(
            (restored, harness.repo.writes()),
            (
                Some(StrategyChoice::Random),
                vec![AssignmentStrategy::DeterministicByName]
            )
        );
    }

    #[gpui::test]
    fn a_failed_strategy_save_shows_the_error_and_reloads_the_stored_strategy(
        cx: &mut TestAppContext,
    ) {
        let harness = StrategyHarness::new(AssignmentStrategy::Random, cx);
        harness.repo.lock().writes_fail = true;
        let view = harness.open(cx);

        view.update(cx, |view, cx| {
            view.set_strategy(StrategyChoice::DeterministicByName, cx)
        });
        harness.settle(cx);

        let read_only = crate::test_support::shown_in_english(StorageError::Connection {
            reason: crate::test_support::SETTINGS_WRITE_FAILED.to_owned(),
        });
        let save_error = view.read_with(cx, |view, _| view.strategy_error.clone());
        assert_eq!(
            (save_error, shown_strategy(&view, cx)),
            (Some(read_only), Some(StrategyChoice::Random))
        );
    }

    const ENGINE: &str = "test-engine";

    fn listed_voice(id: &str, name: &str) -> TtsVoice {
        TtsVoice {
            id: VoiceId(id.to_owned()),
            name: name.to_owned(),
            locale: "en-US".to_owned(),
            gender: forge_speech_types::VoiceGender::Neutral,
            engine_id: EngineId(ENGINE.to_owned()),
            is_neural: false,
            sample_rate_hint: 22_050,
        }
    }

    fn single(id: &str) -> AssignmentStrategy {
        AssignmentStrategy::Single {
            voice_id: VoiceId(id.to_owned()),
            engine_id: EngineId(ENGINE.to_owned()),
        }
    }

    impl StrategyHarness {
        fn speak_listing(&self, voices: Vec<TtsVoice>) -> SpeakQueueHandle {
            crate::test_support::spawn_speak_queue_listing(
                &self.rt,
                &EngineId(ENGINE.to_owned()),
                voices,
            )
        }

        fn open_in_window<'a>(
            &self,
            speak: Option<SpeakQueueHandle>,
            cx: &'a mut TestAppContext,
        ) -> (Entity<VoiceAliasesView>, &'a mut VisualTestContext) {
            let mut viewers = forge_storage::MockViewerRepo::new();
            viewers.expect_count().returning(|| Ok(0));
            let repo = Arc::clone(&self.repo) as Arc<dyn VoiceAliasRepo>;
            let handle = self.rt.handle().clone();
            let (view, vcx) = cx.add_window_view(|_window, cx| {
                VoiceAliasesView::new(repo, Arc::new(viewers), speak, handle, cx)
            });
            self.settle(vcx);
            (view, vcx)
        }
    }

    fn pick_single_voice(view: &Entity<VoiceAliasesView>, vcx: &mut VisualTestContext) {
        view.update_in(vcx, |view, window, cx| {
            view.choose_strategy(StrategyChoice::SingleVoice, window, cx)
        });
    }

    fn answer_picker(
        view: &Entity<VoiceAliasesView>,
        event: PickerEvent,
        vcx: &mut VisualTestContext,
    ) {
        let picker = view.read_with(vcx, |view, _| {
            view.voice_picker.as_ref().map(|open| open.picker.clone())
        });
        picker
            .expect("the voice picker is open")
            .update(vcx, |_, cx| cx.emit(event));
    }

    fn picker_open(view: &Entity<VoiceAliasesView>, cx: &mut TestAppContext) -> bool {
        view.read_with(cx, |view, _| view.voice_picker.is_some())
    }

    fn sublabel_voice(view: &Entity<VoiceAliasesView>, cx: &mut TestAppContext) -> Option<String> {
        view.read_with(cx, |view, _| {
            view.single_voice
                .as_ref()
                .map(|voice| view.voice_label(voice))
        })
    }

    #[gpui::test]
    fn the_single_voice_choice_explains_itself_when_no_voice_is_available(cx: &mut TestAppContext) {
        let harness = StrategyHarness::new(AssignmentStrategy::Random, cx);
        let speak = harness.speak_listing(Vec::new());
        for speak in [None, Some(speak)] {
            let (view, vcx) = harness.open_in_window(speak, cx);

            pick_single_voice(&view, vcx);
            harness.settle(vcx);

            let error = view.read_with(vcx, |view, _| view.strategy_error.clone());
            assert_eq!(
                (
                    error,
                    picker_open(&view, vcx),
                    harness.repo.writes(),
                    shown_strategy(&view, vcx)
                ),
                (
                    Some("No voices are available yet. Enable a TTS engine first.".to_owned()),
                    false,
                    Vec::new(),
                    Some(StrategyChoice::Random)
                )
            );
        }
    }

    #[gpui::test]
    fn choosing_a_voice_stores_exactly_that_voice_and_closes_the_picker(cx: &mut TestAppContext) {
        let harness = StrategyHarness::new(AssignmentStrategy::Random, cx);
        let speak = harness.speak_listing(vec![
            listed_voice("first", "First"),
            listed_voice("second", "Second"),
        ]);
        let (view, vcx) = harness.open_in_window(Some(speak), cx);
        pick_single_voice(&view, vcx);
        let opened = picker_open(&view, vcx);

        answer_picker(&view, PickerEvent::Selected("1".into()), vcx);
        harness.settle(vcx);

        assert_eq!(
            (
                opened,
                picker_open(&view, vcx),
                harness.repo.writes(),
                shown_strategy(&view, vcx),
                sublabel_voice(&view, vcx)
            ),
            (
                true,
                false,
                vec![single("second")],
                Some(StrategyChoice::SingleVoice),
                Some("Second".to_owned())
            )
        );
    }

    #[gpui::test]
    fn cancelling_the_picker_leaves_the_strategy_and_the_store_alone(cx: &mut TestAppContext) {
        let harness = StrategyHarness::new(single("stored"), cx);
        let speak = harness.speak_listing(vec![listed_voice("stored", "Stored")]);
        let (view, vcx) = harness.open_in_window(Some(speak), cx);
        pick_single_voice(&view, vcx);

        answer_picker(&view, PickerEvent::Cancelled, vcx);
        harness.settle(vcx);

        assert_eq!(
            (
                picker_open(&view, vcx),
                harness.repo.writes(),
                shown_strategy(&view, vcx),
                sublabel_voice(&view, vcx)
            ),
            (
                false,
                Vec::new(),
                Some(StrategyChoice::SingleVoice),
                Some("Stored".to_owned())
            )
        );
    }

    #[gpui::test]
    fn dismissing_the_picker_closes_it_without_writing(cx: &mut TestAppContext) {
        let harness = StrategyHarness::new(AssignmentStrategy::Random, cx);
        let speak = harness.speak_listing(vec![listed_voice("first", "First")]);
        let (view, vcx) = harness.open_in_window(Some(speak), cx);
        pick_single_voice(&view, vcx);

        view.update(vcx, |view, cx| view.close_voice_picker(cx));
        harness.settle(vcx);

        assert_eq!(
            (picker_open(&view, vcx), harness.repo.writes()),
            (false, Vec::new())
        );
    }

    #[gpui::test]
    fn a_picker_answer_naming_no_listed_voice_writes_nothing(cx: &mut TestAppContext) {
        let harness = StrategyHarness::new(AssignmentStrategy::Random, cx);
        let speak = harness.speak_listing(vec![listed_voice("first", "First")]);
        let (view, vcx) = harness.open_in_window(Some(speak), cx);
        for answer in ["7", "not-a-number"] {
            pick_single_voice(&view, vcx);

            answer_picker(&view, PickerEvent::Selected(answer.into()), vcx);
            harness.settle(vcx);

            assert_eq!(
                (picker_open(&view, vcx), harness.repo.writes()),
                (false, Vec::new()),
                "{answer}"
            );
        }
    }

    #[gpui::test]
    fn the_stored_voice_comes_back_by_name_and_by_id_once_its_engine_is_gone(
        cx: &mut TestAppContext,
    ) {
        for (voices, expected) in [
            (vec![listed_voice("stored", "Stored Voice")], "Stored Voice"),
            (vec![listed_voice("other", "Other")], "stored"),
        ] {
            let harness = StrategyHarness::new(single("stored"), cx);
            let speak = harness.speak_listing(voices);
            let (view, vcx) = harness.open_in_window(Some(speak), cx);

            assert_eq!(
                (shown_strategy(&view, vcx), sublabel_voice(&view, vcx)),
                (Some(StrategyChoice::SingleVoice), Some(expected.to_owned()))
            );
        }
    }

    #[gpui::test]
    fn a_failed_voice_write_reloads_the_stored_strategy(cx: &mut TestAppContext) {
        let harness = StrategyHarness::new(AssignmentStrategy::Random, cx);
        harness.repo.lock().writes_fail = true;
        let speak = harness.speak_listing(vec![listed_voice("first", "First")]);
        let (view, vcx) = harness.open_in_window(Some(speak), cx);
        pick_single_voice(&view, vcx);

        answer_picker(&view, PickerEvent::Selected("0".into()), vcx);
        harness.settle(vcx);

        assert_eq!(
            (shown_strategy(&view, vcx), sublabel_voice(&view, vcx)),
            (Some(StrategyChoice::Random), None)
        );
    }

    #[gpui::test]
    fn the_voice_picker_stays_shut_until_the_strategy_has_loaded(cx: &mut TestAppContext) {
        let harness = StrategyHarness::new(AssignmentStrategy::Random, cx);
        harness.repo.lock().read = StrategyRead::Unreachable;
        let speak = harness.speak_listing(vec![listed_voice("first", "First")]);
        let (view, vcx) = harness.open_in_window(Some(speak), cx);

        pick_single_voice(&view, vcx);

        assert!(!picker_open(&view, vcx));
    }

    const FLAKY_VIEWER: &str = "twitch:alice";

    struct FlakyAliases {
        view: Entity<VoiceAliasesView>,
        lists_fail: Arc<std::sync::atomic::AtomicBool>,
        rt: tokio::runtime::Runtime,
    }

    fn aliases_listing(
        cx: &mut TestAppContext,
        lists_fail: bool,
        speak: Option<SpeakQueueHandle>,
        rt: tokio::runtime::Runtime,
    ) -> FlakyAliases {
        use std::sync::atomic::{AtomicBool, Ordering};

        install_language(Language::En);
        cx.update(|cx| {
            cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
            cx.set_global(Toasts::new());
        });
        let lists_fail = Arc::new(AtomicBool::new(lists_fail));
        let failing = Arc::clone(&lists_fail);
        let mut repo = forge_storage::MockVoiceAliasRepo::new();
        repo.expect_list().returning(move || {
            if failing.load(Ordering::SeqCst) {
                Err(StorageError::Connection {
                    reason: "database is locked".to_owned(),
                })
            } else {
                Ok(vec![keyed("a", FLAKY_VIEWER, "voice-a")])
            }
        });
        repo.expect_get_strategy()
            .returning(|| Ok(AssignmentStrategy::Random));
        let mut viewers = forge_storage::MockViewerRepo::new();
        viewers.expect_count().returning(|| Ok(0));
        let handle = rt.handle().clone();
        let view = cx.update(|cx| {
            cx.new(|cx| VoiceAliasesView::new(Arc::new(repo), Arc::new(viewers), speak, handle, cx))
        });
        FlakyAliases {
            view,
            lists_fail,
            rt,
        }
    }

    impl FlakyAliases {
        fn wait_for(
            &self,
            cx: &mut TestAppContext,
            mut reached: impl FnMut(&mut TestAppContext) -> bool,
        ) {
            crate::test_support::pump_until(&self.rt, || {
                cx.run_until_parked();
                reached(cx)
            });
        }

        fn load_failed(&self, cx: &mut TestAppContext) -> bool {
            self.view.read_with(cx, |view, _| view.load_failed)
        }

        fn rows(&self, cx: &mut TestAppContext) -> usize {
            self.view.read_with(cx, |view, _| view.aliases.len())
        }
    }

    #[gpui::test]
    fn a_failed_list_load_shows_the_retry_state_with_one_toast(cx: &mut TestAppContext) {
        let listing = aliases_listing(cx, true, None, runtime());

        listing.wait_for(cx, |cx| listing.load_failed(cx));

        assert_eq!(
            (
                cx.update(|cx| error_toasts(cx)),
                listing.view.read_with(cx, |view, _| view.loading),
            ),
            (1, false)
        );
    }

    #[gpui::test]
    fn a_retry_after_a_failed_list_load_lands_the_aliases(cx: &mut TestAppContext) {
        let listing = aliases_listing(cx, true, None, runtime());
        listing.wait_for(cx, |cx| listing.load_failed(cx));
        listing
            .lists_fail
            .store(false, std::sync::atomic::Ordering::SeqCst);

        listing.view.update(cx, |view, cx| view.retry_load(cx));
        let retrying = listing.load_failed(cx);
        listing.wait_for(cx, |cx| listing.rows(cx) == 1);

        assert_eq!((retrying, listing.load_failed(cx)), (false, false));
    }

    #[gpui::test]
    fn a_retry_that_fails_again_returns_to_the_retry_state(cx: &mut TestAppContext) {
        let listing = aliases_listing(cx, true, None, runtime());
        listing.wait_for(cx, |cx| listing.load_failed(cx));

        listing.view.update(cx, |view, cx| view.retry_load(cx));
        listing.wait_for(cx, |cx| listing.load_failed(cx));

        assert_eq!(cx.update(|cx| error_toasts(cx)), 2);
    }

    #[gpui::test]
    fn a_preview_the_speech_queue_refused_toasts_the_failure(cx: &mut TestAppContext) {
        let queue_runtime = runtime();
        let (speak, _events, _resolver) =
            queue_runtime.block_on(async { spawn_speak_queue(Vec::new()) });
        drop(queue_runtime);
        let listing = aliases_listing(cx, false, Some(speak), runtime());
        listing.wait_for(cx, |cx| listing.rows(cx) == 1);

        listing.view.update(cx, |view, cx| {
            view.preview(&AliasId("a".to_owned()), cx);
        });
        listing.wait_for(cx, |cx| cx.update(|cx| error_toasts(cx)) == 1);

        let message = cx.update(|cx| cx.global::<Toasts>().items()[0].message.to_string());
        assert_eq!(message, tr!("tts_aliases_preview_failed"));
    }
}
