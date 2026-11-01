use forge_components::{
    ActivePresentation, BORDER_THIN, Density, FONT_XS, FONT_XXS, ForgePalette, Spacing,
    empty_state, ghost_button, scaled, secondary_button, spacing, status_dot, tooltip_builder, tr,
};
use forge_platform_core::{BanDuration, UnbanAbility};
use forge_storage::BAN_LEDGER_LIST_CAP;
use forge_types::PlatformId;
use gpui::{
    AnyElement, App, ClickEvent, Context, FontWeight, Pixels, SharedString, div, prelude::*, px,
    uniform_list,
};

use super::BannedPanel;
use super::ban_group::{BanGroup, BanRow, GroupStatus, UnbanState, lists_only_what_forge_saw};
use super::row_keys::BanCursor;
use crate::audience::chat::ban_labels::{
    banned_at_label, expiry_label, optional_label, refusal_label, unavailable_label,
};
use crate::integrations::integration_catalog::platform_display_name;
use crate::integrations::integration_disabled::disclaimer_line;
use crate::integrations::platform_visuals::platform_color;

const GROUP_DOT: Pixels = px(6.0);
const ROW_HEIGHT: Pixels = px(38.0);
const NAME_COLUMN_W: Pixels = px(160.0);
const MODERATOR_COLUMN_W: Pixels = px(130.0);
const BANNED_AT_COLUMN_W: Pixels = px(90.0);
const EXPIRY_COLUMN_W: Pixels = px(170.0);
const UNBAN_COLUMN_W: Pixels = px(84.0);

impl BannedPanel {
    fn render_note(text: impl Into<SharedString>, color: gpui::Rgba, cx: &App) -> AnyElement {
        div()
            .font_family(cx.body_family())
            .text_size(FONT_XS)
            .text_color(color)
            .child(text.into())
            .into_any_element()
    }

