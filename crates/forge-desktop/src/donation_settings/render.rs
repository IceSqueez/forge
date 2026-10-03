use std::time::Duration;

use forge_components::{
    BORDER_THIN, ConfirmTone, Density, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon,
    OverlayPosition, Radius, Spacing, body_family, confirm_modal, ghost_button_with_icon, icon,
    mono_family, overlay, primary_button, radio_row, radius, secondary_button, segment, segmented,
    spacing, status_dot, tr,
};
use forge_monobank::MonobankJar;
use gpui::{
    AnyElement, App, ClickEvent, Context, Div, ElementId, Entity, Pixels, Rgba, SharedString,
    Window, div, prelude::*, px,
};

use super::{Busy, DonationSettingsView, JarSource, Outcome, jar_title};
use crate::builtin_sections::{card_shell, divider};
use crate::presentation::ActivePresentation;

const STATUS_DOT: Pixels = px(6.0);
const EYE_GLYPH: Pixels = px(12.0);
const LINK_GLYPH: Pixels = px(11.0);
const POLL_PRESETS_SECS: [u64; 6] = [10, 15, 30, 60, 120, 300];
const SECONDS_PER_MINUTE: u64 = 60;

impl DonationSettingsView {
    fn header(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let (dot, label) = match self.has_token {
            Some(true) => (palette.success, tr!("donation_token_status_saved")),
            Some(false) => (palette.warning, tr!("donation_token_status_missing")),
            None => (palette.text_faint, tr!("donation_token_status_unknown")),
        };
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Md, density))
            .child(
                text(
                    tr!("donation_settings_title", service = self.service_name()),
                    FONT_SM,
                    palette.text_primary,
                )
                .flex_1()
                .min_w(px(0.0)),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(status_dot(dot, STATUS_DOT))
                    .child(text(label, FONT_XS, palette.text_muted)),
            )
            .into_any_element()
    }

    fn token_field(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let eye = if self.token_revealed {
            Icon::EyeOff
        } else {
            Icon::Eye
        };
        let value_box = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .bg(palette.shell)
            .border(BORDER_THIN)
            .border_color(palette.border_input)
            .rounded(radius(Radius::Md))
            .px(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Xs, density))
            .child(div().flex_1().min_w(px(0.0)).child(self.token.clone()))
            .child(
                div()
                    .id("donation-token-reveal")
                    .flex_none()
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_reveal(cx)))
                    .child(icon(eye, EYE_GLYPH, palette.text_faint)),
            );
        let hint_key = if self.has_token == Some(true) {
            "donation_token_hint_replace"
        } else {
            "donation_token_hint_new"
        };
        let token_page = div()
            .id("donation-token-page")
            .flex_none()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .cursor_pointer()
            .text_color(palette.brand)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.open_token_page(cx)))
            .child(text(
                tr!("donation_token_page_link"),
                FONT_XS,
                palette.brand,
            ))
            .child(icon(Icon::ExternalLink, LINK_GLYPH, palette.brand));
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(mono_label(tr!("donation_token_label"), palette))
            .child(value_box)
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(spacing(Spacing::Sm, density))
                    .child(
                        text(
                            tr!(hint_key, service = self.service_name()),
                            FONT_XS,
                            palette.text_muted,
                        )
                        .flex_1()
                        .min_w(px(0.0)),
                    )
                    .child(token_page),
            )
            .into_any_element()
    }

    fn jar_picker(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut list = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(mono_label(tr!("donation_jar_label"), palette));
        if self.jars.is_empty() {
            let hint = if self.busy == Some(Busy::LoadingJars) {
                tr!("donation_jar_loading")
            } else {
                tr!("donation_jar_hint")
            };
            return list
                .child(text(hint, FONT_XS, palette.text_muted))
                .into_any_element();
        }
        let shown = self.shown_jar().cloned();
        for jar in &self.jars {
            let selected = shown.as_ref() == Some(&jar.id);
            let jar_id = jar.id.clone();
            let row = radio_row(
                ElementId::Name(format!("donation-jar-{}", jar.id).into()),
                selected,
                palette.brand,
                jar_content(jar, density, palette),
                palette,
            )
            .disabled(self.busy.is_some())
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.pick_jar(jar_id.clone(), cx)),
            );
            list = list.child(row);
        }
        list.into_any_element()
    }

    fn outcome_row(&self, palette: &ForgePalette, density: Density) -> Option<AnyElement> {
        let (color, message): (Rgba, String) = match self.outcome.as_ref()? {
            Outcome::Success(message) => (palette.success, message.clone()),
            Outcome::Problem(message) => (palette.random, message.clone()),
        };
        Some(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xs, density))
                .child(status_dot(color, STATUS_DOT))
                .child(text(message, FONT_XS, color).flex_1().min_w(px(0.0)))
                .into_any_element(),
        )
    }

    fn action_row(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let check_label = match (self.is_monobank(), self.busy) {
            (true, Some(Busy::LoadingJars)) => tr!("donation_jar_loading_button"),
            (true, _) => tr!("donation_jar_load"),
            (false, Some(Busy::Checking)) => tr!("donation_token_checking"),
            (false, _) => tr!("donation_token_check"),
        };
        let save_label = if self.busy == Some(Busy::Saving) {
            tr!("donation_token_saving")
        } else if self.is_monobank() && self.jar_source == Some(JarSource::Stored) {
            tr!("donation_jar_save")
        } else {
            tr!("donation_token_save")
        };
        let check = secondary_button(check_label, palette)
            .density(density)
            .disabled(!self.can_check(cx))
            .busy(matches!(
                self.busy,
                Some(Busy::Checking | Busy::LoadingJars)
            ))
            .on_click(
                "donation-token-check",
                cx.listener(|this, _: &ClickEvent, _, cx| this.check_token(cx)),
            );
        let save = primary_button(save_label, palette)
            .density(density)
            .disabled(!self.can_save(cx))
            .busy(self.busy == Some(Busy::Saving))
            .on_click(
                "donation-token-save",
                cx.listener(|this, _: &ClickEvent, _, cx| this.save(cx)),
            );
        let mut left = div().flex().items_center();
        if self.has_token == Some(true) {
            left = left.child(
                ghost_button_with_icon(Icon::Trash, tr!("donation_token_remove"), palette)
                    .density(density)
                    .ink(palette.random)
                    .disabled(self.busy.is_some())
                    .on_click(
                        "donation-token-remove",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.request_remove(cx)),
                    ),
            );
        }
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(spacing(Spacing::Sm, density))
            .child(left)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(check)
                    .child(save),
            )
            .into_any_element()
    }

    fn polling_section(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let current = self.poll_interval?;
        let segments = POLL_PRESETS_SECS
            .iter()
            .map(|secs| {
                let interval = Duration::from_secs(*secs);
                segment(
                    ElementId::Name(format!("donation-poll-{secs}").into()),
                    interval_label(*secs),
                    interval == current,
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.set_poll_interval(interval, cx)
                    }),
                )
            })
            .collect();
        Some(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density))
                .child(mono_label(tr!("donation_poll_label"), palette))
                .child(div().flex().child(segmented(segments, palette)))
                .child(text(
                    tr!("donation_poll_hint", seconds = current.as_secs() as i64),
                    FONT_XS,
                    palette.text_muted,
                ))
                .into_any_element(),
        )
    }
}

