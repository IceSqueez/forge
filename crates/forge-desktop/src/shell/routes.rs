use super::*;
use crate::navigation::{SettingsSection, TtsSection};
use forge_platform_core::IntegrationCategory;
use forge_types::{ActionId, TriggerInstanceId};
use std::sync::Arc;

use forge_storage::{CredentialsRepo, GlobalsRepo, ScriptRepo, SettingsRepo};
use forge_types::IntegrationAvailability;
use gpui::{AppContext, Context};

use crate::actions_screen::ScreenActionsView;
use crate::chat::{ChatServices, ChatView};
use crate::clip_key_state::ClipKeyState;
use crate::event_feed::EventFeedView;
use crate::globals_view::GlobalsView;
use crate::home::HomeView;
use crate::integrations_hub::IntegrationsHubView;
use crate::overlays_screen::{OverlaysLaunch, OverlaysView};
use crate::pack_services::PackServices;
use crate::queues::QueuesView;
use crate::runtime_handles::RuntimeHandles;
use crate::script_editor::ScriptEditorView;
use crate::server_console::ServerConsoleView;
use crate::settings::SettingsView;
use crate::soundboard::SoundboardView;
use crate::topics::Topics;
use crate::triggers_screen::TriggersRegistryView;
use crate::tts::TtsView;
use forge_components::ActivePresentation;
use forge_hotkey::HotkeySyncedTriggerRepo;

impl AppShell {
    pub(super) fn home_route(
        topics: &Topics,
        handles: &Arc<RuntimeHandles>,
        cx: &mut Context<Self>,
    ) -> RoutedContent {
        let home_pack = PackServices::of(handles);
        let home_lifecycle = topics.integration_lifecycle.clone();
        let home = cx.new(|cx| {
            HomeView::new(
                topics.home_stats.clone(),
                topics.event_loss.clone(),
                home_pack,
                cx,
            )
            .with_lifecycle(home_lifecycle, cx)
        });
        Self::follow_nav(&home, cx);
        let home_stats = topics.home_stats.clone();
        let backend = Arc::clone(&handles.backend);
        let stats_registry = Arc::clone(&handles.automation.trigger_registry);
        let rt_handle = handles.rt_handle.clone();
        cx.spawn(async move |_shell, cx| {
            refresh_dashboard_stats(home_stats, backend, stats_registry, rt_handle, cx).await;
        })
        .detach();
        home.into()
    }

    pub(super) fn chat_route(
        topics: &Topics,
        handles: &Arc<RuntimeHandles>,
        cx: &mut Context<Self>,
    ) -> RoutedContent {
        let palette = cx.palette();
        let viewer_repo = handles.backend.viewer_repo();
        let services = ChatServices {
            rt_handle: handles.rt_handle.clone(),
            action_engine: handles.automation.action_engine.clone(),
            chat_history_repo: handles.backend.chat_history_repo(),
            voice_alias_repo: handles.backend.voice_alias_repo(),
            speak: handles.speech.speak.clone(),
            bot_accounts: handles.automation.bot_accounts.clone(),
        };
        let bus = Arc::clone(&handles.core.bus);
        let builtins = handles.integrations.builtins.clone();
        cx.new(|cx| {
            ChatView::new(
                topics.chat_feed.clone(),
                topics.home_stats.clone(),
                viewer_repo,
                services,
                palette,
                cx,
            )
            .with_follow_lookups(builtins.clone(), cx)
            .with_ban_list(builtins, Arc::clone(&bus))
            .with_lifecycle(topics.integration_lifecycle.clone(), cx)
            .with_chat_bus(bus, cx)
            .with_twitch_bot(topics.twitch_bot.clone(), cx)
        })
        .into()
    }

    pub(super) fn event_feed_route(
        topics: &Topics,
        handles: &Arc<RuntimeHandles>,
        cx: &mut Context<Self>,
    ) -> RoutedContent {
        let rt_handle = handles.rt_handle.clone();
        let bus = Arc::clone(&handles.core.bus);
        cx.new(|cx| EventFeedView::new(topics.event_log.clone(), rt_handle, bus, cx))
            .into()
    }

