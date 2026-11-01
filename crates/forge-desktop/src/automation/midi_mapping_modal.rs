use forge_components::FONT_META;
use forge_components::FONT_MICRO;
use forge_components::FONT_ROW;
use forge_components::Spacing;
use forge_components::focus_inside_modal;
use forge_components::spacing;
use std::sync::Arc;

use forge_components::OverflowRevealingFocus;
use forge_components::{
    ActivePresentation, BORDER_THIN, Density, FONT_SECTION_LABEL, ForgePalette, Icon, InputEvent,
    KeyActivated, OverlayPosition, Radius, TextInput, dropdown, dropdown_list, dropdown_row,
    field_label, ghost_button_with_icon, grow_cell, icon, modal, overlay, primary_button, scaled,
    secondary_button, tr,
};
use forge_storage::ActionRepo;
use forge_types::{ActionId, TriggerInstanceId};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, EventEmitter, Pixels, SharedString, Subscription,
    Window, div, prelude::*, px,
};

use crate::automation::action_picker::{ActionPicker, ActionPickerCopy, ActionPickerEvent};
use crate::automation::midi_signal::{
    MIDI_CHANNELS, MidiSignal, kind_label, selector_key, shown_channel, stored_channel,
};
use forge_midi::vocabulary::event_kinds::SIGNAL_KINDS;

const MODAL_W: Pixels = px(520.0);
const BODY_PAD_H: Pixels = px(18.0);
const BODY_MAX_H: Pixels = px(460.0);

const SECTION_LABEL_MB: Pixels = px(8.0);
const FIELD_LABEL_FS: Pixels = FONT_MICRO;

const GRID_GAP: Pixels = px(8.0);
const INPUT_FLEX: f32 = 1.3;
const TYPE_FLEX: f32 = 1.0;
const CHANNEL_FLEX: f32 = 0.7;

const CONTROL_RADIUS: Pixels = px(7.0);
const CONTROL_PAD_H: Pixels = px(9.0);
const DISPLAY_PAD_H: Pixels = px(11.0);
const CONTROL_FS: Pixels = FONT_ROW;
const DISPLAY_GAP: Pixels = px(7.0);
const LISTEN_GLYPH: Pixels = px(12.0);
const CONTROL_HEIGHT: Pixels = px(32.0);
const FILTER_HEADER_GAP: Pixels = px(12.0);

const FOOTER_GAP: Pixels = px(8.0);
const FOOTER_HINT_FS: Pixels = FONT_META;

pub(crate) struct MappingModalLaunch {
    pub instance_id: Option<TriggerInstanceId>,
    pub signal: MidiSignal,
    pub linked_action: Option<ActionId>,
    pub devices: Vec<String>,
    pub input_enabled: bool,
}

pub(crate) struct MappingDraft {
    pub instance_id: Option<TriggerInstanceId>,
    pub signal: MidiSignal,
    pub action_id: ActionId,
    pub name: String,
}

pub(crate) enum MidiMappingModalEvent {
    Save(Box<MappingDraft>),
    Delete(TriggerInstanceId),
    Relearn,
    Cancel,
}

