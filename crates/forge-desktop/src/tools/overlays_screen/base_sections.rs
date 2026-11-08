use crate::shared::option_choices::ChoiceOption;
use forge_components::Density;
use forge_components::Spacing;
use forge_components::spacing;
use forge_components::{
    ActivePresentation, BORDER_THIN, FONT_XS, FONT_XXS, ForgePalette, Icon, Picker, PickerEvent,
    PickerItem, PickerLabels, field_label, ghost_button_with_icon, icon, scaled, section_label,
    toggle, tr,
};
use forge_overlay::config::{
    BACKGROUND_COLOR, BACKGROUND_OPACITY, BORDER_COLOR, CARD_MAX_HEIGHT, CARD_MAX_WIDTH,
    CARD_MIN_HEIGHT, CARD_MIN_WIDTH, COMMAND_PREFIX, DESIGN_HEIGHT, DESIGN_WIDTH, DURATION,
    HEADLINE, HIDDEN_ACCOUNTS, HIDE_COMMANDS, HIDE_ROWS, HIDE_ROWS_AFTER_SECS, IMAGE, IMAGE_HEIGHT,
    IMAGE_POSITION, KICK_ICON, LABEL_OFFSET_X, LABEL_OFFSET_Y, LABEL_SIDE, MAIN_TEXT_COLOR,
    MARGIN_BOTTOM, MARGIN_LEFT, MARGIN_RIGHT, MARGIN_TOP, ROW_LIMIT, ROW_WIDTH, SHOW_AVATARS,
    SOUND, SPEECH, SPEECH_VOICE, SUBLINE, TEXT_COLOR, TEXT_OVERFLOW, THIRD_PARTY_EMOTES,
    TWITCH_ICON, YOUTUBE_ICON,
};
use forge_overlay::{ConfigSection, DeliveryDisposition, OverlayConfig};
use forge_runtime::{OverlayDelivery, OverlayDispatch, ShowEnd, ShowTicket};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, Pixels, SharedString, Subscription, Task, Window,
    div, prelude::*, px,
};

use super::property_panel::{
    FIELD_GAP, NOTICE_LINE_H, NOTICE_PAD, NOTICE_RADIUS, OverlayPropertyPanel, SECTION_GAP,
    SECTION_TOP_GAP,
};
use super::sizing_section::with_source_box;
use crate::app::async_bridge;
use crate::shared::config_form::{
    ChoiceDropdown, ConfigField, ConfigFieldHandlers, render_config_control,
};
use crate::shared::repo_failure::RepoFailure;

const LOOK_FIELD_KEY: &str = "look";
const ROW_GAP: Pixels = px(8.0);
const FLAG_GLYPH: Pixels = px(12.0);

pub(super) const FIELD_SECTIONS: [ConfigSection; 8] = [
    ConfigSection::Content,
    ConfigSection::Text,
    ConfigSection::Card,
    ConfigSection::Placement,
    ConfigSection::Motion,
    ConfigSection::Chat,
    ConfigSection::Audio,
    ConfigSection::Timing,
];

pub(super) fn section_heading(section: ConfigSection) -> String {
    match section {
        ConfigSection::Content => tr!("overlays_panel_section_content"),
        ConfigSection::Text => tr!("overlays_panel_section_text"),
        ConfigSection::Card => tr!("overlays_panel_section_card"),
        ConfigSection::Placement => tr!("overlays_panel_section_placement"),
        ConfigSection::Motion => tr!("overlays_panel_section_motion"),
        ConfigSection::Chat => tr!("overlays_panel_section_chat"),
        ConfigSection::Audio => tr!("overlays_panel_section_audio"),
        ConfigSection::Timing => tr!("overlays_panel_section_timing"),
    }
}

