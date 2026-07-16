use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_SM,
    FONT_XS, ForgePalette, Icon, InputEvent, OverlayPosition, Radius, Spacing, TextInput,
    breadcrumb, field_label, ghost_button_with_icon, icon, modal, overlay, primary_button,
    primary_button_with_icon, radius, row_card, secondary_button, slider, spacing, tr, with_alpha,
};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, Pixels, SharedString, Subscription, Window, div,
    prelude::*, px,
};

use crate::presentation::ActivePresentation;

const CARD_ACTION_RADIUS: Pixels = px(5.0);
const CARD_ACTION_GLYPH: Pixels = px(12.0);
const EMPTY_GLYPH: Pixels = px(24.0);
const CARDS_PER_ROW: usize = 3;
const VOLUME_MAX: f32 = 1.5;
const MODAL_WIDTH: Pixels = px(480.0);

const DEVICES: [(&str, &str); 3] = [
    ("default", "System default"),
    ("cable", "CABLE Input (VB-Audio Virtual Cable)"),
    ("headphones", "Headphones"),
];

struct SoundClip {
    id: u64,
    name: String,
    file_name: String,
    hotkey: Option<String>,
    device_label: String,
    /// Playback gain, `0.0..=VOLUME_MAX` (`1.0` = 100%).
    volume: f32,
    duration_label: String,
}

struct AddClipModal {
    editing: Option<u64>,
    file_name: Option<String>,
    name_input: Entity<TextInput>,
    hotkey_input: Entity<TextInput>,
    device_idx: usize,
    volume: f32,
    saving: bool,
    error: Option<SharedString>,
    _name_sub: Subscription,
    _hotkey_sub: Subscription,
}

pub struct SoundboardView {
    clips: Vec<SoundClip>,
    next_id: u64,
    loading: bool,
    error: Option<SharedString>,
    feedback: Option<SharedString>,
    modal: Option<AddClipModal>,
}

