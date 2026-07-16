use std::future::Future;
use std::sync::Arc;

use forge_components::{
    BORDER_THIN, ConfirmTone, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_SM, FONT_XS,
    ForgePalette, Icon, InputEvent, OverlayPosition, Radius, Spacing, TextInput, badge, card,
    confirm_modal, icon, modal, overlay, primary_button, primary_button_with_icon, radius,
    search_input, secondary_button, spacing, toggle, tr, with_alpha,
};
use forge_speak_queue::{Priority, RequestId, SpeakCommand, SpeakQueueHandle, SpeakRequest};
use forge_storage::{AliasId, AssignmentStrategy, VoiceAlias, VoiceAliasRepo};
use forge_voice::{AliasState, EngineId, VoiceId};
use gpui::{
    AnyElement, App, ClickEvent, Context, Div, Entity, FontWeight, Pixels, Rgba, SharedString,
    Subscription, Window, div, prelude::*, px, relative,
};

use crate::presentation::ActivePresentation;

const SEARCH_W: Pixels = px(240.0);
const MODAL_W: Pixels = px(440.0);
const ACTIONS_W: Pixels = px(90.0);
const AVATAR: Pixels = px(22.0);
const TABLE_RADIUS: Pixels = px(8.0);
const ROLE_BADGE_FS: Pixels = px(8.5);
const ENGINE_GLYPH: Pixels = px(12.0);
const ACTION_GLYPH: Pixels = px(14.0);

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EngineKind {
    Local,
    Cloud,
}

struct AliasRow {
    id: AliasId,
    viewer_id: String,
    viewer_name: String,
    kind: EngineKind,
    engine_id: String,
    engine_label: String,
    voice_id: String,
    voice_label: String,
    pitch_semitones: Option<f32>,
    rate_multiplier: Option<f32>,
    blocked: bool,
}

struct EngineOption {
    id: &'static str,
    label: &'static str,
}

const ENGINE_OPTIONS: [EngineOption; 4] = [
    EngineOption {
        id: "piper",
        label: "Piper",
    },
    EngineOption {
        id: "espeak-ng",
        label: "eSpeak-NG",
    },
    EngineOption {
        id: "polly",
        label: "Amazon Polly",
    },
    EngineOption {
        id: "elevenlabs",
        label: "ElevenLabs",
    },
];

struct AliasForm {
    editing: Option<AliasId>,
    viewer: Entity<TextInput>,
    voice: Entity<TextInput>,
    pitch: Entity<TextInput>,
    rate: Entity<TextInput>,
    engine: Option<String>,
    blocked: bool,
    saving: bool,
    _subs: Vec<Subscription>,
}

pub struct VoiceAliasesView {
    repo: Arc<dyn VoiceAliasRepo>,
    speak: Option<SpeakQueueHandle>,
    rt_handle: tokio::runtime::Handle,
    loading: bool,
    strategy: StrategyChoice,
    aliases: Vec<AliasRow>,
    total_count: usize,
    search: Entity<TextInput>,
    form: Option<AliasForm>,
    pending_delete: Option<usize>,
    _search_sub: Subscription,
}

impl VoiceAliasesView {
    pub fn new(
        repo: Arc<dyn VoiceAliasRepo>,
        speak: Option<SpeakQueueHandle>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let search = cx.new(|cx| search_input(tr!("tts_aliases_search_placeholder"), palette, cx));
        let search_sub = cx.subscribe(&search, |_this, _input, event: &InputEvent, cx| {
            if let InputEvent::Changed(_) = event {
                cx.notify();
            }
        });

        let view = Self {
            repo,
            speak,
            rt_handle,
            loading: true,
            strategy: StrategyChoice::DeterministicByName,
            aliases: Vec::new(),
            total_count: 0,
            search,
            form: None,
            pending_delete: None,
            _search_sub: search_sub,
        };
        view.reload(cx);
        view
    }

