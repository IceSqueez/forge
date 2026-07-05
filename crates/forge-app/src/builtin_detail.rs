use std::borrow::Cow;
use std::sync::Arc;

use forge_events::Event;
use forge_obs::{ObsClient, ObsSource};
use forge_platform_core::{
    BuiltinContent, BuiltinControl, BuiltinHealth, BuiltinId, BuiltinStatus, CapabilityFlags,
    DetailSection, HeaderAction, HealthMetric, PickerKind, QuickAction, QuickActions, SectionIcon,
};
use forge_types::Variant;
use forge_widgets::{
    ConfirmKind, ConfirmModalParams, ConfirmTone, ForgePalette, HeaderCardParams, PickerItem,
    PickerModalProps, Spacing, ToastVariant, builtin_content_renderer, builtin_header_card,
    builtin_health_grid, builtin_quick_actions_grid, confirm_modal, picker_modal, sp, spf,
    toast_banner,
};
use iced::widget::container;
use iced::{Alignment, Element, Length, Subscription, Task};

use crate::message::{BuiltinDetailMsg, Message};
use crate::runtime_view::RuntimeView;

pub enum PickerItemsState {
    Idle,
    Loading,
    Loaded(Vec<PickerItem>),
    Failed(String),
}

pub struct PendingPicker {
    pub action_index: usize,
    pub kind: PickerKind,
    pub search: String,
    pub items: PickerItemsState,
    pub current_scene: Option<String>,
}

pub struct BuiltinDetailState {
    pub id: BuiltinId,
    pub builtin_status: Arc<dyn BuiltinStatus>,
    pub builtin_health: Arc<dyn BuiltinHealth>,
    pub builtin_content: Arc<dyn BuiltinContent>,
    pub builtin_quick_actions: Arc<dyn QuickActions>,
    pub builtin_control: Option<Arc<dyn BuiltinControl>>,
    pub health_metrics: [HealthMetric; 4],
    pub pending_picker: Option<PendingPicker>,
    pub quick_action_toast: Option<String>,
    /// Two-phase disconnect gate — armed by the header's Disconnect action,
    /// rendered by the shared `confirm_modal`. `false` = no confirm showing.
    pub pending_disconnect: bool,
    display_name: String,
    version: Option<String>,
    endpoint: Option<String>,
    capability_flags: CapabilityFlags,
    header_actions: Vec<HeaderAction>,
    icon: SectionIcon,
    sections: Vec<DetailSection>,
    quick_actions: Vec<QuickAction>,
}

impl BuiltinDetailState {
    pub fn new(
        id: BuiltinId,
        icon: SectionIcon,
        builtin_status: Arc<dyn BuiltinStatus>,
        builtin_health: Arc<dyn BuiltinHealth>,
        builtin_content: Arc<dyn BuiltinContent>,
        builtin_quick_actions: Arc<dyn QuickActions>,
        builtin_control: Option<Arc<dyn BuiltinControl>>,
    ) -> Self {
        let display_name = builtin_status.display_name().to_owned();
        let version = builtin_status.version().map(ToOwned::to_owned);
        let endpoint = builtin_status.endpoint().map(ToOwned::to_owned);
        let capability_flags = builtin_status.capability_flags();
        let header_actions = builtin_status.header_actions();
        let health_metrics = builtin_health.metrics();
        let sections = builtin_content.sections();
        let quick_actions = builtin_quick_actions.actions();
        Self {
            id,
            builtin_status,
            builtin_health,
            builtin_content,
            builtin_quick_actions,
            builtin_control,
            health_metrics,
            pending_picker: None,
            quick_action_toast: None,
            pending_disconnect: false,
            display_name,
            version,
            endpoint,
            capability_flags,
            header_actions,
            icon,
            sections,
            quick_actions,
        }
    }
}

pub fn on_event(state: Option<&mut BuiltinDetailState>, event: &Event) -> Task<Message> {
    let Some(state) = state else {
        return Task::none();
    };
    if event.kind != "quick_action.done" {
        return Task::none();
    }
    let quick_action_fallback = forge_widgets::tr!("builtin_quick_action_fallback");
    let label = event.payload["label"]
        .as_str()
        .unwrap_or(quick_action_fallback.as_str());
    let outcome = event.payload["outcome"].as_str().unwrap_or("done");
    state.quick_action_toast = Some(if outcome == "success" {
        format!("{label} — done")
    } else {
        format!("{label} — {outcome}")
    });
    Task::none()
}

