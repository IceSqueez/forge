use super::*;

impl AppShell {
    pub(super) fn builtin_detail_screen(
        id: &IntegrationId,
        topics: &Topics,
        handles: &Arc<RuntimeHandles>,
        cx: &mut Context<Self>,
    ) -> AnyView {
        let builtin = match id.as_str() {
            forge_obs::vocabulary::INTEGRATION_ID => {
                match handles.integrations.obs_install_seed.live() {
                    Some(client) => Some(client.builtin_object()),
                    None => return Self::obs_connect_screen(handles, cx),
                }
            }
            forge_vtube::vocabulary::INTEGRATION_ID => {
                match handles.integrations.vtube_install_seed.live() {
                    Some(client) => Some(client.builtin_object()),
                    None => return Self::vtube_connect_screen(handles, cx),
                }
            }
            forge_midi::vocabulary::INTEGRATION_ID => return Self::midi_screen(handles, cx),
            forge_hotkey::vocabulary::INTEGRATION_ID => {
                return Self::hotkeys_screen(
                    handles,
                    handles.integrations.hotkey_reconciler.clone(),
                    cx,
                );
            }
            forge_discord::vocabulary::INTEGRATION_ID => {
                match handles.integrations.discord_client.clone() {
                    Some(client) => return Self::discord_screen(handles, client, cx),
                    None => handles.integrations.builtins.get(id),
                }
            }
            _ => handles.integrations.builtins.get(id),
        };
        let object = builtin.unwrap_or_else(|| unavailable_builtin(id));

        let services = Self::detail_services(handles);
        let detail = cx.new(|cx| IntegrationDetail::new(object, services, cx));
        Self::attach_detail_extras(&detail, id, topics, handles, cx);
        Self::follow_nav(&detail, cx);
        cx.subscribe(&detail, |this, _view, _: &ObsConnected, cx| {
            this.enable_integration(&forge_obs::OBS_INTEGRATION.id);
            this.rebuild_current(cx);
        })
        .detach();
        cx.subscribe(&detail, |this, _view, _: &IntegrationSignedOut, cx| {
            this.rebuild_current(cx);
        })
        .detach();
        detail.into()
    }

    fn detail_services(handles: &Arc<RuntimeHandles>) -> DetailServices {
        let credentials = handles.backend.credentials_repo();
        let settings = handles.backend.settings_repo();
        let history = handles.backend.history_repo();
        let trigger_registry = handles.automation.trigger_registry.clone();
        let bus = Arc::clone(&handles.core.bus) as Arc<dyn EventPublisher>;
        let rt_handle = handles.rt_handle.clone();
        let action_engine = handles.automation.action_engine.clone();
        let twitch_slot = handles
            .integrations
            .supervisor
            .slot(&forge_platform_twitch::TWITCH_INTEGRATION.id);
        let kick_slot = handles
            .integrations
            .supervisor
            .slot(&forge_platform_kick::KICK_INTEGRATION.id);
        let youtube_slot = handles
            .integrations
            .supervisor
            .slot(&forge_platform_youtube::YOUTUBE_INTEGRATION.id);
        let obs_install_seed = handles.integrations.obs_install_seed.clone();
        let sign_out = handles.integrations.sign_out.clone();
        DetailServices {
            rt_handle,
            action_engine,
            credentials,
            settings,
            history,
            trigger_registry,
            bus,
            twitch_slot,
            kick_slot,
            youtube_slot,
            twitch_sign_in: handles.integrations.twitch_sign_in.clone(),
            youtube_sign_in: handles.integrations.youtube_sign_in.clone(),
            kick_sign_in: handles.integrations.kick_sign_in.clone(),
            obs_install_seed,
            sign_out,
        }
    }

    fn attach_detail_extras(
        detail: &Entity<IntegrationDetail>,
        id: &IntegrationId,
        topics: &Topics,
        handles: &Arc<RuntimeHandles>,
        cx: &mut Context<Self>,
    ) {
        if let Some(service) = handles.integrations.donation_services.service(id) {
            detail.update(cx, |detail, cx| {
                detail.attach_donation_settings(service, cx)
            });
        }
        if let Some(bot) = handles.integrations.twitch_bot.clone() {
            let presence = topics.twitch_bot.clone();
            detail.update(cx, |detail, cx| detail.attach_twitch_bot(bot, presence, cx));
        }
    }

    pub(super) fn failed_screen(
        id: &IntegrationId,
        topics: &Topics,
        handles: &Arc<RuntimeHandles>,
        cx: &mut Context<Self>,
    ) -> AnyView {
        let lifecycle = topics.integration_lifecycle.clone();
        let slot = handles.integrations.supervisor.slot(id);
        let rt_handle = handles.rt_handle.clone();
        let id = id.clone();
        let view =
            cx.new(|cx| IntegrationFailedView::new(id.clone(), lifecycle, slot, rt_handle, cx));
        Self::follow_nav(&view, cx);
        Self::follow_live_detail(&view, cx);
        view.into()
    }

