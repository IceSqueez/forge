use forge_audio::{
    DeviceId, DeviceInfo, list_input_devices, list_output_devices, refresh_input_devices,
    refresh_output_devices,
};
use forge_components::{ForgePalette, dropdown, dropdown_list, dropdown_row, tr};
use gpui::{AnyElement, ClickEvent, Context, ElementId, FocusHandle, prelude::*};

use crate::shared::repo_failure::RepoFailure;

#[derive(Clone, Copy)]
pub(crate) enum DeviceDirection {
    Output,
    Input,
}

pub(crate) fn device_label(device: &DeviceInfo) -> String {
    if device.is_default {
        tr!("widget_device_default_label", name = device.name.as_str())
    } else {
        device.name.clone()
    }
}

pub(crate) fn selected_device_label(devices: &[DeviceInfo], index: usize) -> String {
    devices
        .get(index)
        .map(device_label)
        .unwrap_or_else(|| "-".to_string())
}

pub(crate) fn device_index(devices: &[DeviceInfo], chosen: Option<&DeviceId>) -> usize {
    chosen
        .and_then(|id| devices.iter().position(|device| device.id == *id))
        .unwrap_or(0)
}

pub(crate) fn device_id_at(devices: &[DeviceInfo], index: usize) -> Option<String> {
    devices
        .get(index)
        .map(|device| device.id.as_str().to_owned())
}

pub(crate) async fn enumerate_devices(
    direction: DeviceDirection,
    uncached: bool,
) -> Result<Vec<DeviceInfo>, RepoFailure> {
    tokio::task::spawn_blocking(move || {
        match (direction, uncached) {
            (DeviceDirection::Output, true) => refresh_output_devices(),
            (DeviceDirection::Output, false) => list_output_devices(),
            (DeviceDirection::Input, true) => refresh_input_devices(),
            (DeviceDirection::Input, false) => list_input_devices(),
        }
        .map_err(RepoFailure::from)
    })
    .await
    .map_err(RepoFailure::from)?
}

pub(crate) struct DevicePickerIds {
    pub(crate) list: &'static str,
    pub(crate) row: &'static str,
}

pub(crate) struct DevicePickerHandlers<V: 'static> {
    pub(crate) select: fn(&mut V, usize, &mut Context<V>),
    pub(crate) close: fn(&mut V, &mut Context<V>),
}

pub(crate) fn device_picker_overlay<V: 'static>(
    ids: DevicePickerIds,
    devices: &[DeviceInfo],
    selected_index: usize,
    handlers: DevicePickerHandlers<V>,
    overlay_focus: &FocusHandle,
    palette: &ForgePalette,
    cx: &mut Context<V>,
) -> AnyElement {
    let DevicePickerHandlers { select, close } = handlers;
    let mut panel = dropdown_list(ids.list, palette);
    for (index, device) in devices.iter().enumerate() {
        panel = panel.child(
            dropdown_row(
                ElementId::from((ids.row, index)),
                device_label(device),
                palette,
            )
            .current(index == selected_index)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| select(this, index, cx))),
        );
    }

    let view = cx.entity();
    dropdown(panel)
        .dismiss_on_escape(overlay_focus)
        .on_dismiss(move |_window, cx| view.update(cx, close))
        .into_any_element()
}
