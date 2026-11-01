use super::*;
use forge_runtime::ActionsService;
use forge_storage::{GlobalsRepo, QueueRepo, ScriptRepo};
use forge_types::ExecutionMode;
use gpui::MouseButton;
use std::collections::HashMap;

use forge_components::ThemeId;
use forge_runtime::QueueScheduler;
use forge_runtime::{TIMER_TICK_KIND, TimerTickDescriptor};
use forge_soundboard::ClipLibrary;
use forge_storage::MockActionRepo;
use forge_storage::MockQueueRepo;
use forge_storage::MockSoundboardClipsRepo;
use forge_storage::MockTriggerInstanceRepo;
use forge_storage::{DataProvider, SoundboardClipsRepo};
use forge_types::{PermissionRung, PlatformScope, TriggerInstance, TriggerInstanceId};

use crate::test_support::{
    Declares, StubEventLog, StubHistory, StubOverlays, StubTrigger, runtime, test_backend,
    trigger_registry,
};
use forge_components::Presentation;

const CHAT_KIND: &str = "twitch.chat";
const REWARD_KIND: &str = "twitch.reward_redemption";

struct Seed {
    name: &'static str,
    group: &'static str,
    trigger_kinds: &'static [&'static str],
}

fn action(seed: &Seed) -> Action {
    Action {
        id: ActionId::new(),
        name: seed.name.to_owned(),
        group: Some(seed.group.to_owned()),
        queue_id: QueueId::new(),
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: Default::default(),
        description: None,
        sub_actions: Vec::new(),
    }
}

fn instance(kind_id: &str) -> TriggerInstance {
    TriggerInstance {
        id: TriggerInstanceId::new(),
        kind_id: kind_id.to_owned(),
        name: kind_id.to_owned(),
        overrides: Default::default(),
        enabled: true,
        user_defined: true,
        platform_scope: PlatformScope::Any,
        cooldown_secs: 0,
        cooldown_global: true,
        permission_rung: PermissionRung::Everyone,
    }
}

fn registry() -> TriggerRegistry {
    let mut registry = trigger_registry(vec![
        StubTrigger::new(CHAT_KIND, "Chat", TriggerCategory::Chat, Declares::Nothing),
        StubTrigger::new(
            REWARD_KIND,
            "Reward",
            TriggerCategory::ChannelPoints,
            Declares::Nothing,
        ),
    ]);
    registry.register(Box::new(TimerTickDescriptor)).unwrap();
    registry
}

struct Fixture {
    host: Entity<ScreenActionsView>,
    view: Entity<ActionTree>,
    actions: Vec<Action>,
    rt: tokio::runtime::Runtime,
}

