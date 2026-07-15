use std::collections::HashSet;
use std::sync::Arc;

use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_SM,
    FONT_XS, ForgePalette, Icon, InputEvent, OverlayPosition, Radius, Spacing, TextInput,
    breadcrumb, field_label, icon, modal, overlay, primary_button, primary_button_with_icon,
    radius, secondary_button, spacing, toggle, with_alpha,
};
use forge_events::{Event, EventSource};
use forge_runtime::{EventBus, MembershipOutcome, QueueSchedulerHandle};
use forge_storage::{ActionRepo, QueueRepo};
use forge_types::{Queue, QueueId};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, Pixels, SharedString, Subscription, Window, div,
    prelude::*, px,
};

use crate::presentation::ActivePresentation;
use crate::queue_health::QueueHealth;

/// Status-chip corner radius: the parity source pins the pill at a fixed 8px, one
/// step over `Radius::Sm` (6px), so it is carried as a named off-scale literal.
const BADGE_RADIUS: Pixels = px(8.0);
/// Running-status dot diameter (the source's fixed 5px).
const STATUS_DOT: Pixels = px(5.0);
/// Concurrent-pill corner radius (the source's fixed 5px, one under `Radius::Sm`).
const PILL_RADIUS: Pixels = px(5.0);
/// Glyph size of the small in-card status/panel marks (the source's 9–12px marks).
const BADGE_GLYPH: Pixels = px(9.0);
const PANEL_GLYPH: Pixels = px(12.0);
/// The trailing per-card menu affordance glyph (the source's fixed 14px dots mark;
/// decorative in the parity source — no dropdown is attached).
const MENU_GLYPH: Pixels = px(14.0);
/// Leading action glyph on the pause-all / new-queue header buttons (source's 13px).
const HEADER_BTN_GLYPH: Pixels = px(13.0);
/// Queue cards per grid row — the source packs two cards per row and pads a trailing
/// odd card with an equal-flex spacer so it keeps its half-width.
const CARDS_PER_ROW: usize = 2;
/// Most concurrent-action pills shown before collapsing the rest into a "+N more"
/// pill (the source's cap).
const MAX_PILLS: usize = 3;
/// Concurrency implied by a serial (blocking) queue and by a parallel one — the
/// parity source derives the slot count from the blocking flag rather than a
/// persisted column, so a queue created/edited here follows the same rule.
const SERIAL_CONCURRENCY: u32 = 1;
const PARALLEL_CONCURRENCY: u32 = 8;
/// Exact edit/new-queue modal card width — the parity source pins this at 440px; the
/// modal `.width()` override hits it exactly rather than snapping to a `ModalSize`.
const MODAL_WIDTH: Pixels = px(440.0);

/// One action queue as the screen caches it. A view-model standing in for
/// `forge-runtime`'s live queue slot plus its storage row: `id`, `name`, `blocking` are
/// read straight off the persisted [`Queue`] row, `concurrency` is derived from the
/// blocking flag (not yet a persisted column), `actions` is the count of stored actions
/// assigned to this queue, and `desc` is derived from the queue name.
///
/// `pending`, `in_flight` and `running` stay empty because no bus event attributes them
/// per-queue. `paused` is seeded from the scheduler's paused set at load time and then
/// overlaid at render with the live paused set the runtime→UI bridge streams into the
/// [`QueueHealth`] topic, so a pause/resume driven from anywhere (a control here or a
/// queue-control sub-action) shows on the card.
struct QueueRow {
    id: QueueId,
    name: String,
    desc: String,
    blocking: bool,
    concurrency: u32,
    paused: bool,
    pending: u32,
    in_flight: u32,
    actions: u32,
    /// Action ids the scheduler reports as executing right now. Empty = idle; a
    /// single entry with a serial queue renders the serial panel; more than one on a
    /// parallel queue renders the concurrent-pills panel.
    running: Vec<String>,
    /// Minutes since the queue was paused, for the paused panel's caption. `None`
    /// falls back to a generic "queue is paused" line.
    paused_since_min: Option<i64>,
}

