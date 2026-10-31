use super::editor::kind_word;
use super::{ACTION_HOVER_ALPHA, GlobalsView, ROW_DOT, RowMenu, VALUE_ICON};
use crate::globals::{Global, variant_kind_color};
use crate::load_notice::{LoadNotice, NoticeWidth, load_notice};
use crate::navigable_list::navigable_list;
use forge_components::{
    ActivePresentation, ColumnWidth, DataRow, Density, FONT_XS, ForgePalette, Icon, KeyActivated,
    Radius, Spacing, badge, body_family, column, context_menu, empty_state, hover_reveal, icon,
    menu_divider, menu_item, mono_family, radius, scaled, spacing, status_dot, toggle, tr,
    virtual_table, with_alpha,
};
use forge_types::{Variant, VariantKind};
use gpui::prelude::*;
use gpui::{
    ClickEvent, Context, MouseButton, MouseDownEvent, Rgba, SharedString, Window, div, px, svg,
};

impl GlobalsView {
    pub(super) fn render_table(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let rows = &self.visible;

        let count = rows.len();
        let body = if count == 0 {
            let notice = load_notice(
                &self.list_load,
                LoadNotice {
                    id: "globals-load",
                    loading: tr!("globals_loading"),
                    width: NoticeWidth::Pane,
                },
                cx.listener(|this, _: &ClickEvent, _, cx| this.retry_list_load(cx)),
                palette,
                density,
            )
            .filter(|_| self.globals.read(cx).total() == 0);
            let state = notice.unwrap_or_else(|| {
                empty_state(tr!(self.empty_caption_key(cx)), palette)
                    .density(density)
                    .into_any_element()
            });
            div()
                .w_full()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(state)
                .into_any_element()
        } else {
            let columns = vec![
                column("", ColumnWidth::Fixed(px(24.0))),
                column(tr!("globals_editor_section_name"), ColumnWidth::Flex(8.0)),
                column(
                    tr!("globals_editor_section_type"),
                    ColumnWidth::Fixed(px(80.0)),
                ),
                column(tr!("globals_editor_section_value"), ColumnWidth::Flex(8.0)),
                column(tr!("globals_col_modified"), ColumnWidth::Fixed(px(120.0))),
                column(
                    tr!("globals_col_reads_writes"),
                    ColumnWidth::Fixed(px(96.0)),
                ),
                column(tr!("globals_col_persist"), ColumnWidth::Fixed(px(64.0))),
                column(tr!("globals_col_actions"), ColumnWidth::Fixed(px(84.0))),
            ];
            let pal = *palette;
            virtual_table(
                "globals-scroll",
                palette,
                columns,
                count,
                &self.table_scroll,
                density.tighter(),
            )
            .build(
                move |this, ix, _window, cx| {
                    let g = this.visible[ix].clone();
                    this.build_row(&g, &pal, cx)
                },
                cx,
            )
        };

        let focus = self.table_keys.focus().clone();
        navigable_list(div().id("globals-table"), &focus, cx)
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(body)
    }

    pub(super) fn visible_names(&self) -> Vec<SharedString> {
        self.visible.iter().map(|g| g.name.clone()).collect()
    }

