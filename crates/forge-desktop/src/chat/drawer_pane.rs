use super::viewer_actions::{ViewerAction, ViewerTarget};
use super::viewer_detail::{drawer_role_badge, viewer_avatar, with_transparent};
use super::{ChatView, DRAWER_TIMEOUT_SECONDS, DrawerResizeDrag};
use crate::chat_author::AuthorKey;
use crate::chat_drawer::{ViewerSummary, author_summary, selected_summary};
use crate::navigable_list::{ListStep, NavigableList, navigable_list};
use forge_components::{
    BORDER_THIN, Density, FONT_XS, FONT_XXS, ForgePalette, Icon, MenuPlacement, Radius, ResizeEdge,
    ResizeRange, Spacing, body_family, icon, install_resize, menu_button, menu_divider, menu_item,
    mono_family, spacing, tr,
};
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, Pixels, ScrollStrategy, SharedString, Window, div,
    prelude::*, px, uniform_list,
};

const DRAWER_MIN: Pixels = px(260.0);
const DRAWER_MAX: Pixels = px(520.0);
const AVATAR_ROW: Pixels = px(22.0);
const ROW_STRIPE: Pixels = px(2.0);
const VIEWERS_ICON: Pixels = px(13.0);
const BADGE_ROW: Pixels = px(8.5);

impl NavigableList for ChatView {
    fn step_list(&mut self, step: ListStep, cx: &mut Context<Self>) {
        let selected = self.selected_viewer.clone();
        let next = self
            .drawer_list_keys
            .step(&self.drawer_keys, selected.as_ref(), step);
        if let Some(next) = next {
            self.drawer_list_scroll
                .scroll_to_item(next, ScrollStrategy::Nearest);
            cx.notify();
        }
    }

    fn activate_list(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(key) = self.drawer_list_keys.activation_target(&self.drawer_keys) {
            self.select_viewer(key, cx);
        }
    }
}

