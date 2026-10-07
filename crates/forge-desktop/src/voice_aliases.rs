use std::future::Future;
use std::sync::Arc;

use forge_components::{
    BORDER_THIN, ColumnWidth, Confirm, ConfirmTone, DataRow, Density, FONT_XS, FONT_XXS,
    ForgePalette, Icon, OverlayPosition, Radius, SearchState, Spacing, ToastKind, avatar_tile,
    badge, body_family, card, column, confirm_modal, data_table, empty_state, hash_accent, icon,
    mono_family, overlay, platform_color, primary_button_with_icon, radius, segment, segmented,
    spacing, status_dot, toolbar_row, tr, virtual_table, with_alpha,
};
use forge_speak_queue::{
    EmoteTokenSet, Priority, RequestId, SpeakCommand, SpeakQueueHandle, SpeakRequest,
};
use forge_storage::{
    AliasId, AssignmentStrategy, StorageError, ViewerRepo, VoiceAlias, VoiceAliasRepo,
};
use forge_tts_core::engine_display_name;
use forge_voice::AliasState;
use gpui::{
    AnyElement, ClickEvent, Context, Entity, FontWeight, Pixels, Rgba, SharedString, Subscription,
    UniformListScrollHandle, Window, div, prelude::*, px,
};

use crate::async_bridge;
use crate::presentation::ActivePresentation;
use crate::toasts::PushToast;
use crate::voice_alias_form::{
    AliasForm, AliasFormEvent, AliasIdentity, AliasValues, PlatformScope, alias_key, fmt_field,
    split_alias_key,
};
use crate::voice_alias_store::save_alias;

const SEARCH_W: Pixels = px(240.0);
const ACTIONS_W: Pixels = px(90.0);
const AVATAR: Pixels = px(22.0);
const TABLE_RADIUS: Pixels = px(8.0);
const ROLE_BADGE_FS: Pixels = px(8.5);
const ENGINE_GLYPH: Pixels = px(12.0);
const ACTION_GLYPH: Pixels = px(13.0);
const BANNER_ICON: Pixels = px(18.0);
const ROW_PAD_V: Pixels = px(9.0);
const ROW_PAD_H: Pixels = px(12.0);
const VOICE_FS: Pixels = px(11.5);
const META_FS: Pixels = px(11.0);
const PAGE_PAD_H: Pixels = px(18.0);
const PAGE_GAP_V: Pixels = px(12.0);
const PAGE_PAD_BOTTOM: Pixels = px(16.0);
const BANNER_PAD_V: Pixels = px(12.0);
const BANNER_PAD_H: Pixels = px(14.0);
const BANNER_GAP: Pixels = px(14.0);
const BANNER_TITLE_FS: Pixels = px(12.5);
const BANNER_TITLE_MB: Pixels = px(1.0);
const PLATFORM_DOT: Pixels = px(6.0);
const PLATFORM_BADGE_PAD_V: Pixels = px(1.0);
const PLATFORM_BADGE_PAD_H: Pixels = px(6.0);

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
    platform: PlatformScope,
    engine_id: String,
    engine_label: String,
    voice_id: String,
    voice_label: String,
    pitch_semitones: Option<f32>,
    rate_multiplier: Option<f32>,
    blocked: bool,
}

pub struct VoiceAliasesView {
    repo: Arc<dyn VoiceAliasRepo>,
    viewer_repo: Arc<dyn ViewerRepo>,
    speak: Option<SpeakQueueHandle>,
    rt_handle: tokio::runtime::Handle,
    loading: bool,
    strategy: StrategyChoice,
    aliases: Vec<AliasRow>,
    visible: Vec<usize>,
    total_count: usize,
    viewer_count: usize,
    search: SearchState,
    table_scroll: UniformListScrollHandle,
    form: Option<Entity<AliasForm>>,
    _form_sub: Option<Subscription>,
    pending_delete: Confirm<usize>,
    _search_sub: Subscription,
}