pub fn update(
    state: &mut Option<BuiltinDetailState>,
    rt: &RuntimeView,
    msg: BuiltinDetailMsg,
) -> Task<Message> {
    let Some(state) = state.as_mut() else {
        return Task::none();
    };
    match msg {
        BuiltinDetailMsg::HealthDelta(delta) => {
            let idx = delta.index as usize;
            if idx < 4 {
                state.health_metrics[idx].value = delta.new_value;
            }
            Task::none()
        }
        BuiltinDetailMsg::HeaderActionClicked(HeaderAction::Disconnect) => {
            // Arms the confirm gate only (PL-03-F6 — was a bare
            // immediate-execute site). The actual disconnect body moved to
            // `DisconnectConfirmAccepted`.
            state.pending_disconnect = true;
            Task::none()
        }
        BuiltinDetailMsg::HeaderActionClicked(action) => {
            let Some(ctrl) = state.builtin_control.clone() else {
                return Task::none();
            };
            match action {
                HeaderAction::Reconnect => Task::perform(
                    async move { ctrl.reconnect().await.map_err(|e| e.to_string()) },
                    |r| Message::BuiltinDetail(BuiltinDetailMsg::ControlResult(r)),
                ),
                HeaderAction::RefreshToken => Task::perform(
                    async move { ctrl.refresh_token().await.map_err(|e| e.to_string()) },
                    |r| Message::BuiltinDetail(BuiltinDetailMsg::ControlResult(r)),
                ),
                HeaderAction::Settings => Task::none(),
                HeaderAction::Disconnect => unreachable!("handled by the arm above"),
            }
        }
        BuiltinDetailMsg::DisconnectConfirmDismissed => {
            state.pending_disconnect = false;
            Task::none()
        }
        BuiltinDetailMsg::DisconnectConfirmAccepted => {
            state.pending_disconnect = false;
            let Some(ctrl) = state.builtin_control.clone() else {
                return Task::none();
            };
            Task::perform(
                async move { ctrl.disconnect().await.map_err(|e| e.to_string()) },
                |r| Message::BuiltinDetail(BuiltinDetailMsg::ControlResult(r)),
            )
        }
        BuiltinDetailMsg::ControlResult(Err(e)) => {
            tracing::warn!(error = %e, "builtin control action failed");
            Task::none()
        }
        BuiltinDetailMsg::ControlResult(Ok(())) => Task::none(),
        BuiltinDetailMsg::QuickActionClicked(idx) => {
            let Some(action) = state.quick_actions.get(idx) else {
                return Task::none();
            };
            if !action.enabled {
                return Task::none();
            }
            let picker_kind = action.picker;
            let spec = action.subaction_template.clone();
            let label = action.label.clone();
            let builtin_id = state.id.as_str().to_owned();

            if let Some(kind) = picker_kind {
                let obs_client = rt.obs_client.clone();
                state.pending_picker = Some(PendingPicker {
                    action_index: idx,
                    kind,
                    search: String::new(),
                    items: PickerItemsState::Loading,
                    current_scene: None,
                });
                match obs_client {
                    Some(client) => {
                        Task::perform(async move { fetch_picker_items(client, kind).await }, |r| {
                            Message::BuiltinDetail(BuiltinDetailMsg::PickerItemsLoaded(r))
                        })
                    }
                    None => {
                        let err_msg = forge_widgets::tr!("builtin_obs_not_connected");
                        Task::perform(
                            async move { Err::<(Vec<PickerItem>, Option<String>), _>(err_msg) },
                            |r| Message::BuiltinDetail(BuiltinDetailMsg::PickerItemsLoaded(r)),
                        )
                    }
                }
            } else {
                let engine = rt.action_engine.clone();
                Task::perform(
                    async move {
                        if let Some(e) = engine {
                            let _ = e.execute_quick_action(spec, builtin_id, label).await;
                        }
                    },
                    |_| Message::Noop,
                )
            }
        }
        BuiltinDetailMsg::PickerSearchChanged(s) => {
            if let Some(pending) = state.pending_picker.as_mut() {
                pending.search = s;
            }
            Task::none()
        }
        BuiltinDetailMsg::PickerItemsLoaded(Ok((items, current_scene))) => {
            if let Some(pending) = state.pending_picker.as_mut() {
                pending.items = PickerItemsState::Loaded(items);
                pending.current_scene = current_scene;
            }
            Task::none()
        }
        BuiltinDetailMsg::PickerItemsLoaded(Err(e)) => {
            if let Some(pending) = state.pending_picker.as_mut() {
                pending.items = PickerItemsState::Failed(e);
            }
            Task::none()
        }
        BuiltinDetailMsg::PickerItemSelected(item_idx) => {
            let (selected_id, action_index, kind, current_scene) = {
                let Some(pending) = state.pending_picker.as_ref() else {
                    return Task::none();
                };
                let PickerItemsState::Loaded(items) = &pending.items else {
                    return Task::none();
                };
                let Some(item) = items.get(item_idx) else {
                    return Task::none();
                };
                (
                    item.id.clone(),
                    pending.action_index,
                    pending.kind,
                    pending.current_scene.clone(),
                )
            };

            state.pending_picker = None;

            let Some(action) = state.quick_actions.get(action_index) else {
                return Task::none();
            };
            let mut spec = action.subaction_template.clone();
            let label = action.label.clone();
            let builtin_id = state.id.as_str().to_owned();

            match kind {
                PickerKind::Scene => {
                    spec.config
                        .insert("scene".to_owned(), Variant::String(selected_id));
                }
                PickerKind::Source => {
                    if let Some(scene) = current_scene {
                        spec.config
                            .insert("scene".to_owned(), Variant::String(scene));
                    }
                    spec.config
                        .insert("source".to_owned(), Variant::String(selected_id));
                }
                PickerKind::Input => {
                    spec.config
                        .insert("source".to_owned(), Variant::String(selected_id));
                }
                PickerKind::Hotkey | PickerKind::Expression | PickerKind::MidiPort => {
                    return Task::none();
                }
            }

            let engine = rt.action_engine.clone();
            Task::perform(
                async move {
                    if let Some(e) = engine {
                        let _ = e.execute_quick_action(spec, builtin_id, label).await;
                    }
                },
                |_| Message::Noop,
            )
        }
        BuiltinDetailMsg::PickerCancelled => {
            state.pending_picker = None;
            Task::none()
        }
        BuiltinDetailMsg::DismissToast => {
            state.quick_action_toast = None;
            Task::none()
        }
    }
}

