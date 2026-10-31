use std::sync::Arc;

use crate::chat_row::ChatBody;
use forge_components::FORGE_DEFAULT;
use forge_registry::SubActionRegistry;
use forge_runtime::{ActionCancelRegistry, EventBus, spawn_action_engine};
use forge_storage::Language;
use forge_storage::MockChatHistoryRepo;
use forge_storage::MockViewerRepo;
use forge_storage::MockVoiceAliasRepo;
use forge_types::EventId;
use forge_types::PlatformId;
use gpui::{App, AppContext as _, Context, Entity, TestAppContext, VisualTestContext};
use time::OffsetDateTime;

use super::viewer_actions::ViewerTarget;
use super::viewer_count::{StaleViewer, note_refreshed, read_viewers};
use super::{ChatView, PlatformFilter};
use crate::chat_author::AuthorKey;
use crate::chat_feed::{ChatFeed, ChatMessage};
use crate::home_stats::{HomeStats, Integration};
use crate::i18n::install_language;
use crate::integration_lifecycle::IntegrationLifecycle;
use crate::test_support::{
    StubActions, StubEventLog, StubHistory, install_presentation, pump, pump_until, runtime,
    switch_lifecycle,
};
use forge_components::Toasts;
use forge_runtime::{LifecycleState, LifecycleStates};
use forge_storage::{ChatAuthorKey, StorageError, Viewer};
use std::collections::HashMap;

const CAP: usize = 5;
const OVERFLOW: usize = 8;

fn at(unix: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(unix).unwrap()
}

fn stored_viewer(platform: PlatformId, viewer_id: &str, last_seen_at: i64) -> Viewer {
    Viewer {
        viewer_id: viewer_id.to_owned(),
        platform,
        username: viewer_id.to_owned(),
        first_seen_at: at(0),
        last_seen_at: at(last_seen_at),
        custom_greeting: false,
    }
}

fn line_by(key: &AuthorKey, unix: i64) -> ChatMessage {
    let (author_id, username) = match &key.handle {
        crate::chat_author::AuthorHandle::ViewerId(id) => (Some(id.clone()), id.clone()),
        crate::chat_author::AuthorHandle::Name(name) => (None, name.clone()),
    };
    ChatMessage {
        platform: key.platform,
        received_at: at(unix),
        username,
        author_id,
        ..message(0, false)
    }
}

fn stale_keys(
    view: &Entity<ChatView>,
    refreshed: &HashMap<AuthorKey, OffsetDateTime>,
    cx: &mut TestAppContext,
) -> Vec<(AuthorKey, OffsetDateTime)> {
    view.read_with(cx, |view, cx| {
        view.stale_viewers(refreshed, cx)
            .into_iter()
            .map(|stale| (stale.key, stale.seen_at))
            .collect()
    })
}

#[test]
fn note_refreshed_records_only_an_author_whose_stored_row_caught_up() {
    let twitch = AuthorKey::by_viewer_id(PlatformId::Twitch, "42");
    for (viewers, caught_up, case) in [
        (
            vec![stored_viewer(PlatformId::Twitch, "42", 100)],
            true,
            "same moment",
        ),
        (
            vec![stored_viewer(PlatformId::Twitch, "42", 101)],
            true,
            "row is newer",
        ),
        (
            vec![stored_viewer(PlatformId::Twitch, "42", 99)],
            false,
            "row is older",
        ),
        (
            vec![stored_viewer(PlatformId::Kick, "42", 100)],
            false,
            "other platform",
        ),
        (
            vec![stored_viewer(PlatformId::Twitch, "7", 100)],
            false,
            "other viewer",
        ),
        (Vec::new(), false, "not stored yet"),
    ] {
        let mut refreshed = HashMap::new();

        note_refreshed(
            vec![StaleViewer {
                key: twitch.clone(),
                seen_at: at(100),
            }],
            &viewers,
            &mut refreshed,
        );

        assert_eq!(
            refreshed.get(&twitch).copied(),
            caught_up.then(|| at(100)),
            "{case}"
        );
    }
}

