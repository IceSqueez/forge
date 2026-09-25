use std::path::PathBuf;

use forge_components::{
    BORDER_THIN, ChipGlyph, FONT_XS, Icon, InputEvent, OverlayPosition, Radius, Spacing, TextInput,
    body_family, chip, ghost_button_with_icon, icon, modal, mono_family, overlay, primary_button,
    radius, secondary_button, spacing, toggle, tr, with_alpha,
};
use forge_types::ClipId;
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, EventEmitter, Keystroke, Pixels, SharedString,
    Subscription, Window, div, prelude::*, px,
};

use crate::async_bridge;
use crate::combo_capture::{CapturedKey, ComboCapture, captured_key, warn_if_typing_key};
use crate::combo_conflict::{Claimant, ComboHolder, ComboHolders, conflict_prompt};
use crate::hotkey_action_modal::keycaps;
use crate::presentation::ActivePresentation;
use crate::soundboard::{
    CATEGORY_ORDER, audio_dialog_extensions, category_color, category_label, field_lite_label,
};

const MODAL_W: Pixels = px(440.0);
const LABEL_FS: Pixels = px(11.5);
const HINT_FS: Pixels = px(10.5);
const CATEGORY_GAP: Pixels = px(4.0);
const KEY_DISPLAY_PAD_H: Pixels = px(11.0);
const KEY_DISPLAY_RADIUS: Pixels = px(7.0);
const KEY_DISPLAY_GAP: Pixels = px(7.0);
const KEY_LISTEN_GLYPH: Pixels = px(12.0);
const KEY_BUTTON_HEIGHT: Pixels = px(32.0);
const ERROR_TINT_BG: f32 = 0.10;
const ERROR_TINT_BORDER: f32 = 0.30;

pub struct ClipDraft {
    pub edit_id: Option<ClipId>,
    pub name: String,
    pub file_path: PathBuf,
    pub category: String,
    pub loop_playback: bool,
    pub hotkey: Option<String>,
    /// The holder the user chose to replace; its binding is cleared only when the draft is saved.
    pub release: Option<ComboHolder>,
}

pub enum ClipEditorEvent {
    Submit(ClipDraft),
    Cancel,
}

pub struct ClipEditorLaunch {
    pub edit_id: Option<ClipId>,
    pub name: String,
    pub category: String,
    pub file_path: Option<PathBuf>,
    pub loop_playback: bool,
    pub hotkey: Option<String>,
}

struct PendingConflict {
    combo: String,
    holder: ComboHolder,
}

struct KeyField {
    combo: Option<String>,
    capture: ComboCapture,
    holders: ComboHolders,
    conflict: Option<PendingConflict>,
    release: Option<ComboHolder>,
}

pub struct ClipEditor {
    file_path: Option<PathBuf>,
    name_input: Entity<TextInput>,
    category: String,
    loop_playback: bool,
    saving: bool,
    error: Option<SharedString>,
    edit_id: Option<ClipId>,
    key: Option<KeyField>,
    /// Written back unchanged when there is no key field to edit it.
    kept_hotkey: Option<String>,
    rt_handle: tokio::runtime::Handle,
    _name_sub: Subscription,
}

impl EventEmitter<ClipEditorEvent> for ClipEditor {}