#[derive(PartialEq, Eq)]
enum OpenPanel {
    Type,
    Device,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChannelEntry {
    Any,
    Only(i64),
    Invalid,
}

fn channel_text(stored: Option<i64>) -> String {
    stored.map_or_else(String::new, |channel| shown_channel(channel).to_string())
}

impl ChannelEntry {
    fn parse(raw: &str) -> Self {
        let raw = raw.trim();
        if raw.is_empty() {
            return Self::Any;
        }
        match raw.parse::<i64>() {
            Ok(shown) if (1..=MIDI_CHANNELS).contains(&shown) => Self::Only(stored_channel(shown)),
            _ => Self::Invalid,
        }
    }
}

pub(crate) struct MidiMappingModal {
    instance_id: Option<TriggerInstanceId>,
    kind_id: String,
    selector: Option<i64>,
    channel_field: Entity<TextInput>,
    device: Option<String>,
    devices: Vec<String>,
    input_enabled: bool,
    listening: bool,
    open_panel: Option<OpenPanel>,
    picker: Entity<ActionPicker>,
    focus_pending: bool,
    _subs: Vec<Subscription>,
}

impl EventEmitter<MidiMappingModalEvent> for MidiMappingModal {}

impl MidiMappingModal {
    pub(crate) fn new(
        launch: MappingModalLaunch,
        action_repo: Arc<dyn ActionRepo>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let density = cx.density();
        let MappingModalLaunch {
            instance_id,
            signal,
            linked_action,
            devices,
            input_enabled,
        } = launch;
        let channel_text = channel_text(signal.channel);
        let channel_field = cx.new(|cx| {
            let mut field = TextInput::new(tr!("midi_modal_channel_any"), cx)
                .mono()
                .compact()
                .fixed_box(CONTROL_HEIGHT, scaled(CONTROL_PAD_H, density))
                .with_font_size(CONTROL_FS)
                .static_chrome(|palette| palette.border_input, Radius::Sm);
            if !channel_text.is_empty() {
                field.set_content(channel_text, cx);
            }
            field
        });
        let picker = cx.new(|cx| {
            ActionPicker::new(
                ActionPickerCopy {
                    id_prefix: "midi-modal",
                    section: tr!("midi_modal_section_action"),
                    filter: tr!("midi_modal_filter_actions"),
                    loading: tr!("midi_modal_actions_loading"),
                    none: tr!("midi_modal_actions_none"),
                    no_match: tr!("midi_modal_actions_empty"),
                    failed: |reason| tr!("midi_toast_error", message = reason),
                },
                linked_action,
                action_repo,
                &rt_handle,
                cx,
            )
        });
        let subs = vec![
            cx.subscribe(&channel_field, Self::on_channel_event),
            cx.subscribe(&picker, Self::on_picker_event),
        ];

        Self {
            instance_id,
            kind_id: signal.kind_id,
            selector: signal.selector,
            channel_field,
            device: signal.device,
            devices,
            input_enabled,
            listening: false,
            open_panel: None,
            picker,
            focus_pending: true,
            _subs: subs,
        }
    }

    pub(crate) fn apply_capture(&mut self, signal: MidiSignal, cx: &mut Context<Self>) {
        self.listening = false;
        self.kind_id = signal.kind_id;
        self.selector = signal.selector;
        self.device = signal.device;
        let channel = channel_text(signal.channel);
        self.channel_field
            .update(cx, |field, cx| field.set_content(channel, cx));
        cx.notify();
    }

