use gpui::{
    App, AppContext, ClickEvent, Context, ElementId, Entity, EventEmitter, FontWeight,
    InteractiveElement, IntoElement, ParentElement, Pixels, Render, Rgba, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Window, div, px,
};

use crate::buttons::secondary_button;
use crate::icons::{Icon, icon};
use crate::palette::{ForgePalette, with_alpha};
use crate::text_input::{InputEvent, TextInput};
use crate::tokens::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_MD, FONT_SM, FONT_XS,
    Radius, Spacing, radius, spacing,
};

const HEADER_GAP: Pixels = px(12.0);
const ICON_TILE: Pixels = px(36.0);
const ICON_TILE_GLYPH: Pixels = px(20.0);
const ICON_TILE_ALPHA: f32 = 0.12;
const SECTION_STACK_GAP: Pixels = px(8.0);
const BULLET_GLYPH: Pixels = px(14.0);
const ESC_GLYPH: Pixels = px(12.0);
const ESC_GAP: Pixels = px(5.0);
const ACTIONS_GAP: Pixels = px(8.0);
const CHIP_PAD_Y: Pixels = px(1.0);
const ACCENT_HOVER_ALPHA: f32 = 0.92;
const CARD_WIDTH: Pixels = px(520.0);

type ActionHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulletKind {
    Check,
    Warning,
    Info,
}

pub struct BulletItem {
    pub kind: BulletKind,
    pub text: SharedString,
}

impl BulletItem {
    pub fn new(kind: BulletKind, text: impl Into<SharedString>) -> Self {
        Self {
            kind,
            text: text.into(),
        }
    }
}

/// `Confirmed` is emitted only while the typed phrase matches the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeToConfirmEvent {
    Confirmed,
    Cancelled,
}

pub(crate) fn bullet_glyph(kind: BulletKind, palette: &ForgePalette) -> (Icon, Rgba) {
    match kind {
        BulletKind::Check => (Icon::CircleCheck, palette.success),
        BulletKind::Warning => (Icon::AlertTriangle, palette.warning),
        BulletKind::Info => (Icon::InfoCircle, palette.info),
    }
}

/// Exact, case-sensitive, untrimmed match - the confirm gate is live only when this holds.
pub(crate) fn matches(typed: &str, target: &str) -> bool {
    typed == target
}

fn match_border(matched: bool, palette: &ForgePalette) -> Rgba {
    if matched {
        palette.brand
    } else {
        palette.border_input
    }
}

fn sp(s: Spacing) -> Pixels {
    spacing(s, Density::Cozy)
}

/// The binary must call `bind_text_input_keys` once at boot for the input's editing keys.
pub struct TypeToConfirm {
    input: Entity<TextInput>,
    target: SharedString,
    title: SharedString,
    explanation: SharedString,
    section_caption: SharedString,
    instruction_prefix: SharedString,
    instruction_suffix: SharedString,
    bullets: Vec<BulletItem>,
    confirm_label: SharedString,
    cancel_label: SharedString,
    esc_hint: Option<SharedString>,
    palette: ForgePalette,
    matched: bool,
    _input_sub: Subscription,
}

impl EventEmitter<TypeToConfirmEvent> for TypeToConfirm {}

pub fn type_to_confirm(
    target: impl Into<SharedString>,
    palette: &ForgePalette,
    cx: &mut Context<TypeToConfirm>,
) -> TypeToConfirm {
    let target = target.into();
    let palette = *palette;
    let matched = matches("", target.as_ref());

    let input = cx.new(|cx| {
        TextInput::new("", cx)
            .with_palette(palette)
            .with_font_size(FONT_SM)
            .static_chrome(match_border(matched, &palette), Radius::Md)
    });

    let sub = cx.subscribe(&input, |this, input, event: &InputEvent, cx| match event {
        InputEvent::Changed(text) => {
            let now = matches(text.as_ref(), this.target.as_ref());
            if now != this.matched {
                this.matched = now;
                let border = match_border(now, &this.palette);
                input.update(cx, |inp, cx| {
                    inp.set_static_chrome(Some((border, Radius::Md)));
                    cx.notify();
                });
                cx.notify();
            }
        }
        InputEvent::Submitted(_) => {
            if this.matched {
                cx.emit(TypeToConfirmEvent::Confirmed);
            }
        }
        InputEvent::Cancelled => cx.emit(TypeToConfirmEvent::Cancelled),
    });

    TypeToConfirm {
        input,
        target,
        title: SharedString::default(),
        explanation: SharedString::default(),
        section_caption: SharedString::default(),
        instruction_prefix: SharedString::default(),
        instruction_suffix: SharedString::default(),
        bullets: Vec::new(),
        confirm_label: SharedString::new_static("Confirm"),
        cancel_label: SharedString::new_static("Cancel"),
        esc_hint: None,
        palette,
        matched,
        _input_sub: sub,
    }
}

