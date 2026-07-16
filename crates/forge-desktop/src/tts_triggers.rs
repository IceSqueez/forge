use std::sync::Arc;

use forge_components::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_MD, FONT_SM, FONT_XS,
    ForgePalette, Icon, Radius, Spacing, badge, card, icon, radius, spacing, toggle, with_alpha,
};
use forge_runtime::TtsTriggerSettingsHandle;
use forge_storage::{TtsTriggerSettings, TtsTriggerSettingsRepo};
use gpui::{AnyElement, ClickEvent, Context, Pixels, Rgba, Window, div, prelude::*, px};

use crate::presentation::ActivePresentation;

const ICON_CHIP: Pixels = px(30.0);
const BITS_CHIP_PAD_V: Pixels = px(3.0);
const BITS_CHIP_PAD_H: Pixels = px(9.0);
const PANEL_PAD_V: Pixels = px(13.0);
const PANEL_PAD_H: Pixels = px(14.0);

pub struct TtsTriggersView {
    repo: Arc<dyn TtsTriggerSettingsRepo>,
    settings_handle: TtsTriggerSettingsHandle,
    rt_handle: tokio::runtime::Handle,
    command_enabled: bool,
    channel_points_enabled: bool,
    bits_enabled: bool,
    sub_messages_enabled: bool,
    read_username: bool,
    speak_emotes: bool,
    bits_skip_line: bool,
    save_error: Option<String>,
}

impl TtsTriggersView {
    pub fn new(
        repo: Arc<dyn TtsTriggerSettingsRepo>,
        settings_handle: TtsTriggerSettingsHandle,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let defaults = TtsTriggerSettings::default();
        let view = Self {
            repo,
            settings_handle,
            rt_handle,
            command_enabled: defaults.command_enabled,
            channel_points_enabled: defaults.channel_points_enabled,
            bits_enabled: defaults.bits_enabled,
            sub_messages_enabled: defaults.sub_messages_enabled,
            read_username: defaults.read_username,
            speak_emotes: defaults.speak_emotes,
            bits_skip_line: defaults.bits_skip_line,
            save_error: None,
        };
        view.reload(cx);
        view
    }