impl VoiceAliasesView {
    pub fn new(
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

        let view = Self {
            repo,
            viewer_repo,
            speak,
            rt_handle,
            loading: true,
            strategy: StrategyChoice::DeterministicByName,
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
        view
    }

    fn reload(&self, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.repo);
        let viewer_repo = Arc::clone(&self.viewer_repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let aliases = repo.list().await.map_err(|e| e.to_string())?;
                let strategy = repo.get_strategy().await.map_err(|e| e.to_string())?;
                let viewers = viewer_repo.count().await.map_err(|e| e.to_string())?;
                Ok::<_, String>((aliases, strategy, viewers))
            },
            |this, result, cx| match result {
                Ok((aliases, strategy, viewers)) => {
                    this.apply_loaded(aliases, strategy, viewers, cx)
                }
                Err(message) => this.on_repo_error(&message, cx),
            },
            cx,
        );
    }

    fn spawn_write(
        &self,
        work: impl Future<Output = Result<Vec<VoiceAlias>, String>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        async_bridge::run_async(
            &self.rt_handle,
            work,
            |this, result, cx| match result {
                Ok(aliases) => this.apply_aliases(aliases, cx),
                Err(message) => this.on_repo_error(&message, cx),
            },
            cx,
        );
    }

    fn apply_loaded(
        &mut self,
        aliases: Vec<VoiceAlias>,
        strategy: AssignmentStrategy,
        viewers: u64,
        cx: &mut Context<Self>,
    ) {
        self.strategy = choice_from_strategy(&strategy);
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

    fn rebuild_visible(&mut self) {
        self.visible = self
            .aliases
            .iter()
            .enumerate()
            .filter(|(_, a)| self.search.matches(&a.viewer_name))
            .map(|(index, _)| index)
            .collect();
    }

    fn on_repo_error(&mut self, message: &str, cx: &mut Context<Self>) {
        eprintln!("forge-desktop: voice aliases operation failed: {message}");
        self.loading = false;
        cx.notify();
    }

    fn set_strategy(&mut self, choice: StrategyChoice, cx: &mut Context<Self>) {
        self.strategy = choice;
        cx.notify();
        let Some(strategy) = self.strategy_to_assignment(choice) else {
            return;
        };
        let repo = Arc::clone(&self.repo);
        let speak = self.speak.clone();
        self.rt_handle.spawn(async move {
            if let Err(e) = repo.set_strategy(&strategy).await {
                eprintln!("forge-desktop: voice strategy persist failed: {e}");
            }
            if let Some(handle) = speak
                && let Err(e) = handle.send(SpeakCommand::SetStrategy(strategy)).await
            {
                eprintln!("forge-desktop: voice strategy hot-reload failed: {e}");
            }
        });
    }

    fn strategy_to_assignment(&self, choice: StrategyChoice) -> Option<AssignmentStrategy> {
        match choice {
            StrategyChoice::DeterministicByName => Some(AssignmentStrategy::DeterministicByName),
            StrategyChoice::Random => Some(AssignmentStrategy::Random),
            StrategyChoice::SingleVoice => {
                let voices = self.speak.as_ref()?.available_voices();
                let first = voices.first()?;
                Some(AssignmentStrategy::Single {
                    voice_id: first.id.clone(),
                    engine_id: first.engine_id.clone(),
                })
            }
        }
    }

    fn open_assign(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let form = cx.new(|cx| AliasForm::new(None, None, false, AliasValues::default(), cx));
        self.mount_form(form, window, cx);
    }

    fn open_edit(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.aliases.get(index) else {
            return;
        };
        let id_keyed = row.viewer_id != alias_key(row.platform, &row.viewer_name);
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
        form.update(cx, |f, cx| f.focus(window, cx));
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
                        cx.push_toast(ToastKind::Error, tr!("tts_aliases_viewer_taken"));
                    }
                    this.on_repo_error(&error.to_string(), cx);
                }
            },
            cx,
        );
    }

    fn preview(&self, index: usize) {
        let Some(row) = self.aliases.get(index) else {
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
        self.rt_handle.spawn(async move {
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
            if let Err(e) = handle.send(SpeakCommand::Enqueue(request)).await {
                eprintln!("forge-desktop: voice alias preview failed: {e}");
            }
        });
    }

    fn request_delete(&mut self, index: usize, cx: &mut Context<Self>) {
        self.pending_delete.request(index);
        cx.notify();
    }

    fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.pending_delete.cancel();
        cx.notify();
    }

    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.pending_delete.take() else {
            return;
        };
        let Some(row) = self.aliases.get(index) else {
            cx.notify();
            return;
        };
        let id = row.id.clone();
        cx.notify();

        let repo = Arc::clone(&self.repo);
        let speak = self.speak.clone();
        self.spawn_write(
            async move {
                repo.delete(&id).await.map_err(|e| e.to_string())?;
                if let Some(handle) = speak
                    && let Err(e) = handle.send(SpeakCommand::RemoveAlias(id)).await
                {
                    eprintln!("forge-desktop: voice alias hot-reload (remove) failed: {e}");
                }
                repo.list().await.map_err(|e| e.to_string())
            },
            cx,
        );
    }

    fn strategy_banner(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let segments = StrategyChoice::ALL
            .into_iter()
            .map(|choice| {
                let active = self.strategy == choice;
                segment(
                    SharedString::from(format!("va-strat-{}", choice.key())),
                    choice.label(),
                    active,
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.set_strategy(choice, cx)),
                )
            })
            .collect();
        let strategy_segments = segmented(segments, palette);

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
                    .child(tr!("tts_aliases_strategy_sublabel")),
            );

        let row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(BANNER_GAP)
            .child(icon(Icon::Wand, BANNER_ICON, palette.brand))
            .child(heading)
            .child(strategy_segments);

        div()
            .w_full()
            .px(PAGE_PAD_H)
            .py(PAGE_GAP_V)
            .child(
                card(row, palette)
                    .padding_xy(BANNER_PAD_V, BANNER_PAD_H)
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
            .child(tr!("tts_aliases_count", count = self.total_count as i64));

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

        toolbar_row(div().w(SEARCH_W).child(self.search.field().clone()), right)
            .density(density)
            .px(PAGE_PAD_H)
            .pb(PAGE_GAP_V)
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

        let hover_bg = with_alpha(palette.border_regular, 0.08);
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
            let mut state = empty_state(caption, palette).density(density);
            if self.loading {
                state = state.loading("voice-aliases-loading");
            }
            let header_only = data_table(palette, columns, Vec::new())
                .density(density)
                .header_padding(px(7.0), px(12.0))
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
            .header_padding(px(7.0), px(12.0))
            .row_padding(ROW_PAD_V, ROW_PAD_H)
            .row_hover(hover_bg)
            .trailing_rule(false)
            .build(
                move |this, ix, _window, cx| {
                    let index = this.visible[ix];
                    match this.aliases.get(index) {
                        Some(row) => this.alias_row(index, row, &pal, density, cx),
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
            .pt(px(8.0))
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(tr!(
                "tts_aliases_footer_caption",
                shown = shown as i64,
                total = self.total_count as i64,
                auto = auto as i64
            ));

        div()
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .px(PAGE_PAD_H)
            .pb(PAGE_PAD_BOTTOM)
            .child(frame)
            .child(footer)
            .into_any_element()
    }

    fn alias_row(
        &self,
        index: usize,
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
                palette.random,
                palette,
            ));
        }

        let voice_inner: AnyElement = if muted {
            div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xxs, density))
                .child(icon(Icon::VolumeOff, ENGINE_GLYPH, palette.random))
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
                        .child(format!("{} · {}", row.engine_label, row.voice_label)),
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
        let mut preview = div()
            .id((gpui::ElementId::from("va-preview"), row_key.clone()))
            .flex()
            .child(icon(Icon::PlayerPlay, ACTION_GLYPH, preview_color));
        if !muted {
            preview = preview
                .cursor_pointer()
                .on_click(cx.listener(move |this, _: &ClickEvent, _, _| this.preview(index)));
        }
        let edit = div()
            .id((gpui::ElementId::from("va-edit"), row_key.clone()))
            .flex()
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.open_edit(index, window, cx)
            }))
            .child(icon(Icon::Pencil, ACTION_GLYPH, palette.text_faint));
        let delete = div()
            .id((gpui::ElementId::from("va-delete"), row_key.clone()))
            .flex()
            .cursor_pointer()
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.request_delete(index, cx)),
            )
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
                .copied()
                .map(|index| self.delete_confirm(index, palette, cx))
        }
    }

    fn delete_confirm(
        &self,
        index: usize,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let viewer = self
            .aliases
            .get(index)
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
        .py(PLATFORM_BADGE_PAD_V)
        .px(PLATFORM_BADGE_PAD_H)
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
    let (platform, _) = split_alias_key(&a.viewer_id);
    let viewer_name = match a.viewer_name.split_once(':') {
        Some((token, name)) if PlatformScope::from_token(token) == Some(platform) => {
            name.to_owned()
        }
        _ => a.viewer_name,
    };
    AliasRow {
        id: a.id,
        viewer_id: a.viewer_id,
        viewer_name,
        platform,
        engine_id: engine,
        engine_label,
        voice_id: voice.clone(),
        voice_label: voice,
        pitch_semitones: a.pitch_semitones,
        rate_multiplier: a.rate_multiplier,
        blocked: matches!(a.state, AliasState::Blocked),
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
        forge_tts_core::ENGINE_ID_ELEVENLABS => (Icon::Microphone2, palette.brand),
        forge_tts_core::ENGINE_ID_POLLY => (Icon::BrandAws, palette.bits),
        forge_tts_core::ENGINE_ID_AZURE => (Icon::Cloud, palette.info),
        forge_tts_core::ENGINE_ID_OPENAI => (Icon::Bolt, palette.accent_teal),
        _ => (Icon::Cloud, palette.text_muted),
    }
}