impl QueueRow {
    /// The serial/parallel caption under the concurrency metric.
    fn mode_label(&self) -> &'static str {
        if self.blocking { "serial" } else { "parallel" }
    }
}

/// In-flight new/configure-queue form. The name lives in a child [`TextInput`]
/// entity (gpui's text control is a stateful entity, not a value); the form holds
/// that field entity plus the blocking flag, a saving flag, and — on a configure —
/// the target queue id in `editing`. Mirrors the parity source's new/edit form pair,
/// which carries exactly a name and a blocking flag.
struct EditQueueModal {
    editing: Option<QueueId>,
    name_input: Entity<TextInput>,
    blocking: bool,
    saving: bool,
    _name_sub: Subscription,
}

/// The Queues screen view-entity: a breadcrumb header (`Automation / Queues`) with
/// pause-all + new-queue actions, over a scrollable two-column grid of queue cards
/// (name, running/paused badge, description, a concurrency/pending/actions metric
/// row, a live-running panel and pause/drain/configure actions), plus a centred
/// new/configure-queue modal overlay.
///
/// Pulls its queue roster off storage on mount and re-pulls after every create/configure
/// (never patching a row locally), and is live-wired for the queue lifecycle:
/// pause/resume/drain/pause-all optimistically nudge the cached rows for instant feedback
/// and dispatch the matching [`QueueSchedulerHandle`] verb (fire-and-forget on the tokio
/// runtime), while the live paused set streams back over the runtime→UI bridge through
/// the observed [`QueueHealth`] topic. Create/configure persist through
/// [`QueueRepo::save`] then register/reconfigure the live scheduler slot; a queue that
/// storage accepts but the live registry rejects (`NotFound`) is flagged in `diverged`
/// and carries a "not live" badge until restart.
pub struct QueuesView {
    queues: Vec<QueueRow>,
    /// True until the first roster pull lands, so the empty body shows a loading caption
    /// rather than the "no queues" caption before any row arrives.
    loading: bool,
    feedback: Option<SharedString>,
    modal: Option<EditQueueModal>,
    /// Queues persisted but rejected by the live scheduler after a successful storage
    /// write. Not a persisted field, so it is pruned (never rebuilt) on each re-pull;
    /// overlaid onto each card as the "NOT LIVE · RESTART" badge.
    diverged: HashSet<QueueId>,
    /// The shared, bridge-fed live paused set, keyed by [`QueueId`]. Observed for
    /// repaint; overlaid onto each row's paused state at render time so a pause/resume
    /// driven from elsewhere (a queue-control sub-action) shows here too.
    queue_health: Entity<QueueHealth>,
    /// The queue scheduler command handle the controls dispatch through.
    scheduler: QueueSchedulerHandle,
    /// The event bus a drain publishes its `queue.drain_requested` marker on, mirroring
    /// the parity source's drain (publish-then-pause).
    bus: Arc<EventBus>,
    /// Storage handle the roster is read from and create/configure persist through.
    queue_repo: Arc<dyn QueueRepo>,
    /// Storage handle the per-queue assigned-action counts are derived from.
    action_repo: Arc<dyn ActionRepo>,
    /// The tokio runtime handle a control's fire-and-forget dispatch runs on, so the
    /// scheduler round-trip has a reactor rather than gpui's foreground executor.
    rt_handle: tokio::runtime::Handle,
    _health_obs: Subscription,
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
        // Repaint whenever the bridge advances the shared live paused set.
        let health_obs = cx.observe(&queue_health, |_this, _health, cx| cx.notify());
        let view = Self {
            queues: vec![],
            loading: true,
            feedback: None,
            modal: None,
            diverged: HashSet::new(),
            queue_health,
            scheduler,
            bus,
            queue_repo,
            action_repo,
            rt_handle,
            _health_obs: health_obs,
        };
        view.reload(cx);
        view
    }

    // --- async pull + reconcile -------------------------------------------

    /// Pulls the full roster off storage and reconciles the cached rows with it. Every
    /// create/configure routes back here for a full re-pull rather than patching a row
    /// locally, so the roster always mirrors the persisted rows.
    fn reload(&self, cx: &mut Context<Self>) {
        let queue_repo = Arc::clone(&self.queue_repo);
        let action_repo = Arc::clone(&self.action_repo);
        let scheduler = self.scheduler.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let _ = tx.send(load_queues(queue_repo, action_repo, scheduler).await);
        });
        cx.spawn(async move |this, cx| match rx.await {
            Ok(Ok(rows)) => {
                let _ = this.update(cx, |this, cx| this.apply_rows(rows, cx));
            }
            Ok(Err(message)) => {
                let _ = this.update(cx, |this, cx| this.on_repo_error(&message, cx));
            }
            Err(_) => {}
        })
        .detach();
    }

    /// Replaces the cached roster with a fresh pull, pruning divergence flags for ids
    /// that no longer exist (the badge is not a persisted field, so a bare reload would
    /// otherwise strand it).
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

    /// Records (or clears) the saved-but-not-live divergence flag for one queue after a
    /// scheduler call. `Applied`/`AlreadyRegistered` clear it; a channel error or a
    /// `NotFound` outcome leaves storage and the live registry out of sync until restart,
    /// so the flag is set and the card carries the "not live" badge.
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

    // --- command dispatch -------------------------------------------------

    /// Fire-and-forget `pause`/`resume` of a queue on the tokio runtime. A send or
    /// round-trip failure logs a PII-safe notice (only the scheduler error is printed,
    /// which carries a queue id at most, never viewer data) and drops the command —
    /// controls stay responsive rather than blocking the foreground executor.
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

    /// Drain dispatch, mirroring the parity source: publish a `queue.drain_requested`
    /// marker on the bus, then pause the slot so no new work starts while it drains.
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

    /// Pauses every queue in one task, mirroring the parity source's pause-all.
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

    // --- queue actions ----------------------------------------------------

    fn pause(&mut self, id: QueueId, cx: &mut Context<Self>) {
        if let Some(q) = self.queues.iter_mut().find(|q| q.id == id) {
            q.paused = true;
            q.paused_since_min = Some(0);
        }
        self.dispatch_pause(id);
        cx.notify();
    }

    fn resume(&mut self, id: QueueId, cx: &mut Context<Self>) {
        if let Some(q) = self.queues.iter_mut().find(|q| q.id == id) {
            q.paused = false;
            q.paused_since_min = None;
        }
        self.dispatch_resume(id);
        cx.notify();
    }

    /// Drains a queue: optimistically pauses the cached row and notes the request, then
    /// dispatches the drain (publish `queue.drain_requested` + pause) through the
    /// scheduler handle.
    fn drain(&mut self, id: QueueId, cx: &mut Context<Self>) {
        if let Some(q) = self.queues.iter_mut().find(|q| q.id == id) {
            q.paused = true;
            q.paused_since_min = Some(0);
            self.feedback = Some(format!("Draining “{}”.", q.name).into());
        }
        self.dispatch_drain(id);
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
        self.dispatch_pause_all(ids);
        cx.notify();
    }

    // --- modal lifecycle --------------------------------------------------

    fn open_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let modal = Self::build_modal(None, "", false, cx);
        modal.name_input.read(cx).focus(window);
        self.modal = Some(modal);
        cx.notify();
    }

    fn open_configure(&mut self, id: QueueId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(q) = self.queues.iter().find(|q| q.id == id) else {
            return;
        };
        let name = q.name.clone();
        let blocking = q.blocking;
        let modal = Self::build_modal(Some(id), &name, blocking, cx);
        modal.name_input.read(cx).focus(window);
        self.modal = Some(modal);
        cx.notify();
    }

    /// Assembles an [`EditQueueModal`], creating the child name-input entity,
    /// prefilling it on a configure and wiring its submit/cancel/change events.
    fn build_modal(
        editing: Option<QueueId>,
        name_seed: &str,
        blocking: bool,
        cx: &mut Context<Self>,
    ) -> EditQueueModal {
        let palette = cx.palette();
        let name_seed = name_seed.to_owned();
        let name_input = cx.new(|cx| {
            let mut ti = TextInput::new("Queue name (required)", cx).with_palette(palette);
            ti.set_content(name_seed, cx);
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
            name_input,
            blocking,
            saving: false,
            _name_sub: name_sub,
        }
    }

    fn close_modal(&mut self, cx: &mut Context<Self>) {
        self.modal = None;
        cx.notify();
    }

    fn toggle_blocking(&mut self, cx: &mut Context<Self>) {
        if let Some(modal) = self.modal.as_mut() {
            modal.blocking = !modal.blocking;
        }
        cx.notify();
    }

    /// Whether the open modal can be saved: a non-empty name and not mid-save. Reads
    /// the name field entity, so it takes the app context.
    fn modal_saveable(&self, cx: &Context<Self>) -> bool {
        self.modal.as_ref().is_some_and(|modal| {
            !modal.saving && !modal.name_input.read(cx).content().trim().is_empty()
        })
    }

    /// Persists the open form then applies it to the live scheduler. Mirrors the parity
    /// source's persist-then-apply: write the [`Queue`] row through [`QueueRepo::save`]
    /// first, and only on a successful write register (create) or reconfigure (edit) the
    /// live slot. The scheduler outcome sets/clears the row's divergence flag, then the
    /// roster is fully re-pulled rather than patched. A storage-write failure keeps the
    /// modal open with its Save control re-enabled so the user can retry.
    fn save(&mut self, cx: &mut Context<Self>) {
        if !self.modal_saveable(cx) {
            return;
        }
        let Some(modal) = self.modal.as_mut() else {
            return;
        };
        let name = modal.name_input.read(cx).content().trim().to_owned();
        let blocking = modal.blocking;
        let editing = modal.editing;
        let queue = Queue {
            id: editing.unwrap_or_else(QueueId::new),
            name,
            blocking,
        };
        let id = queue.id;
        let is_edit = editing.is_some();
        modal.saving = true;
        cx.notify();

        let queue_repo = Arc::clone(&self.queue_repo);
        let scheduler = self.scheduler.clone();
        let (tx, rx) =
            tokio::sync::oneshot::channel::<Result<Result<MembershipOutcome, String>, String>>();
        self.rt_handle.spawn(async move {
            let outcome = async {
                queue_repo.save(&queue).await.map_err(|e| e.to_string())?;
                let membership = if is_edit {
                    scheduler.reconfigure(queue).await
                } else {
                    scheduler.register(queue).await
                };
                Ok::<_, String>(membership.map_err(|e| e.to_string()))
            }
            .await;
            let _ = tx.send(outcome);
        });

        cx.spawn(async move |this, cx| match rx.await {
            Ok(Ok(membership)) => {
                let _ = this.update(cx, |this, cx| {
                    this.apply_membership_outcome(id, membership);
                    this.modal = None;
                    this.reload(cx);
                    cx.notify();
                });
            }
            Ok(Err(message)) => {
                let _ = this.update(cx, |this, cx| this.on_save_error(&message, cx));
            }
            Err(_) => {}
        })
        .detach();
    }

    /// Handles a storage-write failure on save: clears the saving flag so the modal's
    /// Save control re-enables for a retry, and logs a PII-safe notice.
    fn on_save_error(&mut self, message: &str, cx: &mut Context<Self>) {
        eprintln!("forge-desktop: queue save failed: {message}");
        if let Some(modal) = self.modal.as_mut() {
            modal.saving = false;
        }
        cx.notify();
    }

    // --- render helpers ---------------------------------------------------

    fn queue_card(
        &self,
        index: usize,
        q: &QueueRow,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Live paused set (bridge-fed) overlaid onto the row's optimistic flag: a
        // pause/resume driven from elsewhere shows here, and a just-clicked control
        // reads paused before its acknowledging event lands.
        let paused = q.paused || self.queue_health.read(cx).is_paused(q.id);
        let not_live = self.diverged.contains(&q.id);

        let border_color = if paused {
            with_alpha(palette.warning, 0.35)
        } else {
            palette.border_regular
        };

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .p(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(border_color)
            .bg(palette.elevated)
            .child(self.card_header(q, paused, not_live, palette, density))
            .child(self.card_metrics(q, paused, palette, density))
            .child(self.running_panel(q, paused, palette, density))
            .child(self.card_buttons(index, q, paused, palette, density, cx))
            .into_any_element()
    }

    fn card_header(
        &self,
        q: &QueueRow,
        paused: bool,
        not_live: bool,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        let name = div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_SM)
            .text_color(palette.text_primary)
            .child(q.name.clone());

        let mut name_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(name)
            .child(status_badge(paused, palette));
        if not_live {
            name_row = name_row.child(not_live_badge(palette));
        }

        let desc = div()
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FONT_XS)
            .text_color(palette.text_muted)
            .child(q.desc.clone());

        let left = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(name_row)
            .child(desc);

        div()
            .flex()
            .items_start()
            .justify_between()
            .child(left)
            .child(icon(Icon::DotsVertical, MENU_GLYPH, palette.text_faint))
            .into_any_element()
    }

    fn card_metrics(
        &self,
        q: &QueueRow,
        paused: bool,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
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
            "held"
        } else if q.in_flight > 0 {
            "in flight"
        } else {
            "idle"
        };

        let row = div()
            .flex()
            .flex_row()
            .child(metric_col(
                "CONCURRENCY",
                q.concurrency.to_string(),
                q.mode_label(),
                palette.text_primary,
                palette.text_faint,
                palette,
                density,
            ))
            .child(metric_col(
                "PENDING",
                q.pending.to_string(),
                pending_hint,
                pending_value_color,
                pending_hint_color,
                palette,
                density,
            ))
            .child(metric_col(
                "ACTIONS",
                q.actions.to_string(),
                "assigned",
                palette.text_primary,
                palette.text_faint,
                palette,
                density,
            ));

        div()
            .w_full()
            .pt(spacing(Spacing::Xs, density))
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
                "Resume",
                palette.shell,
                Some(palette.success),
                palette,
                density,
                cx.listener(move |this, _: &ClickEvent, _, cx| this.resume(id, cx)),
            )
        } else {
            card_button(
                ("q-pause", index),
                Icon::PlayerPause,
                "Pause",
                palette.warning,
                None,
                palette,
                density,
                cx.listener(move |this, _: &ClickEvent, _, cx| this.pause(id, cx)),
            )
        };

        let drain = card_button(
            ("q-drain", index),
            Icon::Eraser,
            "Drain",
            palette.text_secondary,
            None,
            palette,
            density,
            cx.listener(move |this, _: &ClickEvent, _, cx| this.drain(id, cx)),
        );

        let configure = card_button(
            ("q-configure", index),
            Icon::Settings,
            "Configure",
            palette.text_secondary,
            None,
            palette,
            density,
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
        let cards: Vec<AnyElement> = self
            .queues
            .iter()
            .enumerate()
            .map(|(index, q)| self.queue_card(index, q, palette, density, cx))
            .collect();

        let mut grid = div().w_full().flex().flex_col().gap(gap);
        let mut iter = cards.into_iter().peekable();
        while iter.peek().is_some() {
            let mut row = div().w_full().flex().flex_row().gap(gap);
            for _ in 0..CARDS_PER_ROW {
                match iter.next() {
                    Some(card) => row = row.child(div().flex_1().child(card)),
                    None => row = row.child(div().flex_1()),
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
            "Configure queue"
        } else {
            "New queue"
        };

        let blocking = modal_state.blocking;
        let name_field = field_label(palette, "NAME", div().child(modal_state.name_input.clone()));

        // BLOCKING (serial-execution) toggle: label + description over the kit switch.
        let toggle_label = div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child("Serial execution"),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child("Run one action at a time; later actions wait their turn"),
            );
        let blocking_row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .p(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.surface_overlay)
            .child(toggle_label)
            .child(toggle(blocking, palette).on_click(
                "q-modal-blocking",
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_blocking(cx)),
            ));

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(name_field)
            .child(blocking_row);

        let saveable = self.modal_saveable(cx);
        let save_label = if modal_state.editing.is_some() {
            "Save"
        } else {
            "Create"
        };
        let cancel = secondary_button("Cancel", palette).on_click(
            "q-modal-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.close_modal(cx)),
        );
        let save = primary_button(save_label, palette)
            .disabled(!saveable)
            .on_click(
                "q-modal-save",
                cx.listener(|this, _: &ClickEvent, _, cx| this.save(cx)),
            );
        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(cancel)
            .child(save);

        let card = modal(title, body, palette)
            .header_icon(Icon::Notebook, palette.brand)
            .subtitle("How actions run in this queue")
            .width(MODAL_WIDTH)
            .footer(footer)
            .kbd_hint("Enter to save · Esc to cancel")
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

impl Render for QueuesView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let pause_all = warning_ghost_button(
            "q-pause-all",
            Icon::PlayerPause,
            "Pause all",
            &palette,
            density,
            cx.listener(|this, _: &ClickEvent, _, cx| this.pause_all(cx)),
        );
        let new_queue = primary_button_with_icon(Icon::Plus, "New queue", &palette)
            .density(density)
            .on_click(
                "q-new",
                cx.listener(|this, _: &ClickEvent, window, cx| this.open_new(window, cx)),
            );
        let header_actions = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(pause_all)
            .child(new_queue);

        let header = breadcrumb(
            vec![
                BreadcrumbCrumb::leaf("Automation"),
                BreadcrumbCrumb::leaf("Queues"),
            ],
            &palette,
        )
        .right(header_actions);

        let feedback = self
            .feedback
            .clone()
            .map(|message| self.feedback_banner(message, &palette, density));

        let body = if self.queues.is_empty() {
            let caption = if self.loading {
                "Loading queues…"
            } else {
                "No queues configured."
            };
            div()
                .w_full()
                .py(spacing(Spacing::Lg, density))
                .child(
                    div()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .text_size(FONT_SM)
                        .text_color(palette.text_muted)
                        .child(caption),
                )
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
            .child(div().w_full().p(spacing(Spacing::Md, density)).child(body));

        let modal_overlay = self
            .modal
            .as_ref()
            .map(|modal_state| self.render_modal(modal_state, &palette, density, cx));

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(header)
            .children(feedback)
            .child(scroll)
            .children(modal_overlay)
    }
}

