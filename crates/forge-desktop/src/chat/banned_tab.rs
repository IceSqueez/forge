use std::sync::Arc;

use forge_components::{ForgePalette, Icon, Platform, segment, segmented, tr};
use forge_runtime::EventBus;
use gpui::{AnyElement, AppContext, ClickEvent, Context, Entity, IntoElement};

use super::ChatView;
use super::banned_panel::BannedPanel;
use crate::integrations::BuiltinRegistry;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ChatTab {
    #[default]
    Feed,
    Banned,
}

#[derive(Default)]
pub(crate) struct BannedHost {
    pub tab: ChatTab,
    builtins: BuiltinRegistry,
    bus: Option<Arc<EventBus>>,
    panel: Option<Entity<BannedPanel>>,
}

impl BannedHost {
    pub fn panel(&self) -> Option<&Entity<BannedPanel>> {
        self.panel.as_ref().filter(|_| self.tab == ChatTab::Banned)
    }
}

impl ChatView {
    #[must_use]
    pub fn with_ban_list(mut self, builtins: BuiltinRegistry, bus: Arc<EventBus>) -> Self {
        self.banned.builtins = builtins;
        self.banned.bus = Some(bus);
        self
    }

    pub(crate) fn note_moderated(&mut self, platform: Platform, cx: &mut Context<Self>) {
        if let Some(panel) = &self.banned.panel {
            panel.update(cx, |panel, cx| panel.note_moderated(platform, cx));
        }
    }

    pub(crate) fn set_tab(&mut self, tab: ChatTab, cx: &mut Context<Self>) {
        self.banned.tab = tab;
        if let Some(panel) = &self.banned.panel {
            panel.update(cx, |panel, cx| {
                panel.set_visible(tab == ChatTab::Banned, cx)
            });
        } else if tab == ChatTab::Banned {
            let builtins = self.banned.builtins.clone();
            let lifecycle = self.lifecycle.clone();
            let bus = self.banned.bus.clone();
            let rt_handle = self.rt_handle.clone();
            let action_engine = self.action_engine.clone();
            self.banned.panel = Some(cx.new(|cx| {
                BannedPanel::new(builtins, lifecycle, bus, rt_handle, action_engine, cx)
            }));
        }
        cx.notify();
    }

    pub(super) fn render_tabs(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let active = self.banned.tab;
        segmented(
            vec![
                segment(
                    "chat-tab-feed",
                    tr!("chat_tab_feed"),
                    active == ChatTab::Feed,
                    cx.listener(|this, _: &ClickEvent, _, cx| this.set_tab(ChatTab::Feed, cx)),
                )
                .icon(Icon::MessageCircle),
                segment(
                    "chat-tab-banned",
                    tr!("chat_tab_banned"),
                    active == ChatTab::Banned,
                    cx.listener(|this, _: &ClickEvent, _, cx| this.set_tab(ChatTab::Banned, cx)),
                )
                .icon(Icon::Ban),
            ],
            palette,
        )
        .into_any_element()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use forge_components::Platform;
    use forge_events::{Event, EventSource};
    use forge_registry::{
        FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionRegistry,
        SubActionRunner,
    };
    use forge_runtime::{ActionCancelRegistry, ActionEngineHandle, EventBus, spawn_action_engine};
    use forge_storage::chat_history::MockChatHistoryRepo;
    use forge_storage::history::MockHistoryRepo;
    use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry};
    use gpui::{Entity, TestAppContext};

    use super::ChatTab;
    use crate::chat::ChatView;
    use crate::chat::banned_panel::tests::{FakeBans, registry_with, settle};
    use crate::chat::tests::mount_gated;
    use crate::chat::viewer_actions::ViewerTarget;
    use crate::home_stats::Integration;
    use crate::test_support::{StubActions, StubEventLog, runtime};
    use crate::toasts::Toasts;

    const PAST_ANY_RELIST_DEBOUNCE: Duration = Duration::from_secs(10);

    fn mount_with_bans(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        source: &Arc<FakeBans>,
    ) -> Entity<ChatView> {
        mount_with_bans_on(cx, rt, source, EventBus::new(Arc::new(StubEventLog)))
    }

    fn mount_with_bans_on(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        source: &Arc<FakeBans>,
        bus: Arc<EventBus>,
    ) -> Entity<ChatView> {
        let (_feed, view) = mount_gated(cx, rt, None, MockChatHistoryRepo::new());
        let builtins = registry_with(&[(Integration::Twitch, Some(Arc::clone(source)))]);
        view.update(cx, |view, _| {
            view.banned.builtins = builtins;
            view.banned.bus = Some(bus);
        });
        view
    }