#[test]
fn note_refreshed_never_records_an_author_with_no_stored_identity() {
    let by_name = AuthorKey::by_name(PlatformId::Kick, "ann");
    let mut refreshed = HashMap::new();

    note_refreshed(
        vec![StaleViewer {
            key: by_name,
            seen_at: at(100),
        }],
        &[stored_viewer(PlatformId::Kick, "ann", 200)],
        &mut refreshed,
    );

    assert!(refreshed.is_empty());
}

#[tokio::test]
async fn read_viewers_skips_an_author_with_no_row_and_keeps_the_ones_found() {
    let mut repo = MockViewerRepo::new();
    repo.expect_get()
        .returning(|platform, id| Ok((id == "known").then(|| stored_viewer(platform, id, 1))));
    let authors = ["unknown", "known"].map(|id| ChatAuthorKey {
        source: PlatformId::Twitch,
        author_id: id.to_owned(),
    });

    let found = read_viewers(&repo, &authors).await.unwrap();

    assert_eq!(found, vec![stored_viewer(PlatformId::Twitch, "known", 1)]);
}

#[tokio::test]
async fn read_viewers_reports_a_store_failure_instead_of_a_partial_answer() {
    let mut repo = MockViewerRepo::new();
    repo.expect_get().returning(|_, _| {
        Err(StorageError::Connection {
            reason: "database is locked".to_owned(),
        })
    });
    let author = ChatAuthorKey {
        source: PlatformId::Twitch,
        author_id: "42".to_owned(),
    };

    assert!(read_viewers(&repo, &[author]).await.is_err());
}

#[gpui::test]
fn an_author_is_stale_until_refreshed_and_again_after_their_next_message(cx: &mut TestAppContext) {
    install_presentation(cx);
    let rt = runtime();
    let (feed, view) = mount(cx, &rt);
    let author = AuthorKey::by_viewer_id(PlatformId::Twitch, "42");
    feed.update(cx, |feed, _| feed.push(line_by(&author, 100)));

    let unseen = stale_keys(&view, &HashMap::new(), cx);
    let refreshed = HashMap::from([(author.clone(), at(100))]);
    let caught_up = stale_keys(&view, &refreshed, cx);
    feed.update(cx, |feed, _| feed.push(line_by(&author, 200)));
    let spoke_again = stale_keys(&view, &refreshed, cx);

    assert_eq!(
        (unseen, caught_up, spoke_again),
        (
            vec![(author.clone(), at(100))],
            Vec::new(),
            vec![(author, at(200))]
        )
    );
}

#[gpui::test]
fn an_author_known_only_by_name_is_never_looked_up(cx: &mut TestAppContext) {
    install_presentation(cx);
    let rt = runtime();
    let (feed, view) = mount(cx, &rt);
    let by_name = AuthorKey::by_name(PlatformId::Kick, "ann");
    feed.update(cx, |feed, _| feed.push(line_by(&by_name, 100)));

    assert_eq!(stale_keys(&view, &HashMap::new(), cx), Vec::new());
}

pub(super) fn message(ix: usize, is_bot: bool) -> ChatMessage {
    ChatMessage {
        id: format!("m{ix}").into(),
        event_id: EventId::new(),
        received_at: OffsetDateTime::from_unix_timestamp(0).unwrap(),
        platform: PlatformId::Twitch,
        badges: vec![],
        username: format!("user{ix}").into(),
        author_id: None,
        author_color: None,
        body: ChatBody::Message("hi".into()),
        is_event: false,
        is_bot,
        moderated: false,
        reply: None,
    }
}

pub(super) fn mount(
    cx: &mut TestAppContext,
    rt: &tokio::runtime::Runtime,
) -> (Entity<ChatFeed>, Entity<ChatView>) {
    mount_gated(cx, rt, None, MockChatHistoryRepo::new())
}

pub(super) fn mount_with_chat_history(
    cx: &mut TestAppContext,
    rt: &tokio::runtime::Runtime,
    chat_history: MockChatHistoryRepo,
) -> (Entity<ChatFeed>, Entity<ChatView>) {
    mount_gated(cx, rt, None, chat_history)
}

