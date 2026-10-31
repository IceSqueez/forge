use super::viewer_actions::{ViewerAction, ViewerTarget};
use super::viewer_follow::follow_display;
use super::{ChatView, INFINITY_GLYPH};
use crate::chat_drawer::{DASH, SubStatus, ViewerSummary};
use crate::platform_visuals::{BadgeKind, badge_color, badge_label};
use forge_components::{
    BORDER_THIN, DISABLED_OPACITY, Density, FONT_MD, FONT_SM, FONT_XS, FONT_XXS, ForgePalette,
    Icon, KeyActivated, Radius, Spacing, avatar_tile, badge, body_family, icon, mono_family,
    radius, spacing, tr,
};
use gpui::{
    AnyElement, App, ClickEvent, Context, Div, FontWeight, Pixels, Rgba, SharedString, Window, div,
    prelude::*, px,
};

const AVATAR_DETAIL: Pixels = px(38.0);
const DRAWER_ICON: Pixels = px(11.0);
const BADGE_DETAIL: Pixels = px(9.0);

pub(super) fn viewer_avatar(
    letter: char,
    color: Rgba,
    size: Pixels,
    corner: Radius,
    font: Pixels,
    palette: &ForgePalette,
) -> impl IntoElement {
    avatar_tile(letter.to_string(), color, palette)
        .size(size)
        .corner(radius(corner))
        .font(font)
}

pub(super) fn drawer_role_badge(
    kind: BadgeKind,
    size: Pixels,
    palette: &ForgePalette,
) -> impl IntoElement {
    badge(
        palette.surface_overlay,
        badge_color(kind, palette),
        badge_label(kind),
        false,
        size,
    )
}

fn stat_cell(
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
    color: Rgba,
    palette: &ForgePalette,
    density: Density,
) -> Div {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, density))
        .p(spacing(Spacing::Xs, density))
        .rounded(radius(Radius::Sm))
        .bg(palette.elevated)
        .child(
            div()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_muted)
                .child(label.into()),
        )
        .child(
            div()
                .font_family(mono_family())
                .text_size(FONT_XS)
                .text_color(color)
                .child(value.into()),
        )
}

fn sub_display(status: SubStatus, palette: &ForgePalette) -> (SharedString, Rgba) {
    match status {
        SubStatus::Unlimited => (INFINITY_GLYPH.into(), palette.success),
        SubStatus::Subscribed => (tr!("chat_stat_sub_yes").into(), palette.success),
        SubStatus::None => (DASH.into(), palette.text_faint),
    }
}

pub(super) fn with_transparent(color: Rgba) -> Rgba {
    Rgba { a: 0.0, ..color }
}

fn drawer_ghost_button(
    id: &'static str,
    glyph: Icon,
    label: impl Into<SharedString>,
    enabled: bool,
    palette: &ForgePalette,
    density: Density,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let border = palette.border_regular;
    let border_hover = palette.border_input;
    let surf = palette.surface_overlay;
    let text = palette.text_secondary;
    let text_hover = palette.text_primary;
    let button = div()
        .id(id)
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .gap(spacing(Spacing::Xxs, density))
        .py(spacing(Spacing::Xxs, density))
        .px(spacing(Spacing::Sm, density))
        .rounded(radius(Radius::Sm))
        .border(BORDER_THIN)
        .border_color(border)
        .child(icon(glyph, DRAWER_ICON, text))
        .child(
            div()
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(text)
                .child(label.into()),
        );
    if enabled {
        button
            .cursor_pointer()
            .hover(move |s| s.bg(surf).border_color(border_hover).text_color(text_hover))
            .on_click(handler)
    } else {
        button.opacity(DISABLED_OPACITY)
    }
}

impl ChatView {
    pub(super) fn render_selected_detail(
        &self,
        detail: Option<ViewerSummary>,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let frame = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .p(spacing(Spacing::Sm, density))
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular);

        let Some(summary) = detail else {
            return frame
                .child(
                    div()
                        .font_family(body_family())
                        .text_size(FONT_XS)
                        .text_color(palette.text_faint)
                        .child(tr!("chat_drawer_click_hint")),
                )
                .into_any_element();
        };

        let avatar = viewer_avatar(
            summary.avatar_letter,
            summary.avatar_color,
            AVATAR_DETAIL,
            Radius::Md,
            FONT_MD,
            palette,
        );

        let mut name_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(SharedString::from(summary.username.clone())),
            );
        if let Some(role) = summary.role {
            name_row = name_row.child(drawer_role_badge(role, BADGE_DETAIL, palette));
        }

        let last_seen = tr!("chat_drawer_last_seen", when = summary.last_seen_label());
        let name_col = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(name_row)
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(last_seen),
            );

        let info = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(avatar)
            .child(name_col);

        let (sub_value, sub_color) = sub_display(summary.sub, palette);
        let follow = follow_display(
            summary.key.platform,
            summary.role,
            self.follows.status_of(&summary.key),
            palette,
        );
        let tile_hover = palette.surface_overlay;
        let history_key = summary.key.clone();
        let history_name = summary.username.clone();
        let grid = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div().flex().gap(spacing(Spacing::Xs, density)).child(
                    stat_cell(
                        tr!("chat_stat_messages"),
                        summary.message_count.to_string(),
                        palette.text_primary,
                        palette,
                        density,
                    )
                    .id("chat-drawer-messages")
                    .cursor_pointer()
                    .hover(move |style| style.bg(tile_hover))
                    .key_activated(palette.text_primary)
                    .on_click(cx.listener(
                        move |this, _: &ClickEvent, _, cx| {
                            this.open_viewer_history(history_key.clone(), history_name.clone(), cx)
                        },
                    )),
                ),
            )
            .child(
                div()
                    .flex()
                    .gap(spacing(Spacing::Xs, density))
                    .child(stat_cell(
                        tr!("chat_stat_sub"),
                        sub_value,
                        sub_color,
                        palette,
                        density,
                    ))
                    .children(follow.map(|(follow_value, follow_color)| {
                        stat_cell(
                            tr!("chat_stat_follow"),
                            follow_value,
                            follow_color,
                            palette,
                            density,
                        )
                    })),
            );

        let target = ViewerTarget::new(&summary.key, summary.username.clone());
        let can_whisper = target.supports(&ViewerAction::Whisper(String::new()));
        let shoutout_target = target.clone();
        let whisper_key = summary.key.clone();
        let actions = div()
            .flex()
            .gap(spacing(Spacing::Xs, density))
            .child(drawer_ghost_button(
                "chat-drawer-shoutout",
                Icon::Bolt,
                tr!("chat_drawer_shoutout"),
                target.supports(&ViewerAction::Shoutout),
                palette,
                density,
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.shoutout_viewer(shoutout_target.clone(), cx)
                }),
            ))
            .child(drawer_ghost_button(
                "chat-drawer-whisper",
                Icon::MessageCircle,
                tr!("chat_drawer_whisper"),
                can_whisper,
                palette,
                density,
                cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.open_whisper(whisper_key.clone(), window, cx)
                }),
            ))
            .child(self.render_drawer_menu(&summary.key, &target, palette, cx));

        let whisper = (self.whisper_open && can_whisper)
            .then(|| self.render_whisper_compose(summary.username.clone(), palette, density, cx));

        frame
            .child(info)
            .child(grid)
            .child(actions)
            .children(whisper)
            .into_any_element()
    }
}