    fn show(cx: &mut TestAppContext, view: &Entity<ChatView>, tab: ChatTab) {
        view.update(cx, |view, cx| view.set_tab(tab, cx));
    }

    fn panel_shown(cx: &mut TestAppContext, view: &Entity<ChatView>) -> bool {
        view.read_with(cx, |view, _| view.banned.panel().is_some())
    }

    #[gpui::test]
    fn the_ban_list_is_not_fetched_while_the_feed_tab_is_shown(cx: &mut TestAppContext) {
        let rt = runtime();
        let source = FakeBans::answering(Vec::new());
        let view = mount_with_bans(cx, &rt, &source);

        show(cx, &view, ChatTab::Feed);
        settle(cx, &rt);

        assert_eq!(source.calls(), 0);
        assert!(!panel_shown(cx, &view));
    }

    #[gpui::test]
    fn opening_the_banned_tab_lists_once_and_reopening_it_reuses_the_panel(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let source = FakeBans::answering(Vec::new());
        let view = mount_with_bans(cx, &rt, &source);

        show(cx, &view, ChatTab::Banned);
        settle(cx, &rt);
        assert_eq!(source.calls(), 1);

        show(cx, &view, ChatTab::Feed);
        assert!(!panel_shown(cx, &view));
        show(cx, &view, ChatTab::Banned);
        settle(cx, &rt);

        assert!(panel_shown(cx, &view));
        assert_eq!(source.calls(), 1);
    }

    #[gpui::test]
    fn ban_changes_while_the_feed_tab_is_shown_are_fetched_only_when_banned_is_reopened(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let source = FakeBans::answering(Vec::new());
        let bus = EventBus::new(Arc::new(StubEventLog));
        let view = mount_with_bans_on(cx, &rt, &source, Arc::clone(&bus));
        show(cx, &view, ChatTab::Banned);
        settle(cx, &rt);
        show(cx, &view, ChatTab::Feed);

        bus.publish(Event::new(
            EventSource::Twitch,
            "twitch.channel.ban",
            serde_json::Value::Null,
        ));
        settle(cx, &rt);
        cx.executor().advance_clock(PAST_ANY_RELIST_DEBOUNCE);
        settle(cx, &rt);
        assert_eq!(source.calls(), 1);

        show(cx, &view, ChatTab::Banned);
        settle(cx, &rt);

        assert_eq!(source.calls(), 2);
    }

    const TWITCH_BAN: &str = "twitch.moderation.ban_user";
    const YOUTUBE_BAN: &str = "youtube.moderation.ban_user";
    const KICK_BAN: &str = "kick.moderation.ban";
    const KICK_TIMEOUT: &str = "kick.moderation.timeout";

    struct ModerationRunner {
        kind: &'static str,
        outcome: SubActionOutcome,
    }

    #[async_trait::async_trait]
    impl SubActionRunner for ModerationRunner {
        fn id(&self) -> &str {
            self.kind
        }
        fn category(&self) -> SubActionCategory {
            SubActionCategory::Util
        }
        fn label(&self) -> &str {
            self.kind
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
            _: &SubActionConfig,
            ctx: &RunContext<'_>,
        ) -> (SubActionTelemetry, Option<ArgStack>) {
            (
                StepTimer::start(ctx, self.kind).finish(self.outcome.clone()),
                None,
            )
        }
    }

    struct Moderated {
        view: Entity<ChatView>,
        twitch: Arc<FakeBans>,
        youtube: Arc<FakeBans>,
        kick: Arc<FakeBans>,
    }

    fn moderated_view(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        outcome: SubActionOutcome,
    ) -> Moderated {
        let _enter = rt.enter();
        cx.update(|cx| cx.set_global(Toasts::new()));
        let twitch = FakeBans::answering(Vec::new());
        let youtube = FakeBans::answering(Vec::new());
        let kick = FakeBans::answering(Vec::new());
        let (_feed, view) = mount_gated(cx, rt, None, MockChatHistoryRepo::new());
        let mut registry = SubActionRegistry::new();
        for kind in [TWITCH_BAN, YOUTUBE_BAN, KICK_BAN, KICK_TIMEOUT] {
            registry
                .register(Box::new(ModerationRunner {
                    kind,
                    outcome: outcome.clone(),
                }))
                .expect("distinct kinds");
        }
        let mut history = MockHistoryRepo::new();
        history.expect_save().returning(|_| Ok(()));
        let engine: ActionEngineHandle = spawn_action_engine(
            EventBus::new(Arc::new(StubEventLog)),
            crate::test_support::stub_catalog(),
            Arc::new(StubActions),
            Arc::new(history),
            Arc::new(registry),
            Arc::new(ActionCancelRegistry::new()),
        );
        let builtins = registry_with(&[
            (Integration::Twitch, Some(Arc::clone(&twitch))),
            (Integration::YouTube, Some(Arc::clone(&youtube))),
            (Integration::Kick, Some(Arc::clone(&kick))),
        ]);
        view.update(cx, |view, _| {
            view.banned.builtins = builtins;
            view.banned.bus = Some(EventBus::new(Arc::new(StubEventLog)));
            view.action_engine = engine;
        });
        Moderated {
            view,
            twitch,
            youtube,
            kick,
        }
    }

