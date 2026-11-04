use super::*;

impl TriggerList {
    pub(in crate::automation::triggers_screen) fn render_modal_layers(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let disable = self
            .confirm_disable
            .get()
            .copied()
            .map(|id| self.render_disable_confirm(id, palette, cx));
        let delete = self
            .pending_delete
            .get()
            .copied()
            .map(|id| self.render_delete_confirm(id, palette, cx));
        let bulk_disable = self
            .confirm_bulk_disable
            .get()
            .map(|ids| self.render_bulk_disable_confirm(ids, palette, cx));
        let bulk_delete = self
            .pending_bulk_delete
            .get()
            .map(|ids| self.render_bulk_delete_confirm(ids, palette, cx));
        let row_menu = self.render_row_context_menu(palette, cx);
        [disable, delete, bulk_disable, bulk_delete, row_menu]
            .into_iter()
            .flatten()
            .collect()
    }

    fn render_row_context_menu(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let id = self.menu_open?;
        let position = self.menu_click_pos?;
        let instance = self.find(id)?;
        let block_delete = instance.used_in_count > 0;
        let view = cx.entity();

        let items = vec![
            menu_item(
                SharedString::from(format!("triggers-menu-rename-{id}")),
                tr!("triggers_menu_rename"),
                cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.start_rename(id, window, cx)
                }),
            )
            .icon(Icon::Pencil)
            .into(),
            menu_divider(),
            menu_item(
                SharedString::from(format!("triggers-menu-delete-{id}")),
                tr!("triggers_menu_delete"),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.request_delete(id, cx)),
            )
            .icon(Icon::Eraser)
            .color(palette.danger)
            .disabled(block_delete)
            .into(),
        ];

        Some(
            context_menu(position, palette)
                .items(items)
                .dismiss_on_escape(&self.menu_focus)
                .on_dismiss(move |_window, cx| {
                    view.update(cx, |this, cx| this.close_menu(cx));
                })
                .into_any_element(),
        )
    }

    fn render_disable_confirm(
        &self,
        id: TriggerInstanceId,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let count = self.find(id).map(|i| i.used_in_count).unwrap_or_default();
        let card = confirm_modal(
            tr!("triggers_confirm_disable_title"),
            tr!(
                "triggers_confirm_disable_body",
                count = i64::try_from(count).unwrap_or(i64::MAX)
            ),
            ConfirmTone::Warning,
            palette,
        )
        .esc_hint(tr!("widget_confirm_esc_to_cancel"))
        .on_cancel(
            "triggers-disable-cancel",
            tr!("triggers_confirm_disable_dismiss"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_disable(cx)),
        )
        .on_confirm(
            "triggers-disable-confirm",
            tr!("triggers_confirm_disable_accept"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_disable_now(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("triggers-disable-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_disable(cx));
            })
            .into_any_element()
    }

    fn render_delete_confirm(
        &self,
        id: TriggerInstanceId,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (name, used_in) = self
            .find(id)
            .map(|i| (i.name.clone(), i.used_in_count))
            .unwrap_or_default();
        let blocked = used_in > 0;
        let message = if blocked {
            tr!("triggers_delete_reference_block")
        } else {
            tr!("triggers_confirm_delete_body")
        };
        let mut card = confirm_modal(
            tr!("triggers_confirm_delete_title"),
            message,
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(name)
        .esc_hint(tr!("widget_confirm_esc_to_cancel"))
        .on_cancel(
            "triggers-delete-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_delete(cx)),
        );
        if !blocked {
            card = card.on_confirm(
                "triggers-delete-confirm",
                tr!("common_delete"),
                cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_delete(cx)),
            );
        }

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("triggers-delete-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_delete(cx));
            })
            .into_any_element()
    }
}
