use std::sync::Arc;

use forge_platform_core::{
    CapabilityFlags, DetailSection, HeaderAction, HealthMetric, IntegrationContent,
    IntegrationHealth, IntegrationId, IntegrationStatus, PickerKind, QuickAction, QuickActions,
    SectionIcon,
};
use forge_widgets::{
    Density, ForgePalette, HeaderCardParams, Spacing, integration_content_renderer,
    integration_header_card, integration_health_grid, integration_quick_actions_grid, spacing,
    tokens::FONT_BODY_MD,
};
use iced::widget::container;
use iced::{Alignment, Element, Length, Subscription, Task};

use crate::app::App;
use crate::message::{IntegrationDetailMsg, Message};

pub struct IntegrationDetailState {
    pub id: IntegrationId,
    pub integration_status: Arc<dyn IntegrationStatus>,
    pub integration_health: Arc<dyn IntegrationHealth>,
    pub integration_content: Arc<dyn IntegrationContent>,
    pub integration_quick_actions: Arc<dyn QuickActions>,
    pub health_metrics: [HealthMetric; 4],
    pub pending_picker: Option<PickerKind>,
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
        IntegrationDetailMsg::QuickActionClicked(_idx) => Task::none(),
        IntegrationDetailMsg::PickerOpened(kind) => {
            state.pending_picker = Some(kind);
            Task::none()
        }
        IntegrationDetailMsg::PickerClosed => {
            state.pending_picker = None;
            Task::none()
        }
    }
}

pub fn view<'a>(
    state: &'a IntegrationDetailState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let section_gap = spacing(Spacing::Xxxl, Density::Cozy) as f32;

    let params = HeaderCardParams {
        display_name: &state.display_name,
        version: state.version.as_deref(),
        endpoint: state.endpoint.as_deref(),
        uptime: state.integration_status.uptime(),
        capability_flags: &state.capability_flags,
        header_actions: &state.header_actions,
        connection: state.integration_status.connection(),
        icon: state.icon.clone(),
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
        spacing(Spacing::Xxxl, Density::Cozy),
        spacing(Spacing::Huge, Density::Cozy),
    ]);

    let base: Element<'_, Message> = iced::widget::scrollable(padded).into();

    if state.pending_picker.is_some() {
        let shell_bg = palette.shell;
        let overlay = container(
            iced::widget::text("Picker coming in alpha-8")
                .size(FONT_BODY_MD)
                .color(palette.text_primary),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(iced::Color {
                a: 0.75,
                ..shell_bg
            })),
            ..container::Style::default()
        });

        iced::widget::stack![base, overlay].into()
    } else {
        base
    }
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

    struct TestQuickActions;

    impl QuickActions for TestQuickActions {
        fn actions(&self) -> Vec<QuickAction> {
            vec![]
        }
    }

    fn make_state() -> IntegrationDetailState {
        IntegrationDetailState::new(
            IntegrationId::new("test"),
            SectionIcon::new("broadcast"),
            Arc::new(TestStatus {
                id: IntegrationId::new("test"),
            }),
            Arc::new(TestHealth),
            Arc::new(TestContent),
            Arc::new(TestQuickActions),
        )
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
    fn picker_opened_sets_pending() {
        let mut app = App {
            integration_detail: Some(make_state()),
            ..App::default()
        };
        let _ = handle_integration_detail_msg(
            &mut app,
            IntegrationDetailMsg::PickerOpened(PickerKind::Scene),
        );
        assert_eq!(
            app.integration_detail.as_ref().unwrap().pending_picker,
            Some(PickerKind::Scene)
        );
    }

    #[test]
    fn picker_closed_clears_pending() {
        let mut state = make_state();
        state.pending_picker = Some(PickerKind::Scene);
        let mut app = App {
            integration_detail: Some(state),
            ..App::default()
        };
        let _ = handle_integration_detail_msg(&mut app, IntegrationDetailMsg::PickerClosed);
        assert!(
            app.integration_detail
                .as_ref()
                .unwrap()
                .pending_picker
                .is_none()
        );
    }

    #[test]
    fn handle_is_noop_when_state_absent() {
        let mut app = App::default();
        let _ = handle_integration_detail_msg(
            &mut app,
            IntegrationDetailMsg::PickerOpened(PickerKind::Scene),
        );
        assert!(app.integration_detail.is_none());
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
        state.pending_picker = Some(PickerKind::Source);
        let (_, palette) = forge_widgets::catppuccin_mocha();
        let _ = view(&state, &palette);
    }
}
