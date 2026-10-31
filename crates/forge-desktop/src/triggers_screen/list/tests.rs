use std::sync::{Arc, Mutex};

use gpui::{Modifiers, VisualTestContext, point};

use super::*;
use crate::test_support::{
    StubActions, error_toasts, lifecycle_switch, owned_triggers, press_undo, pump, pump_until,
    runtime, switch_lifecycle, test_backend, trigger_of,
};
use forge_components::{Density, Presentation, ThemeId};
use forge_runtime::{LifecycleState, LifecycleStates};
use forge_storage::{MockTriggerInstanceRepo, SettingsRepo};
use forge_types::TriggerInstance;
use std::sync::atomic::{AtomicBool, Ordering};

const TWITCH_CHAT: &str = "twitch.chat";

const LISTED: [&str; 3] = ["Alfa", "Alma", "Bravo"];

struct Hosting(Entity<TriggersRegistryView>);

impl Render for Hosting {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.0.clone())
    }
}

struct KeyedList {
    list: ListRig,
    vcx: VisualTestContext,
}

impl KeyedList {
    fn mount(cx: &mut gpui::TestAppContext) -> Self {
        let list = open_trigger_list(cx, LISTED.map(row_named).to_vec());
        cx.update(crate::actions::register_key_bindings);
        let hosted = list.host.clone();
        let window = cx.add_window(|_, _| Hosting(hosted));
        let vcx = VisualTestContext::from_window(window.into(), cx);
        let mut keyed = Self { list, vcx };
        keyed.settle();
        keyed
    }

    fn settle(&mut self) {
        self.list.settle(&mut self.vcx);
    }

    fn press(&mut self, keys: &str) {
        self.vcx.simulate_keystrokes(keys);
        self.settle();
    }

    fn search(&mut self, query: &str) {
        let changed = InputEvent::Changed(query.to_owned().into());
        self.list.entity.update(&mut self.vcx, |view, cx| {
            let field = view.search.field().clone();
            view.on_search_event(field, &changed, cx);
        });
        self.vcx.run_until_parked();
    }

    fn row_point(&mut self, position: usize) -> Point<Pixels> {
        let (bounds, row_height) = self.list.entity.read_with(&self.vcx, |view, _| {
            let scroll = view.list_scroll.0.borrow();
            let contents = scroll.last_item_size.unwrap().contents;
            let rows = view.visible.len() as f32;
            (scroll.base_handle.bounds(), contents.height / rows)
        });
        point(
            bounds.left() + px(60.0),
            bounds.top() + row_height * (position as f32 + 0.5),
        )
    }

    fn click_row(&mut self, position: usize) {
        let row = self.row_point(position);
        self.vcx.simulate_click(row, Modifiers::none());
        self.settle();
    }

    fn right_click_row(&mut self, position: usize) {
        let row = self.row_point(position);
        self.vcx
            .simulate_mouse_down(row, MouseButton::Right, Modifiers::none());
        self.vcx
            .simulate_mouse_up(row, MouseButton::Right, Modifiers::none());
        self.vcx.run_until_parked();
    }

    fn name_of(&self, id: Option<TriggerInstanceId>) -> Option<String> {
        self.list.entity.read_with(&self.vcx, |view, _| {
            id.and_then(|id| view.find(id).map(|row| row.name.clone()))
        })
    }

    fn cursor(&self) -> Option<String> {
        let at = self
            .list
            .entity
            .read_with(&self.vcx, |view, _| view.list_cursor);
        self.name_of(at)
    }

    fn selected(&self) -> Option<String> {
        let selected = self
            .list
            .entity
            .read_with(&self.vcx, |view, _| view.selected);
        self.name_of(selected)
    }
}

#[gpui::test]
fn arrows_continue_from_the_row_a_click_selected(cx: &mut gpui::TestAppContext) {
    let mut rig = KeyedList::mount(cx);
    rig.press("down");

    rig.click_row(1);
    rig.press("down");

    assert_eq!(rig.cursor().as_deref(), Some("Bravo"));
}

#[gpui::test]
fn enter_after_a_click_keeps_the_clicked_row_over_the_row_the_arrows_left_behind(
    cx: &mut gpui::TestAppContext,
) {
    let mut rig = KeyedList::mount(cx);
    rig.press("down");

    rig.click_row(2);
    rig.press("enter");

    assert_eq!(rig.selected().as_deref(), Some("Bravo"));
}

#[gpui::test]
fn a_hidden_cursor_row_hands_the_arrows_to_the_visible_selection(cx: &mut gpui::TestAppContext) {
    let mut rig = KeyedList::mount(cx);
    rig.click_row(1);
    rig.press("down");
    assert_eq!(rig.cursor().as_deref(), Some("Bravo"));

    rig.search("al");
    rig.press("up");

    assert_eq!(rig.cursor().as_deref(), Some("Alfa"));
}

