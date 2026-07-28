use forge_components::{
    BORDER_THIN, FONT_XS, FONT_XXS, ForgePalette, Icon, InputEvent, OverlayPosition, Radius,
    TextInput, body_family, ghost_button_with_icon, icon, modal, mono_family, overlay,
    primary_button, secondary_button, tr,
};
use forge_discord::validate_webhook_url;
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, Pixels, SharedString, Subscription,
    Window, div, prelude::*, px,
};

use crate::presentation::ActivePresentation;
use crate::toasts::copy_to_clipboard;

const MODAL_W: Pixels = px(520.0);
const BODY_PAD_V: Pixels = px(16.0);
const BODY_PAD_H: Pixels = px(18.0);

const FIELD_GAP: Pixels = px(6.0);
const FIELD_MB: Pixels = px(14.0);
const BOX_PAD_V: Pixels = px(7.0);
const BOX_PAD_H: Pixels = px(11.0);
const BOX_GAP: Pixels = px(8.0);
const CONTROL_GLYPH: Pixels = px(12.0);
const NAME_LOCK_FS: Pixels = px(12.0);

const FOOTER_GAP: Pixels = px(8.0);
const TEST_HEIGHT: Pixels = px(32.0);

pub struct WebhookModalLaunch {
    pub original_name: Option<String>,
    pub name: String,
    pub url: String,
}

pub struct WebhookDraft {
    pub original_name: Option<String>,
    pub name: String,
    pub url: String,
}

pub enum DiscordWebhookModalEvent {
    Save(Box<WebhookDraft>),
    Test(Box<WebhookDraft>),
    Cancel,
}

pub struct DiscordWebhookModal {
    original_name: Option<String>,
    name: Entity<TextInput>,
    url: Entity<TextInput>,
    url_revealed: bool,
    testing: bool,
    focus_pending: bool,
    _subs: Vec<Subscription>,
}

impl EventEmitter<DiscordWebhookModalEvent> for DiscordWebhookModal {}

impl DiscordWebhookModal {
    pub fn new(launch: WebhookModalLaunch, cx: &mut Context<Self>) -> Self {
        let palette = cx.palette();
        let WebhookModalLaunch {
            original_name,
            name,
            url,
        } = launch;

        let name_field = cx.new(|cx| {
            let mut input = TextInput::new(tr!("discord_modal_name_placeholder"), cx)
                .with_palette(palette)
                .with_font_size(FONT_XS);
            input.set_content(name, cx);
            input
        });
        let url_field = cx.new(|cx| {
            let mut input = TextInput::new(tr!("discord_modal_url_placeholder"), cx)
                .with_palette(palette)
                .with_font_size(FONT_XS)
                .mono()
                .plain()
                .secure(true);
            input.set_content(url, cx);
            input
        });

        let subs = vec![
            cx.subscribe(&name_field, Self::on_field_event),
            cx.subscribe(&url_field, Self::on_field_event),
        ];

        Self {
            original_name,
            name: name_field,
            url: url_field,
            url_revealed: false,
            testing: false,
            focus_pending: true,
            _subs: subs,
        }
    }

    pub fn set_testing(&mut self, testing: bool, cx: &mut Context<Self>) {
        self.testing = testing;
        cx.notify();
    }

