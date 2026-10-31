use forge_components::{
    BORDER_THIN, Density, FONT_XS, FONT_XXS, ForgePalette, Spacing, body_family, empty_state,
    ghost_button, mono_family, secondary_button, spacing, status_dot, tr,
};
use forge_platform_core::{BanDuration, UnbanAbility};
use forge_storage::BAN_LEDGER_LIST_CAP;
use forge_types::PlatformId;
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, Pixels, SharedString, div, prelude::*, px,
};

use super::BannedPanel;
use super::ban_group::{BanGroup, BanRow, GroupStatus, UnbanState, lists_only_what_forge_saw};
use super::row_keys::BanCursor;
use crate::chat::ban_labels::{
    banned_at_label, expiry_label, optional_label, refusal_label, unavailable_label,
};
use crate::chat::platform_display_name;
use crate::chat::platform_gate::platform_integration;
use crate::integration_disabled::disclaimer_line;
use crate::platform_visuals::platform_color;

const GROUP_DOT: Pixels = px(6.0);
const NAME_COLUMN_W: Pixels = px(160.0);
const MODERATOR_COLUMN_W: Pixels = px(130.0);
const BANNED_AT_COLUMN_W: Pixels = px(90.0);
const EXPIRY_COLUMN_W: Pixels = px(170.0);
const UNBAN_COLUMN_W: Pixels = px(84.0);

impl BannedPanel {
    fn render_note(text: impl Into<SharedString>, color: gpui::Rgba) -> AnyElement {
        div()
            .font_family(body_family())
            .text_size(FONT_XS)
            .text_color(color)
            .child(text.into())
            .into_any_element()
    }

    pub(super) fn render_group_header(
        group: &BanGroup,
        palette: &ForgePalette,
        density: Density,
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
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(palette.text_primary)
                    .child(platform_display_name(group.platform)),
            )
            .child(
                div()
                    .font_family(mono_family())
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
    ) -> Option<AnyElement> {
        let disclaimer = disclaimer_line(
            &platform_integration(platform).builtin_id(),
            palette,
            density,
        );
        let seen_note = lists_only_what_forge_saw(platform).then(|| {
            div()
                .font_family(body_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(tr!("chat_banned_seen_by_forge"))
                .into_any_element()
        });
        let capped_note = (lists_only_what_forge_saw(platform)
            && listed_rows >= BAN_LEDGER_LIST_CAP)
            .then(|| {
                div()
                    .font_family(body_family())
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

    fn render_column_header(palette: &ForgePalette, density: Density) -> AnyElement {
        let cell = |label: String, width: Option<Pixels>| {
            let base = div()
                .font_family(mono_family())
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
        let text_cell = |text: String, color: gpui::Rgba, width: Option<Pixels>| {
            let base = div()
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(color)
                .truncate()
                .child(text);
            match width {
                Some(width) => base.flex_none().w(width),
                None => base.flex_1().min_w(px(0.0)),
            }
        };
        let viewer_id = entry.viewer_id.clone();
        let unban_button = secondary_button(tr!("chat_banned_unban"), palette)
            .density(density)
            .disabled(!row.can_unban(platform))
            .busy(row.unban == UnbanState::Running)
            .on_click(
                SharedString::from(format!("chat-banned-unban-{platform:?}-{viewer_id}")),
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.unban(platform, viewer_id.clone(), cx);
                }),
            );
        let note = match (&entry.unban, &row.unban) {
            (_, UnbanState::Failed(error)) => Some(Self::render_note(
                tr!("chat_banned_unban_failed", error = error.clone()),
                palette.danger,
            )),
            (UnbanAbility::Refused(refusal), _) => Some(Self::render_note(
                refusal_label(*refusal),
                palette.text_faint,
            )),
            (UnbanAbility::Allowed, _) => None,
        };

        let mut shown = div()
            .id(row_id)
            .relative()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Xs, density))
            .border_t(BORDER_THIN)
            .border_color(palette.border_regular)
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.point_at(cursor.clone(), cx)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Sm, density))
                    .child(text_cell(
                        entry.name.clone(),
                        palette.text_primary,
                        Some(NAME_COLUMN_W),
                    ))
                    .child(text_cell(
                        optional_label(entry.reason.as_deref()),
                        palette.text_secondary,
                        None,
                    ))
                    .child(text_cell(
                        optional_label(entry.moderator.as_deref()),
                        palette.text_secondary,
                        Some(MODERATOR_COLUMN_W),
                    ))
                    .child(text_cell(
                        banned_at_label(entry.created_at),
                        palette.text_muted,
                        Some(BANNED_AT_COLUMN_W),
                    ))
                    .child(text_cell(
                        expiry_label(entry.duration),
                        match entry.duration {
                            BanDuration::Permanent => palette.danger,
                            BanDuration::Until(_) => palette.warning,
                        },
                        Some(EXPIRY_COLUMN_W),
                    ))
                    .child(
                        div()
                            .flex_none()
                            .w(UNBAN_COLUMN_W)
                            .flex()
                            .justify_end()
                            .child(unban_button),
                    ),
            )
            .children(note);
        if at_cursor {
            shown = shown
                .bg(palette.elevated)
                .child(self.row_keys.row_anchor(Some(&self.list_scroll)));
        }
        shown.into_any_element()
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
        let padded = |content: AnyElement| {
            div()
                .px(spacing(Spacing::Sm, density))
                .py(spacing(Spacing::Sm, density))
                .flex()
                .items_center()
                .gap(spacing(Spacing::Sm, density))
                .child(content)
        };
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
            ))
            .into_any_element(),
            GroupStatus::Loaded => {
                let rows: Vec<AnyElement> = group
                    .rows
                    .iter()
                    .map(|row| self.render_row(platform, row, palette, density, cx))
                    .collect();
                let more = group.next.as_ref().map(|_| {
                    let failed = group
                        .more_failed
                        .map(|reason| Self::render_note(unavailable_label(reason), palette.danger));
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
                    .child(Self::render_column_header(palette, density))
                    .children(rows)
                    .children(more)
                    .into_any_element()
            }
        }
    }
}