impl Fixture {
    fn new(cx: &mut gpui::TestAppContext, seeds: &[Seed]) -> Self {
        cx.update(|cx| {
            cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
        });
        let rt = runtime();
        let actions: Vec<Action> = seeds.iter().map(action).collect();
        let mut instances = Vec::new();
        let mut users: HashMap<TriggerInstanceId, Vec<ActionId>> = HashMap::new();
        let mut linked: HashMap<ActionId, Vec<TriggerInstance>> = HashMap::new();
        for (seed, action) in seeds.iter().zip(&actions) {
            for kind in seed.trigger_kinds {
                let instance = instance(kind);
                users.insert(instance.id, vec![action.id]);
                linked.entry(action.id).or_default().push(instance.clone());
                instances.push(instance);
            }
        }

        let mut action_repo = MockActionRepo::new();
        let listed = actions.clone();
        action_repo
            .expect_list()
            .returning(move || Ok(listed.clone()));
        let found = actions.clone();
        action_repo
            .expect_get()
            .returning(move |id| Ok(found.iter().find(|a| a.id == id).cloned()));
        action_repo
            .expect_telemetry()
            .returning(|_| Ok(forge_storage::ActionTelemetry::default()));
        let mut trigger_repo = MockTriggerInstanceRepo::new();
        let by_action = linked.clone();
        trigger_repo
            .expect_instances_by_action()
            .returning(move || Ok(by_action.clone().into_iter().collect()));
        trigger_repo
            .expect_list_for_action()
            .returning(move |id| Ok(linked.get(&id).cloned().unwrap_or_default()));
        trigger_repo
            .expect_list_all()
            .returning(move || Ok(instances.clone()));
        trigger_repo
            .expect_actions_by_instance()
            .returning(move || Ok(users.clone()));

        let action_repo: Arc<dyn ActionRepo> = Arc::new(action_repo);
        let mut queues = MockQueueRepo::new();
        queues.expect_list().returning(|| Ok(Vec::new()));
        let queue_repo: Arc<dyn QueueRepo> = Arc::new(queues);
        let trigger_repo: Arc<dyn TriggerInstanceRepo> = Arc::new(trigger_repo);
        let mut stored_clips = MockSoundboardClipsRepo::new();
        stored_clips.expect_list().returning(|| Ok(Vec::new()));
        let clips: Arc<dyn SoundboardClipsRepo> = Arc::new(stored_clips);
        let service = Arc::new(ActionsService::new(
            Arc::clone(&action_repo),
            Arc::clone(&queue_repo),
            Arc::new(StubHistory),
            Arc::clone(&trigger_repo),
        ));
        let (backend, _writes) = test_backend();
        let mut globals = forge_storage::MockGlobalsRepo::new();
        globals.expect_list().returning(|| Ok(Vec::new()));
        let (bus, scheduler) = rt.block_on(async {
            let bus = EventBus::new(Arc::new(StubEventLog));
            let engine = crate::test_support::stub_action_engine(Arc::clone(&bus));
            let scheduler = QueueScheduler::spawn(engine, Arc::clone(&bus), Vec::new());
            (bus, scheduler)
        });
        let handle = rt.handle().clone();
        let host = cx.update(|cx| {
            cx.new(|cx| {
                ScreenActionsView::new(
                    super::ActionsDeps {
                        action_repo,
                        queue_repo,
                        actions_service: service,
                        trigger_instance_repo: trigger_repo,
                        script_repo: Arc::new(crate::test_support::MemoryScripts::default())
                            as Arc<dyn ScriptRepo>,
                        clip_library: Arc::new(ClipLibrary::new(
                            clips,
                            backend.media_repo(),
                            Arc::new(forge_events::test_support::NullPublisher),
                        )),
                        globals_repo: Arc::new(globals) as Arc<dyn GlobalsRepo>,
                        overlay_repo: Arc::new(StubOverlays),
                        speak: None,
                        sub_action_registry: Arc::new(SubActionRegistry::new()),
                        trigger_registry: Arc::new(registry()),
                        rt_handle: handle,
                        bus,
                        scheduler,
                    },
                    backend.settings_repo(),
                    Arc::new(OverlayKindRegistry::new()),
                    None,
                    cx,
                )
            })
        });
        let view = host.read_with(cx, |host, _| host.tree.clone());
        let fixture = Self {
            host,
            view,
            actions,
            rt,
        };
        fixture.settle(cx);
        fixture
    }

    fn settle(&self, cx: &mut gpui::TestAppContext) {
        crate::test_support::settle_runtime_work(cx, &self.rt);
    }

    fn set_filter(&self, cx: &mut gpui::TestAppContext, filter: ActionsFilter) {
        self.view.update(cx, |view, cx| view.set_filter(filter, cx));
    }

    fn visible(&self, cx: &mut gpui::TestAppContext, name: &str) -> bool {
        self.view.read_with(cx, |view, _| {
            let action = view
                .groups
                .iter()
                .flat_map(|g| g.actions.iter())
                .find(|a| a.name == name)
                .unwrap();
            view.action_passes(action)
        })
    }

    fn sync_detail(&self, cx: &mut gpui::TestAppContext, name: &str, kinds: &[&str]) {
        let action = self
            .actions
            .iter()
            .find(|a| a.name == name)
            .unwrap()
            .clone();
        let kinds: Vec<String> = kinds.iter().map(|kind| instance(kind).kind_id).collect();
        self.view
            .update(cx, |view, _| view.sync_filter_membership(action.id, &kinds));
    }
}