impl TypeToConfirm {
    #[must_use]
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = title.into();
        self
    }

    #[must_use]
    pub fn explanation(mut self, explanation: impl Into<SharedString>) -> Self {
        self.explanation = explanation.into();
        self
    }

    #[must_use]
    pub fn section_caption(mut self, caption: impl Into<SharedString>) -> Self {
        self.section_caption = caption.into();
        self
    }

    #[must_use]
    pub fn instruction(
        mut self,
        prefix: impl Into<SharedString>,
        suffix: impl Into<SharedString>,
    ) -> Self {
        self.instruction_prefix = prefix.into();
        self.instruction_suffix = suffix.into();
        self
    }

    #[must_use]
    pub fn bullet(mut self, kind: BulletKind, text: impl Into<SharedString>) -> Self {
        self.bullets.push(BulletItem::new(kind, text));
        self
    }

    #[must_use]
    pub fn bullets(mut self, bullets: Vec<BulletItem>) -> Self {
        self.bullets = bullets;
        self
    }

    #[must_use]
    pub fn confirm_label(mut self, label: impl Into<SharedString>) -> Self {
        self.confirm_label = label.into();
        self
    }

    #[must_use]
    pub fn cancel_label(mut self, label: impl Into<SharedString>) -> Self {
        self.cancel_label = label.into();
        self
    }

    #[must_use]
    pub fn esc_hint(mut self, phrase: impl Into<SharedString>) -> Self {
        self.esc_hint = Some(phrase.into());
        self
    }

    pub fn focus_input(&self, window: &mut Window, cx: &mut App) {
        self.input.read(cx).focus(window);
    }
}

fn confirm_button(
    id: ElementId,
    label: SharedString,
    palette: &ForgePalette,
    enabled: bool,
    handler: ActionHandler,
) -> impl IntoElement {
    let accent = palette.warning;
    let ink = palette.shell;
    let disabled_bg = palette.surface_overlay;
    let disabled_ink = palette.disabled;

    let base = div()
        .flex()
        .items_center()
        .py(sp(Spacing::Xxs))
        .px(sp(Spacing::Sm))
        .rounded(radius(Radius::Md))
        .font_family(DEFAULT_BODY_FAMILY)
        .text_size(FONT_SM);

    if enabled {
        base.id(id)
            .bg(accent)
            .font_weight(FontWeight::MEDIUM)
            .text_color(ink)
            .cursor_pointer()
            .hover(move |style| style.bg(with_alpha(accent, ACCENT_HOVER_ALPHA)))
            .on_click(handler)
            .child(label)
            .into_any_element()
    } else {
        base.bg(disabled_bg)
            .text_color(disabled_ink)
            .child(label)
            .into_any_element()
    }
}

impl Render for TypeToConfirm {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let entity = cx.entity();

        let tile = div()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(ICON_TILE)
            .rounded(radius(Radius::Md))
            .bg(with_alpha(p.warning, ICON_TILE_ALPHA))
            .child(icon(Icon::AlertTriangle, ICON_TILE_GLYPH, p.warning));