    fn reload(&self, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.repo);
        let (tx, rx) = tokio::sync::oneshot::channel::<
            Result<(Vec<VoiceAlias>, AssignmentStrategy), String>,
        >();
        self.rt_handle.spawn(async move {
            let outcome = async {
                let aliases = repo.list().await.map_err(|e| e.to_string())?;
                let strategy = repo.get_strategy().await.map_err(|e| e.to_string())?;
                Ok((aliases, strategy))
            }
            .await;
            let _ = tx.send(outcome);
        });
        cx.spawn(async move |this, cx| match rx.await {
            Ok(Ok((aliases, strategy))) => {
                let _ = this.update(cx, |this, cx| this.apply_loaded(aliases, strategy, cx));
            }
            Ok(Err(message)) => {
                let _ = this.update(cx, |this, cx| this.on_repo_error(&message, cx));
            }
            Err(_) => {}
        })
        .detach();
    }

    fn spawn_write(
        &self,
        work: impl Future<Output = Result<Vec<VoiceAlias>, String>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let _ = tx.send(work.await);
        });
        cx.spawn(async move |this, cx| match rx.await {
            Ok(Ok(aliases)) => {
                let _ = this.update(cx, |this, cx| this.apply_aliases(aliases, cx));
            }
            Ok(Err(message)) => {
                let _ = this.update(cx, |this, cx| this.on_repo_error(&message, cx));
            }
            Err(_) => {}
        })
        .detach();
    }

    fn apply_loaded(
        &mut self,
        aliases: Vec<VoiceAlias>,
        strategy: AssignmentStrategy,
        cx: &mut Context<Self>,
    ) {
        self.strategy = choice_from_strategy(&strategy);
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
        self.loading = false;
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
        let form = self.build_form(None, "", None, "", "", "", false, cx);
        form.viewer.read(cx).focus(window);
        self.form = Some(form);
        cx.notify();
    }

    fn open_edit(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.aliases.get(index) else {
            return;
        };
        let id = row.id.clone();
        let viewer = row.viewer_name.clone();
        let engine = (!row.blocked).then(|| row.engine_id.clone());
        let voice = row.voice_id.clone();
        let pitch = fmt_field(row.pitch_semitones);
        let rate = fmt_field(row.rate_multiplier);
        let blocked = row.blocked;
        let form = self.build_form(
            Some(id),
            &viewer,
            engine,
            &voice,
            &pitch,
            &rate,
            blocked,
            cx,
        );
        form.viewer.read(cx).focus(window);
        self.form = Some(form);
        cx.notify();
    }

    fn close_form(&mut self, cx: &mut Context<Self>) {
        self.form = None;
        cx.notify();
    }

    fn set_form_engine(&mut self, id: &'static str, cx: &mut Context<Self>) {
        if let Some(form) = self.form.as_mut() {
            form.engine = Some(id.to_owned());
        }
        cx.notify();
    }

    fn toggle_form_blocked(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.form.as_mut() {
            form.blocked = !form.blocked;
        }
        cx.notify();
    }

    fn save_form(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.form.as_ref() else {
            return;
        };
        if form.saving {
            return;
        }
        if form.viewer.read(cx).content().trim().is_empty() {
            return;
        }
        let alias = form_to_alias(form, cx);
        if let Some(form) = self.form.as_mut() {
            form.saving = true;
        }
        cx.notify();

        let repo = Arc::clone(&self.repo);
        let speak = self.speak.clone();
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<Vec<VoiceAlias>, String>>();
        self.rt_handle.spawn(async move {
            let outcome = async {
                repo.upsert(&alias).await.map_err(|e| e.to_string())?;
                if let Some(handle) = speak
                    && let Err(e) = handle.send(SpeakCommand::SetAlias(alias)).await
                {
                    eprintln!("forge-desktop: voice alias hot-reload failed: {e}");
                }
                repo.list().await.map_err(|e| e.to_string())
            }
            .await;
            let _ = tx.send(outcome);
        });
        cx.spawn(async move |this, cx| match rx.await {
            Ok(Ok(aliases)) => {
                let _ = this.update(cx, |this, cx| {
                    this.apply_aliases(aliases, cx);
                    this.form = None;
                    cx.notify();
                });
            }
            Ok(Err(message)) => {
                let _ = this.update(cx, |this, cx| {
                    if let Some(form) = this.form.as_mut() {
                        form.saving = false;
                    }
                    this.on_repo_error(&message, cx);
                });
            }
            Err(_) => {}
        })
        .detach();
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
                alias_override: None,
                engine_override: None,
                voice_override: None,
                source_event_id: forge_types::EventId::new(),
                is_reward: false,
            };
            if let Err(e) = handle.send(SpeakCommand::Enqueue(request)).await {
                eprintln!("forge-desktop: voice alias preview failed: {e}");
            }
        });
    }

    fn request_delete(&mut self, index: usize, cx: &mut Context<Self>) {
        self.pending_delete = Some(index);
        cx.notify();
    }

    fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.pending_delete = None;
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

    fn saveable(&self, cx: &Context<Self>) -> bool {
        self.form
            .as_ref()
            .is_some_and(|f| !f.saving && !f.viewer.read(cx).content().trim().is_empty())
    }

    #[allow(clippy::too_many_arguments)]
    fn build_form(
        &self,
        editing: Option<AliasId>,
        viewer: &str,
        engine: Option<String>,
        voice: &str,
        pitch: &str,
        rate: &str,
        blocked: bool,
        cx: &mut Context<Self>,
    ) -> AliasForm {
        let palette = cx.palette();
        let viewer = text_field(
            tr!("tts_aliases_form_viewer_placeholder"),
            viewer,
            palette,
            cx,
        );
        let voice = text_field(
            tr!("tts_aliases_form_voice_placeholder"),
            voice,
            palette,
            cx,
        );
        let pitch = text_field(
            tr!("tts_aliases_form_pitch_placeholder"),
            pitch,
            palette,
            cx,
        );
        let rate = text_field(tr!("tts_aliases_form_rate_placeholder"), rate, palette, cx);

        let mut subs = Vec::new();
        subs.push(cx.subscribe(
            &viewer,
            |this, _input, event: &InputEvent, cx| match event {
                InputEvent::Submitted(_) => this.save_form(cx),
                InputEvent::Changed(_) => cx.notify(),
                InputEvent::Cancelled => this.close_form(cx),
            },
        ));
        for field in [&voice, &pitch, &rate] {
            subs.push(
                cx.subscribe(field, |this, _input, event: &InputEvent, cx| match event {
                    InputEvent::Changed(_) => cx.notify(),
                    InputEvent::Cancelled => this.close_form(cx),
                    InputEvent::Submitted(_) => this.save_form(cx),
                }),
            );
        }

        AliasForm {
            editing,
            viewer,
            voice,
            pitch,
            rate,
            engine,
            blocked,
            saving: false,
            _subs: subs,
        }
    }

    fn strategy_banner(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut segmented = div()
            .flex()
            .flex_row()
            .gap(spacing(Spacing::Xxs, density))
            .p(spacing(Spacing::Xxs, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.shell);
        for choice in StrategyChoice::ALL {
            let active = self.strategy == choice;
            segmented = segmented.child(seg_button(
                SharedString::from(format!("va-strat-{}", choice.key())),
                choice.label(),
                active,
                palette,
                cx.listener(move |this, _: &ClickEvent, _, cx| this.set_strategy(choice, cx)),
            ));
        }

        let row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(
                div()
                    .flex_1()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(tr!("tts_aliases_strategy_label")),
            )
            .child(segmented);

        div()
            .w_full()
            .px(spacing(Spacing::Md, density))
            .pt(spacing(Spacing::Sm, density))
            .pb(spacing(Spacing::Sm, density))
            .child(
                card(row, palette)
                    .padding_xy(spacing(Spacing::Sm, density), spacing(Spacing::Sm, density))
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
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FONT_SM)
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

        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .px(spacing(Spacing::Md, density))
            .pb(spacing(Spacing::Sm, density))
            .child(div().w(SEARCH_W).child(self.search.clone()))
            .child(right)
            .into_any_element()
    }

    fn table(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let header = card(header_row(palette), palette)
            .background(palette.shell)
            .split_radius(TABLE_RADIUS, px(0.0))
            .padding_xy(spacing(Spacing::Xs, density), spacing(Spacing::Sm, density))
            .full_width();

        let needle = self.search.read(cx).content().to_ascii_lowercase();
        let visible: Vec<(usize, &AliasRow)> = self
            .aliases
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                needle.is_empty() || a.viewer_name.to_ascii_lowercase().contains(&needle)
            })
            .collect();

        let body: AnyElement = if visible.is_empty() {
            let caption = if self.loading {
                tr!("tts_aliases_loading")
            } else {
                tr!("tts_aliases_empty")
            };
            div()
                .w_full()
                .py(spacing(Spacing::Lg, density))
                .px(spacing(Spacing::Sm, density))
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child(caption)
                .into_any_element()
        } else {
            let total = visible.len();
            let mut col = div().w_full().flex().flex_col();
            for (pos, (index, row)) in visible.iter().enumerate() {
                let last = pos + 1 == total;
                col = col.child(self.alias_row(pos, *index, row, last, palette, density, cx));
            }
            col.into_any_element()
        };

        let body_frame = div()
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .rounded_b(TABLE_RADIUS)
            .overflow_hidden()
            .child(
                div()
                    .id("va-table-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .child(body),
            );

        let footer = div()
            .w_full()
            .py(spacing(Spacing::Xs, density))
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XS)
            .text_color(palette.text_faint)
            .child(tr!(
                "tts_aliases_footer_caption",
                shown = visible.len() as i64,
                total = self.total_count as i64
            ));

        div()
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .px(spacing(Spacing::Md, density))
            .pb(spacing(Spacing::Md, density))
            .child(header)
            .child(body_frame)
            .child(footer)
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn alias_row(
        &self,
        pos: usize,
        index: usize,
        row: &AliasRow,
        last: bool,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = row.blocked;
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
        let (avatar_bg, avatar_fg) = if muted {
            (palette.surface_overlay, palette.text_muted)
        } else {
            (avatar_color_for(&row.viewer_name, palette), palette.shell)
        };
        let avatar = div()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(AVATAR)
            .rounded(radius(Radius::Sm))
            .bg(avatar_bg)
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(FONT_XS)
                    .text_color(avatar_fg)
                    .child(initial.to_string()),
            );
        let mut viewer_inner = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(avatar)
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_SM)
                    .text_color(name_color)
                    .child(row.viewer_name.clone()),
            );
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
                .child(icon(Icon::Volume, ENGINE_GLYPH, palette.random))
                .child(
                    div()
                        .font_family(DEFAULT_MONO_FAMILY)
                        .text_size(FONT_SM)
                        .text_color(palette.random)
                        .child(tr!("tts_aliases_never_speak")),
                )
                .into_any_element()
        } else {
            div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xxs, density))
                .child(icon(
                    engine_glyph(row.kind),
                    ENGINE_GLYPH,
                    engine_color(row.kind, palette),
                ))
                .child(
                    div()
                        .font_family(DEFAULT_MONO_FAMILY)
                        .text_size(FONT_SM)
                        .text_color(palette.text_primary)
                        .child(format!("{} · {}", row.engine_label, row.voice_label)),
                )
                .into_any_element()
        };

        let (pitch_color, speed_color) = if muted {
            (palette.surface_overlay, palette.surface_overlay)
        } else {
            (palette.text_muted, palette.text_muted)
        };
        let pitch_cell = mono_cell(fmt_pitch(row.pitch_semitones, muted), pitch_color);
        let speed_cell = mono_cell(fmt_rate(row.rate_multiplier, muted), speed_color);

        let preview_color = if muted {
            palette.surface_overlay
        } else {
            palette.success
        };
        let mut preview = div().id(("va-preview", index)).flex().child(icon(
            Icon::PlayerPlay,
            ACTION_GLYPH,
            preview_color,
        ));
        if !muted {
            preview = preview
                .cursor_pointer()
                .on_click(cx.listener(move |this, _: &ClickEvent, _, _| this.preview(index)));
        }
        let edit = div()
            .id(("va-edit", index))
            .flex()
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.open_edit(index, window, cx)
            }))
            .child(icon(Icon::Pencil, ACTION_GLYPH, palette.text_muted));
        let delete = div()
            .id(("va-delete", index))
            .flex()
            .cursor_pointer()
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.request_delete(index, cx)),
            )
            .child(icon(Icon::X, ACTION_GLYPH, palette.text_muted));
        let actions = div()
            .w(ACTIONS_W)
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .gap(spacing(Spacing::Sm, density))
            .child(preview)
            .child(edit)
            .child(delete);

        let bg = if pos.is_multiple_of(2) {
            palette.elevated
        } else {
            palette.shell
        };
        let mut root = div()
            .w_full()
            .flex()
            .items_center()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .bg(bg)
            .child(weighted(VIEWER_GROW, viewer_inner))
            .child(weighted(VOICE_GROW, voice_inner))
            .child(weighted(PITCH_GROW, pitch_cell))
            .child(weighted(SPEED_GROW, speed_cell))
            .child(actions);
        if !last {
            root = root
                .border_b(BORDER_THIN)
                .border_color(palette.border_regular);
        }
        root.into_any_element()
    }

    fn active_overlay(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if let Some(form) = self.form.as_ref() {
            Some(self.form_modal(form, palette, density, cx))
        } else {
            self.pending_delete
                .map(|index| self.delete_confirm(index, palette, cx))
        }
    }

    fn form_modal(
        &self,
        form: &AliasForm,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = if form.editing.is_some() {
            tr!("tts_aliases_form_title_edit")
        } else {
            tr!("tts_aliases_form_title_assign")
        };

        let viewer_field = form_field(
            tr!("tts_aliases_form_viewer_label"),
            form.viewer.clone(),
            palette,
            density,
        );

        let block_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(spacing(Spacing::Xxs, density))
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_SM)
                            .text_color(palette.text_primary)
                            .child(tr!("tts_aliases_form_block_label")),
                    )
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_XS)
                            .text_color(palette.text_muted)
                            .child(tr!("tts_aliases_form_block_desc")),
                    ),
            )
            .child(toggle(form.blocked, palette).on_click(
                "va-form-block",
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_form_blocked(cx)),
            ));

        let config: AnyElement = if form.blocked {
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_faint)
                .child(tr!("tts_aliases_form_blocked_note"))
                .into_any_element()
        } else {
            let mut chips = div().flex().flex_wrap().gap(spacing(Spacing::Xxs, density));
            for opt in &ENGINE_OPTIONS {
                let active = form.engine.as_deref() == Some(opt.id);
                let id = opt.id;
                chips = chips.child(seg_button(
                    SharedString::from(format!("va-form-eng-{id}")),
                    opt.label,
                    active,
                    palette,
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.set_form_engine(id, cx)),
                ));
            }
            let engine_block = labelled(
                tr!("tts_aliases_form_engine_label"),
                chips,
                palette,
                density,
            );
            let voice_block = form_field(
                tr!("tts_aliases_form_voice_label"),
                form.voice.clone(),
                palette,
                density,
            );
            let pitch_block = form_field(
                tr!("tts_aliases_form_pitch_label"),
                form.pitch.clone(),
                palette,
                density,
            );
            let rate_block = form_field(
                tr!("tts_aliases_form_rate_label"),
                form.rate.clone(),
                palette,
                density,
            );
            div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Sm, density))
                .child(engine_block)
                .child(voice_block)
                .child(
                    div()
                        .flex()
                        .gap(spacing(Spacing::Sm, density))
                        .child(div().flex_1().child(pitch_block))
                        .child(div().flex_1().child(rate_block)),
                )
                .into_any_element()
        };

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(viewer_field)
            .child(block_row)
            .child(config);

        let save_label = if form.editing.is_some() {
            tr!("common_save")
        } else {
            tr!("tts_aliases_form_create")
        };
        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(secondary_button(tr!("common_cancel"), palette).on_click(
                "va-form-cancel",
                cx.listener(|this, _: &ClickEvent, _, cx| this.close_form(cx)),
            ))
            .child(
                primary_button(save_label, palette)
                    .disabled(!self.saveable(cx))
                    .on_click(
                        "va-form-save",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.save_form(cx)),
                    ),
            );

        let card = modal(title, body, palette)
            .width(MODAL_W)
            .footer(footer)
            .on_close(
                "va-form-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.close_form(cx)),
            );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("va-form-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.close_form(cx));
            })
            .into_any_element()
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

        let banner = self.strategy_banner(&palette, density, cx);
        let toolbar = self.toolbar(&palette, density, cx);
        let table = self.table(&palette, density, cx);
        let overlay = self.active_overlay(&palette, density, cx);

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