    fn reload(&self, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.repo);
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<TtsTriggerSettings, String>>();
        self.rt_handle.spawn(async move {
            let _ = tx.send(repo.get_trigger_settings().await.map_err(|e| e.to_string()));
        });
        cx.spawn(async move |this, cx| match rx.await {
            Ok(Ok(settings)) => {
                let _ = this.update(cx, |this, cx| this.apply_loaded(settings, cx));
            }
            Ok(Err(message)) => {
                eprintln!("forge-desktop: tts trigger settings load failed: {message}");
            }
            Err(_) => {}
        })
        .detach();
    }

    fn apply_loaded(&mut self, settings: TtsTriggerSettings, cx: &mut Context<Self>) {
        self.command_enabled = settings.command_enabled;
        self.channel_points_enabled = settings.channel_points_enabled;
        self.bits_enabled = settings.bits_enabled;
        self.sub_messages_enabled = settings.sub_messages_enabled;
        self.read_username = settings.read_username;
        self.speak_emotes = settings.speak_emotes;
        self.bits_skip_line = settings.bits_skip_line;
        self.save_error = None;
        cx.notify();
    }

    fn to_settings(&self) -> TtsTriggerSettings {
        TtsTriggerSettings {
            command_enabled: self.command_enabled,
            channel_points_enabled: self.channel_points_enabled,
            bits_enabled: self.bits_enabled,
            sub_messages_enabled: self.sub_messages_enabled,
            read_username: self.read_username,
            speak_emotes: self.speak_emotes,
            bits_skip_line: self.bits_skip_line,
        }
    }

    fn persist(&self, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.repo);
        let handle = self.settings_handle.clone();
        let settings = self.to_settings();
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<(), String>>();
        self.rt_handle.spawn(async move {
            let outcome = match repo.set_trigger_settings(&settings).await {
                Ok(()) => {
                    handle.swap(settings);
                    Ok(())
                }
                Err(e) => {
                    let message = e.to_string();
                    eprintln!("forge-desktop: tts trigger settings persist failed: {message}");
                    Err(message)
                }
            };
            let _ = tx.send(outcome);
        });
        cx.spawn(async move |this, cx| match rx.await {
            Ok(Ok(())) => {
                let _ = this.update(cx, |this, cx| {
                    this.save_error = None;
                    cx.notify();
                });
            }
            Ok(Err(message)) => {
                let _ = this.update(cx, |this, cx| {
                    this.save_error = Some(message);
                    cx.notify();
                });
            }
            Err(_) => {}
        })
        .detach();
    }

    fn toggle_command(&mut self, cx: &mut Context<Self>) {
        self.command_enabled = !self.command_enabled;
        cx.notify();
        self.persist(cx);
    }

    fn toggle_channel_points(&mut self, cx: &mut Context<Self>) {
        self.channel_points_enabled = !self.channel_points_enabled;
        cx.notify();
        self.persist(cx);
    }

    fn toggle_bits(&mut self, cx: &mut Context<Self>) {
        self.bits_enabled = !self.bits_enabled;
        cx.notify();
        self.persist(cx);
    }

    fn toggle_subs(&mut self, cx: &mut Context<Self>) {
        self.sub_messages_enabled = !self.sub_messages_enabled;
        cx.notify();
        self.persist(cx);
    }

    fn toggle_read_username(&mut self, cx: &mut Context<Self>) {
        self.read_username = !self.read_username;
        cx.notify();
        self.persist(cx);
    }

    fn toggle_speak_emotes(&mut self, cx: &mut Context<Self>) {
        self.speak_emotes = !self.speak_emotes;
        cx.notify();
        self.persist(cx);
    }

    fn toggle_bits_skip_line(&mut self, cx: &mut Context<Self>) {
        self.bits_skip_line = !self.bits_skip_line;
        cx.notify();
        self.persist(cx);
    }

    fn header_group(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child("WHAT GETS SPOKEN"),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_muted)
                    .child("Enable sources and set who can trigger them"),
            )
            .into_any_element()
    }

    fn error_banner(&self, palette: &ForgePalette, density: Density) -> Option<AnyElement> {
        self.save_error.as_ref().map(|err| {
            div()
                .w_full()
                .py(spacing(Spacing::Xs, density))
                .px(spacing(Spacing::Sm, density))
                .rounded(radius(Radius::Sm))
                .border(BORDER_THIN)
                .border_color(palette.random)
                .bg(with_alpha(palette.random, 0.1))
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.random)
                .child(err.clone())
                .into_any_element()
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn card_header(
        &self,
        chip: AnyElement,
        title: &'static str,
        subtitle: &'static str,
        subtitle_mono: bool,
        toggle_el: AnyElement,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        let subtitle_family = if subtitle_mono {
            DEFAULT_MONO_FAMILY
        } else {
            DEFAULT_BODY_FAMILY
        };
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(chip)
            .child(
                div()
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
                            .child(title),
                    )
                    .child(
                        div()
                            .font_family(subtitle_family)
                            .text_size(FONT_XS)
                            .text_color(palette.text_muted)
                            .child(subtitle),
                    ),
            )
            .child(toggle_el)
            .into_any_element()
    }

    fn trigger_card_command(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let border_color = if self.command_enabled {
            palette.brand
        } else {
            palette.border_regular
        };

        let chip = chip_30(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.brand)
                .child("!"),
            palette,
        );
        let toggle_el = toggle(self.command_enabled, palette)
            .on_click(
                "tts-trig-command",
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_command(cx)),
            )
            .into_any_element();
        let header = self.card_header(
            chip,
            "Chat command",
            "!tts <message>",
            true,
            toggle_el,
            palette,
            density,
        );

        let chips = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(spacing(Spacing::Xs, density))
            .child(role_chip("Subscribers", palette.success, palette))
            .child(role_chip("VIPs", palette.brand, palette))
            .child(role_chip("Mods", palette.warning, palette));

        let meta = div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XS)
            .text_color(palette.text_muted)
            .child("cooldown 8s · max 250 chars");

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Lg))
            .border(BORDER_THIN)
            .border_color(border_color)
            .bg(palette.elevated)
            .child(header)
            .child(chips)
            .child(meta)
            .into_any_element()
    }

    fn trigger_card_channel_points(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chip = chip_30(icon(Icon::Diamond, FONT_MD, palette.brand), palette);
        let toggle_el = toggle(self.channel_points_enabled, palette)
            .on_click(
                "tts-trig-points",
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_channel_points(cx)),
            )
            .into_any_element();
        let header = self.card_header(
            chip,
            "Channel point reward",
            "\"Speak my message\" · 500 pts",
            false,
            toggle_el,
            palette,
            density,
        );

        let chips = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(spacing(Spacing::Xs, density))
            .child(role_chip("Everyone", palette.text_primary, palette));

        let meta = div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XS)
            .text_color(palette.text_muted)
            .child("no cooldown · priority queue");

        card(
            div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density))
                .child(header)
                .child(chips)
                .child(meta),
            palette,
        )
        .radius(Radius::Lg)
        .padding_xy(spacing(Spacing::Sm, density), spacing(Spacing::Sm, density))
        .full_width()
        .into_any_element()
    }

    fn trigger_card_bits(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chip = chip_30(icon(Icon::Diamond, FONT_MD, palette.warning), palette);
        let toggle_el = toggle(self.bits_enabled, palette)
            .on_click(
                "tts-trig-bits",
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_bits(cx)),
            )
            .into_any_element();
        let header = self.card_header(
            chip,
            "Bits / cheers",
            "Speak cheer message",
            false,
            toggle_el,
            palette,
            density,
        );

        let min_bits = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child("Minimum"),
            )
            .child(
                card(
                    div()
                        .font_family(DEFAULT_MONO_FAMILY)
                        .text_size(FONT_SM)
                        .text_color(palette.warning)
                        .child("100 bits"),
                    palette,
                )
                .background(palette.shell)
                .radius(Radius::Sm)
                .padding_xy(BITS_CHIP_PAD_V, BITS_CHIP_PAD_H),
            );

        let meta = div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XS)
            .text_color(palette.text_muted)
            .child("louder = longer message");

        card(
            div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density))
                .child(header)
                .child(min_bits)
                .child(meta),
            palette,
        )
        .radius(Radius::Lg)
        .padding_xy(spacing(Spacing::Sm, density), spacing(Spacing::Sm, density))
        .full_width()
        .into_any_element()
    }

    fn trigger_card_subs(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chip = chip_30(icon(Icon::Star, FONT_MD, palette.brand), palette);
        let toggle_el = toggle(self.sub_messages_enabled, palette)
            .on_click(
                "tts-trig-subs",
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_subs(cx)),
            )
            .into_any_element();
        let header = self.card_header(
            chip,
            "Sub messages",
            "Speak resub / gift messages",
            false,
            toggle_el,
            palette,
            density,
        );

        let disabled_note = (!self.sub_messages_enabled).then(|| {
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child("Disabled - toggle to enable")
        });

        card(
            div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density))
                .child(header)
                .children(disabled_note),
            palette,
        )
        .radius(Radius::Lg)
        .padding_xy(spacing(Spacing::Sm, density), spacing(Spacing::Sm, density))
        .full_width()
        .into_any_element()
    }

    fn toggle_row(
        &self,
        label: &'static str,
        on: bool,
        id: &'static str,
        palette: &ForgePalette,
        density: Density,
        handler: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> AnyElement {
        div()
            .w_full()
            .flex()
            .items_center()
            .py(spacing(Spacing::Xs, density))
            .child(
                div()
                    .flex_1()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(label),
            )
            .child(toggle(on, palette).on_click(id, handler))
            .into_any_element()
    }

    fn format_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let header = panel_header("MESSAGE FORMAT", palette);

        let username_row = self.toggle_row(
            "Read username before message",
            self.read_username,
            "tts-trig-read-username",
            palette,
            density,
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_read_username(cx)),
        );

        let template_section = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(panel_header("TEMPLATE", palette))
            .child(
                card(
                    div()
                        .w_full()
                        .font_family(DEFAULT_MONO_FAMILY)
                        .text_size(FONT_SM)
                        .text_color(palette.text_primary)
                        .child("{user} says: {message}"),
                    palette,
                )
                .background(palette.shell)
                .radius(Radius::Sm)
                .padding_xy(spacing(Spacing::Xs, density), spacing(Spacing::Xs, density))
                .full_width(),
            );

        let emotes_row = self.toggle_row(
            "Speak emotes as words",
            self.speak_emotes,
            "tts-trig-speak-emotes",
            palette,
            density,
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_speak_emotes(cx)),
        );

        card(
            div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density))
                .child(header)
                .child(hairline(palette))
                .child(username_row)
                .child(hairline(palette))
                .child(template_section)
                .child(hairline(palette))
                .child(emotes_row),
            palette,
        )
        .radius(Radius::Lg)
        .padding_xy(PANEL_PAD_V, PANEL_PAD_H)
        .full_width()
        .into_any_element()
    }

    fn queue_behavior_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let header = panel_header("QUEUE BEHAVIOR", palette);

        let skip_row = self.toggle_row(
            "Bits & points skip the line",
            self.bits_skip_line,
            "tts-trig-bits-skip",
            palette,
            density,
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_bits_skip_line(cx)),
        );

        card(
            div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density))
                .child(header)
                .child(hairline(palette))
                .child(queue_value_row("Max queue length", "20", palette, density))
                .child(hairline(palette))
                .child(queue_value_row(
                    "Per-user limit in queue",
                    "2",
                    palette,
                    density,
                ))
                .child(hairline(palette))
                .child(skip_row),
            palette,
        )
        .radius(Radius::Lg)
        .padding_xy(PANEL_PAD_V, PANEL_PAD_H)
        .full_width()
        .into_any_element()
    }
}

