use std::collections::BTreeMap;
use std::sync::Arc;

use forge_components::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_SM, FONT_XS, FONT_XXS,
    ForgePalette, Icon, Radius, Spacing, anchored_popover_below, card, ghost_button, icon, overlay,
    primary_button, radius, spacing, tr, with_alpha,
};
use forge_hotkey::{HotkeyClient, HotkeyCombo, HotkeyId};
use forge_storage::DataProvider;
use forge_types::{Action, ActionId, PlatformScope, TriggerInstance, TriggerInstanceId, Variant};
use gpui::{
    AnyElement, ClickEvent, Context, Keystroke, Pixels, SharedString, Subscription, Window, div,
    prelude::*, px,
};

use crate::presentation::ActivePresentation;

const PICKER_WIDTH: Pixels = px(240.0);

const TRIGGER_HEIGHT: Pixels = px(34.0);

#[derive(Clone)]
struct HotkeyBinding {
    hotkey_id: HotkeyId,
    combo: String,
    action_name: Option<String>,
}

struct ConflictModal {
    combo: String,
    existing_hotkey_id: Option<HotkeyId>,
}

pub struct SettingsHotkeysView {
    backend: Arc<dyn DataProvider>,
    rt_handle: tokio::runtime::Handle,
    hotkey_client: Option<Arc<HotkeyClient>>,
    bindings: Vec<HotkeyBinding>,
    actions: Vec<(ActionId, String)>,
    captured_combo: Option<String>,
    selected_action: Option<ActionId>,
    bind_error: Option<String>,
    conflict: Option<ConflictModal>,
    capturing: bool,
    capture_sub: Option<Subscription>,
    picker_open: bool,
    bindings_loading: bool,
    bind_in_progress: bool,
}