    fn on_channel_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Cancelled => self.cancel(cx),
            InputEvent::Changed(_) => {
                self.sync_channel_validity(cx);
                cx.notify();
            }
            InputEvent::Submitted(_) => cx.notify(),
            InputEvent::Blurred(_) => {}
        }
    }

    fn on_picker_event(
        &mut self,
        _picker: Entity<ActionPicker>,
        event: &ActionPickerEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            ActionPickerEvent::SelectionChanged => cx.notify(),
            ActionPickerEvent::Confirmed => self.save(cx),
            ActionPickerEvent::Cancelled => self.cancel(cx),
        }
    }

    fn channel_entry(&self, cx: &Context<Self>) -> ChannelEntry {
        ChannelEntry::parse(self.channel_field.read(cx).content())
    }

    fn channel(&self, cx: &Context<Self>) -> Option<i64> {
        match self.channel_entry(cx) {
            ChannelEntry::Only(channel) => Some(channel),
            ChannelEntry::Any | ChannelEntry::Invalid => None,
        }
    }

    fn sync_channel_validity(&self, cx: &mut Context<Self>) {
        let invalid = self.channel_entry(cx) == ChannelEntry::Invalid;
        self.channel_field
            .update(cx, |field, cx| field.set_invalid(invalid, cx));
    }

    fn signal(&self, cx: &Context<Self>) -> MidiSignal {
        MidiSignal {
            kind_id: self.kind_id.clone(),
            selector: selector_key(&self.kind_id).and(self.selector),
            channel: self.channel(cx),
            device: self.device.clone(),
        }
    }

    fn select_kind(&mut self, kind_id: &str, cx: &mut Context<Self>) {
        if selector_key(kind_id) != selector_key(&self.kind_id) {
            self.selector = None;
        }
        self.kind_id = kind_id.to_owned();
        self.open_panel = None;
        cx.notify();
    }

    fn select_device(&mut self, device: Option<String>, cx: &mut Context<Self>) {
        self.device = device;
        self.open_panel = None;
        cx.notify();
    }

    fn toggle_panel(&mut self, panel: OpenPanel, cx: &mut Context<Self>) {
        self.open_panel = if self.open_panel.as_ref() == Some(&panel) {
            None
        } else {
            Some(panel)
        };
        cx.notify();
    }

    fn close_panel(&mut self, cx: &mut Context<Self>) {
        self.open_panel = None;
        cx.notify();
    }

    fn relearn(&mut self, cx: &mut Context<Self>) {
        if !self.input_enabled {
            return;
        }
        self.listening = true;
        cx.emit(MidiMappingModalEvent::Relearn);
        cx.notify();
    }

    fn selected_action(&self, cx: &App) -> Option<ActionId> {
        self.picker.read(cx).selected()
    }

    fn can_save(&self, cx: &Context<Self>) -> bool {
        self.selected_action(cx).is_some()
            && self.signal(cx).is_complete()
            && self.channel_entry(cx) != ChannelEntry::Invalid
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if !self.can_save(cx) {
            return;
        }
        let Some(action_id) = self.selected_action(cx) else {
            return;
        };
        let signal = self.signal(cx);
        let name = signal.label();
        cx.emit(MidiMappingModalEvent::Save(Box::new(MappingDraft {
            instance_id: self.instance_id,
            signal,
            action_id,
            name,
        })));
    }

    fn delete(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.instance_id {
            cx.emit(MidiMappingModalEvent::Delete(id));
        }
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(MidiMappingModalEvent::Cancel);
    }

    fn render_input_display(&self, palette: &ForgePalette, cx: &Context<Self>) -> AnyElement {
        let density = cx.density();
        let border = if self.listening {
            palette.info
        } else {
            palette.border_input
        };
        let mut display = div()
            .w_full()
            .flex()
            .items_center()
            .gap(scaled(DISPLAY_GAP, density))
            .h(CONTROL_HEIGHT)
            .px(scaled(DISPLAY_PAD_H, density))
            .rounded(CONTROL_RADIUS)
            .border(BORDER_THIN)
            .border_color(border)
            .bg(palette.shell)
            .font_family(cx.mono_family())
            .text_size(CONTROL_FS);
        if self.listening {
            display = display
                .text_color(palette.info)
                .child(icon(Icon::Antenna, LISTEN_GLYPH, palette.info))
                .child(tr!("midi_modal_listening"));
        } else {
            let signal = self.signal(cx);
            display = display
                .text_color(palette.text_primary)
                .child(signal.label());
        }
        field_label(palette, tr!("midi_modal_input"), display)
            .tone(palette.text_muted)
            .size(FIELD_LABEL_FS)
            .into_any_element()
    }

    fn render_type_select(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let open = matches!(self.open_panel, Some(OpenPanel::Type));
        let trigger = self.select_trigger(
            "midi-modal-type",
            kind_label(&self.kind_id),
            open,
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_panel(OpenPanel::Type, cx)),
            density,
            cx,
        );

        let panel = open.then(|| {
            let rows: Vec<AnyElement> = SIGNAL_KINDS
                .iter()
                .enumerate()
                .map(|(index, kind)| {
                    let kind_id = (*kind).to_owned();
                    self.panel_row(
                        ("midi-modal-type-option", index),
                        kind_label(kind),
                        *kind == self.kind_id,
                        palette,
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.select_kind(&kind_id, cx)
                        }),
                    )
                })
                .collect();
            self.select_panel(rows, palette, cx)
        });

        let mut field = div().relative().w_full().child(trigger);
        if let Some(panel) = panel {
            field = field.child(panel);
        }
        field_label(palette, tr!("midi_modal_type"), field)
            .tone(palette.text_muted)
            .size(FIELD_LABEL_FS)
            .into_any_element()
    }

    fn render_device_select(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let open = matches!(self.open_panel, Some(OpenPanel::Device));
        let label = self
            .device
            .clone()
            .unwrap_or_else(|| tr!("midi_modal_device_any"));
        let trigger = self.select_trigger(
            "midi-modal-device",
            label,
            open,
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_panel(OpenPanel::Device, cx)),
            density,
            cx,
        );

        let panel = open.then(|| {
            let mut rows = vec![self.panel_row(
                "midi-modal-device-any",
                tr!("midi_modal_device_any"),
                self.device.is_none(),
                palette,
                cx.listener(|this, _: &ClickEvent, _, cx| this.select_device(None, cx)),
            )];
            for (index, name) in self.devices.iter().enumerate() {
                let pick = name.clone();
                rows.push(self.panel_row(
                    ("midi-modal-device-option", index),
                    name.clone(),
                    self.device.as_deref() == Some(name.as_str()),
                    palette,
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.select_device(Some(pick.clone()), cx)
                    }),
                ));
            }
            self.select_panel(rows, palette, cx)
        });

        let mut field = div().relative().w_full().child(trigger);
        if let Some(panel) = panel {
            field = field.child(panel);
        }
        field_label(palette, tr!("midi_modal_device"), field)
            .tone(palette.text_muted)
            .size(FIELD_LABEL_FS)
            .into_any_element()
    }

    fn select_trigger(
        &self,
        id: &'static str,
        label: String,
        open: bool,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
        density: Density,
        cx: &App,
    ) -> AnyElement {
        let palette = &cx.palette();
        let border = if open {
            palette.border_active
        } else {
            palette.border_input
        };
        let hover_border = palette.border_active;
        div()
            .id(id)
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(scaled(GRID_GAP, density))
            .h(CONTROL_HEIGHT)
            .px(scaled(CONTROL_PAD_H, density))
            .rounded(CONTROL_RADIUS)
            .border(BORDER_THIN)
            .border_color(border)
            .bg(palette.shell)
            .cursor_pointer()
            .hover(move |s| s.border_color(hover_border))
            .key_activated(palette.text_primary)
            .on_click(on_click)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(cx.body_family())
                    .text_size(CONTROL_FS)
                    .text_color(palette.text_primary)
                    .child(label),
            )
            .child(icon(Icon::ChevronDown, CONTROL_FS, palette.text_faint))
            .into_any_element()
    }

    fn panel_row(
        &self,
        id: impl Into<gpui::ElementId>,
        label: String,
        selected: bool,
        palette: &ForgePalette,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> AnyElement {
        dropdown_row(id, label, palette)
            .current(selected)
            .on_click(on_click)
            .into_any_element()
    }

    fn select_panel(
        &self,
        rows: Vec<AnyElement>,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        dropdown(dropdown_list("midi-modal-select-list", palette).children(rows))
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_panel(cx));
            })
            .into_any_element()
    }

    fn render_signal(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let relearn = ghost_button_with_icon(Icon::Antenna, tr!("midi_modal_relearn"), palette)
            .height(CONTROL_HEIGHT)
            .disabled(!self.input_enabled)
            .on_click(
                "midi-modal-relearn",
                cx.listener(|this, _: &ClickEvent, _, cx| this.relearn(cx)),
            );

        let channel = field_label(
            palette,
            tr!("midi_modal_channel"),
            self.channel_field.clone(),
        )
        .tone(palette.text_muted)
        .size(FIELD_LABEL_FS);

        let grid = div()
            .w_full()
            .flex()
            .items_end()
            .gap(scaled(GRID_GAP, density))
            .child(grow_cell(
                self.render_input_display(palette, cx),
                INPUT_FLEX,
            ))
            .child(grow_cell(self.render_type_select(palette, cx), TYPE_FLEX))
            .child(grow_cell(channel, CHANNEL_FLEX))
            .child(div().flex_none().child(relearn));

        let mut column = div()
            .w_full()
            .flex()
            .flex_col()
            .child(section_caption(
                &tr!("midi_modal_section_signal"),
                palette,
                cx,
            ))
            .child(grid)
            .child(
                div()
                    .w_full()
                    .mt(spacing(Spacing::Sm, density))
                    .child(self.render_device_select(palette, cx)),
            );
        if !self.input_enabled {
            column = column.child(
                div()
                    .w_full()
                    .mt(spacing(Spacing::Sm, density))
                    .font_family(cx.body_family())
                    .text_size(FOOTER_HINT_FS)
                    .text_color(palette.warning)
                    .child(tr!("midi_modal_input_disabled")),
            );
        }
        column.mb(spacing(Spacing::Md, density)).into_any_element()
    }

    fn render_footer(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let leading: AnyElement = if self.instance_id.is_some() {
            ghost_button_with_icon(Icon::Trash, tr!("common_delete"), palette)
                .ink(palette.danger)
                .on_click(
                    "midi-modal-delete",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.delete(cx)),
                )
                .into_any_element()
        } else {
            let hint = if self.can_save(cx) {
                tr!("midi_modal_hint_ready")
            } else {
                tr!("midi_modal_hint_pick_action")
            };
            div()
                .font_family(cx.body_family())
                .text_size(FOOTER_HINT_FS)
                .text_color(palette.text_faint)
                .child(hint)
                .into_any_element()
        };

        let confirm_label = if self.instance_id.is_some() {
            tr!("midi_modal_save_changes")
        } else {
            tr!("midi_modal_add_mapping")
        };

        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(scaled(FILTER_HEADER_GAP, density))
            .child(leading)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(scaled(FOOTER_GAP, density))
                    .child(secondary_button(tr!("common_cancel"), palette).on_click(
                        "midi-modal-cancel",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
                    ))
                    .child(
                        primary_button(confirm_label, palette)
                            .disabled(!self.can_save(cx))
                            .on_click(
                                "midi-modal-save",
                                cx.listener(|this, _: &ClickEvent, _, cx| this.save(cx)),
                            ),
                    ),
            )
            .into_any_element()
    }
}

