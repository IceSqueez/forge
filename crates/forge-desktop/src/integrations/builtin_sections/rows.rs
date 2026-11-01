use super::*;

pub(super) fn content_list_item_row(
    item: &ContentListItem,
    row_index: usize,
    rows: &RowHooks,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    let dim = if item.enabled { 1.0 } else { 0.5 };
    let text_color = with_alpha(
        if !item.enabled {
            palette.text_faint
        } else if item.active {
            palette.text_primary
        } else {
            palette.text_secondary
        },
        dim,
    );
    let icon_color = with_alpha(
        match &item.icon_tint {
            Some(tint) => token_color_value(tint, palette),
            None if item.active => palette.success,
            None => palette.text_faint,
        },
        dim,
    );

    let (name_family, name_font) = if item.monospace_name {
        (cx.mono_family(), ROW_MONO_NAME_FONT)
    } else {
        (cx.body_family(), ROW_NAME_FONT)
    };
    let name_el = div()
        .flex_1()
        .min_w(px(0.0))
        .font_family(name_family)
        .text_size(name_font)
        .text_color(text_color)
        .child(item.name.clone());

    let mut trailing = div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, density));
    if item.active
        && let Some(label) = &item.active_label
    {
        trailing = trailing.child(active_badge(&user_text_string(label), palette, density, cx));
    }
    for (index, token) in item.trailing.iter().enumerate() {
        let site = TokenSite {
            hooks: rows,
            row: &item.name,
            row_index,
            index,
        };
        trailing = trailing.child(trailing_token_elem(token, dim, &site, palette, density, cx));
    }

    let content = div()
        .debug_selector(|| rows.element_id("row", row_index).to_string())
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Sm, density))
        .child(icon(
            Icon::from_name(item.icon.as_str()),
            ROW_GLYPH,
            icon_color,
        ))
        .child(name_el)
        .child(trailing);

    if item.active {
        return active_row_wrapper(content.into_any_element(), rows.pad_y, palette, density);
    }
    match (&item.on_click, &rows.run_step) {
        (Some(step), Some(run)) => {
            let run = Rc::clone(run);
            let click = StepClick {
                row: SharedString::from(item.name.clone()),
                step: step.clone(),
            };
            let hover_bg = palette.surface_overlay;
            row_surface(rows.pad_y, palette, density)
                .id(rows.element_id("row", row_index))
                .cursor_pointer()
                .hover(move |s| s.bg(hover_bg))
                .key_activated(palette.text_primary)
                .on_click(move |_: &ClickEvent, window, cx| run(&click, window, cx))
                .child(content)
                .into_any_element()
        }
        _ => plain_row_wrapper(content.into_any_element(), rows.pad_y, palette, density),
    }
}

pub(super) fn key_value_row_elem(
    item: &KeyValueRow,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    let name_el = div()
        .flex_1()
        .min_w(px(0.0))
        .font_family(cx.mono_family())
        .text_size(FONT_SM)
        .text_color(palette.text_primary)
        .child(user_text_string(&item.name));

    let mut row = div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Sm, density))
        .child(icon(
            Icon::from_name(item.icon.as_str()),
            FONT_SM,
            palette.text_secondary,
        ))
        .child(name_el);
    if let Some(tag) = &item.tag {
        row = row.child(mono(user_text_string(tag), FONT_XS, palette.text_faint, cx));
    }
    if let Some(action) = &item.action {
        let ic = match action {
            RowAction::Play => Icon::PlayerPlay,
        };
        row = row.child(icon(ic, FONT_SM, palette.success));
    }
    plain_row_wrapper(row.into_any_element(), DEFAULT_ROW_PAD_Y, palette, density)
}

pub(super) fn active_item_row_elem(
    item: &ActiveRow,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    let text_color = if item.active {
        palette.text_primary
    } else {
        palette.text_secondary
    };
    let name_el = div()
        .flex_1()
        .min_w(px(0.0))
        .font_family(cx.mono_family())
        .text_size(FONT_SM)
        .text_color(text_color)
        .child(item.name.clone());

    let mut row = div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Sm, density))
        .child(name_el);
    if item.active {
        row = row.child(active_badge(
            &tr!("widget_builtin_active_badge"),
            palette,
            density,
            cx,
        ));
    } else if let Some(mode) = &item.mode_label {
        row = row.child(body(
            user_text_string(mode),
            FONT_XS,
            palette.text_faint,
            cx,
        ));
    }

    if item.active {
        active_row_wrapper(row.into_any_element(), DEFAULT_ROW_PAD_Y, palette, density)
    } else {
        plain_row_wrapper(row.into_any_element(), DEFAULT_ROW_PAD_Y, palette, density)
    }
}