    fn on_field_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Cancelled => self.cancel(cx),
            InputEvent::Submitted(_) => self.save(cx),
            InputEvent::Changed(_) => cx.notify(),
        }
    }

    fn toggle_reveal(&mut self, cx: &mut Context<Self>) {
        self.url_revealed = !self.url_revealed;
        let secure = !self.url_revealed;
        self.url
            .update(cx, |input, cx| input.set_secure(secure, cx));
        cx.notify();
    }

    fn copy_url(&mut self, cx: &mut Context<Self>) {
        let url = self.url.read(cx).content().to_owned();
        if url.is_empty() {
            return;
        }
        copy_to_clipboard(url, cx);
    }

    fn draft(&self, cx: &Context<Self>) -> WebhookDraft {
        WebhookDraft {
            original_name: self.original_name.clone(),
            name: self.entered_name(cx),
            url: self.url.read(cx).content().trim().to_owned(),
        }
    }

    fn entered_name(&self, cx: &Context<Self>) -> String {
        match &self.original_name {
            Some(name) => name.clone(),
            None => self.name.read(cx).content().trim().to_owned(),
        }
    }

    fn url_is_valid(&self, cx: &Context<Self>) -> bool {
        validate_webhook_url(self.url.read(cx).content()).is_ok()
    }

    fn can_save(&self, cx: &Context<Self>) -> bool {
        !self.testing && !self.entered_name(cx).is_empty() && self.url_is_valid(cx)
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if !self.can_save(cx) {
            return;
        }
        cx.emit(DiscordWebhookModalEvent::Save(Box::new(self.draft(cx))));
    }

    fn test(&mut self, cx: &mut Context<Self>) {
        if self.testing || !self.url_is_valid(cx) {
            return;
        }
        self.testing = true;
        cx.emit(DiscordWebhookModalEvent::Test(Box::new(self.draft(cx))));
        cx.notify();
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(DiscordWebhookModalEvent::Cancel);
    }

    fn render_name(&self, palette: &ForgePalette) -> AnyElement {
        let value: AnyElement = match &self.original_name {
            Some(name) => div()
                .w_full()
                .font_family(mono_family())
                .text_size(NAME_LOCK_FS)
                .text_color(palette.text_muted)
                .child(name.clone())
                .into_any_element(),
            None => div().w_full().child(self.name.clone()).into_any_element(),
        };

        let mut column = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(FIELD_GAP)
            .mb(FIELD_MB)
            .child(field_caption(&tr!("discord_modal_name_label"), palette))
            .child(value_box(palette).child(div().flex_1().min_w(px(0.0)).child(value)));

        if self.original_name.is_some() {
            column = column.child(field_hint(&tr!("discord_modal_name_locked"), palette));
        }

        column.into_any_element()
    }

    fn render_url(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let reveal_glyph = if self.url_revealed {
            Icon::EyeOff
        } else {
            Icon::Eye
        };
        let entered = !self.url.read(cx).content().trim().is_empty();
        let invalid = entered && !self.url_is_valid(cx);

        let box_row = value_box(palette)
            .child(div().flex_1().min_w(px(0.0)).child(self.url.clone()))
            .child(
                div()
                    .id("discord-url-reveal")
                    .flex()
                    .flex_shrink_0()
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_reveal(cx)))
                    .child(icon(reveal_glyph, CONTROL_GLYPH, palette.text_faint)),
            )
            .child(
                div()
                    .id("discord-url-copy")
                    .flex()
                    .flex_shrink_0()
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.copy_url(cx)))
                    .child(icon(Icon::Copy, CONTROL_GLYPH, palette.text_faint)),
            );

        let hint = if invalid {
            div()
                .font_family(body_family())
                .text_size(FONT_XXS)
                .text_color(palette.random)
                .child(tr!("discord_modal_url_invalid"))
        } else {
            field_hint(&tr!("discord_modal_url_hint"), palette)
        };

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(FIELD_GAP)
            .mb(FIELD_MB)
            .child(field_caption(&tr!("discord_modal_url_label"), palette))
            .child(box_row)
            .child(hint)
            .into_any_element()
    }

    fn render_footer(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let test_label = if self.testing {
            tr!("discord_modal_testing")
        } else {
            tr!("discord_modal_test")
        };
        let confirm_label = if self.original_name.is_some() {
            tr!("discord_modal_save_changes")
        } else {
            tr!("discord_modal_add")
        };

        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(FOOTER_GAP)
            .child(
                ghost_button_with_icon(Icon::Send, test_label, palette)
                    .height(TEST_HEIGHT)
                    .on_click(
                        "discord-modal-test",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.test(cx)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(FOOTER_GAP)
                    .child(secondary_button(tr!("common_cancel"), palette).on_click(
                        "discord-modal-cancel",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
                    ))
                    .child(
                        primary_button(confirm_label, palette)
                            .disabled(!self.can_save(cx))
                            .on_click(
                                "discord-modal-save",
                                cx.listener(|this, _: &ClickEvent, _, cx| this.save(cx)),
                            ),
                    ),
            )
            .into_any_element()
    }
}

fn value_box(palette: &ForgePalette) -> gpui::Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap(BOX_GAP)
        .py(BOX_PAD_V)
        .px(BOX_PAD_H)
        .rounded(forge_components::radius(Radius::Md))
        .border(BORDER_THIN)
        .border_color(palette.border_input)
        .bg(palette.shell)
}

fn field_caption(label: &str, palette: &ForgePalette) -> impl IntoElement {
    div()
        .font_family(mono_family())
        .text_size(FONT_XXS)
        .text_color(palette.text_muted)
        .child(SharedString::from(label.to_uppercase()))
}

fn field_hint(text: &str, palette: &ForgePalette) -> gpui::Div {
    div()
        .font_family(body_family())
        .text_size(FONT_XXS)
        .text_color(palette.text_faint)
        .child(text.to_owned())
}

impl Render for DiscordWebhookModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        if self.focus_pending {
            self.focus_pending = false;
            if self.original_name.is_some() {
                self.url.update(cx, |input, cx| input.focus(window, cx));
            } else {
                self.name.update(cx, |input, cx| input.focus(window, cx));
            }
        }

        let body = div()
            .w_full()
            .py(BODY_PAD_V)
            .px(BODY_PAD_H)
            .flex()
            .flex_col()
            .child(self.render_name(&palette))
            .child(self.render_url(&palette, cx));

        let title = if self.original_name.is_some() {
            tr!("discord_modal_title_edit")
        } else {
            tr!("discord_modal_title_add")
        };

        let card = modal(title, body, &palette)
            .header_icon(Icon::BrandDiscord, palette.brand)
            .subtitle(tr!("discord_modal_subtitle"))
            .width(MODAL_W)
            .flush_body()
            .footer(self.render_footer(&palette, cx))
            .on_close(
                "discord-modal-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            );

        let view = cx.entity();
        div().absolute().top_0().left_0().size_full().child(
            overlay(card, &palette)
                .position(OverlayPosition::Center)
                .on_dismiss("discord-modal-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel(cx));
                }),
        )
    }
}
