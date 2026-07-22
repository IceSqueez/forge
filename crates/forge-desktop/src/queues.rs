use std::collections::HashSet;
use std::sync::Arc;

use forge_components::confirm::ConfirmTone;
use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, ChipGlyph, Confirm, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY,
    Density, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon, InputEvent, MenuItem, MenuPlacement,
    ModalSize, OverlayPosition, Radius, SearchState, Spacing, TextInput, badge, card, chip,
    confirm_modal, empty_state, ghost_button_with_icon, header_stat, header_stats, icon,
    menu_button, menu_divider, menu_item, modal, overlay, page_frame, primary_button,
    primary_button_with_icon, radius, secondary_button, slider, spacing, spinner, tr, with_alpha,
};
use forge_events::{Event, EventSource};
use forge_runtime::{EventBus, MembershipOutcome, QueueSchedulerHandle};
use forge_storage::{ActionRepo, QueueRepo};
use forge_types::{Queue, QueueId};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, FontWeight, Pixels, Point, SharedString,
    Subscription, Window, div, prelude::*, px,
};

use crate::async_bridge;
use crate::presentation::ActivePresentation;
use crate::queue_health::QueueHealth;

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

    fn keeps(self, row: &QueueRow, effective_paused: bool) -> bool {
        match self {
            QueueFilter::All => true,
            QueueFilter::Running => !effective_paused,
            QueueFilter::Paused => effective_paused,
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

    fn dot(self, palette: &ForgePalette) -> gpui::Rgba {
        match self {
            QueueFilter::All => palette.brand,
            QueueFilter::Running => palette.success,
            QueueFilter::Paused => palette.warning,
            QueueFilter::Parallel => palette.info,
            QueueFilter::Sequential => palette.bits,
        }
    }
}

fn queue_matches(
    row: &QueueRow,
    filter: QueueFilter,
    search: &SearchState,
    effective_paused: bool,
) -> bool {
    filter.keeps(row, effective_paused) && search.matches(&row.name)
}

struct QueueRow {
    id: QueueId,
    name: String,
    description: String,
    blocking: bool,
    concurrency: u32,
    paused: bool,
    pending: u32,
    in_flight: u32,
    actions: u32,
    running: Vec<String>,
    paused_since_min: Option<i64>,
}

impl QueueRow {
    fn mode_label(&self) -> SharedString {
        if self.blocking {
            SharedString::from(tr!("queues_metric_serial"))
        } else {
            SharedString::from(tr!("queues_metric_parallel"))
        }
    }
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

pub struct QueuesView {
    queues: Vec<QueueRow>,
    loading: bool,
    feedback: Option<SharedString>,
    modal: Option<EditQueueModal>,
    pending_delete: Confirm<QueueId>,
    menu_open: Option<QueueId>,
    menu_click_pos: Option<Point<Pixels>>,
    diverged: HashSet<QueueId>,
    queue_health: Entity<QueueHealth>,
    scheduler: QueueSchedulerHandle,
    bus: Arc<EventBus>,
    queue_repo: Arc<dyn QueueRepo>,
    action_repo: Arc<dyn ActionRepo>,
    rt_handle: tokio::runtime::Handle,
    status_filter: QueueFilter,
    search: SearchState,
    _health_obs: Subscription,
    _search_sub: Subscription,
}

impl QueuesView {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        queue_health: Entity<QueueHealth>,
        scheduler: QueueSchedulerHandle,
        bus: Arc<EventBus>,
        queue_repo: Arc<dyn QueueRepo>,
        action_repo: Arc<dyn ActionRepo>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let health_obs = cx.observe(&queue_health, |_this, _health, cx| cx.notify());
        let search = SearchState::new(cx, palette, tr!("queues_search_placeholder"));
        let search_sub = cx.subscribe(search.field(), Self::on_search_event);
        let view = Self {
            queues: vec![],
            loading: true,
            feedback: None,
            modal: None,
            pending_delete: Confirm::default(),
            menu_open: None,
            menu_click_pos: None,
            diverged: HashSet::new(),
            queue_health,
            scheduler,
            bus,
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

    fn effective_paused(&self, row: &QueueRow, cx: &Context<Self>) -> bool {
        row.paused || self.queue_health.read(cx).is_paused(row.id)
    }

    fn visible_indices(&self, cx: &Context<Self>) -> Vec<usize> {
        self.queues
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                let effective_paused = self.effective_paused(row, cx);
                queue_matches(row, self.status_filter, &self.search, effective_paused)
            })
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

    fn dispatch_pause(&self, id: QueueId) {
        let scheduler = self.scheduler.clone();
        self.rt_handle.spawn(async move {
            if let Err(err) = scheduler.pause(id).await {
                eprintln!("forge-desktop: queue pause failed: {err}");
            }
        });
    }

    fn dispatch_resume(&self, id: QueueId) {
        let scheduler = self.scheduler.clone();
        self.rt_handle.spawn(async move {
            if let Err(err) = scheduler.resume(id).await {
                eprintln!("forge-desktop: queue resume failed: {err}");
            }
        });
    }

    fn dispatch_drain(&self, id: QueueId) {
        let scheduler = self.scheduler.clone();
        let bus = Arc::clone(&self.bus);
        self.rt_handle.spawn(async move {
            bus.publish(Event::new(
                EventSource::Core,
                "queue.drain_requested",
                serde_json::json!({ "queue_id": id.to_string() }),
            ));
            if let Err(err) = scheduler.pause(id).await {
                eprintln!("forge-desktop: queue drain pause failed: {err}");
            }
        });
    }

    fn dispatch_pause_all(&self, ids: Vec<QueueId>) {
        let scheduler = self.scheduler.clone();
        self.rt_handle.spawn(async move {
            for id in ids {
                if let Err(err) = scheduler.pause(id).await {
                    eprintln!("forge-desktop: queue pause-all failed: {err}");
                }
            }
        });
    }

    fn persist_paused(&self, id: QueueId, paused: bool) {
        let queue_repo = Arc::clone(&self.queue_repo);
        self.rt_handle.spawn(async move {
            match queue_repo.get(id).await {
                Ok(Some(mut queue)) => {
                    if queue.paused != paused {
                        queue.paused = paused;
                        if let Err(err) = queue_repo.save(&queue).await {
                            eprintln!("forge-desktop: queue pause persist failed: {err}");
                        }
                    }
                }
                Ok(None) => {}
                Err(err) => {
                    eprintln!("forge-desktop: queue pause persist load failed: {err}");
                }
            }
        });
    }

    fn pause(&mut self, id: QueueId, cx: &mut Context<Self>) {
        if let Some(q) = self.queues.iter_mut().find(|q| q.id == id) {
            q.paused = true;
            q.paused_since_min = Some(0);
        }
        self.dispatch_pause(id);
        self.persist_paused(id, true);
        cx.notify();
    }

    fn resume(&mut self, id: QueueId, cx: &mut Context<Self>) {
        if let Some(q) = self.queues.iter_mut().find(|q| q.id == id) {
            q.paused = false;
            q.paused_since_min = None;
        }
        self.dispatch_resume(id);
        self.persist_paused(id, false);
        cx.notify();
    }

    fn drain(&mut self, id: QueueId, cx: &mut Context<Self>) {
        if let Some(q) = self.queues.iter_mut().find(|q| q.id == id) {
            q.paused = true;
            q.paused_since_min = Some(0);
            self.feedback = Some(tr!("queues_drain_feedback", name = q.name.as_str()).into());
        }
        self.dispatch_drain(id);
        self.persist_paused(id, true);
        cx.notify();
    }

    fn pause_all(&mut self, cx: &mut Context<Self>) {
        let ids: Vec<QueueId> = self.queues.iter().map(|q| q.id).collect();
        for q in &mut self.queues {
            if !q.paused {
                q.paused = true;
                q.paused_since_min = Some(0);
            }
        }
        for id in &ids {
            self.persist_paused(*id, true);
        }
        self.dispatch_pause_all(ids);
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
        let modal = Self::build_modal(None, "", "", PARALLEL_CONCURRENCY, cx);
        modal.name_input.update(cx, |f, cx| f.focus(window, cx));
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
        let modal = Self::build_modal(Some(id), &name, &description, concurrency, cx);
        modal.name_input.update(cx, |f, cx| f.focus(window, cx));
        self.modal = Some(modal);
        cx.notify();
    }

    fn build_modal(
        editing: Option<QueueId>,
        name_seed: &str,
        desc_seed: &str,
        concurrency: u32,
        cx: &mut Context<Self>,
    ) -> EditQueueModal {
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
                InputEvent::Submitted(_) => this.save(cx),
                InputEvent::Cancelled => this.close_modal(cx),
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

    fn close_modal(&mut self, cx: &mut Context<Self>) {
        self.modal = None;
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

    fn set_concurrency(&mut self, value: u32, cx: &mut Context<Self>) {
        if let Some(modal) = self.modal.as_mut() {
            modal.concurrency = value.clamp(MIN_CONCURRENCY, MAX_CONCURRENCY);
        }
        cx.notify();
    }

    fn modal_saveable(&self, cx: &Context<Self>) -> bool {
        self.modal.as_ref().is_some_and(|modal| {
            !modal.saving && !modal.name_input.read(cx).content().trim().is_empty()
        })
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if !self.modal_saveable(cx) {
            return;
        }
        let Some(modal) = self.modal.as_ref() else {
            return;
        };
        let name = modal.name_input.read(cx).content().trim().to_owned();
        let description = modal.desc_input.read(cx).content().trim().to_owned();
        let concurrency = modal.concurrency.clamp(MIN_CONCURRENCY, MAX_CONCURRENCY);
        let editing = modal.editing;
        let paused = editing
            .and_then(|id| self.queues.iter().find(|q| q.id == id))
            .map(|q| q.paused)
            .unwrap_or(false);
        let queue = Queue {
            id: editing.unwrap_or_else(QueueId::new),
            name,
            description,
            concurrency,
            paused,
        };
        let id = queue.id;
        let is_edit = editing.is_some();
        if let Some(modal) = self.modal.as_mut() {
            modal.saving = true;
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
                    this.modal = None;
                    this.reload(cx);
                    cx.notify();
                }
                Err(message) => this.on_save_error(&message, cx),
            },
            cx,
        );
    }

    fn on_save_error(&mut self, message: &str, cx: &mut Context<Self>) {
        eprintln!("forge-desktop: queue save failed: {message}");
        if let Some(modal) = self.modal.as_mut() {
            modal.saving = false;
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
        let paused = self.effective_paused(q, cx);
        let not_live = self.diverged.contains(&q.id);

        let border_color = if paused {
            with_alpha(palette.warning, 0.35)
        } else {
            palette.border_input
        };

        let body = div()
            .h_full()
            .flex()
            .flex_col()
            .child(self.card_header(index, q, paused, not_live, palette, cx))
            .child(self.card_metrics(q, paused, palette))
            .child(self.running_panel(q, paused, palette, density))
            .child(self.card_buttons(index, q, paused, palette, density, cx));

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
        paused: bool,
        not_live: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let spin_id = SharedString::from(format!("q-badge-spin-{}", q.id));
        let name = div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_SM)
            .text_color(palette.text_primary)
            .child(q.name.clone());

        let mut name_row = div()
            .flex()
            .items_center()
            .gap(BAR_GAP)
            .child(name)
            .child(status_badge(spin_id, paused, palette));
        if not_live {
            name_row = name_row.child(not_live_badge(palette));
        }

        let desc_text = if q.description.is_empty() {
            SharedString::from("\u{a0}")
        } else {
            SharedString::from(q.description.clone())
        };
        let desc = div()
            .font_family(DEFAULT_BODY_FAMILY)
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
            .child(self.card_menu(index, q, paused, palette, cx))
            .into_any_element()
    }

    fn card_menu(
        &self,
        index: usize,
        q: &QueueRow,
        paused: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = q.id;
        let is_default = q.name == "Default";
        let menu_open = self.menu_open == Some(id);
        let menu_pos = if menu_open { self.menu_click_pos } else { None };
        let view = cx.entity();

        let pause_resume: MenuItem = if paused {
            menu_item(
                ("q-menu-resume", index),
                tr!("queues_menu_resume"),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.resume(id, cx)),
            )
            .icon(Icon::PlayerPlay)
            .color(palette.warning)
            .into()
        } else {
            menu_item(
                ("q-menu-pause", index),
                tr!("queues_menu_pause"),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.pause(id, cx)),
            )
            .icon(Icon::PlayerPause)
            .color(palette.warning)
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
                    ("q-menu-drain", index),
                    tr!("queues_menu_drain"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.drain(id, cx)),
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

    fn card_metrics(&self, q: &QueueRow, paused: bool, palette: &ForgePalette) -> AnyElement {
        let pending_value_color = if paused {
            palette.warning
        } else {
            palette.text_primary
        };
        let pending_hint_color = if paused {
            palette.warning
        } else {
            palette.text_faint
        };
        let pending_hint = if paused {
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
                q.mode_label(),
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

    fn running_panel(
        &self,
        q: &QueueRow,
        paused: bool,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        if paused {
            return paused_panel(q, palette, density);
        }
        if q.running.is_empty() {
            return idle_panel(palette, density);
        }
        if !q.blocking && q.running.len() > 1 {
            return concurrent_panel(q, palette, density);
        }
        serial_panel(q, palette, density)
    }

    fn card_buttons(
        &self,
        index: usize,
        q: &QueueRow,
        paused: bool,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = q.id;

        let action = if paused {
            card_button(
                ("q-resume", index),
                Icon::PlayerPlay,
                SharedString::from(tr!("queues_resume_btn")),
                palette.shell,
                Some(palette.success),
                palette,
                cx.listener(move |this, _: &ClickEvent, _, cx| this.resume(id, cx)),
            )
        } else {
            card_button(
                ("q-pause", index),
                Icon::PlayerPause,
                SharedString::from(tr!("queues_pause_btn")),
                palette.warning,
                None,
                palette,
                cx.listener(move |this, _: &ClickEvent, _, cx| this.pause(id, cx)),
            )
        };

        let drain = card_button(
            ("q-drain", index),
            Icon::Eraser,
            SharedString::from(tr!("queues_drain_btn")),
            palette.text_secondary,
            None,
            palette,
            cx.listener(move |this, _: &ClickEvent, _, cx| this.drain(id, cx)),
        );

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
            .flex_row()
            .gap(spacing(Spacing::Xs, density))
            .child(action)
            .child(drain)
            .child(configure)
            .into_any_element()
    }

    fn queue_grid(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let gap = spacing(Spacing::Sm, density);
        let visible = self.visible_indices(cx);
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

    fn render_modal(
        &self,
        modal_state: &EditQueueModal,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = if modal_state.editing.is_some() {
            tr!("queues_edit_title", name = modal_state.orig_name.clone())
        } else {
            tr!("queues_create_title")
        };

        let concurrency = modal_state.concurrency;
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
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(SharedString::from(
                        tr!("queues_create_name_label").to_uppercase(),
                    )),
            )
            .child(div().child(modal_state.name_input.clone()));

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
                            .font_family(DEFAULT_MONO_FAMILY)
                            .text_size(FONT_XXS)
                            .text_color(palette.text_faint)
                            .child(SharedString::from(
                                tr!("queues_create_desc_label").to_uppercase(),
                            )),
                    )
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_XXS)
                            .text_color(palette.text_muted)
                            .child(SharedString::from(tr!("queues_create_desc_optional"))),
                    ),
            )
            .child(div().child(modal_state.desc_input.clone()));

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
                                palette,
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
                            .font_family(DEFAULT_MONO_FAMILY)
                            .text_size(FONT_SM)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(palette.text_primary)
                            .child(concurrency.to_string()),
                    ),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
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
                    .font_family(DEFAULT_MONO_FAMILY)
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

        let saveable = self.modal_saveable(cx);
        let save_label = if modal_state.editing.is_some() {
            tr!("queues_edit_btn")
        } else {
            tr!("queues_create_btn")
        };
        let cancel = secondary_button(tr!("queues_create_cancel"), palette).on_click(
            "q-modal-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.close_modal(cx)),
        );
        let save = primary_button(save_label, palette)
            .disabled(!saveable)
            .on_click(
                "q-modal-save",
                cx.listener(|this, _: &ClickEvent, _, cx| this.save(cx)),
            );
        let hint = div()
            .font_family(DEFAULT_BODY_FAMILY)
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

        let card = modal(title, body, palette)
            .header_icon(Icon::Stack2, palette.bits)
            .subtitle(tr!("queues_create_subtitle"))
            .size(ModalSize::Md)
            .footer(footer)
            .on_close(
                "q-modal-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.close_modal(cx)),
            );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("q-modal-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.close_modal(cx));
            })
            .into_any_element()
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

    fn feedback_banner(
        &self,
        message: SharedString,
        palette: &ForgePalette,
        density: Density,
    ) -> impl IntoElement {
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Md, density))
            .bg(with_alpha(palette.success, 0.10))
            .border_b(BORDER_THIN)
            .border_color(with_alpha(palette.success, 0.25))
            .child(icon(Icon::Notebook, FONT_XS, palette.success))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_secondary)
                    .child(message),
            )
    }
}