fn weighted(grow: f32, child: impl IntoElement) -> Div {
    let mut cell = div().min_w(px(0.0)).child(child);
    let style = cell.style();
    style.flex_grow = Some(grow);
    style.flex_basis = Some(relative(0.0).into());
    cell
}

fn header_row(palette: &ForgePalette) -> impl IntoElement {
    let caption = |text: SharedString| {
        div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XS)
            .text_color(palette.text_muted)
            .child(text)
    };
    div()
        .w_full()
        .flex()
        .items_center()
        .child(weighted(
            VIEWER_GROW,
            caption(tr!("tts_aliases_col_viewer").into()),
        ))
        .child(weighted(
            VOICE_GROW,
            caption(tr!("tts_aliases_col_voice").into()),
        ))
        .child(weighted(
            PITCH_GROW,
            caption(tr!("tts_aliases_col_pitch").into()),
        ))
        .child(weighted(
            SPEED_GROW,
            caption(tr!("tts_aliases_col_speed").into()),
        ))
        .child(
            div()
                .w(ACTIONS_W)
                .flex_none()
                .flex()
                .justify_end()
                .child(caption(tr!("tts_aliases_col_actions").into())),
        )
}

fn mono_cell(value: String, color: Rgba) -> impl IntoElement {
    div()
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_SM)
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