struct Hosting(Entity<ScreenActionsView>);

impl Render for Hosting {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.0.clone())
    }
}

impl Fixture {
    fn in_window(&self, cx: &mut gpui::TestAppContext) -> gpui::VisualTestContext {
        cx.update(crate::app::actions::register_key_bindings);
        let hosted = self.host.clone();
        let window = cx.add_window(|_, _| Hosting(hosted));
        let vcx = gpui::VisualTestContext::from_window(window.into(), cx);
        vcx.run_until_parked();
        vcx
    }

    fn collapse(&self, cx: &mut gpui::TestAppContext, group: &str) {
        self.view.update(cx, |view, cx| {
            let name = group.to_uppercase();
            let index = view.groups.iter().position(|g| g.name == name).unwrap();
            view.toggle_group(index, cx);
        });
    }

    fn search(&self, cx: &mut gpui::TestAppContext, query: &str) {
        self.view.update(cx, |view, cx| {
            view.search
                .on_changed(&InputEvent::Changed(query.to_owned().into()));
            cx.notify();
        });
    }

    fn name_of(&self, id: ActionId) -> &str {
        let found = self.actions.iter().find(|a| a.id == id);
        &found.unwrap().name
    }

    fn press(&self, vcx: &mut gpui::VisualTestContext, keys: &str) {
        vcx.simulate_keystrokes(keys);
        vcx.run_until_parked();
    }

    fn cursor_name(&self, vcx: &mut gpui::VisualTestContext) -> Option<String> {
        let ids: Vec<ActionId> = self.actions.iter().map(|a| a.id).collect();
        self.view.read_with(vcx, |view, _| {
            let at = ids.iter().find(|id| view.keys.is_at(id));
            at.map(|id| self.name_of(*id).to_owned())
        })
    }

    fn selected_name(&self, vcx: &mut gpui::VisualTestContext) -> Option<String> {
        let selected = self.view.read_with(vcx, |view, _| view.selected);
        selected.map(|id| self.name_of(id).to_owned())
    }

    fn keyboard_walk(&self, vcx: &mut gpui::VisualTestContext, rows: usize) -> Vec<String> {
        (0..rows)
            .map(|_| {
                self.press(vcx, "down");
                self.cursor_name(vcx).unwrap()
            })
            .collect()
    }

    fn row_bounds(
        &self,
        vcx: &mut gpui::VisualTestContext,
        position: usize,
    ) -> gpui::Bounds<Pixels> {
        self.view.read_with(vcx, |view, _| {
            let child = view.tree_action_rows()[position].child_index;
            view.scroll.bounds_for_item(child).unwrap()
        })
    }

    fn click_step_counter(&self, vcx: &mut gpui::VisualTestContext, position: usize) {
        let bounds = self.row_bounds(vcx, position);
        let counter = gpui::point(
            bounds.right() - ROW_GUTTER - RIGHT_SLOT_W / 2.0,
            bounds.center().y,
        );
        vcx.simulate_click(counter, gpui::Modifiers::none());
        self.settle(vcx);
    }

    fn right_click_row(&self, vcx: &mut gpui::VisualTestContext, position: usize) {
        let center = self.row_bounds(vcx, position).center();
        vcx.simulate_mouse_down(center, MouseButton::Right, gpui::Modifiers::none());
        vcx.simulate_mouse_up(center, MouseButton::Right, gpui::Modifiers::none());
        vcx.run_until_parked();
    }

    fn click_row(&self, vcx: &mut gpui::VisualTestContext, position: usize) {
        let bounds = self.row_bounds(vcx, position);
        let name_area = gpui::point(bounds.left() + ROW_INDENT, bounds.center().y);
        vcx.simulate_click(name_area, gpui::Modifiers::none());
        self.settle(vcx);
    }