    fn build_row(&self, g: &Global, palette: &ForgePalette, cx: &mut Context<Self>) -> DataRow {
        let density = cx.density();
        let group: SharedString = format!("globals-row-{}", g.name).into();
        let kind = g.kind();
        let name = g.name.clone();

        let dot_color = if g.persisted {
            palette.brand
        } else {
            palette.warning
        };
        let dot = div()
            .flex()
            .justify_center()
            .child(status_dot(dot_color, ROW_DOT));

        let name_cell = self.name_cell(&name, palette, cx);

        let kind_pill = div().flex().items_center().child(badge(
            palette.surface_overlay,
            variant_kind_color(kind, palette),
            kind_word(kind),
            true,
            px(9.5),
        ));

        let value_cell = self.value_cell(g, palette, cx);

        let modified_cell = div()
            .font_family(body_family())
            .text_size(FONT_XS)
            .text_color(palette.text_muted)
            .child(g.modified_label());

        let rw_cell = div()
            .font_family(mono_family())
            .text_size(FONT_XS)
            .text_color(palette.text_muted)
            .child(format!("{} · {}", g.reads, g.writes));

        let toggle_name = name.clone();
        let persist_cell =
            div()
                .flex()
                .justify_center()
                .child(toggle(g.persisted, palette).on_click(
                    (gpui::ElementId::from("globals-persist"), name.clone()),
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.toggle_persist(toggle_name.clone(), cx)
                    }),
                ));

        let edit_name = name.clone();
        let delete_name = name.clone();
        let actions = div()
            .flex()
            .items_center()
            .justify_end()
            .gap(scaled(px(2.0), density))
            .child(self.row_action(
                (gpui::ElementId::from("globals-edit"), name.clone()),
                Icon::Edit,
                palette.text_secondary,
                with_alpha(palette.brand, ACTION_HOVER_ALPHA),
                cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.open_edit(edit_name.clone(), window, cx)
                }),
            ))
            .child(self.row_action(
                (gpui::ElementId::from("globals-delete"), name.clone()),
                Icon::X,
                palette.text_secondary,
                palette.danger,
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.request_delete(delete_name.clone(), cx)
                }),
            ));
        let actions_cell = hover_reveal(actions, group.clone());

        DataRow::with_reveal_group(
            vec![
                dot.into_any_element(),
                name_cell,
                kind_pill.into_any_element(),
                value_cell.into_any_element(),
                modified_cell.into_any_element(),
                rw_cell.into_any_element(),
                persist_cell.into_any_element(),
                actions_cell.into_any_element(),
            ],
            group,
        )
        .highlighted(self.table_keys.is_at(&g.name))
    }

    fn name_cell(
        &self,
        name: &SharedString,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        if let Some(rename) = self.renaming.as_ref().filter(|r| &r.original == name) {
            return div()
                .w_full()
                .child(rename.editor.clone())
                .into_any_element();
        }

        let rename_target = name.clone();
        let menu_target = name.clone();
        div()
            .font_family(mono_family())
            .text_size(FONT_XS)
            .text_color(palette.text_primary)
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if event.click_count >= 2 {
                        this.start_rename(rename_target.clone(), window, cx);
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    this.open_row_menu(menu_target.clone(), event.position, cx);
                }),
            )
            .child(name.clone())
            .into_any_element()
    }

    pub(super) fn open_row_menu(
        &mut self,
        name: SharedString,
        position: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.row_menu = Some(RowMenu { name, position });
        cx.notify();
    }

    fn close_row_menu(&mut self, cx: &mut Context<Self>) {
        if self.row_menu.is_some() {
            self.row_menu = None;
            cx.notify();
        }
    }

    pub(super) fn render_row_context_menu(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let menu = self.row_menu.as_ref()?;
        let name = menu.name.clone();
        let persisted = self
            .globals
            .read(cx)
            .entries()
            .iter()
            .any(|g| g.name == name && g.persisted);
        let view = cx.entity();

        let rename_name = name.clone();
        let rename_view = view.clone();
        let persist_name = name.clone();
        let persist_view = view.clone();
        let delete_name = name.clone();
        let delete_view = view.clone();
        let persist_label = if persisted {
            tr!("globals_menu_session_only")
        } else {
            tr!("globals_menu_persist")
        };

        let items = vec![
            menu_item(
                "globals-menu-rename",
                tr!("globals_menu_rename"),
                move |_e, window, cx| {
                    let name = rename_name.clone();
                    rename_view.update(cx, |this, cx| {
                        this.close_row_menu(cx);
                        this.start_rename(name, window, cx);
                    });
                },
            )
            .icon(Icon::Pencil)
            .into(),
            menu_item(
                "globals-menu-persist",
                persist_label,
                move |_e, _window, cx| {
                    let name = persist_name.clone();
                    persist_view.update(cx, |this, cx| {
                        this.close_row_menu(cx);
                        this.toggle_persist(name, cx);
                    });
                },
            )
            .icon(Icon::Pin)
            .into(),
            menu_divider(),
            menu_item(
                "globals-menu-delete",
                tr!("common_delete"),
                move |_e, _window, cx| {
                    let name = delete_name.clone();
                    delete_view.update(cx, |this, cx| {
                        this.close_row_menu(cx);
                        this.request_delete(name, cx);
                    });
                },
            )
            .icon(Icon::X)
            .color(palette.danger)
            .into(),
        ];

        Some(
            context_menu(menu.position, palette)
                .items(items)
                .dismiss_on_escape(&self.menu_focus)
                .on_dismiss(move |_window, cx| {
                    view.update(cx, |this, cx| this.close_row_menu(cx));
                })
                .into_any_element(),
        )
    }

    fn value_cell(
        &self,
        g: &Global,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let density = cx.density();
        let complex = matches!(g.kind(), VariantKind::Array | VariantKind::Object);
        let text = match &g.value {
            Variant::String(s) => format!("\"{s}\""),
            other => other.to_string(),
        };
        let mut cell = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(text),
            );
        if complex {
            let group: SharedString = format!("globals-inspect-{}", g.name).into();
            let hover_bg = palette.surface_overlay;
            let idle = palette.text_secondary;
            let active = palette.brand;
            let target = g.clone();
            cell = cell.child(
                div()
                    .id((gpui::ElementId::from("globals-inspect"), g.name.clone()))
                    .group(group.clone())
                    .flex()
                    .items_center()
                    .justify_center()
                    .p(px(2.0))
                    .rounded(radius(Radius::Sm))
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover_bg))
                    .key_activated(palette.text_primary)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.open_inspect(target.clone(), cx);
                    }))
                    .child(
                        svg()
                            .flex_none()
                            .size(VALUE_ICON)
                            .path(Icon::ExternalLink.path())
                            .text_color(idle)
                            .group_hover(group, move |s| s.text_color(active)),
                    ),
            );
        }
        cell.into_any_element()
    }

    pub(super) fn open_inspect(&mut self, g: Global, cx: &mut Context<Self>) {
        self.table_keys.move_to(g.name.clone());
        self.inspecting = Some(g);
        cx.notify();
    }

    pub(super) fn close_inspect(&mut self, cx: &mut Context<Self>) {
        self.inspecting = None;
        cx.notify();
    }

    fn row_action(
        &self,
        id: impl Into<gpui::ElementId>,
        glyph: Icon,
        idle: Rgba,
        hover_bg: Rgba,
        handler: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> impl IntoElement {
        div()
            .id(id.into())
            .flex()
            .items_center()
            .justify_center()
            .p(px(4.0))
            .rounded(radius(Radius::Sm))
            .cursor_pointer()
            .hover(move |s| s.bg(hover_bg))
            .key_activated(idle)
            .on_click(handler)
            .child(icon(glyph, FONT_XS, idle))
    }
}