impl SoundboardView {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        let clips = seed_clips();
        let next_id = clips.iter().map(|c| c.id).max().map_or(0, |m| m + 1);
        Self {
            clips,
            next_id,
            loading: false,
            error: None,
            feedback: None,
            modal: None,
        }
    }

    fn play(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some((name, device)) = self
            .clips
            .iter()
            .find(|c| c.id == id)
            .map(|c| (c.name.clone(), c.device_label.clone()))
        else {
            return;
        };
        self.feedback = Some(
            tr!(
                "soundboard_playing_feedback",
                name = name.as_str(),
                device = device.as_str()
            )
            .into(),
        );
        cx.notify();
    }

    fn delete(&mut self, id: u64, cx: &mut Context<Self>) {
        let name = self
            .clips
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.name.clone());
        self.clips.retain(|c| c.id != id);
        if let Some(name) = name {
            self.feedback = Some(tr!("soundboard_removed_feedback", name = name.as_str()).into());
        }
        cx.notify();
    }

    fn open_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let modal = Self::build_modal(None, "", None, "", 1.0, 0, cx);
        modal.name_input.read(cx).focus(window);
        self.modal = Some(modal);
        cx.notify();
    }

    fn open_edit(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(clip) = self.clips.iter().find(|c| c.id == id) else {
            return;
        };
        let name = clip.name.clone();
        let file = Some(clip.file_name.clone());
        let hotkey = clip.hotkey.clone().unwrap_or_default();
        let volume = clip.volume;
        let device_idx = DEVICES
            .iter()
            .position(|(_, label)| *label == clip.device_label)
            .unwrap_or(0);
        let modal = Self::build_modal(Some(id), &name, file, &hotkey, volume, device_idx, cx);
        modal.name_input.read(cx).focus(window);
        self.modal = Some(modal);
        cx.notify();
    }

    fn build_modal(
        editing: Option<u64>,
        name_seed: &str,
        file_seed: Option<String>,
        hotkey_seed: &str,
        volume: f32,
        device_idx: usize,
        cx: &mut Context<Self>,
    ) -> AddClipModal {
        let palette = cx.palette();
        let name_seed = name_seed.to_owned();
        let hotkey_seed = hotkey_seed.to_owned();

        let name_input = cx.new(|cx| {
            let mut ti =
                TextInput::new(tr!("soundboard_modal_name_placeholder"), cx).with_palette(palette);
            ti.set_content(name_seed, cx);
            ti
        });
        let hotkey_input = cx.new(|cx| {
            let mut ti = TextInput::new(tr!("soundboard_modal_hotkey_placeholder"), cx)
                .with_palette(palette);
            ti.set_content(hotkey_seed, cx);
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
        let hotkey_sub = cx.subscribe(
            &hotkey_input,
            |this, _f, event: &InputEvent, cx| match event {
                InputEvent::Submitted(_) => this.save(cx),
                InputEvent::Cancelled => this.close_modal(cx),
                InputEvent::Changed(_) => cx.notify(),
            },
        );

        AddClipModal {
            editing,
            file_name: file_seed,
            name_input,
            hotkey_input,
            device_idx,
            volume,
            saving: false,
            error: None,
            _name_sub: name_sub,
            _hotkey_sub: hotkey_sub,
        }
    }

    fn close_modal(&mut self, cx: &mut Context<Self>) {
        self.modal = None;
        cx.notify();
    }

    fn browse_file(&mut self, cx: &mut Context<Self>) {
        let name_input = match self.modal.as_mut() {
            Some(modal) => {
                modal.file_name = Some("new-clip.wav".to_owned());
                modal.error = None;
                modal.name_input.clone()
            }
            None => return,
        };
        if name_input.read(cx).content().trim().is_empty() {
            name_input.update(cx, |ti, cx| ti.set_content("new-clip", cx));
        }
        cx.notify();
    }

    fn set_device(&mut self, idx: usize, cx: &mut Context<Self>) {
        if let Some(modal) = self.modal.as_mut() {
            modal.device_idx = idx;
        }
        cx.notify();
    }

    fn set_volume(&mut self, value: f32, cx: &mut Context<Self>) {
        if let Some(modal) = self.modal.as_mut() {
            modal.volume = value;
        }
        cx.notify();
    }

    fn modal_saveable(&self, cx: &Context<Self>) -> bool {
        self.modal.as_ref().is_some_and(|modal| {
            !modal.saving
                && modal.file_name.is_some()
                && !modal.name_input.read(cx).content().trim().is_empty()
        })
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if !self.modal_saveable(cx) {
            if let Some(modal) = self.modal.as_mut() {
                modal.error = Some(tr!("soundboard_modal_validation_error").into());
            }
            cx.notify();
            return;
        }

        let Some(modal) = self.modal.as_ref() else {
            return;
        };
        let name = modal.name_input.read(cx).content().trim().to_owned();
        let hotkey_raw = modal.hotkey_input.read(cx).content().trim().to_owned();
        let hotkey = (!hotkey_raw.is_empty()).then_some(hotkey_raw);
        let file_name = modal.file_name.clone().unwrap_or_default();
        let device_label = DEVICES
            .get(modal.device_idx)
            .map_or(DEVICES[0].1, |d| d.1)
            .to_owned();
        let volume = modal.volume;
        let editing = modal.editing;

        match editing {
            Some(id) => {
                if let Some(clip) = self.clips.iter_mut().find(|c| c.id == id) {
                    clip.name = name.clone();
                    clip.file_name = file_name;
                    clip.hotkey = hotkey;
                    clip.device_label = device_label;
                    clip.volume = volume;
                }
            }
            None => {
                let id = self.next_id;
                self.next_id += 1;
                self.clips.push(SoundClip {
                    id,
                    name: name.clone(),
                    file_name,
                    hotkey,
                    device_label,
                    volume,
                    duration_label: "\u{2014}".to_owned(),
                });
            }
        }

        self.modal = None;
        self.feedback = Some(tr!("soundboard_saved_feedback", name = name.as_str()).into());
        cx.notify();
    }

    fn clip_card(
        &self,
        clip: &SoundClip,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = clip.id;

        let name = div()
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FONT_SM)
            .text_color(palette.text_primary)
            .child(clip.name.clone());

        let mut chips = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(card_chip(
                clip.duration_label.clone(),
                palette.text_muted,
                palette,
            ));
        if let Some(hotkey) = clip.hotkey.clone() {
            chips = chips.child(card_chip(hotkey, palette.warning, palette));
        }

        let pct = (clip.volume * 100.0).round() as i32;
        let pct_color = if clip.volume > 1.0 {
            palette.warning
        } else {
            palette.text_secondary
        };
        let device_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .flex_1()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_muted)
                    .child(clip.device_label.clone()),
            )
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(pct_color)
                    .child(format!("{pct}%")),
            );

        let separator = div().w_full().h(px(1.0)).bg(palette.border_regular);

        let actions = div()
            .flex()
            .items_center()
            .justify_end()
            .gap(spacing(Spacing::Xs, density))
            .child(self.card_action(
                ("sb-play", id as usize),
                Icon::PlayerPlay,
                palette.success,
                palette,
                cx.listener(move |this, _: &ClickEvent, _, cx| this.play(id, cx)),
            ))
            .child(self.card_action(
                ("sb-edit", id as usize),
                Icon::InfoCircle,
                palette.info,
                palette,
                cx.listener(move |this, _: &ClickEvent, window, cx| this.open_edit(id, window, cx)),
            ))
            .child(self.card_action(
                ("sb-delete", id as usize),
                Icon::X,
                palette.random,
                palette,
                cx.listener(move |this, _: &ClickEvent, _, cx| this.delete(id, cx)),
            ));

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .p(spacing(Spacing::Md, density))
            .rounded(radius(Radius::Lg))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.elevated)
            .child(name)
            .child(chips)
            .child(device_row)
            .child(separator)
            .child(actions)
            .into_any_element()
    }

    fn card_action(
        &self,
        id: impl Into<gpui::ElementId>,
        glyph: Icon,
        hue: gpui::Rgba,
        palette: &ForgePalette,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> impl IntoElement {
        let wash = with_alpha(hue, 0.1);
        div()
            .id(id.into())
            .flex()
            .items_center()
            .justify_center()
            .py(spacing(Spacing::Xxs, Density::Cozy))
            .px(spacing(Spacing::Xs, Density::Cozy))
            .rounded(CARD_ACTION_RADIUS)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .cursor_pointer()
            .hover(move |s| s.bg(wash))
            .on_click(handler)
            .child(icon(glyph, CARD_ACTION_GLYPH, hue))
    }

    fn render_body(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let inner = if self.loading {
            centered_message(tr!("soundboard_loading"), palette.text_muted, density)
        } else if let Some(error) = self.error.clone() {
            centered_message(error, palette.random, density)
        } else if self.clips.is_empty() {
            self.empty_state(palette, density)
        } else {
            self.clip_grid(palette, density, cx)
        };

        div()
            .id("sb-scroll")
            .flex_1()
            .h_full()
            .overflow_y_scroll()
            .bg(palette.base)
            .child(div().w_full().p(spacing(Spacing::Md, density)).child(inner))
            .into_any_element()
    }

    fn empty_state(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Lg, density))
            .child(icon(Icon::Music, EMPTY_GLYPH, palette.text_faint))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_muted)
                    .child(tr!("soundboard_empty_title")),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_faint)
                    .child(tr!("soundboard_empty_hint")),
            )
            .into_any_element()
    }

    fn clip_grid(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let gap = spacing(Spacing::Md, density);
        let cards: Vec<AnyElement> = self
            .clips
            .iter()
            .map(|clip| self.clip_card(clip, palette, density, cx))
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
        modal_state: &AddClipModal,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = if modal_state.editing.is_some() {
            tr!("soundboard_modal_title_edit")
        } else {
            tr!("soundboard_modal_title_add")
        };

        let file_set = modal_state.file_name.is_some();
        let file_label = modal_state
            .file_name
            .clone()
            .unwrap_or_else(|| tr!("soundboard_modal_no_file"));
        let browse = ghost_button_with_icon(
            Icon::FolderOpen,
            tr!("soundboard_modal_browse_btn"),
            palette,
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
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(if file_set {
                        palette.text_secondary
                    } else {
                        palette.text_muted
                    })
                    .child(file_label),
            )
            .child(browse);

        let mut device_list = div().flex().flex_col().gap(spacing(Spacing::Xxs, density));
        for (idx, (_, label)) in DEVICES.iter().enumerate() {
            let selected = modal_state.device_idx == idx;
            let title_ink = if selected {
                palette.text_primary
            } else {
                palette.text_secondary
            };
            let device_title = div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(title_ink)
                .child(*label);
            let leading_tint = if selected {
                palette.brand
            } else {
                palette.text_faint
            };
            device_list = device_list.child(
                row_card(device_title, palette)
                    .density(density)
                    .bordered(palette.border_regular, BORDER_THIN, radius(Radius::Sm))
                    .selected(selected)
                    .leading(icon(Icon::Volume, FONT_SM, leading_tint))
                    .on_click(
                        ("sb-device", idx),
                        cx.listener(move |this, _: &ClickEvent, _, cx| this.set_device(idx, cx)),
                    ),
            );
        }

        let pct = (modal_state.volume * 100.0).round() as i32;
        let pct_color = if modal_state.volume > 1.0 {
            palette.warning
        } else {
            palette.text_secondary
        };
        let volume_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(div().flex_1().child(
                slider(modal_state.volume, 0.0, VOLUME_MAX, palette).on_change(
                    "sb-modal-volume",
                    cx.listener(|this, value: &f32, _, cx| this.set_volume(*value, cx)),
                ),
            ))
            .child(
                div()
                    .w(px(40.0))
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(pct_color)
                    .child(format!("{pct}%")),
            );

        let mut body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(field_label(
                palette,
                tr!("soundboard_modal_section_file"),
                file_row,
            ))
            .child(field_label(
                palette,
                tr!("soundboard_modal_section_name"),
                div().child(modal_state.name_input.clone()),
            ))
            .child(field_label(
                palette,
                tr!("soundboard_modal_section_hotkey"),
                div().child(modal_state.hotkey_input.clone()),
            ))
            .child(field_label(
                palette,
                tr!("soundboard_modal_section_device"),
                device_list,
            ))
            .child(field_label(
                palette,
                tr!("soundboard_modal_section_volume"),
                volume_row,
            ));

        if let Some(error) = modal_state.error.clone() {
            body = body.child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .p(spacing(Spacing::Xs, density))
                    .rounded(radius(Radius::Sm))
                    .bg(with_alpha(palette.random, 0.10))
                    .border(BORDER_THIN)
                    .border_color(with_alpha(palette.random, 0.30))
                    .child(icon(Icon::InfoCircle, FONT_XS, palette.random))
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_XS)
                            .text_color(palette.text_primary)
                            .child(error),
                    ),
            );
        }

        let saveable = self.modal_saveable(cx);
        let cancel = secondary_button(tr!("soundboard_modal_cancel_btn"), palette).on_click(
            "sb-modal-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.close_modal(cx)),
        );
        let save = primary_button(tr!("soundboard_modal_save_btn"), palette)
            .disabled(!saveable)
            .on_click(
                "sb-modal-save",
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
            .header_icon(Icon::Music, palette.brand)
            .width(MODAL_WIDTH)
            .footer(footer)
            .kbd_hint(tr!("soundboard_modal_kbd_hint"))
            .on_close(
                "sb-modal-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.close_modal(cx)),
            );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("sb-modal-scrim", move |_window, cx| {
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
            .child(icon(Icon::Volume, FONT_XS, palette.success))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_secondary)
                    .child(message),
            )
    }
}