pub(super) fn subscription_row_elem(
    item: &SubscriptionRow,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    let name_el = div()
        .flex_1()
        .min_w(px(0.0))
        .overflow_hidden()
        .truncate()
        .font_family(cx.mono_family())
        .text_size(FONT_XS)
        .text_color(palette.text_primary)
        .child(user_text_string(&item.name));

    let mut row = div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Sm, density))
        .child(status_dot(
            subscription_status_color(&item.status, palette),
            SUBSCRIPTION_DOT,
        ))
        .child(name_el);
    if let Some(ver) = &item.version {
        row = row.child(mono(ver.clone(), FONT_XXS, palette.text_faint, cx));
    }
    let trailing: AnyElement = if let Some(err) = &item.error_label {
        body(user_text_string(err), FONT_XXS, palette.danger, cx).into_any_element()
    } else if let Some(count) = item.event_count {
        body(
            tr!(
                "widget_builtin_event_count",
                count = i64::try_from(count).unwrap_or(i64::MAX)
            ),
            FONT_XXS,
            palette.text_muted,
            cx,
        )
        .into_any_element()
    } else {
        div().into_any_element()
    };
    row = row.child(trailing);
    plain_row_wrapper(row.into_any_element(), DEFAULT_ROW_PAD_Y, palette, density)
}

pub(super) fn scope_row_elem(
    scope: &str,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    let row = div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Sm, density))
        .child(icon(Icon::CircleCheck, FONT_XS, palette.success))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .font_family(cx.mono_family())
                .text_size(FONT_XS)
                .text_color(palette.text_primary)
                .child(scope.to_owned()),
        );
    plain_row_wrapper(row.into_any_element(), DEFAULT_ROW_PAD_Y, palette, density)
}

pub(super) fn active_row_wrapper(
    content: AnyElement,
    pad_y: f32,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let padded = div()
        .flex_1()
        .min_w(px(0.0))
        .py(px(pad_y))
        .px(scaled(ROW_PAD_X, density))
        .child(content);
    div()
        .w_full()
        .flex()
        .bg(palette.shell)
        .child(div().flex_none().w(ACCENT_EDGE_W).bg(palette.success))
        .child(padded)
        .into_any_element()
}

pub(super) fn row_surface(pad_y: f32, palette: &ForgePalette, density: Density) -> Div {
    div()
        .w_full()
        .py(px(pad_y))
        .px(scaled(ROW_PAD_X, density))
        .bg(palette.elevated)
}

pub(super) fn plain_row_wrapper(
    content: AnyElement,
    pad_y: f32,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    row_surface(pad_y, palette, density)
        .child(content)
        .into_any_element()
}

pub(super) fn active_badge(
    label: &str,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    div()
        .flex_none()
        .py(BADGE_PAD_V)
        .px(spacing(Spacing::Xs, density))
        .rounded(radius(Radius::Md))
        .bg(palette.surface_overlay)
        .child(body(
            label.to_uppercase(),
            PANEL_BADGE_FONT,
            palette.success,
            cx,
        ))
        .into_any_element()
}

pub(super) fn trailing_token_elem(
    token: &TrailingToken,
    dim: f32,
    site: &TokenSite,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    match token {
        TrailingToken::Badge(label, color) => {
            let tc = with_alpha(token_color_value(color, palette), dim);
            div()
                .flex_none()
                .py(BADGE_PAD_V)
                .px(spacing(Spacing::Xs, density))
                .rounded(radius(Radius::Md))
                .bg(palette.surface_overlay)
                .child(body(user_text_string(label), PANEL_BADGE_FONT, tc, cx))
                .into_any_element()
        }
        TrailingToken::Icon(ic, color) => {
            let tc = with_alpha(token_color_value(color, palette), dim);
            icon(Icon::from_name(ic.as_str()), ROW_TRAILING_GLYPH, tc).into_any_element()
        }
        TrailingToken::Label(label) => mono(
            user_text_string(label),
            FONT_XXS,
            with_alpha(palette.text_faint, dim),
            cx,
        )
        .into_any_element(),
        TrailingToken::TintedLabel(label, color) => mono(
            user_text_string(label),
            ROW_TINTED_LABEL_FONT,
            with_alpha(token_color_value(color, palette), dim),
            cx,
        )
        .into_any_element(),
        TrailingToken::ActionIcon {
            icon: ic,
            tint,
            step,
        } => {
            let tc = with_alpha(token_color_value(tint, palette), dim);
            let glyph = Icon::from_name(ic.as_str());
            let Some(run) = site.hooks.run_step.clone() else {
                return icon(glyph, ROW_TRAILING_GLYPH, tc).into_any_element();
            };
            let hover_tint = palette.text_primary;
            let click = StepClick {
                row: SharedString::from(site.row.to_owned()),
                step: step.clone(),
            };
            let element_id = site
                .hooks
                .element_id(&format!("token-{}", site.index), site.row_index);
            div()
                .debug_selector(|| element_id.to_string())
                .id(element_id.clone())
                .flex_none()
                .size(ROW_TRAILING_GLYPH)
                .cursor_pointer()
                .key_activated(palette.text_primary)
                .on_click(move |_: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    run(&click, window, cx);
                })
                .child(
                    svg()
                        .size(ROW_TRAILING_GLYPH)
                        .path(glyph.path())
                        .text_color(tc)
                        .hover(move |s| s.text_color(hover_tint)),
                )
                .into_any_element()
        }
    }
}