impl Render for DonationSettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        let mut body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .py(spacing(Spacing::Md, density))
            .px(spacing(Spacing::Md, density))
            .child(self.token_field(&palette, density, cx));
        if self.is_monobank() {
            body = body.child(self.jar_picker(&palette, density, cx));
        }
        body = body
            .children(self.outcome_row(&palette, density))
            .child(self.action_row(&palette, density, cx))
            .child(encryption_note(&palette, density));
        let mut shell = card_shell(&palette)
            .child(self.header(&palette, density))
            .child(divider(&palette))
            .child(body);
        if let Some(polling) = self.polling_section(&palette, density, cx) {
            shell = shell.child(divider(&palette)).child(
                div()
                    .w_full()
                    .py(spacing(Spacing::Md, density))
                    .px(spacing(Spacing::Md, density))
                    .child(polling),
            );
        }
        shell
    }
}

pub(crate) fn remove_confirm(
    view: &Entity<DonationSettingsView>,
    palette: &ForgePalette,
    cx: &App,
) -> Option<AnyElement> {
    let settings = view.read(cx);
    if !settings.is_remove_pending() {
        return None;
    }
    let body_key = if settings.is_monobank() {
        "donation_token_remove_body_monobank"
    } else {
        "donation_token_remove_body"
    };
    let name = settings.service_name();
    let cancel_view = view.clone();
    let confirm_view = view.clone();
    let dismiss_view = view.clone();
    let card = confirm_modal(
        tr!("donation_token_remove_title"),
        tr!(body_key),
        ConfirmTone::Destructive,
        palette,
    )
    .item_name(name)
    .on_cancel(
        "donation-token-remove-cancel",
        tr!("widget_confirm_cancel"),
        move |_: &ClickEvent, _, cx| {
            cancel_view.update(cx, |this, cx| this.cancel_remove(cx));
        },
    )
    .on_confirm(
        "donation-token-remove-confirm",
        tr!("donation_token_remove"),
        move |_: &ClickEvent, _, cx| {
            confirm_view.update(cx, |this, cx| this.confirm_remove(cx));
        },
    );
    Some(
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("donation-token-remove-scrim", move |_window, cx| {
                dismiss_view.update(cx, |this, cx| this.cancel_remove(cx));
            })
            .into_any_element(),
    )
}

