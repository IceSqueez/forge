use std::collections::HashSet;
use std::sync::Arc;

use forge_components::confirm::ConfirmTone;
use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, ChipGlyph, Confirm, Density, FONT_SM, FONT_XS, FONT_XXS,
    ForgePalette, Icon, InputEvent, MenuItem, MenuPlacement, ModalSize, OverlayPosition, Radius,
    SearchState, Spacing, TextInput, ToastKind, badge, body_family, card, chip, confirm_modal,
    empty_state, ghost_button_with_icon, header_stat, header_stats, icon, menu_button,
    menu_divider, menu_item, modal, mono_family, overlay, page_frame, primary_button,
    primary_button_with_icon, radius, secondary_button, slider, spacing, spinner, tooltip_builder,
    tr, with_alpha,
};
use forge_runtime::{
    MAX_PENDING_PER_QUEUE, MembershipOutcome, QueueIntake, QueueMode, QueueProcessing,
    QueueSchedulerHandle,
};
use forge_storage::{ActionRepo, QueueRepo};
use forge_types::{Queue, QueueId};
use gpui::{
    AnyElement, App, ClickEvent, Context, Div, Entity, EventEmitter, FontWeight, Pixels, Point,
    Rgba, SharedString, Stateful, Subscription, Window, div, prelude::*, px,
};

use crate::async_bridge;
use crate::presentation::ActivePresentation;
use crate::queue_health::QueueHealth;
use crate::toasts::PushToast;

const BADGE_RADIUS: Pixels = px(8.0);
const PILL_RADIUS: Pixels = px(5.0);
const BADGE_GLYPH: Pixels = px(9.0);
const PANEL_GLYPH: Pixels = px(12.0);
const CARDS_PER_ROW: usize = 2;
const STATS_FS: Pixels = px(11.5);
const CARD_PAD: Pixels = px(14.0);
const SECTION_GAP: Pixels = px(10.0);
const HEADER_GAP: Pixels = px(12.0);
const BAR_GAP: Pixels = px(8.0);
const STAT_VALUE_FS: Pixels = px(13.0);
const DESC_FS: Pixels = px(11.0);
const MAX_PILLS: usize = 3;
const SERIAL_CONCURRENCY: u32 = 1;
const PARALLEL_CONCURRENCY: u32 = 8;
const MIN_CONCURRENCY: u32 = 1;
const MAX_CONCURRENCY: u32 = 16;
const SEARCH_W: Pixels = px(240.0);

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum QueueFilter {
    #[default]
    All,
    Running,
    Paused,
    Parallel,
    Sequential,
}

impl QueueFilter {
    const TABS: [(&'static str, QueueFilter); 5] = [
        ("queue-filter-all", QueueFilter::All),
        ("queue-filter-running", QueueFilter::Running),
        ("queue-filter-paused", QueueFilter::Paused),
        ("queue-filter-parallel", QueueFilter::Parallel),
        ("queue-filter-sequential", QueueFilter::Sequential),
    ];

    fn keeps(self, row: &QueueRow) -> bool {
        match self {
            QueueFilter::All => true,
            QueueFilter::Running => row.mode == QueueMode::RUNNING,
            QueueFilter::Paused => row.mode != QueueMode::RUNNING,
            QueueFilter::Parallel => !row.blocking,
            QueueFilter::Sequential => row.blocking,
        }
    }

    fn label_key(self) -> &'static str {
        match self {
            QueueFilter::All => "queues_filter_all",
            QueueFilter::Running => "queues_filter_running",
            QueueFilter::Paused => "queues_filter_paused",
            QueueFilter::Parallel => "queues_filter_parallel",
            QueueFilter::Sequential => "queues_filter_sequential",
        }
    }

    fn dot(self, palette: &ForgePalette) -> Rgba {
        match self {
            QueueFilter::All => palette.brand,
            QueueFilter::Running => palette.success,
            QueueFilter::Paused => palette.warning,
            QueueFilter::Parallel => palette.info,
            QueueFilter::Sequential => palette.bits,
        }
    }
}

fn queue_matches(row: &QueueRow, filter: QueueFilter, search: &SearchState) -> bool {
    filter.keeps(row) && search.matches(&row.name)
}

struct ModePreset {
    mode: QueueMode,
    label_key: &'static str,
    tooltip_key: &'static str,
    element_id: &'static str,
    glyph: Icon,
}

const MODE_PRESETS: [ModePreset; 3] = [
    ModePreset {
        mode: QueueMode::PAUSED,
        label_key: "queues_pause_btn",
        tooltip_key: "queues_mode_pause_tooltip",
        element_id: "q-mode-pause",
        glyph: Icon::PlayerPause,
    },
    ModePreset {
        mode: QueueMode::DRAINING,
        label_key: "queues_drain_btn",
        tooltip_key: "queues_mode_drain_tooltip",
        element_id: "q-mode-drain",
        glyph: Icon::FilterOff,
    },
    ModePreset {
        mode: QueueMode::HOLDING,
        label_key: "queues_hold_btn",
        tooltip_key: "queues_mode_hold_tooltip",
        element_id: "q-mode-hold",
        glyph: Icon::ClockPause,
    },
];

fn mode_ink(mode: QueueMode, palette: &ForgePalette) -> Rgba {
    match (mode.processing, mode.intake) {
        (QueueProcessing::Running, QueueIntake::Accept) => palette.success,
        (QueueProcessing::Running, QueueIntake::Skip) => palette.info,
        (QueueProcessing::Frozen, QueueIntake::Accept) => palette.bits,
        (QueueProcessing::Frozen, QueueIntake::Skip) => palette.warning,
    }
}

fn mode_glyph(mode: QueueMode) -> Icon {
    match (mode.processing, mode.intake) {
        (QueueProcessing::Running, QueueIntake::Accept) => Icon::PlayerPlay,
        (QueueProcessing::Running, QueueIntake::Skip) => Icon::FilterOff,
        (QueueProcessing::Frozen, QueueIntake::Accept) => Icon::ClockPause,
        (QueueProcessing::Frozen, QueueIntake::Skip) => Icon::PlayerPause,
    }
}