    pub(super) fn disabled_screen(
        id: &IntegrationId,
        topics: &Topics,
        handles: &Arc<RuntimeHandles>,
        cx: &mut Context<Self>,
    ) -> AnyView {
        let launch = DisabledLaunch {
            slot: handles.integrations.supervisor.slot(id),
            backend: Arc::clone(&handles.backend),
            sub_actions: Arc::clone(&handles.automation.sub_action_registry),
            triggers: Arc::clone(&handles.automation.trigger_registry),
            rt_handle: handles.rt_handle.clone(),
        };
        let lifecycle = topics.integration_lifecycle.clone();
        let id = id.clone();
        let view = cx.new(|cx| IntegrationDisabledView::new(id, lifecycle, launch, cx));
        Self::follow_nav(&view, cx);
        view.into()
    }

    pub(super) fn obs_connect_screen(
        handles: &Arc<RuntimeHandles>,
        cx: &mut Context<Self>,
    ) -> AnyView {
        let credentials = handles.backend.credentials_repo();
        let settings = handles.backend.settings_repo();
        let bus = Arc::clone(&handles.core.bus) as Arc<dyn EventPublisher>;
        let rt_handle = handles.rt_handle.clone();
        let seed = handles.integrations.obs_install_seed.clone();
        let connect =
            cx.new(|cx| ObsConnectView::new(rt_handle, credentials, settings, bus, seed, cx));
        Self::follow_nav(&connect, cx);
        cx.subscribe(&connect, |this, _view, _: &ObsConnected, cx| {
            this.enable_integration(&forge_obs::OBS_INTEGRATION.id);
            this.rebuild_current(cx);
        })
        .detach();
        connect.into()
    }

    pub(super) fn hotkeys_screen(
        handles: &Arc<RuntimeHandles>,
        reconciler: Arc<HotkeyReconciler>,
        cx: &mut Context<Self>,
    ) -> AnyView {
        let backend = Arc::clone(&handles.backend);
        let settings = handles.backend.settings_repo();
        let bus = Arc::clone(&handles.core.bus);
        let rt_handle = handles.rt_handle.clone();
        let engine = handles
            .integrations
            .supervisor
            .slot(&forge_hotkey::HOTKEY_INTEGRATION.id);
        let view = cx.new(|cx| {
            HotkeysScreenView::new(reconciler, engine, backend, settings, bus, rt_handle, cx)
        });
        Self::routed(view, cx)
    }

    pub(super) fn discord_screen(
        handles: &Arc<RuntimeHandles>,
        client: Arc<forge_discord::DiscordClient>,
        cx: &mut Context<Self>,
    ) -> AnyView {
        let action_repo = handles.backend.action_repo();
        let bus = Arc::clone(&handles.core.bus);
        let rt_handle = handles.rt_handle.clone();
        let integration = handles
            .integrations
            .supervisor
            .slot(&forge_discord::DISCORD_INTEGRATION.id);
        let view = cx
            .new(|cx| DiscordScreenView::new(client, integration, action_repo, bus, rt_handle, cx));
        Self::routed(view, cx)
    }

    pub(super) fn midi_screen(handles: &Arc<RuntimeHandles>, cx: &mut Context<Self>) -> AnyView {
        let sink = Arc::clone(&handles.integrations.midi_sink);
        let integration = handles
            .integrations
            .supervisor
            .slot(&forge_midi::MIDI_INTEGRATION.id);
        let trigger_repo = handles.backend.trigger_instance_repo();
        let action_repo = handles.backend.action_repo();
        let settings = handles.backend.settings_repo();
        let bus = Arc::clone(&handles.core.bus);
        let rt_handle = handles.rt_handle.clone();
        let view = cx.new(|cx| {
            MidiScreenView::new(
                sink,
                integration,
                trigger_repo,
                action_repo,
                settings,
                bus,
                rt_handle,
                cx,
            )
        });
        Self::routed(view, cx)
    }

    pub(super) fn vtube_connect_screen(
        handles: &Arc<RuntimeHandles>,
        cx: &mut Context<Self>,
    ) -> AnyView {
        let credentials = handles.backend.credentials_repo();
        let settings = handles.backend.settings_repo();
        let bus = Arc::clone(&handles.core.bus) as Arc<dyn EventPublisher>;
        let rt_handle = handles.rt_handle.clone();
        let seed = handles.integrations.vtube_install_seed.clone();
        let connect =
            cx.new(|cx| VTubeConnectView::new(rt_handle, credentials, settings, bus, seed, cx));
        Self::follow_nav(&connect, cx);
        cx.subscribe(&connect, |this, _view, _: &VTubeConnected, cx| {
            this.enable_integration(&forge_vtube::VTUBE_INTEGRATION.id);
            this.rebuild_current(cx);
        })
        .detach();
        connect.into()
    }
}