impl Render for SoundboardView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let add_btn =
            primary_button_with_icon(Icon::Plus, tr!("soundboard_add_clip_btn"), &palette)
                .density(density)
                .on_click(
                    "sb-add",
                    cx.listener(|this, _: &ClickEvent, window, cx| this.open_add(window, cx)),
                );
        let header = breadcrumb(
            vec![
                BreadcrumbCrumb::leaf(tr!("soundboard_breadcrumb_builtin")),
                BreadcrumbCrumb::leaf(tr!("soundboard_breadcrumb_soundboard")),
            ],
            &palette,
        )
        .right(add_btn);

        let feedback = self
            .feedback
            .clone()
            .map(|message| self.feedback_banner(message, &palette, density));

        let body = self.render_body(&palette, density, cx);
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
            .child(body)
            .children(modal_overlay)
    }
}

fn card_chip(
    label: impl Into<SharedString>,
    ink: gpui::Rgba,
    palette: &ForgePalette,
) -> impl IntoElement {
    div()
        .py(spacing(Spacing::Xxs, Density::Cozy))
        .px(spacing(Spacing::Xs, Density::Cozy))
        .rounded(radius(Radius::Sm))
        .bg(palette.surface_overlay)
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XS)
                .text_color(ink)
                .child(label.into()),
        )
}

