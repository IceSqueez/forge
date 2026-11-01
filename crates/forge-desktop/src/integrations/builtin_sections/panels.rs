use super::*;

#[derive(Clone, Copy)]
pub(crate) struct PanelIcon<'a> {
    name: &'a str,
    tint: Option<&'a Tone>,
}

impl<'a> PanelIcon<'a> {
    pub(super) fn of(icon: &'a SectionIcon, tint: &'a Option<Tone>) -> Self {
        Self {
            name: icon.as_str(),
            tint: tint.as_ref(),
        }
    }

    pub(crate) fn untinted(name: &'a str) -> Self {
        Self { name, tint: None }
    }

    pub(super) fn color(self, palette: &ForgePalette) -> Rgba {
        self.tint.map_or(palette.text_secondary, |tint| {
            token_color_value(tint, palette)
        })
    }
}

pub(crate) fn panel_header_row(
    panel_icon: PanelIcon<'_>,
    title: &str,
    inline_label: Option<&str>,
    count: Option<&str>,
    refresh: Option<SectionRefresh>,
    density: Density,
    cx: &App,
) -> AnyElement {
    let palette = &cx.palette();
    let mut left = div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .items_center()
        .gap(scaled(PANEL_TITLE_GAP, density))
        .child(icon(
            Icon::from_name(panel_icon.name),
            FONT_SM,
            panel_icon.color(palette),
        ))
        .child(
            body(title.to_owned(), PANEL_TITLE_FONT, palette.text_primary, cx)
                .font_weight(FontWeight::MEDIUM),
        );
    if let Some(label) = inline_label {
        left = left.child(body(
            label.to_owned(),
            PANEL_TITLE_FONT,
            palette.text_faint,
            cx,
        ));
    }

    let mut row = div()
        .w_full()
        .flex_none()
        .flex()
        .items_center()
        .py(spacing(Spacing::Sm, density))
        .px(scaled(PANEL_ROW_PAD_H, density))
        .child(left);
    if let Some(c) = count {
        row = row.child(mono(c.to_owned(), FONT_XXS, palette.text_faint, cx));
    }
    if let Some(handler) = refresh {
        row = row.child(
            div()
                .id(SharedString::from(format!("panel-refresh-{title}")))
                .flex_none()
                .cursor_pointer()
                .child(icon(Icon::Refresh, PANEL_REFRESH_GLYPH, palette.text_faint))
                .key_activated(palette.text_primary)
                .on_click(move |event, window, cx| handler(event, window, cx)),
        );
    }
    row.into_any_element()
}

pub(super) fn info_card_header(
    title: &str,
    live: bool,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    let mut row = div()
        .w_full()
        .flex()
        .items_center()
        .py(spacing(Spacing::Sm, density))
        .px(spacing(Spacing::Md, density))
        .child(div().flex_1().min_w(px(0.0)).child(body(
            title.to_owned(),
            FONT_SM,
            palette.text_primary,
            cx,
        )));
    if live {
        let badge = div()
            .flex()
            .items_center()
            .gap(scaled(STATUS_BADGE_GAP, density))
            .py(BADGE_PAD_V)
            .px(spacing(Spacing::Xs, density))
            .rounded(PILL_RADIUS)
            .bg(palette.surface_overlay)
            .child(status_dot(palette.success, LIVE_DOT))
            .child(body(
                tr!("widget_builtin_live_badge"),
                FONT_XS,
                palette.success,
                cx,
            ));
        row = row.child(badge);
    }
    row.into_any_element()
}

pub(super) fn list_footer_bar(
    footer: &ListFooter,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    let mut row = div()
        .w_full()
        .flex_none()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Sm, density))
        .py(scaled(LIST_ROW_PAD_V, density))
        .px(scaled(PANEL_ROW_PAD_H, density))
        .border_t(BORDER_THIN)
        .border_color(palette.border_regular)
        .bg(palette.shell);

    let lead = div().flex_1().min_w(px(0.0));
    if let Some(cta) = &footer.cta_label {
        row = row.child(lead.child(body(user_text_string(cta), FONT_XS, palette.brand, cx)));
    } else {
        row = row.child(lead);
    }
    if let Some(trail) = &footer.trailing_label {
        row = row.child(mono(
            user_text_string(trail),
            FONT_XS,
            palette.text_faint,
            cx,
        ));
    }
    row.into_any_element()
}