#[gpui::test]
fn enter_on_a_cursor_row_the_search_hid_selects_nothing(cx: &mut gpui::TestAppContext) {
    let mut rig = KeyedList::mount(cx);
    rig.press("down down");

    rig.search("bravo");
    rig.press("enter");

    assert_eq!(rig.selected(), None);
}

#[gpui::test]
fn an_open_row_menu_keeps_arrows_and_enter_from_the_list(cx: &mut gpui::TestAppContext) {
    let mut rig = KeyedList::mount(cx);
    rig.press("down");
    rig.right_click_row(2);

    rig.press("down enter");

    assert_eq!(rig.cursor().as_deref(), Some("Alfa"));
    assert_eq!(rig.selected(), None);
}

#[gpui::test]
fn undoing_a_delete_brings_the_trigger_back(cx: &mut gpui::TestAppContext) {
    let seeded = row_named("Greeter");
    let id = seeded.id;
    let list = open_trigger_list(cx, vec![seeded]);

    list.entity.update(cx, |view, cx| {
        view.request_delete(id, cx);
        view.confirm_delete(cx);
    });
    list.wait_for_names(cx, &[]);
    press_undo(cx);

    list.wait_for_names(cx, &["Greeter".to_owned()]);
}

#[gpui::test]
fn toggling_twice_before_the_store_answers_stores_only_the_second_state(
    cx: &mut gpui::TestAppContext,
) {
    let seeded = row_named("Greeter");
    let id = seeded.id;
    let writes = Arc::new(Mutex::new(Vec::new()));
    let list = open_list_recording_writes(cx, vec![seeded], Arc::clone(&writes));

    list.entity.update(cx, |view, cx| {
        view.toggle_enable(id, cx);
        view.toggle_enable(id, cx);
    });
    list.settle(cx);

    let shown = list.entity.read_with(cx, |view, _| {
        view.instances
            .iter()
            .find(|row| row.id == id)
            .map(|row| row.enabled)
    });
    assert_eq!(
        (writes.lock().unwrap().clone(), shown, list.errors(cx)),
        (vec![true], Some(true), 0)
    );
}

#[gpui::test]
fn toggling_twice_with_the_store_answering_between_writes_in_click_order(
    cx: &mut gpui::TestAppContext,
) {
    let seeded = row_named("Greeter");
    let id = seeded.id;
    let writes = Arc::new(Mutex::new(Vec::new()));
    let list = open_list_recording_writes(cx, vec![seeded], Arc::clone(&writes));

    for _ in 0..2 {
        list.entity
            .update(cx, |view, cx| view.toggle_enable(id, cx));
        list.settle(cx);
    }

    assert_eq!(writes.lock().unwrap().clone(), vec![false, true]);
}

#[gpui::test]
fn two_triggers_toggled_together_both_land(cx: &mut gpui::TestAppContext) {
    let first = row_named("First");
    let second = row_named("Second");
    let (first_id, second_id) = (first.id, second.id);
    let writes = Arc::new(Mutex::new(Vec::new()));
    let list = open_list_recording_writes(cx, vec![first, second], Arc::clone(&writes));

    list.entity.update(cx, |view, cx| {
        view.toggle_enable(first_id, cx);
        view.toggle_enable(second_id, cx);
    });
    list.settle(cx);

    assert_eq!(writes.lock().unwrap().clone(), vec![false, false]);
}

fn twitch() -> IntegrationId {
    IntegrationId::new("twitch")
}

fn twitch_in(state: LifecycleState) -> LifecycleStates {
    LifecycleStates::from([(twitch(), state)])
}

#[gpui::test]
fn a_trigger_tracks_its_integration_switching_off_and_back_on(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy)));
    let rt = runtime();
    let (lifecycle, switch) = lifecycle_switch(cx, &rt, twitch_in(LifecycleState::Running));
    let mut repo = MockTriggerInstanceRepo::new();
    repo.expect_list_all().returning(|| Ok(Vec::new()));
    let (settings, _writes) = test_backend();
    let host = cx.update(|cx| {
        cx.new(|cx| {
            TriggersRegistryView::new(
                Arc::new(repo),
                Arc::new(StubActions),
                Arc::new(owned_triggers(&[(TWITCH_CHAT, "twitch")])),
                settings as Arc<dyn SettingsRepo>,
                rt.handle().clone(),
                None,
                cx,
            )
            .with_integration_switch(switch, cx)
        })
    });
    let list = host.read_with(cx, |host, _| host.list.clone());
    let owner = |cx: &mut gpui::TestAppContext| {
        list.read_with(cx, |list, _| list.switched_off_owner(TWITCH_CHAT))
    };

    let before = owner(cx);
    switch_lifecycle(cx, &lifecycle, twitch_in(LifecycleState::Disabled));
    let off = owner(cx);
    switch_lifecycle(cx, &lifecycle, twitch_in(LifecycleState::Starting));
    let back_on = owner(cx);

    assert_eq!((before, off, back_on), (None, Some(twitch()), None));
}

