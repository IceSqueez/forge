use std::collections::HashSet;

use forge_components::{
    FONT_XS, FONT_XXS, ForgePalette, Icon, badge, body_family, icon, icon_button, mono_family,
    spinner, toggle, tr,
};
use forge_platform_core::{CollectionItem, CollectionItemAccess, CollectionItemId};
use gpui::{
    AnyElement, ClickEvent, Context, EventEmitter, SharedString, Window, div, prelude::*, px,
};

use crate::collection_text::localized_collection_text;
use crate::presentation::ActivePresentation;
use crate::quick_action_field_rows::{FIELD_FONT, failure_with_retry, field_frame};

const LIST_GAP: gpui::Pixels = px(6.0);
const ROW_GAP: gpui::Pixels = px(10.0);
const CONTROL_GAP: gpui::Pixels = px(8.0);
const TOGGLE_LABEL_GAP: gpui::Pixels = px(6.0);
const TITLE_GAP: gpui::Pixels = px(2.0);
const REASON_GAP: gpui::Pixels = px(5.0);
const REASON_FONT: gpui::Pixels = px(11.0);
const EMPTY_FONT: gpui::Pixels = px(12.5);

pub enum CollectionListEvent {
    Edit(CollectionItemId),
    Delete(CollectionItemId),
    Toggle {
        item: CollectionItemId,
        toggle: String,
        on: bool,
    },
    Retry,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListLoad {
    Loading,
    Ready,
    Failed(String),
}

struct ToggleColumn {
    key: String,
    label: String,
}

pub struct CollectionList {
    toggles: Vec<ToggleColumn>,
    items: Vec<CollectionItem>,
    load: ListLoad,
    busy: HashSet<CollectionItemId>,
}

impl EventEmitter<CollectionListEvent> for CollectionList {}

impl CollectionList {
    pub fn new(toggles: impl IntoIterator<Item = (String, String)>) -> Self {
        Self {
            toggles: toggles
                .into_iter()
                .map(|(key, label)| ToggleColumn {
                    key,
                    label: localized_collection_text(&label),
                })
                .collect(),
            items: Vec::new(),
            load: ListLoad::Loading,
            busy: HashSet::new(),
        }
    }

    pub fn load(&self) -> &ListLoad {
        &self.load
    }

    pub fn count(&self) -> usize {
        self.items.len()
    }

    pub fn item(&self, id: &CollectionItemId) -> Option<&CollectionItem> {
        self.items.iter().find(|item| &item.id == id)
    }

    pub fn begin_loading(&mut self, cx: &mut Context<Self>) {
        if self.load != ListLoad::Ready {
            self.load = ListLoad::Loading;
            cx.notify();
        }
    }

    pub fn apply_listing(
        &mut self,
        listing: Result<Vec<CollectionItem>, String>,
        cx: &mut Context<Self>,
    ) {
        match listing {
            Ok(items) => {
                self.busy
                    .retain(|id| items.iter().any(|item| &item.id == id));
                self.items = items;
                self.load = ListLoad::Ready;
            }
            Err(reason) => self.load = ListLoad::Failed(reason),
        }
        cx.notify();
    }

    pub fn upsert(&mut self, item: CollectionItem, cx: &mut Context<Self>) {
        match self
            .items
            .iter_mut()
            .find(|existing| existing.id == item.id)
        {
            Some(existing) => *existing = item,
            None => self.items.push(item),
        }
        cx.notify();
    }

    pub fn remove(&mut self, id: &CollectionItemId, cx: &mut Context<Self>) {
        self.items.retain(|item| &item.id != id);
        self.busy.remove(id);
        cx.notify();
    }

    pub fn set_busy(&mut self, id: &CollectionItemId, busy: bool, cx: &mut Context<Self>) {
        if busy {
            self.busy.insert(id.clone());
        } else {
            self.busy.remove(id);
        }
        cx.notify();
    }