    fn target(platform: Platform) -> ViewerTarget {
        ViewerTarget {
            platform,
            name: "viewer".to_owned(),
            viewer_id: Some("42".to_owned()),
        }
    }

    fn ban(cx: &mut TestAppContext, view: &Entity<ChatView>, platform: Platform) {
        view.update(cx, |view, cx| view.ban_viewer(target(platform), cx));
    }

    fn settle_past_debounce(cx: &mut TestAppContext, rt: &tokio::runtime::Runtime) {
        settle(cx, rt);
        cx.executor().advance_clock(PAST_ANY_RELIST_DEBOUNCE);
        settle(cx, rt);
    }

    #[gpui::test]
    fn a_successful_card_ban_relists_that_ledger_backed_platform_once_after_the_debounce(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let m = moderated_view(cx, &rt, SubActionOutcome::Success);
        show(cx, &m.view, ChatTab::Banned);
        settle(cx, &rt);
        assert_eq!((m.youtube.calls(), m.kick.calls()), (1, 1));

        ban(cx, &m.view, Platform::YouTube);
        settle(cx, &rt);
        assert_eq!(m.youtube.calls(), 1);
        settle_past_debounce(cx, &rt);
        assert_eq!((m.youtube.calls(), m.kick.calls()), (2, 1));

        ban(cx, &m.view, Platform::Kick);
        settle_past_debounce(cx, &rt);
        assert_eq!((m.youtube.calls(), m.kick.calls()), (2, 2));
    }

    #[gpui::test]
    fn a_successful_card_timeout_relists_the_platform(cx: &mut TestAppContext) {
        let rt = runtime();
        let m = moderated_view(cx, &rt, SubActionOutcome::Success);
        show(cx, &m.view, ChatTab::Banned);
        settle(cx, &rt);

        m.view.update(cx, |view, cx| {
            view.timeout_viewer(target(Platform::Kick), cx)
        });
        settle_past_debounce(cx, &rt);

        assert_eq!(m.kick.calls(), 2);
    }

    #[gpui::test]
    fn a_failed_card_ban_does_not_relist(cx: &mut TestAppContext) {
        let rt = runtime();
        let m = moderated_view(cx, &rt, SubActionOutcome::Failed("denied".to_owned()));
        show(cx, &m.view, ChatTab::Banned);
        settle(cx, &rt);

        ban(cx, &m.view, Platform::YouTube);
        ban(cx, &m.view, Platform::Kick);
        settle_past_debounce(cx, &rt);

        assert_eq!((m.youtube.calls(), m.kick.calls()), (1, 1));
    }

    #[gpui::test]
    fn a_successful_twitch_card_ban_never_relists_through_the_moderation_hook(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let m = moderated_view(cx, &rt, SubActionOutcome::Success);
        show(cx, &m.view, ChatTab::Banned);
        settle(cx, &rt);

        ban(cx, &m.view, Platform::Twitch);
        settle_past_debounce(cx, &rt);

        assert_eq!(m.twitch.calls(), 1);
    }

    #[gpui::test]
    fn a_card_ban_while_banned_is_hidden_fetches_nothing_until_the_tab_is_shown(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let m = moderated_view(cx, &rt, SubActionOutcome::Success);
        show(cx, &m.view, ChatTab::Banned);
        settle(cx, &rt);
        show(cx, &m.view, ChatTab::Feed);

        ban(cx, &m.view, Platform::YouTube);
        settle_past_debounce(cx, &rt);
        assert_eq!(m.youtube.calls(), 1);

        show(cx, &m.view, ChatTab::Banned);
        settle(cx, &rt);

        assert_eq!(m.youtube.calls(), 2);
    }

    #[gpui::test]
    fn a_card_ban_before_the_banned_tab_was_ever_opened_fetches_nothing(cx: &mut TestAppContext) {
        let rt = runtime();
        let m = moderated_view(cx, &rt, SubActionOutcome::Success);

        ban(cx, &m.view, Platform::YouTube);
        ban(cx, &m.view, Platform::Kick);
        settle_past_debounce(cx, &rt);

        assert_eq!((m.youtube.calls(), m.kick.calls()), (0, 0));
        assert!(!panel_shown(cx, &m.view));
    }
}