fn mode_badge_key(mode: QueueMode) -> &'static str {
    match (mode.processing, mode.intake) {
        (QueueProcessing::Running, QueueIntake::Accept) => "queues_status_running",
        (QueueProcessing::Running, QueueIntake::Skip) => "queues_status_draining",
        (QueueProcessing::Frozen, QueueIntake::Accept) => "queues_status_held",
        (QueueProcessing::Frozen, QueueIntake::Skip) => "queues_status_paused",
    }
}

fn mode_caption_key(mode: QueueMode) -> &'static str {
    match (mode.processing, mode.intake) {
        (QueueProcessing::Running, QueueIntake::Accept) => "queues_mode_running_caption",
        (QueueProcessing::Running, QueueIntake::Skip) => "queues_mode_drain_caption",
        (QueueProcessing::Frozen, QueueIntake::Accept) => "queues_mode_hold_caption",
        (QueueProcessing::Frozen, QueueIntake::Skip) => "queues_mode_pause_caption",
    }
}

struct QueueRow {
    id: QueueId,
    name: String,
    description: String,
    blocking: bool,
    concurrency: u32,
    mode: QueueMode,
    pending: u32,
    in_flight: u32,
    overflowed: u64,
    actions: u32,
    running: Vec<String>,
}

impl QueueRow {
    fn concurrency_label(&self) -> SharedString {
        if self.blocking {
            SharedString::from(tr!("queues_metric_serial"))
        } else {
            SharedString::from(tr!("queues_metric_parallel"))
        }
    }

    fn frozen(&self) -> bool {
        self.mode.processing == QueueProcessing::Frozen
    }
}

struct QueueDraft {
    editing: Option<QueueId>,
    name: String,
    description: String,
    concurrency: u32,
}

enum EditQueueEvent {
    Submit(QueueDraft),
    Cancel,
}

struct EditQueueModal {
    editing: Option<QueueId>,
    orig_name: String,
    name_input: Entity<TextInput>,
    desc_input: Entity<TextInput>,
    concurrency: u32,
    saving: bool,
    _name_sub: Subscription,
}

impl EventEmitter<EditQueueEvent> for EditQueueModal {}

impl EditQueueModal {
    fn new(
        editing: Option<QueueId>,
        name_seed: &str,
        desc_seed: &str,
        concurrency: u32,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let orig_name = name_seed.to_owned();
        let name_seed = name_seed.to_owned();
        let desc_seed = desc_seed.to_owned();
        let name_input = cx.new(|cx| {
            let mut ti =
                TextInput::new(tr!("queues_create_name_placeholder"), cx).with_palette(palette);
            ti.set_content(name_seed, cx);
            ti
        });
        let desc_input = cx.new(|cx| {
            let mut ti =
                TextInput::new(tr!("queues_create_desc_placeholder"), cx).with_palette(palette);
            ti.set_content(desc_seed, cx);
            ti
        });
        let name_sub = cx.subscribe(
            &name_input,
            |this, _f, event: &InputEvent, cx| match event {
                InputEvent::Submitted(_) => this.submit(cx),
                InputEvent::Cancelled => this.cancel(cx),
                InputEvent::Changed(_) => cx.notify(),
            },
        );
        EditQueueModal {
            editing,
            orig_name,
            name_input,
            desc_input,
            concurrency: concurrency.clamp(MIN_CONCURRENCY, MAX_CONCURRENCY),
            saving: false,
            _name_sub: name_sub,
        }
    }

    fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.name_input.update(cx, |f, cx| f.focus(window, cx));
    }

    fn set_concurrency(&mut self, value: u32, cx: &mut Context<Self>) {
        self.concurrency = value.clamp(MIN_CONCURRENCY, MAX_CONCURRENCY);
        cx.notify();
    }

    fn is_saveable(&self, cx: &App) -> bool {
        !self.saving && !self.name_input.read(cx).content().trim().is_empty()
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if !self.is_saveable(cx) {
            return;
        }
        let draft = QueueDraft {
            editing: self.editing,
            name: self.name_input.read(cx).content().trim().to_owned(),
            description: self.desc_input.read(cx).content().trim().to_owned(),
            concurrency: self.concurrency.clamp(MIN_CONCURRENCY, MAX_CONCURRENCY),
        };
        cx.emit(EditQueueEvent::Submit(draft));
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(EditQueueEvent::Cancel);
    }
}

impl Render for EditQueueModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        let title = if self.editing.is_some() {
            tr!("queues_edit_title", name = self.orig_name.clone())
        } else {
            tr!("queues_create_title")
        };

        let concurrency = self.concurrency;
        let concurrency_hint = if concurrency <= SERIAL_CONCURRENCY {
            SharedString::from(tr!("queues_concurrency_serial"))
        } else {
            SharedString::from(tr!("queues_concurrency_parallel", count = concurrency))
        };
        let name_field = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(SharedString::from(
                        tr!("queues_create_name_label").to_uppercase(),
                    )),
            )
            .child(div().child(self.name_input.clone()));

        let desc_field = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(
                        div()
                            .font_family(mono_family())
                            .text_size(FONT_XXS)
                            .text_color(palette.text_faint)
                            .child(SharedString::from(
                                tr!("queues_create_desc_label").to_uppercase(),
                            )),
                    )
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XXS)
                            .text_color(palette.text_muted)
                            .child(SharedString::from(tr!("queues_create_desc_optional"))),
                    ),
            )
            .child(div().child(self.desc_input.clone()));

        let concurrency_box = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .py(px(10.0))
            .px(px(12.0))
            .rounded(px(7.0))
            .border(BORDER_THIN)
            .border_color(palette.border_input)
            .bg(palette.shell)
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(14.0))
                    .child(
                        div().flex_1().child(
                            slider(
                                concurrency as f32,
                                MIN_CONCURRENCY as f32,
                                MAX_CONCURRENCY as f32,
                                &palette,
                            )
                            .on_change(
                                "q-modal-concurrency",
                                cx.listener(|this, value: &f32, _, cx| {
                                    this.set_concurrency(value.round() as u32, cx)
                                }),
                            ),
                        ),
                    )
                    .child(
                        div()
                            .w(px(28.0))
                            .flex()
                            .justify_end()
                            .font_family(mono_family())
                            .text_size(FONT_SM)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(palette.text_primary)
                            .child(concurrency.to_string()),
                    ),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(DESC_FS)
                    .text_color(palette.text_muted)
                    .child(concurrency_hint),
            );

        let concurrency_field = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(SharedString::from(
                        tr!("queues_concurrency_label").to_uppercase(),
                    )),
            )
            .child(concurrency_box);

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(name_field)
            .child(desc_field)
            .child(concurrency_field);

        let saveable = self.is_saveable(cx);
        let save_label = if self.editing.is_some() {
            tr!("queues_edit_btn")
        } else {
            tr!("queues_create_btn")
        };
        let cancel = secondary_button(tr!("queues_create_cancel"), &palette).on_click(
            "q-modal-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
        );
        let save = primary_button(save_label, &palette)
            .disabled(!saveable)
            .on_click(
                "q-modal-save",
                cx.listener(|this, _: &ClickEvent, _, cx| this.submit(cx)),
            );
        let hint = div()
            .font_family(body_family())
            .text_size(FONT_XS)
            .text_color(palette.text_faint)
            .child(SharedString::from(tr!("queues_create_kbd_hint")));
        let buttons = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(cancel)
            .child(save);
        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(hint)
            .child(buttons);

        let card = modal(title, body, &palette)
            .header_icon(Icon::Stack2, palette.bits)
            .subtitle(tr!("queues_create_subtitle"))
            .size(ModalSize::Md)
            .footer(footer)
            .on_close(
                "q-modal-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            );

        let view = cx.entity();
        overlay(card, &palette)
            .position(OverlayPosition::Center)
            .on_dismiss("q-modal-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel(cx));
            })
            .into_any_element()
    }
}

