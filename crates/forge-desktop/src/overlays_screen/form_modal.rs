use forge_components::{
    FONT_XS, FONT_XXS, ForgePalette, Icon, InputEvent, OverlayPosition, TextInput, body_family,
    icon, modal, mono_family, overlay, pad_tile, primary_button, secondary_button, tr,
};
use forge_storage::OverlayId;
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, Pixels, SharedString, Subscription,
    Window, div, prelude::*, px,
};

use crate::builtin_sections::grow_cell;
use crate::presentation::ActivePresentation;

const MODAL_W: Pixels = px(520.0);
const BODY_PAD_V: Pixels = px(16.0);
const BODY_PAD_H: Pixels = px(18.0);

const FIELD_GAP: Pixels = px(6.0);
const FIELD_MB: Pixels = px(16.0);
const TILE_GAP: Pixels = px(8.0);
const LOCKED_GAP: Pixels = px(8.0);
const LOCKED_GLYPH: Pixels = px(15.0);
const FOOTER_GAP: Pixels = px(8.0);

pub struct OverlayTypeChoice {
    pub kind_id: String,
    pub label: String,
    pub summary: String,
    pub icon: Icon,
}

pub struct OverlayFormLaunch {
    /// `Some` renames an existing record; the overlay type and the identity slug are then fixed.
    pub target: Option<OverlayId>,
    pub display_name: String,
    pub kind_id: String,
    pub types: Vec<OverlayTypeChoice>,
}

pub enum OverlayFormEvent {
    Submit {
        target: Option<OverlayId>,
        display_name: String,
        kind_id: String,
    },
    Cancel,
}

pub struct OverlayFormModal {
    target: Option<OverlayId>,
    name: Entity<TextInput>,
    types: Vec<OverlayTypeChoice>,
    kind_id: String,
    focus_pending: bool,
    _subs: Vec<Subscription>,
}

impl EventEmitter<OverlayFormEvent> for OverlayFormModal {}

impl OverlayFormModal {
    pub fn new(launch: OverlayFormLaunch, cx: &mut Context<Self>) -> Self {
        let palette = cx.palette();
        let OverlayFormLaunch {
            target,
            display_name,
            kind_id,
            types,
        } = launch;

        let name = cx.new(|cx| {
            let mut input = TextInput::new(tr!("overlays_form_name_placeholder"), cx)
                .with_palette(palette)
                .with_font_size(FONT_XS);
            input.set_content(display_name, cx);
            input
        });
        let subs = vec![cx.subscribe(&name, Self::on_name_event)];

        Self {
            target,
            name,
            types,
            kind_id,
            focus_pending: true,
            _subs: subs,
        }
    }

    fn on_name_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Cancelled => self.cancel(cx),
            InputEvent::Submitted(_) => self.submit(cx),
            InputEvent::Changed(_) => cx.notify(),
        }
    }

    fn is_rename(&self) -> bool {
        self.target.is_some()
    }

    fn entered_name(&self, cx: &Context<Self>) -> String {
        self.name.read(cx).content().trim().to_owned()
    }

    fn can_submit(&self, cx: &Context<Self>) -> bool {
        !self.entered_name(cx).is_empty() && !self.kind_id.is_empty()
    }

    fn pick_type(&mut self, kind_id: String, cx: &mut Context<Self>) {
        self.kind_id = kind_id;
        cx.notify();
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if !self.can_submit(cx) {
            return;
        }
        cx.emit(OverlayFormEvent::Submit {
            target: self.target.clone(),
            display_name: self.entered_name(cx),
            kind_id: self.kind_id.clone(),
        });
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(OverlayFormEvent::Cancel);
    }

    fn render_name(&self, palette: &ForgePalette) -> AnyElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(FIELD_GAP)
            .mb(FIELD_MB)
            .child(caption(tr!("overlays_form_name_label"), palette))
            .child(div().w_full().child(self.name.clone()))
            .into_any_element()
    }

    fn render_type_picker(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let mut row = div().w_full().flex().items_stretch().gap(TILE_GAP);
        for choice in &self.types {
            let selected = choice.kind_id == self.kind_id;
            let pick = choice.kind_id.clone();
            let tile = pad_tile(
                SharedString::from(format!("overlay-type-{}", choice.kind_id)),
                icon(choice.icon, LOCKED_GLYPH, palette.brand),
                div().child(choice.label.clone()),
                palette,
            )
            .sublabel(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(choice.summary.clone()),
            )
            .selected(selected)
            .accent(palette.brand)
            .hover_border(palette.brand)
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.pick_type(pick.clone(), cx)),
            );
            row = row.child(grow_cell(tile, 1.0));
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(FIELD_GAP)
            .child(caption(tr!("overlays_form_type_label"), palette))
            .child(row)
            .into_any_element()
    }

    fn render_locked_type(&self, palette: &ForgePalette) -> AnyElement {
        let chosen = self
            .types
            .iter()
            .find(|choice| choice.kind_id == self.kind_id);
        let (glyph, label) = match chosen {
            Some(choice) => (choice.icon, choice.label.clone()),
            None => (Icon::AlertTriangle, tr!("overlays_type_unavailable")),
        };

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(FIELD_GAP)
            .child(caption(tr!("overlays_form_type_label"), palette))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(LOCKED_GAP)
                    .child(icon(glyph, LOCKED_GLYPH, palette.text_muted))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_muted)
                            .child(label),
                    ),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(tr!("overlays_form_type_locked")),
            )
            .into_any_element()
    }

    fn render_footer(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let confirm = if self.is_rename() {
            tr!("overlays_form_save")
        } else {
            tr!("overlays_form_create")
        };

        div()
            .w_full()
            .flex()
            .items_center()
            .justify_end()
            .gap(FOOTER_GAP)
            .child(secondary_button(tr!("common_cancel"), palette).on_click(
                "overlay-form-cancel",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            ))
            .child(
                primary_button(confirm, palette)
                    .disabled(!self.can_submit(cx))
                    .on_click(
                        "overlay-form-submit",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.submit(cx)),
                    ),
            )
            .into_any_element()
    }
}

fn caption(label: String, palette: &ForgePalette) -> impl IntoElement {
    div()
        .font_family(mono_family())
        .text_size(FONT_XXS)
        .text_color(palette.text_muted)
        .child(SharedString::from(label.to_uppercase()))
}

impl Render for OverlayFormModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        if self.focus_pending {
            self.focus_pending = false;
            self.name.update(cx, |input, cx| input.focus(window, cx));
        }

        let type_section = if self.is_rename() {
            self.render_locked_type(&palette)
        } else {
            self.render_type_picker(&palette, cx)
        };

        let body = div()
            .w_full()
            .py(BODY_PAD_V)
            .px(BODY_PAD_H)
            .flex()
            .flex_col()
            .child(self.render_name(&palette))
            .child(type_section);

        let (title, subtitle) = if self.is_rename() {
            (
                tr!("overlays_form_title_rename"),
                tr!("overlays_form_subtitle_rename"),
            )
        } else {
            (
                tr!("overlays_form_title_create"),
                tr!("overlays_form_subtitle_create"),
            )
        };

        let card = modal(title, body, &palette)
            .header_icon(Icon::Browser, palette.brand)
            .subtitle(subtitle)
            .width(MODAL_W)
            .flush_body()
            .footer(self.render_footer(&palette, cx))
            .on_close(
                "overlay-form-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            );

        let view = cx.entity();
        div().absolute().top_0().left_0().size_full().child(
            overlay(card, &palette)
                .position(OverlayPosition::Center)
                .on_dismiss("overlay-form-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel(cx));
                }),
        )
    }
}
