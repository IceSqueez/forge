use forge_components::{
    BORDER_THIN, FONT_XS, ForgePalette, Icon, Radius, body_family, icon, mono_family, radius, tr,
};
use forge_platform_core::IntegrationDeclaration;
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, Pixels, SharedString, div, prelude::*, px,
    relative,
};

use super::IntegrationsHubView;
use crate::screen::Screen;
use crate::titlebar::brand_mark;

const COLUMN_WIDTH: Pixels = px(1040.0);
const COLUMN_MAX_FRACTION: f32 = 0.94;
const COLUMN_PAD_TOP: Pixels = px(36.0);
const COLUMN_PAD_BOTTOM: Pixels = px(28.0);

const HERO_MB: Pixels = px(22.0);
const MARK_SIZE: Pixels = px(52.0);
const MARK_CORNER: Pixels = px(13.0);
const MARK_GLYPH: Pixels = px(26.0);
const MARK_MB: Pixels = px(14.0);
const TITLE_SIZE: Pixels = px(22.0);
const LEAD_SIZE: Pixels = px(13.0);
const LEAD_MT: Pixels = px(6.0);
const LEAD_LINE_HEIGHT: f32 = 1.5;

const PROGRESS_MAX_W: Pixels = px(560.0);
const PROGRESS_MB: Pixels = px(22.0);
const PROGRESS_PAD_V: Pixels = px(9.0);
const PROGRESS_PAD_H: Pixels = px(13.0);
const PROGRESS_CORNER: Pixels = px(9.0);
const PROGRESS_GAP: Pixels = px(10.0);
const PROGRESS_ICON: Pixels = px(16.0);
const PROGRESS_TEXT: Pixels = px(12.5);
const PROGRESS_COUNT: Pixels = px(11.0);

const BUILT_IN_GAP: Pixels = px(8.0);
const BUILT_IN_ICON: Pixels = px(13.0);
const BUILT_IN_TEXT: Pixels = px(11.5);
const BUILT_IN_MT: Pixels = px(4.0);

const BAR_PAD_V: Pixels = px(12.0);
const BAR_PAD_H: Pixels = px(28.0);
const CTA_PAD_V: Pixels = px(8.0);
const CTA_PAD_H: Pixels = px(18.0);
const CTA_GAP: Pixels = px(5.0);
const CTA_ICON: Pixels = px(14.0);
const CTA_IDLE_OPACITY: f32 = 0.5;

impl IntegrationsHubView {
    pub(super) fn render_welcome(
        &mut self,
        hub: &[IntegrationDeclaration],
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let enabled: Vec<&'static str> = {
            let lifecycle = self.lifecycle.read(cx);
            hub.iter()
                .filter(|declaration| lifecycle.is_on(&declaration.id))
                .map(|declaration| declaration.brand_name)
                .collect()
        };
        let sections: Vec<AnyElement> = self
            .visible_categories()
            .into_iter()
            .filter_map(|category| self.section(category, hub, palette, cx))
            .collect();

        let column = div()
            .w(COLUMN_WIDTH)
            .max_w(relative(COLUMN_MAX_FRACTION))
            .pt(COLUMN_PAD_TOP)
            .pb(COLUMN_PAD_BOTTOM)
            .child(hero(palette))
            .child(progress(&enabled, hub.len(), palette))
            .children(sections)
            .child(built_in_note(palette));

        let scroll = div()
            .id("integrations-welcome-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .bg(palette.base)
            .flex()
            .justify_center()
            .child(column);

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(scroll)
            .child(self.action_bar(!enabled.is_empty(), palette, cx))
            .into_any_element()
    }