    fn emit_toggle(&mut self, index: usize, toggle_index: usize, cx: &mut Context<Self>) {
        let (Some(item), Some(column)) = (self.items.get(index), self.toggles.get(toggle_index))
        else {
            return;
        };
        if self.busy.contains(&item.id) || item.access != CollectionItemAccess::Manageable {
            return;
        }
        let on = !item.toggles.get(&column.key).copied().unwrap_or(false);
        cx.emit(CollectionListEvent::Toggle {
            item: item.id.clone(),
            toggle: column.key.clone(),
            on,
        });
    }

    fn emit_for(
        &mut self,
        index: usize,
        event: fn(CollectionItemId) -> CollectionListEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(item) = self.items.get(index) else {
            return;
        };
        if self.busy.contains(&item.id) || item.access != CollectionItemAccess::Manageable {
            return;
        }
        cx.emit(event(item.id.clone()));
    }

    fn render_row(
        &self,
        index: usize,
        item: &CollectionItem,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut titles = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(TITLE_GAP)
            .child(
                div()
                    .truncate()
                    .font_family(body_family())
                    .text_size(FIELD_FONT)
                    .text_color(palette.text_primary)
                    .child(item.title.clone()),
            );
        let controls = match &item.access {
            CollectionItemAccess::ReadOnly { reason } => {
                titles = titles.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(REASON_GAP)
                        .child(icon(Icon::Lock, FONT_XXS, palette.text_faint))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .font_family(body_family())
                                .text_size(REASON_FONT)
                                .text_color(palette.text_faint)
                                .child(localized_collection_text(reason)),
                        ),
                );
                badge(
                    palette.surface_overlay,
                    palette.text_muted,
                    tr!("collection_read_only"),
                    true,
                    FONT_XXS,
                )
                .flex_none()
                .into_any_element()
            }
            CollectionItemAccess::Manageable if self.busy.contains(&item.id) => spinner(
                ("collection-row-spin", index),
                Icon::Refresh,
                FONT_XS,
                palette.text_muted,
            )
            .into_any_element(),
            CollectionItemAccess::Manageable => self.render_controls(index, item, palette, cx),
        };
        field_frame(palette)
            .gap(ROW_GAP)
            .child(titles)
            .child(controls)
            .into_any_element()
    }

    fn render_controls(
        &self,
        index: usize,
        item: &CollectionItem,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut controls = div().flex_none().flex().items_center().gap(CONTROL_GAP);
        for (toggle_index, column) in self.toggles.iter().enumerate() {
            let on = item.toggles.get(&column.key).copied().unwrap_or(false);
            let id: SharedString = format!("collection-toggle-{toggle_index}").into();
            controls = controls.child(
                div()
                    .flex()
                    .items_center()
                    .gap(TOGGLE_LABEL_GAP)
                    .child(
                        div()
                            .font_family(mono_family())
                            .text_size(FONT_XXS)
                            .text_color(palette.text_faint)
                            .child(column.label.to_uppercase()),
                    )
                    .child(toggle(on, palette).on_color(palette.success).on_click(
                        (id, index),
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.emit_toggle(index, toggle_index, cx)
                        }),
                    )),
            );
        }
        controls
            .child(icon_button(Icon::Pencil, palette).on_click(
                ("collection-row-edit", index),
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.emit_for(index, CollectionListEvent::Edit, cx)
                }),
            ))
            .child(
                icon_button(Icon::Trash, palette)
                    .ink(palette.random)
                    .on_click(
                        ("collection-row-delete", index),
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.emit_for(index, CollectionListEvent::Delete, cx)
                        }),
                    ),
            )
            .into_any_element()
    }
}