fn seg_button(
    id: SharedString,
    label: impl Into<SharedString>,
    active: bool,
    palette: &ForgePalette,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let fg = if active {
        palette.shell
    } else {
        palette.text_secondary
    };
    let weight = if active {
        FontWeight::MEDIUM
    } else {
        FontWeight::NORMAL
    };
    let mut chip = div()
        .id(id)
        .py(spacing(Spacing::Xxs, Density::Cozy))
        .px(spacing(Spacing::Sm, Density::Cozy))
        .rounded(radius(Radius::Sm))
        .cursor_pointer()
        .font_family(DEFAULT_BODY_FAMILY)
        .font_weight(weight)
        .text_size(FONT_XS)
        .text_color(fg)
        .on_click(handler)
        .child(label.into());
    if active {
        chip = chip.bg(palette.brand);
    } else {
        let hover = with_alpha(palette.border_regular, 0.06);
        chip = chip.hover(move |s| s.bg(hover));
    }
    chip
}

fn labelled(
    label: impl Into<SharedString>,
    control: impl IntoElement,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, density))
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child(label.into()),
        )
        .child(control)
}

fn form_field(
    label: impl Into<SharedString>,
    input: Entity<TextInput>,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    labelled(label, input, palette, density)
}

fn text_field(
    placeholder: impl Into<SharedString>,
    initial: &str,
    palette: ForgePalette,
    cx: &mut Context<VoiceAliasesView>,
) -> Entity<TextInput> {
    let initial = initial.to_owned();
    cx.new(|cx| {
        let mut input = TextInput::new(placeholder, cx).with_palette(palette);
        if !initial.is_empty() {
            input.set_content(initial, cx);
        }
        input
    })
}