    fn clicked_walk(&self, vcx: &mut gpui::VisualTestContext) -> Vec<String> {
        let rows = self
            .view
            .read_with(vcx, |view, _| view.tree_action_rows().len());
        (0..rows)
            .map(|position| {
                self.click_row(vcx, position);
                self.selected_name(vcx).unwrap()
            })
            .collect()
    }
}

fn label(filter: ActionsFilter) -> &'static str {
    match filter {
        ActionsFilter::All => "all",
        ActionsFilter::Chat => "chat",
        ActionsFilter::Timers => "timers",
        ActionsFilter::Points => "points",
    }
}

fn seeds() -> Vec<Seed> {
    vec![
        Seed {
            name: "Hydrate",
            group: "Chat Commands",
            trigger_kinds: &[TIMER_TICK_KIND],
        },
        Seed {
            name: "Lurk",
            group: "Chat Commands",
            trigger_kinds: &[CHAT_KIND],
        },
        Seed {
            name: "Raid train",
            group: "Timers",
            trigger_kinds: &[],
        },
        Seed {
            name: "Stretch",
            group: "Misc",
            trigger_kinds: &[TIMER_TICK_KIND],
        },
        Seed {
            name: "Hug",
            group: "Misc",
            trigger_kinds: &[REWARD_KIND],
        },
        Seed {
            name: "Shoutout",
            group: "Misc",
            trigger_kinds: &[CHAT_KIND],
        },
        Seed {
            name: "Free hug",
            group: "Channel Points",
            trigger_kinds: &[],
        },
        Seed {
            name: "Spin the wheel",
            group: "Misc",
            trigger_kinds: &[TIMER_TICK_KIND, REWARD_KIND],
        },
    ]
}

#[gpui::test]
fn trigger_filters_show_an_action_with_a_matching_trigger_outside_the_filter_group(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = Fixture::new(cx, &seeds());
    for (filter, name) in [
        (ActionsFilter::Chat, "Shoutout"),
        (ActionsFilter::Timers, "Hydrate"),
        (ActionsFilter::Points, "Hug"),
    ] {
        fixture.set_filter(cx, filter);
        assert!(fixture.visible(cx, name), "{name} under {}", label(filter));
    }
}

#[gpui::test]
fn trigger_filters_hide_an_action_without_a_trigger_of_their_category(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = Fixture::new(cx, &seeds());
    for (filter, name) in [
        (ActionsFilter::Chat, "Hug"),
        (ActionsFilter::Chat, "Stretch"),
        (ActionsFilter::Timers, "Lurk"),
        (ActionsFilter::Timers, "Hug"),
        (ActionsFilter::Points, "Lurk"),
        (ActionsFilter::Points, "Hydrate"),
    ] {
        fixture.set_filter(cx, filter);
        assert!(!fixture.visible(cx, name), "{name} under {}", label(filter));
    }
}

#[gpui::test]
fn an_action_with_timer_and_reward_triggers_shows_under_both_filters(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = Fixture::new(cx, &seeds());
    for filter in [ActionsFilter::Timers, ActionsFilter::Points] {
        fixture.set_filter(cx, filter);
        assert!(fixture.visible(cx, "Spin the wheel"), "{}", label(filter));
    }
}

#[gpui::test]
fn a_group_named_after_a_filter_puts_no_action_under_it_without_a_trigger(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = Fixture::new(cx, &seeds());
    for (filter, name) in [
        (ActionsFilter::Timers, "Raid train"),
        (ActionsFilter::Chat, "Hydrate"),
        (ActionsFilter::Points, "Free hug"),
    ] {
        fixture.set_filter(cx, filter);
        assert!(!fixture.visible(cx, name), "{name} under {}", label(filter));
    }
}