impl SettingsHotkeysView {
    pub fn new(
        backend: Arc<dyn DataProvider>,
        rt_handle: tokio::runtime::Handle,
        hotkey_client: Option<Arc<HotkeyClient>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut view = Self {
            backend,
            rt_handle,
            hotkey_client,
            bindings: Vec::new(),
            actions: Vec::new(),
            captured_combo: None,
            selected_action: None,
            bind_error: None,
            conflict: None,
            capturing: false,
            capture_sub: None,
            picker_open: false,
            bindings_loading: false,
            bind_in_progress: false,
        };
        view.load(cx);
        view
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        self.bindings_loading = true;
        let backend = Arc::clone(&self.backend);
        let registered: Vec<(HotkeyId, String)> = self
            .hotkey_client
            .as_ref()
            .map(|c| {
                c.registered_combos()
                    .into_iter()
                    .map(|(id, combo)| (id, combo.as_str().to_owned()))
                    .collect()
            })
            .unwrap_or_default();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let _ = tx.send(load_data(backend, registered).await);
        });
        cx.spawn(async move |this, cx| {
            if let Ok(result) = rx.await {
                let _ = this.update(cx, |this, cx| this.apply_loaded(result, cx));
            }
        })
        .detach();
        cx.notify();
    }

    fn apply_loaded(&mut self, result: Result<LoadedData, String>, cx: &mut Context<Self>) {
        self.bindings_loading = false;
        match result {
            Ok(data) => {
                self.actions = data.actions;
                self.bindings = data.bindings;
            }
            Err(message) => {
                tracing::warn!(error = %message, "failed to load hotkey bindings");
                self.bind_error =
                    Some(tr!("settings_hotkeys_error_load_bindings", error = message));
            }
        }
        cx.notify();
    }

    fn toggle_capture(&mut self, cx: &mut Context<Self>) {
        if self.capturing {
            self.end_capture();
            cx.notify();
            return;
        }
        self.capturing = true;
        self.bind_error = None;
        let weak = cx.entity().downgrade();
        let sub = cx.intercept_keystrokes(move |event, _window, cx| {
            let keystroke = event.keystroke.clone();
            let _ = weak.update(cx, |this, cx| this.on_capture_keystroke(keystroke, cx));
            cx.stop_propagation();
        });
        self.capture_sub = Some(sub);
        cx.notify();
    }

    fn end_capture(&mut self) {
        self.capturing = false;
        self.capture_sub = None;
    }

    fn on_capture_keystroke(&mut self, keystroke: Keystroke, cx: &mut Context<Self>) {
        if !self.capturing {
            return;
        }
        if keystroke.key == "escape" {
            self.captured_combo = None;
            self.bind_error = None;
            self.end_capture();
            cx.notify();
            return;
        }
        let Some(combo) = keystroke_to_combo(&keystroke) else {
            return;
        };
        self.captured_combo = Some(combo);
        self.bind_error = None;
        self.end_capture();
        cx.notify();
    }

    fn toggle_picker(&mut self, cx: &mut Context<Self>) {
        self.picker_open = !self.picker_open;
        cx.notify();
    }

    fn close_picker(&mut self, cx: &mut Context<Self>) {
        if self.picker_open {
            self.picker_open = false;
            cx.notify();
        }
    }

    fn select_action(&mut self, action_id: ActionId, cx: &mut Context<Self>) {
        self.selected_action = Some(action_id);
        self.picker_open = false;
        cx.notify();
    }

    fn selected_action_name(&self) -> Option<&str> {
        let id = self.selected_action?;
        self.actions
            .iter()
            .find(|(aid, _)| *aid == id)
            .map(|(_, name)| name.as_str())
    }

    fn bind(&mut self, cx: &mut Context<Self>) {
        let Some(combo_str) = self.captured_combo.clone() else {
            self.bind_error = Some(tr!("settings_hotkeys_error_no_combo"));
            cx.notify();
            return;
        };
        let Some(action_id) = self.selected_action else {
            self.bind_error = Some(tr!("settings_hotkeys_error_no_action"));
            cx.notify();
            return;
        };
        let Some(client) = self.hotkey_client.clone() else {
            self.bind_error = Some(tr!("settings_hotkeys_error_unavailable"));
            cx.notify();
            return;
        };
        if let Some(existing) = self.bindings.iter().find(|b| b.combo == combo_str) {
            self.conflict = Some(ConflictModal {
                combo: combo_str,
                existing_hotkey_id: Some(existing.hotkey_id),
            });
            cx.notify();
            return;
        }
        self.bind_in_progress = true;
        self.bind_error = None;
        let backend = Arc::clone(&self.backend);
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let _ = tx.send(do_bind(client, backend, combo_str, action_id).await);
        });
        cx.spawn(async move |this, cx| {
            if let Ok(result) = rx.await {
                let _ = this.update(cx, |this, cx| this.apply_bind_result(result, cx));
            }
        })
        .detach();
        cx.notify();
    }

    fn apply_bind_result(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        self.bind_in_progress = false;
        match result {
            Ok(()) => {
                self.captured_combo = None;
                self.selected_action = None;
                self.load(cx);
            }
            Err(message) => {
                if is_already_registered(&message) {
                    let combo = self.captured_combo.clone().unwrap_or_default();
                    let existing = self
                        .bindings
                        .iter()
                        .find(|b| b.combo == combo)
                        .map(|b| b.hotkey_id);
                    self.conflict = Some(ConflictModal {
                        combo,
                        existing_hotkey_id: existing,
                    });
                } else {
                    tracing::warn!(error = %message, "hotkey bind failed");
                    self.bind_error = Some(message);
                }
                cx.notify();
            }
        }
    }

    fn unbind(&mut self, hotkey_id: HotkeyId, cx: &mut Context<Self>) {
        let Some(client) = self.hotkey_client.clone() else {
            return;
        };
        let backend = Arc::clone(&self.backend);
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let _ = tx.send(do_unbind(client, backend, hotkey_id).await);
        });
        cx.spawn(async move |this, cx| {
            if let Ok(result) = rx.await {
                let _ = this.update(cx, |this, cx| this.apply_unbind_result(result, cx));
            }
        })
        .detach();
    }

    fn apply_unbind_result(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        match result {
            Ok(()) => self.load(cx),
            Err(message) => {
                tracing::warn!(error = %message, "hotkey unbind failed");
                self.bind_error = Some(tr!("settings_hotkeys_error_unbind", error = message));
                cx.notify();
            }
        }
    }

    fn conflict_replace(&mut self, cx: &mut Context<Self>) {
        let Some(modal) = self.conflict.take() else {
            return;
        };
        let Some(existing_id) = modal.existing_hotkey_id else {
            self.bind_error = Some(tr!("settings_hotkeys_error_conflict_not_found"));
            cx.notify();
            return;
        };
        let Some(combo_str) = self.captured_combo.clone() else {
            cx.notify();
            return;
        };
        let Some(action_id) = self.selected_action else {
            cx.notify();
            return;
        };
        let Some(client) = self.hotkey_client.clone() else {
            self.bind_error = Some(tr!("settings_hotkeys_error_unavailable"));
            cx.notify();
            return;
        };
        self.bind_in_progress = true;
        self.bind_error = None;
        let backend = Arc::clone(&self.backend);
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let _ = tx.send(do_replace(client, backend, existing_id, combo_str, action_id).await);
        });
        cx.spawn(async move |this, cx| {
            if let Ok(result) = rx.await {
                let _ = this.update(cx, |this, cx| this.apply_replace_result(result, cx));
            }
        })
        .detach();
        cx.notify();
    }

    fn apply_replace_result(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        self.bind_in_progress = false;
        match result {
            Ok(()) => {
                self.captured_combo = None;
                self.selected_action = None;
                self.load(cx);
            }
            Err(message) => {
                tracing::warn!(error = %message, "hotkey replace failed");
                self.bind_error = Some(tr!("settings_hotkeys_error_replace", error = message));
                cx.notify();
            }
        }
    }

    fn conflict_cancel(&mut self, cx: &mut Context<Self>) {
        self.conflict = None;
        cx.notify();
    }

    fn dismiss_error(&mut self, cx: &mut Context<Self>) {
        self.bind_error = None;
        cx.notify();
    }

    fn section_header(
        &self,
        key: &'static str,
        count: Option<usize>,
        palette: &ForgePalette,
    ) -> impl IntoElement {
        let mut row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(tr!(key)),
            );
        if let Some(count) = count {
            row = row.child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(format!("({count})")),
            );
        }
        row
    }

    fn capture_field(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let (label, text_color, border_color) = if self.capturing {
            (
                tr!("settings_hotkeys_capture_prompt"),
                palette.brand,
                palette.brand,
            )
        } else if let Some(combo) = &self.captured_combo {
            (combo.clone(), palette.text_primary, palette.border_input)
        } else {
            (
                tr!("widget_key_capture_placeholder"),
                palette.text_faint,
                palette.border_input,
            )
        };

        div()
            .id("settings-hotkeys-capture")
            .flex_1()
            .flex()
            .items_center()
            .h(TRIGGER_HEIGHT)
            .px(spacing(Spacing::Sm, Density::Cozy))
            .rounded(radius(Radius::Sm))
            .bg(if self.capturing {
                palette.elevated
            } else {
                palette.base
            })
            .border(BORDER_THIN)
            .border_color(border_color)
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_capture(cx)))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(text_color)
                    .child(label),
            )
            .into_any_element()
    }

    fn action_picker(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let (label, label_color) = match self.selected_action_name() {
            Some(name) => (name.to_owned(), palette.text_primary),
            None => (tr!("settings_hotkeys_select_action"), palette.text_faint),
        };

        let trigger = div()
            .id("settings-hotkeys-action-trigger")
            .flex()
            .items_center()
            .justify_between()
            .w(PICKER_WIDTH)
            .h(TRIGGER_HEIGHT)
            .px(spacing(Spacing::Sm, Density::Cozy))
            .rounded(radius(Radius::Sm))
            .bg(palette.base)
            .border(BORDER_THIN)
            .border_color(palette.border_input)
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_picker(cx)))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(label_color)
                    .child(label),
            )
            .child(icon(Icon::ChevronDown, FONT_XS, palette.text_faint));

        let mut field = div().relative().child(trigger);
        if self.picker_open {
            field = field.child(self.picker_overlay(palette, cx));
        }
        field.into_any_element()
    }

    fn picker_overlay(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let mut panel = div()
            .flex()
            .flex_col()
            .w(PICKER_WIDTH)
            .py(spacing(Spacing::Xs, Density::Cozy))
            .bg(palette.elevated)
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_input)
            .occlude();

        if self.actions.is_empty() {
            panel = panel.child(
                div()
                    .px(spacing(Spacing::Sm, Density::Cozy))
                    .py(spacing(Spacing::Xs, Density::Cozy))
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_faint)
                    .child("-"),
            );
        }

        for (action_id, name) in &self.actions {
            let selected = self.selected_action == Some(*action_id);
            let aid = *action_id;
            let mut item = div()
                .id(SharedString::from(format!("settings-hotkeys-action-{aid}")))
                .flex()
                .items_center()
                .w_full()
                .gap(spacing(Spacing::Sm, Density::Cozy))
                .px(spacing(Spacing::Sm, Density::Cozy))
                .py(spacing(Spacing::Xs, Density::Cozy))
                .rounded(radius(Radius::Sm))
                .cursor_pointer()
                .hover(|style| style.bg(palette.surface_overlay))
                .on_click(
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.select_action(aid, cx)),
                )
                .child(
                    div()
                        .flex_1()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .text_size(FONT_SM)
                        .text_color(palette.text_primary)
                        .child(name.clone()),
                );
            if selected {
                item = item.child(icon(Icon::CircleCheck, FONT_SM, palette.brand));
            }
            panel = panel.child(item);
        }

        let view = cx.entity();
        anchored_popover_below(TRIGGER_HEIGHT, panel)
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_picker(cx));
            })
            .into_any_element()
    }

    fn bind_button(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let can_bind = self.captured_combo.is_some()
            && self.selected_action.is_some()
            && !self.bind_in_progress;
        primary_button(tr!("settings_hotkeys_bind_btn"), palette)
            .disabled(!can_bind)
            .on_click(
                "settings-hotkeys-bind",
                cx.listener(|this, _: &ClickEvent, _, cx| this.bind(cx)),
            )
            .into_any_element()
    }

    fn bind_error_banner(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let message = self.bind_error.as_ref()?;
        Some(
            div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xs, Density::Cozy))
                .px(spacing(Spacing::Sm, Density::Cozy))
                .py(px(6.0))
                .rounded(radius(Radius::Sm))
                .bg(with_alpha(palette.warning, 0.1))
                .border(BORDER_THIN)
                .border_color(palette.warning)
                .child(icon(Icon::AlertTriangle, px(13.0), palette.warning))
                .child(
                    div()
                        .flex_1()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .text_size(FONT_SM)
                        .text_color(palette.warning)
                        .child(message.clone()),
                )
                .child(
                    div()
                        .id("settings-hotkeys-dismiss-error")
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.dismiss_error(cx)))
                        .child(icon(Icon::X, px(13.0), palette.text_muted)),
                )
                .into_any_element(),
        )
    }

    fn bindings_list(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        if self.bindings.is_empty() {
            return div()
                .py(px(8.0))
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_faint)
                .child(tr!("settings_hotkeys_no_bindings"))
                .into_any_element();
        }

        let mut list = div().flex().flex_col().gap(px(2.0));
        for binding in &self.bindings {
            list = list.child(self.binding_row(binding, palette, cx));
        }
        list.into_any_element()
    }

    fn binding_row(
        &self,
        binding: &HotkeyBinding,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let hotkey_id = binding.hotkey_id;
        let action_child = match &binding.action_name {
            Some(name) => div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_secondary)
                .child(name.clone()),
            None => div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_faint)
                .child(SharedString::from("-")),
        };

        div()
            .flex()
            .items_center()
            .w_full()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .px(px(8.0))
            .py(px(6.0))
            .rounded(radius(Radius::Sm))
            .bg(palette.surface_overlay)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(binding.combo.clone()),
            )
            .child(icon(Icon::ArrowRight, px(11.0), palette.text_faint))
            .child(action_child)
            .child(div().flex_1())
            .child(
                div()
                    .id(SharedString::from(format!(
                        "settings-hotkeys-unbind-{}",
                        hotkey_id.0
                    )))
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| this.unbind(hotkey_id, cx)),
                    )
                    .child(icon(Icon::X, px(13.0), palette.text_muted)),
            )
    }

    fn conflict_overlay(
        &self,
        modal: &ConflictModal,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, Density::Cozy))
            .w(px(440.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap(px(4.0))
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_SM)
                            .text_color(palette.text_secondary)
                            .child(tr!("settings_hotkeys_conflict_body_prefix")),
                    )
                    .child(
                        div()
                            .font_family(DEFAULT_MONO_FAMILY)
                            .text_size(FONT_SM)
                            .text_color(palette.warning)
                            .child(modal.combo.clone()),
                    )
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_SM)
                            .text_color(palette.text_secondary)
                            .child(tr!("settings_hotkeys_conflict_body_suffix")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(spacing(Spacing::Sm, Density::Cozy))
                    .child(ghost_button(tr!("common_cancel"), palette).on_click(
                        "settings-hotkeys-conflict-cancel",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.conflict_cancel(cx)),
                    ))
                    .child(
                        primary_button(tr!("settings_hotkeys_replace_btn"), palette).on_click(
                            "settings-hotkeys-conflict-replace",
                            cx.listener(|this, _: &ClickEvent, _, cx| this.conflict_replace(cx)),
                        ),
                    ),
            );

        let weak = cx.entity().downgrade();
        overlay(card(body, palette), palette)
            .on_dismiss("settings-hotkeys-conflict-dismiss", move |_window, cx| {
                let _ = weak.update(cx, |this, cx| this.conflict_cancel(cx));
            })
            .into_any_element()
    }

    fn unavailable_notice(&self, palette: &ForgePalette) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .px(spacing(Spacing::Sm, Density::Cozy))
            .py(px(6.0))
            .rounded(radius(Radius::Sm))
            .bg(with_alpha(palette.warning, 0.1))
            .border(BORDER_THIN)
            .border_color(palette.warning)
            .child(icon(Icon::AlertTriangle, px(13.0), palette.warning))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.warning)
                    .child(tr!("settings_hotkeys_error_unavailable")),
            )
            .into_any_element()
    }
}