    pub(super) fn render_group_header(
        group: &BanGroup,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> AnyElement {
        let kind = group.platform;
        let count = match group.status {
            GroupStatus::Loaded => group.rows.len().to_string(),
            GroupStatus::Loading | GroupStatus::Unavailable(_) => String::new(),
        };
        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Xs, density))
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(status_dot(platform_color(kind, palette), GROUP_DOT))
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_XS)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(palette.text_primary)
                    .child(platform_display_name(group.platform)),
            )
            .child(
                div()
                    .font_family(cx.mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(count),
            )
            .into_any_element()
    }

    pub(super) fn render_group_notes(
        platform: PlatformId,
        listed_rows: usize,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> Option<AnyElement> {
        let disclaimer = disclaimer_line(&platform.integration_id(), palette, density, cx);
        let seen_note = lists_only_what_forge_saw(platform).then(|| {
            div()
                .font_family(cx.body_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(tr!("chat_banned_seen_by_forge"))
                .into_any_element()
        });
        let capped_note = (lists_only_what_forge_saw(platform)
            && listed_rows >= BAN_LEDGER_LIST_CAP)
            .then(|| {
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(tr!("chat_banned_list_capped", count = BAN_LEDGER_LIST_CAP))
                    .into_any_element()
            });
        if disclaimer.is_none() && seen_note.is_none() && capped_note.is_none() {
            return None;
        }
        Some(
            div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xxs, density))
                .px(spacing(Spacing::Sm, density))
                .py(spacing(Spacing::Xs, density))
                .border_b(BORDER_THIN)
                .border_color(palette.border_regular)
                .children(seen_note)
                .children(capped_note)
                .children(disclaimer)
                .into_any_element(),
        )
    }

    fn render_column_header(palette: &ForgePalette, density: Density, cx: &App) -> AnyElement {
        let cell = |label: String, width: Option<Pixels>| {
            let base = div()
                .font_family(cx.mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(label);
            match width {
                Some(width) => base.flex_none().w(width),
                None => base.flex_1().min_w(px(0.0)),
            }
        };
        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Xxs, density))
            .child(cell(tr!("chat_banned_col_viewer"), Some(NAME_COLUMN_W)))
            .child(cell(tr!("chat_banned_col_reason"), None))
            .child(cell(
                tr!("chat_banned_col_moderator"),
                Some(MODERATOR_COLUMN_W),
            ))
            .child(cell(
                tr!("chat_banned_col_banned"),
                Some(BANNED_AT_COLUMN_W),
            ))
            .child(cell(tr!("chat_banned_col_expires"), Some(EXPIRY_COLUMN_W)))
            .child(div().flex_none().w(UNBAN_COLUMN_W))
            .into_any_element()
    }

    fn render_row_note(id: SharedString, text: String, color: gpui::Rgba, cx: &App) -> AnyElement {
        let selector = id.to_string();
        div()
            .id(id)
            .debug_selector(|| selector)
            .flex_1()
            .min_w(px(0.0))
            .font_family(cx.body_family())
            .text_size(FONT_XS)
            .text_color(color)
            .truncate()
            .tooltip(tooltip_builder(SharedString::from(text.clone())))
            .child(text)
            .into_any_element()
    }

    fn render_row(
        &self,
        platform: PlatformId,
        row: &BanRow,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entry = &row.entry;
        let cursor = BanCursor::Viewer {
            platform,
            viewer_id: entry.viewer_id.clone(),
        };
        let at_cursor = self.row_keys.is_at(&cursor);
        let row_id =
            SharedString::from(format!("chat-banned-row-{platform:?}-{}", entry.viewer_id));
        let row_selector = row_id.to_string();
        let unban_button = Self::unban_button(platform, row, palette, density, cx);
        let note = Self::ban_row_note(platform, row, palette, cx);

        let mut shown = div()
            .id(row_id)
            .debug_selector(|| row_selector)
            .relative()
            .flex()
            .flex_col()
            .justify_center()
            .w_full()
            .h(scaled(ROW_HEIGHT, density))
            .px(spacing(Spacing::Sm, density))
            .border_t(BORDER_THIN)
            .border_color(palette.border_regular)
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.point_at(cursor.clone(), cx)),
            )
            .child(Self::ban_row_columns(
                row,
                note,
                unban_button,
                palette,
                density,
                cx,
            ));
        if at_cursor {
            shown = shown
                .bg(palette.elevated)
                .child(self.row_keys.row_anchor(Some(&self.list_scroll)));
        }
        shown.into_any_element()
    }

    fn ban_text_cell(
        text: String,
        color: gpui::Rgba,
        width: Option<Pixels>,
        cx: &App,
    ) -> gpui::Div {
        let base = div()
            .font_family(cx.body_family())
            .text_size(FONT_XS)
            .text_color(color)
            .truncate()
            .child(text);
        match width {
            Some(width) => base.flex_none().w(width),
            None => base.flex_1().min_w(px(0.0)),
        }
    }

    fn unban_button(
        platform: PlatformId,
        row: &BanRow,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let viewer_id = row.entry.viewer_id.clone();
        secondary_button(tr!("chat_banned_unban"), palette)
            .density(density)
            .disabled(!row.can_unban(platform))
            .busy(row.unban == UnbanState::Running)
            .on_click(
                SharedString::from(format!("chat-banned-unban-{platform:?}-{viewer_id}")),
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.unban(platform, viewer_id.clone(), cx);
                }),
            )
    }

    fn ban_row_note(
        platform: PlatformId,
        row: &BanRow,
        palette: &ForgePalette,
        cx: &App,
    ) -> Option<AnyElement> {
        let entry = &row.entry;
        let note_id =
            SharedString::from(format!("chat-banned-note-{platform:?}-{}", entry.viewer_id));
        match (&entry.unban, &row.unban) {
            (_, UnbanState::Failed(error)) => Some(Self::render_row_note(
                note_id,
                tr!("chat_banned_unban_failed", error = error.clone()),
                palette.danger,
                cx,
            )),
            (UnbanAbility::Refused(refusal), _) => Some(Self::render_row_note(
                note_id,
                refusal_label(*refusal),
                palette.text_faint,
                cx,
            )),
            (UnbanAbility::Allowed, _) => None,
        }
    }

    fn ban_row_columns(
        row: &BanRow,
        note: Option<AnyElement>,
        unban_button: impl IntoElement,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> gpui::Div {
        let entry = &row.entry;
        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(Self::ban_text_cell(
                entry.name.clone(),
                palette.text_primary,
                Some(NAME_COLUMN_W),
                cx,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(Self::ban_text_cell(
                        optional_label(entry.reason.as_deref()),
                        palette.text_secondary,
                        None,
                        cx,
                    ))
                    .children(note),
            )
            .child(Self::ban_text_cell(
                optional_label(entry.moderator.as_deref()),
                palette.text_secondary,
                Some(MODERATOR_COLUMN_W),
                cx,
            ))
            .child(Self::ban_text_cell(
                banned_at_label(entry.created_at),
                palette.text_muted,
                Some(BANNED_AT_COLUMN_W),
                cx,
            ))
            .child(Self::ban_text_cell(
                expiry_label(entry.duration),
                match entry.duration {
                    BanDuration::Permanent => palette.danger,
                    BanDuration::Until(_) => palette.warning,
                },
                Some(EXPIRY_COLUMN_W),
                cx,
            ))
            .child(
                div()
                    .flex_none()
                    .w(UNBAN_COLUMN_W)
                    .flex()
                    .justify_end()
                    .child(unban_button),
            )
    }

    fn keyed_control_row(
        &self,
        cursor: &BanCursor,
        row: gpui::Div,
        palette: &ForgePalette,
    ) -> gpui::Div {
        if self.row_keys.is_at(cursor) {
            row.relative()
                .bg(palette.elevated)
                .child(self.row_keys.row_anchor(Some(&self.list_scroll)))
        } else {
            row
        }
    }

    pub(super) fn render_group_body(
        &self,
        group: &BanGroup,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let platform = group.platform;
        let padded = |content: AnyElement| Self::padded_group_row(content, density);
        match &group.status {
            GroupStatus::Loading => padded(
                empty_state(tr!("chat_banned_loading"), palette)
                    .density(density)
                    .loading(SharedString::from(format!(
                        "chat-banned-loading-{platform:?}"
                    )))
                    .into_any_element(),
            )
            .into_any_element(),
            GroupStatus::Unavailable(reason) => self
                .keyed_control_row(
                    &BanCursor::Retry(platform),
                    padded(Self::render_note(
                        unavailable_label(*reason),
                        palette.text_muted,
                        cx,
                    ))
                    .child(
                        ghost_button(tr!("chat_banned_retry"), palette)
                            .density(density)
                            .on_click(
                                SharedString::from(format!("chat-banned-retry-{platform:?}")),
                                cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    this.retry(platform, cx)
                                }),
                            ),
                    ),
                    palette,
                )
                .into_any_element(),
            GroupStatus::Loaded if group.rows.is_empty() => padded(Self::render_note(
                tr!("chat_banned_empty"),
                palette.text_faint,
                cx,
            ))
            .into_any_element(),
            GroupStatus::Loaded => self.render_loaded_group(group, palette, density, cx),
        }
    }

    fn render_loaded_group(
        &self,
        group: &BanGroup,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let platform = group.platform;
        let padded = |content: AnyElement| Self::padded_group_row(content, density);
        let row_palette = *palette;
        let rows = uniform_list(
            SharedString::from(format!("chat-banned-rows-{platform:?}")),
            group.rows.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _window, cx| {
                let Some(group) = this.groups.iter().find(|g| g.platform == platform) else {
                    return Vec::new();
                };
                group
                    .rows
                    .get(range)
                    .into_iter()
                    .flatten()
                    .map(|row| this.render_row(platform, row, &row_palette, density, cx))
                    .collect()
            }),
        )
        .track_scroll(&group.row_scroll)
        .h(scaled(ROW_HEIGHT, density) * group.rows.len())
        .min_h(px(0.0));
        let more = group.next.as_ref().map(|_| {
            let failed = group
                .more_failed
                .map(|reason| Self::render_note(unavailable_label(reason), palette.danger, cx));
            self.keyed_control_row(
                &BanCursor::LoadMore(platform),
                padded(
                    ghost_button(tr!("chat_banned_load_more"), palette)
                        .density(density)
                        .busy(group.loading_more)
                        .on_click(
                            SharedString::from(format!("chat-banned-more-{platform:?}")),
                            cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.load_more(platform, cx);
                            }),
                        )
                        .into_any_element(),
                )
                .border_t(BORDER_THIN)
                .border_color(palette.border_regular)
                .children(failed),
                palette,
            )
        });
        div()
            .flex()
            .flex_col()
            .min_h(px(0.0))
            .child(Self::render_column_header(palette, density, cx))
            .child(rows)
            .children(more)
            .into_any_element()
    }

    fn padded_group_row(content: AnyElement, density: Density) -> gpui::Div {
        div()
            .px(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Sm, density))
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(content)
    }
}