pub(super) fn new_view(
    feed: Entity<ChatFeed>,
    rt: &tokio::runtime::Runtime,
    chat_history: MockChatHistoryRepo,
    voice_aliases: MockVoiceAliasRepo,
    cx: &mut Context<ChatView>,
) -> ChatView {
    let home_stats = cx.new(|_| HomeStats::new());
    let mut viewers = MockViewerRepo::new();
    viewers.expect_get().returning(|_, _| Ok(None));
    let engine = spawn_action_engine(
        EventBus::new(Arc::new(StubEventLog)),
        crate::test_support::stub_catalog(),
        Arc::new(StubActions),
        Arc::new(StubHistory),
        Arc::new(SubActionRegistry::new()),
        Arc::new(ActionCancelRegistry::new()),
    );
    ChatView::new(
        feed,
        home_stats,
        rt.handle().clone(),
        Arc::new(viewers),
        Arc::new(chat_history),
        engine,
        Arc::new(voice_aliases),
        None,
        forge_types::Shared::default(),
        FORGE_DEFAULT,
        cx,
    )
}

fn capped_feed(cx: &mut App) -> Entity<ChatFeed> {
    cx.new(|_| {
        let mut feed = ChatFeed::new();
        feed.set_capacity(CAP);
        feed
    })
}

pub(super) fn mount_gated(
    cx: &mut TestAppContext,
    rt: &tokio::runtime::Runtime,
    lifecycle: Option<Entity<IntegrationLifecycle>>,
    chat_history: MockChatHistoryRepo,
) -> (Entity<ChatFeed>, Entity<ChatView>) {
    let _enter = rt.enter();
    let feed = cx.update(capped_feed);
    let view = cx.new(|cx| {
        let view = new_view(
            feed.clone(),
            rt,
            chat_history,
            MockVoiceAliasRepo::new(),
            cx,
        );
        match lifecycle {
            Some(lifecycle) => view.with_lifecycle(lifecycle, cx),
            None => view,
        }
    });
    (feed, view)
}

pub(super) fn mount_in_window<'a>(
    cx: &'a mut TestAppContext,
    rt: &tokio::runtime::Runtime,
    voice_aliases: MockVoiceAliasRepo,
) -> (Entity<ChatView>, &'a mut VisualTestContext) {
    install_language(Language::En);
    install_presentation(cx);
    cx.update(|cx| cx.set_global(Toasts::new()));
    let _enter = rt.enter();
    let feed = cx.update(capped_feed);
    let (view, vcx) = cx.add_window_view(|_window, cx| {
        new_view(feed, rt, MockChatHistoryRepo::new(), voice_aliases, cx)
    });
    vcx.update(|window, cx| {
        window.activate_window();
        forge_components::bind_text_input_keys(cx);
    });
    (view, vcx)
}

pub(super) fn mount_under_root<'a>(
    cx: &'a mut TestAppContext,
    rt: &tokio::runtime::Runtime,
    decorate: impl FnOnce(ChatView) -> ChatView,
) -> (Entity<ChatView>, &'a mut VisualTestContext) {
    install_language(Language::En);
    install_presentation(cx);
    cx.update(|cx| cx.set_global(Toasts::new()));
    let _enter = rt.enter();
    let feed = cx.update(capped_feed);
    let (view, vcx) = crate::test_support::open_under_root(cx, |cx| {
        cx.new(|cx| {
            decorate(new_view(
                feed,
                rt,
                MockChatHistoryRepo::new(),
                MockVoiceAliasRepo::new(),
                cx,
            ))
        })
    });
    (view, vcx)
}

pub(super) fn push_each(
    cx: &mut TestAppContext,
    feed: &Entity<ChatFeed>,
    messages: Vec<ChatMessage>,
) {
    for message in messages {
        feed.update(cx, |feed, cx| {
            feed.push(message);
            cx.notify();
        });
        cx.run_until_parked();
    }
}

