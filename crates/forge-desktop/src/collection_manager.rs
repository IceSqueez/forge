use std::sync::Arc;

use forge_components::{
    Confirm, ConfirmTone, ForgePalette, Icon, OverlayPosition, ToastKind, body_family,
    confirm_modal, modal, mono_family, overlay, primary_button_with_icon, secondary_button, tr,
};
use forge_platform_core::{
    BuiltinCollections, CollectionFailure, CollectionItem, CollectionItemId, CollectionMetadata,
    CollectionOutcome, RevisionWait,
};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, Rgba, Subscription, Task, Window, div,
    prelude::*, px,
};
use tokio::runtime::Handle;

use crate::async_bridge;
use crate::collection_form::{CollectionForm, CollectionFormEvent};
use crate::collection_list::{CollectionList, CollectionListEvent, ListLoad};
use crate::collection_text::{collection_failure_message, localized_collection_text};
use crate::presentation::ActivePresentation;
use crate::toasts::PushToast;

const MANAGER_WIDTH: gpui::Pixels = px(560.0);
const BODY_MAX_HEIGHT: gpui::Pixels = px(440.0);
const FOOTER_GAP: gpui::Pixels = px(12.0);
const BUTTON_GAP: gpui::Pixels = px(8.0);
const FOOTER_FONT: gpui::Pixels = px(11.0);

pub enum CollectionManagerEvent {
    Closed,
}

struct OpenForm {
    view: Entity<CollectionForm>,
    _sub: Subscription,
}

pub struct CollectionManager {
    capability: Arc<dyn BuiltinCollections>,
    metadata: CollectionMetadata,
    title: String,
    accent: Rgba,
    rt_handle: Handle,
    list: Entity<CollectionList>,
    form: Option<OpenForm>,
    pending_delete: Confirm<CollectionItemId>,
    listing_ticket: u64,
    connected: bool,
    _list_sub: Subscription,
    _revision_watch: Task<()>,
}

impl EventEmitter<CollectionManagerEvent> for CollectionManager {}