fn section_caption(label: &str, palette: &ForgePalette, cx: &App) -> impl IntoElement {
    div()
        .w_full()
        .mb(SECTION_LABEL_MB)
        .font_family(cx.mono_family())
        .text_size(FONT_SECTION_LABEL)
        .text_color(palette.text_muted)
        .child(SharedString::from(label.to_uppercase()))
}

impl Render for MidiMappingModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let density = cx.density();
        let palette = cx.palette();
        if self.focus_pending {
            self.focus_pending = false;
            focus_inside_modal(window, cx, |window, cx| {
                self.picker
                    .update(cx, |picker, cx| picker.focus_filter(window, cx))
            });
        }

        let body = div()
            .id("midi-modal-body")
            .w_full()
            .max_h(BODY_MAX_H)
            .min_h_0()
            .overflow_y_revealing_focus()
            .py(spacing(Spacing::Md, density))
            .px(scaled(BODY_PAD_H, density))
            .flex()
            .flex_col()
            .child(self.render_signal(&palette, cx))
            .child(self.picker.clone());

        let (title, subtitle) = if self.instance_id.is_some() {
            (tr!("midi_modal_title_edit"), self.signal(cx).label())
        } else {
            (
                tr!("midi_modal_title_add"),
                tr!("midi_modal_subtitle_captured"),
            )
        };

        let card = modal(title, body, &palette)
            .header_icon(Icon::Piano, palette.info)
            .subtitle(subtitle)
            .width(MODAL_W)
            .flush_body()
            .body_scrolls_itself()
            .footer(self.render_footer(&palette, cx))
            .on_close(
                "midi-modal-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            );

        let view = cx.entity();
        div().absolute().top_0().left_0().size_full().child(
            overlay(card, &palette)
                .position(OverlayPosition::Center)
                .on_dismiss("midi-modal-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel(cx));
                }),
        )
    }
}