impl Render for TtsTriggersView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let row1 = div()
            .w_full()
            .flex()
            .flex_row()
            .gap(spacing(Spacing::Sm, density))
            .child(half(self.trigger_card_command(&palette, density, cx)))
            .child(half(
                self.trigger_card_channel_points(&palette, density, cx),
            ));

        let row2 = div()
            .w_full()
            .flex()
            .flex_row()
            .gap(spacing(Spacing::Sm, density))
            .child(half(self.trigger_card_bits(&palette, density, cx)))
            .child(half(self.trigger_card_subs(&palette, density, cx)));

        let row3 = div()
            .w_full()
            .flex()
            .flex_row()
            .gap(spacing(Spacing::Sm, density))
            .child(half(self.format_card(&palette, density, cx)))
            .child(half(self.queue_behavior_card(&palette, density, cx)));

        div()
            .id("tts-triggers")
            .size_full()
            .overflow_y_scroll()
            .bg(palette.base)
            .p(spacing(Spacing::Md, density))
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(spacing(Spacing::Sm, density))
                    .child(self.header_group(&palette, density))
                    .children(self.error_banner(&palette, density))
                    .child(row1)
                    .child(row2)
                    .child(row3),
            )
    }
}

fn half(card: AnyElement) -> AnyElement {
    div().flex_1().min_w(px(0.0)).child(card).into_any_element()
}

