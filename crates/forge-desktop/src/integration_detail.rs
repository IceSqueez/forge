use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, ConfirmTone, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density,
    FONT_LG, FONT_SM, FONT_XS, ForgePalette, Icon, OverlayPosition, Picker, PickerEvent,
    PickerItem, PickerLabels, Radius, Spacing, breadcrumb, confirm_modal, icon, overlay, radius,
    spacing, with_alpha,
};
use forge_obs::{ObsClient, ObsSource};
use forge_platform_core::{
    BuiltinContent, BuiltinControl, BuiltinHealth, BuiltinStatus, CapabilityFlags, ConnectionState,
    DetailSection, HeaderAction, HealthDelta, HealthMetric, PickerKind, QuickAction, QuickActions,
    SectionIcon,
};
use forge_runtime::ActionEngineHandle;
use forge_types::Variant;
use futures_util::StreamExt as _;
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, FontWeight, Rgba, Subscription, Window,
    div, prelude::*, px,
};
use std::sync::Arc;
use std::time::Duration;

use crate::builtin_sections::{content_sections, format_uptime, health_grid};
use crate::platforms::PlatformConnectivity;
use crate::presentation::ActivePresentation;
use crate::screen::Screen;
use crate::sidebar::NavRequested;

/// The single generic integration detail screen. It consumes the four `Builtin*`
/// trait outputs — status, health metrics, content sections, quick actions — and
/// renders them uniformly, so no integration has any per-screen detail code: a
/// new integration reaches this view by supplying the four traits, nothing here
/// changes. It never switches on the integration id when rendering.
///
/// The view holds the live trait objects and a cached snapshot read from them: the
/// snapshot is read synchronously on mount and re-read whenever the observed
/// connectivity topic advances (a `platform.connection.changed` fold), so the header,
/// alt-state, health, content and quick actions track the real `ConnectionState`.
pub struct IntegrationDetail {
    // Live trait surface, held so the snapshot can be re-read on a connection change.
    status: Arc<dyn BuiltinStatus>,
    health: Arc<dyn BuiltinHealth>,
    content: Arc<dyn BuiltinContent>,
    quick: Arc<dyn QuickActions>,
    // The lifecycle-verb handle (reconnect / disconnect / refresh-token). `None`
    // when this integration exposes no control surface (seed fallback / no
    // credentials); a lifecycle action is then a silent no-op, matching the header
    // buttons that still render but do nothing.
    control: Option<Arc<dyn BuiltinControl>>,
    // The tokio runtime handle onto which a control verb is spawned: the verb does
    // real network I/O, so it must run with a tokio reactor rather than on gpui's
    // foreground executor.
    rt_handle: tokio::runtime::Handle,
    // The action-engine write edge. A quick action carries a pre-filled SubAction
    // template that is dispatched through this handle — the SAME path a real
    // trigger-driven SubAction takes — so a quick action is never a side channel.
    action_engine: ActionEngineHandle,
    // The concrete OBS client, present only for the OBS integration. A picker quick
    // action reaches through it to enumerate scenes / sources / audio inputs for the
    // target list; `None` for every other integration (and the seed fallback), where
    // the OBS-only picker actions never appear.
    obs_source: Option<Arc<ObsClient>>,
    icon: SectionIcon,
    display_name: String,
    version: Option<String>,
    endpoint: Option<String>,
    uptime: Option<Duration>,
    connection: ConnectionState,
    capability_flags: CapabilityFlags,
    header_actions: Vec<HeaderAction>,
    health_metrics: [HealthMetric; 4],
    sections: Vec<DetailSection>,
    quick_actions: Vec<QuickAction>,
    /// Two-phase disconnect gate: armed by the header Disconnect action, rendered
    /// by the shared confirm modal. `false` = no confirm showing.
    pending_disconnect: bool,
    /// The open picker quick action awaiting a target: the searchable picker entity,
    /// which action it will complete, the target kind, and (for a source pick) the
    /// scene the sources were read from. `None` = no picker showing.
    pending_picker: Option<PendingPicker>,
    /// Transient feedback line for a dispatched lifecycle/quick action. Without a
    /// live runtime the action is stubbed and only this toast is shown.
    toast: Option<String>,
    /// Held so the connectivity observation lives for the view's lifetime.
    _conn_obs: Subscription,
}