#[gpui::test]
fn removing_the_last_matching_trigger_drops_the_action_from_its_filter(
    cx: &mut gpui::TestAppContext,
) {
    for (filter, name) in [
        (ActionsFilter::Timers, "Hydrate"),
        (ActionsFilter::Points, "Hug"),
    ] {
        let fixture = Fixture::new(cx, &seeds());
        fixture.set_filter(cx, filter);
        fixture.sync_detail(cx, name, &[CHAT_KIND]);
        assert!(!fixture.visible(cx, name), "{name} under {}", label(filter));
    }
}

#[gpui::test]
fn removing_one_trigger_kind_keeps_the_action_under_the_other_filter(
    cx: &mut gpui::TestAppContext,
) {
    for (remaining, kept, dropped) in [
        (
            TIMER_TICK_KIND,
            ActionsFilter::Timers,
            ActionsFilter::Points,
        ),
        (REWARD_KIND, ActionsFilter::Points, ActionsFilter::Timers),
    ] {
        let fixture = Fixture::new(cx, &seeds());
        fixture.sync_detail(cx, "Spin the wheel", &[remaining]);
        fixture.set_filter(cx, kept);
        assert!(
            fixture.visible(cx, "Spin the wheel"),
            "kept {}",
            label(kept)
        );
        fixture.set_filter(cx, dropped);
        assert!(
            !fixture.visible(cx, "Spin the wheel"),
            "dropped {}",
            label(dropped)
        );
    }
}

#[gpui::test]
fn adding_timer_and_reward_triggers_puts_the_action_into_both_filters(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = Fixture::new(cx, &seeds());
    fixture.sync_detail(cx, "Lurk", &[CHAT_KIND, TIMER_TICK_KIND, REWARD_KIND]);
    for filter in [ActionsFilter::Timers, ActionsFilter::Points] {
        fixture.set_filter(cx, filter);
        assert!(fixture.visible(cx, "Lurk"), "{}", label(filter));
    }
}

#[test]
fn actions_by_trigger_filter_buckets_actions_per_filter_and_skips_unmapped_kinds() {
    let timer_a = instance(TIMER_TICK_KIND);
    let timer_b = instance(TIMER_TICK_KIND);
    let reward = instance(REWARD_KIND);
    let chat = instance(CHAT_KIND);
    let (first, second, redeemer, chatter) = (
        ActionId::new(),
        ActionId::new(),
        ActionId::new(),
        ActionId::new(),
    );
    let users: HashMap<TriggerInstanceId, Vec<ActionId>> = HashMap::from([
        (timer_a.id, vec![first]),
        (timer_b.id, vec![second]),
        (reward.id, vec![first, redeemer]),
        (chat.id, vec![chatter]),
    ]);
    let instances = vec![timer_a, timer_b, reward, chat];
    let mut repo = MockTriggerInstanceRepo::new();
    repo.expect_list_all()
        .returning(move || Ok(instances.clone()));
    repo.expect_actions_by_instance()
        .returning(move || Ok(users.clone()));
    let filter_by_kind = HashMap::from([
        (TIMER_TICK_KIND.to_owned(), ActionsFilter::Timers),
        (REWARD_KIND.to_owned(), ActionsFilter::Points),
    ]);

    let members = runtime()
        .block_on(actions_by_trigger_filter(&repo, &filter_by_kind))
        .unwrap();

    assert_eq!(members.len(), 2);
    assert_eq!(
        members[&ActionsFilter::Timers],
        HashSet::from([first, second])
    );
    assert_eq!(
        members[&ActionsFilter::Points],
        HashSet::from([first, redeemer])
    );
}

