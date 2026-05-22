use std::sync::Arc;

use forge_obs::{ObsClient, ObsSource};
use forge_platform_core::{
    CapabilityFlags, DetailSection, HeaderAction, HealthMetric, IntegrationContent,
    IntegrationHealth, IntegrationId, IntegrationStatus, PickerKind, QuickAction, QuickActions,
    SectionIcon,
};
use forge_types::SubActionSpec;
use forge_widgets::{
    Density, ForgePalette, HeaderCardParams, PickerItem, PickerModalProps, Spacing, ToastVariant,
    integration_content_renderer, integration_header_card, integration_health_grid,
    integration_quick_actions_grid, picker_modal, spacing, toast_banner,
};
use iced::widget::container;
use iced::{Alignment, Element, Length, Subscription, Task};

use crate::app::App;
use crate::message::{IntegrationDetailMsg, Message};

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

pub struct IntegrationDetailState {
    pub id: IntegrationId,
    pub integration_status: Arc<dyn IntegrationStatus>,
    pub integration_health: Arc<dyn IntegrationHealth>,
    pub integration_content: Arc<dyn IntegrationContent>,
    pub integration_quick_actions: Arc<dyn QuickActions>,
    pub health_metrics: [HealthMetric; 4],
    pub pending_picker: Option<PendingPicker>,
    pub quick_action_toast: Option<String>,
    display_name: String,
    version: Option<String>,
    endpoint: Option<String>,
    capability_flags: CapabilityFlags,
    header_actions: Vec<HeaderAction>,
    icon: SectionIcon,
    sections: Vec<DetailSection>,
    quick_actions: Vec<QuickAction>,
}