impl QueuesView {
    fn render_stats<'a>(
        &self,
        palette: &'a ForgePalette,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<'a> {
        let paused_count = self
            .queues
            .iter()
            .filter(|q| self.effective_paused(q, cx))
            .count();
        let running_count = self.queues.len().saturating_sub(paused_count);

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

        let stats = self.render_stats(&palette, cx);
        let subheader_left = self.render_subheader_left(&palette, density, cx);
        let subheader_right = self.render_subheader_right(&palette, density, cx);

        let subtitle = div()
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(STATS_FS)
            .text_color(palette.text_muted)
            .child(tr!("queues_subtitle"));

        let feedback = self
            .feedback
            .clone()
            .map(|message| self.feedback_banner(message, &palette, density));

        let visible_count = self.visible_indices(cx).len();
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

        let body_col = div()
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .children(feedback)
            .child(scroll);

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

        let modal_overlay = self
            .modal
            .as_ref()
            .map(|modal_state| self.render_modal(modal_state, &palette, density, cx));

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

fn status_badge(spin_id: SharedString, paused: bool, palette: &ForgePalette) -> AnyElement {
    let (mark, label, ink): (AnyElement, SharedString, gpui::Rgba) = if paused {
        (
            icon(Icon::PlayerPause, BADGE_GLYPH, palette.warning).into_any_element(),
            SharedString::from(tr!("queues_status_paused")),
            palette.warning,
        )
    } else {
        (
            spinner(spin_id, Icon::Loader2, BADGE_GLYPH, palette.success).into_any_element(),
            SharedString::from(tr!("queues_status_running")),
            palette.success,
        )
    };
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
                .font_family(DEFAULT_MONO_FAMILY)
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
    value_color: gpui::Rgba,
    hint_color: gpui::Rgba,
    palette: &ForgePalette,
) -> impl IntoElement {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XXS)
                .text_color(palette.text_muted)
                .mb(px(3.0))
                .child(caption),
        )
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .font_weight(FontWeight::MEDIUM)
                .text_size(STAT_VALUE_FS)
                .text_color(value_color)
                .child(value),
        )
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XXS)
                .text_color(hint_color)
                .child(hint),
        )
}