#[gpui::test]
fn the_delete_note_reflects_whether_pending_scheduled_runs_are_known(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = Fixture::new(cx, &seeds());
    let id = fixture.actions[0].id;
    let other = ActionId::new();
    let repo: Arc<dyn forge_storage::ScheduledRunRepo> =
        Arc::new(forge_storage::MockScheduledRunRepo::new());

    for (language, unknown) in [
        (
            forge_storage::Language::En,
            "Any pending scheduled runs of this action will be cancelled too.",
        ),
        (
            forge_storage::Language::Uk,
            "Також буде скасовано всі її заплановані запуски, що очікують.",
        ),
    ] {
        crate::app::i18n::install_language(language);
        let note =
            |with_repo: bool, count: Option<(ActionId, u64)>, cx: &mut gpui::TestAppContext| {
                fixture.view.update(cx, |view, _| {
                    view.scheduled_runs = with_repo.then(|| Arc::clone(&repo));
                    view.delete_scheduled_count = count;
                    view.delete_scheduled_note(id)
                })
            };
        let expected_unknown = Some(unknown.to_owned());
        assert_eq!(
            note(true, None, cx),
            expected_unknown,
            "{language:?} no count yet"
        );
        assert_eq!(
            note(true, Some((other, 3)), cx),
            expected_unknown,
            "{language:?} count belongs to another action"
        );
        assert_eq!(
            note(true, Some((id, 0)), cx),
            None,
            "{language:?} known zero"
        );
        assert_eq!(note(false, None, cx), None, "{language:?} no repo");
        assert!(
            note(true, Some((id, 3)), cx).is_some_and(|text| text != unknown),
            "{language:?} known count"
        );
    }
}

#[gpui::test]
fn the_editor_pickers_star_into_the_app_wide_favorites_copies(cx: &mut gpui::TestAppContext) {
    use crate::shared::picker_favorites::{
        SUB_ACTION_FAVORITES, StoredFavorites, TRIGGER_FAVORITES,
    };
    let fixture = Fixture::new(cx, &[]);
    let (elsewhere, _writes) = test_backend();
    let handle = fixture.rt.handle().clone();

    let app_wide = cx.update(|cx| {
        let repo = elsewhere.settings_repo();
        (
            StoredFavorites::shared(SUB_ACTION_FAVORITES, Arc::clone(&repo), &handle, cx)
                .entity_id(),
            StoredFavorites::shared(TRIGGER_FAVORITES, repo, &handle, cx).entity_id(),
        )
    });

    let editor = fixture.host.read_with(cx, |host, _| host.editor.clone());
    let own = editor.read_with(cx, |view, _| {
        (
            view.sub_action_favorites.entity_id(),
            view.trigger_favorites.entity_id(),
        )
    });
    assert_eq!(own, app_wide);
}

#[gpui::test]
fn arrows_walk_the_tree_in_the_order_it_renders_past_collapsed_groups_and_filters(
    cx: &mut gpui::TestAppContext,
) {
    let all = [
        "Free hug",
        "Hydrate",
        "Lurk",
        "Hug",
        "Shoutout",
        "Spin the wheel",
        "Stretch",
        "Raid train",
    ];
    for (filter, query, collapsed, expected) in [
        (ActionsFilter::All, "", None, all.to_vec()),
        (
            ActionsFilter::All,
            "",
            Some("Misc"),
            vec!["Free hug", "Hydrate", "Lurk", "Raid train"],
        ),
        (
            ActionsFilter::Timers,
            "",
            None,
            vec!["Hydrate", "Spin the wheel", "Stretch"],
        ),
        (
            ActionsFilter::Timers,
            "",
            Some("Chat Commands"),
            vec!["Spin the wheel", "Stretch"],
        ),
        (ActionsFilter::All, "hu", None, vec!["Free hug", "Hug"]),
    ] {
        let fixture = Fixture::new(cx, &seeds());
        let vcx = &mut fixture.in_window(cx);
        fixture.set_filter(vcx, filter);
        fixture.search(vcx, query);
        if let Some(group) = collapsed {
            fixture.collapse(vcx, group);
        }
        vcx.run_until_parked();

        assert_eq!(
            fixture.keyboard_walk(vcx, expected.len()),
            expected,
            "keys under {} {query:?} {collapsed:?}",
            label(filter)
        );
        assert_eq!(
            fixture.clicked_walk(vcx),
            expected,
            "rendered rows under {} {query:?} {collapsed:?}",
            label(filter)
        );
    }
}