pub struct QueuesView {
    queues: Vec<QueueRow>,
    loading: bool,
    modal: Option<Entity<EditQueueModal>>,
    _modal_sub: Option<Subscription>,
    pending_delete: Confirm<QueueId>,
    menu_open: Option<QueueId>,
    menu_click_pos: Option<Point<Pixels>>,
    diverged: HashSet<QueueId>,
    scheduler: QueueSchedulerHandle,
    queue_repo: Arc<dyn QueueRepo>,
    action_repo: Arc<dyn ActionRepo>,
    rt_handle: tokio::runtime::Handle,
    status_filter: QueueFilter,
    search: SearchState,
    _health_obs: Subscription,
    _search_sub: Subscription,
}

impl QueuesView {
    pub fn new(
        queue_health: Entity<QueueHealth>,
        scheduler: QueueSchedulerHandle,
        queue_repo: Arc<dyn QueueRepo>,
        action_repo: Arc<dyn ActionRepo>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let health_obs = cx.observe(&queue_health, |this, health, cx| {
            this.sync_modes(&health, cx);
        });
        let search = SearchState::new(cx, palette, tr!("queues_search_placeholder"));
        let search_sub = cx.subscribe(search.field(), Self::on_search_event);
        let view = Self {
            queues: vec![],
            loading: true,
            modal: None,
            _modal_sub: None,
            pending_delete: Confirm::default(),
            menu_open: None,
            menu_click_pos: None,
            diverged: HashSet::new(),
            scheduler,
            queue_repo,
            action_repo,
            rt_handle,
            status_filter: QueueFilter::default(),
            search,
            _health_obs: health_obs,
            _search_sub: search_sub,
        };
        view.reload(cx);
        view
    }

    fn on_search_event(
        &mut self,
        _f: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if self.search.on_changed(event) {
            cx.notify();
        }
    }

    fn set_status_filter(&mut self, filter: QueueFilter, cx: &mut Context<Self>) {
        self.status_filter = filter;
        cx.notify();
    }

    fn sync_modes(&mut self, health: &Entity<QueueHealth>, cx: &mut Context<Self>) {
        let mut changed = false;
        {
            let health = health.read(cx);
            for row in &mut self.queues {
                if let Some(mode) = health.mode(row.id)
                    && row.mode != mode
                {
                    row.mode = mode;
                    changed = true;
                }
            }
        }
        if changed {
            self.reload(cx);
            cx.notify();
        }
    }

    fn visible_indices(&self) -> Vec<usize> {
        self.queues
            .iter()
            .enumerate()
            .filter(|(_, row)| queue_matches(row, self.status_filter, &self.search))
            .map(|(i, _)| i)
            .collect()
    }

    fn reload(&self, cx: &mut Context<Self>) {
        let queue_repo = Arc::clone(&self.queue_repo);
        let action_repo = Arc::clone(&self.action_repo);
        let scheduler = self.scheduler.clone();
        async_bridge::run_async(
            &self.rt_handle,
            load_queues(queue_repo, action_repo, scheduler),
            |this, result, cx| match result {
                Ok(rows) => this.apply_rows(rows, cx),
                Err(message) => this.on_repo_error(&message, cx),
            },
            cx,
        );
    }

    fn apply_rows(&mut self, rows: Vec<QueueRow>, cx: &mut Context<Self>) {
        self.diverged.retain(|id| rows.iter().any(|r| r.id == *id));
        self.queues = rows;
        self.loading = false;
        cx.notify();
    }

    fn on_repo_error(&mut self, message: &str, cx: &mut Context<Self>) {
        eprintln!("forge-desktop: queues operation failed: {message}");
        self.loading = false;
        cx.notify();
    }

    fn apply_membership_outcome(
        &mut self,
        id: QueueId,
        outcome: Result<MembershipOutcome, String>,
    ) {
        match outcome {
            Ok(MembershipOutcome::Applied) | Ok(MembershipOutcome::AlreadyRegistered) => {
                self.diverged.remove(&id);
            }
            Ok(MembershipOutcome::NotFound) => {
                eprintln!("forge-desktop: scheduler reported queue not found: {id}");
                self.diverged.insert(id);
            }
            Err(err) => {
                eprintln!("forge-desktop: scheduler membership call failed: {err}");
                self.diverged.insert(id);
            }
        }
    }

    fn on_scheduler_error(&mut self, message: impl Into<SharedString>, cx: &mut Context<Self>) {
        cx.push_toast(ToastKind::Error, message);
        self.reload(cx);
    }