pub(super) fn base_label(key: &str) -> Option<String> {
    match key {
        SOUND => Some(tr!("overlays_base_sound")),
        SPEECH => Some(tr!("overlays_base_speech")),
        SPEECH_VOICE => Some(tr!("overlays_base_voice")),
        DURATION => Some(tr!("overlays_base_duration")),
        MAIN_TEXT_COLOR => Some(tr!("overlays_main_text_color")),
        TEXT_COLOR => Some(tr!("overlays_text_color")),
        BORDER_COLOR => Some(tr!("overlays_border_color")),
        BACKGROUND_COLOR => Some(tr!("overlays_alert_background_color")),
        BACKGROUND_OPACITY => Some(tr!("overlays_alert_background_opacity")),
        IMAGE => Some(tr!("overlays_alert_image")),
        IMAGE_POSITION => Some(tr!("overlays_alert_image_position")),
        IMAGE_HEIGHT => Some(tr!("overlays_alert_image_height")),
        CARD_MIN_WIDTH => Some(tr!("overlays_alert_card_min_width")),
        CARD_MIN_HEIGHT => Some(tr!("overlays_alert_card_min_height")),
        CARD_MAX_WIDTH => Some(tr!("overlays_alert_card_max_width")),
        CARD_MAX_HEIGHT => Some(tr!("overlays_alert_card_max_height")),
        TEXT_OVERFLOW => Some(tr!("overlays_alert_text_overflow")),
        ROW_LIMIT => Some(tr!("overlays_chat_row_limit")),
        ROW_WIDTH => Some(tr!("overlays_chat_row_width")),
        HIDE_ROWS => Some(tr!("overlays_chat_hide_rows")),
        HIDE_ROWS_AFTER_SECS => Some(tr!("overlays_chat_hide_rows_after")),
        HIDE_COMMANDS => Some(tr!("overlays_chat_hide_commands")),
        COMMAND_PREFIX => Some(tr!("overlays_chat_command_prefix")),
        HIDDEN_ACCOUNTS => Some(tr!("overlays_chat_hidden_accounts")),
        SHOW_AVATARS => Some(tr!("overlays_chat_show_avatars")),
        TWITCH_ICON => Some(tr!("overlays_chat_twitch_icon")),
        YOUTUBE_ICON => Some(tr!("overlays_chat_youtube_icon")),
        KICK_ICON => Some(tr!("overlays_chat_kick_icon")),
        THIRD_PARTY_EMOTES => Some(tr!("overlays_chat_third_party_emotes")),
        LABEL_SIDE => Some(tr!("overlays_frame_label_side")),
        LABEL_OFFSET_X => Some(tr!("overlays_frame_label_offset_x")),
        LABEL_OFFSET_Y => Some(tr!("overlays_frame_label_offset_y")),
        DESIGN_WIDTH => Some(tr!("overlays_source_width")),
        DESIGN_HEIGHT => Some(tr!("overlays_source_height")),
        MARGIN_TOP => Some(tr!("overlays_source_margin_top")),
        MARGIN_RIGHT => Some(tr!("overlays_source_margin_right")),
        MARGIN_BOTTOM => Some(tr!("overlays_source_margin_bottom")),
        MARGIN_LEFT => Some(tr!("overlays_source_margin_left")),
        _ => None,
    }
}

