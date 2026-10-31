use super::viewer_actions::ViewerTarget;
use super::{ChatView, PlatformFilter};
use crate::chat_gap_row::chat_gap_row;
use crate::chat_row::{ChatRow, chat_row};
use crate::platform_visuals::{platform_mark, role_badge};
use forge_components::{
    BORDER_THIN, ChipGlyph, DateShape, Density, FONT_XS, ForgePalette, Icon, Radius,
    SEARCH_FIELD_WIDTH, Spacing, chip, empty_state, fmt_display, icon, mono_family, radius,
    spacing, tr,
};
use forge_types::PlatformId;
use gpui::{
    AnyElement, ClickEvent, Context, MouseButton, MouseDownEvent, Pixels, div, list, prelude::*, px,
};
use std::collections::VecDeque;
use std::rc::Rc;

const CHIP_DIVIDER_H: Pixels = px(14.0);
const ICON_BTN_PAD: Pixels = px(5.0);
const ICON_BTN_RADIUS: Pixels = px(5.0);
const EXPORT_ICON: Pixels = px(14.0);
const SEARCH_MISS_OPACITY: f32 = 0.3;

impl ChatView {
    pub(super) fn render_header_right(&self) -> impl IntoElement + use<> {
        self.viewer_count.clone()
    }