    fn dispatch_mode(&self, id: QueueId, mode: QueueMode, cx: &mut Context<Self>) {
        let scheduler = self.scheduler.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                scheduler
                    .set_mode(id, mode)
                    .await
                    .map_err(|e| e.to_string())
            },
            |this, result, cx| {
                if let Err(message) = result {
                    this.on_scheduler_error(tr!("queues_mode_change_failed", error = message), cx);
                }
            },
            cx,
        );
    }

    fn dispatch_pause_all(&self, ids: Vec<QueueId>, cx: &mut Context<Self>) {
        let scheduler = self.scheduler.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let mut failure: Option<String> = None;
                for id in ids {
                    if let Err(err) = scheduler.set_mode(id, QueueMode::PAUSED).await
                        && failure.is_none()
                    {
                        failure = Some(err.to_string());
                    }
                }
                match failure {
                    Some(message) => Err(message),
                    None => Ok(()),
                }
            },
            |this, result, cx| {
                if let Err(message) = result {
                    this.on_scheduler_error(tr!("queues_pause_all_failed", error = message), cx);
                }
            },
            cx,
        );
    }

    fn set_mode(&mut self, id: QueueId, mode: QueueMode, cx: &mut Context<Self>) {
        if let Some(q) = self.queues.iter_mut().find(|q| q.id == id) {
            q.mode = mode;
        }
        self.dispatch_mode(id, mode, cx);
        cx.notify();
    }

    fn toggle_mode(&mut self, id: QueueId, preset: QueueMode, cx: &mut Context<Self>) {
        let current = self.queues.iter().find(|q| q.id == id).map(|q| q.mode);
        let next = if current == Some(preset) {
            QueueMode::RUNNING
        } else {
            preset
        };
        self.set_mode(id, next, cx);
    }

    fn resume(&mut self, id: QueueId, cx: &mut Context<Self>) {
        self.set_mode(id, QueueMode::RUNNING, cx);
    }

    fn free(&mut self, id: QueueId, cx: &mut Context<Self>) {
        let Some(q) = self.queues.iter().find(|q| q.id == id) else {
            return;
        };
        let name = q.name.clone();
        let dropped = q.pending as i64;

        let scheduler = self.scheduler.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move { scheduler.clear(id, true).await.map_err(|e| e.to_string()) },
            move |this, result, cx| match result {
                Ok(()) => {
                    cx.push_toast(
                        ToastKind::Info,
                        tr!(
                            "queues_free_feedback",
                            name = name.as_str(),
                            count = dropped
                        ),
                    );
                    this.reload(cx);
                }
                Err(message) => {
                    this.on_scheduler_error(tr!("queues_free_failed", error = message), cx)
                }
            },
            cx,
        );
    }

    fn pause_all(&mut self, cx: &mut Context<Self>) {
        let ids: Vec<QueueId> = self.queues.iter().map(|q| q.id).collect();
        for q in &mut self.queues {
            q.mode = QueueMode::PAUSED;
        }
        self.dispatch_pause_all(ids, cx);
        cx.notify();
    }

    fn request_delete(&mut self, id: QueueId, cx: &mut Context<Self>) {
        self.menu_open = None;
        self.pending_delete.request(id);
        cx.notify();
    }

    fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.pending_delete.cancel();
        cx.notify();
    }

    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(deleted_id) = self.pending_delete.take() else {
            return;
        };
        cx.notify();

        let queue_repo = Arc::clone(&self.queue_repo);
        let action_repo = Arc::clone(&self.action_repo);
        let scheduler = self.scheduler.clone();
        async_bridge::run_async(
            &self.rt_handle,
            delete_queue(queue_repo, action_repo, scheduler, deleted_id),
            |this, result, cx| match result {
                Ok(()) => this.reload(cx),
                Err(message) => this.on_repo_error(&message, cx),
            },
            cx,
        );
    }

    fn open_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let modal = cx.new(|cx| EditQueueModal::new(None, "", "", PARALLEL_CONCURRENCY, cx));
        modal.update(cx, |m, cx| m.focus(window, cx));
        self._modal_sub = Some(cx.subscribe(&modal, Self::on_modal_event));
        self.modal = Some(modal);
        cx.notify();
    }

    fn open_configure(&mut self, id: QueueId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(q) = self.queues.iter().find(|q| q.id == id) else {
            return;
        };
        let name = q.name.clone();
        let description = q.description.clone();
        let concurrency = q.concurrency;
        let modal =
            cx.new(|cx| EditQueueModal::new(Some(id), &name, &description, concurrency, cx));
        modal.update(cx, |m, cx| m.focus(window, cx));
        self._modal_sub = Some(cx.subscribe(&modal, Self::on_modal_event));
        self.modal = Some(modal);
        cx.notify();
    }

    fn on_modal_event(
        &mut self,
        _modal: Entity<EditQueueModal>,
        event: &EditQueueEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            EditQueueEvent::Submit(draft) => self.persist(draft, cx),
            EditQueueEvent::Cancel => self.close_modal(cx),
        }
    }

    fn close_modal(&mut self, cx: &mut Context<Self>) {
        self.modal = None;
        self._modal_sub = None;
        cx.notify();
    }

    fn toggle_menu(&mut self, id: QueueId, position: Point<Pixels>, cx: &mut Context<Self>) {
        if self.menu_open == Some(id) {
            self.menu_open = None;
        } else {
            self.menu_open = Some(id);
            self.menu_click_pos = Some(position);
        }
        cx.notify();
    }

    fn close_menu(&mut self, cx: &mut Context<Self>) {
        self.menu_open = None;
        cx.notify();
    }

    fn persist(&mut self, draft: &QueueDraft, cx: &mut Context<Self>) {
        let editing = draft.editing;
        let queue = Queue {
            id: editing.unwrap_or_default(),
            name: draft.name.clone(),
            description: draft.description.clone(),
            concurrency: draft.concurrency.clamp(MIN_CONCURRENCY, MAX_CONCURRENCY),
        };
        let id = queue.id;
        let is_edit = editing.is_some();
        if let Some(modal) = self.modal.as_ref() {
            modal.update(cx, |m, cx| {
                m.saving = true;
                cx.notify();
            });
        }
        cx.notify();

        let queue_repo = Arc::clone(&self.queue_repo);
        let scheduler = self.scheduler.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                queue_repo.save(&queue).await.map_err(|e| e.to_string())?;
                let membership = if is_edit {
                    scheduler.reconfigure(queue).await
                } else {
                    scheduler.register(queue).await
                };
                Ok::<_, String>(membership.map_err(|e| e.to_string()))
            },
            move |this, result, cx| match result {
                Ok(membership) => {
                    this.apply_membership_outcome(id, membership);
                    this.close_modal(cx);
                    this.reload(cx);
                }
                Err(message) => this.on_save_error(&message, cx),
            },
            cx,
        );
    }

    fn on_save_error(&mut self, message: &str, cx: &mut Context<Self>) {
        eprintln!("forge-desktop: queue save failed: {message}");
        if let Some(modal) = self.modal.as_ref() {
            modal.update(cx, |m, cx| {
                m.saving = false;
                cx.notify();
            });
        }
        cx.notify();
    }

    fn queue_card(
        &self,
        index: usize,
        q: &QueueRow,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let not_live = self.diverged.contains(&q.id);

        let border_color = if q.mode == QueueMode::RUNNING {
            palette.border_input
        } else {
            with_alpha(mode_ink(q.mode, palette), 0.35)
        };

        let body = div()
            .h_full()
            .flex()
            .flex_col()
            .child(self.card_header(index, q, not_live, palette, cx))
            .child(self.card_metrics(q, palette))
            .child(status_panel(q, palette, density))
            .child(self.card_buttons(index, q, palette, density, cx));

        card(body, palette)
            .full_width()
            .full_height()
            .padding(CARD_PAD)
            .border_color(border_color)
            .into_any_element()
    }

    fn card_header(
        &self,
        index: usize,
        q: &QueueRow,
        not_live: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let spin_id = SharedString::from(format!("q-badge-spin-{}", q.id));
        let name = div()
            .font_family(mono_family())
            .text_size(FONT_SM)
            .text_color(palette.text_primary)
            .child(q.name.clone());

        let mut name_row = div()
            .flex()
            .items_center()
            .gap(BAR_GAP)
            .child(name)
            .child(status_badge(spin_id, q.mode, palette));
        if not_live {
            name_row = name_row.child(not_live_badge(palette));
        }

        let desc_text = if q.description.is_empty() {
            SharedString::from("\u{a0}")
        } else {
            SharedString::from(q.description.clone())
        };
        let desc = div()
            .font_family(body_family())
            .text_size(DESC_FS)
            .text_color(palette.text_muted)
            .child(desc_text);

        let left = div()
            .flex()
            .flex_col()
            .gap(px(3.0))
            .child(name_row)
            .child(desc);

        div()
            .flex()
            .items_start()
            .justify_between()
            .mb(HEADER_GAP)
            .child(left)
            .child(self.card_menu(index, q, palette, cx))
            .into_any_element()
    }

    fn card_menu(
        &self,
        index: usize,
        q: &QueueRow,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = q.id;
        let is_default = q.name == "Default";
        let menu_open = self.menu_open == Some(id);
        let menu_pos = if menu_open { self.menu_click_pos } else { None };
        let view = cx.entity();

        let pause_resume: MenuItem = if q.mode == QueueMode::RUNNING {
            menu_item(
                ("q-menu-pause", index),
                tr!("queues_menu_pause"),
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.set_mode(id, QueueMode::PAUSED, cx)
                }),
            )
            .icon(Icon::PlayerPause)
            .color(palette.warning)
            .into()
        } else {
            menu_item(
                ("q-menu-resume", index),
                tr!("queues_menu_resume"),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.resume(id, cx)),
            )
            .icon(Icon::PlayerPlay)
            .color(palette.success)
            .into()
        };

        menu_button(Icon::DotsVertical, menu_open, palette)
            .placement(MenuPlacement::BottomRight)
            .open_at(menu_pos)
            .items(vec![
                menu_item(
                    ("q-menu-configure", index),
                    tr!("queues_menu_configure"),
                    cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.open_configure(id, window, cx)
                    }),
                )
                .icon(Icon::Settings)
                .into(),
                menu_divider(),
                pause_resume,
                menu_item(
                    ("q-menu-free", index),
                    tr!("queues_menu_free"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.free(id, cx)),
                )
                .icon(Icon::Eraser)
                .into(),
                menu_divider(),
                menu_item(
                    ("q-menu-delete", index),
                    tr!("queues_menu_delete"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.request_delete(id, cx)),
                )
                .icon(Icon::Trash)
                .color(palette.random)
                .disabled(is_default)
                .into(),
            ])
            .on_toggle(
                ("q-menu-trigger", index),
                cx.listener(move |this, ev: &ClickEvent, _, cx| {
                    this.toggle_menu(id, ev.position(), cx)
                }),
            )
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_menu(cx));
            })
            .into_any_element()
    }

    fn card_metrics(&self, q: &QueueRow, palette: &ForgePalette) -> AnyElement {
        let running = q.mode == QueueMode::RUNNING;
        let pending_value_color = if running {
            palette.text_primary
        } else {
            mode_ink(q.mode, palette)
        };
        let pending_hint_color = if running {
            palette.text_faint
        } else {
            mode_ink(q.mode, palette)
        };
        let pending_hint = if q.frozen() {
            SharedString::from(tr!("queues_metric_held"))
        } else if q.in_flight > 0 {
            SharedString::from(tr!("queues_metric_in_flight"))
        } else {
            SharedString::from(tr!("queues_metric_idle"))
        };

        let row = div()
            .flex()
            .flex_row()
            .child(metric_col(
                SharedString::from(tr!("queues_metric_concurrency")),
                q.concurrency.to_string(),
                q.concurrency_label(),
                palette.text_primary,
                palette.text_faint,
                palette,
            ))
            .child(metric_col(
                SharedString::from(tr!("queues_metric_pending")),
                q.pending.to_string(),
                pending_hint,
                pending_value_color,
                pending_hint_color,
                palette,
            ))
            .child(metric_col(
                SharedString::from(tr!("queues_metric_actions")),
                q.actions.to_string(),
                SharedString::from(tr!("queues_metric_assigned")),
                palette.text_primary,
                palette.text_faint,
                palette,
            ));

        div()
            .w_full()
            .pt(SECTION_GAP)
            .mb(SECTION_GAP)
            .border_t(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(row)
            .into_any_element()
    }

    fn card_buttons(
        &self,
        index: usize,
        q: &QueueRow,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = q.id;

        let mut mode_row = div()
            .w_full()
            .flex()
            .flex_row()
            .items_stretch()
            .gap(spacing(Spacing::Xs, density));
        for preset in MODE_PRESETS {
            let active = q.mode == preset.mode;
            let ink = if active {
                palette.shell
            } else {
                palette.text_secondary
            };
            let fill = active.then(|| mode_ink(preset.mode, palette));
            let hint = if active {
                tr!("queues_mode_active_tooltip")
            } else {
                tr!(preset.tooltip_key)
            };
            let mode = preset.mode;
            mode_row = mode_row.child(
                card_button(
                    (preset.element_id, index),
                    preset.glyph,
                    SharedString::from(tr!(preset.label_key)),
                    ink,
                    fill,
                    palette,
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_mode(id, mode, cx)),
                )
                .tooltip(tooltip_builder(hint, palette)),
            );
        }

        let free = card_button(
            ("q-free", index),
            Icon::Eraser,
            SharedString::from(tr!("queues_free_btn")),
            palette.text_secondary,
            None,
            palette,
            cx.listener(move |this, _: &ClickEvent, _, cx| this.free(id, cx)),
        )
        .tooltip(tooltip_builder(tr!("queues_free_tooltip"), palette));

        let mode_row = mode_row
            .child(div().w(BORDER_THIN).bg(palette.border_regular))
            .child(free);

        let configure = card_button(
            ("q-configure", index),
            Icon::Settings,
            SharedString::from(tr!("queues_configure_btn")),
            palette.text_secondary,
            None,
            palette,
            cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.open_configure(id, window, cx)
            }),
        );

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(mode_row)
            .child(div().w_full().flex().flex_row().child(configure))
            .into_any_element()
    }

    fn queue_grid(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let gap = spacing(Spacing::Sm, density);
        let visible = self.visible_indices();
        let cards: Vec<AnyElement> = visible
            .into_iter()
            .map(|index| self.queue_card(index, &self.queues[index], palette, density, cx))
            .collect();

        let mut grid = div().w_full().flex().flex_col().gap(gap);
        let mut iter = cards.into_iter().peekable();
        while iter.peek().is_some() {
            let mut row = div().w_full().flex().flex_row().gap(gap);
            for _ in 0..CARDS_PER_ROW {
                match iter.next() {
                    Some(card) => row = row.child(div().flex_1().min_w_0().child(card)),
                    None => row = row.child(div().flex_1().min_w_0()),
                }
            }
            grid = grid.child(row);
        }
        grid.into_any_element()
    }

    fn render_delete_confirm(
        &self,
        name: SharedString,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let card = confirm_modal(
            tr!("queues_delete_confirm_title"),
            tr!("queues_delete_confirm_body"),
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(name)
        .esc_hint(tr!("widget_confirm_esc_to_cancel"))
        .on_cancel(
            "queues-delete-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_delete(cx)),
        )
        .on_confirm(
            "queues-delete-confirm",
            tr!("common_delete"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_delete(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("queues-delete-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_delete(cx));
            })
    }
}

impl QueuesView {
    fn render_stats<'a>(&self, palette: &'a ForgePalette) -> impl IntoElement + use<'a> {
        let running_count = self
            .queues
            .iter()
            .filter(|q| q.mode == QueueMode::RUNNING)
            .count();
        let paused_count = self.queues.len().saturating_sub(running_count);

        header_stats(
            vec![
                header_stat(
                    self.queues.len().to_string(),
                    palette.text_primary,
                    tr!("queues_stat_queues"),
                ),
                header_stat(
                    running_count.to_string(),
                    palette.success,
                    tr!("queues_stat_running"),
                ),
                header_stat(
                    paused_count.to_string(),
                    palette.warning,
                    tr!("queues_stat_paused"),
                ),
            ],
            palette,
        )
    }

    fn render_subheader_left(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let mut chips = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density));
        for (id, filter) in QueueFilter::TABS {
            let active = self.status_filter == filter;
            chips = chips.child(
                chip(
                    tr!(filter.label_key()),
                    ChipGlyph::Dot(filter.dot(palette)),
                    active,
                    palette,
                )
                .density(density)
                .on_click(
                    id,
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.set_status_filter(filter, cx)
                    }),
                ),
            );
        }

        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(div().w(SEARCH_W).child(self.search.field().clone()))
            .child(chips)
    }

    fn render_subheader_right(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let pause_all =
            ghost_button_with_icon(Icon::PlayerPause, tr!("queues_pause_all_btn"), palette)
                .ink(palette.warning)
                .on_click(
                    "q-pause-all",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.pause_all(cx)),
                );
        let new_queue = primary_button_with_icon(Icon::Plus, tr!("queues_new_queue_btn"), palette)
            .density(density)
            .on_click(
                "q-new",
                cx.listener(|this, _: &ClickEvent, window, cx| this.open_new(window, cx)),
            );

        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(pause_all)
            .child(new_queue)
    }
}