pub(super) fn base_hint(key: &str) -> Option<String> {
    match key {
        SPEECH => Some(tr!("overlays_base_speech_hint")),
        SPEECH_VOICE => Some(tr!("overlays_base_voice_hint")),
        DURATION => Some(tr!("overlays_base_duration_hint")),
        BACKGROUND_COLOR => Some(tr!("overlays_alert_background_color_hint")),
        IMAGE => Some(tr!("overlays_alert_image_hint")),
        CARD_MAX_HEIGHT => Some(tr!("overlays_alert_card_size_hint")),
        TEXT_OVERFLOW => Some(tr!("overlays_alert_text_overflow_hint")),
        ROW_LIMIT => Some(tr!("overlays_chat_row_limit_hint")),
        ROW_WIDTH => Some(tr!("overlays_chat_row_width_hint")),
        HIDE_ROWS => Some(tr!("overlays_chat_hide_rows_hint")),
        HIDE_COMMANDS => Some(tr!("overlays_chat_hide_commands_hint")),
        HIDDEN_ACCOUNTS => Some(tr!("overlays_chat_hidden_accounts_hint")),
        SHOW_AVATARS => Some(tr!("overlays_chat_show_avatars_hint")),
        TWITCH_ICON | YOUTUBE_ICON | KICK_ICON => Some(tr!("overlays_chat_platform_icon_hint")),
        THIRD_PARTY_EMOTES => Some(tr!("overlays_chat_third_party_emotes_hint")),
        LABEL_OFFSET_X => Some(tr!("overlays_frame_label_offset_x_hint")),
        LABEL_OFFSET_Y => Some(tr!("overlays_frame_label_offset_y_hint")),
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
    pub(super) slot_bound: bool,
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
    Withheld,
}

enum ShowFire {
    Idle,
    Sending,
    Playing(OverlayDelivery),
    Applied(OverlayDelivery),
    Withheld,
    Ended(ShowEnd),
    Failed(String),
}

struct LookPicker {
    picker: Entity<Picker>,
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
                slot_bound: false,
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

    pub(super) fn slot_bound(&self) -> bool {
        self.look.slot_bound
    }

    pub(super) fn default_text_hint(&self, key: &str) -> Option<String> {
        if self.slot_bound() {
            return None;
        }
        let kept_on_screen = self.look.disposition.retains_last_content();
        match (key, kept_on_screen) {
            (HEADLINE, false) => Some(tr!("overlays_content_headline_hint")),
            (HEADLINE, true) => Some(tr!("overlays_content_headline_kept_hint")),
            (SUBLINE, false) => Some(tr!("overlays_content_subline_hint")),
            (SUBLINE, true) => Some(tr!("overlays_content_subline_kept_hint")),
            _ => None,
        }
    }

    fn in_flight(&self) -> bool {
        matches!(self.fire, ShowFire::Sending | ShowFire::Playing(_))
    }
}

impl OverlayPropertyPanel {
    pub(super) fn adopt_base(&mut self, launch: BaseLaunch, cx: &mut Context<Self>) {
        self.base = BaseState::new(launch);
        self.mark_default_text_placeholders(cx);
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

    fn mark_default_text_placeholders(&self, cx: &mut Context<Self>) {
        if self.base.slot_bound() {
            return;
        }
        for field in &self.fields {
            if let ConfigField::Input { key, input, .. } = field
                && [HEADLINE, SUBLINE].contains(&key.as_str())
            {
                input.update(cx, |input, cx| {
                    input.set_placeholder(tr!("overlays_content_default_placeholder"), cx);
                });
            }
        }
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

    fn fire_show(&mut self, cx: &mut Context<Self>) {
        if self.base.in_flight() {
            return;
        }
        self.emit_save(cx);
        self.base.fire_epoch = self.base.fire_epoch.wrapping_add(1);
        let epoch = self.base.fire_epoch;
        self.base.fire = ShowFire::Sending;

        let service = self.service.clone();
        let samples = self.samples.clone();
        let id = self.overlay_id.clone();
        if self.base.slot_bound() {
            async_bridge::run_async(
                &self.rt_handle,
                async move {
                    service
                        .test_fire(&id, samples.as_ref())
                        .await
                        .map(|fired| Fired::Applied(fired.delivery))
                        .map_err(RepoFailure::from)
                },
                move |this, result, cx| this.on_show_fired(epoch, result, cx),
                cx,
            );
            cx.notify();
            return;
        }
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let args = samples.sample(&id).await.args();
                match service
                    .preview_send(
                        &id,
                        &OverlayConfig::new(),
                        &args,
                        forge_runtime::OverlaySendOptions::default(),
                    )
                    .await
                {
                    Ok(OverlayDispatch::Applied(delivery)) => Ok(Fired::Applied(delivery)),
                    Ok(OverlayDispatch::Queued(ticket)) => {
                        let delivery = service.receivers(&id).await.outcome();
                        Ok(Fired::Queued(delivery, ticket))
                    }
                    Ok(OverlayDispatch::Withheld) => Ok(Fired::Withheld),
                    Err(error) => Err(RepoFailure::from(error)),
                }
            },
            move |this, result, cx| this.on_show_fired(epoch, result, cx),
            cx,
        );
        cx.notify();
    }

    fn on_show_fired(
        &mut self,
        epoch: u64,
        result: Result<Fired, RepoFailure>,
        cx: &mut Context<Self>,
    ) {
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
            Ok(Fired::Withheld) => self.base.fire = ShowFire::Withheld,
            Err(message) => self.base.fire = ShowFire::Failed(message.text()),
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

    fn open_look(&mut self, _key: String, window: &mut Window, cx: &mut Context<Self>) {
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
                icon: Some(look.icon),
            })
            .collect();
        let labels = PickerLabels {
            placeholder: tr!("widget_picker_search_placeholder").into(),
            empty: tr!("overlays_panel_choice_empty").into(),
            loading: tr!("widget_picker_loading").into(),
        };
        let current = Some(SharedString::from(self.base.look.kind_id.clone()));
        let picker = cx.new(|cx| Picker::new(labels, items, cx).with_current(current));
        let sub = cx.subscribe(&picker, Self::on_look_picked);
        picker.update(cx, |picker, cx| picker.focus(window, cx));
        self.base.picker = Some(LookPicker { picker, _sub: sub });
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
                config: with_source_box(&self.defaults, config),
            });
        }
        cx.notify();
    }

    fn close_look(&mut self, cx: &mut Context<Self>) {
        self.base.picker = None;
        cx.notify();
    }

    pub(super) fn render_look_section(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let field = ConfigField::Choice {
            key: LOOK_FIELD_KEY.to_owned(),
            gate: None,
            options: self
                .base
                .looks
                .iter()
                .map(|look| ChoiceOption::new(look.kind_id.clone(), look.label.clone()))
                .collect(),
            selected: self.base.look.kind_id.clone(),
            options_key: None,
            dependency: None,
        };
        let handlers = ConfigFieldHandlers {
            toggle: Self::ignore_toggle,
            slide: Self::ignore_slide,
            pick: Self::ignore_pick,
            choice: Some(ChoiceDropdown {
                open: Self::open_look,
                close: Self::close_look,
                active: self
                    .base
                    .picker
                    .as_ref()
                    .map(|open| (LOOK_FIELD_KEY.to_owned(), open.picker.clone())),
            }),
        };
        let view = cx.entity();
        let control = render_config_control(
            &field,
            palette,
            "overlays-panel",
            &view,
            &handlers,
            density,
            cx,
        );

        div()
            .flex()
            .flex_col()
            .child(div().pb(SECTION_GAP).child(section_label(
                tr!("overlays_panel_section_look").to_uppercase(),
                palette,
                cx,
            )))
            .child(div().pb(scaled(FIELD_GAP, density)).child(hinted(
                control,
                tr!("overlays_look_hint"),
                palette,
                density,
                cx,
            )))
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
        let density = cx.density();
        let mut rows = Vec::new();
        if let Some(note) = display_note(&self.base.look) {
            rows.push(note_line(note, palette, cx));
        }
        if self.base.hides_itself && self.base.queues_shows() {
            rows.push(own_hide_flag(palette, density, cx));
        }
        if self.base.queues_shows() {
            rows.push(self.render_queue_row(palette, cx));
        }
        rows.push(self.render_fire_row(palette, cx));
        rows.into_iter()
            .map(|row| {
                div()
                    .pb(scaled(FIELD_GAP, density))
                    .child(row)
                    .into_any_element()
            })
            .collect()
    }

    fn render_queue_row(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let waiting = self.base.waiting;
        let line = div()
            .w_full()
            .flex()
            .items_center()
            .gap(scaled(ROW_GAP, density))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .font_family(cx.body_family())
                    .text_size(FONT_XS)
                    .text_color(if waiting > 0 {
                        palette.text_primary
                    } else {
                        palette.text_faint
                    })
                    .child(tr!(
                        "overlays_queue_waiting",
                        count = i64::try_from(waiting).unwrap_or(i64::MAX)
                    )),
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
        let density = cx.density();
        let button = ghost_button_with_icon(Icon::PlayerPlay, tr!("overlays_show_fire"), palette)
            .ink(palette.brand)
            .full_width()
            .disabled(self.base.in_flight())
            .on_click(
                "overlays-panel-fire-show",
                cx.listener(|this, _: &ClickEvent, _, cx| this.fire_show(cx)),
            );
        let (text, warn) = fire_status(
            &self.base.fire,
            self.base.queues_shows(),
            self.base.slot_bound(),
        );
        let status = status_line(text, warn, palette, density, cx);
        div()
            .flex()
            .flex_col()
            .child(button)
            .child(status)
            .into_any_element()
    }

    pub(super) fn render_receiver_section(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let view = cx.entity();
        let row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(scaled(ROW_GAP, density))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .font_family(cx.body_family())
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
                cx,
            )))
            .child(
                div()
                    .pb(scaled(FIELD_GAP, density))
                    .child(hinted(row, hint, palette, density, cx)),
            )
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
            tr!(
                "overlays_show_delivered",
                count = i64::try_from(sources).unwrap_or(i64::MAX)
            ),
            false,
        ),
        OverlayDelivery::OnlyPreview { tabs } => (
            tr!(
                "overlays_show_preview_only",
                count = i64::try_from(tabs).unwrap_or(i64::MAX)
            ),
            true,
        ),
        OverlayDelivery::NoPage => (tr!("overlays_show_no_page"), true),
    }
}

