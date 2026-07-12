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

/// Gap between the alert tile and the title in the header row. Carried as a named
/// off-scale literal (12px, one notch above `Spacing::Sm`) rather than snapped onto
/// the scale — mirroring the off-scale literals the confirm dialog keeps.
const HEADER_GAP: Pixels = px(12.0);
/// Side of the square accent-tinted tile behind the alert glyph.
const ICON_TILE: Pixels = px(36.0);
/// Rendered size of the alert glyph centred in the tile.
const ICON_TILE_GLYPH: Pixels = px(20.0);
/// Alpha of the tone accent behind the alert glyph — a faint wash of the warning hue.
const ICON_TILE_ALPHA: f32 = 0.12;
/// Vertical gap inside the header and confirm bands, stacking a line over the line
/// below it. An off-scale literal (8px, between `Spacing::Xs` and `Spacing::Sm`).
const SECTION_STACK_GAP: Pixels = px(8.0);
/// Rendered size of the leading glyph on each impact bullet.
const BULLET_GLYPH: Pixels = px(14.0);
/// Rendered size of the keyboard glyph leading the inline Escape hint.
const ESC_GLYPH: Pixels = px(12.0);
/// Gap between the keyboard glyph, the `Esc` keycap and the hint phrase.
const ESC_GAP: Pixels = px(5.0);
/// Gap between the cancel and confirm buttons in the footer actions cluster.
const ACTIONS_GAP: Pixels = px(8.0);
/// Vertical inset of the monospace phrase chip wrapped in the instruction line — a
/// 1px off-scale literal that keeps the chip hugging its text.
const CHIP_PAD_Y: Pixels = px(1.0);
/// Alpha the enabled confirm button fades its warning fill to on hover.
const ACCENT_HOVER_ALPHA: f32 = 0.92;
/// Fixed card width. An off-scale literal (520px, between `ModalSize::Sm` 440 and
/// `Md` 540) carried as a named constant rather than snapped onto the `ModalSize`
/// scale — this dialog's width is pinned, not one of the shared modal envelopes.
const CARD_WIDTH: Pixels = px(520.0);

/// Boxed click handler for the confirm / cancel actions. gpui passes the click event
/// plus the window and app contexts, through which the closure reaches this dialog's
/// own entity to emit the resolving event.
type ActionHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// The tone of an impact bullet, fixing its leading glyph and hue: a reassuring
/// `Check` (green), a cautioning `Warning` (yellow), or a neutral `Info` (blue).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulletKind {
    Check,
    Warning,
    Info,
}

/// One line in the impact list — the cascade of what the confirmed action will
/// affect. The `kind` fixes the glyph and hue; `text` is the caller-supplied,
/// already-resolved description (the kit carries no localisation).
pub struct BulletItem {
    pub kind: BulletKind,
    pub text: SharedString,
}

impl BulletItem {
    /// Convenience constructor pairing a `kind` with its already-resolved `text`.
    pub fn new(kind: BulletKind, text: impl Into<SharedString>) -> Self {
        Self {
            kind,
            text: text.into(),
        }
    }
}

/// The event a [`TypeToConfirm`] dialog emits once the user resolves it: they either
/// confirmed (only reachable while the typed phrase matches the target) or cancelled.
/// The consuming screen `cx.subscribe`s to this and dismisses the dialog / carries out
/// the action accordingly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeToConfirmEvent {
    Confirmed,
    Cancelled,
}

/// Resolves a bullet's leading glyph and hue against the active theme. Kept internal
/// so the mapping is pinned in one place (and unit-testable in-crate).
pub(crate) fn bullet_glyph(kind: BulletKind, palette: &ForgePalette) -> (Icon, Rgba) {
    match kind {
        BulletKind::Check => (Icon::CircleCheck, palette.success),
        BulletKind::Warning => (Icon::AlertTriangle, palette.warning),
        BulletKind::Info => (Icon::InfoCircle, palette.info),
    }
}

/// Whether the typed text clears the confirmation gate: an exact, case-sensitive,
/// untrimmed match against the target phrase. Kept internal (and pure) so the enable
/// rule is unit-testable off the render tree — the confirm button is live only when
/// this holds.
pub(crate) fn matches(typed: &str, target: &str) -> bool {
    typed == target
}

/// The confirmation field's border hue for a given match state: `brand` once the
/// typed phrase matches the target, `border_input` otherwise. The field carries this
/// as a pinned static chrome, so the border is the match signal on the input itself.
fn match_border(matched: bool, palette: &ForgePalette) -> Rgba {
    if matched {
        palette.brand
    } else {
        palette.border_input
    }
}

/// Resolves a spacing token at the fixed default density. The dialog carries no
/// per-instance density knob (its bands are chrome, sized once), so every inset snaps
/// to the `Spacing` scale at `Cozy` — the density-neutral multiplier.
fn sp(s: Spacing) -> Pixels {
    spacing(s, Density::Cozy)
}

