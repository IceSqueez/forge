use std::collections::HashMap;
use std::sync::Arc;

use forge_components::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_LG, FONT_SM, FONT_XS,
    FONT_XXS, ForgePalette, Icon, Radius, Spacing, card, ghost_button, icon, overlay,
    primary_button, radius, spacing, tr, with_alpha,
};
use forge_storage::settings::reserved_keys::KEYBOARD_SHORTCUTS;
use forge_storage::{DataProvider, SettingsRepo};
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, Keystroke, SharedString, Subscription, Window,
    div, prelude::*, px,
};

use crate::actions::{
    SHORTCUTS, ShortcutEntry, canonical_chord, chord_is_bindable, effective_chord,
    parse_stored_overrides, reapply_key_bindings,
};
use crate::presentation::ActivePresentation;

struct ShortcutConflict {
    target_id: &'static str,
    owner_id: &'static str,
    chord: String,
}

pub struct SettingsShortcutsView {
    backend: Arc<dyn DataProvider>,
    rt_handle: tokio::runtime::Handle,
    overrides: HashMap<String, String>,
    rebinding: Option<&'static str>,
    rebind_error: Option<String>,
    conflict: Option<ShortcutConflict>,
    save_error: Option<String>,
    capture_sub: Option<Subscription>,
}