impl Render for SettingsHotkeysView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let subtitle = div()
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FONT_SM)
            .text_color(palette.text_muted)
            .child(tr!("settings_hotkeys_scope_subtitle"));

        let mut root = div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(subtitle);

        if self.hotkey_client.is_none() {
            root = root.child(self.unavailable_notice(&palette)).child(
                div()
                    .flex()
                    .flex_col()
                    .gap(spacing(Spacing::Xs, density))
                    .child(self.section_header("settings_hotkeys_backend_section", None, &palette))
                    .child(
                        div()
                            .font_family(DEFAULT_MONO_FAMILY)
                            .text_size(FONT_SM)
                            .text_color(palette.text_secondary)
                            .child(portal_status_label(None)),
                    ),
            );
            return root;
        }

        let mut bind_section = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(self.section_header("settings_hotkeys_bind_section", None, &palette))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(spacing(Spacing::Sm, density))
                    .child(self.capture_field(&palette, cx))
                    .child(self.action_picker(&palette, cx))
                    .child(self.bind_button(&palette, cx)),
            );
        if let Some(banner) = self.bind_error_banner(&palette, cx) {
            bind_section = bind_section.child(banner);
        }

        let registered_section = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(self.section_header(
                "settings_hotkeys_registered_section",
                Some(self.bindings.len()),
                &palette,
            ))
            .child(self.bindings_list(&palette, cx));

        let portal_status = self
            .hotkey_client
            .as_ref()
            .and_then(|c| c.portal_available());
        let backend_section = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(self.section_header("settings_hotkeys_backend_section", None, &palette))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_secondary)
                    .child(portal_status_label(portal_status)),
            );

        root = root
            .child(bind_section)
            .child(registered_section)
            .child(backend_section);

        if let Some(modal) = &self.conflict {
            let overlay = self.conflict_overlay(modal, &palette, cx);
            root = root.child(overlay);
        }

        root
    }
}