#[cfg(test)]
mod tests {
    use forge_storage::MockActionRepo;
    use forge_storage::{Language, StorageError};
    use gpui::{Entity, TestAppContext};

    use super::*;
    use crate::app::async_bridge::Loadable;
    use crate::app::i18n::install_language;
    use crate::test_support::{
        action_running, install_presentation, pump, runtime, shown_in_english,
    };

    #[test]
    fn channel_entry_reads_blank_as_any_and_keeps_only_the_midi_channel_range() {
        for (raw, expected) in [
            ("", ChannelEntry::Any),
            ("   ", ChannelEntry::Any),
            ("0", ChannelEntry::Invalid),
            ("1", ChannelEntry::Only(0)),
            (" 16 ", ChannelEntry::Only(15)),
            ("17", ChannelEntry::Invalid),
            ("-1", ChannelEntry::Invalid),
            ("1.5", ChannelEntry::Invalid),
            ("one", ChannelEntry::Invalid),
            ("99999999999999999999", ChannelEntry::Invalid),
        ] {
            assert_eq!(ChannelEntry::parse(raw), expected, "{raw:?}");
        }
    }

    fn outage() -> StorageError {
        StorageError::Connection {
            reason: "disk gone".to_owned(),
        }
    }

    fn opened(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        listed: fn() -> Result<Vec<forge_types::Action>, StorageError>,
    ) -> Entity<MidiMappingModal> {
        install_language(Language::En);
        install_presentation(cx);
        let mut repo = MockActionRepo::new();
        repo.expect_list().returning(listed);
        cx.update(|cx| {
            cx.new(|cx| {
                MidiMappingModal::new(
                    MappingModalLaunch {
                        instance_id: None,
                        signal: MidiSignal {
                            kind_id: String::new(),
                            selector: None,
                            channel: None,
                            device: None,
                        },
                        linked_action: None,
                        devices: Vec::new(),
                        input_enabled: true,
                    },
                    Arc::new(repo),
                    rt.handle().clone(),
                    cx,
                )
            })
        })
    }