#[gpui::test]
fn a_context_menu_ban_goes_out_only_after_it_is_confirmed(cx: &mut TestAppContext) {
    let rt = runtime();
    let (view, vcx) = mount_in_window(cx, &rt, MockVoiceAliasRepo::new());
    let request = |vcx: &mut VisualTestContext| {
        view.update(vcx, |view, cx| {
            view.open_user_menu(
                gpui::point(gpui::px(0.0), gpui::px(0.0)),
                ViewerTarget {
                    platform: PlatformId::Twitch,
                    name: "spammer".to_owned(),
                    viewer_id: Some("42".to_owned()),
                },
                Some("m1".to_owned()),
                cx,
            );
            view.ctx_ban_viewer(cx);
        });
    };

    request(vcx);
    let asked = view.read_with(vcx, |view, _| view.pending_ban.get().is_some());
    view.update(vcx, |view, cx| view.cancel_ban(cx));
    pump(&rt);
    vcx.run_until_parked();

    request(vcx);
    view.update(vcx, |view, cx| view.confirm_ban(cx));
    pump_until(&rt, || {
        vcx.run_until_parked();
        vcx.update(|_, cx| !cx.global::<Toasts>().items().is_empty())
    });

    assert_eq!(
        (
            asked,
            view.read_with(vcx, |view, _| view.pending_ban.get().is_some()),
            vcx.update(|_, cx| cx.global::<Toasts>().items().len()),
        ),
        (true, false, 1)
    );
}

#[gpui::test]
fn unread_keeps_counting_past_the_feed_cap_while_scrolled_up(cx: &mut TestAppContext) {
    let rt = runtime();
    let (feed, view) = mount(cx, &rt);
    view.update(cx, |view, _| view.auto_scroll = false);

    push_each(
        cx,
        &feed,
        (0..CAP + OVERFLOW).map(|ix| message(ix, false)).collect(),
    );

    assert_eq!(view.read_with(cx, |view, _| view.unread), CAP + OVERFLOW);
}

#[gpui::test]
fn the_visible_rows_and_list_follow_eviction_under_an_active_filter(cx: &mut TestAppContext) {
    let rt = runtime();
    let (feed, view) = mount(cx, &rt);
    view.update(cx, |view, cx| {
        view.hide_bots = true;
        view.reset_chat_list(cx);
    });

    let total = CAP + OVERFLOW;
    push_each(
        cx,
        &feed,
        (0..total).map(|ix| message(ix, ix % 2 == 1)).collect(),
    );

    let start = (total - CAP) as u64;
    let expected: Vec<u64> = (start..total as u64).filter(|seq| seq % 2 == 0).collect();
    let (visible, list_len) = view.read_with(cx, |view, _| {
        (
            view.visible.iter().copied().collect::<Vec<u64>>(),
            view.chat_list.item_count(),
        )
    });
    assert_eq!(visible, expected);
    assert_eq!(list_len, expected.len());
}

pub(super) fn chat_states(entries: &[(Integration, LifecycleState)]) -> LifecycleStates {
    entries
        .iter()
        .map(|(integration, state)| (integration.builtin_id(), state.clone()))
        .collect()
}

fn offered(cx: &mut TestAppContext, view: &Entity<ChatView>) -> [bool; 4] {
    view.read_with(cx, |view, cx| {
        [
            PlatformFilter::All,
            PlatformFilter::Single(PlatformId::Twitch),
            PlatformFilter::Single(PlatformId::YouTube),
            PlatformFilter::Single(PlatformId::Kick),
        ]
        .map(|filter| view.filter_offered(filter, cx))
    })
}

#[gpui::test]
fn without_a_lifecycle_every_platform_filter_is_offered(cx: &mut TestAppContext) {
    let rt = runtime();
    let (_feed, view) = mount(cx, &rt);

    assert_eq!(offered(cx, &view), [true; 4]);
}

#[gpui::test]
fn only_platforms_the_user_keeps_on_are_offered_as_filters(cx: &mut TestAppContext) {
    let rt = runtime();
    let lifecycle = cx.new(|_| {
        IntegrationLifecycle::new(chat_states(&[
            (Integration::Twitch, LifecycleState::Running),
            (
                Integration::YouTube,
                LifecycleState::Failed(crate::test_support::start_failure("quota")),
            ),
            (Integration::Kick, LifecycleState::Disabled),
        ]))
    });
    let (_feed, view) = mount_gated(cx, &rt, Some(lifecycle), MockChatHistoryRepo::new());

    assert_eq!(offered(cx, &view), [true, true, true, false]);
}

