use std::sync::Arc;

use forge_audio::list_output_devices;
use forge_components::{
    BORDER_THIN, Density, FONT_XS, ForgePalette, Icon, Radius, Spacing, body_family, dropdown,
    dropdown_list, dropdown_row, icon, radius, slider, spacing, toggle, tr,
};
use forge_storage::{
    set_soundboard_also_headphones, set_soundboard_master_volume, set_soundboard_output_device,
};
use gpui::{AnyElement, ClickEvent, Context, Pixels, SharedString, div, prelude::*, px};

use super::labels::{field_lite_label, section_label};
use super::{HOTKEY_FS, LABEL_FS, SoundboardView};
use crate::async_bridge::{self, ErrorSink};

pub(super) const ROUTING_PAD: Pixels = px(14.0);
const ROUTING_GAP: Pixels = px(16.0);
const SELECT_RADIUS: Pixels = px(7.0);
const SELECT_PAD_Y: Pixels = px(8.0);
const SELECT_PAD_X: Pixels = px(11.0);
const HINT_FS: Pixels = px(10.5);

impl SoundboardView {
    pub(super) fn reload_devices(&self, cx: &mut Context<Self>) {
        async_bridge::run_blocking(
            &self.rt_handle,
            || list_output_devices().map_err(|e| e.to_string()),
            |this, result: Result<Vec<_>, String>, cx| {
                if let Ok(devices) = result {
                    this.devices = devices;
                    cx.notify();
                }
            },
            cx,
        );
    }

    fn toggle_headphones(&mut self, cx: &mut Context<Self>) {
        self.settings = self
            .player
            .update_settings(|settings| settings.also_headphones = !settings.also_headphones);
        let repo = Arc::clone(&self.settings_repo);
        let value = self.settings.also_headphones;
        async_bridge::report_failure(
            &self.rt_handle,
            async move { set_soundboard_also_headphones(repo.as_ref(), value).await },
            ErrorSink::Toast,
            tr!("soundboard_persist_failed"),
            cx,
        );
        cx.notify();
    }

    fn set_master_volume(&mut self, fraction: f32, cx: &mut Context<Self>) {
        let value = (fraction / 100.0).clamp(0.0, 1.0);
        self.settings = self
            .player
            .update_settings(|settings| settings.master_volume = value);
        let repo = Arc::clone(&self.settings_repo);
        self.master_volume_debounce.schedule_reporting(
            &self.rt_handle,
            tr!("soundboard_persist_failed"),
            async move { set_soundboard_master_volume(repo.as_ref(), value).await },
            ErrorSink::Toast,
            cx,
        );
        cx.notify();
    }

    fn set_output_device(&mut self, device_id: Option<String>, cx: &mut Context<Self>) {
        let chosen = device_id.clone();
        self.settings = self
            .player
            .update_settings(|settings| settings.output_device_id = chosen);
        self.device_menu_open = false;
        let repo = Arc::clone(&self.settings_repo);
        async_bridge::report_failure(
            &self.rt_handle,
            async move { set_soundboard_output_device(repo.as_ref(), device_id).await },
            ErrorSink::Toast,
            tr!("soundboard_persist_failed"),
            cx,
        );
        cx.notify();
    }

    fn toggle_device_menu(&mut self, cx: &mut Context<Self>) {
        self.device_menu_open = !self.device_menu_open;
        cx.notify();
    }

    fn close_device_menu(&mut self, cx: &mut Context<Self>) {
        self.device_menu_open = false;
        cx.notify();
    }

    pub(super) fn device_short_label(&self) -> String {
        match &self.settings.output_device_id {
            Some(id) => self
                .devices
                .iter()
                .find(|d| &d.id.0 == id)
                .map(|d| d.name.clone())
                .unwrap_or_else(|| tr!("soundboard_device_system_default").to_string()),
            None => tr!("soundboard_device_system_default").to_string(),
        }
    }

    pub(super) fn output_ready(&self) -> bool {
        match &self.settings.output_device_id {
            Some(id) => self.devices.iter().any(|d| &d.id.0 == id),
            None => true,
        }
    }

    pub(super) fn render_routing(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let device_label = self.device_short_label();
        let mut device_col = div()
            .flex_1()
            .flex()
            .flex_col()
            .child(field_lite_label(tr!("soundboard_routing_device"), palette))
            .child(self.render_device_select(&device_label, palette, cx));
        device_col = device_col.child(
            div()
                .mt(px(5.0))
                .font_family(body_family())
                .text_size(HINT_FS)
                .text_color(palette.text_faint)
                .child(tr!("soundboard_routing_hint")),
        );

        let pct = (self.settings.master_volume * 100.0).round() as i64;
        let volume_col = div()
            .flex_1()
            .flex()
            .flex_col()
            .child(field_lite_label(
                tr!("soundboard_routing_volume", pct = pct),
                palette,
            ))
            .child(
                slider(self.settings.master_volume * 100.0, 0.0, 100.0, palette)
                    .accent(palette.bits)
                    .on_change(
                        "sb-master-volume",
                        cx.listener(|this, value: &f32, _, cx| this.set_master_volume(*value, cx)),
                    ),
            )
            .child(
                div()
                    .mt(spacing(Spacing::Xs, density))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        toggle(self.settings.also_headphones, palette)
                            .on_color(palette.bits)
                            .on_click(
                                "sb-headphones",
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.toggle_headphones(cx)
                                }),
                            ),
                    )
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(LABEL_FS)
                            .text_color(palette.text_secondary)
                            .child(tr!("soundboard_routing_headphones")),
                    ),
            );

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .p(ROUTING_PAD)
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.elevated)
            .child(section_label(tr!("soundboard_routing_section"), palette))
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap(ROUTING_GAP)
                    .child(device_col)
                    .child(volume_col),
            )
            .into_any_element()
    }

    fn render_device_select(
        &self,
        current_label: &str,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let trigger = div()
            .id("sb-device-trigger")
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .py(SELECT_PAD_Y)
            .px(SELECT_PAD_X)
            .rounded(SELECT_RADIUS)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.shell)
            .cursor_pointer()
            .hover(|s| s.border_color(palette.border_active))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_device_menu(cx)))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(current_label.to_owned()),
            )
            .child(icon(Icon::ChevronDown, HOTKEY_FS, palette.text_faint));

        let mut field = div().relative().w_full().child(trigger);
        if self.device_menu_open {
            let mut list = dropdown_list("sb-device-list", palette).child(self.device_option(
                "sb-dev-default",
                tr!("soundboard_device_system_default"),
                self.settings.output_device_id.is_none(),
                None,
                palette,
                cx,
            ));
            for (idx, device) in self.devices.iter().enumerate() {
                let selected =
                    self.settings.output_device_id.as_deref() == Some(device.id.0.as_str());
                let id = device.id.0.clone();
                list = list.child(self.device_option(
                    ("sb-dev", idx),
                    device.name.clone(),
                    selected,
                    Some(id),
                    palette,
                    cx,
                ));
            }
            let view = cx.entity();
            field = field.child(dropdown(list).on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_device_menu(cx));
            }));
        }
        field.into_any_element()
    }

    fn device_option(
        &self,
        id: impl Into<gpui::ElementId>,
        label: impl Into<SharedString>,
        selected: bool,
        value: Option<String>,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        dropdown_row(id, label, palette)
            .current(selected)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.set_output_device(value.clone(), cx)
            }))
            .into_any_element()
    }
}