#[gpui::test]
fn enter_on_the_cursor_row_opens_the_action_a_click_on_that_row_opens(
    cx: &mut gpui::TestAppContext,
) {
    let opened = |fixture: &Fixture, vcx: &mut gpui::VisualTestContext| {
        fixture.settle(vcx);
        let editor = fixture.host.read_with(vcx, |host, _| host.editor.clone());
        let detail = editor.read_with(vcx, |view, _| view.open_action_id());
        (
            fixture.selected_name(vcx),
            detail.map(|id| fixture.name_of(id).to_owned()),
        )
    };

    let by_key = Fixture::new(cx, &seeds());
    let vcx = &mut by_key.in_window(cx);
    by_key.press(vcx, "down down down enter");
    let entered = opened(&by_key, vcx);

    let by_mouse = Fixture::new(cx, &seeds());
    let vcx = &mut by_mouse.in_window(cx);
    by_mouse.click_row(vcx, 2);
    let clicked = opened(&by_mouse, vcx);

    let lurk = Some("Lurk".to_owned());
    assert_eq!(entered, (lurk.clone(), lurk));
    assert_eq!(entered, clicked);
}

#[gpui::test]
fn a_row_that_stops_being_listed_takes_the_cursor_with_it(cx: &mut gpui::TestAppContext) {
    type Hide = fn(&Fixture, &mut gpui::VisualTestContext);
    let hides: [(&str, Hide); 3] = [
        ("collapse", |fixture, vcx| {
            fixture.collapse(vcx, "Chat Commands")
        }),
        ("filter", |fixture, vcx| {
            fixture.set_filter(vcx, ActionsFilter::Points)
        }),
        ("search", |fixture, vcx| fixture.search(vcx, "raid")),
    ];
    for (how, hide) in hides {
        let fixture = Fixture::new(cx, &seeds());
        let vcx = &mut fixture.in_window(cx);
        fixture.press(vcx, "down down");
        assert_eq!(fixture.cursor_name(vcx).as_deref(), Some("Hydrate"));

        hide(&fixture, vcx);
        vcx.run_until_parked();
        fixture.press(vcx, "enter");
        assert_eq!(fixture.selected_name(vcx), None, "{how}");

        let first = fixture
            .view
            .read_with(vcx, |view, _| view.tree_action_rows()[0].id);
        fixture.press(vcx, "down");
        assert_eq!(
            fixture.cursor_name(vcx).as_deref(),
            Some(fixture.name_of(first)),
            "{how}"
        );
    }
}

#[gpui::test]
fn an_inline_rename_silences_the_list_keys(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new(cx, &seeds());
    let vcx = &mut fixture.in_window(cx);
    fixture.press(vcx, "down");
    let renamed = fixture.actions[0].id;
    fixture.view.update_in(vcx, |view, window, cx| {
        view.start_rename(renamed, window, cx);
        view.step_list(ListStep::Next, cx);
        view.activate_list(window, cx);
    });
    vcx.run_until_parked();

    assert_eq!(fixture.cursor_name(vcx).as_deref(), Some("Free hug"));
    assert_eq!(fixture.selected_name(vcx), None);
}

#[gpui::test]
fn arrows_continue_from_the_row_a_click_selected(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new(cx, &seeds());
    let vcx = &mut fixture.in_window(cx);
    fixture.press(vcx, "down");

    fixture.click_row(vcx, 3);
    fixture.press(vcx, "down");

    assert_eq!(fixture.cursor_name(vcx).as_deref(), Some("Shoutout"));
}

#[gpui::test]
fn enter_after_a_click_keeps_the_clicked_row_over_the_row_the_arrows_left_behind(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = Fixture::new(cx, &seeds());
    let vcx = &mut fixture.in_window(cx);
    fixture.press(vcx, "down");

    fixture.click_row(vcx, 3);
    fixture.press(vcx, "enter");
    fixture.settle(vcx);

    assert_eq!(fixture.selected_name(vcx).as_deref(), Some("Hug"));
}

