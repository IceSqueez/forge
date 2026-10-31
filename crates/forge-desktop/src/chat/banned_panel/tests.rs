use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forge_components::{Density, ThemeId};
use forge_events::{Event, EventSource};
use forge_platform_core::{
    BanDuration, BanEntry, BanListOutcome, BanListSource, BanListUnavailable, BanPage,
    BanPageToken, UnbanAbility, UnbanRefusal,
};
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionRegistry,
    SubActionRunner,
};
use forge_runtime::{ActionCancelRegistry, ActionEngineHandle, EventBus, spawn_action_engine};
use forge_storage::Language;
use forge_storage::MockHistoryRepo;
use forge_types::PlatformId;
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use gpui::{AppContext as _, Entity, TestAppContext, VisualTestContext};

use super::row_keys::BanCursor;
use super::{
    BAN_RELIST_DEBOUNCE, BanGroup, BanRow, BannedPanel, GroupStatus, ListResult,
    UNBAN_RELIST_SUPPRESSION, UnbanState, list_pages, lists_only_what_forge_saw,
};
use crate::chat::platform_gate::platform_integration;
use crate::chat::tests::chat_states;
use crate::home_stats::Integration;
use crate::i18n::{install_language, message_in};
use crate::integration_disabled::disclaimer_line;
use crate::integration_lifecycle::IntegrationLifecycle;
use crate::integrations::{BuiltinObject, BuiltinRegistry};
use crate::test_support::{
    ClickArea, StubActions, StubEventLog, click_outside_the_list, click_until, open_under_root,
    pump, runtime, switch_lifecycle,
};
use crate::unavailable_builtin::unavailable_builtin;
use forge_runtime::LifecycleState;

const UNBAN_KIND: &str = "twitch.moderation.unban_user";
const SETTLE_ROUNDS: usize = 4;

pub(crate) struct FakeBans {
    asked: Mutex<Vec<Option<String>>>,
    answers: Mutex<VecDeque<BanListOutcome>>,
}

impl FakeBans {
    pub(crate) fn answering(answers: Vec<BanListOutcome>) -> Arc<Self> {
        Arc::new(Self {
            asked: Mutex::new(Vec::new()),
            answers: Mutex::new(answers.into()),
        })
    }

    pub(crate) fn asked(&self) -> Vec<Option<String>> {
        self.asked.lock().unwrap().clone()
    }

    pub(crate) fn calls(&self) -> usize {
        self.asked.lock().unwrap().len()
    }
}

#[async_trait::async_trait]
impl BanListSource for FakeBans {
    async fn list_bans(&self, after: Option<&BanPageToken>) -> BanListOutcome {
        self.asked
            .lock()
            .unwrap()
            .push(after.map(|token| token.as_str().to_owned()));
        self.answers
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| outcome(&["1"], None))
    }
}

struct UnbanRunner {
    seen: Arc<Mutex<Vec<SubActionConfig>>>,
    outcome: SubActionOutcome,
}

#[async_trait::async_trait]
impl SubActionRunner for UnbanRunner {
    fn id(&self) -> &str {
        UNBAN_KIND
    }
    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }
    fn label(&self) -> &str {
        UNBAN_KIND
    }
    fn summary(&self) -> &str {
        ""
    }
    fn search_text(&self) -> &str {
        ""
    }
    fn icon_name(&self) -> &str {
        ""
    }
    fn default_config(&self) -> SubActionConfig {
        SubActionConfig::new()
    }
    fn config_fields(&self) -> Vec<FormField> {
        Vec::new()
    }
    fn validate_config(&self, _: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }
    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        self.seen.lock().unwrap().push(config.clone());
        (
            StepTimer::start(ctx, UNBAN_KIND).finish(self.outcome.clone()),
            None,
        )
    }
}

pub(crate) fn entry(viewer_id: &str, unban: UnbanAbility) -> BanEntry {
    BanEntry {
        viewer_id: viewer_id.to_owned(),
        name: format!("viewer{viewer_id}"),
        reason: None,
        moderator: None,
        created_at: None,
        duration: BanDuration::Permanent,
        unban,
    }
}

pub(crate) fn outcome(ids: &[&str], next: Option<&str>) -> BanListOutcome {
    BanListOutcome::Page(BanPage {
        entries: ids
            .iter()
            .map(|id| entry(id, UnbanAbility::Allowed))
            .collect(),
        next: next.map(BanPageToken::new),
    })
}

fn page(ids: &[&str], next: Option<&str>) -> ListResult {
    ListResult::of(outcome(ids, next))
}

fn ids(group: &BanGroup) -> Vec<String> {
    group
        .rows
        .iter()
        .map(|row| row.entry.viewer_id.clone())
        .collect()
}

fn loaded(ids: &[&str], next: Option<&str>) -> BanGroup {
    let mut group = BanGroup::new(PlatformId::Twitch);
    group.apply(false, page(ids, next));
    group
}

fn engine_with(registry: SubActionRegistry) -> ActionEngineHandle {
    let mut history = MockHistoryRepo::new();
    history.expect_save().returning(|_| Ok(()));
    spawn_action_engine(
        EventBus::new(Arc::new(StubEventLog)),
        crate::test_support::stub_catalog(),
        Arc::new(StubActions),
        Arc::new(history),
        Arc::new(registry),
        Arc::new(ActionCancelRegistry::new()),
    )
}

fn unban_engine(
    outcome: SubActionOutcome,
) -> (ActionEngineHandle, Arc<Mutex<Vec<SubActionConfig>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut registry = SubActionRegistry::new();
    registry
        .register(Box::new(UnbanRunner {
            seen: Arc::clone(&seen),
            outcome,
        }))
        .expect("one unban runner");
    (engine_with(registry), seen)
}

pub(crate) fn registry_with(sources: &[(Integration, Option<Arc<FakeBans>>)]) -> BuiltinRegistry {
    let registry = BuiltinRegistry::default();
    for (integration, source) in sources {
        registry.install(BuiltinObject {
            ban_list: source
                .clone()
                .map(|source| source as Arc<dyn BanListSource>),
            ..unavailable_builtin(&integration.builtin_id())
        });
    }
    registry
}

struct Mount {
    builtins: BuiltinRegistry,
    lifecycle: Option<Entity<IntegrationLifecycle>>,
    bus: Option<Arc<EventBus>>,
    engine: Option<ActionEngineHandle>,
}

impl Mount {
    fn over(builtins: BuiltinRegistry) -> Self {
        Self {
            builtins,
            lifecycle: None,
            bus: None,
            engine: None,
        }
    }

