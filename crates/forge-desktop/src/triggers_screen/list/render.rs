use super::*;
use forge_components::ActivePresentation;
use gpui::App;

impl Render for TriggerList {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        drive_overlay_focus(
            self.menu_open.is_some(),
            &self.menu_focus,
            &mut self.menu_focus_restore,
            window,
            cx,
        );
        if !self.focused_once {
            self.focused_once = true;
            window.focus(&self.list_focus, cx);
        }
        if let Some(id) = self.rename_requested.take() {
            self.start_rename(id, window, cx);
        }
        let palette = cx.palette();
        let list_focus = self.list_focus.clone();
        navigable_list(div(), &list_focus, cx)
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .child(self.render_list(&palette, cx))
    }
}

fn row_used_count(
    instance: &TriggerInstanceRow,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    if instance.used_in_count > 0 {
        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .font_family(cx.body_family())
            .text_size(USED_FS)
            .text_color(palette.text_primary)
            .child(tr!("triggers_used_in_prefix"))
            .child(
                div()
                    .font_family(cx.body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(palette.success)
                    .child(instance.used_in_count.to_string()),
            )
            .into_any_element()
    } else {
        div()
            .italic()
            .font_family(cx.body_family())
            .text_size(USED_FS)
            .text_color(palette.text_faint)
            .child(tr!("triggers_row_unused"))
            .into_any_element()
    }
}

impl TriggerList {
    pub(in crate::triggers_screen) fn render_stats(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> AnyElement {
        header_stats(
            vec![
                header_stat(
                    self.instances.len().to_string(),
                    palette.text_primary,
                    tr!("triggers_stat_instances"),
                ),
                header_stat(
                    self.used_count().to_string(),
                    palette.success,
                    tr!("triggers_stat_used"),
                ),
                header_stat(
                    self.disabled_count().to_string(),
                    palette.warning,
                    tr!("triggers_stat_disabled"),
                ),
            ],
            palette,
            density,
            cx,
        )
        .into_any_element()
    }

    fn divider(palette: &ForgePalette) -> AnyElement {
        div()
            .w(FILTER_DIV_W)
            .h(FILTER_DIV_H)
            .bg(palette.border_regular)
            .into_any_element()
    }

    pub(in crate::triggers_screen) fn render_filter_left(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let mut platform_chips = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density));
        for (platform, count) in self.platform_counts() {
            let active = self.platforms.contains(&platform);
            let label = format!("{} {}", platform.display(), count);
            platform_chips = platform_chips.child(
                chip(
                    label,
                    ChipGlyph::Dot(platform.dot(palette)),
                    active,
                    palette,
                )
                .on_click(
                    SharedString::from(format!("triggers-platform-{}", platform.element_key())),
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.toggle_platform(platform.clone(), cx)
                    }),
                ),
            );
        }
        if !self.platforms.is_empty() {
            platform_chips = platform_chips.child(
                div()
                    .id("triggers-platform-clear")
                    .cursor_pointer()
                    .px(spacing(Spacing::Xs, density))
                    .font_family(cx.body_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(tr!("triggers_platform_clear"))
                    .key_activated(palette.text_primary)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.clear_platforms(cx))),
            );
        }

        let usage_chips = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                chip(
                    tr!("triggers_usage_used"),
                    ChipGlyph::Dot(palette.success),
                    self.usage_filter == UsageFilter::Used,
                    palette,
                )
                .on_click(
                    "triggers-usage-used",
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.set_usage_filter(UsageFilter::Used, cx)
                    }),
                ),
            )
            .child(
                chip(
                    tr!("triggers_usage_unused"),
                    ChipGlyph::Dot(palette.text_faint),
                    self.usage_filter == UsageFilter::Unused,
                    palette,
                )
                .on_click(
                    "triggers-usage-unused",
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.set_usage_filter(UsageFilter::Unused, cx)
                    }),
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
            .child(Self::divider(palette))
            .child(platform_chips)
            .child(Self::divider(palette))
            .child(usage_chips)
            .into_any_element()
    }

    fn columns(
        dot: AnyElement,
        name: AnyElement,
        kind: AnyElement,
        used: AnyElement,
        on: AnyElement,
        menu: AnyElement,
    ) -> Div {
        div()
            .flex()
            .items_center()
            .w_full()
            .child(
                div()
                    .w(COL_DOT)
                    .flex_none()
                    .flex()
                    .items_center()
                    .child(dot),
            )
            .child(div().w(COL_NAME).flex_none().overflow_hidden().child(name))
            .child(div().flex_1().min_w(px(0.0)).child(kind))
            .child(
                div()
                    .w(COL_USED)
                    .flex_none()
                    .flex()
                    .justify_end()
                    .child(used),
            )
            .child(
                div()
                    .w(COL_ON)
                    .flex_none()
                    .flex()
                    .justify_center()
                    .child(on),
            )
            .child(
                div()
                    .w(COL_MENU)
                    .flex_none()
                    .flex()
                    .justify_end()
                    .child(menu),
            )
    }

    fn caption_cell(
        palette: &ForgePalette,
        label: impl Into<SharedString>,
        cx: &App,
    ) -> AnyElement {
        div()
            .font_family(cx.mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(label.into())
            .into_any_element()
    }

    fn render_caption(&self, palette: &ForgePalette, density: Density, cx: &App) -> AnyElement {
        let cols = Self::columns(
            div().into_any_element(),
            Self::caption_cell(palette, tr!("triggers_col_name"), cx),
            Self::caption_cell(palette, tr!("triggers_col_kind"), cx),
            Self::caption_cell(palette, tr!("triggers_col_used"), cx),
            Self::caption_cell(palette, tr!("triggers_col_on"), cx),
            div().into_any_element(),
        );
        div()
            .w_full()
            .flex_none()
            .py(scaled(CAPTION_PAD_V, density))
            .px(scaled(CAPTION_PAD_H, density))
            .bg(palette.shell)
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(cols)
            .into_any_element()
    }

    fn render_list(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        if self.visible.is_empty() {
            let notice = load_notice(
                &self.list_load,
                LoadNotice {
                    id: "triggers-load",
                    loading: tr!("triggers_loading"),
                    width: NoticeWidth::Pane,
                },
                cx.listener(|this, _: &ClickEvent, _, cx| this.retry_list_load(cx)),
                palette,
                density,
                cx,
            )
            .filter(|_| self.instances.is_empty());
            let empty = notice.unwrap_or_else(|| self.render_empty(palette, cx));
            return div()
                .id("triggers-list")
                .flex_1()
                .h_full()
                .overflow_y_revealing_focus()
                .bg(palette.base)
                .child(empty)
                .into_any_element();
        }

        let count = self.visible.len();
        let pal = *palette;
        let rows = uniform_list(
            "triggers-rows",
            count,
            cx.processor(move |this, range: std::ops::Range<usize>, _window, cx| {
                let mut out = Vec::with_capacity(range.len());
                for ix in range {
                    let idx = this.visible[ix];
                    if let Some(instance) = this.instances.get(idx) {
                        out.push(this.render_row(instance, &pal, cx));
                    }
                }
                out
            }),
        )
        .track_scroll(&self.list_scroll)
        .flex_1()
        .min_h(px(0.0));

        div()
            .id("triggers-list")
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(self.render_caption(palette, density, cx))
            .child(rows)
            .into_any_element()
    }

    fn render_row_kind(
        &self,
        instance: &TriggerInstanceRow,
        dot_color: Rgba,
        gate_badges: Vec<AnyElement>,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> gpui::Div {
        let descriptor = self.deps.registry.get(&instance.kind_id);
        let kind_glyph = descriptor
            .map(|d| Icon::from_name(d.icon_name()))
            .unwrap_or(Icon::Bolt);
        let kind_label = descriptor
            .map(crate::kind_copy::trigger_name)
            .unwrap_or_else(|| instance.kind_id.clone());

        let mut kind = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .min_w(px(0.0))
            .child(icon(kind_glyph, KIND_GLYPH, dot_color))
            .child(
                div()
                    .flex_shrink(1.0)
                    .min_w(px(0.0))
                    .truncate()
                    .font_family(cx.mono_family())
                    .text_size(KIND_FS)
                    .text_color(palette.text_muted)
                    .child(kind_label),
            );
        if instance.override_count > 0 {
            let label = tr!(
                "triggers_override_badge",
                count = i64::try_from(instance.override_count).unwrap_or(i64::MAX)
            );
            kind = kind.child(badge(
                palette.surface_overlay,
                palette.accent_peach,
                label,
                true,
                BADGE_FS,
            ));
        }
        kind = kind.children(gate_badges);
        let gate = if cooldown_applies(descriptor) {
            cooldown_suffix(instance.cooldown_secs, instance.cooldown_global)
        } else {
            let summary = condition_display(descriptor, &instance.overrides);
            format!(" \u{b7} {summary}")
        };
        kind = kind.child(
            div()
                .flex_none()
                .font_family(cx.mono_family())
                .text_size(KIND_FS)
                .text_color(palette.accent_peach)
                .child(gate),
        );
        if let Some(permission) = descriptor
            .and_then(|d| d.chat_trigger_family())
            .and_then(|_| permission_suffix(instance.permission_rung))
        {
            kind = kind.child(
                div()
                    .flex_none()
                    .font_family(cx.mono_family())
                    .text_size(KIND_FS)
                    .text_color(palette.info)
                    .child(permission),
            );
        }
        kind
    }

    fn render_row_name(
        &self,
        instance: &TriggerInstanceRow,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = instance.id;
        let name_color = if !instance.enabled {
            palette.text_muted
        } else {
            palette.text_primary
        };
        match self.rename.as_ref().filter(|r| r.id == id) {
            Some(rename) => div()
                .w(COL_NAME)
                .flex_none()
                .child(rename.editor.clone())
                .into_any_element(),
            None => div()
                .w(COL_NAME)
                .flex_none()
                .truncate()
                .font_family(cx.mono_family())
                .text_size(NAME_FS)
                .text_color(name_color)
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        if event.click_count >= 2 {
                            this.start_rename(id, window, cx);
                        }
                    }),
                )
                .child(instance.name.clone())
                .into_any_element(),
        }
    }

    fn render_row(
        &self,
        instance: &TriggerInstanceRow,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let id = instance.id;
        let selected = self.selected == Some(id);
        let cursor = self.list_cursor == Some(id);
        let hovered = self.hovered == Some(id);
        let dot_color = platform_dot_color(&instance.kind_id, &self.deps.registry, palette);

        let stripe_color = if selected || cursor {
            dot_color
        } else {
            gpui::transparent_black().into()
        };
        let row_bg: Rgba = if selected || cursor || hovered {
            palette.elevated
        } else {
            gpui::transparent_black().into()
        };

        let switched_off_owner = self.switched_off_owner(&instance.kind_id);
        let uncovered_category = self.uncovered_category(&instance.kind_id);
        let gate_badges: Vec<AnyElement> = switched_off_owner
            .iter()
            .map(|owner| inactive_badge(owner, palette, density, cx).into_any_element())
            .chain(uncovered_category.iter().map(|category| {
                category_inactive_badge(*category, palette, density, cx).into_any_element()
            }))
            .collect();
        let kind = self.render_row_kind(instance, dot_color, gate_badges, palette, density, cx);
        let used = row_used_count(instance, palette, density, cx);

        let renaming = self.rename.as_ref().is_some_and(|r| r.id == id);
        let mut select_region = div()
            .id(SharedString::from(format!("triggers-row-select-{id}")))
            .flex_1()
            .flex()
            .items_center()
            .pl(ROW_PAD_L)
            .py(ROW_PAD_V)
            .child(
                div()
                    .w(COL_DOT)
                    .flex_none()
                    .flex()
                    .items_center()
                    .child(status_dot(dot_color, ROW_DOT)),
            )
            .child(self.render_row_name(instance, palette, cx))
            .child(div().flex_1().min_w(px(0.0)).child(kind))
            .child(
                div()
                    .w(COL_USED)
                    .flex_none()
                    .flex()
                    .justify_end()
                    .pr(USED_CELL_GAP)
                    .child(used),
            );
        if !renaming {
            select_region = select_region.cursor_pointer().on_click(cx.listener(
                move |this, _: &ClickEvent, window, cx| {
                    window.focus(&this.list_focus, cx);
                    this.select(id, cx);
                },
            ));
        }

        let on_cell = div().w(COL_ON).flex_none().flex().justify_center().child(
            toggle(instance.enabled, palette).on_click(
                SharedString::from(format!("triggers-toggle-{id}")),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_enable(id, cx)),
            ),
        );

        let menu_cell = div().w(COL_MENU).flex_none().flex().justify_end().child(
            menu_button(Icon::DotsVertical, false, palette).on_toggle(
                SharedString::from(format!("triggers-row-menu-{id}")),
                cx.listener(move |this, event: &ClickEvent, _, cx| {
                    this.open_row_menu(id, event.position(), cx)
                }),
            ),
        );

        let content = div()
            .w_full()
            .flex()
            .items_center()
            .pr(scaled(ROW_PAD_R, density))
            .child(select_region)
            .child(on_cell)
            .child(menu_cell);

        div()
            .id(SharedString::from(format!("triggers-row-{id}")))
            .w_full()
            .flex()
            .bg(row_bg)
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .when(
                !instance.enabled || switched_off_owner.is_some() || uncovered_category.is_some(),
                |row| row.opacity(ROW_OFF_OPACITY),
            )
            .on_hover(
                cx.listener(move |this, hovered: &bool, _, cx| this.set_hover(id, *hovered, cx)),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                    this.open_row_menu(id, ev.position, cx)
                }),
            )
            .child(div().w(STRIPE_W).flex_none().bg(stripe_color))
            .child(content)
            .into_any_element()
    }

    fn render_empty(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let has_filter = self.has_active_filter();
        let (glyph, glyph_color) = if has_filter {
            (Icon::MoodSmile, palette.text_faint)
        } else {
            (Icon::Bolt, palette.warning)
        };
        let title = if has_filter {
            tr!("triggers_no_results_title")
        } else {
            tr!("triggers_empty_title")
        };
        let body = if has_filter {
            tr!("triggers_no_results_hint")
        } else {
            tr!("triggers_empty_hint")
        };

        let action: AnyElement = if has_filter {
            ghost_button_with_icon(Icon::X, tr!("triggers_clear_filters"), palette)
                .on_click(
                    "triggers-empty-clear",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.clear_filters(cx)),
                )
                .into_any_element()
        } else {
            primary_button_with_icon(Icon::Plus, tr!("triggers_empty_create_first"), palette)
                .on_click(
                    "triggers-empty-create",
                    cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(TriggerListEvent::OpenCreate)),
                )
                .into_any_element()
        };

        let tile = div()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(EMPTY_TILE)
            .rounded(EMPTY_TILE_RADIUS)
            .bg(palette.shell)
            .child(icon(glyph, EMPTY_GLYPH, glyph_color));

        div()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(scaled(EMPTY_PAD_V, density))
            .px(scaled(EMPTY_PAD_H, density))
            .child(tile)
            .child(
                div()
                    .font_family(cx.body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(EMPTY_TITLE_FS)
                    .text_color(palette.text_primary)
                    .child(title),
            )
            .child(
                div()
                    .max_w(px(360.0))
                    .text_center()
                    .font_family(cx.body_family())
                    .text_size(EMPTY_BODY_FS)
                    .text_color(palette.text_muted)
                    .child(body),
            )
            .child(action)
            .into_any_element()
    }
}
