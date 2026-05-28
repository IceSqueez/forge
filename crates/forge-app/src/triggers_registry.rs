use forge_types::{ActionId, TriggerInstanceId};
use iced::{Element, Task};

use crate::message::Message;
use crate::runtime_view::RuntimeView;
use forge_widgets::ForgePalette;

#[derive(Debug, Clone)]
pub struct TriggerInstanceRow {
    pub id: TriggerInstanceId,
    pub name: String,
    pub kind_id: String,
    pub enabled: bool,
    pub used_in_count: usize,
}

#[derive(Debug, Clone)]
pub struct InstanceUsage {
    pub action_id: ActionId,
    pub action_name: String,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum UsageFilter {
    #[default]
    All,
    Used,
    Unused,
}

#[derive(Debug, Clone)]
pub struct ConfirmDisable {
    pub instance_id: TriggerInstanceId,
    pub action_count: usize,
}

pub struct TriggersRegistryState {
    pub instances: Vec<TriggerInstanceRow>,
    pub selected_id: Option<TriggerInstanceId>,
    pub used_in: Vec<InstanceUsage>,
    pub search: String,
    pub platform_filter: Option<String>,
    pub usage_filter: UsageFilter,
    pub sheet_width: f32,
    pub confirm_disable: Option<ConfirmDisable>,
}

impl Default for TriggersRegistryState {
    fn default() -> Self {
        Self {
            instances: Vec::new(),
            selected_id: None,
            used_in: Vec::new(),
            search: String::new(),
            platform_filter: None,
            usage_filter: UsageFilter::All,
            sheet_width: 420.0,
            confirm_disable: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum TriggersRegistryMsg {
    LoadRequested,
    Loaded(Result<Vec<TriggerInstanceRow>, String>),
    SearchChanged(String),
    PlatformFilterChanged(Option<String>),
    UsageFilterChanged(UsageFilter),
    RowSelected(TriggerInstanceId),
    RowDeselected,
    UsedInLoaded(Result<Vec<InstanceUsage>, String>),
    EnableToggled(TriggerInstanceId, bool),
    DisableConfirmAccepted(TriggerInstanceId),
    DisableConfirmDismissed,
    SheetClosed,
    SheetResized(f32),
    SheetWidthLoaded(Option<f32>),
    DeleteRequested(TriggerInstanceId),
    DeleteResult(Result<(), String>),
    NavigateToAction(ActionId),
    ScrollTo(TriggerInstanceId),
}

pub fn update(
    state: &mut TriggersRegistryState,
    rt: &RuntimeView,
    msg: TriggersRegistryMsg,
) -> Task<Message> {
    let _ = (state, rt, msg);
    Task::none()
}

pub fn view<'a>(
    state: &'a TriggersRegistryState,
    rt: &'a RuntimeView,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let _ = (state, rt, palette);
    iced::widget::column![iced::widget::text("Triggers Registry — coming soon")].into()
}