fn keystroke_to_combo(keystroke: &Keystroke) -> Option<String> {
    let key = keystroke.key.as_str();
    if key.is_empty() {
        return None;
    }
    let modifiers = &keystroke.modifiers;
    let mut parts: Vec<&str> = Vec::new();
    if modifiers.control {
        parts.push("Ctrl");
    }
    if modifiers.shift {
        parts.push("Shift");
    }
    if modifiers.alt {
        parts.push("Alt");
    }
    if modifiers.platform {
        parts.push("Meta");
    }
    let raw = if parts.is_empty() {
        key.to_owned()
    } else {
        format!("{}+{key}", parts.join("+"))
    };
    HotkeyCombo::parse(&raw).ok().map(|c| c.as_str().to_owned())
}

fn portal_status_label(available: Option<bool>) -> String {
    match available {
        Some(true) => "Portal (Wayland GlobalShortcuts) - active".to_owned(),
        Some(false) => "Evdev / X11 fallback - active".to_owned(),
        None => "N/A (Windows / macOS native)".to_owned(),
    }
}

fn is_already_registered(err: &str) -> bool {
    err.contains("already registered")
}

struct LoadedData {
    actions: Vec<(ActionId, String)>,
    bindings: Vec<HotkeyBinding>,
}

async fn load_data(
    backend: Arc<dyn DataProvider>,
    registered: Vec<(HotkeyId, String)>,
) -> Result<LoadedData, String> {
    let actions: Vec<Action> = backend
        .action_repo()
        .list()
        .await
        .map_err(|e| e.to_string())?;
    let actions: Vec<(ActionId, String)> = actions.into_iter().map(|a| (a.id, a.name)).collect();

    let instances = backend
        .trigger_instance_repo()
        .list_all()
        .await
        .map_err(|e| e.to_string())?;

    let mut combo_to_action: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for instance in &instances {
        if instance.kind_id != "hotkey.global.pressed" {
            continue;
        }
        let Some(Variant::String(combo)) = instance.overrides.get("combo") else {
            continue;
        };
        let action_ids = backend
            .trigger_instance_repo()
            .actions_using(instance.id)
            .await
            .map_err(|e| e.to_string())?;
        if let Some(aid) = action_ids.into_iter().next()
            && let Some(action) = backend
                .action_repo()
                .get(aid)
                .await
                .map_err(|e| e.to_string())?
        {
            combo_to_action.insert(combo.clone(), action.name);
        }
    }

    let bindings = registered
        .into_iter()
        .map(|(hotkey_id, combo)| {
            let action_name = combo_to_action.get(&combo).cloned();
            HotkeyBinding {
                hotkey_id,
                combo,
                action_name,
            }
        })
        .collect();

    Ok(LoadedData { actions, bindings })
}

