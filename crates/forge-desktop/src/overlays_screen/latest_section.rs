use std::sync::Arc;

use forge_components::{
    BORDER_THIN, ConfirmTone, FONT_XS, FONT_XXS, ForgePalette, Icon, OverlayPosition, body_family,
    confirm_modal, ghost_button_with_icon, overlay, section_label, tr,
};
use forge_events::{Event, LatestChanged};
use forge_overlay::config::{HEADLINE, PLACEHOLDER, PLATFORM, SUBLINE};
use forge_overlay::{
    LatestBinding, OverlayConfig, OverlayKindRegistry, latest_binding, latest_content,
};
use forge_runtime::{EventBus, LatestValues};
use forge_types::Variant;
use gpui::{AnyElement, App, ClickEvent, Context, FontWeight, Pixels, Task, div, prelude::*, px};

use super::OverlaysView;
use super::property_panel::{
    FIELD_GAP, NOTICE_LINE_H, NOTICE_PAD, NOTICE_RADIUS, OverlayPropertyPanel, SECTION_GAP,
    SECTION_TOP_GAP,
};
use crate::async_bridge::{self, BridgeFlow};
use crate::config_form::ConfigField;
use crate::latest_labels::slot_label;

const VALUE_GAP: Pixels = px(2.0);
const STATUS_TOP: Pixels = px(4.0);
const NOTHING_SHOWN: &str = "-";