    fn action_bar(
        &self,
        can_enter: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let skip = div()
            .id("integrations-welcome-skip")
            .cursor_pointer()
            .font_family(body_family())
            .text_size(FONT_XS)
            .text_color(palette.text_faint)
            .hover(|style| style.text_color(palette.text_secondary))
            .child(tr!("integration_welcome_skip"))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.finish_welcome(cx)));

        let label = if can_enter {
            tr!("integration_welcome_enter")
        } else {
            tr!("integration_welcome_turn_one_on")
        };
        let mut enter = div()
            .id("integrations-welcome-enter")
            .flex()
            .items_center()
            .gap(CTA_GAP)
            .py(CTA_PAD_V)
            .px(CTA_PAD_H)
            .rounded(radius(Radius::Sm))
            .bg(palette.brand)
            .font_family(body_family())
            .font_weight(FontWeight::MEDIUM)
            .text_size(FONT_XS)
            .text_color(palette.shell)
            .child(label)
            .child(icon(Icon::ArrowRight, CTA_ICON, palette.shell));
        enter = if can_enter {
            enter
                .cursor_pointer()
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.finish_welcome(cx)))
        } else {
            enter.opacity(CTA_IDLE_OPACITY)
        };

        div()
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .py(BAR_PAD_V)
            .px(BAR_PAD_H)
            .bg(palette.shell)
            .border_t(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(skip)
            .child(enter)
            .into_any_element()
    }

    fn finish_welcome(&mut self, cx: &mut Context<Self>) {
        self.go(Screen::Home, cx);
    }
}

fn hero(palette: &ForgePalette) -> impl IntoElement {
    div()
        .mb(HERO_MB)
        .flex()
        .flex_col()
        .items_center()
        .child(
            div()
                .mb(MARK_MB)
                .child(brand_mark(MARK_SIZE, MARK_CORNER, MARK_GLYPH, palette)),
        )
        .child(
            div()
                .font_family(body_family())
                .font_weight(FontWeight::BOLD)
                .text_size(TITLE_SIZE)
                .text_color(palette.text_primary)
                .child(tr!("integration_welcome_title")),
        )
        .child(
            div()
                .mt(LEAD_MT)
                .flex()
                .flex_col()
                .items_center()
                .font_family(body_family())
                .text_size(LEAD_SIZE)
                .line_height(relative(LEAD_LINE_HEIGHT))
                .text_color(palette.text_muted)
                .child(tr!("integration_welcome_lead"))
                .child(tr!("integration_welcome_lead_later")),
        )
}

fn progress(enabled: &[&'static str], total: usize, palette: &ForgePalette) -> impl IntoElement {
    let (glyph, tint, summary) = if enabled.is_empty() {
        (
            Icon::CircleDashed,
            palette.text_faint,
            tr!("integration_welcome_nothing_on"),
        )
    } else {
        (
            Icon::CircleCheckFilled,
            palette.success,
            tr!(
                "integration_welcome_some_on",
                count = enabled.len() as i64,
                names = enabled.join(", ")
            ),
        )
    };
    let count = tr!(
        "integration_welcome_progress",
        on = enabled.len() as i64,
        total = total as i64
    );
    div()
        .w_full()
        .flex()
        .justify_center()
        .mb(PROGRESS_MB)
        .child(
            div()
                .w_full()
                .max_w(PROGRESS_MAX_W)
                .flex()
                .items_center()
                .gap(PROGRESS_GAP)
                .py(PROGRESS_PAD_V)
                .px(PROGRESS_PAD_H)
                .bg(palette.shell)
                .border(BORDER_THIN)
                .border_color(palette.border_regular)
                .rounded(PROGRESS_CORNER)
                .child(icon(glyph, PROGRESS_ICON, tint))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_family(body_family())
                        .text_size(PROGRESS_TEXT)
                        .text_color(palette.text_primary)
                        .child(summary),
                )
                .child(
                    div()
                        .flex_none()
                        .font_family(mono_family())
                        .text_size(PROGRESS_COUNT)
                        .text_color(palette.text_faint)
                        .child(SharedString::from(count)),
                ),
        )
}

fn built_in_note(palette: &ForgePalette) -> impl IntoElement {
    div()
        .mt(BUILT_IN_MT)
        .flex()
        .items_center()
        .gap(BUILT_IN_GAP)
        .font_family(body_family())
        .text_size(BUILT_IN_TEXT)
        .text_color(palette.text_faint)
        .child(icon(Icon::Sparkles, BUILT_IN_ICON, palette.text_muted))
        .child(tr!("integration_welcome_built_in"))
}