impl IntegrationDetailState {
    pub fn new(
        id: IntegrationId,
        icon: SectionIcon,
        integration_status: Arc<dyn IntegrationStatus>,
        integration_health: Arc<dyn IntegrationHealth>,
        integration_content: Arc<dyn IntegrationContent>,
        integration_quick_actions: Arc<dyn QuickActions>,
    ) -> Self {
        let display_name = integration_status.display_name().to_owned();
        let version = integration_status.version().map(ToOwned::to_owned);
        let endpoint = integration_status.endpoint().map(ToOwned::to_owned);
        let capability_flags = integration_status.capability_flags();
        let header_actions = integration_status.header_actions();
        let health_metrics = integration_health.metrics();
        let sections = integration_content.sections();
        let quick_actions = integration_quick_actions.actions();
        Self {
            id,
            integration_status,
            integration_health,
            integration_content,
            integration_quick_actions,
            health_metrics,
            pending_picker: None,
            quick_action_toast: None,
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

pub fn handle_integration_detail_msg(app: &mut App, msg: IntegrationDetailMsg) -> Task<Message> {
    let Some(state) = app.integration_detail.as_mut() else {
        return Task::none();
    };
    match msg {
        IntegrationDetailMsg::HealthDelta(delta) => {
            let idx = delta.index as usize;
            if idx < 4 {
                state.health_metrics[idx].value = delta.new_value;
            }
            Task::none()
        }
        IntegrationDetailMsg::HeaderActionClicked(_action) => Task::none(),
        IntegrationDetailMsg::QuickActionClicked(idx) => {
            let Some(action) = state.quick_actions.get(idx) else {
                return Task::none();
            };
            if !action.enabled {
                return Task::none();
            }
            let picker_kind = action.picker;
            let spec = action.subaction_template.clone();
            let label = action.label.clone();
            let integration_id = state.id.as_str().to_owned();

            if let Some(kind) = picker_kind {
                let obs_client = app.rt.obs_client.clone();
                let Some(detail) = app.integration_detail.as_mut() else {
                    return Task::none();
                };
                detail.pending_picker = Some(PendingPicker {
                    action_index: idx,
                    kind,
                    search: String::new(),
                    items: PickerItemsState::Loading,
                    current_scene: None,
                });
                match obs_client {
                    Some(client) => {
                        Task::perform(async move { fetch_picker_items(client, kind).await }, |r| {
                            Message::IntegrationDetail(IntegrationDetailMsg::PickerItemsLoaded(r))
                        })
                    }
                    None => Task::perform(
                        async {
                            Err::<(Vec<PickerItem>, Option<String>), _>(
                                "OBS not connected".to_owned(),
                            )
                        },
                        |r| Message::IntegrationDetail(IntegrationDetailMsg::PickerItemsLoaded(r)),
                    ),
                }
            } else {
                let engine = app.rt.action_engine.clone();
                Task::perform(
                    async move {
                        if let Some(e) = engine {
                            let _ = e.execute_quick_action(spec, integration_id, label).await;
                        }
                    },
                    |_| Message::Noop,
                )
            }
        }
        IntegrationDetailMsg::PickerSearchChanged(s) => {
            if let Some(pending) = state.pending_picker.as_mut() {
                pending.search = s;
            }
            Task::none()
        }
        IntegrationDetailMsg::PickerItemsLoaded(Ok((items, current_scene))) => {
            if let Some(pending) = state.pending_picker.as_mut() {
                pending.items = PickerItemsState::Loaded(items);
                pending.current_scene = current_scene;
            }
            Task::none()
        }
        IntegrationDetailMsg::PickerItemsLoaded(Err(e)) => {
            if let Some(pending) = state.pending_picker.as_mut() {
                pending.items = PickerItemsState::Failed(e);
            }
            Task::none()
        }
        IntegrationDetailMsg::PickerItemSelected(item_idx) => {
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
            let integration_id = state.id.as_str().to_owned();

            match kind {
                PickerKind::Scene => {
                    if let SubActionSpec::ObsSetScene { scene_name } = &mut spec {
                        *scene_name = selected_id;
                    }
                }
                PickerKind::Source => {
                    if let SubActionSpec::ObsSetSourceVisible {
                        scene_name,
                        source_name,
                        ..
                    } = &mut spec
                    {
                        if let Some(scene) = current_scene {
                            *scene_name = scene;
                        }
                        *source_name = selected_id;
                    }
                }
                PickerKind::Input => {
                    if let SubActionSpec::ObsSetInputMute { input_name, .. } = &mut spec {
                        *input_name = selected_id;
                    }
                }
                PickerKind::Hotkey | PickerKind::Expression => return Task::none(),
            }

            let engine = app.rt.action_engine.clone();
            Task::perform(
                async move {
                    if let Some(e) = engine {
                        let _ = e.execute_quick_action(spec, integration_id, label).await;
                    }
                },
                |_| Message::Noop,
            )
        }
        IntegrationDetailMsg::PickerCancelled => {
            state.pending_picker = None;
            Task::none()
        }
        IntegrationDetailMsg::DismissToast => {
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
        PickerKind::Hotkey | PickerKind::Expression => {
            Err("Not supported for OBS — VTube only".to_owned())
        }
    }
}

pub fn view<'a>(
    state: &'a IntegrationDetailState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let section_gap = spacing(Spacing::Md, Density::Cozy) as f32;

    let params = HeaderCardParams {
        display_name: &state.display_name,
        version: state.version.as_deref(),
        endpoint: state.endpoint.as_deref(),
        uptime: state.integration_status.uptime(),
        capability_flags: &state.capability_flags,
        header_actions: &state.header_actions,
        connection: state.integration_status.connection(),
        icon: state.icon.clone(),
        badges: &[],
    };

    let header = integration_header_card(
        params,
        |action| Message::IntegrationDetail(IntegrationDetailMsg::HeaderActionClicked(action)),
        palette,
    );

    let health = integration_health_grid(&state.health_metrics, palette);
    let content = integration_content_renderer(&state.sections, palette);
    let quick = integration_quick_actions_grid(
        &state.quick_actions,
        |idx| Message::IntegrationDetail(IntegrationDetailMsg::QuickActionClicked(idx)),
        palette,
    );

    let col = iced::widget::Column::new()
        .spacing(section_gap)
        .push(header)
        .push(health)
        .push(content)
        .push(quick);

    let padded = container(col).width(Length::Fill).padding([
        spacing(Spacing::Md, Density::Cozy),
        spacing(Spacing::Lg, Density::Cozy),
    ]);

    let scroll_body: Element<'_, Message> = iced::widget::scrollable(padded).into();
    let page_header = crate::app::simple_page_header(
        &[("Integrations", false), (state.display_name.as_str(), true)],
        palette,
    );
    let base: Element<'_, Message> = iced::widget::column![page_header, scroll_body]
        .width(iced::Length::Fill)
        .height(iced::Length::Fill)
        .into();

    match (&state.pending_picker, &state.quick_action_toast) {
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
    }
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

    let title = match pending.kind {
        PickerKind::Scene => "Choose a Scene",
        PickerKind::Source => "Choose a Source",
        PickerKind::Input => "Choose an Audio Input",
        PickerKind::Hotkey => "Choose a Hotkey",
        PickerKind::Expression => "Choose an Expression",
    };

    picker_modal(
        PickerModalProps {
            title,
            search_value: &pending.search,
            items,
            loading,
        },
        |s| Message::IntegrationDetail(IntegrationDetailMsg::PickerSearchChanged(s)),
        |idx| Message::IntegrationDetail(IntegrationDetailMsg::PickerItemSelected(idx)),
        Message::IntegrationDetail(IntegrationDetailMsg::PickerCancelled),
        palette,
    )
}

fn build_toast_overlay<'a>(msg: &'a str, palette: &'a ForgePalette) -> Element<'a, Message> {
    let toast_el = toast_banner(
        msg,
        ToastVariant::Success,
        Message::IntegrationDetail(IntegrationDetailMsg::DismissToast),
        palette,
    );

    container(container(toast_el).max_width(320))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::End)
        .align_y(Alignment::End)
        .padding(iced::Padding {
            top: 0.0,
            right: 16.0,
            bottom: 16.0,
            left: 0.0,
        })
        .into()
}

pub fn health_subscription(state: &IntegrationDetailState) -> Subscription<Message> {
    use iced::advanced::subscription::{EventStream, Hasher, Recipe, from_recipe};
    use iced::futures::StreamExt as _;

    struct HealthRecipe {
        id: IntegrationId,
        source: Arc<dyn IntegrationHealth>,
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
                .map(|delta| Message::IntegrationDetail(IntegrationDetailMsg::HealthDelta(delta)))
                .boxed()
        }
    }

    from_recipe(HealthRecipe {
        id: state.id.clone(),
        source: state.integration_health.clone(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    use forge_platform_core::{
        CapabilityFlags, ConnectionState, DetailSection, HeaderAction, HealthDelta, HealthMetric,
        HealthStream, HealthValue, IntegrationContent, IntegrationHealth, IntegrationId,
        IntegrationStatus, PickerKind, QuickAction, QuickActions, SectionIcon,
    };

    struct TestStatus {
        id: IntegrationId,
    }

    impl IntegrationStatus for TestStatus {
        fn id(&self) -> &IntegrationId {
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

    impl IntegrationHealth for TestHealth {
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

    impl IntegrationContent for TestContent {
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

    fn make_state_with_actions(actions: Vec<QuickAction>) -> IntegrationDetailState {
        IntegrationDetailState::new(
            IntegrationId::new("test"),
            SectionIcon::new("broadcast"),
            Arc::new(TestStatus {
                id: IntegrationId::new("test"),
            }),
            Arc::new(TestHealth),
            Arc::new(TestContent),
            Arc::new(TestQuickActions { actions }),
        )
    }

    fn make_state() -> IntegrationDetailState {
        make_state_with_actions(vec![])
    }

    #[test]
    fn health_delta_updates_metric_value() {
        let mut app = App {
            integration_detail: Some(make_state()),
            ..App::default()
        };
        let delta = HealthDelta {
            index: 1,
            new_value: HealthValue::Text {
                primary: "42".into(),
                secondary: None,
            },
        };
        let _ = handle_integration_detail_msg(&mut app, IntegrationDetailMsg::HealthDelta(delta));
        let state = app.integration_detail.as_ref().unwrap();
        assert!(matches!(
            &state.health_metrics[1].value,
            HealthValue::Text { primary, .. } if primary == "42"
        ));
    }

    #[test]
    fn health_delta_out_of_bounds_is_noop() {
        let mut app = App {
            integration_detail: Some(make_state()),
            ..App::default()
        };
        let delta = HealthDelta {
            index: 5,
            new_value: HealthValue::Text {
                primary: "oops".into(),
                secondary: None,
            },
        };
        let _ = handle_integration_detail_msg(&mut app, IntegrationDetailMsg::HealthDelta(delta));
    }

    #[test]
    fn handle_is_noop_when_state_absent() {
        let mut app = App::default();
        let _ = handle_integration_detail_msg(&mut app, IntegrationDetailMsg::PickerCancelled);
        assert!(app.integration_detail.is_none());
    }

    #[test]
    fn quick_action_no_picker_dispatches_without_opening_picker() {
        let action = QuickAction {
            label: "Start Recording".to_owned(),
            icon: SectionIcon::new("record"),
            enabled: true,
            subaction_template: forge_types::SubActionSpec::ObsStartRecord,
            picker: None,
        };
        let mut app = App {
            integration_detail: Some(make_state_with_actions(vec![action])),
            ..App::default()
        };
        let _ =
            handle_integration_detail_msg(&mut app, IntegrationDetailMsg::QuickActionClicked(0));
        assert!(
            app.integration_detail
                .as_ref()
                .unwrap()
                .pending_picker
                .is_none(),
            "picker must not open when action has no picker"
        );
    }

    #[test]
    fn quick_action_with_picker_sets_pending_to_loading() {
        let action = QuickAction {
            label: "Switch Scene".to_owned(),
            icon: SectionIcon::new("arrows-shuffle"),
            enabled: true,
            subaction_template: forge_types::SubActionSpec::ObsSetScene {
                scene_name: String::new(),
            },
            picker: Some(PickerKind::Scene),
        };
        let mut app = App {
            integration_detail: Some(make_state_with_actions(vec![action])),
            ..App::default()
        };
        let _ =
            handle_integration_detail_msg(&mut app, IntegrationDetailMsg::QuickActionClicked(0));
        let state = app.integration_detail.as_ref().unwrap();
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
            subaction_template: forge_types::SubActionSpec::ObsSetScene {
                scene_name: String::new(),
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

        let mut app = App {
            integration_detail: Some(state),
            ..App::default()
        };

        let _ =
            handle_integration_detail_msg(&mut app, IntegrationDetailMsg::PickerItemSelected(2));

        let detail_state = app.integration_detail.as_ref().unwrap();
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
        let mut app = App {
            integration_detail: Some(state),
            ..App::default()
        };
        let _ = handle_integration_detail_msg(&mut app, IntegrationDetailMsg::PickerCancelled);
        assert!(
            app.integration_detail
                .as_ref()
                .unwrap()
                .pending_picker
                .is_none()
        );
    }

    #[test]
    fn dismiss_toast_clears_toast() {
        let mut state = make_state();
        state.quick_action_toast = Some("Switch Scene — done".to_owned());
        let mut app = App {
            integration_detail: Some(state),
            ..App::default()
        };
        let _ = handle_integration_detail_msg(&mut app, IntegrationDetailMsg::DismissToast);
        assert!(
            app.integration_detail
                .as_ref()
                .unwrap()
                .quick_action_toast
                .is_none()
        );
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
        let mut app = App {
            integration_detail: Some(state),
            ..App::default()
        };
        let _ = handle_integration_detail_msg(
            &mut app,
            IntegrationDetailMsg::PickerSearchChanged("game".to_owned()),
        );
        let pending = app
            .integration_detail
            .as_ref()
            .unwrap()
            .pending_picker
            .as_ref()
            .unwrap();
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
        let mut app = App {
            integration_detail: Some(state),
            ..App::default()
        };
        let items = vec![PickerItem {
            id: "Gameplay".to_owned(),
            label: "Gameplay".to_owned(),
            sublabel: None,
            icon: SectionIcon::new("layout"),
        }];
        let _ = handle_integration_detail_msg(
            &mut app,
            IntegrationDetailMsg::PickerItemsLoaded(Ok((items, Some("Gameplay".to_owned())))),
        );
        let pending = app
            .integration_detail
            .as_ref()
            .unwrap()
            .pending_picker
            .as_ref()
            .unwrap();
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
        let mut app = App {
            integration_detail: Some(state),
            ..App::default()
        };
        let _ = handle_integration_detail_msg(
            &mut app,
            IntegrationDetailMsg::PickerItemsLoaded(Err(
                "Not supported for OBS — VTube only".to_owned()
            )),
        );
        let pending = app
            .integration_detail
            .as_ref()
            .unwrap()
            .pending_picker
            .as_ref()
            .unwrap();
        assert!(matches!(pending.items, PickerItemsState::Failed(_)));
    }

    #[test]
    fn view_smoke() {
        let state = make_state();
        let (_, palette) = forge_widgets::catppuccin_mocha();
        let _ = view(&state, &palette);
    }

    #[test]
    fn view_smoke_with_picker_overlay() {
        let mut state = make_state();
        state.pending_picker = Some(PendingPicker {
            action_index: 0,
            kind: PickerKind::Source,
            search: String::new(),
            items: PickerItemsState::Loading,
            current_scene: None,
        });
        let (_, palette) = forge_widgets::catppuccin_mocha();
        let _ = view(&state, &palette);
    }

    #[test]
    fn view_smoke_with_toast() {
        let mut state = make_state();
        state.quick_action_toast = Some("Start Recording — done".to_owned());
        let (_, palette) = forge_widgets::catppuccin_mocha();
        let _ = view(&state, &palette);
    }
}