impl EventEmitter<NavRequested> for IntegrationDetail {}

impl IntegrationDetail {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        icon: SectionIcon,
        status: Arc<dyn BuiltinStatus>,
        health: Arc<dyn BuiltinHealth>,
        content: Arc<dyn BuiltinContent>,
        quick: Arc<dyn QuickActions>,
        control: Option<Arc<dyn BuiltinControl>>,
        obs_source: Option<Arc<ObsClient>>,
        rt_handle: tokio::runtime::Handle,
        action_engine: ActionEngineHandle,
        connectivity: Entity<PlatformConnectivity>,
        cx: &mut Context<Self>,
    ) -> Self {
        // The connectivity fold advances on a `platform.connection.changed`; re-read
        // this integration's live snapshot from its trait objects whenever it does.
        let conn_obs = cx.observe(&connectivity, |this, _, cx| this.reload(cx));

        let display_name = status.display_name().to_owned();
        let version = status.version().map(ToOwned::to_owned);
        let endpoint = status.endpoint().map(ToOwned::to_owned);
        let uptime = status.uptime();
        let connection = status.connection();
        let capability_flags = status.capability_flags();
        let header_actions = status.header_actions();
        let health_metrics = health.metrics();
        let sections = content.sections();
        let quick_actions = quick.actions();

        // View-scoped live health drain: seeded synchronously above from
        // `metrics()`, then this per-instance `stream()` folds each delta into the
        // grid. The task is tied to this view's lifetime — once the user navigates
        // away and the entity is released, `this.update` returns `Err` and the loop
        // ends. It is deliberately NOT a boot-global drain: a lagging health stream
        // must never stall the shared runtime→UI bridge topics.
        let mut health_stream = health.stream();
        cx.spawn(async move |this, cx| {
            while let Some(delta) = health_stream.next().await {
                if this
                    .update(cx, |detail, cx| detail.apply_health_delta(delta, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();

        Self {
            status,
            health,
            content,
            quick,
            control,
            rt_handle,
            action_engine,
            obs_source,
            icon,
            display_name,
            version,
            endpoint,
            uptime,
            connection,
            capability_flags,
            header_actions,
            health_metrics,
            sections,
            quick_actions,
            pending_disconnect: false,
            pending_picker: None,
            toast: None,
            _conn_obs: conn_obs,
        }
    }

    /// Re-reads the cached snapshot from the live trait objects and repaints. Called
    /// when the connectivity topic advances so the header, alt-state, health, content
    /// and quick actions reflect the integration's current `ConnectionState`.
    fn reload(&mut self, cx: &mut Context<Self>) {
        self.display_name = self.status.display_name().to_owned();
        self.version = self.status.version().map(ToOwned::to_owned);
        self.endpoint = self.status.endpoint().map(ToOwned::to_owned);
        self.uptime = self.status.uptime();
        self.connection = self.status.connection();
        self.capability_flags = self.status.capability_flags();
        self.header_actions = self.status.header_actions();
        self.health_metrics = self.health.metrics();
        self.sections = self.content.sections();
        self.quick_actions = self.quick.actions();
        cx.notify();
    }

    /// Folds a single live health delta into the cached 4-metric grid and
    /// repaints. The grid is fixed at four cells, so an out-of-range index is
    /// ignored (no repaint). Driven by the view-scoped health drain started on
    /// mount.
    fn apply_health_delta(&mut self, delta: HealthDelta, cx: &mut Context<Self>) {
        let idx = delta.index as usize;
        if idx < self.health_metrics.len() {
            self.health_metrics[idx].value = delta.new_value;
            cx.notify();
        }
    }

    fn go_back(&mut self, cx: &mut Context<Self>) {
        cx.emit(NavRequested(Screen::Platforms));
    }

    fn on_header_action(&mut self, action: HeaderAction, cx: &mut Context<Self>) {
        match action {
            // Destructive: arm the two-phase confirm gate; the verb itself fires
            // only once the modal is accepted (see `confirm_disconnect`).
            HeaderAction::Disconnect => {
                self.pending_disconnect = true;
                cx.notify();
            }
            HeaderAction::Reconnect => self.dispatch_control(ControlVerb::Reconnect),
            HeaderAction::RefreshToken => self.dispatch_control(ControlVerb::RefreshToken),
            HeaderAction::Settings => {
                self.toast = Some("Settings coming soon".to_owned());
                cx.notify();
            }
        }
    }

    /// Spawns a lifecycle verb onto the tokio runtime. With no `control` surface the
    /// dispatch is a silent no-op (the header button still renders but does nothing),
    /// matching how the integration presents an absent control. The resulting steady
    /// connection state is not returned here: it is observed through the
    /// `platform.connection.changed` bridge, which advances the connectivity topic and
    /// triggers `reload`. A rejected verb is logged with the trait's coarse,
    /// PII-safe reason and never surfaced as transport detail.
    fn dispatch_control(&self, verb: ControlVerb) {
        let Some(ctrl) = self.control.clone() else {
            return;
        };
        self.rt_handle.spawn(async move {
            let outcome = match verb {
                ControlVerb::Reconnect => ctrl.reconnect().await,
                ControlVerb::Disconnect => ctrl.disconnect().await,
                ControlVerb::RefreshToken => ctrl.refresh_token().await,
            };
            if let Err(failure) = outcome {
                eprintln!("forge-desktop: integration control action failed: {failure}");
            }
        });
    }

    fn cancel_disconnect(&mut self, cx: &mut Context<Self>) {
        self.pending_disconnect = false;
        cx.notify();
    }

    fn confirm_disconnect(&mut self, cx: &mut Context<Self>) {
        self.pending_disconnect = false;
        self.dispatch_control(ControlVerb::Disconnect);
        cx.notify();
    }

    fn on_quick_action(&mut self, idx: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(action) = self.quick_actions.get(idx) else {
            return;
        };
        if !action.enabled {
            return;
        }
        // A picker action (OBS scene/source/input) needs a target chosen before its
        // template is complete; firing the bare template would dispatch a switch with
        // no scene/source. Open the picker, load the target list from OBS, and defer the
        // dispatch until the user selects one (see `pick_target`).
        if let Some(kind) = action.picker {
            self.open_picker(idx, kind, window, cx);
            return;
        }
        // Non-picker action: dispatch the pre-filled SubAction template through the
        // action engine — the SAME path a trigger-driven SubAction takes, never a
        // side channel. Real dispatch needs a tokio reactor, so it is spawned onto
        // the runtime handle fire-and-forget; a rejected dispatch is logged
        // PII-safely and the outcome is otherwise observed through the bus.
        let step = action.subaction_template.clone();
        let builtin_id = self.status.id().as_str().to_owned();
        let label = action.label.clone();
        let engine = self.action_engine.clone();
        self.rt_handle.spawn(async move {
            if let Err(failure) = engine.execute_quick_action(step, builtin_id, label).await {
                eprintln!("forge-desktop: quick action dispatch failed: {failure}");
            }
        });
    }

    /// Opens the searchable target picker for a picker quick action and starts the
    /// async scene/source/input fetch off the OBS client. The picker shows its loading
    /// placeholder until [`Self::apply_picker_items`] folds the fetched rows in. With no
    /// OBS client (seed fallback / disconnected) the load resolves immediately to an
    /// empty list, so the picker opens showing "no matches" rather than nothing.
    fn open_picker(
        &mut self,
        action_index: usize,
        kind: PickerKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let palette = cx.palette();
        let picker = cx.new(|cx| {
            let mut picker = Picker::new(picker_labels(kind), Vec::new(), palette, cx);
            picker.set_loading(true, cx);
            picker
        });
        let sub = cx.subscribe(&picker, Self::on_picker_event);
        picker.read(cx).focus(window, cx);

        match self.obs_source.clone() {
            Some(client) => self.spawn_picker_fetch(picker.clone(), client, kind, cx),
            // No client to enumerate targets: settle into the empty state at once.
            None => picker.update(cx, |picker, cx| picker.set_loading(false, cx)),
        }

        self.pending_picker = Some(PendingPicker {
            picker,
            action_index,
            kind,
            current_scene: None,
            _sub: sub,
        });
        cx.notify();
    }

    /// Runs the OBS scene/source/input enumeration on the tokio runtime (it awaits real
    /// WebSocket I/O and so cannot run on gpui's foreground executor) and hops the result
    /// back onto the view through a oneshot channel, applying it via
    /// [`Self::apply_picker_items`].
    fn spawn_picker_fetch(
        &self,
        picker: Entity<Picker>,
        client: Arc<ObsClient>,
        kind: PickerKind,
        cx: &mut Context<Self>,
    ) {
        let (tx, rx) =
            tokio::sync::oneshot::channel::<Result<(Vec<PickerItem>, Option<String>), String>>();
        self.rt_handle.spawn(async move {
            let _ = tx.send(fetch_picker_items(client, kind).await);
        });
        cx.spawn(async move |this, cx| {
            let Ok(result) = rx.await else {
                return;
            };
            let _ = this.update(cx, |detail, cx| {
                detail.apply_picker_items(&picker, result, cx)
            });
        })
        .detach();
    }

    /// Folds a resolved target fetch into the open picker: on success it loads the rows
    /// and records the scene the sources were read from (needed to complete a source
    /// pick); on failure it clears the loading placeholder, leaving the picker in its
    /// empty state. A stale result (the picker was cancelled meanwhile) is ignored.
    fn apply_picker_items(
        &mut self,
        picker: &Entity<Picker>,
        result: Result<(Vec<PickerItem>, Option<String>), String>,
        cx: &mut Context<Self>,
    ) {
        if self
            .pending_picker
            .as_ref()
            .is_none_or(|pending| pending.picker != *picker)
        {
            return;
        }
        match result {
            Ok((items, current_scene)) => {
                if let Some(pending) = self.pending_picker.as_mut() {
                    pending.current_scene = current_scene;
                }
                picker.update(cx, |picker, cx| {
                    picker.set_items(items, cx);
                    picker.set_loading(false, cx);
                });
            }
            Err(reason) => {
                eprintln!("forge-desktop: obs picker load failed: {reason}");
                picker.update(cx, |picker, cx| picker.set_loading(false, cx));
            }
        }
        cx.notify();
    }

    fn on_picker_event(
        &mut self,
        _picker: Entity<Picker>,
        event: &PickerEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            PickerEvent::Selected(id) => self.pick_target(id.to_string(), cx),
            PickerEvent::Cancelled => self.cancel_picker(cx),
        }
    }

    fn cancel_picker(&mut self, cx: &mut Context<Self>) {
        self.pending_picker = None;
        cx.notify();
    }

    /// Completes the pending picker quick action with the chosen target: injects it into
    /// the pre-filled SubAction template's config (scene → `scene`; source → `source`,
    /// plus the enclosing `scene`; input → `source`) and dispatches through the action
    /// engine — the SAME path [`Self::on_quick_action`] takes for a non-picker action.
    fn pick_target(&mut self, selected_id: String, cx: &mut Context<Self>) {
        let Some(pending) = self.pending_picker.take() else {
            return;
        };
        let Some(action) = self.quick_actions.get(pending.action_index) else {
            cx.notify();
            return;
        };
        let mut step = action.subaction_template.clone();
        let label = action.label.clone();
        let builtin_id = self.status.id().as_str().to_owned();

        match pending.kind {
            PickerKind::Scene => {
                step.config
                    .insert("scene".to_owned(), Variant::String(selected_id));
            }
            PickerKind::Source => {
                if let Some(scene) = pending.current_scene {
                    step.config
                        .insert("scene".to_owned(), Variant::String(scene));
                }
                step.config
                    .insert("source".to_owned(), Variant::String(selected_id));
            }
            PickerKind::Input => {
                step.config
                    .insert("source".to_owned(), Variant::String(selected_id));
            }
            PickerKind::Hotkey | PickerKind::Expression | PickerKind::MidiPort => {
                cx.notify();
                return;
            }
        }

        let engine = self.action_engine.clone();
        self.rt_handle.spawn(async move {
            if let Err(failure) = engine.execute_quick_action(step, builtin_id, label).await {
                eprintln!("forge-desktop: quick action dispatch failed: {failure}");
            }
        });
        cx.notify();
    }

    fn dismiss_toast(&mut self, cx: &mut Context<Self>) {
        self.toast = None;
        cx.notify();
    }

    fn header_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (letter, brand) = hero_identity(self.icon.as_str(), &self.display_name, palette);

        let tile = div()
            .flex_none()
            .size(px(48.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(11.0))
            .bg(brand)
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(24.0))
                    .text_color(palette.shell)
                    .child(letter),
            );

        let mut name_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_LG)
                    .text_color(palette.text_primary)
                    .child(self.display_name.clone()),
            );
        if let Some(version) = &self.version {
            name_row = name_row.child(pill(version.clone(), palette.text_muted, palette));
        }
        if self.capability_flags.limited {
            let label = self
                .capability_flags
                .label
                .clone()
                .unwrap_or_else(|| "Limited".to_owned());
            name_row = name_row.child(pill(label.to_uppercase(), palette.warning, palette));
        }

        let sub = sub_line(self.endpoint.as_deref(), self.uptime);
        let info = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(name_row)
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_muted)
                    .child(sub),
            );

        let mut actions = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density));
        for (i, action) in self.header_actions.iter().enumerate() {
            actions = actions.child(self.action_button(i, action.clone(), palette, density, cx));
        }

        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Md, density))
            .py(spacing(Spacing::Md, density))
            .px(spacing(Spacing::Md, density))
            .rounded(radius(Radius::Lg))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.elevated)
            .child(tile)
            .child(info)
            .child(actions)
            .into_any_element()
    }

    fn action_button(
        &self,
        idx: usize,
        action: HeaderAction,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = header_action_label(&action);
        let text_color = match action {
            HeaderAction::Disconnect => palette.random,
            _ => palette.text_secondary,
        };
        let hover_bg = with_alpha(palette.border_regular, 0.06);
        div()
            .id(("header-action", idx))
            .flex()
            .items_center()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Md, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .cursor_pointer()
            .hover(move |s| s.bg(hover_bg))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.on_header_action(action.clone(), cx)
            }))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(text_color)
                    .child(label),
            )
            .into_any_element()
    }

    /// The quick-actions card: a bolt-led header over a divider and a row of up to
    /// four accent-tinted action buttons. Disabled actions dim and show an `N/A`
    /// trailing marker; enabled ones dispatch through [`Self::on_quick_action`].
    fn quick_actions_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let header = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Md, density))
            .child(icon(Icon::Bolt, FONT_SM, palette.warning))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child("Quick actions"),
            );

        let divider = div().w_full().h(BORDER_THIN).bg(palette.border_regular);

        let mut btn_row = div()
            .w_full()
            .flex()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Md, density));
        let capped = self.quick_actions.len().min(4);
        for i in 0..capped {
            let action = &self.quick_actions[i];
            btn_row = btn_row.child(self.quick_action_button(
                i,
                action,
                quick_action_accent(i, palette),
                palette,
                density,
                cx,
            ));
        }
        for _ in capped..4 {
            btn_row = btn_row.child(div().flex_1());
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.elevated)
            .child(header)
            .child(divider)
            .child(btn_row)
            .into_any_element()
    }

    fn quick_action_button(
        &self,
        idx: usize,
        action: &QuickAction,
        accent: Rgba,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let enabled = action.enabled;
        let (icon_color, label_color, bg_color, border_color) = if enabled {
            (
                accent,
                palette.text_primary,
                palette.shell,
                palette.border_regular,
            )
        } else {
            (
                with_alpha(palette.text_faint, 0.5),
                with_alpha(palette.text_faint, 0.5),
                with_alpha(palette.shell, 0.5),
                with_alpha(palette.border_regular, 0.5),
            )
        };

        let mut content = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(icon(
                Icon::from_name(action.icon.as_str()),
                FONT_SM,
                icon_color,
            ))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(label_color)
                    .child(action.label.clone()),
            );
        if !enabled {
            content = content.child(div().flex_1()).child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(with_alpha(palette.text_faint, 0.5))
                    .child("N/A"),
            );
        }

        let mut btn = div()
            .id(("quick-action", idx))
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(border_color)
            .bg(bg_color)
            .child(content);
        if enabled {
            let hover_bg = with_alpha(bg_color, (bg_color.a + 0.06).min(1.0));
            btn = btn
                .cursor_pointer()
                .hover(move |s| s.bg(hover_bg))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.on_quick_action(idx, window, cx)
                }));
        }
        btn.into_any_element()
    }

    fn disconnect_overlay(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let card = confirm_modal(
            "Disconnect integration",
            "Chats and events from this integration stop until you reconnect.",
            ConfirmTone::Warning,
            palette,
        )
        .item_name(self.display_name.clone())
        .esc_hint("to cancel")
        .on_cancel(
            "integration-disconnect-cancel",
            "Cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_disconnect(cx)),
        )
        .on_confirm(
            "integration-disconnect-confirm",
            "Disconnect",
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_disconnect(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("integration-disconnect-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_disconnect(cx));
            })
    }

    /// The runtime-gated alt-state banner, selected purely from the integration's
    /// `ConnectionState` — no per-integration branch. A live `Connected` integration
    /// shows no banner; the transient and disconnected states each surface a strip
    /// above the detail (reconnecting / connecting-in-flight / not-connected), while
    /// the full detail frame stays visible beneath.
    fn state_banner(&self, palette: &ForgePalette, density: Density) -> Option<AnyElement> {
        let (accent, glyph, title, detail): (Rgba, Icon, &str, &str) = match self.connection {
            ConnectionState::Connected => return None,
            ConnectionState::Connecting => (
                palette.info,
                Icon::Refresh,
                "Connecting…",
                "Establishing a session with this integration.",
            ),
            ConnectionState::Reconnecting => (
                palette.warning,
                Icon::Refresh,
                "Reconnecting…",
                "The session dropped; forge is re-establishing it.",
            ),
            ConnectionState::Disconnected => (
                palette.text_muted,
                Icon::PlugConnected,
                "Not connected",
                "Use Reconnect above to link this integration.",
            ),
        };

        let text_col = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(title.to_owned()),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(detail.to_owned()),
            );

        Some(
            div()
                .w_full()
                .flex()
                .items_start()
                .gap(spacing(Spacing::Sm, density))
                .py(spacing(Spacing::Sm, density))
                .px(spacing(Spacing::Md, density))
                .rounded(radius(Radius::Md))
                .border(BORDER_THIN)
                .border_color(accent)
                .bg(palette.elevated)
                .child(icon(glyph, FONT_SM, accent))
                .child(text_col)
                .into_any_element(),
        )
    }

    fn toast_banner(
        &self,
        message: String,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .absolute()
            .right(spacing(Spacing::Md, density))
            .bottom(spacing(Spacing::Md, density))
            .id("integration-toast")
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Md, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.success)
            .bg(palette.elevated)
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.dismiss_toast(cx)))
            .child(icon(Icon::CircleCheck, FONT_SM, palette.success))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(message),
            )
            .into_any_element()
    }
}

