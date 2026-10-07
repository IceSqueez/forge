use forge_components::{
    BORDER_THIN, FONT_XS, FONT_XXS, ForgePalette, Icon, body_family, icon, mono_family,
    section_label, tr,
};
use forge_overlay::config::{SOUND, SPEECH};
use forge_overlay::{OverlayConfig, PreviewCanvas};
use forge_storage::OverlayDefinition;
use forge_types::Variant;
use gpui::{AnyElement, ClickEvent, Context, Pixels, SharedString, div, prelude::*, px, relative};

use super::OverlaysView;
use crate::toasts::copy_to_clipboard;

const CARD_TOP: Pixels = px(10.0);
const CARD_PAD: Pixels = px(12.0);
const CARD_RADIUS: Pixels = px(10.0);
const CARD_GAP: Pixels = px(8.0);
const HEAD_GAP: Pixels = px(6.0);
const HEAD_GLYPH: Pixels = px(12.0);
const ROW_GAP: Pixels = px(8.0);
const KEY_W: Pixels = px(52.0);
const COPY_GAP: Pixels = px(3.0);
const COPY_GLYPH: Pixels = px(12.0);
const TIP_GAP: Pixels = px(4.0);
const HALF: f32 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ObsRecommendations {
    pub(super) shutdown_when_hidden: bool,
    pub(super) refresh_on_activate: bool,
    pub(super) control_audio: bool,
}

pub(super) fn obs_recommendations(config: &OverlayConfig, receiver: bool) -> ObsRecommendations {
    let carries = |key: &str| {
        config
            .get(key)
            .and_then(Variant::as_str)
            .is_some_and(|value| !value.trim().is_empty())
    };
    ObsRecommendations {
        shutdown_when_hidden: false,
        refresh_on_activate: false,
        control_audio: receiver || carries(SOUND) || carries(SPEECH),
    }
}

fn switch_text(on: bool) -> String {
    if on {
        tr!("overlays_obs_on")
    } else {
        tr!("overlays_obs_off")
    }
}

impl OverlaysView {
    pub(super) fn render_obs_card(
        &self,
        definition: &OverlayDefinition,
        canvas: PreviewCanvas,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let page = self
            .handles
            .kinds
            .get(&definition.kind_id)
            .is_some_and(|descriptor| descriptor.has_visual_page());
        let config = self.effective_config(definition).unwrap_or_default();
        let advice = obs_recommendations(&config, self.receiver.as_ref() == Some(&definition.id));

        let id = definition.id.clone();
        let url_row = match self.overlay_url(&definition.id) {
            Some(url) => value_row(
                tr!("overlays_obs_url"),
                url,
                Some(copy_link(
                    "overlays-obs-copy-url",
                    palette,
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.copy_url(&id, cx)),
                )),
                palette,
            ),
            None => value_row(
                tr!("overlays_obs_url"),
                tr!("overlays_url_not_served"),
                None,
                palette,
            ),
        };

        let size_row = page.then(|| {
            let width = canvas.width.to_string();
            let height = canvas.height.to_string();
            div()
                .w_full()
                .flex()
                .gap(ROW_GAP)
                .child(div().w(relative(HALF)).child(copyable_row(
                    "width",
                    tr!("overlays_obs_width"),
                    width,
                    palette,
                )))
                .child(div().w(relative(HALF)).child(copyable_row(
                    "height",
                    tr!("overlays_obs_height"),
                    height,
                    palette,
                )))
        });

        let tips = div()
            .w_full()
            .flex()
            .flex_wrap()
            .gap_y(TIP_GAP)
            .child(tip(
                tr!("overlays_obs_shutdown"),
                switch_text(advice.shutdown_when_hidden),
                palette,
            ))
            .child(tip(
                tr!("overlays_obs_refresh"),
                switch_text(advice.refresh_on_activate),
                palette,
            ))
            .child(tip(
                tr!("overlays_obs_custom_css"),
                tr!("overlays_obs_custom_css_value"),
                palette,
            ))
            .child(tip(
                tr!("overlays_obs_audio"),
                switch_text(advice.control_audio),
                palette,
            ));

        div()
            .flex_none()
            .w_full()
            .mt(CARD_TOP)
            .flex()
            .flex_col()
            .gap(CARD_GAP)
            .p(CARD_PAD)
            .rounded(CARD_RADIUS)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.shell)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(HEAD_GAP)
                    .child(icon(Icon::Browser, HEAD_GLYPH, palette.text_faint))
                    .child(section_label(
                        tr!("overlays_obs_title").to_uppercase(),
                        palette,
                    )),
            )
            .child(url_row)
            .children(size_row)
            .child(tips)
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(tr!("overlays_preview_approximate")),
            )
            .into_any_element()
    }
}

fn copy_link(
    id: impl Into<SharedString>,
    palette: &ForgePalette,
    on_click: impl Fn(&ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> AnyElement {
    div()
        .id(id.into())
        .flex_none()
        .flex()
        .items_center()
        .gap(COPY_GAP)
        .cursor_pointer()
        .font_family(mono_family())
        .text_size(FONT_XXS)
        .text_color(palette.brand)
        .on_click(on_click)
        .child(icon(Icon::Copy, COPY_GLYPH, palette.brand))
        .child(tr!("overlays_url_copy"))
        .into_any_element()
}

fn copyable_row(name: &str, label: String, value: String, palette: &ForgePalette) -> AnyElement {
    let copied = value.clone();
    let link = copy_link(
        SharedString::from(format!("overlays-obs-copy-{name}")),
        palette,
        move |_: &ClickEvent, _, cx| copy_to_clipboard(copied.clone(), cx),
    );
    value_row(label, value, Some(link), palette)
}

fn value_row(
    label: String,
    value: String,
    action: Option<AnyElement>,
    palette: &ForgePalette,
) -> AnyElement {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(ROW_GAP)
        .child(
            div()
                .flex_none()
                .w(KEY_W)
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(label.to_uppercase()),
        )
        .child(
            div()
                .min_w(px(0.0))
                .truncate()
                .font_family(mono_family())
                .text_size(FONT_XS)
                .text_color(palette.text_secondary)
                .child(value),
        )
        .children(action)
        .into_any_element()
}

fn tip(label: String, value: String, palette: &ForgePalette) -> AnyElement {
    div()
        .w(relative(HALF))
        .flex()
        .items_center()
        .gap(HEAD_GAP)
        .pr(ROW_GAP)
        .font_family(body_family())
        .text_size(FONT_XXS)
        .child(
            div()
                .min_w(px(0.0))
                .truncate()
                .text_color(palette.text_muted)
                .child(label),
        )
        .child(
            div()
                .flex_none()
                .font_family(mono_family())
                .text_color(palette.text_secondary)
                .child(value),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(entries: &[(&str, &str)]) -> OverlayConfig {
        entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), Variant::String((*value).to_owned())))
            .collect()
    }

    #[test]
    fn the_card_asks_obs_to_control_audio_only_when_the_overlay_can_make_sound() {
        for (stored, receiver, expected) in [
            (config(&[]), false, false),
            (config(&[(SOUND, ""), (SPEECH, "   ")]), false, false),
            (config(&[]), true, true),
            (
                config(&[(SOUND, "clip:01J9P4S2M7Q8V3X5Y6Z7A8B9C0")]),
                false,
                true,
            ),
            (config(&[(SPEECH, "{user} followed")]), false, true),
        ] {
            assert_eq!(
                obs_recommendations(&stored, receiver).control_audio,
                expected,
                "{stored:?} receiver={receiver}"
            );
        }
    }
}