struct ListRig {
    host: Entity<TriggersRegistryView>,
    entity: Entity<TriggerList>,
    rows: Arc<std::sync::Mutex<Vec<TriggerInstance>>>,
    rt: tokio::runtime::Runtime,
}

fn open_trigger_list(cx: &mut gpui::TestAppContext, seeded: Vec<TriggerInstance>) -> ListRig {
    open_list_over(cx, seeded, None)
}

fn open_list_recording_writes(
    cx: &mut gpui::TestAppContext,
    seeded: Vec<TriggerInstance>,
    writes: Arc<std::sync::Mutex<Vec<bool>>>,
) -> ListRig {
    open_list_over(cx, seeded, Some(writes))
}

fn open_list_over(
    cx: &mut gpui::TestAppContext,
    seeded: Vec<TriggerInstance>,
    writes: Option<Arc<std::sync::Mutex<Vec<bool>>>>,
) -> ListRig {
    crate::i18n::install_language(forge_storage::Language::En);
    cx.update(|cx| {
        cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
        cx.set_global(forge_components::Toasts::new());
    });
    let rt = runtime();
    let rows = Arc::new(std::sync::Mutex::new(seeded));
    let mut repo = MockTriggerInstanceRepo::new();
    let listed = Arc::clone(&rows);
    repo.expect_list_user_defined()
        .returning(move || Ok(listed.lock().unwrap().clone()));
    repo.expect_list_all().returning(|| Ok(Vec::new()));
    repo.expect_actions_by_instance()
        .returning(|| Ok(Default::default()));
    let found = Arc::clone(&rows);
    repo.expect_get().returning(move |id| {
        let found = found.lock().unwrap();
        Ok(found.iter().find(|row| row.id == id).cloned())
    });
    repo.expect_actions_using().returning(|_| Ok(Vec::new()));
    repo.expect_set_enabled()
        .returning(move |_, enabled| match &writes {
            Some(writes) => {
                writes.lock().unwrap().push(enabled);
                Ok(())
            }
            None => Err(forge_storage::StorageError::Connection {
                reason: "database is locked".to_owned(),
            }),
        });
    let archived = Arc::new(std::sync::Mutex::new(Vec::new()));
    let (live, shelf) = (Arc::clone(&rows), Arc::clone(&archived));
    repo.expect_archive().returning(move |id| {
        let mut live = live.lock().unwrap();
        let position = live.iter().position(|row| row.id == id).unwrap();
        shelf.lock().unwrap().push(live.remove(position));
        Ok(true)
    });
    let (live, shelf) = (Arc::clone(&rows), archived);
    repo.expect_restore().returning(move |_| {
        live.lock().unwrap().append(&mut shelf.lock().unwrap());
        Ok(true)
    });
    let (settings, _writes) = test_backend();
    let handle = rt.handle().clone();
    let host = cx.update(|cx| {
        cx.new(|cx| {
            TriggersRegistryView::new(
                Arc::new(repo),
                Arc::new(StubActions),
                Arc::new(forge_registry::TriggerRegistry::new()),
                settings as Arc<dyn SettingsRepo>,
                handle,
                None,
                cx,
            )
        })
    });
    let entity = host.read_with(cx, |host, _| host.list.clone());
    pump_until(&rt, || {
        cx.run_until_parked();
        entity.read_with(cx, |list, _| !matches!(list.list_load, Loadable::Loading))
    });
    ListRig {
        host,
        entity,
        rows,
        rt,
    }
}

impl ListRig {
    fn settle(&self, cx: &mut gpui::TestAppContext) {
        cx.run_until_parked();
        pump(&self.rt);
        cx.run_until_parked();
    }

    fn names(&self, cx: &mut gpui::TestAppContext) -> Vec<String> {
        self.entity.read_with(cx, |view, _| {
            view.instances.iter().map(|row| row.name.clone()).collect()
        })
    }

    fn wait_for_names(&self, cx: &mut gpui::TestAppContext, expected: &[String]) {
        pump_until(&self.rt, || {
            cx.run_until_parked();
            self.names(cx) == expected
        });
    }

    fn errors(&self, cx: &mut gpui::TestAppContext) -> usize {
        cx.update(|cx| error_toasts(cx))
    }
}

fn row_named(name: &str) -> TriggerInstance {
    let mut instance = trigger_of(TWITCH_CHAT);
    instance.name = name.to_owned();
    instance
}