fn centered_message(
    message: impl Into<SharedString>,
    ink: gpui::Rgba,
    density: Density,
) -> AnyElement {
    div()
        .w_full()
        .flex()
        .flex_col()
        .items_center()
        .py(spacing(Spacing::Lg, density))
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(ink)
                .child(message.into()),
        )
        .into_any_element()
}

fn seed_clips() -> Vec<SoundClip> {
    let clip =
        |id: u64, name: &str, hotkey: &str, device: &str, volume: f32, dur: &str| SoundClip {
            id,
            name: name.to_owned(),
            file_name: format!("{}.wav", name.to_lowercase().replace(' ', "-")),
            hotkey: Some(hotkey.to_owned()),
            device_label: device.to_owned(),
            volume,
            duration_label: dur.to_owned(),
        };
    vec![
        clip(0, "Airhorn", "1", DEVICES[1].1, 1.0, "0:02"),
        clip(1, "Sad trombone", "2", DEVICES[1].1, 0.9, "0:03"),
        clip(2, "Bruh", "3", DEVICES[1].1, 1.2, "0:01"),
        clip(3, "New follow", "5", DEVICES[0].1, 0.85, "0:03"),
        clip(4, "Intro sting", "8", DEVICES[1].1, 1.0, "0:08"),
        clip(5, "Applause", "Q", DEVICES[2].1, 0.8, "0:04"),
    ]
}