// ── view-specific fragments ───────────────────────────────────────────────

/// A running/paused status chip: a `border_regular`-filled pill with a leading
/// success dot (running) or warning pause mark (paused) and a mono caption inking
/// the matching hue. Distinct from the kit `badge` (no leading mark), so carried
/// locally — matching the parity source's status badge.
fn status_badge(paused: bool, palette: &ForgePalette) -> AnyElement {
    let (mark, label, ink): (AnyElement, &str, gpui::Rgba) = if paused {
        (
            icon(Icon::PlayerPause, BADGE_GLYPH, palette.warning).into_any_element(),
            "PAUSED",
            palette.warning,
        )
    } else {
        (
            div()
                .size(STATUS_DOT)
                .rounded(radius(Radius::Pill))
                .bg(palette.success)
                .into_any_element(),
            "RUNNING",
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

/// One column of the concurrency/pending/actions metric row: a mono caption, a
/// larger mono value, and a mono sub-hint.
#[allow(clippy::too_many_arguments)]
fn metric_col(
    caption: &'static str,
    value: String,
    hint: &'static str,
    value_color: gpui::Rgba,
    hint_color: gpui::Rgba,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, density))
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_secondary)
                .child(caption),
        )
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_SM)
                .text_color(value_color)
                .child(value),
        )
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XS)
                .text_color(hint_color)
                .child(hint),
        )
}