impl ClipEditor {
    pub fn new(
        launch: ClipEditorLaunch,
        holders: Option<ComboHolders>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let name_input = cx.new(|cx| {
            TextInput::new(tr!("soundboard_modal_name_placeholder"), cx).with_palette(palette)
        });
        if !launch.name.is_empty() {
            let name = launch.name;
            name_input.update(cx, |ti, cx| ti.set_content(name, cx));
        }
        let name_sub = cx.subscribe(
            &name_input,
            |this, _input, event: &InputEvent, cx| match event {
                InputEvent::Submitted(_) => this.submit(cx),
                InputEvent::Cancelled => this.cancel(cx),
                InputEvent::Changed(_) => cx.notify(),
                InputEvent::Blurred(_) => {}
            },
        );
        let kept_hotkey = launch.hotkey.clone();
        let key = holders.map(|holders| KeyField {
            combo: launch.hotkey,
            capture: ComboCapture::default(),
            holders,
            conflict: None,
            release: None,
        });
        ClipEditor {
            file_path: launch.file_path,
            name_input,
            category: launch.category,
            loop_playback: launch.loop_playback,
            saving: false,
            error: None,
            edit_id: launch.edit_id,
            key,
            kept_hotkey,
            rt_handle,
            _name_sub: name_sub,
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.name_input.update(cx, |f, cx| f.focus(window, cx));
    }

    pub fn set_holders(&mut self, holders: ComboHolders, cx: &mut Context<Self>) {
        if let Some(key) = &mut self.key {
            key.holders = holders;
        }
        cx.notify();
    }

    pub fn set_saving(&mut self, cx: &mut Context<Self>) {
        self.saving = true;
        self.error = None;
        cx.notify();
    }

    pub fn fail_save(&mut self, message: String, cx: &mut Context<Self>) {
        self.saving = false;
        self.error = Some(message.into());
        cx.notify();
    }

    fn set_category(&mut self, category: String, cx: &mut Context<Self>) {
        self.category = category;
        cx.notify();
    }

    fn toggle_loop(&mut self, cx: &mut Context<Self>) {
        self.loop_playback = !self.loop_playback;
        cx.notify();
    }

    fn browse_file(&mut self, cx: &mut Context<Self>) {
        let filter = async_bridge::DialogFilter {
            name: tr!("soundboard_file_filter_audio"),
            extensions: audio_dialog_extensions(),
        };
        async_bridge::spawn_dialog(
            &self.rt_handle,
            async_bridge::pick_file(Some(filter)),
            |this, result, cx| {
                if let Ok(path) = result {
                    this.apply_picked_file(path, cx);
                }
            },
            cx,
        );
    }

    fn apply_picked_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.file_path = Some(path.clone());
        self.error = None;
        let name_input = self.name_input.clone();
        if name_input.read(cx).content().trim().is_empty()
            && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
        {
            let stem = stem.to_owned();
            name_input.update(cx, |ti, cx| ti.set_content(stem, cx));
        }
        cx.notify();
    }

    fn capturing(&self) -> bool {
        self.key.as_ref().is_some_and(|key| key.capture.is_active())
    }

    fn start_key_capture(&mut self, cx: &mut Context<Self>) {
        let Some(key) = &mut self.key else {
            return;
        };
        key.capture.start(cx, Self::on_capture_keystroke);
        cx.notify();
    }

    fn on_capture_keystroke(&mut self, keystroke: Keystroke, cx: &mut Context<Self>) -> bool {
        if !self.capturing() {
            return false;
        }
        match captured_key(&keystroke) {
            CapturedKey::Cancel => self.stop_key_capture(cx),
            CapturedKey::Combo(combo) => self.on_captured_combo(combo, cx),
            CapturedKey::Unusable => {}
        }
        true
    }

    fn stop_key_capture(&mut self, cx: &mut Context<Self>) {
        if let Some(key) = &mut self.key {
            key.capture.stop();
        }
        cx.notify();
    }

    fn on_captured_combo(&mut self, combo: String, cx: &mut Context<Self>) {
        let claimant = Claimant::Clip(self.edit_id);
        let Some(key) = &mut self.key else {
            return;
        };
        key.capture.stop();
        warn_if_typing_key(&combo, cx);
        match key.holders.holder_of(&combo, claimant) {
            Some(holder) => key.conflict = Some(PendingConflict { combo, holder }),
            None => {
                key.combo = Some(combo);
                key.release = None;
            }
        }
        cx.notify();
    }

    fn replace_holder(&mut self, cx: &mut Context<Self>) {
        if let Some(key) = &mut self.key
            && let Some(PendingConflict { combo, holder }) = key.conflict.take()
        {
            key.combo = Some(combo);
            key.release = Some(holder);
        }
        cx.notify();
    }

    fn cancel_conflict(&mut self, cx: &mut Context<Self>) {
        if let Some(key) = &mut self.key {
            key.conflict = None;
        }
        cx.notify();
    }

    fn clear_key(&mut self, cx: &mut Context<Self>) {
        if let Some(key) = &mut self.key {
            key.capture.stop();
            key.combo = None;
            key.release = None;
        }
        cx.notify();
    }