fn fmt_pitch(value: Option<f32>, blocked: bool) -> String {
    if blocked {
        return "-".to_owned();
    }
    match value {
        Some(p) if p >= 0.0 => format!("+{p:.0} st"),
        Some(p) => format!("{p:.0} st"),
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
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use std::time::Duration;

    use forge_components::ThemeId;
    use forge_storage::{DataProvider, Language};
    use forge_voice::{EngineId, VoiceId};
    use gpui::{TestAppContext, VisualTestContext};

    use super::*;
    use crate::i18n::install_language;
    use crate::presentation::Presentation;
    use crate::test_support::{
        LiveAliases, Sandboxed, alias_shape, error_toasts, live_alias, runtime, sandboxed_backend,
        spawn_speak_queue,
    };
    use crate::toasts::Toasts;

    const TEST_KEY: [u8; 32] = [0x5a; 32];
    const SETTLE_ROUNDS: usize = 2000;

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

    fn settle(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        done: impl Fn(&mut TestAppContext) -> bool,
    ) {
        for _ in 0..SETTLE_ROUNDS {
            rt.block_on(async { tokio::time::sleep(Duration::from_millis(1)).await });
            cx.run_until_parked();
            if done(cx) {
                return;
            }
        }
        panic!("the save never settled");
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
        settle(cx, &rt, |_| {
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
        settle(cx, &rt, |cx| cx.update(|cx| error_toasts(cx)) > 0);

        assert_eq!(cx.update(|cx| error_toasts(cx)), 1);
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
            settle(vcx, &rt, |cx| {
                view.read_with(cx, |view, _| !view.aliases.is_empty())
            });

            vcx.update(|window, cx| view.update(cx, |view, cx| view.open_edit(0, window, cx)));
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
            settle(vcx, &rt, |_| {
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
        settle(vcx, &rt, |cx| {
            view.read_with(cx, |view, _| !view.aliases.is_empty())
        });

        vcx.update(|window, cx| view.update(cx, |view, cx| view.open_edit(0, window, cx)));
        vcx.run_until_parked();
        vcx.simulate_input("x");
        vcx.simulate_keystrokes("enter");
        let repo = fixture.backend.voice_alias_repo();
        let saved = || rt.block_on(repo.list()).expect("list");
        settle(vcx, &rt, |_| {
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
            ("twitch:Alice", "Alice", PlatformScope::Twitch, "Alice"),
            ("kick:Alice", "kick:Alice", PlatformScope::Kick, "Alice"),
            ("Alice", "Alice", PlatformScope::Any, "Alice"),
            (
                "youtube:Alice",
                "kick:Alice",
                PlatformScope::YouTube,
                "kick:Alice",
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
}