impl Render for QueuesView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let stats = self.render_stats(&palette);
        let subheader_left = self.render_subheader_left(&palette, density, cx);
        let subheader_right = self.render_subheader_right(&palette, density, cx);

        let subtitle = div()
            .font_family(body_family())
            .text_size(STATS_FS)
            .text_color(palette.text_muted)
            .child(tr!("queues_subtitle"));

        let visible_count = self.visible_indices().len();
        let body = if self.queues.is_empty() {
            let caption = if self.loading {
                SharedString::from(tr!("queues_loading"))
            } else {
                SharedString::from(tr!("queues_empty"))
            };
            let mut state = empty_state(caption, &palette).density(density);
            if self.loading {
                state = state.loading("queues-loading");
            }
            state.into_any_element()
        } else if visible_count == 0 {
            empty_state(tr!("queues_no_filter_match"), &palette)
                .density(density)
                .into_any_element()
        } else {
            self.queue_grid(&palette, density, cx)
        };

        let scroll = div()
            .id("queues-scroll")
            .flex_1()
            .h_full()
            .overflow_y_scroll()
            .bg(palette.base)
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(spacing(Spacing::Sm, density))
                    .p(spacing(Spacing::Md, density))
                    .child(subtitle)
                    .child(body),
            );

        let body_col = div().flex_1().h_full().flex().flex_col().child(scroll);

        let frame = page_frame(
            vec![
                BreadcrumbCrumb::leaf(tr!("queues_breadcrumb_automation")),
                BreadcrumbCrumb::leaf(tr!("queues_breadcrumb_queues")),
            ],
            &palette,
        )
        .header_right(stats)
        .subheader_left(subheader_left)
        .subheader_right(subheader_right)
        .density(density)
        .body(body_col);

        let modal_overlay = self.modal.clone();

        let delete_overlay = self.pending_delete.get().copied().and_then(|id| {
            let name = self.queues.iter().find(|q| q.id == id)?.name.clone();
            Some(self.render_delete_confirm(SharedString::from(name), &palette, cx))
        });

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(frame)
            .children(modal_overlay)
            .children(delete_overlay)
    }
}