impl Render for CollectionList {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let mut column = div().w_full().flex().flex_col().gap(LIST_GAP);
        match &self.load {
            ListLoad::Loading => {
                column = column.child(
                    field_frame(&palette)
                        .gap(CONTROL_GAP)
                        .child(spinner(
                            "collection-list-spin",
                            Icon::Refresh,
                            FONT_XS,
                            palette.text_muted,
                        ))
                        .child(
                            div()
                                .font_family(body_family())
                                .text_size(FIELD_FONT)
                                .text_color(palette.text_muted)
                                .child(tr!("collection_loading")),
                        ),
                );
            }
            ListLoad::Failed(reason) => {
                column = column.child(failure_with_retry(
                    "collection-list-retry",
                    reason,
                    &palette,
                    cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(CollectionListEvent::Retry)),
                ));
            }
            ListLoad::Ready if self.items.is_empty() => {
                column = column.child(
                    div()
                        .font_family(body_family())
                        .text_size(EMPTY_FONT)
                        .text_color(palette.text_muted)
                        .child(tr!("collection_empty")),
                );
            }
            ListLoad::Ready => {
                for (index, item) in self.items.iter().enumerate() {
                    column = column.child(self.render_row(index, item, &palette, cx));
                }
            }
        }
        column
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use forge_platform_core::CollectionItemAccess;
    use gpui::{Entity, Subscription, TestAppContext};

    use super::*;

    const ENABLED: &str = "enabled";

    #[derive(Debug, PartialEq, Eq)]
    enum Seen {
        Edit(String),
        Delete(String),
        Toggle(String, String, bool),
        Retry,
    }

    struct Recorder {
        seen: Vec<Seen>,
        _sub: Subscription,
    }

    fn item(id: &str, enabled: bool, access: CollectionItemAccess) -> CollectionItem {
        CollectionItem {
            id: CollectionItemId::new(id),
            title: id.to_owned(),
            values: BTreeMap::new(),
            toggles: BTreeMap::from([(ENABLED.to_owned(), enabled)]),
            access,
        }
    }

    fn manageable(id: &str) -> CollectionItem {
        item(id, true, CollectionItemAccess::Manageable)
    }

    fn read_only(id: &str) -> CollectionItem {
        item(
            id,
            true,
            CollectionItemAccess::ReadOnly {
                reason: "elsewhere".to_owned(),
            },
        )
    }

    fn mount(cx: &mut TestAppContext) -> (Entity<CollectionList>, Entity<Recorder>) {
        let list = cx.update(|cx| {
            cx.new(|_| CollectionList::new([(ENABLED.to_owned(), "Enabled".to_owned())]))
        });
        let recorder = cx.update(|cx| {
            cx.new(|cx| Recorder {
                seen: Vec::new(),
                _sub: cx.subscribe(&list, |this: &mut Recorder, _, event, _| {
                    this.seen.push(match event {
                        CollectionListEvent::Edit(id) => Seen::Edit(id.to_string()),
                        CollectionListEvent::Delete(id) => Seen::Delete(id.to_string()),
                        CollectionListEvent::Toggle { item, toggle, on } => {
                            Seen::Toggle(item.to_string(), toggle.clone(), *on)
                        }
                        CollectionListEvent::Retry => Seen::Retry,
                    });
                }),
            })
        });
        (list, recorder)
    }

    fn listed(cx: &mut TestAppContext, items: Vec<CollectionItem>) -> Entity<CollectionList> {
        let (list, _) = mount(cx);
        cx.update(|cx| list.update(cx, |list, cx| list.apply_listing(Ok(items), cx)));
        list
    }

    fn poke_every_control(cx: &mut TestAppContext, list: &Entity<CollectionList>) {
        cx.update(|cx| {
            list.update(cx, |list, cx| {
                list.emit_toggle(0, 0, cx);
                list.emit_for(0, CollectionListEvent::Edit, cx);
                list.emit_for(0, CollectionListEvent::Delete, cx);
            })
        });
        cx.run_until_parked();
    }

    fn seen(cx: &mut TestAppContext, recorder: &Entity<Recorder>) -> Vec<Seen> {
        cx.update(|cx| recorder.update(cx, |recorder, _| std::mem::take(&mut recorder.seen)))
    }

    #[gpui::test]
    fn a_reload_over_a_ready_listing_keeps_the_rows_on_screen(cx: &mut TestAppContext) {
        let list = listed(cx, vec![manageable("a")]);

        cx.update(|cx| list.update(cx, |list, cx| list.begin_loading(cx)));

        assert_eq!(
            cx.update(|cx| list.read(cx).load().clone()),
            ListLoad::Ready
        );
    }

    #[gpui::test]
    fn a_reload_over_a_failed_listing_shows_loading_again(cx: &mut TestAppContext) {
        let (list, _) = mount(cx);
        cx.update(|cx| {
            list.update(cx, |list, cx| {
                list.apply_listing(Err("offline".to_owned()), cx);
                list.begin_loading(cx);
            })
        });

        assert_eq!(
            cx.update(|cx| list.read(cx).load().clone()),
            ListLoad::Loading
        );
    }

    #[gpui::test]
    fn a_manageable_row_emits_edit_delete_and_the_flipped_toggle(cx: &mut TestAppContext) {
        let (list, recorder) = mount(cx);
        cx.update(|cx| {
            list.update(cx, |list, cx| {
                list.apply_listing(Ok(vec![manageable("a")]), cx)
            })
        });

        poke_every_control(cx, &list);

        assert_eq!(
            seen(cx, &recorder),
            vec![
                Seen::Toggle("a".to_owned(), ENABLED.to_owned(), false),
                Seen::Edit("a".to_owned()),
                Seen::Delete("a".to_owned()),
            ]
        );
    }

    #[gpui::test]
    fn read_only_and_busy_rows_emit_nothing(cx: &mut TestAppContext) {
        for busy in [false, true] {
            let (list, recorder) = mount(cx);
            let row = if busy {
                manageable("a")
            } else {
                read_only("a")
            };
            cx.update(|cx| {
                list.update(cx, |list, cx| {
                    list.apply_listing(Ok(vec![row]), cx);
                    if busy {
                        list.set_busy(&CollectionItemId::new("a"), true, cx);
                    }
                })
            });

            poke_every_control(cx, &list);

            assert_eq!(seen(cx, &recorder), Vec::new(), "busy={busy}");
        }
    }

    #[gpui::test]
    fn a_fresh_listing_drops_busy_markers_only_for_rows_that_vanished(cx: &mut TestAppContext) {
        let (list, recorder) = mount(cx);
        cx.update(|cx| {
            list.update(cx, |list, cx| {
                list.apply_listing(Ok(vec![manageable("a"), manageable("b")]), cx);
                list.set_busy(&CollectionItemId::new("a"), true, cx);
                list.set_busy(&CollectionItemId::new("b"), true, cx);
                list.apply_listing(Ok(vec![manageable("a")]), cx);
                list.upsert(manageable("b"), cx);
                list.emit_for(0, CollectionListEvent::Edit, cx);
                list.emit_for(1, CollectionListEvent::Edit, cx);
            })
        });
        cx.run_until_parked();

        assert_eq!(seen(cx, &recorder), vec![Seen::Edit("b".to_owned())]);
    }

    #[gpui::test]
    fn upsert_replaces_a_known_row_in_place_and_appends_a_new_one(cx: &mut TestAppContext) {
        let list = listed(cx, vec![manageable("a"), manageable("b")]);
        let mut renamed = manageable("a");
        renamed.title = "Hydrate".to_owned();

        cx.update(|cx| {
            list.update(cx, |list, cx| {
                list.upsert(renamed, cx);
                list.upsert(manageable("c"), cx);
            })
        });

        let titles: Vec<String> = cx.update(|cx| {
            ["a", "b", "c"]
                .iter()
                .filter_map(|id| list.read(cx).item(&CollectionItemId::new(*id)))
                .map(|item| item.title.clone())
                .collect()
        });
        assert_eq!(
            (titles, cx.update(|cx| list.read(cx).count())),
            (
                vec!["Hydrate".to_owned(), "b".to_owned(), "c".to_owned()],
                3
            )
        );
    }

    #[gpui::test]
    fn removing_a_row_clears_its_busy_marker(cx: &mut TestAppContext) {
        let (list, recorder) = mount(cx);
        cx.update(|cx| {
            list.update(cx, |list, cx| {
                list.apply_listing(Ok(vec![manageable("a")]), cx);
                list.set_busy(&CollectionItemId::new("a"), true, cx);
                list.remove(&CollectionItemId::new("a"), cx);
                list.upsert(manageable("a"), cx);
                list.emit_for(0, CollectionListEvent::Delete, cx);
            })
        });
        cx.run_until_parked();

        assert_eq!(seen(cx, &recorder), vec![Seen::Delete("a".to_owned())]);
    }
}