async fn cleanup_stale_combo_instances(
    backend: &Arc<dyn DataProvider>,
    combo_str: &str,
) -> Result<(), String> {
    let instances = backend
        .trigger_instance_repo()
        .list_all()
        .await
        .map_err(|e| e.to_string())?;

    for instance in instances {
        if instance.kind_id != "hotkey.global.pressed" {
            continue;
        }
        let Some(Variant::String(existing_combo)) = instance.overrides.get("combo") else {
            continue;
        };
        if existing_combo != combo_str {
            continue;
        }
        let action_ids = backend
            .trigger_instance_repo()
            .actions_using(instance.id)
            .await
            .map_err(|e| e.to_string())?;
        for aid in action_ids {
            backend
                .trigger_instance_repo()
                .unlink_action(aid, instance.id)
                .await
                .map_err(|e| e.to_string())?;
        }
        backend
            .trigger_instance_repo()
            .delete(instance.id)
            .await
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

async fn do_bind(
    client: Arc<HotkeyClient>,
    backend: Arc<dyn DataProvider>,
    combo_str: String,
    action_id: ActionId,
) -> Result<(), String> {
    let combo = HotkeyCombo::parse(&combo_str).map_err(|e| e.to_string())?;
    client.register(combo).await.map_err(|e| e.to_string())?;

    cleanup_stale_combo_instances(&backend, &combo_str).await?;

    let mut overrides = BTreeMap::new();
    overrides.insert("combo".to_owned(), Variant::String(combo_str.clone()));
    let instance = TriggerInstance {
        id: TriggerInstanceId::new(),
        kind_id: "hotkey.global.pressed".to_owned(),
        name: combo_str,
        overrides,
        enabled: true,
        user_defined: true,
        platform_scope: PlatformScope::default(),
        cooldown_secs: 0,
        cooldown_global: true,
    };
    backend
        .trigger_instance_repo()
        .save(&instance)
        .await
        .map_err(|e| e.to_string())?;
    backend
        .trigger_instance_repo()
        .link_action(action_id, instance.id, 0)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

async fn do_unbind(
    client: Arc<HotkeyClient>,
    backend: Arc<dyn DataProvider>,
    hotkey_id: HotkeyId,
) -> Result<(), String> {
    let combo = client
        .registered_combos()
        .into_iter()
        .find(|(id, _)| *id == hotkey_id)
        .map(|(_, combo)| combo.as_str().to_owned());

    client
        .unregister(hotkey_id)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(combo) = combo {
        cleanup_stale_combo_instances(&backend, &combo).await?;
    }

    Ok(())
}

async fn do_replace(
    client: Arc<HotkeyClient>,
    backend: Arc<dyn DataProvider>,
    existing_id: HotkeyId,
    combo_str: String,
    action_id: ActionId,
) -> Result<(), String> {
    client
        .unregister(existing_id)
        .await
        .map_err(|e| e.to_string())?;
    do_bind(client, backend, combo_str, action_id).await
}