fn row_from_alias(a: VoiceAlias) -> AliasRow {
    let engine = a.engine_id.0;
    let kind = if is_local_engine(&engine) {
        EngineKind::Local
    } else {
        EngineKind::Cloud
    };
    let engine_label = engine_display_label(&engine);
    let voice = a.voice_id.0;
    AliasRow {
        id: a.id,
        viewer_id: a.viewer_id,
        viewer_name: a.viewer_name,
        kind,
        engine_id: engine,
        engine_label,
        voice_id: voice.clone(),
        voice_label: voice,
        pitch_semitones: a.pitch_semitones,
        rate_multiplier: a.rate_multiplier,
        blocked: matches!(a.state, AliasState::Blocked),
    }
}

fn form_to_alias(form: &AliasForm, cx: &App) -> VoiceAlias {
    let viewer = form.viewer.read(cx).content().trim().to_owned();
    let engine = form.engine.clone().unwrap_or_default();
    let voice = form.voice.read(cx).content().trim().to_owned();
    let pitch = form.pitch.read(cx).content().trim().parse::<f32>().ok();
    let rate = form.rate.read(cx).content().trim().parse::<f32>().ok();
    VoiceAlias {
        id: form.editing.clone().unwrap_or_default(),
        viewer_id: viewer.clone(),
        viewer_name: viewer,
        engine_id: EngineId(engine.trim().to_owned()),
        voice_id: VoiceId(voice),
        pitch_semitones: pitch,
        rate_multiplier: rate,
        state: if form.blocked {
            AliasState::Blocked
        } else {
            AliasState::Active
        },
    }
}