async fn fetch_picker_items(
    obs_client: Arc<ObsClient>,
    kind: PickerKind,
) -> Result<(Vec<PickerItem>, Option<String>), String> {
    match kind {
        PickerKind::Scene => {
            let scenes = obs_client.scenes().await.map_err(|e| e.to_string())?;
            let items = scenes
                .into_iter()
                .map(|s| PickerItem {
                    id: s.clone(),
                    label: s,
                    sublabel: None,
                    icon: SectionIcon::new("layout"),
                })
                .collect();
            Ok((items, None))
        }
        PickerKind::Source => {
            let scene = obs_client
                .current_scene()
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "no active scene".to_owned())?;
            let sources = obs_client
                .sources(&scene)
                .await
                .map_err(|e| e.to_string())?;
            let items = sources
                .into_iter()
                .map(|s| PickerItem {
                    id: s.name.clone(),
                    label: s.name,
                    sublabel: Some(if s.visible {
                        "visible".to_owned()
                    } else {
                        "hidden".to_owned()
                    }),
                    icon: SectionIcon::new("device-desktop"),
                })
                .collect();
            Ok((items, Some(scene)))
        }
        PickerKind::Input => {
            let inputs = obs_client.audio_inputs().await.map_err(|e| e.to_string())?;
            let items = inputs
                .into_iter()
                .map(|name| PickerItem {
                    id: name.clone(),
                    label: name,
                    sublabel: None,
                    icon: SectionIcon::new("volume"),
                })
                .collect();
            Ok((items, None))
        }
        PickerKind::Hotkey | PickerKind::Expression | PickerKind::MidiPort => {
            Err(forge_widgets::tr!("builtin_obs_not_supported"))
        }
    }
}