    pub(super) fn render_filter_left(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let platform_chips = [
            (
                "chat-chip-all",
                tr!("chat_filter_all"),
                PlatformFilter::All,
                palette.brand,
            ),
            (
                "chat-chip-twitch",
                "Twitch".to_owned(),
                PlatformFilter::Single(PlatformId::Twitch),
                palette.brand,
            ),
            (
                "chat-chip-youtube",
                "YouTube".to_owned(),
                PlatformFilter::Single(PlatformId::YouTube),
                palette.danger,
            ),
            (
                "chat-chip-kick",
                "Kick".to_owned(),
                PlatformFilter::Single(PlatformId::Kick),
                palette.info,
            ),
        ];

        let mut chips = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density));
        for (id, label, filter, dot) in platform_chips {
            if !self.filter_offered(filter, cx) {
                continue;
            }
            let active = self.platform_filter == filter;
            chips = chips.child(chip(label, ChipGlyph::Dot(dot), active, palette).on_click(
                id,
                cx.listener(move |this, _, _, cx| this.set_platform_filter(filter, cx)),
            ));
        }
        chips = chips.child(
            div()
                .w(BORDER_THIN)
                .h(CHIP_DIVIDER_H)
                .bg(palette.border_regular),
        );
        chips = chips.child(
            chip(
                tr!("chat_filter_events"),
                ChipGlyph::None,
                self.events_only,
                palette,
            )
            .on_click(
                "chat-chip-events",
                cx.listener(|this, _, _, cx| this.toggle_events(cx)),
            ),
        );
        chips = chips.child(
            chip(
                tr!("chat_filter_hide_bots"),
                ChipGlyph::Icon(Icon::BellOff, palette.text_faint),
                self.hide_bots,
                palette,
            )
            .on_click(
                "chat-chip-hide-bots",
                cx.listener(|this, _, _, cx| this.toggle_hide_bots(cx)),
            ),
        );

        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(
                div()
                    .w(SEARCH_FIELD_WIDTH)
                    .child(self.search.field().clone()),
            )
            .child(chips)
            .into_any_element()
    }

    pub(super) fn render_filter_right(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let surf = palette.surface_overlay;
        let green = palette.success;

        div()
            .id("chat-export")
            .flex()
            .items_center()
            .justify_center()
            .p(ICON_BTN_PAD)
            .rounded(ICON_BTN_RADIUS)
            .cursor_pointer()
            .hover(move |s| s.bg(surf))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.export_chat_log(cx)))
            .child(icon(Icon::Download, EXPORT_ICON, green))
            .into_any_element()
    }

    pub(super) fn render_chat_area(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let query = self.search.query().to_string();
        let search_active = !query.is_empty();

        let snapshot: Rc<VecDeque<u64>> = self.visible.clone();
        let empty = snapshot.is_empty();
        let feed = self.feed.clone();

        let row_gap = spacing(Spacing::Xxs, density);
        let pal = *palette;
        let view = cx.entity();
        let list_el = list(self.chat_list.clone(), move |ix, _window, app| {
            let Some(seq) = snapshot.get(ix).copied() else {
                return div().into_any_element();
            };
            let previous = ix
                .checked_sub(1)
                .and_then(|prev| snapshot.get(prev).copied());
            let feed_state = feed.read(app);
            let Some(msg) = feed_state.get(seq) else {
                return div().into_any_element();
            };
            let gap = feed_state.gap_before(previous, seq);
            let data = ChatRow {
                id: msg.id.clone(),
                timestamp: fmt_display(DateShape::TimeWithSeconds, msg.received_at).into(),
                platform: platform_mark(msg.platform, &pal),
                badges: msg
                    .badges
                    .iter()
                    .map(|&kind| role_badge(kind, &pal))
                    .collect(),
                username: msg.username.clone(),
                username_color: Self::username_color(msg, &pal),
                body: msg.body.clone(),
                moderated: msg.moderated,
                reply: msg.reply.clone(),
            };
            let author_key = msg.author_key();
            let menu_view = view.clone();
            let menu_target = author_key
                .as_ref()
                .map(|key| ViewerTarget::new(key, msg.username.to_string()));
            let menu_message_id = msg.id.clone();
            let view = view.clone();
            let row = chat_row(&pal, data).on_username_click(
                (gpui::ElementId::from("chat-username"), msg.id.clone()),
                move |_: &ClickEvent, _, app| {
                    if let Some(key) = author_key.clone() {
                        view.update(app, |this, cx| this.open_viewer(key, cx));
                    }
                },
            );
            let mut framed = div().pb(row_gap);
            if !gap.is_empty() {
                framed = framed.child(div().pb(row_gap).child(chat_gap_row(
                    gap.label(),
                    &pal,
                    density,
                )));
            }
            framed = framed.child(row);
            if let Some(menu_target) = menu_target {
                framed = framed.on_mouse_down(
                    MouseButton::Right,
                    move |event: &MouseDownEvent, _window, app| {
                        let position = event.position;
                        let viewer = menu_target.clone();
                        let message_id = menu_message_id.to_string();
                        menu_view.update(app, |this, cx| {
                            this.open_user_menu(position, viewer, message_id, cx)
                        });
                    },
                );
            }
            if search_active && !msg.matches_query(&query) {
                framed.opacity(SEARCH_MISS_OPACITY).into_any_element()
            } else {
                framed.into_any_element()
            }
        })
        .flex_1()
        .min_h(px(0.0))
        .py(spacing(Spacing::Sm, density))
        .px(spacing(Spacing::Md, density));

        let body: AnyElement = if empty {
            div()
                .w_full()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(empty_state(tr!("chat_no_filter_matches"), palette).density(density))
                .into_any_element()
        } else {
            list_el.into_any_element()
        };

        let mut area = div()
            .relative()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(body);

        if self.unread > 0 {
            let label = if self.unread == 1 {
                tr!("chat_new_message")
            } else {
                tr!(
                    "chat_new_messages",
                    count = i64::try_from(self.unread).unwrap_or(i64::MAX)
                )
            };
            let pill = div()
                .id("chat-unread-pill")
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xxs, density))
                .py(spacing(Spacing::Xs, density))
                .px(spacing(Spacing::Sm, density))
                .rounded(radius(Radius::Pill))
                .bg(palette.brand)
                .cursor_pointer()
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.jump_to_latest(cx)))
                .child(icon(Icon::ArrowDown, FONT_XS, palette.shell))
                .child(
                    div()
                        .font_family(mono_family())
                        .text_size(FONT_XS)
                        .text_color(palette.shell)
                        .child(label),
                );
            let overlay = div()
                .absolute()
                .inset_0()
                .flex()
                .justify_center()
                .items_end()
                .pb(spacing(Spacing::Md, density))
                .child(pill);
            area = area.child(overlay);
        }

        area
    }
}