impl Render for IntegrationDetail {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let header_card = self.header_card(&palette, density, cx);
        let reconnecting = matches!(
            self.connection,
            ConnectionState::Connecting | ConnectionState::Reconnecting
        );
        let state_banner = self.state_banner(&palette, density);
        let health = health_grid(&self.health_metrics, reconnecting, &palette, density);
        let content = content_sections(&self.sections, &palette, density);
        let quick = self.quick_actions_card(&palette, density, cx);

        let body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .children(state_banner)
            .child(header_card)
            .child(health)
            .child(content)
            .child(quick);

        let crumbs = breadcrumb(
            vec![
                BreadcrumbCrumb::link(
                    "Platforms",
                    "integration-crumb-platforms",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.go_back(cx)),
                ),
                BreadcrumbCrumb::leaf(self.display_name.clone()),
            ],
            &palette,
        );

        let scroll = div()
            .id("integration-detail-scroll")
            .flex_1()
            .overflow_y_scroll()
            .bg(palette.base)
            .child(
                div()
                    .w_full()
                    .py(spacing(Spacing::Md, density))
                    .px(spacing(Spacing::Lg, density))
                    .child(body),
            );

        let disconnect_overlay = self
            .pending_disconnect
            .then(|| self.disconnect_overlay(&palette, cx));
        let picker_overlay = self.pending_picker.as_ref().map(|pending| {
            let view = cx.entity();
            overlay(pending.picker.clone(), &palette)
                .position(OverlayPosition::Center)
                .on_dismiss("integration-picker-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel_picker(cx));
                })
                .into_any_element()
        });
        let toast = self
            .toast
            .clone()
            .map(|m| self.toast_banner(m, &palette, density, cx));

        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(crumbs)
            .child(scroll)
            .children(disconnect_overlay)
            .children(picker_overlay)
            .children(toast)
    }
}