    fn settled(cx: &mut TestAppContext, rt: &tokio::runtime::Runtime) {
        pump(rt);
        cx.run_until_parked();
    }

    #[gpui::test]
    fn the_list_is_loading_until_the_repository_answers(cx: &mut TestAppContext) {
        let rt = runtime();
        let modal = opened(cx, &rt, || Ok(Vec::new()));

        let before = modal.read_with(cx, |modal, cx| {
            matches!(modal.picker.read(cx).actions, Loadable::Loading)
        });
        settled(cx, &rt);
        let after = modal.read_with(
            cx,
            |modal, cx| matches!(&modal.picker.read(cx).actions, Loadable::Loaded(listed) if listed.is_empty()),
        );

        assert_eq!((before, after), (true, true));
    }

    #[gpui::test]
    fn loaded_actions_are_listed_ignoring_case(cx: &mut TestAppContext) {
        let rt = runtime();
        let modal = opened(cx, &rt, || {
            Ok(["beta", "Alpha", "Gamma"]
                .map(|name| action_running(name, Vec::new()))
                .to_vec())
        });
        settled(cx, &rt);

        let names: Vec<String> = modal.read_with(cx, |modal, cx| {
            modal
                .picker
                .read(cx)
                .actions
                .loaded()
                .expect("the list loaded")
                .iter()
                .map(|choice| choice.name.clone())
                .collect()
        });

        assert_eq!(names, ["Alpha", "beta", "Gamma"]);
    }

    #[gpui::test]
    fn a_repository_failure_keeps_the_text_shown_to_the_user(cx: &mut TestAppContext) {
        let rt = runtime();
        let modal = opened(cx, &rt, || Err(outage()));
        settled(cx, &rt);

        let failure = modal.read_with(cx, |modal, cx| modal.picker.read(cx).actions.failure());

        assert_eq!(failure, Some(shown_in_english(outage())));
    }
}