    fn open(self, cx: &mut TestAppContext, rt: &tokio::runtime::Runtime) -> Entity<BannedPanel> {
        let _enter = rt.enter();
        let engine = self
            .engine
            .unwrap_or_else(|| engine_with(SubActionRegistry::new()));
        let panel = cx.new(|cx| {
            BannedPanel::new(
                self.builtins,
                self.lifecycle,
                self.bus,
                rt.handle().clone(),
                engine,
                cx,
            )
        });
        settle(cx, rt);
        panel
    }

    fn open_under_root<'a>(
        self,
        cx: &'a mut TestAppContext,
        rt: &tokio::runtime::Runtime,
    ) -> (Entity<BannedPanel>, &'a mut VisualTestContext) {
        let _enter = rt.enter();
        let engine = self
            .engine
            .unwrap_or_else(|| engine_with(SubActionRegistry::new()));
        let (builtins, lifecycle, bus) = (self.builtins, self.lifecycle, self.bus);
        crate::test_support::install_presentation(cx);
        let (panel, vcx) = open_under_root(cx, |cx| {
            cx.new(|cx| BannedPanel::new(builtins, lifecycle, bus, rt.handle().clone(), engine, cx))
        });
        settle(vcx, rt);
        (panel, vcx)
    }
}

pub(crate) fn settle(cx: &mut TestAppContext, rt: &tokio::runtime::Runtime) {
    for _ in 0..SETTLE_ROUNDS {
        cx.run_until_parked();
        pump(rt);
    }
    cx.run_until_parked();
}

fn shown(cx: &mut TestAppContext, panel: &Entity<BannedPanel>) -> Vec<PlatformId> {
    panel.read_with(cx, |panel, _| {
        panel.groups.iter().map(|group| group.platform).collect()
    })
}

fn twitch_ids(cx: &mut TestAppContext, panel: &Entity<BannedPanel>) -> Vec<String> {
    panel.read_with(cx, |panel, _| {
        panel
            .groups
            .iter()
            .find(|group| group.platform == PlatformId::Twitch)
            .map(ids)
            .unwrap_or_default()
    })
}

fn twitch_row_state(
    cx: &mut TestAppContext,
    panel: &Entity<BannedPanel>,
    viewer_id: &str,
) -> Option<UnbanState> {
    panel.read_with(cx, |panel, _| {
        panel
            .groups
            .iter()
            .find(|group| group.platform == PlatformId::Twitch)
            .and_then(|group| {
                group
                    .rows
                    .iter()
                    .find(|row| row.entry.viewer_id == viewer_id)
            })
            .map(|row| row.unban.clone())
    })
}

fn ban_event(kind: &str) -> Event {
    Event::new(EventSource::Twitch, kind, serde_json::Value::Null)
}

#[test]
fn a_first_page_replaces_the_rows_and_marks_the_group_loaded() {
    let mut group = loaded(&["1", "2"], Some("p2"));

    group.apply(false, page(&["3"], None));

    assert_eq!(ids(&group), ["3"]);
    assert_eq!(group.status, GroupStatus::Loaded);
    assert_eq!(group.next, None);
}

#[test]
fn load_more_appends_only_viewers_not_already_listed() {
    let mut group = loaded(&["1", "2"], Some("p2"));

    group.apply(true, page(&["2", "3", "1", "4"], Some("p3")));

    assert_eq!(ids(&group), ["1", "2", "3", "4"]);
    assert_eq!(group.next, Some(BanPageToken::new("p3")));
}

#[test]
fn a_relist_keeps_each_viewers_unban_state_by_viewer_id() {
    let mut group = loaded(&["1", "2", "3"], None);
    group.rows[0].unban = UnbanState::Running;
    group.rows[1].unban = UnbanState::Failed("rate limited".to_owned());

    group.apply(false, page(&["4", "2", "1"], None));

    let states: Vec<(String, UnbanState)> = group
        .rows
        .iter()
        .map(|row| (row.entry.viewer_id.clone(), row.unban.clone()))
        .collect();
    assert_eq!(
        states,
        [
            ("4".to_owned(), UnbanState::Idle),
            (
                "2".to_owned(),
                UnbanState::Failed("rate limited".to_owned())
            ),
            ("1".to_owned(), UnbanState::Running),
        ]
    );
}

#[test]
fn a_failed_load_more_keeps_the_rows_and_the_next_page_and_reports_under_the_button() {
    let mut group = loaded(&["1"], Some("p2"));
    group.loading_more = true;

    group.apply(true, ListResult::Unavailable(BanListUnavailable::Transport));

    assert_eq!(ids(&group), ["1"]);
    assert_eq!(group.next, Some(BanPageToken::new("p2")));
    assert_eq!(group.status, GroupStatus::Loaded);
    assert_eq!(group.more_failed, Some(BanListUnavailable::Transport));
    assert!(!group.loading_more);
}

#[test]
fn a_successful_load_more_clears_the_previous_load_more_error() {
    let mut group = loaded(&["1"], Some("p2"));
    group.apply(
        true,
        ListResult::Unavailable(BanListUnavailable::QuotaExhausted),
    );

    group.apply(true, page(&["2"], None));

    assert_eq!(group.more_failed, None);
    assert_eq!(ids(&group), ["1", "2"]);
}

#[test]
fn an_unavailable_first_page_drops_the_rows_and_the_next_page() {
    let mut group = loaded(&["1"], Some("p2"));

    group.apply(
        false,
        ListResult::Unavailable(BanListUnavailable::MissingScope),
    );

    assert!(group.rows.is_empty());
    assert_eq!(group.next, None);
    assert_eq!(
        group.status,
        GroupStatus::Unavailable(BanListUnavailable::MissingScope)
    );
}