    fn is_saveable(&self, cx: &App) -> bool {
        !self.saving
            && !self.capturing()
            && self.file_path.is_some()
            && !self.name_input.read(cx).content().trim().is_empty()
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if !self.is_saveable(cx) {
            self.error = Some(tr!("soundboard_modal_validation_error").into());
            cx.notify();
            return;
        }
        let (hotkey, release) = match &self.key {
            Some(key) => (key.combo.clone(), key.release.clone()),
            None => (self.kept_hotkey.clone(), None),
        };
        let draft = ClipDraft {
            edit_id: self.edit_id,
            name: self.name_input.read(cx).content().trim().to_owned(),
            file_path: self.file_path.clone().unwrap_or_default(),
            category: self.category.clone(),
            loop_playback: self.loop_playback,
            hotkey,
            release,
        };
        self.error = None;
        cx.emit(ClipEditorEvent::Submit(draft));
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(ClipEditorEvent::Cancel);
    }

    fn render_key_field(&self, key: &KeyField, cx: &mut Context<Self>) -> AnyElement {
        let palette = cx.palette();
        let density = cx.density();
        let capturing = key.capture.is_active();
        let border = if capturing {
            palette.success
        } else {
            palette.border_input
        };
        let mut display = div()
            .id("sb-modal-key-display")
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(KEY_DISPLAY_GAP)
            .h(KEY_BUTTON_HEIGHT)
            .px(KEY_DISPLAY_PAD_H)
            .rounded(KEY_DISPLAY_RADIUS)
            .border(BORDER_THIN)
            .border_color(border)
            .bg(palette.shell)
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.start_key_capture(cx)));
        display = match (&key.combo, capturing) {
            (_, true) => display
                .child(icon(Icon::Keyboard, KEY_LISTEN_GLYPH, palette.success))
                .child(
                    div()
                        .font_family(body_family())
                        .text_size(FONT_XS)
                        .text_color(palette.success)
                        .child(tr!("hotkeys_capture_prompt")),
                ),
            (Some(combo), false) => display.child(keycaps(combo, &palette)),
            (None, false) => display.child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!("soundboard_modal_key_none")),
            ),
        };

        let set_label = if key.combo.is_some() {
            tr!("soundboard_modal_key_change")
        } else {
            tr!("soundboard_modal_key_set")
        };
        let set = ghost_button_with_icon(Icon::Keyboard, set_label, &palette)
            .density(density)
            .height(KEY_BUTTON_HEIGHT)
            .on_click(
                "sb-modal-key-set",
                cx.listener(|this, _: &ClickEvent, _, cx| this.start_key_capture(cx)),
            );
        let clear = key.combo.as_ref().map(|_| {
            ghost_button_with_icon(Icon::X, tr!("soundboard_modal_key_clear"), &palette)
                .density(density)
                .height(KEY_BUTTON_HEIGHT)
                .on_click(
                    "sb-modal-key-clear",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.clear_key(cx)),
                )
        });

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Sm, density))
                    .child(display)
                    .child(div().flex_none().child(set))
                    .children(clear.map(|clear| div().flex_none().child(clear))),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(HINT_FS)
                    .text_color(palette.text_faint)
                    .child(tr!("soundboard_modal_key_hint")),
            )
            .into_any_element()
    }

    fn render_conflict(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let pending = self.key.as_ref()?.conflict.as_ref()?;
        let palette = cx.palette();
        Some(conflict_prompt(
            "sb-modal-key",
            pending.combo.clone(),
            &pending.holder.label(),
            &palette,
            cx,
            Self::cancel_conflict,
            Self::replace_holder,
        ))
    }
}