/// Resolves the hero identity (initial letter + brand hue) from the seed icon
/// token, falling back to the display name's first letter on an unknown token.
fn hero_identity(icon_str: &str, display_name: &str, palette: &ForgePalette) -> (String, Rgba) {
    match icon_str {
        "brand-twitch" => ("T".to_owned(), palette.brand),
        "brand-youtube" => ("Y".to_owned(), palette.random),
        "brand-kick" => ("K".to_owned(), palette.info),
        "broadcast" => ("O".to_owned(), palette.success),
        "mood-smile" => ("V".to_owned(), palette.warning),
        "brand-discord" => ("D".to_owned(), palette.brand),
        "piano" => ("M".to_owned(), palette.accent_teal),
        "keyboard" => ("H".to_owned(), palette.info),
        _ => {
            let initial = display_name
                .chars()
                .next()
                .map(|c| c.to_ascii_uppercase().to_string())
                .unwrap_or_else(|| "?".to_owned());
            (initial, palette.brand)
        }
    }
}

/// The lifecycle verb a control dispatch runs, routing the three `BuiltinControl`
/// methods through the single spawn site in [`IntegrationDetail::dispatch_control`].
enum ControlVerb {
    Reconnect,
    Disconnect,
    RefreshToken,
}

/// The open picker quick action awaiting a target selection. Holds the searchable picker
/// entity plus enough context to complete the deferred dispatch once a row is picked:
/// which quick action opened it, the target kind, and — for a source pick — the scene the
/// source list was read from (injected alongside the chosen source).
struct PendingPicker {
    picker: Entity<Picker>,
    action_index: usize,
    kind: PickerKind,
    current_scene: Option<String>,
    _sub: Subscription,
}

