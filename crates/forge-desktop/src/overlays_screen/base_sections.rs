use forge_components::{
    BORDER_THIN, FONT_XS, FONT_XXS, ForgePalette, Icon, Picker, PickerEvent, PickerItem,
    PickerLabels, body_family, field_label, ghost_button_with_icon, icon, section_label, toggle,
    tr,
};
use forge_overlay::config::{DURATION, SOUND, SPEECH, SPEECH_VOICE};
use forge_overlay::{ConfigSection, DeliveryDisposition, OverlayConfig};
use forge_runtime::{OverlayDelivery, OverlayDispatch, ShowEnd, ShowTicket};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, Pixels, Point, SharedString, Subscription, Task,
    Window, div, prelude::*, px,
};

use super::property_panel::{
    FIELD_GAP, NOTICE_LINE_H, NOTICE_PAD, NOTICE_RADIUS, OverlayPropertyPanel, SECTION_GAP,
    SECTION_TOP_GAP,
};
use crate::async_bridge;
use crate::config_form::{ConfigField, ConfigFieldHandlers, render_config_control};
use crate::presentation::ActivePresentation;

const LOOK_FIELD_KEY: &str = "look";
const ROW_GAP: Pixels = px(8.0);
const HINT_TOP: Pixels = px(4.0);
const FLAG_GLYPH: Pixels = px(12.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PanelSection {
    Content,
    Style,
    Behavior,
    Audio,
    Display,
}

pub(super) const FIELD_SECTIONS: [PanelSection; 4] = [
    PanelSection::Content,
    PanelSection::Style,
    PanelSection::Behavior,
    PanelSection::Audio,
];

impl PanelSection {
    pub(super) fn of(key: &str, declared: ConfigSection) -> Self {
        match key {
            SOUND | SPEECH | SPEECH_VOICE => Self::Audio,
            DURATION => Self::Display,
            _ => match declared {
                ConfigSection::Content => Self::Content,
                ConfigSection::Style => Self::Style,
                ConfigSection::Behavior => Self::Behavior,
            },
        }
    }

    pub(super) fn heading(self) -> String {
        match self {
            Self::Content => tr!("overlays_panel_section_content"),
            Self::Style => tr!("overlays_panel_section_style"),
            Self::Behavior => tr!("overlays_panel_section_behavior"),
            Self::Audio => tr!("overlays_panel_section_audio"),
            Self::Display => tr!("overlays_panel_section_display"),
        }
    }
}

pub(super) fn base_label(key: &str) -> Option<String> {
    match key {
        SOUND => Some(tr!("overlays_base_sound")),
        SPEECH => Some(tr!("overlays_base_speech")),
        SPEECH_VOICE => Some(tr!("overlays_base_voice")),
        DURATION => Some(tr!("overlays_base_duration")),
        _ => None,
    }
}

pub(super) fn base_hint(key: &str) -> Option<String> {
    match key {
        SPEECH => Some(tr!("overlays_base_speech_hint")),
        SPEECH_VOICE => Some(tr!("overlays_base_voice_hint")),
        DURATION => Some(tr!("overlays_base_duration_hint")),
        _ => None,
    }
}

#[derive(Clone)]
pub(super) struct LookSummary {
    pub(super) kind_id: String,
    pub(super) label: String,
    pub(super) summary: String,
    pub(super) icon: Icon,
    pub(super) disposition: DeliveryDisposition,
    pub(super) draws: bool,
}

pub(super) struct BaseLaunch {
    pub(super) look: LookSummary,
    pub(super) looks: Vec<LookSummary>,
    pub(super) receiver: bool,
}

pub(super) enum BaseEvent {
    ChangeLook {
        kind_id: String,
        config: OverlayConfig,
    },
    SetReceiver(bool),
}

enum Fired {
    Applied(OverlayDelivery),
    Queued(OverlayDelivery, ShowTicket),
}

enum ShowFire {
    Idle,
    Sending,
    Playing(OverlayDelivery),
    Applied(OverlayDelivery),
    Ended(ShowEnd),
    Failed(String),
}

struct LookPicker {
    picker: Entity<Picker>,
    position: Point<Pixels>,
    _sub: Subscription,
}

pub(super) struct BaseState {
    look: LookSummary,
    looks: Vec<LookSummary>,
    picker: Option<LookPicker>,
    receiver: bool,
    waiting: usize,
    hides_itself: bool,
    fire: ShowFire,
    fire_epoch: u64,
    queue_watch: Option<Task<()>>,
}

impl BaseState {
    pub(super) fn unset() -> Self {
        Self::new(BaseLaunch {
            look: LookSummary {
                kind_id: String::new(),
                label: String::new(),
                summary: String::new(),
                icon: Icon::Browser,
                disposition: DeliveryDisposition::Replace,
                draws: true,
            },
            looks: Vec::new(),
            receiver: false,
        })
    }

    fn new(launch: BaseLaunch) -> Self {
        Self {
            look: launch.look,
            looks: launch.looks,
            picker: None,
            receiver: launch.receiver,
            waiting: 0,
            hides_itself: false,
            fire: ShowFire::Idle,
            fire_epoch: 0,
            queue_watch: None,
        }
    }

    pub(super) fn kind_id(&self) -> &str {
        &self.look.kind_id
    }

    fn queues_shows(&self) -> bool {
        self.look.disposition == DeliveryDisposition::Transient
    }

    fn in_flight(&self) -> bool {
        matches!(self.fire, ShowFire::Sending | ShowFire::Playing(_))
    }
}

impl OverlayPropertyPanel {
    pub(super) fn adopt_base(&mut self, launch: BaseLaunch, cx: &mut Context<Self>) {
        self.base = BaseState::new(launch);
        self.watch_queue(cx);
        cx.notify();
    }

    pub(super) fn set_receiver(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.base.receiver == on {
            return;
        }
        self.base.receiver = on;
        cx.notify();
    }

    pub(super) fn set_hides_itself(&mut self, flagged: bool, cx: &mut Context<Self>) {
        if self.base.hides_itself == flagged {
            return;
        }
        self.base.hides_itself = flagged;
        cx.notify();
    }

    pub(super) fn look_kind(&self) -> &str {
        self.base.kind_id()
    }

    fn watch_queue(&mut self, cx: &mut Context<Self>) {
        if !self.base.queues_shows() {
            return;
        }
        let mut depth = self.service.watch_pending_shows(&self.overlay_id);
        self.base.waiting = depth.depth();
        self.base.queue_watch = Some(cx.spawn(async move |this, cx| {
            while let Some(waiting) = depth.changed().await {
                if this
                    .update(cx, |this, cx| this.apply_waiting(waiting, cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
    }

    fn apply_waiting(&mut self, waiting: usize, cx: &mut Context<Self>) {
        if self.base.waiting == waiting {
            return;
        }
        self.base.waiting = waiting;
        cx.notify();
    }

    fn clear_queue(&mut self) {
        self.service.clear_shows(&self.overlay_id);
    }

    fn toggle_receiver(&mut self, cx: &mut Context<Self>) {
        let next = !self.base.receiver;
        self.base.receiver = next;
        cx.emit(BaseEvent::SetReceiver(next));
        cx.notify();
    }

    /// Fires the stored content through the same path an action step takes, with sample values
    /// for the event variables, so the show queues, sounds and speaks as a real one would.
    fn fire_show(&mut self, cx: &mut Context<Self>) {
        if self.base.in_flight() {
            return;
        }
        self.emit_save(cx);
        self.base.fire_epoch = self.base.fire_epoch.wrapping_add(1);
        let epoch = self.base.fire_epoch;
        self.base.fire = ShowFire::Sending;

        let service = self.service.clone();
        let id = self.overlay_id.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let args = service.sample(&id).await.args();
                match service
                    .send_to(&id, &OverlayConfig::new(), &args, None)
                    .await
                {
                    Ok(OverlayDispatch::Applied(delivery)) => Ok(Fired::Applied(delivery)),
                    Ok(OverlayDispatch::Queued(ticket)) => {
                        let delivery = service.receivers(&id).await.outcome();
                        Ok(Fired::Queued(delivery, ticket))
                    }
                    Err(error) => Err(error.to_string()),
                }
            },
            move |this, result, cx| this.on_show_fired(epoch, result, cx),
            cx,
        );
        cx.notify();
    }

    fn on_show_fired(&mut self, epoch: u64, result: Result<Fired, String>, cx: &mut Context<Self>) {
        if self.base.fire_epoch != epoch {
            return;
        }
        match result {
            Ok(Fired::Applied(delivery)) => self.base.fire = ShowFire::Applied(delivery),
            Ok(Fired::Queued(delivery, ticket)) => {
                self.base.fire = ShowFire::Playing(delivery);
                async_bridge::run_async(
                    &self.rt_handle,
                    ticket.finished(),
                    move |this, end, cx| this.on_show_ended(epoch, end, cx),
                    cx,
                );
            }
            Err(message) => self.base.fire = ShowFire::Failed(message),
        }
        cx.notify();
    }

    fn on_show_ended(&mut self, epoch: u64, end: ShowEnd, cx: &mut Context<Self>) {
        if self.base.fire_epoch != epoch {
            return;
        }
        self.base.fire = ShowFire::Ended(end);
        cx.notify();
    }

    fn open_look(
        &mut self,
        _key: String,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.base.picker.take().is_some() {
            cx.notify();
            return;
        }
        let items: Vec<PickerItem> = self
            .base
            .looks
            .iter()
            .map(|look| PickerItem {
                id: SharedString::from(look.kind_id.clone()),
                label: SharedString::from(look.label.clone()),
                sublabel: Some(SharedString::from(look.summary.clone())),
                icon: look.icon,
            })
            .collect();
        let labels = PickerLabels {
            title: tr!("overlays_look_picker_title").into(),
            placeholder: tr!("widget_picker_search_placeholder").into(),
            empty: tr!("overlays_panel_choice_empty").into(),
            loading: tr!("widget_picker_loading").into(),
            cancel: tr!("common_cancel").into(),
        };
        let palette = cx.palette();
        let picker = cx.new(|cx| Picker::new(labels, items, palette, cx));
        let sub = cx.subscribe(&picker, Self::on_look_picked);
        picker.update(cx, |picker, cx| picker.focus(window, cx));
        self.base.picker = Some(LookPicker {
            picker,
            position,
            _sub: sub,
        });
        cx.notify();
    }

    fn on_look_picked(
        &mut self,
        _picker: Entity<Picker>,
        event: &PickerEvent,
        cx: &mut Context<Self>,
    ) {
        self.base.picker = None;
        if let PickerEvent::Selected(kind_id) = event
            && kind_id.as_ref() != self.base.look.kind_id
        {
            let config = self.pending_config(cx);
            self.stored = config.clone();
            cx.emit(BaseEvent::ChangeLook {
                kind_id: kind_id.to_string(),
                config,
            });
        }
        cx.notify();
    }

    fn close_look(&mut self, cx: &mut Context<Self>) {
        self.base.picker = None;
        cx.notify();
    }

    pub(super) fn look_popover(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let open = self.base.picker.as_ref()?;
        let view = cx.entity();
        Some(
            forge_components::anchored_popover(open.position, open.picker.clone())
                .on_dismiss(move |_window, cx| {
                    view.update(cx, |this, cx| this.close_look(cx));
                })
                .into_any_element(),
        )
    }

    pub(super) fn render_look_section(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let field = ConfigField::Choice {
            key: LOOK_FIELD_KEY.to_owned(),
            gate: None,
            options: self
                .base
                .looks
                .iter()
                .map(|look| (look.kind_id.clone(), look.label.clone()))
                .collect(),
            selected: self.base.look.kind_id.clone(),
            dependency: None,
        };
        let handlers = ConfigFieldHandlers {
            toggle: Self::ignore_toggle,
            slide: Self::ignore_slide,
            pick: Self::ignore_pick,
            open_choice: Some(Self::open_look),
        };
        let view = cx.entity();
        let control = render_config_control(&field, palette, "overlays-panel", &view, &handlers);

        div()
            .flex()
            .flex_col()
            .child(div().pb(SECTION_GAP).child(section_label(
                tr!("overlays_panel_section_look").to_uppercase(),
                palette,
            )))
            .child(
                div()
                    .pb(FIELD_GAP)
                    .child(hinted(control, tr!("overlays_look_hint"), palette)),
            )
            .into_any_element()
    }

    fn ignore_toggle(&mut self, _key: String, _cx: &mut Context<Self>) {}

    fn ignore_slide(&mut self, _key: String, _value: i64, _cx: &mut Context<Self>) {}

    fn ignore_pick(&mut self, _key: String, _value: String, _cx: &mut Context<Self>) {}

    pub(super) fn render_display_extras(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut rows = Vec::new();
        if let Some(note) = display_note(&self.base.look) {
            rows.push(note_line(note, palette));
        }
        if self.base.hides_itself && self.base.queues_shows() {
            rows.push(own_hide_flag(palette));
        }
        if self.base.queues_shows() {
            rows.push(self.render_queue_row(palette, cx));
        }
        rows.push(self.render_fire_row(palette, cx));
        rows.into_iter()
            .map(|row| div().pb(FIELD_GAP).child(row).into_any_element())
            .collect()
    }

    fn render_queue_row(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let waiting = self.base.waiting;
        let line = div()
            .w_full()
            .flex()
            .items_center()
            .gap(ROW_GAP)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(if waiting > 0 {
                        palette.text_primary
                    } else {
                        palette.text_faint
                    })
                    .child(tr!("overlays_queue_waiting", count = waiting as i64)),
            )
            .child(
                ghost_button_with_icon(Icon::Trash, tr!("overlays_queue_clear"), palette)
                    .disabled(waiting == 0)
                    .on_click(
                        "overlays-panel-clear-queue",
                        cx.listener(|this, _: &ClickEvent, _, _| this.clear_queue()),
                    ),
            );
        field_label(palette, tr!("overlays_queue_label").to_uppercase(), line)
            .tone(palette.text_faint)
            .into_any_element()
    }

    fn render_fire_row(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let button = ghost_button_with_icon(Icon::PlayerPlay, tr!("overlays_show_fire"), palette)
            .ink(palette.brand)
            .full_width()
            .disabled(self.base.in_flight())
            .on_click(
                "overlays-panel-fire-show",
                cx.listener(|this, _: &ClickEvent, _, cx| this.fire_show(cx)),
            );
        let status = fire_status(&self.base.fire, self.base.queues_shows())
            .map(|(text, warn)| status_line(text, warn, palette));
        div()
            .flex()
            .flex_col()
            .child(button)
            .children(status)
            .into_any_element()
    }

    pub(super) fn render_receiver_section(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(ROW_GAP)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(tr!("overlays_receiver_toggle")),
            )
            .child(toggle(self.base.receiver, palette).on_click(
                "overlays-panel-receiver",
                move |_: &ClickEvent, _window: &mut Window, cx: &mut App| {
                    view.update(cx, |this, cx| this.toggle_receiver(cx));
                },
            ));
        let hint = if self.base.receiver {
            tr!("overlays_receiver_hint_on")
        } else {
            tr!("overlays_receiver_hint_off")
        };

        div()
            .flex()
            .flex_col()
            .pt(SECTION_TOP_GAP)
            .child(div().pb(SECTION_GAP).child(section_label(
                tr!("overlays_panel_section_receiver").to_uppercase(),
                palette,
            )))
            .child(div().pb(FIELD_GAP).child(hinted(row, hint, palette)))
            .into_any_element()
    }
}

fn display_note(look: &LookSummary) -> Option<String> {
    if !look.draws {
        return Some(tr!("overlays_display_draws_nothing"));
    }
    match look.disposition {
        DeliveryDisposition::Transient => None,
        DeliveryDisposition::Replace => Some(tr!("overlays_display_replace")),
        DeliveryDisposition::Append => Some(tr!("overlays_display_append")),
    }
}

fn delivery_text(delivery: OverlayDelivery) -> (String, bool) {
    match delivery {
        OverlayDelivery::Delivered { sources } => (
            tr!("overlays_show_delivered", count = sources as i64),
            false,
        ),
        OverlayDelivery::OnlyPreview { tabs } => {
            (tr!("overlays_show_preview_only", count = tabs as i64), true)
        }
        OverlayDelivery::NoPage => (tr!("overlays_show_no_page"), true),
    }
}

fn fire_status(fire: &ShowFire, queues: bool) -> Option<(String, bool)> {
    match fire {
        ShowFire::Idle => Some((
            if queues {
                tr!("overlays_show_fire_hint")
            } else {
                tr!("overlays_show_fire_hint_arrival")
            },
            false,
        )),
        ShowFire::Sending => Some((tr!("overlays_show_sending"), false)),
        ShowFire::Playing(delivery) | ShowFire::Applied(delivery) => Some(delivery_text(*delivery)),
        ShowFire::Ended(ShowEnd::Shown(_)) => Some((tr!("overlays_show_ended"), false)),
        ShowFire::Ended(ShowEnd::Cleared) => Some((tr!("overlays_show_cleared"), true)),
        ShowFire::Ended(ShowEnd::Withdrawn) => Some((tr!("overlays_show_withdrawn"), true)),
        ShowFire::Failed(message) => Some((message.clone(), true)),
    }
}

fn hint_text(text: String, palette: &ForgePalette) -> gpui::Div {
    div()
        .pt(HINT_TOP)
        .font_family(body_family())
        .text_size(FONT_XXS)
        .line_height(NOTICE_LINE_H)
        .text_color(palette.text_faint)
        .child(text)
}

pub(super) fn hinted(
    control: impl IntoElement,
    hint: String,
    palette: &ForgePalette,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .child(control)
        .child(hint_text(hint, palette))
        .into_any_element()
}

fn status_line(text: String, warn: bool, palette: &ForgePalette) -> AnyElement {
    let tone = if warn {
        palette.warning
    } else {
        palette.text_faint
    };
    hint_text(text, palette).text_color(tone).into_any_element()
}

fn note_line(text: String, palette: &ForgePalette) -> AnyElement {
    div()
        .font_family(body_family())
        .text_size(FONT_XXS)
        .line_height(NOTICE_LINE_H)
        .text_color(palette.text_muted)
        .child(text)
        .into_any_element()
}

fn own_hide_flag(palette: &ForgePalette) -> AnyElement {
    div()
        .flex()
        .items_start()
        .gap(ROW_GAP)
        .p(NOTICE_PAD)
        .rounded(NOTICE_RADIUS)
        .border(BORDER_THIN)
        .border_color(palette.warning)
        .bg(palette.base)
        .child(icon(Icon::AlertTriangle, FLAG_GLYPH, palette.warning))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .font_family(body_family())
                .text_size(FONT_XXS)
                .line_height(NOTICE_LINE_H)
                .text_color(palette.text_muted)
                .child(tr!("overlays_display_own_hide")),
        )
        .into_any_element()
}