#[test]
fn can_unban_only_an_allowed_idle_or_failed_row_with_a_viewer_id() {
    let refused = UnbanAbility::Refused(UnbanRefusal::BannedOutsideForge);
    for (platform, viewer_id, ability, state, expected) in [
        (
            PlatformId::Twitch,
            "1",
            UnbanAbility::Allowed,
            UnbanState::Idle,
            true,
        ),
        (
            PlatformId::Twitch,
            "1",
            UnbanAbility::Allowed,
            UnbanState::Failed("x".to_owned()),
            true,
        ),
        (
            PlatformId::Twitch,
            "1",
            UnbanAbility::Allowed,
            UnbanState::Running,
            false,
        ),
        (PlatformId::Twitch, "1", refused, UnbanState::Idle, false),
        (
            PlatformId::Twitch,
            "",
            UnbanAbility::Allowed,
            UnbanState::Idle,
            false,
        ),
        (
            PlatformId::YouTube,
            "UCabc",
            UnbanAbility::Allowed,
            UnbanState::Idle,
            true,
        ),
        (
            PlatformId::Kick,
            "4242",
            UnbanAbility::Allowed,
            UnbanState::Idle,
            true,
        ),
        (
            PlatformId::Kick,
            "kick_user",
            UnbanAbility::Allowed,
            UnbanState::Idle,
            false,
        ),
    ] {
        let mut row = BanRow::listed(entry(viewer_id, ability));
        row.unban = state.clone();

        assert_eq!(
            row.can_unban(platform),
            expected,
            "{platform:?} {viewer_id:?} {ability:?} {state:?}"
        );
    }
}

#[gpui::test]
fn only_running_platforms_that_offer_a_ban_list_get_a_group(cx: &mut TestAppContext) {
    let rt = runtime();
    let twitch = FakeBans::answering(Vec::new());
    let youtube = FakeBans::answering(Vec::new());
    let lifecycle = cx.new(|_| {
        IntegrationLifecycle::new(chat_states(&[
            (Integration::Twitch, LifecycleState::Running),
            (
                Integration::YouTube,
                LifecycleState::Failed(crate::test_support::start_failure("quota")),
            ),
            (Integration::Kick, LifecycleState::Running),
        ]))
    });
    let mut mount = Mount::over(registry_with(&[
        (Integration::Twitch, Some(Arc::clone(&twitch))),
        (Integration::YouTube, Some(Arc::clone(&youtube))),
        (Integration::Kick, None),
    ]));
    mount.lifecycle = Some(lifecycle);

    let panel = mount.open(cx, &rt);

    assert_eq!(shown(cx, &panel), [PlatformId::Twitch]);
    assert_eq!(twitch.asked(), [None]);
    assert_eq!(youtube.calls(), 0);
}

#[gpui::test]
fn groups_follow_platform_order_whatever_order_the_builtins_were_installed(
    cx: &mut TestAppContext,
) {
    let rt = runtime();
    let sources = [
        (Integration::Kick, Some(FakeBans::answering(Vec::new()))),
        (Integration::Twitch, Some(FakeBans::answering(Vec::new()))),
        (Integration::YouTube, Some(FakeBans::answering(Vec::new()))),
    ];

    let panel = Mount::over(registry_with(&sources)).open(cx, &rt);

    assert_eq!(
        shown(cx, &panel),
        [PlatformId::Twitch, PlatformId::YouTube, PlatformId::Kick]
    );
}

#[gpui::test]
fn the_first_page_is_listed_without_a_token_and_shown_loaded(cx: &mut TestAppContext) {
    let rt = runtime();
    let twitch = FakeBans::answering(vec![outcome(&["7", "8"], Some("p2"))]);

    let panel = Mount::over(registry_with(&[(
        Integration::Twitch,
        Some(Arc::clone(&twitch)),
    )]))
    .open(cx, &rt);

    assert_eq!(twitch.asked(), [None]);
    assert_eq!(twitch_ids(cx, &panel), ["7", "8"]);
    assert_eq!(
        panel.read_with(cx, |panel, _| panel.groups[0].status.clone()),
        GroupStatus::Loaded
    );
}

#[gpui::test]
fn load_more_asks_for_the_next_token_and_appends_without_duplicates(cx: &mut TestAppContext) {
    let rt = runtime();
    let twitch = FakeBans::answering(vec![
        outcome(&["1", "2"], Some("p2")),
        outcome(&["2", "3"], None),
    ]);
    let panel = Mount::over(registry_with(&[(
        Integration::Twitch,
        Some(Arc::clone(&twitch)),
    )]))
    .open(cx, &rt);

    panel.update(cx, |panel, cx| panel.load_more(PlatformId::Twitch, cx));
    settle(cx, &rt);

    assert_eq!(twitch.asked(), [None, Some("p2".to_owned())]);
    assert_eq!(twitch_ids(cx, &panel), ["1", "2", "3"]);
}

#[gpui::test]
fn load_more_on_the_last_page_or_while_one_is_in_flight_asks_nothing(cx: &mut TestAppContext) {
    let rt = runtime();
    let last = FakeBans::answering(vec![outcome(&["1"], None)]);
    let panel = Mount::over(registry_with(&[(
        Integration::Twitch,
        Some(Arc::clone(&last)),
    )]))
    .open(cx, &rt);
    panel.update(cx, |panel, cx| panel.load_more(PlatformId::Twitch, cx));
    settle(cx, &rt);
    assert_eq!(last.calls(), 1);

    let paged = FakeBans::answering(vec![outcome(&["1"], Some("p2"))]);
    let panel = Mount::over(registry_with(&[(
        Integration::Twitch,
        Some(Arc::clone(&paged)),
    )]))
    .open(cx, &rt);
    panel.update(cx, |panel, cx| {
        panel.load_more(PlatformId::Twitch, cx);
        panel.load_more(PlatformId::Twitch, cx);
    });
    settle(cx, &rt);
    assert_eq!(paged.calls(), 2);
}

#[gpui::test]
fn a_result_for_a_superseded_request_is_dropped(cx: &mut TestAppContext) {
    let rt = runtime();
    let twitch = FakeBans::answering(vec![outcome(&["1"], None)]);
    let panel = Mount::over(registry_with(&[(
        Integration::Twitch,
        Some(Arc::clone(&twitch)),
    )]))
    .open(cx, &rt);
    let current = panel.read_with(cx, |panel, _| panel.groups[0].request_id);

    panel.update(cx, |panel, cx| {
        panel.apply_listed(
            PlatformId::Twitch,
            current - 1,
            false,
            page(&["9"], None),
            cx,
        );
    });
    assert_eq!(twitch_ids(cx, &panel), ["1"]);

    panel.update(cx, |panel, cx| {
        panel.apply_listed(PlatformId::Twitch, current, false, page(&["9"], None), cx);
    });
    assert_eq!(twitch_ids(cx, &panel), ["9"]);
}