/// The already-resolved strings the OBS target picker renders for a given [`PickerKind`].
/// Only the three OBS kinds surface a picker here; the others carry no picker quick action
/// on this screen and fall back to a generic title.
fn picker_labels(kind: PickerKind) -> PickerLabels {
    let title = match kind {
        PickerKind::Scene => "Choose a Scene",
        PickerKind::Source => "Choose a Source",
        PickerKind::Input => "Choose an Audio Input",
        PickerKind::Hotkey => "Choose a Hotkey",
        PickerKind::Expression => "Choose an Expression",
        PickerKind::MidiPort => "Choose a MIDI Port",
    };
    PickerLabels {
        title: title.into(),
        placeholder: "Search…".into(),
        empty: "No matches".into(),
        loading: "Loading…".into(),
        cancel: "Cancel".into(),
    }
}

/// Enumerates the OBS targets for a picker kind off the live client, mapping each into a
/// [`PickerItem`]. A source read returns the enclosing scene alongside its rows so the
/// completed dispatch can pin the source to that scene. The three non-OBS kinds never
/// reach this path from this screen and report an unsupported error.
async fn fetch_picker_items(
    client: Arc<ObsClient>,
    kind: PickerKind,
) -> Result<(Vec<PickerItem>, Option<String>), String> {
    match kind {
        PickerKind::Scene => {
            let scenes = client.scenes().await.map_err(|e| e.to_string())?;
            let items = scenes
                .into_iter()
                .map(|name| PickerItem {
                    id: name.clone().into(),
                    label: name.into(),
                    sublabel: None,
                    icon: Icon::from_name("layout"),
                })
                .collect();
            Ok((items, None))
        }
        PickerKind::Source => {
            let scene = client
                .current_scene()
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "no active scene".to_owned())?;
            let sources = client.sources(&scene).await.map_err(|e| e.to_string())?;
            let items = sources
                .into_iter()
                .map(|source| PickerItem {
                    id: source.name.clone().into(),
                    label: source.name.into(),
                    sublabel: Some(if source.visible { "visible" } else { "hidden" }.into()),
                    icon: Icon::from_name("device-desktop"),
                })
                .collect();
            Ok((items, Some(scene)))
        }
        PickerKind::Input => {
            let inputs = client.audio_inputs().await.map_err(|e| e.to_string())?;
            let items = inputs
                .into_iter()
                .map(|name| PickerItem {
                    id: name.clone().into(),
                    label: name.into(),
                    sublabel: None,
                    icon: Icon::from_name("volume"),
                })
                .collect();
            Ok((items, None))
        }
        PickerKind::Hotkey | PickerKind::Expression | PickerKind::MidiPort => {
            Err("unsupported picker kind".to_owned())
        }
    }
}