impl ChatView {
    pub(super) fn render_drawer(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let authors = self.feed.read(cx).authors();
        let total = authors.len();
        let shown = self.drawer_keys.len();
        let detail = selected_summary(
            self.selected_viewer.as_ref(),
            authors,
            &self.viewers,
            self.viewer_messages.read(cx),
            palette,
        );
        let selected_key = detail.as_ref().map(|d| d.key.clone());

        let header = self.render_drawer_header(total, shown, palette, density);
        let detail_el = self.render_selected_detail(detail, palette, density, cx);
        let list_el = self.render_viewer_list(selected_key, shown, palette, density, cx);

        let panel = div()
            .w(self.drawer_width)
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(palette.shell)
            .border_l(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(header)
            .child(detail_el)
            .child(list_el);

        install_resize(
            panel,
            DrawerResizeDrag,
            "chat-drawer-resize",
            ResizeEdge::Left,
            ResizeRange {
                min: DRAWER_MIN,
                max: DRAWER_MAX,
            },
            palette,
            cx.listener(|this, width: &Pixels, _, cx| this.set_drawer_width(*width, cx)),
        )
    }

    fn render_drawer_header(
        &self,
        total: usize,
        shown: usize,
        palette: &ForgePalette,
        density: Density,
    ) -> impl IntoElement + use<> {
        let count = tr!(
            "chat_drawer_active_count",
            total = i64::try_from(total).unwrap_or(i64::MAX),
            shown = i64::try_from(shown).unwrap_or(i64::MAX)
        );
        let title = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(icon(Icon::Users, VIEWERS_ICON, palette.brand))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(tr!("chat_viewers_title")),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(count),
            );

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Sm, density))
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(title)
            .child(self.drawer_search.field().clone())
    }

    pub(super) fn render_drawer_menu(
        &self,
        key: &AuthorKey,
        target: &ViewerTarget,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let shoutout_target = target.clone();
        let whisper_key = key.clone();
        let block_target = target.clone();
        let voice_target = target.clone();
        let has_tts_key = target.tts_alias_key().is_some();
        let timeout_target = target.clone();
        let ban_target = target.clone();
        let items = vec![
            menu_item(
                "chat-drawer-menu-shoutout",
                tr!("chat_drawer_shoutout"),
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.shoutout_viewer(shoutout_target.clone(), cx)
                }),
            )
            .icon(Icon::Flag)
            .disabled(!target.supports(&ViewerAction::Shoutout))
            .into(),
            menu_item(
                "chat-drawer-menu-whisper",
                tr!("chat_drawer_whisper"),
                cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.open_whisper(whisper_key.clone(), window, cx)
                }),
            )
            .icon(Icon::MessageCircle)
            .disabled(!target.supports(&ViewerAction::Whisper(String::new())))
            .into(),
            menu_item(
                "chat-drawer-menu-tts-voice",
                tr!("chat_drawer_set_tts_voice"),
                cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.open_tts_voice(voice_target.clone(), window, cx)
                }),
            )
            .icon(Icon::Pencil)
            .disabled(!has_tts_key)
            .into(),
            menu_divider(),
            menu_item(
                "chat-drawer-menu-block-tts",
                tr!("chat_drawer_block_tts"),
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.block_tts_viewer(block_target.clone(), cx)
                }),
            )
            .color(palette.warning)
            .disabled(!has_tts_key)
            .into(),
            menu_item(
                "chat-drawer-menu-timeout",
                tr!("chat_drawer_timeout"),
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.timeout_viewer(timeout_target.clone(), cx)
                }),
            )
            .color(palette.warning)
            .disabled(!target.supports(&ViewerAction::Timeout {
                seconds: DRAWER_TIMEOUT_SECONDS,
            }))
            .into(),
            menu_item(
                "chat-drawer-menu-ban",
                tr!("chat_drawer_ban"),
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.request_ban(ban_target.clone(), cx)
                }),
            )
            .color(palette.danger)
            .disabled(!target.supports(&ViewerAction::Ban))
            .into(),
        ];

        menu_button(Icon::DotsVertical, self.drawer_menu_open.is_some(), palette)
            .placement(MenuPlacement::TopRight)
            .open_at(self.drawer_menu_open)
            .items(items)
            .on_toggle(
                "chat-drawer-menu-trigger",
                cx.listener(|this, ev: &ClickEvent, _, cx| {
                    this.toggle_drawer_menu(ev.position(), cx)
                }),
            )
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_drawer_menu(cx));
            })
            .into_any_element()
    }

    fn render_viewer_list(
        &self,
        selected_key: Option<AuthorKey>,
        shown: usize,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let header = div()
            .flex_none()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(tr!(
                "chat_drawer_section_active",
                count = i64::try_from(shown).unwrap_or(i64::MAX)
            ));

        let body = if shown == 0 {
            div()
                .py(spacing(Spacing::Xs, density))
                .px(spacing(Spacing::Sm, density))
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_faint)
                .child(tr!("chat_drawer_no_matches"))
                .into_any_element()
        } else {
            let pal = *palette;
            uniform_list(
                "chat-drawer-list",
                shown,
                cx.processor(move |this, range: std::ops::Range<usize>, _window, cx| {
                    let mut rows = Vec::with_capacity(range.len());
                    for ix in range {
                        let Some(summary) = this.drawer_keys.get(ix).and_then(|key| {
                            author_summary(
                                key,
                                this.feed.read(cx).authors(),
                                &this.viewers,
                                this.viewer_messages.read(cx),
                                &pal,
                            )
                        }) else {
                            continue;
                        };
                        let is_sel = selected_key.as_ref() == Some(&summary.key);
                        rows.push(
                            this.render_viewer_row(summary, is_sel, &pal, density, cx)
                                .into_any_element(),
                        );
                    }
                    rows
                }),
            )
            .track_scroll(&self.drawer_list_scroll)
            .flex_1()
            .min_h(px(0.0))
            .into_any_element()
        };

        let focus = self.drawer_list_keys.focus().clone();
        navigable_list(div(), &focus, cx)
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .child(header)
            .child(body)
    }

    fn render_viewer_row(
        &self,
        summary: ViewerSummary,
        is_sel: bool,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let key = summary.key.clone();
        let is_cursor = self.drawer_list_keys.is_at(&key);
        let row_id = summary.key.element_id("chat-drawer-row");
        let stripe = if is_sel {
            palette.brand
        } else {
            with_transparent(palette.brand)
        };
        let selected_bg = palette.surface_overlay;
        let hover_bg = palette.elevated;

        let avatar = viewer_avatar(
            summary.avatar_letter,
            summary.avatar_color,
            AVATAR_ROW,
            Radius::Sm,
            FONT_XXS,
            palette,
        );

        let mut name_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(SharedString::from(summary.username.clone())),
            );
        if let Some(role) = summary.role {
            name_row = name_row.child(drawer_role_badge(role, BADGE_ROW, palette));
        }

        let meta = format!("{} msg", summary.message_count);
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
                    .child(meta),
            );

        let last_seen = div()
            .flex_none()
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(SharedString::from(summary.last_seen_label()));

        let mut row = div()
            .id(row_id)
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .border_l(ROW_STRIPE)
            .border_color(stripe)
            .cursor_pointer()
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.select_viewer(key.clone(), cx)),
            )
            .child(avatar)
            .child(name_col)
            .child(last_seen);

        if is_sel {
            row = row.bg(selected_bg);
        } else if is_cursor {
            row = row.bg(hover_bg);
        } else {
            row = row.hover(move |s| s.bg(hover_bg));
        }
        row
    }
}