fn filtering_kick(
    cx: &mut TestAppContext,
    rt: &tokio::runtime::Runtime,
) -> (Entity<IntegrationLifecycle>, Entity<ChatView>) {
    let lifecycle = cx.new(|_| {
        IntegrationLifecycle::new(chat_states(&[
            (Integration::Twitch, LifecycleState::Running),
            (Integration::Kick, LifecycleState::Running),
        ]))
    });
    let (_feed, view) = mount_gated(cx, rt, Some(lifecycle.clone()), MockChatHistoryRepo::new());
    view.update(cx, |view, cx| {
        view.set_platform_filter(PlatformFilter::Single(PlatformId::Kick), cx)
    });
    (lifecycle, view)
}

#[gpui::test]
fn switching_off_the_filtered_platform_resets_the_filter_to_all(cx: &mut TestAppContext) {
    let rt = runtime();
    let (lifecycle, view) = filtering_kick(cx, &rt);

    switch_lifecycle(
        cx,
        &lifecycle,
        chat_states(&[
            (Integration::Twitch, LifecycleState::Running),
            (Integration::Kick, LifecycleState::Stopping),
        ]),
    );

    assert!(view.read_with(cx, |view, _| view.platform_filter == PlatformFilter::All));
}

#[gpui::test]
fn switching_off_another_platform_keeps_the_active_filter(cx: &mut TestAppContext) {
    let rt = runtime();
    let (lifecycle, view) = filtering_kick(cx, &rt);

    switch_lifecycle(
        cx,
        &lifecycle,
        chat_states(&[
            (Integration::Twitch, LifecycleState::Disabled),
            (Integration::Kick, LifecycleState::Running),
        ]),
    );

    assert!(view.read_with(cx, |view, _| {
        view.platform_filter == PlatformFilter::Single(PlatformId::Kick)
    }));
}

fn drawer_avatar_color(username: &str) -> gpui::Rgba {
    let activity = crate::chat_feed::AuthorActivity {
        name: username.to_owned().into(),
        message_count: 1,
        last_seq: 0,
        role: None,
        last_received_at: OffsetDateTime::from_unix_timestamp(0).unwrap(),
    };
    crate::chat_drawer::summary_from_activity(
        &crate::chat_author::AuthorKey::by_name(PlatformId::Twitch, username),
        &activity,
        1,
        &FORGE_DEFAULT,
    )
    .avatar_color
}

#[test]
fn a_name_without_a_platform_color_takes_the_color_of_its_drawer_avatar() {
    for name in [
        "a",
        "ab",
        "ba",
        "bob",
        "Alice",
        "nightbot",
        "xX_gamer_Xx",
        "user42",
        "Kira",
        "zed",
        "\u{41a}\u{456}\u{442}",
        "streamer_fan_2026",
    ] {
        let msg = ChatMessage {
            username: name.into(),
            ..message(0, false)
        };

        assert_eq!(
            ChatView::username_color(&msg, &FORGE_DEFAULT),
            drawer_avatar_color(name),
            "{name}"
        );
    }
}

#[test]
fn a_platform_sent_name_color_wins_over_the_hashed_accent() {
    let sent = gpui::rgba(0x12345678);
    let msg = ChatMessage {
        username: "bob".into(),
        author_color: Some(sent),
        ..message(0, false)
    };

    assert_eq!(ChatView::username_color(&msg, &FORGE_DEFAULT), sent);
}

struct DrawerRig<'a> {
    view: Entity<ChatView>,
    vcx: &'a mut VisualTestContext,
}

const NEWEST: &str = "ada";
const MIDDLE: &str = "ben";
const OLDEST: &str = "cy";

impl<'a> DrawerRig<'a> {
    fn mount(cx: &'a mut TestAppContext, rt: &tokio::runtime::Runtime) -> Self {
        let (view, vcx) = mount_in_window(cx, rt, MockVoiceAliasRepo::new());
        vcx.update(|_, cx| crate::actions::register_key_bindings(cx));
        let feed = view.read_with(vcx, |view, _| view.feed.clone());
        push_each(
            vcx,
            &feed,
            [(OLDEST, 1), (MIDDLE, 2), (NEWEST, 3)]
                .into_iter()
                .map(|(name, unix)| line_by(&Self::key(name), unix))
                .collect(),
        );
        let focus = view.read_with(vcx, |view, _| view.drawer_list_keys.focus().clone());
        vcx.update(|window, cx| window.focus(&focus, cx));
        vcx.run_until_parked();
        Self { view, vcx }
    }