#[gpui::test]
fn an_open_row_menu_keeps_arrows_and_enter_from_the_list_until_escape_closes_it(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = Fixture::new(cx, &seeds());
    let vcx = &mut fixture.in_window(cx);
    fixture.press(vcx, "down");
    fixture.right_click_row(vcx, 2);
    let menu_row = fixture.view.read_with(vcx, |view, _| view.row_open);
    assert_eq!(menu_row.map(|id| fixture.name_of(id)), Some("Lurk"));

    fixture.press(vcx, "enter down");
    assert_eq!(fixture.cursor_name(vcx).as_deref(), Some("Free hug"));
    assert_eq!(fixture.selected_name(vcx), None);

    fixture.press(vcx, "escape");
    fixture.press(vcx, "down");
    assert_eq!(fixture.cursor_name(vcx).as_deref(), Some("Hydrate"));
}

#[gpui::test]
fn a_hidden_cursor_row_hands_the_arrows_to_the_visible_selection(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new(cx, &seeds());
    let vcx = &mut fixture.in_window(cx);
    fixture.click_row(vcx, 3);
    fixture.press(vcx, "down");
    assert_eq!(fixture.cursor_name(vcx).as_deref(), Some("Shoutout"));

    fixture.search(vcx, "hu");
    vcx.run_until_parked();
    fixture.press(vcx, "up");

    assert_eq!(fixture.cursor_name(vcx).as_deref(), Some("Free hug"));
}

#[gpui::test]
fn a_click_on_the_step_counter_selects_the_row(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new(cx, &seeds());
    let vcx = &mut fixture.in_window(cx);
    fixture.press(vcx, "down");

    fixture.click_step_counter(vcx, 3);

    assert_eq!(
        (fixture.selected_name(vcx), fixture.cursor_name(vcx)),
        (Some("Hug".to_owned()), Some("Hug".to_owned()))
    );
}

#[gpui::test]
fn a_click_on_a_row_being_renamed_does_not_select_it(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new(cx, &seeds());
    let vcx = &mut fixture.in_window(cx);
    let renamed = fixture
        .view
        .read_with(vcx, |view, _| view.tree_action_rows()[3].id);
    fixture.view.update_in(vcx, |view, window, cx| {
        view.start_rename(renamed, window, cx)
    });
    vcx.run_until_parked();

    fixture.click_step_counter(vcx, 3);

    assert_eq!(fixture.selected_name(vcx), None);
}

fn grouped_action(name: &str, group: Option<&str>) -> Action {
    Action {
        id: ActionId::new(),
        name: name.to_owned(),
        group: group.map(str::to_owned),
        queue_id: QueueId::new(),
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::default(),
        description: None,
        sub_actions: Vec::new(),
    }
}

fn shape(actions: Vec<Action>) -> Vec<(String, Vec<String>)> {
    group_actions(actions)
        .into_iter()
        .map(|group| {
            (
                group.name.to_string(),
                group.actions.into_iter().map(|a| a.name).collect(),
            )
        })
        .collect()
}

#[test]
fn groups_merge_regardless_of_case_or_surrounding_space_and_sort_their_members() {
    let groups = shape(vec![
        grouped_action("zeta", Some(" chat ")),
        grouped_action("Alpha", Some("CHAT")),
        grouped_action("beta", Some("Chat")),
        grouped_action("solo", Some("alerts")),
    ]);

    assert_eq!(
        groups,
        vec![
            ("ALERTS".to_owned(), vec!["solo".to_owned()]),
            (
                "CHAT".to_owned(),
                vec!["Alpha".to_owned(), "beta".to_owned(), "zeta".to_owned()]
            ),
        ]
    );
}

#[test]
fn a_missing_or_blank_group_lands_in_the_ungrouped_bucket() {
    let groups = shape(vec![
        grouped_action("none", None),
        grouped_action("empty", Some("")),
        grouped_action("space", Some("   ")),
    ]);

    assert_eq!(
        groups,
        vec![(
            tr!("actions_group_ungrouped").to_uppercase(),
            vec!["empty".to_owned(), "none".to_owned(), "space".to_owned()]
        )]
    );
}