    pub(super) fn globals_route(
        topics: &Topics,
        handles: &Arc<RuntimeHandles>,
        cx: &mut Context<Self>,
    ) -> RoutedContent {
        let globals = topics.globals.clone();
        let backend: Arc<dyn GlobalsRepo> = Arc::clone(&handles.backend) as Arc<dyn GlobalsRepo>;
        let rt_handle = handles.rt_handle.clone();
        let bus = Arc::clone(&handles.core.bus);
        cx.new(|cx| {
            let mut view = GlobalsView::new(globals, backend, rt_handle, cx);
            view.follow_runtime_writes(bus, cx);
            view
        })
        .into()
    }

    pub(super) fn welcome_route(
        topics: &Topics,
        handles: &Arc<RuntimeHandles>,
        cx: &mut Context<Self>,
    ) -> RoutedContent {
        let lifecycle = topics.integration_lifecycle.clone();
        let connectivity = topics.platforms.clone();
        let launch = Self::hub_launch(handles);
        let hub = cx.new(|cx| {
            IntegrationsHubView::new(None, lifecycle, connectivity, launch, cx).welcome()
        });
        Self::follow_nav(&hub, cx);
        Self::follow_live_detail(&hub, cx);
        hub.into()
    }

    pub(super) fn integrations_route(
        focus: Option<IntegrationCategory>,
        topics: &Topics,
        handles: &Arc<RuntimeHandles>,
        cx: &mut Context<Self>,
    ) -> RoutedContent {
        let launch = Self::hub_launch(handles);
        let lifecycle = topics.integration_lifecycle.clone();
        let connectivity = topics.platforms.clone();
        let hub = cx.new(|cx| IntegrationsHubView::new(focus, lifecycle, connectivity, launch, cx));
        Self::follow_nav(&hub, cx);
        Self::follow_live_detail(&hub, cx);
        hub.into()
    }

    pub(super) fn settings_route(
        preselect: Option<SettingsSection>,
        topics: &Topics,
        handles: &Arc<RuntimeHandles>,
        cx: &mut Context<Self>,
    ) -> RoutedContent {
        let handles = Arc::clone(handles);
        let awake = topics.awake.clone();
        let update_flow = topics.update_flow.clone();
        let problem_report = topics.problem_report.clone();
        cx.new(|cx| SettingsView::new(handles, awake, update_flow, problem_report, preselect, cx))
            .into()
    }

    pub(super) fn queues_route(
        topics: &Topics,
        handles: &Arc<RuntimeHandles>,
        cx: &mut Context<Self>,
    ) -> RoutedContent {
        let queue_health = topics.queue_health.clone();
        let scheduler = handles.automation.scheduler.clone();
        let queue_repo = handles.backend.queue_repo();
        let action_repo = handles.backend.action_repo();
        let rt_handle = handles.rt_handle.clone();
        let scheduled = Self::scheduled_runs_section(handles, cx);
        let view = cx.new(|cx| {
            QueuesView::new(
                queue_health,
                scheduler,
                queue_repo,
                action_repo,
                rt_handle,
                cx,
            )
            .with_scheduled(scheduled, cx)
        });
        Self::routed(view, cx).into()
    }

    pub(super) fn soundboard_route(
        topics: &Topics,
        handles: &Arc<RuntimeHandles>,
        cx: &mut Context<Self>,
    ) -> RoutedContent {
        let player = handles.audio.soundboard_player.clone();
        let settings_repo = Arc::clone(&handles.backend) as Arc<dyn forge_storage::SettingsRepo>;
        let rt_handle = handles.rt_handle.clone();
        let bus = Arc::clone(&handles.core.bus);
        let keys = ClipKeyState::new(
            handles.integrations.hotkey_reconciler.clone(),
            Arc::clone(&handles.backend),
        );
        let switch = Self::integration_switch(topics, handles);
        let view = cx.new(|cx| {
            SoundboardView::new(player, settings_repo, rt_handle, bus, keys, cx)
                .with_integration_switch(switch, cx)
        });
        Self::routed(view, cx).into()
    }