        let title_row = div()
            .flex()
            .items_center()
            .gap(HEADER_GAP)
            .child(tile)
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_MD)
                    .text_color(p.text_primary)
                    .child(self.title.clone()),
            );

        let header = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(SECTION_STACK_GAP)
            .py(sp(Spacing::Md))
            .px(sp(Spacing::Lg))
            .child(title_row)
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(p.text_muted)
                    .child(self.explanation.clone()),
            );

        let mut risk = div()
            .w_full()
            .flex()
            .flex_col()
            .py(sp(Spacing::Md))
            .px(sp(Spacing::Lg))
            .bg(p.shell)
            .border_t(BORDER_THIN)
            .border_color(p.border_regular);

        if !self.section_caption.is_empty() {
            risk = risk.child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(p.text_muted)
                    .child(self.section_caption.clone()),
            );
        }
        for item in &self.bullets {
            let (glyph, hue) = bullet_glyph(item.kind, &p);
            risk = risk.child(
                div()
                    .flex()
                    .items_start()
                    .gap(sp(Spacing::Sm))
                    .py(sp(Spacing::Xxs))
                    .child(icon(glyph, BULLET_GLYPH, hue))
                    .child(
                        div()
                            .flex_1()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_SM)
                            .text_color(p.text_primary)
                            .child(item.text.clone()),
                    ),
            );
        }

        let phrase_chip = div()
            .flex_none()
            .py(CHIP_PAD_Y)
            .px(sp(Spacing::Xs))
            .rounded(radius(Radius::Sm))
            .bg(p.surface_overlay)
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_SM)
            .text_color(p.warning)
            .child(self.target.clone());

        let instruction = div()
            .flex()
            .items_center()
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(p.text_primary)
                    .child(format!("{} ", self.instruction_prefix)),
            )
            .child(phrase_chip)
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(p.text_primary)
                    .child(format!(" {}", self.instruction_suffix)),
            );

        let confirm_band = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(SECTION_STACK_GAP)
            .py(sp(Spacing::Md))
            .px(sp(Spacing::Lg))
            .border_t(BORDER_THIN)
            .border_color(p.border_regular)
            .child(instruction)
            .child(self.input.clone());

        let esc = match &self.esc_hint {
            Some(phrase) => div()
                .flex()
                .items_center()
                .gap(ESC_GAP)
                .child(icon(Icon::Keyboard, ESC_GLYPH, p.text_faint))
                .child(
                    div()
                        .font_family(DEFAULT_MONO_FAMILY)
                        .text_size(FONT_XS)
                        .text_color(p.text_faint)
                        .child("Esc"),
                )
                .child(
                    div()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .text_size(FONT_XS)
                        .text_color(p.text_faint)
                        .child(format!(" {phrase}")),
                )
                .into_any_element(),
            None => div().into_any_element(),
        };

        let cancel_entity = entity.clone();
        let cancel = secondary_button(self.cancel_label.clone(), &p).on_click(
            "type-to-confirm-cancel",
            move |_event, _window, app| {
                cancel_entity.update(app, |_this, cx| cx.emit(TypeToConfirmEvent::Cancelled));
            },
        );

        let confirm_entity = entity.clone();
        let confirm_handler: ActionHandler = Box::new(move |_event, _window, app| {
            confirm_entity.update(app, |_this, cx| cx.emit(TypeToConfirmEvent::Confirmed));
        });
        let confirm = confirm_button(
            "type-to-confirm-confirm".into(),
            self.confirm_label.clone(),
            &p,
            self.matched,
            confirm_handler,
        );

        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .py(sp(Spacing::Sm))
            .px(sp(Spacing::Lg))
            .bg(p.shell)
            .border_t(BORDER_THIN)
            .border_color(p.border_regular)
            .child(esc)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(ACTIONS_GAP)
                    .child(cancel)
                    .child(confirm),
            );

        div()
            .flex()
            .flex_col()
            .w(CARD_WIDTH)
            .bg(p.elevated)
            .rounded(radius(Radius::Lg))
            .overflow_hidden()
            .border(BORDER_THIN)
            .border_color(p.border_input)
            .child(header)
            .child(risk)
            .child(confirm_band)
            .child(footer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;

    /// Whether two colours are identical channel-wise. `Rgba` carries no `Debug`, so
    /// this stands in for the `assert_eq!` the tests below would otherwise reach for.
    fn same_rgba(a: Rgba, b: Rgba) -> bool {
        a.r == b.r && a.g == b.g && a.b == b.b && a.a == b.a
    }

    #[test]
    fn matches_holds_only_on_an_exact_case_sensitive_untrimmed_pair() {
        // The confirm gate is an exact, case-sensitive, untrimmed compare. Every
        // `false` row would flip to `true` under a naive `.trim()` or case-folding
        // implementation - that divergence is the signal these rows pin.
        for (typed, target, expected) in [
            ("delete my-channel", "delete my-channel", true), // exact match clears the gate
            ("delete my-channe", "delete my-channel", false), // a different string does not
            ("  target  ", "target", false),                  // no-trim: surrounding space matters
            ("Target", "target", false),                      // case-sensitive
            ("", "", true),                                   // empty target is trivially satisfied
        ] {
            assert_eq!(
                matches(typed, target),
                expected,
                "matches({typed:?}, {target:?})",
            );
        }
    }

    #[test]
    fn match_border_is_brand_when_matched_and_border_input_otherwise() {
        let p = CATPPUCCIN_MOCHA;
        // Guard: the two hues are distinct, so a swapped `if`/`else` returns a
        // detectably wrong colour rather than the same value on both arms.
        assert!(!same_rgba(p.brand, p.border_input));
        assert!(same_rgba(match_border(true, &p), p.brand));
        assert!(same_rgba(match_border(false, &p), p.border_input));
    }

    #[test]
    fn bullet_glyph_pairs_each_kind_with_its_tone_glyph_and_hue() {
        let p = CATPPUCCIN_MOCHA;
        // Guard: the three tone hues are distinct, so asserting the per-kind hue below
        // has teeth - a swapped arm resolves to a different, detectable colour.
        assert!(!same_rgba(p.success, p.warning));
        assert!(!same_rgba(p.warning, p.info));
        assert!(!same_rgba(p.success, p.info));
        for (kind, glyph, hue) in [
            (BulletKind::Check, Icon::CircleCheck, p.success),
            (BulletKind::Warning, Icon::AlertTriangle, p.warning),
            (BulletKind::Info, Icon::InfoCircle, p.info),
        ] {
            let (got_glyph, got_hue) = bullet_glyph(kind, &p);
            assert_eq!(got_glyph, glyph, "glyph for {kind:?}");
            assert!(same_rgba(got_hue, hue), "hue for {kind:?}");
        }
    }
}
