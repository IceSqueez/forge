use super::*;
use forge_components::ActivePresentation;

impl HomeView {
    pub(super) fn render_export_menu(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let rows = dropdown_surface(palette)
            .w(EXPORT_MENU_W)
            .child(
                dropdown_row("home-export-full", tr!("home_export_full"), palette)
                    .icon(Some(Icon::Lock))
                    .on_click(
                        cx.listener(|this, _: &ClickEvent, _, cx| this.export_full_backup(cx)),
                    ),
            )
            .child(
                dropdown_row("home-export-pack", tr!("home_export_pack"), palette)
                    .icon(Some(Icon::Category))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.export_pack(cx))),
            )
            .child(
                dropdown_row("home-export-setup", tr!("home_export_setup"), palette)
                    .icon(Some(Icon::Settings))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.export_setup(cx))),
            );
        dropdown(div().flex().justify_end().child(rows))
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_export_menu(cx));
            })
            .into_any_element()
    }

    pub(super) fn render_hero(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let brand_box = div()
            .flex_none()
            .size(HERO_BRAND_BOX)
            .flex()
            .items_center()
            .justify_center()
            .rounded(radius(Radius::Lg))
            .bg(palette.brand)
            .child(
                div()
                    .font_family(cx.body_family())
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(HERO_BRAND_FONT)
                    .text_color(palette.shell)
                    .child("F"),
            );

        let title_col = div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(HERO_TITLE_FONT)
                    .text_color(palette.text_primary)
                    .child("Forge"),
            )
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_muted)
                    .child(tr!("home_hero_tagline")),
            );

        let transfer_busy = self.transfer_busy(cx);
        let import_btn = ghost_button_with_icon(Icon::Download, tr!("home_hero_import"), palette)
            .density(density)
            .disabled(transfer_busy)
            .on_click(
                "home-import",
                cx.listener(|this, _: &ClickEvent, _, cx| this.request_import(cx)),
            );
        let export_btn = ghost_button_with_icon(Icon::ArrowBarUp, tr!("home_hero_export"), palette)
            .density(density)
            .disabled(transfer_busy)
            .on_click(
                "home-export",
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_export_menu(cx)),
            );
        let export_menu = self
            .export_menu_open
            .then(|| self.render_export_menu(palette, cx));
        let export_control = div().relative().child(export_btn).children(export_menu);
        let new_action_btn =
            ghost_button_with_icon(Icon::Plus, tr!("home_hero_new_action"), palette)
                .density(density)
                .on_click(
                    "home-new-action",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.go(Screen::Actions(None), cx)),
                );

        let buttons = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(import_btn)
            .child(export_control)
            .child(new_action_btn);

        let inner = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Md, density))
            .child(brand_box)
            .child(title_col)
            .child(buttons);

        card(inner, palette)
            .full_width()
            .padding_xy(scaled(HERO_PAD_V, density), HERO_PAD_H)
            .radius(Radius::Lg)
    }

    pub(super) fn jump_card(
        &self,
        spec: JumpCard,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let icon_box = div()
            .flex_none()
            .size(JUMP_ICON_BOX)
            .flex()
            .items_center()
            .justify_center()
            .rounded(JUMP_ICON_RADIUS)
            .bg(palette.surface_overlay)
            .child(icon(spec.glyph, JUMP_ICON_GLYPH, spec.glyph_color));

        let label_col = div()
            .flex_1()
            .flex()
            .flex_col()
            .child(
                div()
                    .font_family(cx.mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(spec.section),
            )
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(spec.title),
            );

        let mut head = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(icon_box)
            .child(label_col);
        if spec.warn {
            head = head.child(icon(Icon::AlertTriangle, JUMP_WARN_ICON, palette.warning));
        }

        let stat_row = div()
            .flex()
            .items_end()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .font_family(cx.mono_family())
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(JUMP_STAT_FONT)
                    .text_color(spec.glyph_color)
                    .child(spec.stat),
            )
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(spec.stat_label),
            );

        let hint_row = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(spec.hint),
            )
            .child(icon(Icon::ArrowRight, JUMP_HINT_ARROW, palette.text_faint));

        let border = palette.border_regular;
        let hover_border = palette.border_input;
        let target = spec.target;
        let key = HomeRow::Jump(target.clone());
        let border = if self.row_keys.is_at(&key) {
            palette.brand
        } else {
            border
        };
        let card = div()
            .id(spec.id)
            .relative()
            .flex()
            .flex_1()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Md, density))
            .px(scaled(JUMP_PAD_H, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(border)
            .bg(palette.elevated)
            .cursor_pointer()
            .hover(move |s| s.border_color(hover_border))
            .key_activated(palette.text_primary)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.go(target.clone(), cx)))
            .child(head)
            .child(stat_row)
            .child(hint_row);
        self.with_row_anchor(&key, card).into_any_element()
    }

    pub(super) fn render_jump_cards(
        &self,
        cards: [JumpCard; 3],
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let mut row = div().w_full().flex().gap(spacing(Spacing::Xs, density));
        for spec in cards {
            row = row.child(self.jump_card(spec, palette, density, cx));
        }
        row
    }
}
