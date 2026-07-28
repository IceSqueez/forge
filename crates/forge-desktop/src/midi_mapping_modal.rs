use std::sync::Arc;

use forge_components::{
    BORDER_THIN, ForgePalette, Icon, InputEvent, OverlayPosition, Radius, SearchState, TextInput,
    anchored_popover_below, body_family, field_label, ghost_button_with_icon, icon, modal,
    mono_family, overlay, primary_button, radius, secondary_button, tr, with_alpha,
};
use forge_storage::ActionRepo;
use forge_types::{ActionId, TriggerInstanceId};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, Pixels, SharedString, Subscription,
    Window, div, prelude::*, px,
};

use crate::async_bridge;
use crate::midi_signal::{MidiSignal, SIGNAL_KINDS, kind_label, selector_key};
use crate::presentation::ActivePresentation;

const MODAL_W: Pixels = px(520.0);
const BODY_PAD_V: Pixels = px(16.0);
const BODY_PAD_H: Pixels = px(18.0);
const BODY_MAX_H: Pixels = px(460.0);

const SECTION_LABEL_FS: Pixels = px(9.5);
const SECTION_LABEL_MB: Pixels = px(8.0);
const FIELD_LABEL_FS: Pixels = px(10.0);

const GRID_GAP: Pixels = px(8.0);
const SIGNAL_MB: Pixels = px(16.0);
const DEVICE_ROW_MT: Pixels = px(10.0);
const INPUT_FLEX: f32 = 1.3;
const TYPE_FLEX: f32 = 1.0;
const CHANNEL_FLEX: f32 = 0.7;

const CONTROL_RADIUS: Pixels = px(7.0);
const CONTROL_PAD_V: Pixels = px(7.0);
const CONTROL_PAD_H: Pixels = px(9.0);
const DISPLAY_PAD_H: Pixels = px(11.0);
const CONTROL_FS: Pixels = px(12.5);
const DISPLAY_GAP: Pixels = px(7.0);
const LISTEN_GLYPH: Pixels = px(12.0);
const TRIGGER_HEIGHT: Pixels = px(34.0);
const RELEARN_HEIGHT: Pixels = px(32.0);
const PANEL_PAD_V: Pixels = px(4.0);
const PANEL_ROW_PAD_V: Pixels = px(5.0);
const PANEL_ROW_PAD_H: Pixels = px(9.0);
const PANEL_MAX_H: Pixels = px(220.0);

const FILTER_MAX_W: Pixels = px(220.0);
const FILTER_HEADER_GAP: Pixels = px(12.0);

const LIST_RADIUS: Pixels = px(9.0);
const LIST_PAD: Pixels = px(8.0);
const LIST_MAX_H: Pixels = px(230.0);
const ROW_GAP: Pixels = px(8.0);
const ROW_PAD_V: Pixels = px(6.0);
const ROW_PAD_H: Pixels = px(9.0);
const ROW_RADIUS: Pixels = px(6.0);
const ROW_MB: Pixels = px(1.0);
const ROW_DOT: Pixels = px(6.0);
const ROW_FS: Pixels = px(12.0);
const ROW_CHECK: Pixels = px(12.0);
const EMPTY_PAD_V: Pixels = px(20.0);

const FOOTER_GAP: Pixels = px(8.0);
const FOOTER_HINT_FS: Pixels = px(11.0);

pub struct MappingModalLaunch {
    pub instance_id: Option<TriggerInstanceId>,
    pub signal: MidiSignal,
    pub linked_action: Option<ActionId>,
    pub devices: Vec<String>,
    pub input_enabled: bool,
}

pub struct MappingDraft {
    pub instance_id: Option<TriggerInstanceId>,
    pub signal: MidiSignal,
    pub action_id: ActionId,
    pub name: String,
}

pub enum MidiMappingModalEvent {
    Save(Box<MappingDraft>),
    Delete(TriggerInstanceId),
    Relearn,
    Cancel,
}

struct ActionChoice {
    id: ActionId,
    name: String,
}

enum ActionsState {
    Loading,
    Ready(Vec<ActionChoice>),
    Failed(String),
}

enum OpenPanel {
    Type,
    Device,
}

pub struct MidiMappingModal {
    instance_id: Option<TriggerInstanceId>,
    kind_id: String,
    selector: Option<i64>,
    channel_field: Entity<TextInput>,
    device: Option<String>,
    devices: Vec<String>,
    input_enabled: bool,
    listening: bool,
    open_panel: Option<OpenPanel>,
    filter: SearchState,
    actions: ActionsState,
    selected_action: Option<ActionId>,
    focus_pending: bool,
    _subs: Vec<Subscription>,
}