impl CollectionManager {
    pub fn new(
        capability: Arc<dyn BuiltinCollections>,
        metadata: CollectionMetadata,
        accent: Rgba,
        rt_handle: Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let toggles = metadata
            .toggles
            .iter()
            .map(|toggle| (toggle.key.clone(), toggle.label.clone()));
        let list = cx.new(|_| CollectionList::new(toggles));
        let list_sub = cx.subscribe(&list, Self::on_list_event);
        let mut revisions = capability.revisions();
        let revision_watch = cx.spawn(async move |this, cx| {
            while revisions.changed().await == RevisionWait::Changed {
                if this.update(cx, |this, cx| this.reload(cx)).is_err() {
                    break;
                }
            }
        });
        let mut manager = Self {
            title: localized_collection_text(&metadata.label),
            capability,
            metadata,
            accent,
            rt_handle,
            list,
            form: None,
            pending_delete: Confirm::default(),
            listing_ticket: 0,
            connected: true,
            _list_sub: list_sub,
            _revision_watch: revision_watch,
        };
        manager.reload(cx);
        manager
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        self.listing_ticket = self.listing_ticket.wrapping_add(1);
        let ticket = self.listing_ticket;
        self.list.update(cx, |list, cx| list.begin_loading(cx));
        let capability = Arc::clone(&self.capability);
        let collection = self.metadata.id.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move { capability.list(&collection).await },
            move |this, result, cx| this.apply_listing(ticket, result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_listing(
        &mut self,
        ticket: u64,
        result: CollectionOutcome<Vec<CollectionItem>>,
        cx: &mut Context<Self>,
    ) {
        if ticket != self.listing_ticket {
            return;
        }
        match result {
            Err(failure @ (CollectionFailure::NotEligible | CollectionFailure::NotConnected)) => {
                self.form = None;
                let reason = collection_failure_message(&failure);
                self.list
                    .update(cx, |list, cx| list.mark_unavailable(reason, cx));
            }
            result => {
                let listing = result.map_err(|failure| collection_failure_message(&failure));
                self.list
                    .update(cx, |list, cx| list.apply_listing(listing, cx));
            }
        }
        cx.notify();
    }

    pub fn set_connected(&mut self, connected: bool, cx: &mut Context<Self>) {
        if self.connected == connected {
            return;
        }
        self.connected = connected;
        if connected {
            self.reload(cx);
            return;
        }
        self.listing_ticket = self.listing_ticket.wrapping_add(1);
        self.form = None;
        self.pending_delete.cancel();
        let reason = collection_failure_message(&CollectionFailure::NotConnected);
        self.list
            .update(cx, |list, cx| list.mark_unavailable(reason, cx));
        cx.notify();
    }

    fn at_capacity(&self, cx: &Context<Self>) -> bool {
        self.metadata
            .capacity
            .is_some_and(|capacity| self.list.read(cx).count() >= capacity)
    }

    fn can_create(&self, cx: &Context<Self>) -> bool {
        *self.list.read(cx).load() == ListLoad::Ready && !self.at_capacity(cx)
    }

    fn on_list_event(
        &mut self,
        _list: Entity<CollectionList>,
        event: &CollectionListEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            CollectionListEvent::Edit(id) => {
                let item = self.list.read(cx).item(id).cloned();
                if let Some(item) = item {
                    self.open_form(Some(item), cx);
                }
            }
            CollectionListEvent::Delete(id) => {
                self.pending_delete.request(id.clone());
                cx.notify();
            }
            CollectionListEvent::Toggle { item, toggle, on } => {
                self.flip_toggle(item.clone(), toggle.clone(), *on, cx)
            }
            CollectionListEvent::Retry => self.reload(cx),
        }
    }

    fn flip_toggle(
        &mut self,
        item: CollectionItemId,
        toggle: String,
        on: bool,
        cx: &mut Context<Self>,
    ) {
        self.list
            .update(cx, |list, cx| list.set_busy(&item, true, cx));
        let capability = Arc::clone(&self.capability);
        let collection = self.metadata.id.clone();
        let target = item.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                capability
                    .set_toggle(&collection, &target, &toggle, on)
                    .await
            },
            move |this, result, cx| this.apply_item_write(&item, result, cx),
            cx,
        );
    }

    fn apply_item_write(
        &mut self,
        item: &CollectionItemId,
        result: CollectionOutcome<CollectionItem>,
        cx: &mut Context<Self>,
    ) {
        self.list.update(cx, |list, cx| {
            list.set_busy(item, false, cx);
            if let Ok(updated) = &result {
                list.upsert(updated.clone(), cx);
            }
        });
        if let Err(failure) = result {
            report_failure(&failure, cx);
        }
        cx.notify();
    }

    fn open_create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_create(cx) {
            return;
        }
        self.open_form(None, cx);
        self.focus_form(window, cx);
    }

    fn open_form(&mut self, item: Option<CollectionItem>, cx: &mut Context<Self>) {
        let schema = self.metadata.fields.clone();
        let view = cx.new(|cx| CollectionForm::new(&schema, item.as_ref(), cx));
        let sub = cx.subscribe(
            &view,
            |this, _, event: &CollectionFormEvent, cx| match event {
                CollectionFormEvent::Cancelled => this.close_form(cx),
            },
        );
        self.form = Some(OpenForm { view, _sub: sub });
        cx.notify();
    }

    fn focus_form(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(form) = &self.form {
            form.view.update(cx, |form, cx| form.focus(window, cx));
        }
    }

    fn close_form(&mut self, cx: &mut Context<Self>) {
        self.form = None;
        cx.notify();
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.form.as_ref().map(|form| form.view.clone()) else {
            return;
        };
        let (values, editing) = {
            let form = form.read(cx);
            if !form.can_submit(cx) {
                return;
            }
            (form.values(cx), form.editing().cloned())
        };
        form.update(cx, |form, cx| form.set_submitting(true, cx));
        let capability = Arc::clone(&self.capability);
        let collection = self.metadata.id.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                match editing {
                    Some(item) => capability.update(&collection, &item, &values).await,
                    None => capability.create(&collection, &values).await,
                }
            },
            move |this, result, cx| this.apply_save(&form, result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_save(
        &mut self,
        form: &Entity<CollectionForm>,
        result: CollectionOutcome<CollectionItem>,
        cx: &mut Context<Self>,
    ) {
        let still_open = self
            .form
            .as_ref()
            .is_some_and(|open| open.view.entity_id() == form.entity_id());
        match result {
            Ok(item) => {
                cx.push_toast(
                    ToastKind::Success,
                    tr!("collection_saved", title = item.title.clone()),
                );
                self.list.update(cx, |list, cx| list.upsert(item, cx));
                if still_open {
                    self.form = None;
                }
            }
            Err(failure) => {
                form.update(cx, |form, cx| form.set_submitting(false, cx));
                let shown_on_field = match &failure {
                    CollectionFailure::InvalidInput {
                        field: Some(key),
                        message,
                    } if still_open => form.update(cx, |form, cx| {
                        form.show_field_error(key, localized_collection_text(message), cx)
                    }),
                    _ => false,
                };
                if !shown_on_field {
                    report_failure(&failure, cx);
                }
            }
        }
        cx.notify();
    }

    fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.pending_delete.cancel();
        cx.notify();
    }

    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.pending_delete.take() else {
            return;
        };
        self.list
            .update(cx, |list, cx| list.set_busy(&item, true, cx));
        let capability = Arc::clone(&self.capability);
        let collection = self.metadata.id.clone();
        let target = item.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move { capability.delete(&collection, &target).await },
            move |this, result, cx| this.apply_delete(&item, result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_delete(
        &mut self,
        item: &CollectionItemId,
        result: CollectionOutcome<()>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(()) => {
                let title = self
                    .list
                    .read(cx)
                    .item(item)
                    .map(|item| item.title.clone())
                    .unwrap_or_default();
                self.list.update(cx, |list, cx| list.remove(item, cx));
                cx.push_toast(ToastKind::Success, tr!("collection_deleted", title = title));
            }
            Err(failure) => {
                self.list
                    .update(cx, |list, cx| list.set_busy(item, false, cx));
                report_failure(&failure, cx);
            }
        }
        cx.notify();
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        cx.emit(CollectionManagerEvent::Closed);
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        if self.form.is_some() {
            self.close_form(cx);
        } else {
            self.close(cx);
        }
    }

    fn count_label(&self, cx: &Context<Self>) -> String {
        let list = self.list.read(cx);
        if *list.load() != ListLoad::Ready {
            return tr!("collection_count_unknown");
        }
        let count = list.count().to_string();
        match self.metadata.capacity {
            Some(capacity) if list.count() >= capacity => {
                tr!("collection_count_full", capacity = capacity.to_string())
            }
            Some(capacity) => tr!(
                "collection_count_of",
                count = count,
                capacity = capacity.to_string()
            ),
            None => tr!("collection_count", count = count),
        }
    }

    fn list_footer(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let count_color = if self.at_capacity(cx) {
            palette.warning
        } else {
            palette.text_faint
        };
        let create = primary_button_with_icon(Icon::Plus, tr!("collection_new"), palette)
            .disabled(!self.can_create(cx))
            .on_click(
                "collection-manager-new",
                cx.listener(|this, _: &ClickEvent, window, cx| this.open_create(window, cx)),
            );
        let close = secondary_button(tr!("collection_close"), palette).on_click(
            "collection-manager-close-footer",
            cx.listener(|this, _: &ClickEvent, _, cx| this.close(cx)),
        );
        footer_row(
            div()
                .font_family(mono_family())
                .text_size(FOOTER_FONT)
                .text_color(count_color)
                .child(self.count_label(cx))
                .into_any_element(),
            close.into_any_element(),
            create.into_any_element(),
        )
    }

    fn form_footer(
        &self,
        form: &Entity<CollectionForm>,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (can_submit, submitting) = {
            let form = form.read(cx);
            (form.can_submit(cx), form.is_submitting())
        };
        let save = primary_button_with_icon(Icon::Check, tr!("collection_save"), palette)
            .busy(submitting)
            .disabled(!can_submit)
            .on_click(
                "collection-manager-save",
                cx.listener(|this, _: &ClickEvent, _, cx| this.submit(cx)),
            );
        let back = secondary_button(tr!("common_cancel"), palette).on_click(
            "collection-manager-back",
            cx.listener(|this, _: &ClickEvent, _, cx| this.close_form(cx)),
        );
        footer_row(
            div()
                .font_family(body_family())
                .text_size(FOOTER_FONT)
                .text_color(palette.text_faint)
                .child(tr!("collection_form_footer"))
                .into_any_element(),
            back.into_any_element(),
            save.into_any_element(),
        )
    }

    fn delete_overlay(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> Option<AnyElement> {
        let pending = self.pending_delete.get()?;
        let title = self
            .list
            .read(cx)
            .item(pending)
            .map(|item| item.title.clone())
            .unwrap_or_default();
        let card = confirm_modal(
            tr!("collection_delete_title"),
            tr!("collection_delete_body"),
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(title)
        .on_cancel(
            "collection-delete-cancel",
            tr!("widget_confirm_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_delete(cx)),
        )
        .on_confirm(
            "collection-delete-confirm",
            tr!("collection_delete_confirm"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_delete(cx)),
        );
        let view = cx.entity();
        Some(
            overlay(card, palette)
                .position(OverlayPosition::Center)
                .on_dismiss("collection-delete-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel_delete(cx));
                })
                .into_any_element(),
        )
    }
}

impl Render for CollectionManager {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let (content, footer, subtitle) = match self.form.as_ref().map(|form| form.view.clone()) {
            Some(form) => {
                let subtitle = match form.read(cx).editing_title() {
                    Some(title) => tr!("collection_form_editing", title = title.to_owned()),
                    None => tr!("collection_form_creating"),
                };
                let footer = self.form_footer(&form, &palette, cx);
                (form.into_any_element(), footer, subtitle)
            }
            None => (
                self.list.clone().into_any_element(),
                self.list_footer(&palette, cx),
                tr!("collection_modal_subtitle"),
            ),
        };
        let body = div()
            .id("collection-manager-body")
            .w_full()
            .max_h(BODY_MAX_HEIGHT)
            .overflow_y_scroll()
            .child(content);

        let card = modal(self.title.clone(), body, &palette)
            .header_icon(Icon::from_name(self.metadata.icon.as_str()), self.accent)
            .subtitle(subtitle)
            .width(MANAGER_WIDTH)
            .footer(footer)
            .on_close(
                "collection-manager-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.close(cx)),
            );

        let view = cx.entity();
        let manager_overlay = overlay(card, &palette)
            .position(OverlayPosition::Center)
            .on_dismiss("collection-manager-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.dismiss(cx));
            })
            .into_any_element();

        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(manager_overlay)
            .children(self.delete_overlay(&palette, cx))
    }
}

fn footer_row(hint: AnyElement, secondary: AnyElement, primary: AnyElement) -> AnyElement {
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap(FOOTER_GAP)
        .child(hint)
        .child(
            div()
                .flex()
                .items_center()
                .gap(BUTTON_GAP)
                .child(secondary)
                .child(primary),
        )
        .into_any_element()
}

fn report_failure(failure: &CollectionFailure, cx: &mut Context<CollectionManager>) {
    tracing::warn!(%failure, "collection write failed");
    cx.push_toast(ToastKind::Error, collection_failure_message(failure));
}
