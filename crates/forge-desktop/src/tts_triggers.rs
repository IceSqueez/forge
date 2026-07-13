use forge_components::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_MD, FONT_SM, FONT_XS,
    ForgePalette, Icon, Radius, Spacing, badge, card, icon, radius, spacing, toggle,
};
use gpui::{AnyElement, ClickEvent, Context, Pixels, Rgba, Window, div, prelude::*, px};

use crate::presentation::ActivePresentation;

/// Source/format icon-chip side — the parity source pins the leading glyph tile at a
/// fixed 30px square, off the `Spacing` scale, so it is carried as a named literal.
const ICON_CHIP: Pixels = px(30.0);
/// Minimum-bits value chip vertical inset (the source's fixed 3px pad).
const BITS_CHIP_PAD_V: Pixels = px(3.0);
/// Minimum-bits value chip horizontal inset (the source's fixed 9px pad).
const BITS_CHIP_PAD_H: Pixels = px(9.0);
/// Format / queue panel vertical inset — the source pins these two cards at a fixed
/// 13px, off the `Spacing` scale, so it is carried as a named literal.
const PANEL_PAD_V: Pixels = px(13.0);
/// Format / queue panel horizontal inset (the source's fixed 14px pad).
const PANEL_PAD_H: Pixels = px(14.0);

/// The TTS Triggers section view-entity: a "what gets spoken" header over three
/// full-width rows of paired cards — a chat-command source and a channel-points
/// source, a bits source and a sub-messages source, then a message-format panel and
/// a queue-behavior panel.
///
/// Owns the seven source/format toggles as local `bool` state, seeded from the
/// trigger-settings defaults. `forge-desktop` wires no TTS-trigger repo yet, so the
/// role chips, cooldown meta and the minimum-bits / template / queue-limit values are
/// static display strings and each toggle flips its cached flag. The real screen loads
/// the settings from `forge-storage`'s trigger-settings repo over the runtime→UI
/// bridge, and a toggle persists through that repo's handle and hot-swaps the live
/// speak-queue trigger config.
pub struct TtsTriggersView {
    command_enabled: bool,
    channel_points_enabled: bool,
    bits_enabled: bool,
    sub_messages_enabled: bool,
    read_username: bool,
    speak_emotes: bool,
    bits_skip_line: bool,
}

impl TtsTriggersView {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            command_enabled: true,
            channel_points_enabled: true,
            bits_enabled: true,
            sub_messages_enabled: false,
            read_username: true,
            speak_emotes: false,
            bits_skip_line: true,
        }
    }

    // --- toggle handlers (view-state stubs) -------------------------------

    fn toggle_command(&mut self, cx: &mut Context<Self>) {
        self.command_enabled = !self.command_enabled;
        cx.notify();
    }

    fn toggle_channel_points(&mut self, cx: &mut Context<Self>) {
        self.channel_points_enabled = !self.channel_points_enabled;
        cx.notify();
    }

    fn toggle_bits(&mut self, cx: &mut Context<Self>) {
        self.bits_enabled = !self.bits_enabled;
        cx.notify();
    }

    fn toggle_subs(&mut self, cx: &mut Context<Self>) {
        self.sub_messages_enabled = !self.sub_messages_enabled;
        cx.notify();
    }

    fn toggle_read_username(&mut self, cx: &mut Context<Self>) {
        self.read_username = !self.read_username;
        cx.notify();
    }

    fn toggle_speak_emotes(&mut self, cx: &mut Context<Self>) {
        self.speak_emotes = !self.speak_emotes;
        cx.notify();
    }

    fn toggle_bits_skip_line(&mut self, cx: &mut Context<Self>) {
        self.bits_skip_line = !self.bits_skip_line;
        cx.notify();
    }

    // --- header -----------------------------------------------------------

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

    /// A source card's header row: the leading glyph tile, a title over a subtitle,
    /// and the trailing enable toggle. The subtitle inks the monospace family only for
    /// the command card (its subtitle is the literal `!tts <message>` invocation).
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

    // --- source cards -----------------------------------------------------

    /// The chat-command source card: a raw bordered container whose border inks the
    /// brand accent while the command is enabled, falling back to the regular border
    /// otherwise — the enable state recolors the whole card frame.
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

        // The disabled note only appears while sub messages are off; when enabled the
        // card carries just its header, matching the source.
        let disabled_note = (!self.sub_messages_enabled).then(|| {
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child("Disabled — toggle to enable")
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

    // --- format + queue panels --------------------------------------------

    /// One label-fills / toggle-trailing settings row inside the format and queue
    /// panels.
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
                    .child(row1)
                    .child(row2)
                    .child(row3),
            )
    }
}

// ── view-specific fragments ───────────────────────────────────────────────

/// Wraps a card so it takes an equal half of a two-up row (the source pins each card
/// to fill its half of the full-width row).
fn half(card: AnyElement) -> AnyElement {
    div().flex_1().min_w(px(0.0)).child(card).into_any_element()
}

/// The leading glyph tile shared by every source card: a fixed-square,
/// `surface_overlay`-filled, `Radius::Sm` chip centering `glyph`.
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

/// A trigger-role chip: a `surface_overlay`-filled badge inking `color`.
fn role_chip(label: &'static str, color: Rgba, palette: &ForgePalette) -> AnyElement {
    badge(palette.surface_overlay, color, label, false, FONT_XS).into_any_element()
}

/// An uppercase monospace panel caption inking `text_muted`.
fn panel_header(label: &'static str, palette: &ForgePalette) -> AnyElement {
    div()
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_XS)
        .text_color(palette.text_muted)
        .child(label)
        .into_any_element()
}

/// A full-width hairline standing in for the source's horizontal divider.
fn hairline(palette: &ForgePalette) -> AnyElement {
    div()
        .w_full()
        .h(BORDER_THIN)
        .bg(palette.border_regular)
        .into_any_element()
}

/// One label-fills / static-value-trailing row inside the queue-behavior panel: the
/// value renders in a `shell`-filled `Radius::Sm` mono chip.
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