    pub(super) fn tts_route(
        preselect: Option<TtsSection>,
        topics: &Topics,
        handles: &Arc<RuntimeHandles>,
        cx: &mut Context<Self>,
    ) -> RoutedContent {
        let speak_state = topics.speak.clone();
        let speak = handles.speech.speak.clone();
        let backend = Arc::clone(&handles.backend);
        let rt_handle = handles.rt_handle.clone();
        let pipeline_config = handles.speech.pipeline_config.clone();
        let bot_accounts = handles.automation.bot_accounts.clone();
        let cloud_tts_gateways = handles.speech.cloud_tts_gateways.clone();
        let view = cx.new(|cx| {
            TtsView::new(
                speak_state,
                speak,
                backend,
                rt_handle,
                pipeline_config,
                bot_accounts,
                cloud_tts_gateways,
                preselect,
                cx,
            )
        });
        Self::routed(view, cx).into()
    }

    pub(super) fn overlays_route(
        handles: &Arc<RuntimeHandles>,
        cx: &mut Context<Self>,
    ) -> RoutedContent {
        let launch = OverlaysLaunch {
            repo: handles.backend.overlay_repo(),
            server: handles.server.clone(),
            rt_handle: handles.rt_handle.clone(),
            kinds: Arc::clone(&handles.overlays.kinds),
            service: handles.overlays.service.clone(),
            samples: Arc::clone(&handles.overlays.samples),
            date_names: handles.date_names.clone(),
            library: Arc::clone(handles.audio.soundboard_player.library()),
            media: handles.backend.media_repo(),
            settings_repo: Arc::clone(&handles.backend) as Arc<dyn SettingsRepo>,
            audio_router: Arc::clone(&handles.audio.audio_router),
            speak: handles.speech.speak.clone(),
            actions: Arc::clone(&handles.overlays.actions),
            triggers: handles.automation.trigger_registry.clone(),
            sub_actions: handles.automation.sub_action_registry.clone(),
            scheduler: handles.automation.scheduler.clone(),
            latest_values: handles.core.latest_values.clone(),
            bus: Arc::clone(&handles.core.bus),
        };
        let view = cx.new(|cx| OverlaysView::new(launch, cx));
        Self::guarded(view.clone(), cx).with_shortcuts(&view)
    }

    pub(super) fn server_route(
        handles: &Arc<RuntimeHandles>,
        cx: &mut Context<Self>,
    ) -> RoutedContent {
        let server = handles.server.clone();
        let rt_handle = handles.rt_handle.clone();
        let credentials: Arc<dyn CredentialsRepo> =
            Arc::clone(&handles.backend) as Arc<dyn CredentialsRepo>;
        let settings: Arc<dyn SettingsRepo> = Arc::clone(&handles.backend) as Arc<dyn SettingsRepo>;
        let console =
            cx.new(|cx| ServerConsoleView::new(server, rt_handle, credentials, settings, cx));
        Self::follow_nav(&console, cx);
        console.into()
    }