fn fire_status(fire: &ShowFire, queues: bool, slot_bound: bool) -> (String, bool) {
    match fire {
        ShowFire::Idle => (
            if slot_bound {
                tr!("overlays_show_fire_hint_latest")
            } else if queues {
                tr!("overlays_show_fire_hint")
            } else {
                tr!("overlays_show_fire_hint_arrival")
            },
            false,
        ),
        ShowFire::Sending => (tr!("overlays_show_sending"), false),
        ShowFire::Playing(delivery) | ShowFire::Applied(delivery) => delivery_text(*delivery),
        ShowFire::Withheld => (tr!("overlays_show_withheld"), true),
        ShowFire::Ended(ShowEnd::Shown(_)) => (tr!("overlays_show_ended"), false),
        ShowFire::Ended(ShowEnd::Cleared) => (tr!("overlays_show_cleared"), true),
        ShowFire::Ended(ShowEnd::Withdrawn) => (tr!("overlays_show_withdrawn"), true),
        ShowFire::Failed(message) => (message.clone(), true),
    }
}

fn hint_text(text: String, palette: &ForgePalette, density: Density, cx: &App) -> gpui::Div {
    div()
        .pt(spacing(Spacing::Xxs, density))
        .font_family(cx.body_family())
        .text_size(FONT_XXS)
        .line_height(NOTICE_LINE_H)
        .text_color(palette.text_faint)
        .child(text)
}