pub fn view<'a>(state: &'a BuiltinDetailState, palette: &'a ForgePalette) -> Element<'a, Message> {
    let section_gap = spf(Spacing::Md);

    let params = HeaderCardParams {
        display_name: &state.display_name,
        version: state.version.as_deref(),
        endpoint: state.endpoint.as_deref(),
        uptime: state.builtin_status.uptime(),
        capability_flags: &state.capability_flags,
        header_actions: &state.header_actions,
        connection: state.builtin_status.connection(),
        icon: state.icon.clone(),
        badges: &[],
    };

    let header = builtin_header_card(
        params,
        |action| Message::BuiltinDetail(BuiltinDetailMsg::HeaderActionClicked(action)),
        palette,
    );

    let health = builtin_health_grid(&state.health_metrics, palette);
    let content = builtin_content_renderer(&state.sections, palette);
    let quick = builtin_quick_actions_grid(
        &state.quick_actions,
        |idx| Message::BuiltinDetail(BuiltinDetailMsg::QuickActionClicked(idx)),
        palette,
    );

    let col = iced::widget::Column::new()
        .spacing(section_gap)
        .push(header)
        .push(health)
        .push(content)
        .push(quick);

    let padded = container(col)
        .width(Length::Fill)
        .padding([sp(Spacing::Md), sp(Spacing::Lg)]);

    let scroll_body: Element<'_, Message> = iced::widget::scrollable(padded).into();
    let page_header = forge_widgets::breadcrumb(
        vec![
            forge_widgets::BreadcrumbCrumb::leaf(forge_widgets::tr!("builtin.breadcrumb")),
            forge_widgets::BreadcrumbCrumb::leaf(state.display_name.clone()),
        ],
        None,
        palette,
    );
    let base: Element<'_, Message> = iced::widget::column![page_header, scroll_body]
        .width(iced::Length::Fill)
        .height(iced::Length::Fill)
        .into();

    let content: Element<'_, Message> = match (&state.pending_picker, &state.quick_action_toast) {
        (Some(pending), Some(toast_msg)) => iced::widget::stack![
            base,
            build_picker_overlay(pending, palette),
            build_toast_overlay(toast_msg, palette)
        ]
        .into(),
        (Some(pending), None) => {
            iced::widget::stack![base, build_picker_overlay(pending, palette)].into()
        }
        (None, Some(toast_msg)) => {
            iced::widget::stack![base, build_toast_overlay(toast_msg, palette)].into()
        }
        (None, None) => base,
    };

    match pending_disconnect_modal(state, palette) {
        Some(modal) => iced::widget::stack![content, modal].into(),
        None => content,
    }
}

/// Renders the shared destructive-confirm modal while a disconnect is armed.
/// Sits on top of the picker/toast overlays — a blocking confirm dialog
/// always dominates.
fn pending_disconnect_modal<'a>(
    state: &'a BuiltinDetailState,
    palette: &'a ForgePalette,
) -> Option<Element<'a, Message>> {
    if !state.pending_disconnect {
        return None;
    }

    Some(confirm_modal(
        ConfirmModalParams {
            kind: ConfirmKind::Client,
            item_name: Cow::Borrowed(state.display_name.as_str()),
            cascade_hint: Some(Cow::Owned(forge_widgets::tr!(
                "builtin_disconnect_confirm_hint"
            ))),
            tone: ConfirmTone::Warning,
        },
        Message::BuiltinDetail(BuiltinDetailMsg::DisconnectConfirmAccepted),
        Message::BuiltinDetail(BuiltinDetailMsg::DisconnectConfirmDismissed),
        palette,
    ))
}

fn build_picker_overlay<'a>(
    pending: &'a PendingPicker,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let (items, loading): (&[PickerItem], bool) = match &pending.items {
        PickerItemsState::Idle | PickerItemsState::Loading => (&[], true),
        PickerItemsState::Loaded(items) => (items.as_slice(), false),
        PickerItemsState::Failed(_) => (&[], false),
    };

    let title: &'static str = match pending.kind {
        PickerKind::Scene => Box::leak(forge_widgets::tr!("builtin.picker.scene").into_boxed_str()),
        PickerKind::Source => {
            Box::leak(forge_widgets::tr!("builtin.picker.source").into_boxed_str())
        }
        PickerKind::Input => {
            Box::leak(forge_widgets::tr!("builtin.picker.audio_input").into_boxed_str())
        }
        PickerKind::Hotkey => {
            Box::leak(forge_widgets::tr!("builtin.picker.hotkey").into_boxed_str())
        }
        PickerKind::Expression => {
            Box::leak(forge_widgets::tr!("builtin.picker.expression").into_boxed_str())
        }
        PickerKind::MidiPort => {
            Box::leak(forge_widgets::tr!("builtin.picker.midi_port").into_boxed_str())
        }
    };

    picker_modal(
        PickerModalProps {
            title,
            search_value: &pending.search,
            items,
            loading,
        },
        |s| Message::BuiltinDetail(BuiltinDetailMsg::PickerSearchChanged(s)),
        |idx| Message::BuiltinDetail(BuiltinDetailMsg::PickerItemSelected(idx)),
        Message::BuiltinDetail(BuiltinDetailMsg::PickerCancelled),
        palette,
    )
}