/// Idle running panel: a `shell`-filled strip with a dashed-circle mark and a muted
/// "No actions running" caption.
fn idle_panel(palette: &ForgePalette, density: Density) -> AnyElement {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, density))
        .py(spacing(Spacing::Xs, density))
        .px(spacing(Spacing::Sm, density))
        .rounded(radius(Radius::Sm))
        .bg(palette.shell)
        .child(icon(Icon::CircleDashed, PANEL_GLYPH, palette.text_faint))
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_faint)
                .child("No actions running"),
        )
        .into_any_element()
}

/// Serial running panel: a `shell`-filled strip with a spinner mark, the running
/// action's mono name, and a trailing muted "running —" caption.
fn serial_panel(q: &QueueRow, palette: &ForgePalette, density: Density) -> AnyElement {
    let name = q.running.first().cloned().unwrap_or_default();
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, density))
        .py(spacing(Spacing::Xs, density))
        .px(spacing(Spacing::Sm, density))
        .rounded(radius(Radius::Sm))
        .bg(palette.shell)
        .child(icon(Icon::Loader2, PANEL_GLYPH, palette.brand))
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
                .text_size(FONT_XS)
                .text_color(palette.text_faint)
                .child("running —"),
        )
        .into_any_element()
}

/// Concurrent running panel: a `shell`-filled block with a "RUNNING NOW" caption over
/// a wrapped row of action pills, capped at [`MAX_PILLS`] with a trailing "+N more".
fn concurrent_panel(q: &QueueRow, palette: &ForgePalette, density: Density) -> AnyElement {
    let shown = q.running.len().min(MAX_PILLS);
    let overflow = q.running.len().saturating_sub(MAX_PILLS);

    let mut pills = div().flex().flex_wrap().gap(spacing(Spacing::Xxs, density));
    for name in &q.running[..shown] {
        pills = pills.child(running_pill(name.clone(), palette));
    }
    if overflow > 0 {
        pills = pills.child(running_pill(format!("+{overflow} more"), palette));
    }

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, density))
        .py(spacing(Spacing::Xs, density))
        .px(spacing(Spacing::Sm, density))
        .rounded(radius(Radius::Sm))
        .bg(palette.shell)
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_faint)
                .child("RUNNING NOW"),
        )
        .child(pills)
        .into_any_element()
}