#[gpui::test]
fn retry_after_an_unavailable_list_loads_the_group(cx: &mut TestAppContext) {
    let rt = runtime();
    let twitch = FakeBans::answering(vec![
        BanListOutcome::Unavailable(BanListUnavailable::Transport),
        outcome(&["5"], None),
    ]);
    let panel = Mount::over(registry_with(&[(
        Integration::Twitch,
        Some(Arc::clone(&twitch)),
    )]))
    .open(cx, &rt);
    assert_eq!(
        panel.read_with(cx, |panel, _| panel.groups[0].status.clone()),
        GroupStatus::Unavailable(BanListUnavailable::Transport)
    );

    panel.update(cx, |panel, cx| panel.retry(PlatformId::Twitch, cx));
    settle(cx, &rt);

    assert_eq!(twitch.asked(), [None, None]);
    assert_eq!(twitch_ids(cx, &panel), ["5"]);
}

#[gpui::test]
fn a_burst_of_ban_events_relists_the_platform_once_after_the_quiet_debounce(
    cx: &mut TestAppContext,
) {
    let rt = runtime();
    let bus = EventBus::new(Arc::new(StubEventLog));
    let twitch = FakeBans::answering(Vec::new());
    let mut mount = Mount::over(registry_with(&[(
        Integration::Twitch,
        Some(Arc::clone(&twitch)),
    )]));
    mount.bus = Some(Arc::clone(&bus));
    let _panel = mount.open(cx, &rt);
    let half = BAN_RELIST_DEBOUNCE / 2;

    bus.publish(ban_event("twitch.channel.ban"));
    settle(cx, &rt);
    cx.executor().advance_clock(half);
    settle(cx, &rt);
    bus.publish(ban_event("twitch.channel.unban"));
    settle(cx, &rt);
    cx.executor().advance_clock(half);
    settle(cx, &rt);
    assert_eq!(twitch.calls(), 1);

    cx.executor().advance_clock(half + Duration::from_millis(1));
    settle(cx, &rt);
    assert_eq!(twitch.calls(), 2);

    cx.executor().advance_clock(BAN_RELIST_DEBOUNCE * 4);
    settle(cx, &rt);
    assert_eq!(twitch.calls(), 2);
}

#[gpui::test]
fn events_that_are_not_twitch_ban_changes_never_relist_twitch(cx: &mut TestAppContext) {
    let rt = runtime();
    let bus = EventBus::new(Arc::new(StubEventLog));
    let twitch = FakeBans::answering(Vec::new());
    let mut mount = Mount::over(registry_with(&[(
        Integration::Twitch,
        Some(Arc::clone(&twitch)),
    )]));
    mount.bus = Some(Arc::clone(&bus));
    let _panel = mount.open(cx, &rt);

    bus.publish(ban_event("twitch.chat.message"));
    bus.publish(ban_event("kick.moderation.banned"));
    settle(cx, &rt);
    cx.executor().advance_clock(BAN_RELIST_DEBOUNCE * 2);
    settle(cx, &rt);

    assert_eq!(twitch.calls(), 1);
}

#[gpui::test]
fn a_platform_restart_reloads_its_ban_list(cx: &mut TestAppContext) {
    let rt = runtime();
    let twitch = FakeBans::answering(vec![outcome(&["1"], None), outcome(&["2"], None)]);
    let lifecycle = cx.new(|_| {
        IntegrationLifecycle::new(chat_states(&[(
            Integration::Twitch,
            LifecycleState::Running,
        )]))
    });
    let mut mount = Mount::over(registry_with(&[(
        Integration::Twitch,
        Some(Arc::clone(&twitch)),
    )]));
    mount.lifecycle = Some(lifecycle.clone());
    let panel = mount.open(cx, &rt);

    switch_lifecycle(
        cx,
        &lifecycle,
        chat_states(&[(Integration::Twitch, LifecycleState::Starting)]),
    );
    settle(cx, &rt);
    assert!(shown(cx, &panel).is_empty());

    switch_lifecycle(
        cx,
        &lifecycle,
        chat_states(&[(Integration::Twitch, LifecycleState::Running)]),
    );
    settle(cx, &rt);

    assert_eq!(twitch.calls(), 2);
    assert_eq!(twitch_ids(cx, &panel), ["2"]);
}

#[gpui::test]
fn a_lifecycle_change_on_another_platform_does_not_refetch(cx: &mut TestAppContext) {
    let rt = runtime();
    let twitch = FakeBans::answering(Vec::new());
    let lifecycle = cx.new(|_| {
        IntegrationLifecycle::new(chat_states(&[
            (Integration::Twitch, LifecycleState::Running),
            (Integration::Kick, LifecycleState::Starting),
        ]))
    });
    let mut mount = Mount::over(registry_with(&[(
        Integration::Twitch,
        Some(Arc::clone(&twitch)),
    )]));
    mount.lifecycle = Some(lifecycle.clone());
    let _panel = mount.open(cx, &rt);

    switch_lifecycle(
        cx,
        &lifecycle,
        chat_states(&[
            (Integration::Twitch, LifecycleState::Running),
            (Integration::Kick, LifecycleState::Running),
        ]),
    );
    settle(cx, &rt);

    assert_eq!(twitch.calls(), 1);
}

fn unban_panel(
    cx: &mut TestAppContext,
    rt: &tokio::runtime::Runtime,
    entries: Vec<BanEntry>,
    result: SubActionOutcome,
) -> (Entity<BannedPanel>, Arc<Mutex<Vec<SubActionConfig>>>) {
    let (panel, seen, _twitch) = unban_panel_listing(
        cx,
        rt,
        vec![BanListOutcome::Page(BanPage {
            entries,
            next: None,
        })],
        result,
    );
    (panel, seen)
}

fn unban_panel_listing(
    cx: &mut TestAppContext,
    rt: &tokio::runtime::Runtime,
    answers: Vec<BanListOutcome>,
    result: SubActionOutcome,
) -> (
    Entity<BannedPanel>,
    Arc<Mutex<Vec<SubActionConfig>>>,
    Arc<FakeBans>,
) {
    let twitch = FakeBans::answering(answers);
    let (engine, seen) = {
        let _enter = rt.enter();
        unban_engine(result)
    };
    let mut mount = Mount::over(registry_with(&[(
        Integration::Twitch,
        Some(Arc::clone(&twitch)),
    )]));
    mount.engine = Some(engine);
    (mount.open(cx, rt), seen, twitch)
}

fn unban(cx: &mut TestAppContext, panel: &Entity<BannedPanel>, viewer_id: &str) {
    let viewer_id = viewer_id.to_owned();
    panel.update(cx, |panel, cx| {
        panel.unban(PlatformId::Twitch, viewer_id, cx)
    });
}