    fn key(name: &str) -> AuthorKey {
        AuthorKey::by_viewer_id(PlatformId::Kick, name)
    }

    fn press(&mut self, keys: &str) {
        self.vcx.simulate_keystrokes(keys);
        self.vcx.run_until_parked();
    }

    fn search(&mut self, query: &str) {
        let changed = forge_components::InputEvent::Changed(query.to_owned().into());
        self.view.update(self.vcx, |view, cx| {
            let field = view.drawer_search.field().clone();
            view.on_drawer_search_event(field, &changed, cx);
        });
        self.vcx.run_until_parked();
    }

    fn selected(&mut self) -> Option<AuthorKey> {
        self.view
            .read_with(self.vcx, |view, _| view.selected_viewer.clone())
    }

    fn add_viewer(&mut self, name: &str, unix: i64) {
        let feed = self.view.read_with(self.vcx, |view, _| view.feed.clone());
        push_each(self.vcx, &feed, vec![line_by(&Self::key(name), unix)]);
    }

    fn click_row(&mut self, position: usize) {
        let (bounds, row_height) = self.view.read_with(self.vcx, |view, _| {
            let scroll = view.drawer_list_scroll.0.borrow();
            let contents = scroll.last_item_size.unwrap().contents;
            let rows = view.drawer_keys.len() as f32;
            (scroll.base_handle.bounds(), contents.height / rows)
        });
        let row_center = gpui::point(
            bounds.center().x,
            bounds.top() + row_height * (position as f32 + 0.5),
        );
        self.vcx.simulate_click(row_center, gpui::Modifiers::none());
        self.vcx.run_until_parked();
    }

    fn at(&mut self, name: &str) -> bool {
        self.view.read_with(self.vcx, |view, _| {
            view.drawer_list_keys.is_at(&Self::key(name))
        })
    }
}

#[gpui::test]
fn arrows_walk_the_viewer_rows_newest_first_and_enter_selects_the_cursor_row(
    cx: &mut TestAppContext,
) {
    let rt = runtime();
    let mut rig = DrawerRig::mount(cx, &rt);

    rig.press("down down");

    assert!(rig.at(MIDDLE));
    assert_eq!(rig.selected(), None);
    rig.press("enter");
    assert_eq!(rig.selected(), Some(DrawerRig::key(MIDDLE)));
}

#[gpui::test]
fn a_drawer_search_that_hides_the_cursor_row_takes_the_cursor_with_it(cx: &mut TestAppContext) {
    let rt = runtime();
    let mut rig = DrawerRig::mount(cx, &rt);
    rig.press("down");

    rig.search(OLDEST);
    rig.press("enter");
    assert_eq!(rig.selected(), None);

    rig.press("down");
    assert!(rig.at(OLDEST));
}

#[gpui::test]
fn arrows_continue_from_the_viewer_a_click_selected(cx: &mut TestAppContext) {
    let rt = runtime();
    let mut rig = DrawerRig::mount(cx, &rt);
    rig.press("down");

    rig.click_row(1);
    rig.press("down");

    assert!(rig.at(OLDEST));
}

#[gpui::test]
fn enter_after_a_click_keeps_the_clicked_viewer_over_the_row_the_arrows_left_behind(
    cx: &mut TestAppContext,
) {
    let rt = runtime();
    let mut rig = DrawerRig::mount(cx, &rt);
    rig.press("down");

    rig.click_row(1);
    rig.press("enter");

    assert_eq!(rig.selected(), Some(DrawerRig::key(MIDDLE)));
}