fn choice_from_strategy(strategy: &AssignmentStrategy) -> StrategyChoice {
    match strategy {
        AssignmentStrategy::DeterministicByName => StrategyChoice::DeterministicByName,
        AssignmentStrategy::Random => StrategyChoice::Random,
        AssignmentStrategy::Single { .. } => StrategyChoice::SingleVoice,
    }
}

fn is_local_engine(engine_id: &str) -> bool {
    matches!(
        engine_id,
        "piper" | "espeak" | "espeak-ng" | "sapi" | "avfoundation"
    )
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

fn fmt_field(value: Option<f32>) -> String {
    value.map(|v| format!("{v}")).unwrap_or_default()
}

fn engine_glyph(kind: EngineKind) -> Icon {
    match kind {
        EngineKind::Local => Icon::Terminal,
        EngineKind::Cloud => Icon::Globe,
    }
}

fn engine_color(kind: EngineKind, palette: &ForgePalette) -> Rgba {
    match kind {
        EngineKind::Local => palette.success,
        EngineKind::Cloud => palette.info,
    }
}

fn avatar_color_for(name: &str, palette: &ForgePalette) -> Rgba {
    let hash = name.bytes().fold(0u32, |acc, b| {
        acc.wrapping_mul(31).wrapping_add(u32::from(b))
    });
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
