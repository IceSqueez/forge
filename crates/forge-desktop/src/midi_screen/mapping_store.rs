use super::{MappingRow, MidiScreenView};
use crate::async_bridge::{self, Loadable, WriteOutcome};
use crate::midi_mapping_modal::MappingDraft;
use crate::midi_signal::MidiSignal;
use crate::repo_failure::RepoFailure;
use forge_components::{PushToast, ToastKind, tr};
use forge_midi::vocabulary::event_kinds::INPUT_PREFIX;
use forge_storage::{
    ActionRepo, JsonDocument, Setting, StorageError, TriggerInstanceRepo, read_stored_setting,
    write_setting,
};
use forge_types::{PermissionRung, PlatformScope, TriggerInstance, TriggerInstanceId};
use gpui::Context;
use std::sync::Arc;

pub const MIDI_KNOWN_DEVICES: Setting<JsonDocument<Vec<String>>> = Setting::new(
    "midi.known_devices",
    JsonDocument::new(),
    forge_storage::SettingPortability::MachineLocal,
    forge_storage::SettingDisclosure::Withheld,
);

pub(super) async fn load_mappings(
    triggers: &dyn TriggerInstanceRepo,
    actions: &dyn ActionRepo,
) -> Result<Vec<MappingRow>, RepoFailure> {
    let instances = triggers.list_all().await.map_err(RepoFailure::from)?;
    let mut rows = Vec::new();
    for instance in instances {
        if !instance.kind_id.starts_with(INPUT_PREFIX) {
            continue;
        }
        let linked = triggers
            .actions_using(instance.id)
            .await
            .map_err(RepoFailure::from)?;
        if !instance.user_defined && linked.is_empty() {
            continue;
        }
        let mut action = None;
        if let Some(first) = linked.first()
            && let Some(found) = actions.get(*first).await.map_err(RepoFailure::from)?
        {
            action = Some((found.id, found.name));
        }
        rows.push(MappingRow {
            signal: MidiSignal::from_instance(&instance.kind_id, &instance.overrides),
            id: instance.id,
            name: instance.name,
            action,
        });
    }
    Ok(rows)
}

pub(super) async fn save_mapping(
    triggers: &dyn TriggerInstanceRepo,
    actions: &dyn ActionRepo,
    draft: MappingDraft,
) -> Result<Vec<MappingRow>, RepoFailure> {
    let overrides = draft.signal.overrides();
    let instance_id = match draft.instance_id {
        Some(id) => {
            let source = triggers
                .get(id)
                .await
                .map_err(RepoFailure::from)?
                .ok_or_else(RepoFailure::trigger_gone)?;
            let updated = TriggerInstance {
                kind_id: draft.signal.kind_id.clone(),
                name: draft.name.clone(),
                overrides,
                ..source
            };
            triggers.save(&updated).await.map_err(RepoFailure::from)?;
            id
        }
        None => {
            let instance = TriggerInstance {
                id: TriggerInstanceId::new(),
                kind_id: draft.signal.kind_id.clone(),
                name: draft.name,
                overrides,
                enabled: true,
                user_defined: true,
                platform_scope: PlatformScope::Any,
                cooldown_secs: 0,
                cooldown_global: true,
                permission_rung: PermissionRung::Everyone,
            };
            triggers.save(&instance).await.map_err(RepoFailure::from)?;
            instance.id
        }
    };

    let linked = triggers
        .actions_using(instance_id)
        .await
        .map_err(RepoFailure::from)?;
    if !linked.contains(&draft.action_id) {
        for previous in linked {
            triggers
                .unlink_action(previous, instance_id)
                .await
                .map_err(RepoFailure::from)?;
        }
        triggers
            .link_action(draft.action_id, instance_id)
            .await
            .map_err(RepoFailure::from)?;
    }

    load_mappings(triggers, actions).await
}

impl MidiScreenView {
    pub(super) fn load(&mut self, cx: &mut Context<Self>) {
        self.load_known_devices(cx);
        let triggers = Arc::clone(&self.trigger_repo);
        let actions = Arc::clone(&self.action_repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move { load_mappings(&*triggers, &*actions).await },
            |this, mappings, cx| this.apply_mappings(mappings, cx),
            cx,
        );
    }

    pub(super) fn load_known_devices(&mut self, cx: &mut Context<Self>) {
        self.known_devices = Loadable::Loading;
        let settings = Arc::clone(&self.settings_repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                read_stored_setting(settings.as_ref(), &MIDI_KNOWN_DEVICES)
                    .await
                    .map(Option::unwrap_or_default)
            },
            |this, result: Result<Vec<String>, StorageError>, cx| {
                if let Err(error) = &result {
                    tracing::warn!(%error, "failed to load known MIDI devices");
                }
                this.known_devices = Loadable::settled(result);
                this.remember_live_ports(cx);
                this.ensure_device_selection();
                cx.notify();
            },
            cx,
        );
        cx.notify();
    }

    pub(super) fn known(&self) -> &[String] {
        self.known_devices.loaded().map_or(&[], Vec::as_slice)
    }

    fn device_write_settled(&mut self, outcome: WriteOutcome, _: &mut Context<Self>) {
        if let WriteOutcome::Failed(error) = outcome {
            self.known_devices = Loadable::Failed(error);
        }
    }

    pub(super) fn apply_mappings(
        &mut self,
        result: Result<Vec<MappingRow>, RepoFailure>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(rows) => {
                self.mappings = rows;
                cx.notify();
            }
            Err(failure) => self.on_repo_error(&failure, cx),
        }
    }

    pub(super) fn on_repo_error(&mut self, failure: &RepoFailure, cx: &mut Context<Self>) {
        tracing::warn!(error = %failure, "midi operation failed");
        let message = failure.text();
        cx.push_toast(
            ToastKind::Error,
            tr!("midi_toast_error", message = message.as_str()),
        );
        cx.notify();
    }

    pub(super) fn refresh_ports(&mut self, cx: &mut Context<Self>) {
        let ports = self
            .client
            .as_ref()
            .map(|client| client.connected_input_ports())
            .unwrap_or_default();
        if ports == self.live_ports {
            return;
        }
        self.live_ports = ports;
        self.remember_live_ports(cx);
        cx.notify();
    }

    pub(super) fn remember_live_ports(&mut self, cx: &mut Context<Self>) {
        let Some(known) = self.known_devices.loaded_mut() else {
            return;
        };
        let discovered: Vec<String> = self
            .live_ports
            .iter()
            .filter(|port| !known.contains(port))
            .cloned()
            .collect();
        if discovered.is_empty() {
            return;
        }
        known.extend(discovered);
        known.sort();
        let devices = known.clone();
        self.ensure_device_selection();
        let repo = Arc::clone(&self.settings_repo);
        self.device_writer.write(
            async move { write_setting(repo.as_ref(), &MIDI_KNOWN_DEVICES, &devices).await },
            Self::device_write_settled,
            cx,
        );
    }
}