impl EventEmitter<MidiMappingModalEvent> for MidiMappingModal {}

impl MidiMappingModal {
    pub fn new(
        launch: MappingModalLaunch,
        action_repo: Arc<dyn ActionRepo>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let MappingModalLaunch {
            instance_id,
            signal,
            linked_action,
            devices,
            input_enabled,
        } = launch;
        let palette = cx.palette();
        let channel_text = signal.channel.map(|c| c.to_string()).unwrap_or_default();
        let channel_field = cx.new(|cx| {
            let mut field = TextInput::new(tr!("midi_modal_channel_any"), cx)
                .with_palette(palette)
                .mono()
                .compact()
                .with_font_size(CONTROL_FS)
                .static_chrome(palette.border_input, Radius::Sm);
            if !channel_text.is_empty() {
                field.set_content(channel_text, cx);
            }
            field
        });
        let filter = SearchState::new(cx, palette, tr!("midi_modal_filter_actions"));
        let subs = vec![
            cx.subscribe(&channel_field, Self::on_channel_event),
            cx.subscribe(filter.field(), Self::on_filter_event),
        ];

        async_bridge::run_async(
            &rt_handle,
            async move {
                action_repo
                    .list()
                    .await
                    .map(|actions| {
                        actions
                            .into_iter()
                            .map(|action| ActionChoice {
                                id: action.id,
                                name: action.name,
                            })
                            .collect::<Vec<_>>()
                    })
                    .map_err(|e| e.to_string())
            },
            |this, result, cx| this.apply_actions(result, cx),
            cx,
        );

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
            filter,
            actions: ActionsState::Loading,
            selected_action: linked_action,
            focus_pending: true,
            _subs: subs,
        }
    }

    pub fn apply_capture(&mut self, signal: MidiSignal, cx: &mut Context<Self>) {
        self.listening = false;
        self.kind_id = signal.kind_id;
        self.selector = signal.selector;
        self.device = signal.device;
        let channel = signal.channel.map(|c| c.to_string()).unwrap_or_default();
        self.channel_field
            .update(cx, |field, cx| field.set_content(channel, cx));
        cx.notify();
    }

    fn apply_actions(&mut self, result: Result<Vec<ActionChoice>, String>, cx: &mut Context<Self>) {
        self.actions = match result {
            Ok(mut actions) => {
                actions.sort_by_key(|a| a.name.to_lowercase());
                ActionsState::Ready(actions)
            }
            Err(message) => ActionsState::Failed(message),
        };
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
            InputEvent::Changed(_) | InputEvent::Submitted(_) => cx.notify(),
        }
    }

    fn on_filter_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, InputEvent::Cancelled) {
            self.cancel(cx);
            return;
        }
        if self.filter.on_changed(event) {
            cx.notify();
        }
    }

    fn channel(&self, cx: &Context<Self>) -> Option<i64> {
        let raw = self.channel_field.read(cx).content().trim().to_owned();
        if raw.is_empty() {
            return None;
        }
        raw.parse::<i64>().ok().map(|value| value.clamp(0, 15))
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
        let same = matches!(
            (&self.open_panel, &panel),
            (Some(OpenPanel::Type), OpenPanel::Type) | (Some(OpenPanel::Device), OpenPanel::Device)
        );
        self.open_panel = if same { None } else { Some(panel) };
        cx.notify();
    }

    fn close_panel(&mut self, cx: &mut Context<Self>) {
        self.open_panel = None;
        cx.notify();
    }

    fn select_action(&mut self, id: ActionId, cx: &mut Context<Self>) {
        self.selected_action = Some(id);
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

    fn can_save(&self, cx: &Context<Self>) -> bool {
        self.selected_action.is_some() && self.signal(cx).is_complete()
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if !self.can_save(cx) {
            return;
        }
        let Some(action_id) = self.selected_action else {
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
        let border = if self.listening {
            palette.info
        } else {
            palette.border_input
        };
        let mut display = div()
            .w_full()
            .flex()
            .items_center()
            .gap(DISPLAY_GAP)
            .py(CONTROL_PAD_V)
            .px(DISPLAY_PAD_H)
            .rounded(CONTROL_RADIUS)
            .border(BORDER_THIN)
            .border_color(border)
            .bg(palette.shell)
            .font_family(mono_family())
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
        let open = matches!(self.open_panel, Some(OpenPanel::Type));
        let trigger = self.select_trigger(
            "midi-modal-type",
            kind_label(&self.kind_id).to_owned(),
            open,
            palette,
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_panel(OpenPanel::Type, cx)),
        );

        let panel = open.then(|| {
            let rows: Vec<AnyElement> = SIGNAL_KINDS
                .iter()
                .enumerate()
                .map(|(index, kind)| {
                    let kind_id = (*kind).to_owned();
                    self.panel_row(
                        ("midi-modal-type-option", index),
                        kind_label(kind).to_owned(),
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
        let open = matches!(self.open_panel, Some(OpenPanel::Device));
        let label = self
            .device
            .clone()
            .unwrap_or_else(|| tr!("midi_modal_device_any"));
        let trigger = self.select_trigger(
            "midi-modal-device",
            label,
            open,
            palette,
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_panel(OpenPanel::Device, cx)),
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
        palette: &ForgePalette,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> AnyElement {
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
            .gap(GRID_GAP)
            .py(CONTROL_PAD_V)
            .px(CONTROL_PAD_H)
            .rounded(CONTROL_RADIUS)
            .border(BORDER_THIN)
            .border_color(border)
            .bg(palette.shell)
            .cursor_pointer()
            .hover(move |s| s.border_color(hover_border))
            .on_click(on_click)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(body_family())
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
        let mut row = div()
            .id(id)
            .w_full()
            .flex()
            .items_center()
            .gap(ROW_GAP)
            .py(PANEL_ROW_PAD_V)
            .px(PANEL_ROW_PAD_H)
            .rounded(ROW_RADIUS)
            .cursor_pointer()
            .hover(|s| s.bg(palette.surface_overlay))
            .on_click(on_click)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(body_family())
                    .text_size(CONTROL_FS)
                    .text_color(palette.text_primary)
                    .child(label),
            );
        if selected {
            row = row.child(icon(Icon::Check, ROW_CHECK, palette.brand));
        }
        row.into_any_element()
    }

    fn select_panel(
        &self,
        rows: Vec<AnyElement>,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let panel = div()
            .w_full()
            .flex()
            .flex_col()
            .py(PANEL_PAD_V)
            .max_h(PANEL_MAX_H)
            .overflow_hidden()
            .bg(palette.elevated)
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_input)
            .occlude()
            .children(rows);
        anchored_popover_below(TRIGGER_HEIGHT, panel)
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_panel(cx));
            })
            .into_any_element()
    }

    fn render_signal(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let relearn = ghost_button_with_icon(Icon::Antenna, tr!("midi_modal_relearn"), palette)
            .height(RELEARN_HEIGHT)
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
            .gap(GRID_GAP)
            .child(weighted(self.render_input_display(palette, cx), INPUT_FLEX))
            .child(weighted(self.render_type_select(palette, cx), TYPE_FLEX))
            .child(weighted(channel, CHANNEL_FLEX))
            .child(div().flex_none().child(relearn));

        let mut column = div()
            .w_full()
            .flex()
            .flex_col()
            .child(section_caption(&tr!("midi_modal_section_signal"), palette))
            .child(grid)
            .child(
                div()
                    .w_full()
                    .mt(DEVICE_ROW_MT)
                    .child(self.render_device_select(palette, cx)),
            );
        if !self.input_enabled {
            column = column.child(
                div()
                    .w_full()
                    .mt(DEVICE_ROW_MT)
                    .font_family(body_family())
                    .text_size(FOOTER_HINT_FS)
                    .text_color(palette.warning)
                    .child(tr!("midi_modal_input_disabled")),
            );
        }
        column.mb(SIGNAL_MB).into_any_element()
    }

    fn render_actions(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let header = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(FILTER_HEADER_GAP)
            .mb(SECTION_LABEL_MB)
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(SECTION_LABEL_FS)
                    .text_color(palette.text_muted)
                    .child(SharedString::from(
                        tr!("midi_modal_section_action").to_uppercase(),
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .max_w(FILTER_MAX_W)
                    .child(self.filter.field().clone()),
            );

        let list = match &self.actions {
            ActionsState::Loading => self.list_message(tr!("midi_modal_actions_loading"), palette),
            ActionsState::Failed(message) => {
                self.list_message(tr!("midi_toast_error", message = message.as_str()), palette)
            }
            ActionsState::Ready(actions) if actions.is_empty() => {
                self.list_message(tr!("midi_modal_actions_none"), palette)
            }
            ActionsState::Ready(actions) => {
                let matched: Vec<&ActionChoice> = actions
                    .iter()
                    .filter(|choice| self.filter.matches(&choice.name))
                    .collect();
                if matched.is_empty() {
                    self.list_message(tr!("midi_modal_actions_empty"), palette)
                } else {
                    let rows: Vec<AnyElement> = matched
                        .into_iter()
                        .enumerate()
                        .map(|(index, choice)| self.render_action_row(index, choice, palette, cx))
                        .collect();
                    div().w_full().flex().flex_col().children(rows)
                }
            }
        };

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(header)
            .child(
                div()
                    .id("midi-modal-actions")
                    .w_full()
                    .max_h(LIST_MAX_H)
                    .overflow_y_scroll()
                    .p(LIST_PAD)
                    .rounded(LIST_RADIUS)
                    .border(BORDER_THIN)
                    .border_color(palette.border_regular)
                    .bg(palette.shell)
                    .child(list),
            )
            .into_any_element()
    }

    fn list_message(&self, message: String, palette: &ForgePalette) -> gpui::Div {
        div().w_full().flex().flex_col().child(
            div()
                .w_full()
                .py(EMPTY_PAD_V)
                .text_align(gpui::TextAlign::Center)
                .font_family(body_family())
                .text_size(ROW_FS)
                .text_color(palette.text_muted)
                .child(message),
        )
    }

    fn render_action_row(
        &self,
        index: usize,
        choice: &ActionChoice,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.selected_action == Some(choice.id);
        let (background, border, ink) = if selected {
            (palette.surface_overlay, palette.brand, palette.text_primary)
        } else {
            (
                with_alpha(palette.shell, 0.0),
                with_alpha(palette.shell, 0.0),
                palette.text_secondary,
            )
        };
        let id = choice.id;
        let hover_bg = palette.base;
        let mut row = div()
            .id(("midi-modal-action", index))
            .w_full()
            .flex()
            .items_center()
            .gap(ROW_GAP)
            .mb(ROW_MB)
            .py(ROW_PAD_V)
            .px(ROW_PAD_H)
            .rounded(ROW_RADIUS)
            .border(BORDER_THIN)
            .border_color(border)
            .bg(background)
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_action(id, cx)))
            .child(
                div()
                    .flex_none()
                    .size(ROW_DOT)
                    .rounded_full()
                    .bg(palette.brand),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(body_family())
                    .text_size(ROW_FS)
                    .text_color(ink)
                    .child(choice.name.clone()),
            );
        if selected {
            row = row.child(icon(Icon::Check, ROW_CHECK, palette.brand));
        } else {
            row = row.hover(move |s| s.bg(hover_bg));
        }
        row.into_any_element()
    }

    fn render_footer(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let leading: AnyElement = if self.instance_id.is_some() {
            ghost_button_with_icon(Icon::Trash, tr!("common_delete"), palette)
                .ink(palette.random)
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
                .font_family(body_family())
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
            .gap(FILTER_HEADER_GAP)
            .child(leading)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(FOOTER_GAP)
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

fn weighted(el: impl IntoElement, grow: f32) -> gpui::Div {
    let mut cell = div().min_w(px(0.0)).child(el);
    let style = cell.style();
    style.flex_grow = Some(grow);
    style.flex_basis = Some(gpui::relative(0.0).into());
    cell
}

fn section_caption(label: &str, palette: &ForgePalette) -> impl IntoElement {
    div()
        .w_full()
        .mb(SECTION_LABEL_MB)
        .font_family(mono_family())
        .text_size(SECTION_LABEL_FS)
        .text_color(palette.text_muted)
        .child(SharedString::from(label.to_uppercase()))
}

impl Render for MidiMappingModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        if self.focus_pending {
            self.focus_pending = false;
            self.filter
                .field()
                .update(cx, |field, cx| field.focus(window, cx));
        }

        let body = div()
            .id("midi-modal-body")
            .w_full()
            .max_h(BODY_MAX_H)
            .overflow_y_scroll()
            .py(BODY_PAD_V)
            .px(BODY_PAD_H)
            .flex()
            .flex_col()
            .child(self.render_signal(&palette, cx))
            .child(self.render_actions(&palette, cx));

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