fn running_pill(label: impl Into<SharedString>, palette: &ForgePalette) -> impl IntoElement {
    div()
        .py(spacing(Spacing::Xxs, Density::Cozy))
        .px(spacing(Spacing::Xs, Density::Cozy))
        .rounded(PILL_RADIUS)
        .bg(palette.border_regular)
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_primary)
                .child(label.into()),
        )
}

/// Paused running panel: a warning-tinted strip with an alert mark and the
/// waiting/paused caption.
fn paused_panel(q: &QueueRow, palette: &ForgePalette, density: Density) -> AnyElement {
    let caption = match q.paused_since_min {
        Some(min) if min > 0 => {
            format!("{} actions waiting — paused {} min ago", q.pending, min)
        }
        _ => "Queue is paused".to_owned(),
    };
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, density))
        .py(spacing(Spacing::Xs, density))
        .px(spacing(Spacing::Sm, density))
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

/// One of a card's action buttons (pause/resume/drain/configure): an equal-flex,
/// centred row of a glyph + label inking `ink`. A `fill` renders a solid button
/// (resume); otherwise it is a bordered ghost.
#[allow(clippy::too_many_arguments)]
fn card_button(
    id: impl Into<gpui::ElementId>,
    glyph: Icon,
    label: &'static str,
    ink: gpui::Rgba,
    fill: Option<gpui::Rgba>,
    palette: &ForgePalette,
    density: Density,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let mut btn = div()
        .id(id.into())
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .gap(spacing(Spacing::Xxs, density))
        .py(spacing(Spacing::Xxs, density))
        .px(spacing(Spacing::Xs, density))
        .rounded(radius(Radius::Sm))
        .cursor_pointer()
        .on_click(handler)
        .child(icon(glyph, PANEL_GLYPH, ink))
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(ink)
                .child(label),
        );
    match fill {
        Some(bg) => btn = btn.bg(bg),
        None => {
            let hover = palette.elevated;
            btn = btn
                .border(BORDER_THIN)
                .border_color(palette.border_regular)
                .hover(move |s| s.bg(hover));
        }
    }
    btn.into_any_element()
}