impl SettingsShortcutsView {
    pub fn new(
        backend: Arc<dyn DataProvider>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut view = Self {
            backend,
            rt_handle,
            overrides: HashMap::new(),
            rebinding: None,
            rebind_error: None,
            conflict: None,
            save_error: None,
            capture_sub: None,
        };
        view.load(cx);
        view
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let raw = repo.get_string(KEYBOARD_SHORTCUTS).await;
            let _ = tx.send(raw);
        });
        cx.spawn(async move |this, cx| {
            if let Ok(result) = rx.await {
                let _ = this.update(cx, |this, cx| this.apply_loaded(result, cx));
            }
        })
        .detach();
    }

    fn apply_loaded(
        &mut self,
        result: Result<Option<String>, forge_storage::StorageError>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(Some(raw)) => self.overrides = parse_stored_overrides(&raw),
            Ok(None) => self.overrides.clear(),
            Err(e) => {
                tracing::warn!(error = %e, "failed to load keyboard shortcuts");
                self.save_error = Some(e.to_string());
            }
        }
        cx.notify();
    }

    fn owner_of(&self, chord: &str, exclude: &str) -> Option<&'static str> {
        SHORTCUTS
            .iter()
            .find(|entry| {
                entry.id != exclude && effective_chord(&self.overrides, entry) == Some(chord)
            })
            .map(|entry| entry.id)
    }

    fn set_override(&mut self, id: &'static str, chord: String) {
        let default = SHORTCUTS
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.default_chord);
        if default == Some(chord.as_str()) {
            self.overrides.remove(id);
        } else {
            self.overrides.insert(id.to_owned(), chord);
        }
    }

    fn start_rebind(&mut self, id: &'static str, cx: &mut Context<Self>) {
        self.rebinding = Some(id);
        self.rebind_error = None;
        self.conflict = None;
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
        self.rebinding = None;
        self.capture_sub = None;
    }

    fn cancel_capture(&mut self, cx: &mut Context<Self>) {
        self.end_capture();
        cx.notify();
    }

    fn on_capture_keystroke(&mut self, keystroke: Keystroke, cx: &mut Context<Self>) {
        let Some(id) = self.rebinding else {
            return;
        };
        if keystroke.key == "escape" && !keystroke.modifiers.modified() {
            self.cancel_capture(cx);
            return;
        }
        let Some(chord) = canonical_chord(&keystroke) else {
            return;
        };
        if !chord_is_bindable(&chord) {
            self.rebind_error = Some(tr!("settings_shortcuts_error_needs_modifier"));
            self.end_capture();
            cx.notify();
            return;
        }
        if let Some(owner) = self.owner_of(&chord, id) {
            self.conflict = Some(ShortcutConflict {
                target_id: id,
                owner_id: owner,
                chord,
            });
            self.end_capture();
            cx.notify();
            return;
        }
        self.rebind_error = None;
        self.end_capture();
        self.set_override(id, chord);
        self.persist_and_apply(cx);
    }

    fn conflict_steal(&mut self, cx: &mut Context<Self>) {
        let Some(conflict) = self.conflict.take() else {
            return;
        };
        self.overrides
            .insert(conflict.owner_id.to_owned(), String::new());
        self.set_override(conflict.target_id, conflict.chord);
        self.rebind_error = None;
        self.persist_and_apply(cx);
    }

    fn conflict_cancel(&mut self, cx: &mut Context<Self>) {
        self.conflict = None;
        cx.notify();
    }

    fn reset_entry(&mut self, id: &'static str, cx: &mut Context<Self>) {
        self.overrides.remove(id);
        self.rebind_error = None;
        self.persist_and_apply(cx);
    }

    fn reset_all(&mut self, cx: &mut Context<Self>) {
        self.overrides.clear();
        self.rebind_error = None;
        self.conflict = None;
        self.end_capture();
        self.persist_and_apply(cx);
    }

    fn persist_and_apply(&mut self, cx: &mut Context<Self>) {
        reapply_key_bindings(cx, &self.overrides);

        let map = self.overrides.clone();
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let _ = tx.send(save_overrides(repo, map).await);
        });
        cx.spawn(async move |this, cx| {
            if let Ok(result) = rx.await {
                let _ = this.update(cx, |this, cx| this.apply_save_result(result, cx));
            }
        })
        .detach();
        cx.notify();
    }

    fn apply_save_result(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        match result {
            Ok(()) => self.save_error = None,
            Err(message) => {
                tracing::warn!(error = %message, "failed to persist keyboard shortcuts");
                self.save_error = Some(message);
            }
        }
        cx.notify();
    }

    fn header(&self, palette: &ForgePalette, density: Density) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(icon(Icon::Keyboard, px(18.0), palette.brand))
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .font_weight(FontWeight::MEDIUM)
                            .text_size(FONT_LG)
                            .text_color(palette.text_primary)
                            .child(tr!("settings_shortcuts_title")),
                    ),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_muted)
                    .child(tr!("settings_shortcuts_subtitle")),
            )
    }

    fn error_banner(&self, palette: &ForgePalette) -> Option<AnyElement> {
        let message = self.rebind_error.as_ref().or(self.save_error.as_ref())?;
        Some(
            div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xs, Density::Cozy))
                .px(spacing(Spacing::Sm, Density::Cozy))
                .py(px(6.0))
                .rounded(radius(Radius::Sm))
                .bg(with_alpha(palette.random, 0.1))
                .border(BORDER_THIN)
                .border_color(palette.random)
                .child(icon(Icon::AlertTriangle, px(13.0), palette.random))
                .child(
                    div()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .text_size(FONT_SM)
                        .text_color(palette.random)
                        .child(message.clone()),
                )
                .into_any_element(),
        )
    }

    fn chord_chip(&self, label: impl Into<SharedString>, palette: &ForgePalette) -> AnyElement {
        div()
            .px(px(8.0))
            .py(px(3.0))
            .rounded(radius(Radius::Sm))
            .bg(palette.surface_overlay)
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XS)
            .text_color(palette.text_primary)
            .child(label.into())
            .into_any_element()
    }

    fn entry_row(
        &self,
        entry: &'static ShortcutEntry,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let label = div()
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FONT_SM)
            .text_color(palette.text_primary)
            .child(tr!(entry.label_key));

        let mut controls = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density));

        if self.rebinding == Some(entry.id) {
            controls = controls
                .child(
                    div()
                        .px(px(8.0))
                        .py(px(3.0))
                        .rounded(radius(Radius::Sm))
                        .bg(with_alpha(palette.brand, 0.12))
                        .border(BORDER_THIN)
                        .border_color(with_alpha(palette.brand, 0.5))
                        .font_family(DEFAULT_BODY_FAMILY)
                        .text_size(FONT_XS)
                        .text_color(palette.brand)
                        .child(tr!("settings_shortcuts_capture_prompt")),
                )
                .child(ghost_button(tr!("common_cancel"), palette).on_click(
                    SharedString::from(format!("shortcut-cancel-{}", entry.id)),
                    cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_capture(cx)),
                ));
        } else {
            match effective_chord(&self.overrides, entry) {
                Some(chord) => {
                    controls = controls.child(self.chord_chip(chord.to_owned(), palette))
                }
                None => {
                    controls = controls.child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_XS)
                            .text_color(palette.text_faint)
                            .child(tr!("settings_shortcuts_unbound")),
                    );
                }
            }
            let id = entry.id;
            controls = controls.child(
                ghost_button(tr!("settings_shortcuts_rebind"), palette).on_click(
                    SharedString::from(format!("shortcut-rebind-{id}")),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.start_rebind(id, cx)),
                ),
            );
            if self.overrides.contains_key(id) {
                controls = controls.child(
                    ghost_button(tr!("settings_shortcuts_reset"), palette).on_click(
                        SharedString::from(format!("shortcut-reset-{id}")),
                        cx.listener(move |this, _: &ClickEvent, _, cx| this.reset_entry(id, cx)),
                    ),
                );
            }
        }

        div()
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .py(px(6.0))
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(label)
            .child(controls)
    }

    fn fixed_section(&self, palette: &ForgePalette, density: Density) -> impl IntoElement {
        let heading = div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(tr!("settings_shortcuts_fixed_section"));

        let fixed_row = |label: SharedString, chord: &'static str, this: &Self| {
            div()
                .flex()
                .items_center()
                .justify_between()
                .w_full()
                .child(
                    div()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .text_size(FONT_SM)
                        .text_color(palette.text_secondary)
                        .child(label),
                )
                .child(this.chord_chip(chord, palette))
        };

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(heading)
            .child(fixed_row(
                tr!("settings_shortcuts_fixed_enter").into(),
                "Enter",
                self,
            ))
            .child(fixed_row(
                tr!("settings_shortcuts_fixed_escape").into(),
                "Esc",
                self,
            ))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(tr!("settings_shortcuts_fixed_note")),
            )
    }

    fn conflict_overlay(
        &self,
        conflict: &ShortcutConflict,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let owner_label = SHORTCUTS
            .iter()
            .find(|entry| entry.id == conflict.owner_id)
            .map(|entry| tr!(entry.label_key))
            .unwrap_or_default();

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .w(px(440.0))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_secondary)
                    .child(tr!(
                        "settings_shortcuts_conflict_body",
                        chord = conflict.chord.as_str(),
                        owner = owner_label
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(spacing(Spacing::Sm, density))
                    .child(ghost_button(tr!("common_cancel"), palette).on_click(
                        "shortcut-conflict-cancel",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.conflict_cancel(cx)),
                    ))
                    .child(
                        primary_button(tr!("settings_shortcuts_conflict_steal"), palette).on_click(
                            "shortcut-conflict-steal",
                            cx.listener(|this, _: &ClickEvent, _, cx| this.conflict_steal(cx)),
                        ),
                    ),
            );

        let weak = cx.entity().downgrade();
        overlay(card(body, palette), palette)
            .on_dismiss("shortcut-conflict-dismiss", move |_window, cx| {
                let _ = weak.update(cx, |this, cx| this.conflict_cancel(cx));
            })
            .into_any_element()
    }
}

impl Render for SettingsShortcutsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let mut list = div().flex().flex_col();
        for entry in SHORTCUTS {
            list = list.child(self.entry_row(entry, &palette, density, cx));
        }

        let reset_all = ghost_button(tr!("settings_shortcuts_reset_all"), &palette).on_click(
            "shortcut-reset-all",
            cx.listener(|this, _: &ClickEvent, _, cx| this.reset_all(cx)),
        );

        let mut body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(self.header(&palette, density));

        if let Some(banner) = self.error_banner(&palette) {
            body = body.child(banner);
        }

        body = body
            .child(list)
            .child(self.fixed_section(&palette, density))
            .child(div().child(reset_all));

        let mut root = div().relative().size_full().child(body);
        if let Some(conflict) = &self.conflict {
            let overlay = self.conflict_overlay(conflict, &palette, density, cx);
            root = root.child(overlay);
        }
        root
    }
}

async fn save_overrides(
    repo: Arc<dyn SettingsRepo>,
    map: HashMap<String, String>,
) -> Result<(), String> {
    let json = serde_json::to_string(&map).map_err(|e| e.to_string())?;
    repo.set_string(KEYBOARD_SHORTCUTS, &json)
        .await
        .map_err(|e| e.to_string())
}