fn idle_panel(palette: &ForgePalette, density: Density) -> AnyElement {
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
        .child(icon(Icon::CircleDashed, PANEL_GLYPH, palette.text_faint))
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_faint)
                .child(SharedString::from(tr!("queues_no_actions_running"))),
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
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_primary)
                .child(name),
        )
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
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
                .font_family(DEFAULT_MONO_FAMILY)
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

fn paused_panel(q: &QueueRow, palette: &ForgePalette, density: Density) -> AnyElement {
    let caption: SharedString = match q.paused_since_min {
        Some(min) if min > 0 => tr!(
            "queues_paused_with_time",
            pending = q.pending as i64,
            mins = min
        )
        .into(),
        _ => tr!("queues_paused_simple").into(),
    };
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(BAR_GAP)
        .py(spacing(Spacing::Xs, density))
        .px(spacing(Spacing::Sm, density))
        .mb(SECTION_GAP)
        .rounded(radius(Radius::Sm))
        .bg(with_alpha(palette.warning, 0.06))
        .border(BORDER_THIN)
        .border_color(with_alpha(palette.warning, 0.20))
        .child(icon(Icon::AlertTriangle, PANEL_GLYPH, palette.warning))
        .child(
            div()
                .flex_1()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_primary)
                .child(caption),
        )
        .into_any_element()
}