fn chip_30(glyph: impl IntoElement, palette: &ForgePalette) -> AnyElement {
    div()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(ICON_CHIP)
        .rounded(radius(Radius::Sm))
        .bg(palette.surface_overlay)
        .child(glyph)
        .into_any_element()
}

fn role_chip(label: &'static str, color: Rgba, palette: &ForgePalette) -> AnyElement {
    badge(palette.surface_overlay, color, label, false, FONT_XS).into_any_element()
}

fn panel_header(label: &'static str, palette: &ForgePalette) -> AnyElement {
    div()
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_XS)
        .text_color(palette.text_muted)
        .child(label)
        .into_any_element()
}

fn hairline(palette: &ForgePalette) -> AnyElement {
    div()
        .w_full()
        .h(BORDER_THIN)
        .bg(palette.border_regular)
        .into_any_element()
}

fn queue_value_row(
    label: &'static str,
    value: &'static str,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    div()
        .w_full()
        .flex()
        .items_center()
        .py(spacing(Spacing::Xs, density))
        .child(
            div()
                .flex_1()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_primary)
                .child(label),
        )
        .child(
            card(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(value),
                palette,
            )
            .background(palette.shell)
            .radius(Radius::Sm)
            .padding_xy(
                spacing(Spacing::Xxs, density),
                spacing(Spacing::Xs, density),
            ),
        )
        .into_any_element()
}