fn status_badge(spin_id: SharedString, mode: QueueMode, palette: &ForgePalette) -> AnyElement {
    let ink = mode_ink(mode, palette);
    let mark: AnyElement = if mode == QueueMode::RUNNING {
        spinner(spin_id, Icon::Loader2, BADGE_GLYPH, ink).into_any_element()
    } else {
        icon(mode_glyph(mode), BADGE_GLYPH, ink).into_any_element()
    };
    let label = SharedString::from(tr!(mode_badge_key(mode)));
    div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xxs, Density::Cozy))
        .py(spacing(Spacing::Xxs, Density::Cozy))
        .px(spacing(Spacing::Xs, Density::Cozy))
        .rounded(BADGE_RADIUS)
        .bg(palette.border_regular)
        .child(mark)
        .child(
            div()
                .font_family(mono_family())
                .text_size(FONT_XS)
                .text_color(ink)
                .child(label),
        )
        .into_any_element()
}

fn metric_col(
    caption: SharedString,
    value: String,
    hint: SharedString,
    value_color: Rgba,
    hint_color: Rgba,
    palette: &ForgePalette,
) -> impl IntoElement {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .child(
            div()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_muted)
                .mb(px(3.0))
                .child(caption),
        )
        .child(
            div()
                .font_family(mono_family())
                .font_weight(FontWeight::MEDIUM)
                .text_size(STAT_VALUE_FS)
                .text_color(value_color)
                .child(value),
        )
        .child(
            div()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(hint_color)
                .child(hint),
        )
}