fn card_button(
    id: impl Into<gpui::ElementId>,
    glyph: Icon,
    label: SharedString,
    ink: gpui::Rgba,
    fill: Option<gpui::Rgba>,
    palette: &ForgePalette,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
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
                .font_family(DEFAULT_BODY_FAMILY)
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
    btn.into_any_element()
}

async fn load_queues(
    queue_repo: Arc<dyn QueueRepo>,
    action_repo: Arc<dyn ActionRepo>,
    scheduler: QueueSchedulerHandle,
) -> Result<Vec<QueueRow>, String> {
    let queues = queue_repo.list().await.map_err(|e| e.to_string())?;
    let actions = action_repo.list().await.map_err(|e| e.to_string())?;
    let paused_ids = scheduler.paused_queues().await.unwrap_or_default();

    let rows = queues
        .into_iter()
        .map(|q| {
            let assigned = actions.iter().filter(|a| a.queue_id == q.id).count() as u32;
            let concurrency = q.concurrency.max(1);
            let paused = paused_ids.contains(&q.id);
            QueueRow {
                id: q.id,
                name: q.name,
                description: q.description,
                blocking: concurrency == SERIAL_CONCURRENCY,
                concurrency,
                paused,
                pending: 0,
                in_flight: 0,
                actions: assigned,
                running: vec![],
                paused_since_min: None,
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
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.warning)
                .child(SharedString::from(tr!("queues_not_live_badge"))),
        )
        .into_any_element()
}