#[gpui::test]
fn opening_the_trigger_list_lands_its_rows(cx: &mut gpui::TestAppContext) {
    let list = open_trigger_list(cx, vec![row_named("Greeter")]);

    assert_eq!(list.names(cx), ["Greeter"]);
}

impl ListRig {
    fn fail_next_read(&self, cx: &mut gpui::TestAppContext) {
        self.entity.update(cx, |view, cx| {
            view.spawn_reload(async { Err(RepoFailure::localized("locked")) }, cx);
        });
        pump_until(&self.rt, || {
            cx.run_until_parked();
            self.entity
                .read_with(cx, |view, _| matches!(view.list_load, Loadable::Failed(_)))
        });
    }
}

#[gpui::test]
fn a_failed_read_of_an_empty_list_reports_inline_and_retry_recovers(cx: &mut gpui::TestAppContext) {
    let list = open_trigger_list(cx, Vec::new());

    list.fail_next_read(cx);
    assert_eq!(list.errors(cx), 0);

    list.entity.update(cx, |view, cx| view.retry_list_load(cx));
    let reading = list
        .entity
        .read_with(cx, |view, _| matches!(view.list_load, Loadable::Loading));
    pump_until(&list.rt, || {
        cx.run_until_parked();
        list.entity
            .read_with(cx, |view, _| view.list_load.is_loaded())
    });
    assert!(reading);
}

#[gpui::test]
fn a_failed_read_over_shown_rows_keeps_them_and_toasts(cx: &mut gpui::TestAppContext) {
    let list = open_trigger_list(cx, vec![row_named("Greeter")]);

    list.fail_next_read(cx);

    assert_eq!(list.names(cx), ["Greeter"]);
    assert_eq!(list.errors(cx), 1);
}

#[gpui::test]
fn a_failed_write_toasts_even_with_a_newer_reload_in_flight(cx: &mut gpui::TestAppContext) {
    let list = open_trigger_list(cx, Vec::new());
    let (_held, reload_gate) = tokio::sync::oneshot::channel::<()>();

    list.entity.update(cx, |view, cx| {
        view.spawn_reload(
            async move {
                reload_gate.await.map_err(RepoFailure::operation)?;
                Ok(Vec::new())
            },
            cx,
        );
        view.spawn_write(
            async { Err::<(), _>(RepoFailure::operation("disk gone")) },
            cx,
        );
    });

    pump_until(&list.rt, || {
        cx.run_until_parked();
        list.errors(cx) == 1
    });
}

#[gpui::test]
fn a_successful_write_relists_and_shows_what_it_wrote(cx: &mut gpui::TestAppContext) {
    let list = open_trigger_list(cx, Vec::new());
    let rows = Arc::clone(&list.rows);

    list.entity.update(cx, |view, cx| {
        view.spawn_write(
            async move {
                rows.lock().unwrap().push(row_named("Written"));
                Ok(())
            },
            cx,
        );
    });

    list.wait_for_names(cx, &["Written".to_owned()]);
}

#[gpui::test]
fn a_stale_reload_that_failed_is_dropped_without_a_toast(cx: &mut gpui::TestAppContext) {
    let list = open_trigger_list(cx, Vec::new());
    let (release_stale, stale_gate) = tokio::sync::oneshot::channel::<()>();
    let stale_landed = Arc::new(AtomicBool::new(false));
    let landed = Arc::clone(&stale_landed);

    list.entity.update(cx, |view, cx| {
        view.spawn_reload(
            async move {
                stale_gate.await.map_err(RepoFailure::operation)?;
                landed.store(true, Ordering::SeqCst);
                Err(RepoFailure::operation("disk gone"))
            },
            cx,
        );
        view.reload(cx);
    });
    list.rows.lock().unwrap().push(row_named("Newest"));
    list.wait_for_names(cx, &["Newest".to_owned()]);
    release_stale.send(()).unwrap();
    pump_until(&list.rt, || stale_landed.load(Ordering::SeqCst));
    cx.run_until_parked();

    assert_eq!(list.errors(cx), 0);
}

#[gpui::test]
fn a_failed_toggle_toasts_and_the_reload_restores_the_stored_state(cx: &mut gpui::TestAppContext) {
    let list = open_trigger_list(cx, vec![row_named("Greeter")]);
    let id = list.rows.lock().unwrap()[0].id;
    let stored = list.rows.lock().unwrap()[0].enabled;
    let shown = |list: &ListRig, cx: &mut gpui::TestAppContext| {
        list.entity
            .read_with(cx, |view, _| view.find(id).map(|row| row.enabled))
    };

    list.entity
        .update(cx, |view, cx| view.toggle_enable(id, cx));
    let optimistic = shown(&list, cx);
    pump_until(&list.rt, || {
        cx.run_until_parked();
        list.errors(cx) == 1 && shown(&list, cx) == Some(stored)
    });

    assert_eq!(optimistic, Some(!stored));
}