fn status_panel(q: &QueueRow, palette: &ForgePalette, density: Density) -> AnyElement {
    if q.mode != QueueMode::RUNNING {
        return mode_panel(q, palette, density);
    }
    if q.running.is_empty() {
        return activity_panel(q, palette, density);
    }
    if !q.blocking && q.running.len() > 1 {
        return concurrent_panel(q, palette, density);
    }
    serial_panel(q, palette, density)
}

fn mode_panel(q: &QueueRow, palette: &ForgePalette, density: Density) -> AnyElement {
    let ink = mode_ink(q.mode, palette);
    status_strip(
        icon(mode_glyph(q.mode), PANEL_GLYPH, ink).into_any_element(),
        SharedString::from(tr!(mode_caption_key(q.mode))),
        palette.text_primary,
        Some(ink),
        q,
        palette,
        density,
    )
}

fn activity_panel(q: &QueueRow, palette: &ForgePalette, density: Density) -> AnyElement {
    if q.in_flight == 0 {
        return status_strip(
            icon(Icon::CircleDashed, PANEL_GLYPH, palette.text_faint).into_any_element(),
            SharedString::from(tr!("queues_no_actions_running")),
            palette.text_faint,
            None,
            q,
            palette,
            density,
        );
    }
    let spin_id = SharedString::from(format!("q-strip-spin-{}", q.id));
    status_strip(
        spinner(spin_id, Icon::Loader2, PANEL_GLYPH, palette.brand).into_any_element(),
        SharedString::from(tr!("queues_running_count", count = q.in_flight as i64)),
        palette.text_primary,
        None,
        q,
        palette,
        density,
    )
}

fn status_strip(
    mark: AnyElement,
    caption: SharedString,
    caption_ink: Rgba,
    tint: Option<Rgba>,
    q: &QueueRow,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let counts = div()
        .font_family(mono_family())
        .text_size(FONT_XXS)
        .text_color(palette.text_faint)
        .child(SharedString::from(tr!(
            "queues_strip_counts",
            pending = q.pending as i64,
            in_flight = q.in_flight as i64
        )));

    let mut strip = div()
        .w_full()
        .flex()
        .items_center()
        .gap(BAR_GAP)
        .py(spacing(Spacing::Xs, density))
        .px(spacing(Spacing::Sm, density))
        .mb(SECTION_GAP)
        .rounded(radius(Radius::Sm))
        .child(mark)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(caption_ink)
                .child(caption),
        );
    if q.overflowed > 0 {
        strip = strip.child(overflow_badge(q.overflowed, palette));
    }
    strip = strip.child(counts);

    match tint {
        Some(ink) => strip
            .bg(with_alpha(ink, 0.06))
            .border(BORDER_THIN)
            .border_color(with_alpha(ink, 0.20)),
        None => strip.bg(palette.shell),
    }
    .into_any_element()
}

fn overflow_badge(overflowed: u64, palette: &ForgePalette) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xxs, Density::Cozy))
        .py(spacing(Spacing::Xxs, Density::Cozy))
        .px(spacing(Spacing::Xs, Density::Cozy))
        .rounded(BADGE_RADIUS)
        .bg(with_alpha(palette.random, 0.12))
        .border(BORDER_THIN)
        .border_color(with_alpha(palette.random, 0.30))
        .child(icon(Icon::AlertTriangle, BADGE_GLYPH, palette.random))
        .child(
            div()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.random)
                .child(SharedString::from(tr!(
                    "queues_overflow_badge",
                    count = overflowed as i64,
                    cap = MAX_PENDING_PER_QUEUE as i64
                ))),
        )
        .into_any_element()
}