/// The header "Pause all" action: a bordered ghost button inking `warning`. The kit
/// ghost button carries no ink override, so this warning-tinted variant is a local
/// fragment — matching the parity source's pause-all button.
fn warning_ghost_button(
    id: &'static str,
    glyph: Icon,
    label: &'static str,
    palette: &ForgePalette,
    density: Density,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let hover = palette.elevated;
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xxs, density))
        .py(spacing(Spacing::Xxs, density))
        .px(spacing(Spacing::Sm, density))
        .rounded(radius(Radius::Sm))
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .on_click(handler)
        .child(icon(glyph, HEADER_BTN_GLYPH, palette.warning))
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.warning)
                .child(label),
        )
}

/// Pulls the queue roster off storage and folds each persisted [`Queue`] into a
/// [`QueueRow`]: assigned-action counts come from the action repo (rows whose
/// `queue_id` matches), concurrency is derived from the blocking flag (not yet a
/// persisted column), the description is derived from the queue name, and the paused
/// flag is seeded from the scheduler's live paused set. The pending / in-flight /
/// running counters stay empty because no bus event attributes them per-queue. Runs off
/// the foreground thread on the tokio runtime.
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
            let concurrency = if q.blocking {
                SERIAL_CONCURRENCY
            } else {
                PARALLEL_CONCURRENCY
            };
            let paused = paused_ids.contains(&q.id);
            QueueRow {
                id: q.id,
                desc: default_description(&q.name),
                name: q.name,
                blocking: q.blocking,
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

/// The card description for a queue, derived from its name — the well-known queues carry
/// a fixed caption, any other queue renders without one. Mirrors the parity source's
/// name-keyed description lookup.
fn default_description(name: &str) -> String {
    match name {
        "Default" => "Catch-all queue for actions without explicit queue assignment",
        "Alerts" => "Subs, raids, cheers · serialized so overlays don't overlap",
        "Background" => "Logging, analytics, side-effect-free tasks · parallel execution",
        "Moderation" => "Auto-bans, timeouts, message deletions · paused for review",
        _ => "",
    }
    .to_owned()
}

/// The saved-but-not-live divergence chip: a warning-tinted, warning-bordered pill with
/// an alert mark and the "NOT LIVE · RESTART" caption. Ported from the parity source's
/// badge — a `warning`@0.12 fill, a `warning`@0.30 hairline border, an 8px radius, a 9px
/// alert glyph and a mono caption.
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
                .child("NOT LIVE · RESTART"),
        )
        .into_any_element()
}
