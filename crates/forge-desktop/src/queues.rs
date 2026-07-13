use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_SM,
    FONT_XS, ForgePalette, Icon, InputEvent, OverlayPosition, Radius, Spacing, TextInput,
    breadcrumb, field_label, icon, modal, overlay, primary_button, primary_button_with_icon,
    radius, secondary_button, spacing, toggle, with_alpha,
};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, Pixels, SharedString, Subscription, Window, div,
    prelude::*, px,
};

use crate::presentation::ActivePresentation;

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

/// One action queue as the screen caches it. A stub view-model standing in for
/// `forge-runtime`'s live queue slot plus its storage row: `concurrency`,
/// `blocking`, `actions`, `desc` are the persisted shape, while `paused`,
/// `pending`, `in_flight`, `running` and `paused_since_min` are the live counters
/// the scheduler feeds. `forge-desktop` wires no scheduler yet, so all of these are
/// seeded static; the real screen reads them over the runtime→UI bridge (a
/// `QueueScheduler` health topic) and drives pause/resume/drain/save through the
/// scheduler handle, never owning them authoritatively.
struct QueueRow {
    id: u64,
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
    editing: Option<u64>,
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
/// Owns its queue list as seeded stub state (no queue scheduler is wired into
/// `forge-desktop` yet); pause/resume/drain/save mutate that list and surface a
/// feedback banner. The real screen drives the queue lifecycle through
/// `forge-runtime`'s `QueueScheduler` via its handle, reading the live counters back
/// over the runtime→UI bridge.
pub struct QueuesView {
    queues: Vec<QueueRow>,
    next_id: u64,
    feedback: Option<SharedString>,
    modal: Option<EditQueueModal>,
}

impl QueuesView {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        let queues = seed_queues();
        let next_id = queues.iter().map(|q| q.id).max().map_or(0, |m| m + 1);
        Self {
            queues,
            next_id,
            feedback: None,
            modal: None,
        }
    }

    // --- queue actions (view-state stubs) ---------------------------------

    fn pause(&mut self, id: u64, cx: &mut Context<Self>) {
        if let Some(q) = self.queues.iter_mut().find(|q| q.id == id) {
            q.paused = true;
            q.paused_since_min = Some(0);
        }
        cx.notify();
    }

    fn resume(&mut self, id: u64, cx: &mut Context<Self>) {
        if let Some(q) = self.queues.iter_mut().find(|q| q.id == id) {
            q.paused = false;
            q.paused_since_min = None;
        }
        cx.notify();
    }

    /// Drains a queue: the parity source pauses the slot and publishes a drain
    /// request on the bus. Here it pauses the cached row and notes the request; the
    /// real drain publishes `queue.drain_requested` through the runtime handle.
    fn drain(&mut self, id: u64, cx: &mut Context<Self>) {
        if let Some(q) = self.queues.iter_mut().find(|q| q.id == id) {
            q.paused = true;
            q.paused_since_min = Some(0);
            self.feedback = Some(
                format!(
                    "Draining “{}”. Live draining is wired via the runtime soon.",
                    q.name
                )
                .into(),
            );
        }
        cx.notify();
    }

    fn pause_all(&mut self, cx: &mut Context<Self>) {
        for q in &mut self.queues {
            if !q.paused {
                q.paused = true;
                q.paused_since_min = Some(0);
            }
        }
        cx.notify();
    }

    // --- modal lifecycle --------------------------------------------------