fn header_action_label(action: &HeaderAction) -> &'static str {
    match action {
        HeaderAction::Reconnect => "Reconnect",
        HeaderAction::RefreshToken => "Refresh token",
        HeaderAction::Disconnect => "Disconnect",
        HeaderAction::Settings => "Settings",
    }
}

fn quick_action_accent(index: usize, palette: &ForgePalette) -> Rgba {
    match index % 4 {
        0 => palette.brand,
        1 => palette.random,
        2 => palette.warning,
        _ => palette.info,
    }
}

/// A rounded `surface_overlay` pill inking a monospace caption — the header's
/// version tag and the limited-capability badge share this shape.
fn pill(label: String, text_color: Rgba, palette: &ForgePalette) -> impl IntoElement {
    div()
        .flex_none()
        .px(px(6.0))
        .rounded(radius(Radius::Md))
        .bg(palette.surface_overlay)
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XS)
                .text_color(text_color)
                .child(label),
        )
}

fn sub_line(endpoint: Option<&str>, uptime: Option<Duration>) -> String {
    match (endpoint, uptime) {
        (Some(ep), Some(d)) => format!("{ep} \u{00b7} up {}", format_uptime(d)),
        (Some(ep), None) => ep.to_owned(),
        (None, Some(d)) => format!("up {}", format_uptime(d)),
        (None, None) => String::new(),
    }
}