#[gpui::test]
fn unban_runs_the_twitch_unban_step_for_the_viewer_and_removes_the_row(cx: &mut TestAppContext) {
    let rt = runtime();
    let (panel, seen) = unban_panel(
        cx,
        &rt,
        vec![
            entry("1001", UnbanAbility::Allowed),
            entry("1002", UnbanAbility::Allowed),
        ],
        SubActionOutcome::Success,
    );

    unban(cx, &panel, "1001");
    settle(cx, &rt);

    assert_eq!(
        *seen.lock().unwrap(),
        [SubActionConfig::from([(
            "target_user_id".to_owned(),
            Variant::String("1001".to_owned())
        )])]
    );
    assert_eq!(twitch_ids(cx, &panel), ["1002"]);
}

#[gpui::test]
fn a_failed_unban_keeps_the_row_and_marks_only_that_row_failed(cx: &mut TestAppContext) {
    let rt = runtime();
    let (panel, _seen) = unban_panel(
        cx,
        &rt,
        vec![
            entry("1001", UnbanAbility::Allowed),
            entry("1002", UnbanAbility::Allowed),
        ],
        SubActionOutcome::Failed(forge_types::VerbatimText::new("missing scope").into()),
    );

    unban(cx, &panel, "1001");
    settle(cx, &rt);

    assert_eq!(twitch_ids(cx, &panel), ["1001", "1002"]);
    assert_eq!(
        twitch_row_state(cx, &panel, "1001"),
        Some(UnbanState::Failed("missing scope".to_owned()))
    );
    assert_eq!(twitch_row_state(cx, &panel, "1002"), Some(UnbanState::Idle));
}

#[gpui::test]
fn a_second_unban_click_while_one_runs_dispatches_nothing_more(cx: &mut TestAppContext) {
    let rt = runtime();
    let (panel, seen) = unban_panel(
        cx,
        &rt,
        vec![entry("1001", UnbanAbility::Allowed)],
        SubActionOutcome::Failed(forge_types::VerbatimText::new("busy").into()),
    );

    unban(cx, &panel, "1001");
    assert_eq!(
        twitch_row_state(cx, &panel, "1001"),
        Some(UnbanState::Running)
    );
    unban(cx, &panel, "1001");
    settle(cx, &rt);

    assert_eq!(seen.lock().unwrap().len(), 1);
}

#[gpui::test]
fn a_ban_placed_outside_forge_is_never_unbanned(cx: &mut TestAppContext) {
    let rt = runtime();
    let (panel, seen) = unban_panel(
        cx,
        &rt,
        vec![entry(
            "1001",
            UnbanAbility::Refused(UnbanRefusal::BannedOutsideForge),
        )],
        SubActionOutcome::Success,
    );

    unban(cx, &panel, "1001");
    settle(cx, &rt);

    assert!(seen.lock().unwrap().is_empty());
    assert_eq!(twitch_row_state(cx, &panel, "1001"), Some(UnbanState::Idle));
}

fn group_of<R>(
    cx: &mut TestAppContext,
    panel: &Entity<BannedPanel>,
    platform: PlatformId,
    read: impl FnOnce(&BanGroup) -> R,
) -> R {
    panel.read_with(cx, |panel, _| {
        read(
            panel
                .groups
                .iter()
                .find(|group| group.platform == platform)
                .expect("the platform has a group"),
        )
    })
}

fn set_visible(cx: &mut TestAppContext, panel: &Entity<BannedPanel>, visible: bool) {
    panel.update(cx, |panel, cx| panel.set_visible(visible, cx));
}

fn with_bus(
    cx: &mut TestAppContext,
    rt: &tokio::runtime::Runtime,
    sources: &[(Integration, Option<Arc<FakeBans>>)],
) -> (Entity<BannedPanel>, Arc<EventBus>) {
    let bus = EventBus::new(Arc::new(StubEventLog));
    let mut mount = Mount::over(registry_with(sources));
    mount.bus = Some(Arc::clone(&bus));
    (mount.open(cx, rt), bus)
}

fn quiet_past_debounce(cx: &mut TestAppContext, rt: &tokio::runtime::Runtime) {
    settle(cx, rt);
    cx.executor()
        .advance_clock(BAN_RELIST_DEBOUNCE + Duration::from_millis(1));
    settle(cx, rt);
}

fn unavailable() -> BanListOutcome {
    BanListOutcome::Unavailable(BanListUnavailable::Transport)
}

#[test]
fn a_relist_walks_the_loaded_depth_page_by_page_and_keeps_what_it_got_before_a_failure() {
    let some = |token: &str| Some(token.to_owned());
    for (answers, depth, asked, expected_ids, expected_next) in [
        (
            vec![
                outcome(&["1", "2"], Some("a")),
                outcome(&["2", "3"], Some("b")),
                outcome(&["4"], Some("c")),
            ],
            3,
            vec![None, some("a"), some("b")],
            Some(vec!["1", "2", "3", "4"]),
            Some("c"),
        ),
        (
            vec![outcome(&["1"], Some("a")), outcome(&["2"], None)],
            3,
            vec![None, some("a")],
            Some(vec!["1", "2"]),
            None,
        ),
        (
            vec![outcome(&["1"], Some("a")), unavailable()],
            3,
            vec![None, some("a")],
            Some(vec!["1"]),
            Some("a"),
        ),
        (
            vec![
                outcome(&["1"], Some("a")),
                outcome(&["2"], Some("b")),
                unavailable(),
            ],
            3,
            vec![None, some("a"), some("b")],
            Some(vec!["1", "2"]),
            Some("b"),
        ),
        (
            vec![outcome(&["1"], Some("a")), outcome(&["2"], Some("b"))],
            1,
            vec![None],
            Some(vec!["1"]),
            Some("a"),
        ),
        (
            vec![unavailable(), outcome(&["2"], None)],
            3,
            vec![None],
            None,
            None,
        ),
    ] {
        let source = FakeBans::answering(answers);

        let result = runtime().block_on(list_pages(source.as_ref(), None, depth));

        let listed = match result {
            ListResult::Page { rows, next } => Some((
                rows.into_iter()
                    .map(|row| row.entry.viewer_id)
                    .collect::<Vec<_>>(),
                next,
            )),
            ListResult::Unavailable(_) => None,
        };
        assert_eq!(source.asked(), asked, "depth {depth}");
        assert_eq!(
            listed,
            expected_ids.map(|ids| (
                ids.into_iter().map(str::to_owned).collect::<Vec<_>>(),
                expected_next.map(BanPageToken::new)
            )),
            "depth {depth}"
        );
    }
}