    fn open_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let modal = Self::build_modal(None, "", false, cx);
        modal.name_input.read(cx).focus(window);
        self.modal = Some(modal);
        cx.notify();
    }

    fn open_configure(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
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
        editing: Option<u64>,
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

    fn save(&mut self, cx: &mut Context<Self>) {
        if !self.modal_saveable(cx) {
            return;
        }
        let Some(modal) = self.modal.as_ref() else {
            return;
        };
        let name = modal.name_input.read(cx).content().trim().to_owned();
        let blocking = modal.blocking;
        let editing = modal.editing;
        let concurrency = if blocking {
            SERIAL_CONCURRENCY
        } else {
            PARALLEL_CONCURRENCY
        };

        match editing {
            Some(id) => {
                if let Some(q) = self.queues.iter_mut().find(|q| q.id == id) {
                    q.name = name.clone();
                    q.blocking = blocking;
                    q.concurrency = concurrency;
                }
            }
            None => {
                let id = self.next_id;
                self.next_id += 1;
                self.queues.push(QueueRow {
                    id,
                    name: name.clone(),
                    desc: String::new(),
                    blocking,
                    concurrency,
                    paused: false,
                    pending: 0,
                    in_flight: 0,
                    actions: 0,
                    running: vec![],
                    paused_since_min: None,
                });
            }
        }

        self.modal = None;
        self.feedback =
            Some(format!("Saved “{name}”. Queue scheduling is wired via the runtime soon.").into());
        cx.notify();
    }

    // --- render helpers ---------------------------------------------------

    fn queue_card(
        &self,
        q: &QueueRow,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let border_color = if q.paused {
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
            .child(self.card_header(q, palette, density))
            .child(self.card_metrics(q, palette, density))
            .child(self.running_panel(q, palette, density))
            .child(self.card_buttons(q, palette, density, cx))
            .into_any_element()
    }

    fn card_header(&self, q: &QueueRow, palette: &ForgePalette, density: Density) -> AnyElement {
        let name = div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_SM)
            .text_color(palette.text_primary)
            .child(q.name.clone());

        let name_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(name)
            .child(status_badge(q.paused, palette));

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

    fn card_metrics(&self, q: &QueueRow, palette: &ForgePalette, density: Density) -> AnyElement {
        let pending_value_color = if q.paused {
            palette.warning
        } else {
            palette.text_primary
        };
        let pending_hint_color = if q.paused {
            palette.warning
        } else {
            palette.text_faint
        };
        let pending_hint = if q.paused {
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

    fn running_panel(&self, q: &QueueRow, palette: &ForgePalette, density: Density) -> AnyElement {
        if q.paused {
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
        q: &QueueRow,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = q.id;

        let action = if q.paused {
            card_button(
                ("q-resume", id as usize),
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
                ("q-pause", id as usize),
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
            ("q-drain", id as usize),
            Icon::Eraser,
            "Drain",
            palette.text_secondary,
            None,
            palette,
            density,
            cx.listener(move |this, _: &ClickEvent, _, cx| this.drain(id, cx)),
        );

        let configure = card_button(
            ("q-configure", id as usize),
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
            .map(|q| self.queue_card(q, palette, density, cx))
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
            div()
                .w_full()
                .py(spacing(Spacing::Lg, density))
                .child(
                    div()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .text_size(FONT_SM)
                        .text_color(palette.text_muted)
                        .child("No queues configured."),
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

/// The representative queue roster the screen seeds before a queue scheduler is
/// wired — names, descriptions and states mirror the design's queue roster so every
/// running-panel variant (serial, idle, concurrent, paused) renders. The live
/// counters (pending/in-flight/running/paused) are runtime-fed; here they are static.
fn seed_queues() -> Vec<QueueRow> {
    vec![
        QueueRow {
            id: 0,
            name: "Default".to_owned(),
            desc: "Catch-all queue for actions without explicit queue assignment".to_owned(),
            blocking: true,
            concurrency: 1,
            paused: false,
            pending: 3,
            in_flight: 1,
            actions: 11,
            running: vec!["!quote".to_owned()],
            paused_since_min: None,
        },
        QueueRow {
            id: 1,
            name: "Alerts".to_owned(),
            desc: "Subs, raids, cheers · serialized so overlays don't overlap".to_owned(),
            blocking: true,
            concurrency: 1,
            paused: false,
            pending: 0,
            in_flight: 0,
            actions: 3,
            running: vec![],
            paused_since_min: None,
        },
        QueueRow {
            id: 2,
            name: "Background".to_owned(),
            desc: "Logging, analytics, side-effect-free tasks · parallel execution".to_owned(),
            blocking: false,
            concurrency: 8,
            paused: false,
            pending: 12,
            in_flight: 4,
            actions: 6,
            running: vec![
                "log_chat".to_owned(),
                "analytics_tick".to_owned(),
                "cache_warm".to_owned(),
                "cron_sweep".to_owned(),
            ],
            paused_since_min: None,
        },
        QueueRow {
            id: 3,
            name: "Moderation".to_owned(),
            desc: "Auto-bans, timeouts, message deletions · paused for review".to_owned(),
            blocking: false,
            concurrency: 2,
            paused: true,
            pending: 7,
            in_flight: 0,
            actions: 4,
            running: vec![],
            paused_since_min: Some(14),
        },
        QueueRow {
            id: 4,
            name: "TTS".to_owned(),
            desc: "Text-to-speech queue, drained continuously while audio plays".to_owned(),
            blocking: true,
            concurrency: 1,
            paused: false,
            pending: 5,
            in_flight: 1,
            actions: 2,
            running: vec!["tts_speak".to_owned()],
            paused_since_min: None,
        },
    ]
}
