use std::collections::HashSet;
use std::sync::Arc;

use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_SM,
    FONT_XS, FONT_XXS, ForgePalette, Icon, InputEvent, OverlayPosition, Radius, Spacing, TextInput,
    breadcrumb, icon, modal, overlay, primary_button, primary_button_with_icon, radius,
    secondary_button, spacing, toggle, tr, with_alpha,
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

const BADGE_RADIUS: Pixels = px(8.0);
const STATUS_DOT: Pixels = px(5.0);
const PILL_RADIUS: Pixels = px(5.0);
const BADGE_GLYPH: Pixels = px(9.0);
const PANEL_GLYPH: Pixels = px(12.0);
const MENU_GLYPH: Pixels = px(14.0);
const HEADER_BTN_GLYPH: Pixels = px(13.0);
const CARDS_PER_ROW: usize = 2;
const MAX_PILLS: usize = 3;
const SERIAL_CONCURRENCY: u32 = 1;
const PARALLEL_CONCURRENCY: u32 = 8;
const MODAL_WIDTH: Pixels = px(440.0);

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
    name_input: Entity<TextInput>,
    blocking: bool,
    saving: bool,
    _name_sub: Subscription,
}

pub struct QueuesView {
    queues: Vec<QueueRow>,
    loading: bool,
    feedback: Option<SharedString>,
    modal: Option<EditQueueModal>,
    diverged: HashSet<QueueId>,
    queue_health: Entity<QueueHealth>,
    scheduler: QueueSchedulerHandle,
    bus: Arc<EventBus>,
    queue_repo: Arc<dyn QueueRepo>,
    action_repo: Arc<dyn ActionRepo>,
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

    fn drain(&mut self, id: QueueId, cx: &mut Context<Self>) {
        if let Some(q) = self.queues.iter_mut().find(|q| q.id == id) {
            q.paused = true;
            q.paused_since_min = Some(0);
            self.feedback = Some(tr!("queues_drain_feedback", name = q.name.as_str()).into());
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

    fn open_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let modal = Self::build_modal(None, "", false, cx);
        modal.name_input.update(cx, |f, cx| f.focus(window, cx));
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
        modal.name_input.update(cx, |f, cx| f.focus(window, cx));
        self.modal = Some(modal);
        cx.notify();
    }

    fn build_modal(
        editing: Option<QueueId>,
        name_seed: &str,
        blocking: bool,
        cx: &mut Context<Self>,
    ) -> EditQueueModal {
        let palette = cx.palette();
        let name_seed = name_seed.to_owned();
        let name_input = cx.new(|cx| {
            let mut ti =
                TextInput::new(tr!("queues_create_name_placeholder"), cx).with_palette(palette);
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

    fn modal_saveable(&self, cx: &Context<Self>) -> bool {
        self.modal.as_ref().is_some_and(|modal| {
            !modal.saving && !modal.name_input.read(cx).content().trim().is_empty()
        })
    }

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
                density,
            ))
            .child(metric_col(
                SharedString::from(tr!("queues_metric_pending")),
                q.pending.to_string(),
                pending_hint,
                pending_value_color,
                pending_hint_color,
                palette,
                density,
            ))
            .child(metric_col(
                SharedString::from(tr!("queues_metric_actions")),
                q.actions.to_string(),
                SharedString::from(tr!("queues_metric_assigned")),
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
                SharedString::from(tr!("queues_resume_btn")),
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
                SharedString::from(tr!("queues_pause_btn")),
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
            SharedString::from(tr!("queues_drain_btn")),
            palette.text_secondary,
            None,
            palette,
            density,
            cx.listener(move |this, _: &ClickEvent, _, cx| this.drain(id, cx)),
        );

        let configure = card_button(
            ("q-configure", index),
            Icon::Settings,
            SharedString::from(tr!("queues_configure_btn")),
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
            tr!("queues_edit_title")
        } else {
            tr!("queues_create_title")
        };

        let blocking = modal_state.blocking;
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
                    .child(SharedString::from(tr!("queues_create_blocking_label"))),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(SharedString::from(tr!("queues_create_blocking_desc"))),
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
            tr!("common_save")
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
        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(cancel)
            .child(save);

        let card = modal(title, body, palette)
            .header_icon(Icon::Notebook, palette.brand)
            .subtitle(tr!("queues_create_subtitle"))
            .width(MODAL_WIDTH)
            .footer(footer)
            .kbd_hint(tr!("queues_create_kbd_hint"))
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
            SharedString::from(tr!("queues_pause_all_btn")),
            &palette,
            density,
            cx.listener(|this, _: &ClickEvent, _, cx| this.pause_all(cx)),
        );
        let new_queue = primary_button_with_icon(Icon::Plus, tr!("queues_new_queue_btn"), &palette)
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
                BreadcrumbCrumb::leaf(tr!("queues_breadcrumb_automation")),
                BreadcrumbCrumb::leaf(tr!("queues_breadcrumb_queues")),
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
                SharedString::from(tr!("queues_loading"))
            } else {
                SharedString::from(tr!("queues_empty"))
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

fn status_badge(paused: bool, palette: &ForgePalette) -> AnyElement {
    let (mark, label, ink): (AnyElement, SharedString, gpui::Rgba) = if paused {
        (
            icon(Icon::PlayerPause, BADGE_GLYPH, palette.warning).into_any_element(),
            SharedString::from(tr!("queues_status_paused")),
            palette.warning,
        )
    } else {
        (
            div()
                .size(STATUS_DOT)
                .rounded(radius(Radius::Pill))
                .bg(palette.success)
                .into_any_element(),
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

#[allow(clippy::too_many_arguments)]
fn metric_col(
    caption: SharedString,
    value: String,
    hint: SharedString,
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
                .child(SharedString::from(tr!("queues_no_actions_running"))),
        )
        .into_any_element()
}

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
        .rounded(radius(Radius::Sm))
        .bg(palette.shell)
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_faint)
                .child(SharedString::from(tr!("queues_running_now_header"))),
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

#[allow(clippy::too_many_arguments)]
fn card_button(
    id: impl Into<gpui::ElementId>,
    glyph: Icon,
    label: SharedString,
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

fn warning_ghost_button(
    id: &'static str,
    glyph: Icon,
    label: SharedString,
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

fn default_description(name: &str) -> String {
    match name {
        "Default" => tr!("queues_desc_default"),
        "Alerts" => tr!("queues_desc_alerts"),
        "Background" => tr!("queues_desc_background"),
        "Moderation" => tr!("queues_desc_moderation"),
        _ => String::new(),
    }
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