#[gpui::test]
fn opening_a_viewer_from_outside_the_list_moves_the_cursor_onto_them(cx: &mut TestAppContext) {
    type Open = fn(&mut ChatView, AuthorKey, &mut gpui::Window, &mut Context<ChatView>);
    let openings: [(&str, Open); 2] = [
        ("open", |view, key, _, cx| view.open_viewer(key, cx)),
        ("whisper", |view, key, window, cx| {
            view.open_whisper(key, window, cx)
        }),
    ];
    for (how, open) in openings {
        let rt = runtime();
        let mut rig = DrawerRig::mount(cx, &rt);
        rig.press("down");

        rig.view.update_in(rig.vcx, |view, window, cx| {
            open(view, DrawerRig::key(MIDDLE), window, cx)
        });
        rig.vcx.run_until_parked();

        assert!(rig.at(MIDDLE), "{how}");
    }
}

#[gpui::test]
fn a_hidden_cursor_row_hands_the_arrows_to_the_visible_selected_viewer(cx: &mut TestAppContext) {
    let rt = runtime();
    let mut rig = DrawerRig::mount(cx, &rt);
    rig.add_viewer("bea", 4);
    rig.click_row(2);
    assert_eq!(rig.selected(), Some(DrawerRig::key(MIDDLE)));
    rig.press("down");
    assert!(rig.at(OLDEST));

    rig.search("be");
    rig.press("up");

    assert!(rig.at("bea"));
}

#[gpui::test]
fn the_keyboard_opens_the_user_menu_on_the_drawer_cursor_and_replies_from_it(
    cx: &mut TestAppContext,
) {
    let rt = runtime();
    let (view, vcx) = mount_under_root(cx, &rt, |view| view);
    let feed = view.read_with(vcx, |view, _| view.feed.clone());
    let mut said = message(1, false);
    said.author_id = Some("42".into());
    feed.update(vcx, |feed, cx| {
        feed.push(said);
        cx.notify();
    });
    pump(&rt);
    crate::test_support::settle_focus(vcx);

    crate::test_support::click_outside_the_list(vcx);
    vcx.simulate_keystrokes("down shift-f10");

    let opened = view.read_with(vcx, |view, _| {
        view.user_menu
            .as_ref()
            .map(|menu| (menu.viewer.name.clone(), menu.message_id.clone()))
    });
    assert_eq!(opened, Some(("user1".to_owned(), Some("m1".to_owned()))));

    crate::test_support::settle_focus(vcx);
    vcx.simulate_keystrokes("down enter");
    crate::test_support::settle_focus(vcx);

    let reply = view.read_with(vcx, |view, _| {
        view.reply_target
            .as_ref()
            .map(|target| target.message_id.clone())
    });
    assert_eq!(reply, Some("m1".to_owned()));
}

#[gpui::test]
fn a_drawer_viewer_with_no_message_on_screen_gets_no_reply_entry(cx: &mut TestAppContext) {
    let rt = runtime();
    let (view, vcx) = mount_in_window(cx, &rt, MockVoiceAliasRepo::new());
    let opened = |vcx: &mut VisualTestContext| {
        view.update(vcx, |view, cx| {
            view.open_user_menu(
                gpui::point(gpui::px(0.0), gpui::px(0.0)),
                ViewerTarget {
                    platform: PlatformId::Twitch,
                    name: "quiet".to_owned(),
                    viewer_id: Some("7".to_owned()),
                },
                None,
                cx,
            );
        });
    };

    opened(vcx);
    vcx.update(|window, cx| view.update(cx, |view, cx| view.open_reply(window, cx)));

    assert!(view.read_with(vcx, |view, _| view.reply_target.is_none()));
}

struct FeedRig<'a> {
    view: Entity<ChatView>,
    vcx: &'a mut VisualTestContext,
}

impl<'a> FeedRig<'a> {
    const FIRST: &'static str = "ada";
    const SECOND: &'static str = "bob";
    const THIRD: &'static str = "cyd";

    fn mount(cx: &'a mut TestAppContext, rt: &tokio::runtime::Runtime) -> Self {
        let (view, vcx) = mount_in_window(cx, rt, MockVoiceAliasRepo::new());
        vcx.update(|_, cx| crate::actions::register_key_bindings(cx));
        let feed = view.read_with(vcx, |view, _| view.feed.clone());
        push_each(
            vcx,
            &feed,
            [Self::FIRST, Self::SECOND, Self::THIRD]
                .into_iter()
                .zip(1..)
                .map(|(name, unix)| line_by(&Self::key(name), unix))
                .collect(),
        );
        let focus = view.read_with(vcx, |view, _| view.feed_keys.focus().clone());
        vcx.update(|window, cx| window.focus(&focus, cx));
        vcx.run_until_parked();
        Self { view, vcx }
    }