/// A destructive-action confirmation dialog that gates its confirm button behind the
/// user re-typing a target phrase exactly. It is a stateful mini-view: it owns a child
/// [`TextInput`] plus the target phrase, recomputes whether they match on every input
/// change, and emits [`TypeToConfirmEvent`] when the user resolves it.
///
/// The card lays out four bands on the shared modal shell (`elevated` fill, hairline
/// border, `Radius::Lg`, [`modal_width`]): a header (alert tile, title, muted
/// explanation), an impact list (a caption over the cascade of affected things), the
/// confirm band (a `type <phrase> to confirm` instruction over the input), and a footer
/// (an inline Escape hint pushed against the cancel + confirm buttons). The confirm
/// button is live — filled with the warning accent — only while the typed phrase
/// matches; otherwise it renders the dimmed, inert filled look and does not fire.
///
/// The card is just the surface: wrap the rendered entity in a centred
/// [`crate::overlay`] to gain the scrim, the enter animation and scrim/Escape
/// dismissal. The binary must have called [`crate::bind_text_input_keys`] once at boot
/// for the input's editing keys, and should [`TypeToConfirm::focus_input`] when the
/// dialog opens so typing lands in the field.
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

/// Builds a type-to-confirm dialog gated on re-typing `target`, resolving its ink from
/// `palette`. Creates the child input and subscribes to it so the confirm gate tracks
/// the typed text. Defaults to English confirm/cancel labels, no explanation, no
/// caption, an empty instruction wrap, no bullets and no Escape hint; layer those on
/// through the builder methods. Construct it inside `cx.new(|cx| type_to_confirm(...))`.
pub fn type_to_confirm(
    target: impl Into<SharedString>,
    palette: &ForgePalette,
    cx: &mut Context<TypeToConfirm>,
) -> TypeToConfirm {
    let target = target.into();
    let palette = *palette;
    let matched = matches("", target.as_ref());

    // The field's border is the match signal on the input itself: `brand` once the
    // typed phrase matches, `border_input` otherwise (no focus-reactive variant). A
    // pinned static chrome captures exactly that — set here for the empty initial
    // state and repinned in the subscription as the match toggles.
    let input = cx.new(|cx| {
        TextInput::new("", cx)
            .with_palette(palette)
            .with_font_size(FONT_SM)
            .static_chrome(match_border(matched, &palette), Radius::Md)
    });

    // Recompute the confirm gate whenever the field changes, repin the field's match
    // border, and translate the input's own submit / cancel keys into the dialog's
    // resolving events so the keyboard reaches the same outcomes as the buttons.
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
    /// Sets the dialog title beside the alert tile.
    #[must_use]
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = title.into();
        self
    }

    /// Sets the muted explanation line under the title.
    #[must_use]
    pub fn explanation(mut self, explanation: impl Into<SharedString>) -> Self {
        self.explanation = explanation.into();
        self
    }

    /// Sets the monospace caption above the impact list (e.g. "WHAT THIS MEANS"). Left
    /// empty, the caption line is omitted.
    #[must_use]
    pub fn section_caption(mut self, caption: impl Into<SharedString>) -> Self {
        self.section_caption = caption.into();
        self
    }

    /// Sets the two fragments wrapping the monospace phrase chip in the confirm
    /// instruction — e.g. prefix "type" and suffix "to confirm" render as
    /// `type <phrase> to confirm`. Both are caller-supplied so they can be localised.
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

    /// Appends one line to the impact list.
    #[must_use]
    pub fn bullet(mut self, kind: BulletKind, text: impl Into<SharedString>) -> Self {
        self.bullets.push(BulletItem::new(kind, text));
        self
    }

    /// Replaces the impact list with `bullets`.
    #[must_use]
    pub fn bullets(mut self, bullets: Vec<BulletItem>) -> Self {
        self.bullets = bullets;
        self
    }

    /// Overrides the confirm button label (defaults to "Confirm").
    #[must_use]
    pub fn confirm_label(mut self, label: impl Into<SharedString>) -> Self {
        self.confirm_label = label.into();
        self
    }

    /// Overrides the cancel button label (defaults to "Cancel").
    #[must_use]
    pub fn cancel_label(mut self, label: impl Into<SharedString>) -> Self {
        self.cancel_label = label.into();
        self
    }

    /// Sets the trailing phrase of the inline Escape hint (e.g. "to cancel"). The
    /// keyboard glyph and the `Esc` keycap ahead of it are structural; only the phrase
    /// is caller-supplied so it can be localised. Left unset, the hint is omitted.
    #[must_use]
    pub fn esc_hint(mut self, phrase: impl Into<SharedString>) -> Self {
        self.esc_hint = Some(phrase.into());
        self
    }

    /// Moves keyboard focus into the confirmation field. Call this when the dialog
    /// opens so the user can type immediately.
    pub fn focus_input(&self, window: &mut Window, cx: &mut App) {
        self.input.read(cx).focus(window);
    }
}

/// The warning-accent confirm button. The button family exposes no arbitrary-hue
/// filled constructor, so the confirm action mirrors the filled-button shape (rounded
/// frame, `shell` ink, hover fade) driven by the `warning` accent. While disabled (the
/// phrase does not yet match) it renders the inert `surface_overlay` fill with the
/// `disabled` ink and does not fire.
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

        // Header: the warning-tinted alert tile beside the title, over the muted
        // explanation.
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

        // Impact list: an optional monospace caption over the cascade of affected
        // things, on the `shell`-tinted band and divided from the header above.
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

        // Confirm band: the `type <phrase> to confirm` instruction over the field,
        // divided from the impact list above.
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

        // Footer: the inline Escape hint pushed against the cancel + confirm cluster,
        // on the `shell`-tinted band and divided from the confirm band above.
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

        // `overflow_hidden` clips the `shell`-tinted bands to the rounded card so their
        // fill does not square off the corners.
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
        // implementation — that divergence is the signal these rows pin.
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
        // has teeth — a swapped arm resolves to a different, detectable colour.
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
