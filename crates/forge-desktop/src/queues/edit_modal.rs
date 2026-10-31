use super::{
    CONCURRENCY_BOX_RADIUS, CONCURRENCY_VALUE_W, DESC_FS, MAX_CONCURRENCY, MIN_CONCURRENCY,
    SERIAL_CONCURRENCY,
};
use forge_components::Density;
use forge_components::ForgePalette;
use forge_components::{
    ActivePresentation, BORDER_THIN, FONT_SM, FONT_XS, FONT_XXS, Icon, InputEvent, ModalSize,
    OverlayPosition, Spacing, TextInput, body_family, modal, mono_family, overlay, primary_button,
    scaled, secondary_button, slider, spacing, tr,
};
use forge_types::QueueId;
use gpui::Div;
use gpui::prelude::*;
use gpui::{
    App, ClickEvent, Context, Entity, EventEmitter, FontWeight, SharedString, Subscription, Window,
    div, px,
};

const CONCURRENCY_BOX_PAD_H: gpui::Pixels = px(12.0);
const CONCURRENCY_SLIDER_GAP: gpui::Pixels = px(14.0);

pub(super) struct QueueDraft {
    pub(super) editing: Option<QueueId>,
    pub(super) name: String,
    pub(super) description: String,
    pub(super) concurrency: u32,
}

pub(super) enum EditQueueEvent {
    Submit(QueueDraft),
    Cancel,
}

pub(super) struct EditQueueModal {
    pub(super) editing: Option<QueueId>,
    original_name: String,
    name_input: Entity<TextInput>,
    desc_input: Entity<TextInput>,
    pub(super) concurrency: u32,
    pub(super) saving: bool,
    _name_sub: Subscription,
    _desc_sub: Subscription,
}

impl EventEmitter<EditQueueEvent> for EditQueueModal {}

impl EditQueueModal {
    pub(super) fn new(
        editing: Option<QueueId>,
        name_seed: &str,
        desc_seed: &str,
        concurrency: u32,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let original_name = name_seed.to_owned();
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
                InputEvent::Blurred(_) => {}
            },
        );
        let desc_sub = cx.subscribe(
            &desc_input,
            |this, _f, event: &InputEvent, cx| match event {
                InputEvent::Submitted(_) => this.submit(cx),
                InputEvent::Cancelled => this.cancel(cx),
                InputEvent::Changed(_) | InputEvent::Blurred(_) => {}
            },
        );
        EditQueueModal {
            editing,
            original_name,
            name_input,
            desc_input,
            concurrency: concurrency.clamp(MIN_CONCURRENCY, MAX_CONCURRENCY),
            saving: false,
            _name_sub: name_sub,
            _desc_sub: desc_sub,
        }
    }

    pub(super) fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
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
            concurrency: self.concurrency,
        };
        cx.emit(EditQueueEvent::Submit(draft));
    }

    pub(super) fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(EditQueueEvent::Cancel);
    }
}

impl EditQueueModal {
    fn name_field(&self, palette: &ForgePalette, density: Density) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(SharedString::from(
                        tr!("queues_create_name_label").to_uppercase(),
                    )),
            )
            .child(div().child(self.name_input.clone()))
    }

    fn desc_field(&self, palette: &ForgePalette, density: Density) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xxs, density))
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
            .child(div().child(self.desc_input.clone()))
    }

    fn concurrency_box(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> Div {
        let concurrency = self.concurrency;
        let concurrency_hint = if concurrency <= SERIAL_CONCURRENCY {
            SharedString::from(tr!("queues_concurrency_serial"))
        } else {
            SharedString::from(tr!("queues_concurrency_parallel", count = concurrency))
        };
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .py(spacing(Spacing::Sm, density))
            .px(scaled(CONCURRENCY_BOX_PAD_H, density))
            .rounded(CONCURRENCY_BOX_RADIUS)
            .border(BORDER_THIN)
            .border_color(palette.border_input)
            .bg(palette.shell)
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(scaled(CONCURRENCY_SLIDER_GAP, density))
                    .child(
                        div().flex_1().child(
                            slider(
                                concurrency as f32,
                                MIN_CONCURRENCY as f32,
                                MAX_CONCURRENCY as f32,
                                palette,
                            )
                            .whole_numbers()
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
                            .w(CONCURRENCY_VALUE_W)
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
            )
    }

    fn concurrency_field(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
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
            .child(self.concurrency_box(palette, density, cx))
    }

    fn footer(&self, palette: &ForgePalette, density: Density, cx: &mut Context<Self>) -> Div {
        let saveable = self.is_saveable(cx);
        let save_label = if self.editing.is_some() {
            tr!("queues_edit_btn")
        } else {
            tr!("queues_create_btn")
        };
        let cancel = secondary_button(tr!("queues_create_cancel"), palette).on_click(
            "q-modal-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
        );
        let save = primary_button(save_label, palette)
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
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(hint)
            .child(buttons)
    }
}

impl Render for EditQueueModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        let title = if self.editing.is_some() {
            tr!("queues_edit_title", name = self.original_name.clone())
        } else {
            tr!("queues_create_title")
        };

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(self.name_field(&palette, density))
            .child(self.desc_field(&palette, density))
            .child(self.concurrency_field(&palette, density, cx));

        let card = modal(title, body, &palette)
            .header_icon(Icon::Stack2, palette.accent_peach)
            .subtitle(tr!("queues_create_subtitle"))
            .size(ModalSize::Md)
            .footer(self.footer(&palette, density, cx))
            .on_close(
                "q-modal-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            );

        let view = cx.entity();
        overlay(card, &palette)
            .position(OverlayPosition::Center)
            .busy(self.saving)
            .on_dismiss("q-modal-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel(cx));
            })
            .into_any_element()
    }
}