fn build_toast_overlay<'a>(msg: &'a str, palette: &'a ForgePalette) -> Element<'a, Message> {
    let toast_el = toast_banner(
        msg,
        ToastVariant::Success,
        Message::BuiltinDetail(BuiltinDetailMsg::DismissToast),
        palette,
    );

    container(container(toast_el).max_width(320))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::End)
        .align_y(Alignment::End)
        .padding(iced::Padding {
            top: 0.0,
            right: spf(Spacing::Md),
            bottom: spf(Spacing::Md),
            left: 0.0,
        })
        .into()
}

pub fn health_subscription(state: &BuiltinDetailState) -> Subscription<Message> {
    use iced::advanced::subscription::{EventStream, Hasher, Recipe, from_recipe};
    use iced::futures::StreamExt as _;

    struct HealthRecipe {
        id: BuiltinId,
        source: Arc<dyn BuiltinHealth>,
    }

    impl Recipe for HealthRecipe {
        type Output = Message;

        fn hash(&self, state: &mut Hasher) {
            use std::hash::Hash as _;
            self.id.hash(state);
        }

        fn stream(
            self: Box<Self>,
            _input: EventStream,
        ) -> iced::futures::stream::BoxStream<'static, Self::Output> {
            self.source
                .stream()
                .map(|delta| Message::BuiltinDetail(BuiltinDetailMsg::HealthDelta(delta)))
                .boxed()
        }
    }

    from_recipe(HealthRecipe {
        id: state.id.clone(),
        source: state.builtin_health.clone(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    use crate::app::App;
    use forge_platform_core::{
        BuiltinContent, BuiltinHealth, BuiltinId, BuiltinStatus, CapabilityFlags, ConnectionState,
        DetailSection, HeaderAction, HealthDelta, HealthMetric, HealthStream, HealthValue,
        PickerKind, QuickAction, QuickActions, SectionIcon,
    };

    struct TestStatus {
        id: BuiltinId,
    }

    impl BuiltinStatus for TestStatus {
        fn id(&self) -> &BuiltinId {
            &self.id
        }

        fn display_name(&self) -> &str {
            "Test Integration"
        }

        fn version(&self) -> Option<&str> {
            Some("v1.0")
        }

        fn connection(&self) -> ConnectionState {
            ConnectionState::Connected
        }

        fn uptime(&self) -> Option<Duration> {
            None
        }

        fn endpoint(&self) -> Option<&str> {
            Some("ws://localhost:4455")
        }

        fn capability_flags(&self) -> CapabilityFlags {
            CapabilityFlags {
                limited: false,
                label: None,
            }
        }

        fn header_actions(&self) -> Vec<HeaderAction> {
            vec![HeaderAction::Reconnect]
        }
    }

    struct TestHealth;

    impl BuiltinHealth for TestHealth {
        fn metrics(&self) -> [HealthMetric; 4] {
            [
                HealthMetric {
                    label: "A".into(),
                    value: HealthValue::Text {
                        primary: "0".into(),
                        secondary: None,
                    },
                },
                HealthMetric {
                    label: "B".into(),
                    value: HealthValue::Text {
                        primary: "0".into(),
                        secondary: None,
                    },
                },
                HealthMetric {
                    label: "C".into(),
                    value: HealthValue::Text {
                        primary: "0".into(),
                        secondary: None,
                    },
                },
                HealthMetric {
                    label: "D".into(),
                    value: HealthValue::Text {
                        primary: "0".into(),
                        secondary: None,
                    },
                },
            ]
        }

        fn stream(&self) -> HealthStream {
            Box::pin(futures_util::stream::empty())
        }
    }

    struct TestContent;

    impl BuiltinContent for TestContent {
        fn sections(&self) -> Vec<DetailSection> {
            vec![]
        }
    }

    struct TestQuickActions {
        actions: Vec<QuickAction>,
    }

    impl QuickActions for TestQuickActions {
        fn actions(&self) -> Vec<QuickAction> {
            self.actions.clone()
        }
    }

    fn make_state_with_actions(actions: Vec<QuickAction>) -> BuiltinDetailState {
        BuiltinDetailState::new(
            BuiltinId::new("test"),
            SectionIcon::new("broadcast"),
            Arc::new(TestStatus {
                id: BuiltinId::new("test"),
            }),
            Arc::new(TestHealth),
            Arc::new(TestContent),
            Arc::new(TestQuickActions { actions }),
            None,
        )
    }

    fn make_state() -> BuiltinDetailState {
        make_state_with_actions(vec![])
    }

    #[test]
    fn health_delta_updates_metric_value() {
        let mut state_opt = Some(make_state());
        let app = App::default();
        let delta = HealthDelta {
            index: 1,
            new_value: HealthValue::Text {
                primary: "42".into(),
                secondary: None,
            },
        };
        let _ = update(
            &mut state_opt,
            &app.rt,
            BuiltinDetailMsg::HealthDelta(delta),
        );
        let state = state_opt.as_ref().unwrap();
        assert!(matches!(
            &state.health_metrics[1].value,
            HealthValue::Text { primary, .. } if primary == "42"
        ));
    }

    #[test]
    fn handle_is_noop_when_state_absent() {
        let mut state_opt: Option<BuiltinDetailState> = None;
        let app = App::default();
        let _ = update(&mut state_opt, &app.rt, BuiltinDetailMsg::PickerCancelled);
        assert!(state_opt.is_none());
    }

    #[test]
    fn quick_action_no_picker_dispatches_without_opening_picker() {
        let action = QuickAction {
            label: "Start Recording".to_owned(),
            icon: SectionIcon::new("record"),
            enabled: true,
            subaction_template: forge_types::SubActionStep {
                kind_id: "obs.record.start".to_owned(),
                config: std::collections::BTreeMap::new(),
                enabled: true,
                label: None,
            },
            picker: None,
        };
        let mut state_opt = Some(make_state_with_actions(vec![action]));
        let app = App::default();
        let _ = update(
            &mut state_opt,
            &app.rt,
            BuiltinDetailMsg::QuickActionClicked(0),
        );
        assert!(
            state_opt.as_ref().unwrap().pending_picker.is_none(),
            "picker must not open when action has no picker"
        );
    }

    #[test]
    fn quick_action_with_picker_sets_pending_to_loading() {
        let action = QuickAction {
            label: "Switch Scene".to_owned(),
            icon: SectionIcon::new("arrows-shuffle"),
            enabled: true,
            subaction_template: forge_types::SubActionStep {
                kind_id: "obs.scenes.switch_current".to_owned(),
                config: std::collections::BTreeMap::from([(
                    "scene".to_owned(),
                    forge_types::Variant::String(String::new()),
                )]),
                enabled: true,
                label: None,
            },
            picker: Some(PickerKind::Scene),
        };
        let mut state_opt = Some(make_state_with_actions(vec![action]));
        let app = App::default();
        let _ = update(
            &mut state_opt,
            &app.rt,
            BuiltinDetailMsg::QuickActionClicked(0),
        );
        let state = state_opt.as_ref().unwrap();
        let pending = state.pending_picker.as_ref().unwrap();
        assert_eq!(pending.kind, PickerKind::Scene);
        assert!(matches!(pending.items, PickerItemsState::Loading));
    }

    #[test]
    fn picker_item_selected_fills_scene_template_and_closes_picker() {
        let action = QuickAction {
            label: "Switch Scene".to_owned(),
            icon: SectionIcon::new("arrows-shuffle"),
            enabled: true,
            subaction_template: forge_types::SubActionStep {
                kind_id: "obs.scenes.switch_current".to_owned(),
                config: std::collections::BTreeMap::from([(
                    "scene".to_owned(),
                    forge_types::Variant::String(String::new()),
                )]),
                enabled: true,
                label: None,
            },
            picker: Some(PickerKind::Scene),
        };
        let mut state = make_state_with_actions(vec![action]);

        let items = vec![
            PickerItem {
                id: "Main".to_owned(),
                label: "Main".to_owned(),
                sublabel: None,
                icon: SectionIcon::new("layout"),
            },
            PickerItem {
                id: "BRB".to_owned(),
                label: "BRB".to_owned(),
                sublabel: None,
                icon: SectionIcon::new("layout"),
            },
            PickerItem {
                id: "Gameplay".to_owned(),
                label: "Gameplay".to_owned(),
                sublabel: None,
                icon: SectionIcon::new("layout"),
            },
        ];
        state.pending_picker = Some(PendingPicker {
            action_index: 0,
            kind: PickerKind::Scene,
            search: String::new(),
            items: PickerItemsState::Loaded(items),
            current_scene: None,
        });

        let mut state_opt = Some(state);
        let app = App::default();
        let _ = update(
            &mut state_opt,
            &app.rt,
            BuiltinDetailMsg::PickerItemSelected(2),
        );

        let detail_state = state_opt.as_ref().unwrap();
        assert!(
            detail_state.pending_picker.is_none(),
            "picker must be closed after selection"
        );
    }

    #[test]
    fn picker_cancelled_clears_pending() {
        let mut state = make_state();
        state.pending_picker = Some(PendingPicker {
            action_index: 0,
            kind: PickerKind::Scene,
            search: String::new(),
            items: PickerItemsState::Loading,
            current_scene: None,
        });
        let mut state_opt = Some(state);
        let app = App::default();
        let _ = update(&mut state_opt, &app.rt, BuiltinDetailMsg::PickerCancelled);
        assert!(state_opt.as_ref().unwrap().pending_picker.is_none());
    }

    #[test]
    fn dismiss_toast_clears_toast() {
        let mut state = make_state();
        state.quick_action_toast = Some("Switch Scene — done".to_owned());
        let mut state_opt = Some(state);
        let app = App::default();
        let _ = update(&mut state_opt, &app.rt, BuiltinDetailMsg::DismissToast);
        assert!(state_opt.as_ref().unwrap().quick_action_toast.is_none());
    }

    #[test]
    fn picker_search_changed_updates_search() {
        let mut state = make_state();
        state.pending_picker = Some(PendingPicker {
            action_index: 0,
            kind: PickerKind::Scene,
            search: String::new(),
            items: PickerItemsState::Loading,
            current_scene: None,
        });
        let mut state_opt = Some(state);
        let app = App::default();
        let _ = update(
            &mut state_opt,
            &app.rt,
            BuiltinDetailMsg::PickerSearchChanged("game".to_owned()),
        );
        let pending = state_opt.as_ref().unwrap().pending_picker.as_ref().unwrap();
        assert_eq!(pending.search, "game");
    }

    #[test]
    fn picker_items_loaded_ok_sets_loaded_state() {
        let mut state = make_state();
        state.pending_picker = Some(PendingPicker {
            action_index: 0,
            kind: PickerKind::Scene,
            search: String::new(),
            items: PickerItemsState::Loading,
            current_scene: None,
        });
        let mut state_opt = Some(state);
        let app = App::default();
        let items = vec![PickerItem {
            id: "Gameplay".to_owned(),
            label: "Gameplay".to_owned(),
            sublabel: None,
            icon: SectionIcon::new("layout"),
        }];
        let _ = update(
            &mut state_opt,
            &app.rt,
            BuiltinDetailMsg::PickerItemsLoaded(Ok((items, Some("Gameplay".to_owned())))),
        );
        let pending = state_opt.as_ref().unwrap().pending_picker.as_ref().unwrap();
        assert!(matches!(pending.items, PickerItemsState::Loaded(_)));
        assert_eq!(pending.current_scene.as_deref(), Some("Gameplay"));
    }

    #[test]
    fn picker_items_loaded_err_sets_failed_state() {
        let mut state = make_state();
        state.pending_picker = Some(PendingPicker {
            action_index: 0,
            kind: PickerKind::Hotkey,
            search: String::new(),
            items: PickerItemsState::Loading,
            current_scene: None,
        });
        let mut state_opt = Some(state);
        let app = App::default();
        let _ = update(
            &mut state_opt,
            &app.rt,
            BuiltinDetailMsg::PickerItemsLoaded(Err(
                "Not supported for OBS — VTube only".to_owned()
            )),
        );
        let pending = state_opt.as_ref().unwrap().pending_picker.as_ref().unwrap();
        assert!(matches!(pending.items, PickerItemsState::Failed(_)));
    }
}