pub(super) fn hinted(
    control: impl IntoElement,
    hint: String,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .child(control)
        .child(hint_text(hint, palette, density, cx))
        .into_any_element()
}

fn status_line(
    text: String,
    warn: bool,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    let tone = if warn {
        palette.warning
    } else {
        palette.text_faint
    };
    hint_text(text, palette, density, cx)
        .text_color(tone)
        .into_any_element()
}

fn note_line(text: String, palette: &ForgePalette, cx: &App) -> AnyElement {
    div()
        .font_family(cx.body_family())
        .text_size(FONT_XXS)
        .line_height(NOTICE_LINE_H)
        .text_color(palette.text_muted)
        .child(text)
        .into_any_element()
}

fn own_hide_flag(palette: &ForgePalette, density: Density, cx: &App) -> AnyElement {
    div()
        .flex()
        .items_start()
        .gap(scaled(ROW_GAP, density))
        .p(scaled(NOTICE_PAD, density))
        .rounded(NOTICE_RADIUS)
        .border(BORDER_THIN)
        .border_color(palette.warning)
        .bg(palette.base)
        .child(icon(Icon::AlertTriangle, FLAG_GLYPH, palette.warning))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .font_family(cx.body_family())
                .text_size(FONT_XXS)
                .line_height(NOTICE_LINE_H)
                .text_color(palette.text_muted)
                .child(tr!("overlays_display_own_hide")),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use forge_overlay::OverlayKindDescriptor;
    use forge_overlay::kinds::alert::AlertOverlayKind;
    use forge_overlay::kinds::card::CardOverlayKind;
    use forge_overlay::kinds::latest::LatestOverlayKind;

    use super::super::look_change::look_summary;
    use super::*;

    fn base_of(descriptor: &dyn OverlayKindDescriptor) -> BaseState {
        BaseState::new(BaseLaunch {
            look: look_summary(descriptor),
            looks: Vec::new(),
            receiver: false,
        })
    }

    #[test]
    fn default_text_hints_say_kept_on_screen_only_for_looks_that_keep_their_content() {
        let card = base_of(&CardOverlayKind);
        let alert = base_of(&AlertOverlayKind);

        assert_eq!(
            [
                card.default_text_hint(HEADLINE),
                card.default_text_hint(SUBLINE)
            ],
            [
                Some(tr!("overlays_content_headline_kept_hint")),
                Some(tr!("overlays_content_subline_kept_hint")),
            ],
            "a card keeps its content, so its default shows until a step replaces it"
        );
        assert_eq!(
            [
                alert.default_text_hint(HEADLINE),
                alert.default_text_hint(SUBLINE)
            ],
            [
                Some(tr!("overlays_content_headline_hint")),
                Some(tr!("overlays_content_subline_hint")),
            ],
            "an alert shows nothing between shows, so its default is never on screen alone"
        );
    }

    #[test]
    fn no_default_text_hint_is_offered_for_a_slot_bound_look_or_a_non_text_field() {
        let latest = base_of(&LatestOverlayKind);
        let card = base_of(&CardOverlayKind);

        for (case, hint) in [
            ("latest headline", latest.default_text_hint(HEADLINE)),
            ("latest subline", latest.default_text_hint(SUBLINE)),
            ("card text color", card.default_text_hint(TEXT_COLOR)),
        ] {
            assert_eq!(hint, None, "{case} was given a default-text hint");
        }
    }

    #[test]
    fn an_idle_show_button_explains_the_return_to_the_current_value_only_for_slot_bound_looks() {
        let hint = |queues, slot_bound| fire_status(&ShowFire::Idle, queues, slot_bound);

        assert_ne!(hint(true, true), hint(true, false));
        assert_eq!(hint(true, true), hint(false, true));
    }
}