fn jar_content(jar: &MonobankJar, density: Density, palette: &ForgePalette) -> Div {
    div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, density))
        .child(text(jar_title(jar), FONT_SM, palette.text_primary))
        .children(jar_meta(jar).map(|meta| mono_text(meta, palette.text_muted)))
}

fn mono_text(content: SharedString, color: Rgba) -> Div {
    div()
        .font_family(mono_family())
        .text_size(FONT_XXS)
        .text_color(color)
        .child(content)
}

fn jar_meta(jar: &MonobankJar) -> Option<SharedString> {
    let currency = jar.currency.as_ref().map(ToString::to_string);
    let goal = jar
        .goal
        .as_ref()
        .map(|goal| tr!("donation_jar_goal", goal = goal.formatted()));
    let parts: Vec<String> = currency.into_iter().chain(goal).collect();
    (!parts.is_empty()).then(|| parts.join(" \u{00b7} ").into())
}

fn interval_label(secs: u64) -> String {
    if secs >= SECONDS_PER_MINUTE && secs.is_multiple_of(SECONDS_PER_MINUTE) {
        tr!(
            "donation_poll_minutes",
            minutes = (secs / SECONDS_PER_MINUTE) as i64
        )
    } else {
        tr!("donation_poll_seconds", seconds = secs as i64)
    }
}

fn encryption_note(palette: &ForgePalette, density: Density) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xxs, density))
        .child(icon(Icon::Lock, LINK_GLYPH, palette.success))
        .child(text(
            tr!("donation_token_encrypted_note"),
            FONT_XS,
            palette.text_muted,
        ))
}

fn mono_label(label: String, palette: &ForgePalette) -> Div {
    div()
        .font_family(mono_family())
        .text_size(FONT_XXS)
        .text_color(palette.text_muted)
        .child(label.to_uppercase())
}

fn text(content: impl Into<SharedString>, size: Pixels, color: Rgba) -> Div {
    div()
        .font_family(body_family())
        .text_size(size)
        .text_color(color)
        .child(content.into())
}