#[derive(Clone)]
pub(super) struct LatestSource {
    pub(super) values: LatestValues,
    pub(super) bus: Arc<EventBus>,
    pub(super) kinds: Arc<OverlayKindRegistry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CurrentValue {
    Filled {
        headline: String,
        subline: String,
        platform: String,
    },
    Empty {
        placeholder: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ResetStage {
    Idle,
    Confirming,
    Resetting,
    Failed(String),
}

pub(super) struct LatestState {
    source: Option<LatestSource>,
    binding: Option<LatestBinding>,
    current: Option<CurrentValue>,
    read_epoch: u64,
    reset: ResetStage,
    _changes: Option<Task<()>>,
}

impl Default for LatestState {
    fn default() -> Self {
        Self {
            source: None,
            binding: None,
            current: None,
            read_epoch: 0,
            reset: ResetStage::Idle,
            _changes: None,
        }
    }
}

impl LatestState {
    pub(super) fn is_bound(&self) -> bool {
        self.source.is_some()
    }

    pub(super) fn current(&self) -> Option<&CurrentValue> {
        self.current.as_ref()
    }

    pub(super) fn reset_stage(&self) -> &ResetStage {
        &self.reset
    }

    fn can_reset(&self) -> bool {
        matches!(self.current, Some(CurrentValue::Filled { .. }))
            && !matches!(self.reset, ResetStage::Resetting | ResetStage::Confirming)
    }
}

pub(super) fn changed_slots(events: &[Event]) -> Vec<String> {
    events
        .iter()
        .filter_map(LatestChanged::from_event)
        .map(|changed| changed.slot)
        .collect()
}

pub(super) fn current_value(content: &OverlayConfig, platform: Option<String>) -> CurrentValue {
    let text = |key: &str| {
        content
            .get(key)
            .and_then(Variant::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    match platform {
        Some(platform) => CurrentValue::Filled {
            headline: text(HEADLINE),
            subline: text(SUBLINE),
            platform,
        },
        None => CurrentValue::Empty {
            placeholder: text(PLACEHOLDER),
        },
    }
}

impl OverlayPropertyPanel {
    pub(super) fn bind_latest(&mut self, source: LatestSource, cx: &mut Context<Self>) {
        if !self.base.slot_bound() {
            return;
        }
        self.localize_platform_placeholder(cx);
        let bus = Arc::clone(&source.bus);
        self.latest._changes = Some(cx.spawn(async move |this, cx| {
            async_bridge::drain_events(&bus, cx, |events, cx| {
                let slots = changed_slots(events);
                if slots.is_empty() {
                    return BridgeFlow::Continue;
                }
                match this.update(cx, |this, cx| this.on_latest_changed(&slots, cx)) {
                    Ok(()) => BridgeFlow::Continue,
                    Err(_) => BridgeFlow::Stop,
                }
            })
            .await;
        }));
        self.latest.source = Some(source);
        self.refresh_latest(cx);
    }

    fn localize_platform_placeholder(&self, cx: &mut Context<Self>) {
        for field in &self.fields {
            if let ConfigField::Input { key, input, .. } = field
                && key == PLATFORM
            {
                input.update(cx, |input, cx| {
                    input.set_placeholder(tr!("latest_platform_all"), cx);
                });
            }
        }
    }

    fn current_binding(&self, cx: &App) -> Option<LatestBinding> {
        let source = self.latest.source.as_ref()?;
        let descriptor = source.kinds.get(self.base.kind_id())?;
        latest_binding(descriptor, &self.pending_config(cx))
    }

    pub(super) fn refresh_latest(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.latest.source.clone() else {
            return;
        };
        let Some(binding) = self.current_binding(cx) else {
            return;
        };
        self.latest.binding = Some(binding.clone());
        self.latest.read_epoch = self.latest.read_epoch.wrapping_add(1);
        let epoch = self.latest.read_epoch;
        let kind_id = self.base.kind_id().to_owned();
        let stored = self.pending_config(cx);

        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let value = source.values.latest(&binding.slot, binding.scope());
                let descriptor = source.kinds.get(&kind_id)?;
                let content = latest_content(descriptor, &stored, value.as_ref());
                Some(current_value(&content, value.map(|value| value.platform)))
            },
            move |this, current, cx| this.on_latest_read(epoch, current, cx),
            cx,
        );
    }

    fn on_latest_read(
        &mut self,
        epoch: u64,
        current: Option<CurrentValue>,
        cx: &mut Context<Self>,
    ) {
        if self.latest.read_epoch != epoch {
            return;
        }
        self.latest.current = current;
        cx.notify();
    }

    fn on_latest_changed(&mut self, slots: &[String], cx: &mut Context<Self>) {
        let watched = self
            .latest
            .binding
            .as_ref()
            .is_some_and(|binding| slots.contains(&binding.slot));
        if watched {
            self.refresh_latest(cx);
        }
    }

    fn request_reset(&mut self, cx: &mut Context<Self>) {
        if !self.latest.can_reset() {
            return;
        }
        self.latest.reset = ResetStage::Confirming;
        cx.notify();
    }

    pub(super) fn cancel_reset(&mut self, cx: &mut Context<Self>) {
        if self.latest.reset != ResetStage::Confirming {
            return;
        }
        self.latest.reset = ResetStage::Idle;
        cx.notify();
    }

    pub(super) fn confirm_reset(&mut self, cx: &mut Context<Self>) {
        if self.latest.reset != ResetStage::Confirming {
            return;
        }
        let (Some(source), Some(binding)) =
            (self.latest.source.clone(), self.latest.binding.clone())
        else {
            self.latest.reset = ResetStage::Idle;
            cx.notify();
            return;
        };
        self.latest.reset = ResetStage::Resetting;
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                source
                    .values
                    .reset(&binding.slot)
                    .await
                    .map_err(|error| error.to_string())
            },
            |this, result, cx| this.on_reset_done(result, cx),
            cx,
        );
        cx.notify();
    }

    fn on_reset_done(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        self.latest.reset = match result {
            Ok(()) => ResetStage::Idle,
            Err(message) => ResetStage::Failed(message),
        };
        self.refresh_latest(cx);
        cx.notify();
    }

    pub(super) fn reset_prompt(&self) -> Option<String> {
        if self.latest.reset != ResetStage::Confirming {
            return None;
        }
        let slot = &self.latest.binding.as_ref()?.slot;
        Some(slot_label(slot).unwrap_or_else(|| slot.clone()))
    }

    pub(super) fn render_latest_section(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.latest.is_bound() {
            return None;
        }
        let reset = ghost_button_with_icon(Icon::Eraser, tr!("overlays_latest_reset"), palette)
            .full_width()
            .disabled(!self.latest.can_reset())
            .on_click(
                "overlays-panel-latest-reset",
                cx.listener(|this, _: &ClickEvent, _, cx| this.request_reset(cx)),
            );
        let (status, warn) = match self.latest.reset_stage() {
            ResetStage::Resetting => (tr!("overlays_latest_resetting"), false),
            ResetStage::Failed(message) => (message.clone(), true),
            ResetStage::Idle | ResetStage::Confirming => (tr!("overlays_latest_reset_hint"), false),
        };

        Some(
            div()
                .flex()
                .flex_col()
                .pt(SECTION_TOP_GAP)
                .child(div().pb(SECTION_GAP).child(section_label(
                    tr!("overlays_latest_section").to_uppercase(),
                    palette,
                )))
                .child(
                    div()
                        .pb(SECTION_GAP)
                        .child(value_card(self.latest.current(), palette)),
                )
                .child(
                    div().pb(FIELD_GAP).child(reset).child(
                        div()
                            .pt(STATUS_TOP)
                            .font_family(body_family())
                            .text_size(FONT_XXS)
                            .line_height(NOTICE_LINE_H)
                            .text_color(if warn {
                                palette.warning
                            } else {
                                palette.text_faint
                            })
                            .child(status),
                    ),
                )
                .into_any_element(),
        )
    }
}

fn value_card(current: Option<&CurrentValue>, palette: &ForgePalette) -> AnyElement {
    let card = div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(VALUE_GAP)
        .p(NOTICE_PAD)
        .rounded(NOTICE_RADIUS)
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .bg(palette.base)
        .font_family(body_family());

    let line = |text: String, size, color| {
        div()
            .min_w(px(0.0))
            .truncate()
            .text_size(size)
            .line_height(NOTICE_LINE_H)
            .text_color(color)
            .child(text)
    };

    match current {
        Some(CurrentValue::Filled {
            headline,
            subline,
            platform,
        }) => card
            .child(
                line(headline.clone(), FONT_XS, palette.text_primary)
                    .font_weight(FontWeight::SEMIBOLD),
            )
            .when(!subline.is_empty(), |card| {
                card.child(line(subline.clone(), FONT_XXS, palette.text_muted))
            })
            .child(line(
                tr!("overlays_latest_from", platform = platform.as_str()),
                FONT_XXS,
                palette.text_faint,
            ))
            .into_any_element(),
        Some(CurrentValue::Empty { placeholder }) => {
            let text = if placeholder.is_empty() {
                tr!("overlays_latest_empty")
            } else {
                placeholder.clone()
            };
            card.child(line(text, FONT_XS, palette.text_faint))
                .child(
                    div()
                        .min_w(px(0.0))
                        .text_size(FONT_XXS)
                        .line_height(NOTICE_LINE_H)
                        .text_color(palette.text_faint)
                        .child(tr!("overlays_latest_empty_hint")),
                )
                .into_any_element()
        }
        None => card
            .child(line(NOTHING_SHOWN.to_owned(), FONT_XS, palette.text_faint))
            .into_any_element(),
    }
}

impl OverlaysView {
    pub(super) fn render_latest_reset_confirm(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let panel = self.panel.as_ref()?.view.clone();
        let slot = panel.read(cx).reset_prompt()?;

        let cancel_panel = panel.clone();
        let confirm_panel = panel.clone();
        let dismiss_panel = panel;
        let card = confirm_modal(
            tr!("overlays_latest_confirm_title"),
            tr!("overlays_latest_confirm_body"),
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(slot)
        .on_cancel(
            "overlays-latest-reset-cancel",
            tr!("common_cancel"),
            move |_: &ClickEvent, _, cx: &mut App| {
                cancel_panel.update(cx, |panel, cx| panel.cancel_reset(cx));
            },
        )
        .on_confirm(
            "overlays-latest-reset-confirm",
            tr!("overlays_latest_reset"),
            move |_: &ClickEvent, _, cx: &mut App| {
                confirm_panel.update(cx, |panel, cx| panel.confirm_reset(cx));
            },
        );

        Some(
            overlay(card, palette)
                .position(OverlayPosition::Center)
                .on_dismiss("overlays-latest-reset-dismiss", move |_window, cx| {
                    dismiss_panel.update(cx, |panel, cx| panel.cancel_reset(cx));
                })
                .into_any_element(),
        )
    }
}