    fn key(name: &str) -> AuthorKey {
        AuthorKey::by_viewer_id(PlatformId::Twitch, name)
    }

    fn press(&mut self, keys: &str) {
        self.vcx.simulate_keystrokes(keys);
        self.vcx.run_until_parked();
    }

    fn cursor_row(&mut self) -> Option<usize> {
        self.view.read_with(self.vcx, |view, _| {
            let seq = *view.feed_keys.current()?;
            view.visible.iter().position(|row| *row == seq)
        })
    }
}

#[gpui::test]
fn the_first_arrow_lands_on_the_newest_message_and_up_walks_older(cx: &mut TestAppContext) {
    let rt = runtime();
    let mut rig = FeedRig::mount(cx, &rt);

    rig.press("up");
    assert_eq!(rig.cursor_row(), Some(2));
    rig.press("up up up");
    assert_eq!(rig.cursor_row(), Some(0));
    rig.press("down");
    assert_eq!(rig.cursor_row(), Some(1));
}

#[gpui::test]
fn down_from_no_cursor_stays_on_the_newest_message(cx: &mut TestAppContext) {
    let rt = runtime();
    let mut rig = FeedRig::mount(cx, &rt);

    rig.press("down");

    assert_eq!(rig.cursor_row(), Some(2));
}

#[gpui::test]
fn enter_opens_the_viewer_of_the_cursor_row(cx: &mut TestAppContext) {
    let rt = runtime();
    let mut rig = FeedRig::mount(cx, &rt);

    rig.press("up up enter");

    let selected = rig
        .view
        .read_with(rig.vcx, |view, _| view.selected_viewer.clone());
    assert_eq!(selected, Some(FeedRig::key(FeedRig::SECOND)));
}

#[gpui::test]
fn enter_without_a_cursor_opens_nothing(cx: &mut TestAppContext) {
    let rt = runtime();
    let mut rig = FeedRig::mount(cx, &rt);

    rig.press("enter");

    let selected = rig
        .view
        .read_with(rig.vcx, |view, _| view.selected_viewer.clone());
    assert_eq!(selected, None);
}

#[gpui::test]
fn a_cursor_stays_on_its_message_and_freezes_following_while_messages_arrive(
    cx: &mut TestAppContext,
) {
    let rt = runtime();
    let mut rig = FeedRig::mount(cx, &rt);
    rig.press("up up");
    let feed = rig.view.read_with(rig.vcx, |view, _| view.feed.clone());

    push_each(
        rig.vcx,
        &feed,
        vec![
            line_by(&FeedRig::key("dee"), 4),
            line_by(&FeedRig::key("eli"), 5),
        ],
    );

    assert_eq!(rig.cursor_row(), Some(1));
    let (following, unread) = rig
        .view
        .read_with(rig.vcx, |view, _| (view.auto_scroll, view.unread));
    assert_eq!((following, unread), (false, 2));
}

#[gpui::test]
fn jumping_to_the_latest_message_drops_the_cursor(cx: &mut TestAppContext) {
    let rt = runtime();
    let mut rig = FeedRig::mount(cx, &rt);
    rig.press("up");

    rig.view.update(rig.vcx, |view, cx| view.jump_to_latest(cx));

    assert_eq!(rig.cursor_row(), None);
}

#[gpui::test]
fn a_filtered_out_cursor_row_activates_nothing(cx: &mut TestAppContext) {
    let rt = runtime();
    let mut rig = FeedRig::mount(cx, &rt);
    rig.press("up");

    rig.view.update(rig.vcx, |view, cx| {
        view.set_platform_filter(PlatformFilter::Single(PlatformId::Kick), cx)
    });
    rig.press("enter");

    let selected = rig
        .view
        .read_with(rig.vcx, |view, _| view.selected_viewer.clone());
    assert_eq!(selected, None);
}
