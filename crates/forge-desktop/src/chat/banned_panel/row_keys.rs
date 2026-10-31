use forge_components::{ForgePalette, Icon, context_menu, menu_header, menu_item, tr};
use forge_types::PlatformId;
use gpui::{AnyElement, ClickEvent, Context, Pixels, Point, SharedString, Window, prelude::*};

use super::BannedPanel;
use super::ban_group::{BanRow, GroupStatus};
use crate::navigable_list::{ListStep, NavigableList};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum BanCursor {
    Viewer {
        platform: PlatformId,
        viewer_id: String,
    },
    LoadMore(PlatformId),
    Retry(PlatformId),
}

pub(super) struct RowMenu {
    platform: PlatformId,
    viewer_id: String,
    position: Point<Pixels>,
}

impl NavigableList for BannedPanel {
    fn step_list(&mut self, step: ListStep, cx: &mut Context<Self>) {
        let rows = self.cursor_rows();
        if self.row_keys.step(&rows, None, step).is_some() {
            cx.notify();
        }
    }

    fn activate_list(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.row_keys.activation_target(&self.cursor_rows()) {
            Some(BanCursor::Viewer {
                platform,
                viewer_id,
            }) => self.open_row_menu(platform, viewer_id, cx),
            Some(BanCursor::LoadMore(platform)) => self.load_more(platform, cx),
            Some(BanCursor::Retry(platform)) => self.retry(platform, cx),
            None => {}
        }
    }

    fn open_list_menu(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(BanCursor::Viewer {
            platform,
            viewer_id,
        }) = self.row_keys.activation_target(&self.cursor_rows())
        {
            self.open_row_menu(platform, viewer_id, cx);
        }
    }
}

impl BannedPanel {
    pub(super) fn cursor_rows(&self) -> Vec<BanCursor> {
        let mut rows = Vec::new();
        for group in &self.groups {
            let platform = group.platform;
            match group.status {
                GroupStatus::Loading => {}
                GroupStatus::Unavailable(_) => rows.push(BanCursor::Retry(platform)),
                GroupStatus::Loaded => {
                    rows.extend(group.rows.iter().map(|row| BanCursor::Viewer {
                        platform,
                        viewer_id: row.entry.viewer_id.clone(),
                    }));
                    if group.next.is_some() {
                        rows.push(BanCursor::LoadMore(platform));
                    }
                }
            }
        }
        rows
    }

    #[cfg(test)]
    pub(crate) fn viewers_at_cursor(&self) -> Vec<String> {
        self.cursor_rows()
            .iter()
            .filter(|row| self.row_keys.is_at(row))
            .filter_map(|row| match row {
                BanCursor::Viewer { viewer_id, .. } => Some(viewer_id.clone()),
                BanCursor::LoadMore(_) | BanCursor::Retry(_) => None,
            })
            .collect()
    }

    pub(super) fn point_at(&mut self, cursor: BanCursor, cx: &mut Context<Self>) {
        self.row_keys.move_to(cursor);
        cx.notify();
    }

    fn open_row_menu(&mut self, platform: PlatformId, viewer_id: String, cx: &mut Context<Self>) {
        let position = self
            .row_keys
            .row_bounds()
            .map(|row| row.bottom_left())
            .unwrap_or_default();
        self.row_menu = Some(RowMenu {
            platform,
            viewer_id,
            position,
        });
        cx.notify();
    }

    fn close_row_menu(&mut self, cx: &mut Context<Self>) {
        self.row_menu = None;
        cx.notify();
    }

    fn row_of(&self, platform: PlatformId, viewer_id: &str) -> Option<&BanRow> {
        self.groups
            .iter()
            .find(|group| group.platform == platform)?
            .rows
            .iter()
            .find(|row| row.entry.viewer_id == viewer_id)
    }

    pub(super) fn drop_row_menu_of_a_gone_row(&mut self) {
        let gone = self
            .row_menu
            .as_ref()
            .is_some_and(|menu| self.row_of(menu.platform, &menu.viewer_id).is_none());
        if gone {
            self.row_menu = None;
        }
    }

    pub(super) fn render_row_menu(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let menu = self.row_menu.as_ref()?;
        let platform = menu.platform;
        let row = self.row_of(platform, &menu.viewer_id)?;
        let viewer_id = menu.viewer_id.clone();
        let view = cx.entity();
        let unban = menu_item(
            "chat-banned-menu-unban",
            tr!("chat_banned_unban"),
            cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.unban(platform, viewer_id.clone(), cx);
            }),
        )
        .icon(Icon::Ban)
        .disabled(!row.can_unban(platform));
        Some(
            context_menu(menu.position, palette)
                .items(vec![
                    menu_header(SharedString::from(row.entry.name.clone())),
                    unban.into(),
                ])
                .dismiss_on_escape(&self.menu_focus)
                .on_dismiss(move |_window, cx| {
                    view.update(cx, |this, cx| this.close_row_menu(cx));
                })
                .into_any_element(),
        )
    }
}