impl Render for ClipEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        let file_set = self.file_path.is_some();
        let file_label: String = self
            .file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(str::to_owned)
            .unwrap_or_else(|| tr!("soundboard_modal_no_file").to_string());
        let browse = ghost_button_with_icon(
            Icon::FolderOpen,
            tr!("soundboard_modal_browse_btn"),
            &palette,
        )
        .density(density)
        .on_click(
            "sb-modal-browse",
            cx.listener(|this, _: &ClickEvent, _, cx| this.browse_file(cx)),
        );
        let file_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(if file_set {
                        palette.text_secondary
                    } else {
                        palette.text_muted
                    })
                    .child(file_label),
            )
            .child(browse);

        let mut category_row = div().flex().items_center().gap(CATEGORY_GAP);
        for (idx, cat) in CATEGORY_ORDER.iter().enumerate() {
            let active = self.category == *cat;
            let color = category_color(cat, &palette);
            let value = (*cat).to_owned();
            category_row = category_row.child(
                chip(category_label(cat), ChipGlyph::Dot(color), active, &palette)
                    .density(density)
                    .on_click(
                        ("sb-modal-cat", idx),
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.set_category(value.clone(), cx)
                        }),
                    ),
            );
        }

        let loop_row = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(spacing(Spacing::Sm, density))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_primary)
                            .child(tr!("soundboard_modal_loop_label")),
                    )
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(HINT_FS)
                            .text_color(palette.text_faint)
                            .child(tr!("soundboard_modal_loop_hint")),
                    ),
            )
            .child(toggle(self.loop_playback, &palette).on_click(
                "sb-modal-loop",
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_loop(cx)),
            ));

        let key_field = self.key.as_ref().map(|key| self.render_key_field(key, cx));

        let mut body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(field_lite_label(
                tr!("soundboard_modal_section_name"),
                &palette,
            ))
            .child(div().child(self.name_input.clone()))
            .child(field_lite_label(
                tr!("soundboard_modal_section_category"),
                &palette,
            ))
            .child(category_row)
            .child(field_lite_label(
                tr!("soundboard_modal_section_file"),
                &palette,
            ))
            .child(file_row)
            .child(field_lite_label(
                tr!("soundboard_modal_section_playback"),
                &palette,
            ))
            .child(loop_row);

        if let Some(key_field) = key_field {
            body = body
                .child(field_lite_label(
                    tr!("soundboard_modal_section_key"),
                    &palette,
                ))
                .child(key_field);
        }

        if let Some(error) = self.error.clone() {
            body = body.child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .p(spacing(Spacing::Xs, density))
                    .rounded(radius(Radius::Sm))
                    .bg(with_alpha(palette.random, ERROR_TINT_BG))
                    .border(BORDER_THIN)
                    .border_color(with_alpha(palette.random, ERROR_TINT_BORDER))
                    .child(icon(Icon::InfoCircle, FONT_XS, palette.random))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_primary)
                            .child(error),
                    ),
            );
        }

        let saveable = self.is_saveable(cx);
        let hint = div()
            .flex_1()
            .font_family(body_family())
            .text_size(LABEL_FS)
            .text_color(palette.text_faint)
            .child(if !saveable {
                tr!("soundboard_modal_fill_required")
            } else if self.edit_id.is_some() {
                tr!("soundboard_modal_ready_edit")
            } else {
                tr!("soundboard_modal_ready")
            });
        let cancel = secondary_button(tr!("soundboard_modal_cancel_btn"), &palette).on_click(
            "sb-modal-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
        );
        let save = primary_button(tr!("soundboard_modal_save_btn"), &palette)
            .disabled(!saveable)
            .on_click(
                "sb-modal-save",
                cx.listener(|this, _: &ClickEvent, _, cx| this.submit(cx)),
            );
        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(spacing(Spacing::Sm, density))
            .child(hint)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(cancel)
                    .child(save),
            );

        let title = if self.edit_id.is_some() {
            tr!("soundboard_modal_title_edit")
        } else {
            tr!("soundboard_modal_title_add")
        };
        let card = modal(title, body, &palette)
            .header_icon(Icon::Music, palette.bits)
            .width(MODAL_W)
            .footer(footer)
            .on_close(
                "sb-modal-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            );
        let view = cx.entity();
        let editor = overlay(card, &palette)
            .position(OverlayPosition::Center)
            .busy(self.saving)
            .on_dismiss("sb-modal-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel(cx));
            });
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(editor)
            .children(self.render_conflict(cx))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::Arc;

    use forge_components::{Density, ThemeId};
    use forge_storage::Language;
    use forge_types::{ActionId, TriggerInstanceId};
    use gpui::{Modifiers, TestAppContext};

    use super::*;
    use crate::combo_conflict::ClipKey;
    use crate::hotkey_bindings::{BindingHalf, BindingRow};
    use crate::presentation::Presentation;
    use crate::toasts::Toasts;

    const ESCAPE: &str = "escape";

    type Saved = (Option<String>, Option<ComboHolder>);

    struct Drafts {
        saved: Vec<Saved>,
        _sub: Subscription,
    }

    struct Rig {
        editor: Entity<ClipEditor>,
        drafts: Entity<Drafts>,
        _runtime: tokio::runtime::Runtime,
    }

    impl Rig {
        fn open(
            cx: &mut TestAppContext,
            edit_id: Option<ClipId>,
            hotkey: Option<&str>,
            holders: Option<ComboHolders>,
        ) -> Self {
            crate::i18n::install_language(Language::En);
            cx.update(|cx| {
                cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
                cx.set_global(Toasts::new());
            });
            let runtime = tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap();
            let handle = runtime.handle().clone();
            let launch = ClipEditorLaunch {
                edit_id,
                name: "airhorn".to_owned(),
                category: CATEGORY_ORDER[0].to_owned(),
                file_path: Some(PathBuf::from("/clips/airhorn.wav")),
                loop_playback: false,
                hotkey: hotkey.map(str::to_owned),
            };
            let editor = cx.update(|cx| cx.new(|cx| ClipEditor::new(launch, holders, handle, cx)));
            let drafts = cx.update(|cx| {
                cx.new(|cx| Drafts {
                    saved: Vec::new(),
                    _sub: cx.subscribe(&editor, |this: &mut Drafts, _, event, _| {
                        if let ClipEditorEvent::Submit(draft) = event {
                            this.saved
                                .push((draft.hotkey.clone(), draft.release.clone()));
                        }
                    }),
                })
            });
            Self {
                editor,
                drafts,
                _runtime: runtime,
            }
        }

        fn act(
            &self,
            cx: &mut TestAppContext,
            f: impl FnOnce(&mut ClipEditor, &mut Context<ClipEditor>),
        ) {
            cx.update(|cx| self.editor.update(cx, f));
        }

        fn press(&self, cx: &mut TestAppContext, modifiers: Modifiers, key: &str) -> bool {
            let stroke = Keystroke {
                modifiers,
                key: key.to_owned(),
                key_char: None,
            };
            cx.update(|cx| {
                self.editor
                    .update(cx, |editor, cx| editor.on_capture_keystroke(stroke, cx))
            })
        }

        fn capture(&self, cx: &mut TestAppContext, key: &str) {
            self.act(cx, |editor, cx| editor.start_key_capture(cx));
            self.press(cx, Modifiers::default(), key);
        }

        fn capturing(&self, cx: &mut TestAppContext) -> bool {
            cx.update(|cx| self.editor.read(cx).capturing())
        }

        fn has_conflict(&self, cx: &mut TestAppContext) -> bool {
            cx.update(|cx| {
                self.editor
                    .read(cx)
                    .key
                    .as_ref()
                    .is_some_and(|key| key.conflict.is_some())
            })
        }

        fn save(&self, cx: &mut TestAppContext) -> Vec<Saved> {
            self.act(cx, |editor, cx| editor.submit(cx));
            cx.update(|cx| self.drafts.read(cx).saved.clone())
        }
    }

    fn clip_holder(combo: &str) -> (ComboHolders, ComboHolder) {
        let id = ClipId::new();
        let holders = ComboHolders {
            rows: Arc::new(Vec::new()),
            clips: vec![ClipKey {
                id,
                name: "bell".to_owned(),
                combo: combo.to_owned(),
            }],
        };
        let holder = ComboHolder::Clip {
            id,
            name: "bell".to_owned(),
        };
        (holders, holder)
    }

    fn trigger_holder(combo: &str) -> (ComboHolders, ComboHolder) {
        let press = TriggerInstanceId::new();
        let holders = ComboHolders {
            rows: Arc::new(vec![BindingRow {
                key: press,
                combo: combo.to_owned(),
                registered: true,
                press: Some(BindingHalf {
                    instance_id: press,
                    enabled: true,
                    action: Some((ActionId::new(), "Scene".to_owned())),
                }),
                release: None,
            }]),
            clips: Vec::new(),
        };
        (holders, ComboHolder::Action(Some("Scene".to_owned())))
    }

    #[gpui::test]
    fn a_captured_free_key_is_saved_as_the_clip_key(cx: &mut TestAppContext) {
        let rig = Rig::open(cx, None, None, Some(ComboHolders::default()));

        rig.capture(cx, "f9");

        assert_eq!(rig.save(cx), [(Some("F9".to_owned()), None)]);
    }

    #[gpui::test]
    fn escape_ends_capture_and_keeps_the_previous_key(cx: &mut TestAppContext) {
        let rig = Rig::open(cx, None, Some("F5"), Some(ComboHolders::default()));

        rig.capture(cx, ESCAPE);

        assert_eq!(rig.save(cx), [(Some("F5".to_owned()), None)]);
    }

    #[gpui::test]
    fn save_is_refused_while_the_key_field_is_listening(cx: &mut TestAppContext) {
        let rig = Rig::open(cx, None, Some("F5"), Some(ComboHolders::default()));

        rig.act(cx, |editor, cx| editor.start_key_capture(cx));

        assert_eq!(rig.save(cx), []);
    }

    #[gpui::test]
    fn an_unusable_keystroke_is_swallowed_and_keeps_the_field_listening(cx: &mut TestAppContext) {
        let rig = Rig::open(cx, None, None, Some(ComboHolders::default()));
        rig.act(cx, |editor, cx| editor.start_key_capture(cx));

        let consumed = rig.press(cx, Modifiers::default(), "");

        assert_eq!((consumed, rig.capturing(cx)), (true, true));
    }

    #[gpui::test]
    fn a_keystroke_outside_capture_is_left_for_the_rest_of_the_app(cx: &mut TestAppContext) {
        let rig = Rig::open(cx, None, None, Some(ComboHolders::default()));

        assert!(!rig.press(cx, Modifiers::default(), "f9"));
    }

    #[gpui::test]
    fn replace_saves_the_key_and_names_the_other_holder_to_release(cx: &mut TestAppContext) {
        for (holders, holder) in [clip_holder("F9"), trigger_holder("F9")] {
            let rig = Rig::open(cx, None, None, Some(holders));
            rig.capture(cx, "f9");

            rig.act(cx, |editor, cx| editor.replace_holder(cx));

            assert_eq!(
                rig.save(cx),
                [(Some("F9".to_owned()), Some(holder.clone()))],
                "{holder:?}"
            );
        }
    }

    #[gpui::test]
    fn a_held_key_waits_for_the_conflict_answer_before_taking_the_field(cx: &mut TestAppContext) {
        let (holders, _) = clip_holder("F9");
        let rig = Rig::open(cx, None, Some("F5"), Some(holders));

        rig.capture(cx, "f9");

        assert!(rig.has_conflict(cx));
    }

    #[gpui::test]
    fn cancelling_a_conflict_keeps_the_previous_key_and_releases_nothing(cx: &mut TestAppContext) {
        let (holders, _) = clip_holder("F9");
        let rig = Rig::open(cx, None, Some("F5"), Some(holders));
        rig.capture(cx, "f9");

        rig.act(cx, |editor, cx| editor.cancel_conflict(cx));

        assert_eq!(rig.save(cx), [(Some("F5".to_owned()), None)]);
    }

    #[gpui::test]
    fn a_later_free_capture_drops_the_holder_an_earlier_replace_named(cx: &mut TestAppContext) {
        let (holders, _) = clip_holder("F9");
        let rig = Rig::open(cx, None, None, Some(holders));
        rig.capture(cx, "f9");
        rig.act(cx, |editor, cx| editor.replace_holder(cx));

        rig.capture(cx, "f10");

        assert_eq!(rig.save(cx), [(Some("F10".to_owned()), None)]);
    }

    #[gpui::test]
    fn clear_drops_the_key_and_the_holder_a_replace_named(cx: &mut TestAppContext) {
        let (holders, _) = clip_holder("F9");
        let rig = Rig::open(cx, None, None, Some(holders));
        rig.capture(cx, "f9");
        rig.act(cx, |editor, cx| editor.replace_holder(cx));

        rig.act(cx, |editor, cx| editor.clear_key(cx));

        assert_eq!(rig.save(cx), [(None, None)]);
    }

    #[gpui::test]
    fn an_edited_clip_recapturing_its_own_key_raises_no_conflict(cx: &mut TestAppContext) {
        let own = ClipId::new();
        let holders = ComboHolders {
            rows: Arc::new(Vec::new()),
            clips: vec![ClipKey {
                id: own,
                name: "airhorn".to_owned(),
                combo: "F9".to_owned(),
            }],
        };
        let rig = Rig::open(cx, Some(own), Some("F9"), Some(holders));

        rig.capture(cx, "f9");

        assert!(!rig.has_conflict(cx));
    }

    #[gpui::test]
    fn without_a_hotkey_engine_the_stored_key_is_written_back_unchanged(cx: &mut TestAppContext) {
        let rig = Rig::open(cx, Some(ClipId::new()), Some("F5"), None);

        assert_eq!(rig.save(cx), [(Some("F5".to_owned()), None)]);
    }
}