#[test]
fn a_load_more_continues_from_its_token_for_one_page() {
    let source = FakeBans::answering(vec![outcome(&["5"], Some("c")), outcome(&["6"], None)]);

    let result = runtime().block_on(list_pages(source.as_ref(), Some(BanPageToken::new("b")), 1));

    assert_eq!(source.asked(), [Some("b".to_owned())]);
    assert!(matches!(
        result,
        ListResult::Page { next: Some(ref next), .. } if *next == BanPageToken::new("c")
    ));
}

#[test]
fn the_relist_depth_counts_successful_load_mores_and_resets_when_the_first_page_fails() {
    let mut group = loaded(&["1"], Some("p2"));
    let mut depths = vec![group.pages];

    group.apply(true, page(&["2"], Some("p3")));
    depths.push(group.pages);
    group.apply(true, ListResult::Unavailable(BanListUnavailable::Transport));
    depths.push(group.pages);
    group.apply(false, page(&["1", "2"], Some("p3")));
    depths.push(group.pages);
    group.apply(
        false,
        ListResult::Unavailable(BanListUnavailable::NotConnected),
    );
    depths.push(group.pages);

    assert_eq!(depths, [1, 2, 2, 2, 1]);
}

#[test]
fn a_recent_unban_hides_the_viewer_until_its_suppression_window_ends() {
    let start = Instant::now();
    let until = start + UNBAN_RELIST_SUPPRESSION;
    for (now, expected, still_suppressed) in [
        (start, vec!["2"], true),
        (until - Duration::from_millis(1), vec!["2"], true),
        (until, vec!["1", "2"], false),
        (until + Duration::from_secs(1), vec!["1", "2"], false),
    ] {
        let mut group = BanGroup::new(PlatformId::Kick);
        group.recently_unbanned = vec![("1".to_owned(), until)];

        let shown = match group.without_recent_unbans(page(&["1", "2"], None), now) {
            ListResult::Page { rows, .. } => rows
                .into_iter()
                .map(|row| row.entry.viewer_id)
                .collect::<Vec<_>>(),
            ListResult::Unavailable(_) => Vec::new(),
        };

        assert_eq!(
            (shown, !group.recently_unbanned.is_empty()),
            (
                expected.into_iter().map(str::to_owned).collect(),
                still_suppressed
            ),
            "{:?} after start",
            now - start
        );
    }
}

#[test]
fn each_ban_group_notes_what_its_platform_list_can_and_cannot_show() {
    let palette = ThemeId::ForgeDefault.palette();
    for (platform, seen_by_forge, disclaimer) in [
        (PlatformId::Twitch, false, false),
        (PlatformId::YouTube, true, false),
        (PlatformId::Kick, true, true),
    ] {
        let notes = BannedPanel::render_group_notes(platform, 0, &palette, Density::Cozy);
        let kick_disclaimer = disclaimer_line(
            &platform_integration(platform).builtin_id(),
            &palette,
            Density::Cozy,
        );

        assert_eq!(
            (
                lists_only_what_forge_saw(platform),
                kick_disclaimer.is_some(),
                notes.is_some()
            ),
            (seen_by_forge, disclaimer, seen_by_forge || disclaimer),
            "{platform:?}"
        );
    }
}

#[test]
fn a_full_page_from_a_complete_ban_list_adds_no_cap_note() {
    let palette = ThemeId::ForgeDefault.palette();

    let notes = BannedPanel::render_group_notes(
        PlatformId::Twitch,
        forge_storage::BAN_LEDGER_LIST_CAP,
        &palette,
        Density::Cozy,
    );

    assert!(notes.is_none());
}

#[gpui::test]
fn while_hidden_a_ban_event_fetches_nothing_and_marks_the_group_stale(cx: &mut TestAppContext) {
    let rt = runtime();
    let twitch = FakeBans::answering(Vec::new());
    let (panel, bus) = with_bus(cx, &rt, &[(Integration::Twitch, Some(Arc::clone(&twitch)))]);

    set_visible(cx, &panel, false);
    bus.publish(ban_event("twitch.channel.ban"));
    quiet_past_debounce(cx, &rt);

    assert_eq!(twitch.calls(), 1);
    assert!(group_of(cx, &panel, PlatformId::Twitch, |group| group.stale));
}

#[gpui::test]
fn while_hidden_a_ban_event_for_a_stopped_platform_adds_no_group(cx: &mut TestAppContext) {
    let rt = runtime();
    let twitch = FakeBans::answering(Vec::new());
    let bus = crate::test_support::quiet_bus(&rt);
    let lifecycle = cx.new(|_| {
        IntegrationLifecycle::new(chat_states(&[(
            Integration::Twitch,
            LifecycleState::Disabled,
        )]))
    });
    let mut mount = Mount::over(registry_with(&[(
        Integration::Twitch,
        Some(Arc::clone(&twitch)),
    )]));
    mount.lifecycle = Some(lifecycle);
    mount.bus = Some(Arc::clone(&bus));
    let panel = mount.open(cx, &rt);

    set_visible(cx, &panel, false);
    bus.publish(ban_event("twitch.channel.ban"));
    quiet_past_debounce(cx, &rt);

    assert_eq!(shown(cx, &panel), Vec::<PlatformId>::new());
}

#[gpui::test]
fn showing_again_relists_the_stale_groups_and_the_ledger_backed_ones(cx: &mut TestAppContext) {
    let rt = runtime();
    let twitch = FakeBans::answering(vec![outcome(&["1"], None), outcome(&["2"], None)]);
    let kick = FakeBans::answering(Vec::new());
    let (panel, bus) = with_bus(
        cx,
        &rt,
        &[
            (Integration::Twitch, Some(Arc::clone(&twitch))),
            (Integration::Kick, Some(Arc::clone(&kick))),
        ],
    );
    set_visible(cx, &panel, false);
    bus.publish(ban_event("twitch.channel.ban"));
    quiet_past_debounce(cx, &rt);

    set_visible(cx, &panel, true);
    settle(cx, &rt);

    assert_eq!((twitch.calls(), kick.calls()), (2, 2));
    assert_eq!(twitch_ids(cx, &panel), ["2"]);
    assert!(!group_of(cx, &panel, PlatformId::Twitch, |group| group.stale));
}