    pub(super) fn actions_route(
        preselect: Option<ActionId>,
        topics: &Topics,
        handles: &Arc<RuntimeHandles>,
        cx: &mut Context<Self>,
    ) -> RoutedContent {
        let action_repo = handles.backend.action_repo();
        let queue_repo = handles.backend.queue_repo();
        let actions_service = Arc::new(forge_runtime::ActionsService::new(
            handles.backend.action_repo(),
            handles.backend.queue_repo(),
            handles.backend.history_repo(),
            handles.backend.trigger_instance_repo(),
        ));
        let trigger_instance_repo = HotkeySyncedTriggerRepo::wrap(
            handles.backend.trigger_instance_repo(),
            handles.integrations.hotkey_reconciler.clone(),
        );
        let script_repo = Arc::clone(&handles.backend) as Arc<dyn ScriptRepo>;
        let clip_library = Arc::clone(handles.audio.soundboard_player.library());
        let globals_repo = Arc::clone(&handles.backend) as Arc<dyn GlobalsRepo>;
        let settings_repo = Arc::clone(&handles.backend) as Arc<dyn SettingsRepo>;
        let overlay_repo = handles.backend.overlay_repo();
        let overlay_kinds = Arc::clone(&handles.overlays.kinds);
        let speak = handles.speech.speak.clone();
        let sub_action_registry = handles.automation.sub_action_registry.clone();
        let trigger_registry = handles.automation.trigger_registry.clone();
        let rt_handle = handles.rt_handle.clone();
        let bus = Arc::clone(&handles.core.bus);
        let scheduler = handles.automation.scheduler.clone();
        let builtins = handles.integrations.builtins.clone();
        let scheduled_run_repo = handles.backend.scheduled_run_repo();
        let switch = Self::integration_switch(topics, handles);
        let view = cx.new(|cx| {
            ScreenActionsView::new(
                crate::actions_screen::ActionsDeps {
                    action_repo,
                    queue_repo,
                    actions_service,
                    trigger_instance_repo,
                    script_repo,
                    clip_library,
                    globals_repo,
                    overlay_repo,
                    speak,
                    sub_action_registry,
                    trigger_registry,
                    rt_handle,
                    bus,
                    scheduler,
                },
                settings_repo,
                overlay_kinds,
                preselect,
                cx,
            )
            .with_builtins(builtins, cx)
            .with_scheduled_runs(scheduled_run_repo, cx)
            .with_pack_export(cx)
            .with_integration_switch(switch, cx)
        });
        Self::follow_nav(&view, cx);
        RoutedContent::from(view.clone()).with_shortcuts(&view)
    }

    pub(super) fn triggers_route(
        preselect: Option<TriggerInstanceId>,
        topics: &Topics,
        handles: &Arc<RuntimeHandles>,
        cx: &mut Context<Self>,
    ) -> RoutedContent {
        let repo = HotkeySyncedTriggerRepo::wrap(
            handles.backend.trigger_instance_repo(),
            handles.integrations.hotkey_reconciler.clone(),
        );
        let action_repo = handles.backend.action_repo();
        let registry = handles.automation.trigger_registry.clone();
        let settings_repo = Arc::clone(&handles.backend) as Arc<dyn SettingsRepo>;
        let rt_handle = handles.rt_handle.clone();
        let builtins = handles.integrations.builtins.clone();
        let switch = Self::integration_switch(topics, handles);
        let view = cx.new(|cx| {
            TriggersRegistryView::new(
                repo,
                action_repo,
                registry,
                settings_repo,
                rt_handle,
                preselect,
                cx,
            )
            .with_builtins(builtins, cx)
            .with_integration_switch(switch, cx)
        });
        Self::follow_nav(&view, cx);
        RoutedContent::from(view.clone()).with_shortcuts(&view)
    }

    pub(super) fn scripts_route(
        handles: &Arc<RuntimeHandles>,
        cx: &mut Context<Self>,
    ) -> RoutedContent {
        let backend = handles.backend.clone();
        let script_registry = handles.automation.script_registry.clone();
        let bus = handles.core.bus.clone();
        let integrations: Arc<dyn IntegrationAvailability> =
            Arc::new(handles.automation.action_engine.integration_gate().clone());
        let rt_handle = handles.rt_handle.clone();
        let editor = cx.new(|cx| {
            ScriptEditorView::new(backend, script_registry, bus, integrations, rt_handle, cx)
        });
        Self::guarded(editor.clone(), cx).with_shortcuts(&editor)
    }
}
