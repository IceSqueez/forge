use crate::presentation::ActivePresentation;
use gpui::{
    AnyElement, App, Div, IntoElement, ParentElement, Pixels, RenderOnce, Rgba, SharedString,
    Styled, Window, div, px,
};

use crate::cards::{card, field_hint, field_title};
use crate::icons::{Icon, icon};
use crate::palette::ForgePalette;
use crate::tokens::{
    BORDER_THIN, Density, FONT_XS, FONT_XXS, Spacing, body_family, mono_family, spacing,
};

const LABEL_GAP: Pixels = px(2.0);
const ROW_PAD_V: Pixels = px(8.0);
const LEADING_ICON: Pixels = px(14.0);
const CARD_PAD: Pixels = px(14.0);
const HEADING_GAP: Pixels = px(12.0);
const SECTION_GAP: Pixels = px(18.0);
const GROUP_GAP: Pixels = px(22.0);

#[derive(IntoElement)]
pub struct SettingRow {
    title: SharedString,
    hint: Option<SharedString>,
    control: AnyElement,
    leading: Option<(Icon, Rgba)>,
    stacked: bool,
    shrink_control: bool,
    title_color: Rgba,
    hint_color: Rgba,
    density: Density,
}

pub fn setting_row(
    title: impl Into<SharedString>,
    hint: Option<SharedString>,
    control: impl IntoElement,
    palette: &ForgePalette,
    density: Density,
) -> SettingRow {
    SettingRow {
        title: title.into(),
        hint,
        control: control.into_any_element(),
        leading: None,
        stacked: false,
        shrink_control: false,
        title_color: palette.text_primary,
        hint_color: palette.text_faint,
        density,
    }
}

pub fn setting_value_row(
    title: impl Into<SharedString>,
    value: impl Into<SharedString>,
    palette: &ForgePalette,
    density: Density,
) -> SettingRow {
    let mut row = setting_row(
        title,
        None,
        div()
            .min_w(px(0.0))
            .font_family(mono_family())
            .text_size(FONT_XS)
            .text_color(palette.text_muted)
            .child(value.into()),
        palette,
        density,
    );
    row.shrink_control = true;
    row
}

impl SettingRow {
    #[must_use]
    pub fn leading(mut self, glyph: Icon, color: Rgba) -> Self {
        self.leading = Some((glyph, color));
        self
    }

    #[must_use]
    pub fn stacked(mut self) -> Self {
        self.stacked = true;
        self
    }

    #[must_use]
    pub fn hint_color(mut self, color: Rgba) -> Self {
        self.hint_color = color;
        self
    }
}

impl RenderOnce for SettingRow {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let density = cx.density();
        let mut labels = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.0))
            .gap(LABEL_GAP)
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(self.title_color)
                    .child(self.title),
            );
        if let Some(hint) = self.hint {
            labels = labels.child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XXS)
                    .text_color(self.hint_color)
                    .child(hint),
            );
        }

        let mut heading = div()
            .flex()
            .flex_1()
            .min_w(px(0.0))
            .items_center()
            .gap(spacing(Spacing::Sm, density));
        if let Some((glyph, color)) = self.leading {
            heading = heading.child(icon(glyph, LEADING_ICON, color));
        }
        heading = heading.child(labels);

        if self.stacked {
            return div()
                .w_full()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, self.density))
                .child(heading)
                .child(div().w_full().child(self.control));
        }

        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(spacing(Spacing::Md, self.density))
            .child(if self.shrink_control {
                heading.flex_none()
            } else {
                heading
            })
            .child(if self.shrink_control {
                div()
                    .flex()
                    .flex_1()
                    .min_w(px(0.0))
                    .justify_end()
                    .child(self.control)
            } else {
                div().flex().flex_none().child(self.control)
            })
    }
}

#[derive(IntoElement)]
pub struct SettingRows {
    rows: Vec<AnyElement>,
    divider: Rgba,
    carded: Option<ForgePalette>,
}

pub fn setting_rows(palette: &ForgePalette) -> SettingRows {
    SettingRows {
        rows: Vec::new(),
        divider: palette.border_regular,
        carded: None,
    }
}

impl SettingRows {
    #[must_use]
    pub fn row(mut self, row: impl IntoElement) -> Self {
        self.rows.push(row.into_any_element());
        self
    }

    #[must_use]
    pub fn rows(mut self, rows: impl IntoIterator<Item = AnyElement>) -> Self {
        self.rows.extend(rows);
        self
    }

    #[must_use]
    pub fn in_card(mut self, palette: &ForgePalette) -> Self {
        self.carded = Some(*palette);
        self
    }
}

impl RenderOnce for SettingRows {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let last = self.rows.len().saturating_sub(1);
        let mut list = div().w_full().flex().flex_col();
        for (idx, row) in self.rows.into_iter().enumerate() {
            let mut cell = div().w_full().py(ROW_PAD_V).child(row);
            if idx < last {
                cell = cell.border_b(BORDER_THIN).border_color(self.divider);
            }
            list = list.child(cell);
        }
        match self.carded {
            Some(palette) => card(list, &palette)
                .padding(CARD_PAD)
                .full_width()
                .into_any_element(),
            None => list.into_any_element(),
        }
    }
}

pub fn setting_block(
    title: impl Into<SharedString>,
    hint: Option<SharedString>,
    content: impl IntoElement,
    palette: &ForgePalette,
) -> Div {
    let mut heading = div()
        .flex()
        .flex_col()
        .gap(LABEL_GAP)
        .child(field_title(title, palette));
    if let Some(hint) = hint {
        heading = heading.child(field_hint(hint, palette));
    }
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(HEADING_GAP)
        .child(heading)
        .child(content)
}

pub fn setting_sections(
    sections: impl IntoIterator<Item = AnyElement>,
    palette: &ForgePalette,
) -> Div {
    let mut column = div().w_full().flex().flex_col().gap(SECTION_GAP);
    for (idx, section) in sections.into_iter().enumerate() {
        let mut cell = div().w_full().child(section);
        if idx > 0 {
            cell = cell
                .pt(SECTION_GAP)
                .border_t(BORDER_THIN)
                .border_color(palette.border_regular);
        }
        column = column.child(cell);
    }
    column
}

pub fn setting_groups(groups: impl IntoIterator<Item = AnyElement>) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(GROUP_GAP)
        .children(groups)
}