#[gpui::test]
fn a_debounced_relist_that_comes_due_after_hiding_waits_for_the_tab(cx: &mut TestAppContext) {
    let rt = runtime();
    let twitch = FakeBans::answering(Vec::new());
    let (panel, bus) = with_bus(cx, &rt, &[(Integration::Twitch, Some(Arc::clone(&twitch)))]);

    bus.publish(ban_event("twitch.channel.ban"));
    settle(cx, &rt);
    set_visible(cx, &panel, false);
    quiet_past_debounce(cx, &rt);

    assert_eq!(twitch.calls(), 1);
    assert!(group_of(cx, &panel, PlatformId::Twitch, |group| group.stale));
}

#[gpui::test]
fn a_platform_restart_while_hidden_is_listed_only_once_shown(cx: &mut TestAppContext) {
    let rt = runtime();
    let twitch = FakeBans::answering(vec![outcome(&["1"], None), outcome(&["2"], None)]);
    let lifecycle = cx.new(|_| {
        IntegrationLifecycle::new(chat_states(&[(
            Integration::Twitch,
            LifecycleState::Running,
        )]))
    });
    let mut mount = Mount::over(registry_with(&[(
        Integration::Twitch,
        Some(Arc::clone(&twitch)),
    )]));
    mount.lifecycle = Some(lifecycle.clone());
    let panel = mount.open(cx, &rt);
    set_visible(cx, &panel, false);

    for state in [LifecycleState::Starting, LifecycleState::Running] {
        switch_lifecycle(cx, &lifecycle, chat_states(&[(Integration::Twitch, state)]));
        settle(cx, &rt);
    }
    assert_eq!(twitch.calls(), 1);

    set_visible(cx, &panel, true);
    settle(cx, &rt);

    assert_eq!(twitch.calls(), 2);
    assert_eq!(twitch_ids(cx, &panel), ["2"]);
}

#[gpui::test]
fn a_relist_after_load_more_keeps_every_page_the_viewer_had_loaded(cx: &mut TestAppContext) {
    let rt = runtime();
    let twitch = FakeBans::answering(vec![
        outcome(&["1", "2"], Some("p2")),
        outcome(&["3"], Some("p3")),
        outcome(&["1", "2"], Some("p2")),
        outcome(&["3", "4"], Some("p3")),
    ]);
    let (panel, bus) = with_bus(cx, &rt, &[(Integration::Twitch, Some(Arc::clone(&twitch)))]);
    panel.update(cx, |panel, cx| panel.load_more(PlatformId::Twitch, cx));
    settle(cx, &rt);

    bus.publish(ban_event("twitch.channel.ban"));
    quiet_past_debounce(cx, &rt);

    let p2 = Some("p2".to_owned());
    assert_eq!(twitch.asked(), [None, p2.clone(), None, p2]);
    assert_eq!(twitch_ids(cx, &panel), ["1", "2", "3", "4"]);
    assert_eq!(
        group_of(cx, &panel, PlatformId::Twitch, |group| group.next.clone()),
        Some(BanPageToken::new("p3"))
    );
}

#[gpui::test]
fn an_unbanned_viewer_stays_off_every_list_inside_the_suppression_window(cx: &mut TestAppContext) {
    let rt = runtime();
    let (panel, _seen, _twitch) = unban_panel_listing(
        cx,
        &rt,
        vec![
            outcome(&["1001", "1002"], Some("p2")),
            outcome(&["1001", "1002"], Some("p2")),
            outcome(&["1001", "1003"], None),
        ],
        SubActionOutcome::Success,
    );
    unban(cx, &panel, "1001");
    settle(cx, &rt);
    cx.executor()
        .advance_clock(UNBAN_RELIST_SUPPRESSION - Duration::from_millis(1));

    panel.update(cx, |panel, cx| panel.retry(PlatformId::Twitch, cx));
    settle(cx, &rt);
    panel.update(cx, |panel, cx| panel.load_more(PlatformId::Twitch, cx));
    settle(cx, &rt);

    assert_eq!(twitch_ids(cx, &panel), ["1002", "1003"]);
}

#[gpui::test]
fn an_unbanned_viewer_listed_again_after_the_suppression_window_is_shown(cx: &mut TestAppContext) {
    let rt = runtime();
    let (panel, _seen, _twitch) = unban_panel_listing(
        cx,
        &rt,
        vec![
            outcome(&["1001", "1002"], None),
            outcome(&["1001", "1002"], None),
        ],
        SubActionOutcome::Success,
    );
    unban(cx, &panel, "1001");
    settle(cx, &rt);
    cx.executor().advance_clock(UNBAN_RELIST_SUPPRESSION);

    panel.update(cx, |panel, cx| panel.retry(PlatformId::Twitch, cx));
    settle(cx, &rt);

    assert_eq!(twitch_ids(cx, &panel), ["1001", "1002"]);
}

#[gpui::test]
fn an_unban_cancelled_before_it_finished_is_reported_in_the_ui_language(cx: &mut TestAppContext) {
    install_language(Language::Uk);
    let rt = runtime();
    let (panel, seen) = unban_panel(
        cx,
        &rt,
        vec![entry("1001", UnbanAbility::Allowed)],
        SubActionOutcome::Success,
    );

    let cancelled = message_in(Language::Uk, "chat_dispatch_cancelled");
    assert_ne!(cancelled, "chat_dispatch_cancelled");

    unban(cx, &panel, "1001");
    drop(rt);
    cx.run_until_parked();

    assert!(seen.lock().unwrap().is_empty());
    assert_eq!(
        twitch_row_state(cx, &panel, "1001"),
        Some(UnbanState::Failed(cancelled))
    );
}

type KeyedPanel<'a> = (
    Entity<BannedPanel>,
    Arc<Mutex<Vec<SubActionConfig>>>,
    Arc<FakeBans>,
    &'a mut VisualTestContext,
);

fn keyed_panel<'a>(
    cx: &'a mut TestAppContext,
    rt: &tokio::runtime::Runtime,
    answers: Vec<BanListOutcome>,
) -> KeyedPanel<'a> {
    let twitch = FakeBans::answering(answers);
    let (engine, seen) = {
        let _enter = rt.enter();
        unban_engine(SubActionOutcome::Success)
    };
    let mut mount = Mount::over(registry_with(&[(
        Integration::Twitch,
        Some(Arc::clone(&twitch)),
    )]));
    mount.engine = Some(engine);
    let (panel, vcx) = mount.open_under_root(cx, rt);
    (panel, seen, twitch, vcx)
}

