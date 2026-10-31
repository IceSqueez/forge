use super::{MappingRow, MidiScreenView};
use crate::async_bridge::{self, ErrorSink};
use crate::repo_failure::RepoFailure;
use forge_components::tr;
use forge_runtime::LifecycleState;
use gpui::Context;

pub(super) fn resolve_selection(known: &[String], current: Option<&str>) -> Option<String> {
    match current {
        Some(name) if known.iter().any(|k| k == name) => Some(name.to_owned()),
        _ => known.first().cloned(),
    }
}

pub(super) fn follow_device(
    known: &[String],
    current: Option<&str>,
    saved: Option<&str>,
) -> Option<String> {
    let saved = saved?;
    if current == Some(saved) {
        return None;
    }
    known.iter().any(|k| k == saved).then(|| saved.to_owned())
}

pub(super) fn visible_mappings<'a>(
    rows: &'a [MappingRow],
    selected: Option<&str>,
) -> Vec<&'a MappingRow> {
    let Some(selected) = selected else {
        return rows.iter().collect();
    };
    rows.iter()
        .filter(|row| {
            row.signal
                .device
                .as_deref()
                .is_none_or(|device| device == selected)
        })
        .collect()
}

impl MidiScreenView {
    pub(super) fn select_device(&mut self, name: String, cx: &mut Context<Self>) {
        if self.selected_device.as_deref() == Some(name.as_str()) {
            return;
        }
        self.selected_device = Some(name);
        cx.notify();
    }

    pub(super) fn ensure_device_selection(&mut self) {
        self.selected_device = resolve_selection(self.known(), self.selected_device.as_deref());
    }

    pub(super) fn follow_saved_device(&mut self, device: Option<&str>) {
        if let Some(next) = follow_device(self.known(), self.selected_device.as_deref(), device) {
            self.selected_device = Some(next);
        }
    }

    pub(super) fn visible_mappings(&self) -> Vec<&MappingRow> {
        visible_mappings(&self.mappings, self.selected_device.as_deref())
    }

    pub(super) fn toggle_enabled(&mut self, cx: &mut Context<Self>) {
        let previous = self.enabled;
        self.enabled = !previous;
        let enabled = self.enabled;
        let integration = self.integration.clone();
        async_bridge::optimistic(
            &self.rt_handle,
            previous,
            async move {
                let integration =
                    integration.ok_or_else(|| RepoFailure::localized(tr!("midi_toggle_failed")))?;
                match integration
                    .set_enabled(enabled)
                    .await
                    .map_err(RepoFailure::from)?
                {
                    LifecycleState::Failed(error) => Err(RepoFailure::operation(error)),
                    _ => Ok(()),
                }
            },
            |this, previous, _message, cx| {
                this.enabled = previous;
                ErrorSink::Toast.report(tr!("midi_toggle_failed"), cx);
            },
            cx,
        );
        cx.notify();
    }

    pub(super) fn rescan(&mut self, cx: &mut Context<Self>) {
        if self.known_devices.failure().is_some() {
            self.load_known_devices(cx);
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        async_bridge::run_async(
            &self.rt_handle,
            async move { client.rescan_ports().await.map_err(RepoFailure::operation) },
            |this, result: Result<(), RepoFailure>, cx| match result {
                Ok(()) => this.refresh_ports(cx),
                Err(failure) => this.on_repo_error(&failure, cx),
            },
            cx,
        );
    }

    pub(super) fn maps_for_device(&self, port: &str) -> usize {
        self.mappings
            .iter()
            .filter(|row| row.signal.device.as_deref() == Some(port))
            .count()
    }
}