fn serial_panel(q: &QueueRow, palette: &ForgePalette, density: Density) -> AnyElement {
    let name = q.running.first().cloned().unwrap_or_default();
    let spin_id = SharedString::from(format!("q-serial-spin-{}", q.id));
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(BAR_GAP)
        .py(spacing(Spacing::Xs, density))
        .px(spacing(Spacing::Sm, density))
        .mb(SECTION_GAP)
        .rounded(radius(Radius::Sm))
        .bg(palette.shell)
        .child(spinner(spin_id, Icon::Loader2, PANEL_GLYPH, palette.brand))
        .child(
            div()
                .flex_1()
                .font_family(mono_family())
                .text_size(FONT_XS)
                .text_color(palette.text_primary)
                .child(name),
        )
        .child(
            div()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(SharedString::from(tr!("queues_running_label"))),
        )
        .into_any_element()
}

fn concurrent_panel(q: &QueueRow, palette: &ForgePalette, density: Density) -> AnyElement {
    let shown = q.running.len().min(MAX_PILLS);
    let overflow = q.running.len().saturating_sub(MAX_PILLS);

    let mut pills = div().flex().flex_wrap().gap(spacing(Spacing::Xxs, density));
    for name in &q.running[..shown] {
        pills = pills.child(running_pill(name.clone(), palette));
    }
    if overflow > 0 {
        pills = pills.child(running_pill(
            tr!("queues_overflow_more", count = overflow as i64),
            palette,
        ));
    }

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, density))
        .py(spacing(Spacing::Xs, density))
        .px(spacing(Spacing::Sm, density))
        .mb(SECTION_GAP)
        .rounded(radius(Radius::Sm))
        .bg(palette.shell)
        .child(
            div()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(SharedString::from(tr!("queues_running_now_header"))),
        )
        .child(pills)
        .into_any_element()
}

fn running_pill(label: impl Into<SharedString>, palette: &ForgePalette) -> impl IntoElement {
    badge(
        palette.border_regular,
        palette.text_primary,
        label,
        true,
        FONT_XS,
    )
    .weight(FontWeight::NORMAL)
    .padding_xy(
        spacing(Spacing::Xxs, Density::Cozy),
        spacing(Spacing::Xs, Density::Cozy),
    )
    .radius(PILL_RADIUS)
}

fn card_button(
    id: impl Into<gpui::ElementId>,
    glyph: Icon,
    label: SharedString,
    ink: Rgba,
    fill: Option<Rgba>,
    palette: &ForgePalette,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let mut btn = div()
        .id(id.into())
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(5.0))
        .py(px(5.0))
        .px(px(11.0))
        .rounded(radius(Radius::Sm))
        .cursor_pointer()
        .on_click(handler)
        .child(icon(glyph, PANEL_GLYPH, ink))
        .child(
            div()
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(ink)
                .child(label),
        );
    match fill {
        Some(bg) => btn = btn.bg(bg),
        None => {
            let hover_border = palette.border_input;
            btn = btn
                .border(BORDER_THIN)
                .border_color(palette.border_regular)
                .hover(move |s| s.border_color(hover_border));
        }
    }
    btn
}

async fn load_queues(
    queue_repo: Arc<dyn QueueRepo>,
    action_repo: Arc<dyn ActionRepo>,
    scheduler: QueueSchedulerHandle,
) -> Result<Vec<QueueRow>, String> {
    let queues = queue_repo.list().await.map_err(|e| e.to_string())?;
    let actions = action_repo.list().await.map_err(|e| e.to_string())?;
    let states = scheduler.queue_states().await.unwrap_or_default();

    let rows = queues
        .into_iter()
        .map(|q| {
            let assigned = actions.iter().filter(|a| a.queue_id == q.id).count() as u32;
            let concurrency = q.concurrency.max(1);
            let state = states.get(&q.id).copied();
            QueueRow {
                id: q.id,
                name: q.name,
                description: q.description,
                blocking: concurrency == SERIAL_CONCURRENCY,
                concurrency,
                mode: state.map_or(QueueMode::RUNNING, |s| s.mode),
                pending: state.map_or(0, |s| s.pending as u32),
                in_flight: state.map_or(0, |s| s.in_flight as u32),
                overflowed: state.map_or(0, |s| s.overflowed),
                actions: assigned,
                running: vec![],
            }
        })
        .collect();

    Ok(rows)
}

async fn delete_queue(
    queue_repo: Arc<dyn QueueRepo>,
    action_repo: Arc<dyn ActionRepo>,
    scheduler: QueueSchedulerHandle,
    deleted_id: QueueId,
) -> Result<(), String> {
    let queues = queue_repo.list().await.map_err(|e| e.to_string())?;
    let Some(default) = queues.into_iter().find(|q| q.name == "Default") else {
        return Err("default queue missing".to_string());
    };
    let default_id = default.id;
    if deleted_id == default_id {
        return Ok(());
    }

    let actions = action_repo.list().await.map_err(|e| e.to_string())?;
    for mut action in actions {
        if action.queue_id == deleted_id {
            action.queue_id = default_id;
            action_repo.save(&action).await.map_err(|e| e.to_string())?;
        }
    }

    queue_repo
        .delete(deleted_id)
        .await
        .map_err(|e| e.to_string())?;
    if let Err(err) = scheduler.deregister(deleted_id).await {
        eprintln!("forge-desktop: queue deregister failed: {err}");
    }
    Ok(())
}

fn not_live_badge(palette: &ForgePalette) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xxs, Density::Cozy))
        .py(spacing(Spacing::Xxs, Density::Cozy))
        .px(spacing(Spacing::Xs, Density::Cozy))
        .rounded(BADGE_RADIUS)
        .bg(with_alpha(palette.warning, 0.12))
        .border(BORDER_THIN)
        .border_color(with_alpha(palette.warning, 0.30))
        .child(icon(Icon::AlertTriangle, BADGE_GLYPH, palette.warning))
        .child(
            div()
                .font_family(mono_family())
                .text_size(FONT_XS)
                .text_color(palette.warning)
                .child(SharedString::from(tr!("queues_not_live_badge"))),
        )
        .into_any_element()
}