fn menu_open(vcx: &mut VisualTestContext, panel: &Entity<BannedPanel>) -> bool {
    panel.read_with(vcx, |panel, _| panel.row_menu.is_some())
}

fn at_viewer(viewer_id: &str) -> BanCursor {
    BanCursor::Viewer {
        platform: PlatformId::Twitch,
        viewer_id: viewer_id.to_owned(),
    }
}

fn cursor_at(vcx: &mut VisualTestContext, panel: &Entity<BannedPanel>, row: &BanCursor) -> bool {
    panel.read_with(vcx, |panel, _| panel.row_keys.is_at(row))
}

#[gpui::test]
fn after_a_click_outside_the_keyboard_unbans_the_viewer_at_the_cursor(cx: &mut TestAppContext) {
    let rt = runtime();
    let (panel, seen, _twitch, vcx) = keyed_panel(cx, &rt, vec![outcome(&["1001", "1002"], None)]);

    click_outside_the_list(vcx);
    vcx.simulate_keystrokes("down down enter");
    assert!(menu_open(vcx, &panel));
    vcx.simulate_keystrokes("down enter");
    settle(vcx, &rt);

    assert_eq!(
        *seen.lock().unwrap(),
        [SubActionConfig::from([(
            "target_user_id".to_owned(),
            Variant::String("1002".to_owned())
        )])]
    );
    assert_eq!(twitch_ids(vcx, &panel), ["1001"]);
    assert!(!menu_open(vcx, &panel));
}

#[gpui::test]
fn the_row_menu_key_opens_the_same_menu_as_enter(cx: &mut TestAppContext) {
    let rt = runtime();
    let (panel, _seen, _twitch, vcx) = keyed_panel(cx, &rt, vec![outcome(&["1001"], None)]);

    click_outside_the_list(vcx);
    vcx.simulate_keystrokes("down shift-f10");

    assert!(menu_open(vcx, &panel));
}

#[gpui::test]
fn enter_alone_on_an_opened_row_menu_unbans_nobody(cx: &mut TestAppContext) {
    let rt = runtime();
    let (panel, seen, _twitch, vcx) = keyed_panel(cx, &rt, vec![outcome(&["1001"], None)]);

    click_outside_the_list(vcx);
    vcx.simulate_keystrokes("down enter enter");
    settle(vcx, &rt);

    assert!(seen.lock().unwrap().is_empty());
    assert!(menu_open(vcx, &panel));
}

#[gpui::test]
fn escape_closes_the_row_menu_and_the_arrows_are_back_on_the_list(cx: &mut TestAppContext) {
    let rt = runtime();
    let (panel, _seen, _twitch, vcx) = keyed_panel(cx, &rt, vec![outcome(&["1", "2"], None)]);

    click_outside_the_list(vcx);
    vcx.simulate_keystrokes("down enter");
    crate::test_support::settle_focus(vcx);
    assert!(menu_open(vcx, &panel));
    vcx.simulate_keystrokes("escape");
    crate::test_support::settle_focus(vcx);
    assert!(!menu_open(vcx, &panel));

    vcx.simulate_keystrokes("down");

    assert!(cursor_at(vcx, &panel, &at_viewer("2")));
}

#[gpui::test]
fn a_refused_viewers_row_menu_offers_nothing_to_confirm(cx: &mut TestAppContext) {
    let rt = runtime();
    let refused = BanListOutcome::Page(BanPage {
        entries: vec![entry(
            "1001",
            UnbanAbility::Refused(UnbanRefusal::BannedOutsideForge),
        )],
        next: None,
    });
    let (panel, seen, _twitch, vcx) = keyed_panel(cx, &rt, vec![refused]);

    click_outside_the_list(vcx);
    vcx.simulate_keystrokes("down enter down enter");
    settle(vcx, &rt);

    assert!(seen.lock().unwrap().is_empty());
    assert!(menu_open(vcx, &panel));
}

#[gpui::test]
fn the_keyboard_reaches_load_more_past_the_last_row_of_its_group(cx: &mut TestAppContext) {
    let rt = runtime();
    let (panel, _seen, twitch, vcx) = keyed_panel(
        cx,
        &rt,
        vec![outcome(&["1"], Some("p2")), outcome(&["2"], None)],
    );

    click_outside_the_list(vcx);
    vcx.simulate_keystrokes("down down enter");
    settle(vcx, &rt);

    assert_eq!(twitch.asked(), [None, Some("p2".to_owned())]);
    assert_eq!(twitch_ids(vcx, &panel), ["1", "2"]);
}

#[gpui::test]
fn the_keyboard_retries_an_unavailable_group(cx: &mut TestAppContext) {
    let rt = runtime();
    let (panel, _seen, twitch, vcx) =
        keyed_panel(cx, &rt, vec![unavailable(), outcome(&["5"], None)]);

    click_outside_the_list(vcx);
    vcx.simulate_keystrokes("down enter");
    settle(vcx, &rt);

    assert_eq!(twitch.calls(), 2);
    assert_eq!(twitch_ids(vcx, &panel), ["5"]);
}

#[gpui::test]
fn a_click_on_a_row_moves_the_cursor_there(cx: &mut TestAppContext) {
    let rt = runtime();
    let (panel, _seen, _twitch, vcx) = keyed_panel(cx, &rt, vec![outcome(&["1", "2", "3"], None)]);
    let middle = at_viewer("2");

    let hit = click_until(
        vcx,
        &ClickArea {
            x: 40.0..41.0,
            y: 40.0..400.0,
            step_x: 1.0,
            step_y: 4.0,
        },
        |vcx| cursor_at(vcx, &panel, &middle),
    );

    assert!(hit);
    vcx.simulate_keystrokes("down");
    assert!(cursor_at(vcx, &panel, &at_viewer("3")));
}

#[gpui::test]
fn a_row_menu_closes_when_a_relist_drops_its_viewer(cx: &mut TestAppContext) {
    let rt = runtime();
    let (panel, _seen, _twitch, vcx) = keyed_panel(cx, &rt, vec![outcome(&["1"], None)]);
    click_outside_the_list(vcx);
    vcx.simulate_keystrokes("down enter");
    crate::test_support::settle_focus(vcx);

    let current = panel.read_with(vcx, |panel, _| panel.groups[0].request_id);
    panel.update(vcx, |panel, cx| {
        panel.apply_listed(PlatformId::Twitch, current, false, page(&["2"], None), cx);
    });
    crate::test_support::settle_focus(vcx);

    assert!(!menu_open(vcx, &panel));
}
