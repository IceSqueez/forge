use super::editor::{PackExportHost, StepDragSession};
use super::{
    ActionForm, ActionGroup, ActionsFilter, AddTriggerStage, CaseField, GridPickerForm,
    HistoryModalHost, LEFT_PANEL_W, Renaming, action_writes, nav, sub_action_modal, test_run,
};
use crate::async_bridge::{self, Loadable};
use crate::collection_options::{ChoiceOptions, CollectionChoiceField};
use crate::navigable_list::ListCursor;
use crate::obs_catalog_options::ObsCatalogField;
use crate::vtube_catalog_options::VTubeCatalogField;
use forge_components::{Confirm, SearchState};
use forge_host::integrations::BuiltinRegistry;
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::{ActionDetail, ActionsService, EventBus, QueueSchedulerHandle};
use forge_soundboard::ClipLibrary;
use forge_speak_queue::SpeakQueueHandle;
use forge_storage::{
    ActionRepo, ActionTelemetry, GlobalsRepo, OverlayRepo, QueueRepo, ScriptRepo,
    TriggerInstanceRepo,
};
use forge_types::{ActionId, ExecutionOutcome};
use gpui::{Context, Entity, FocusHandle, Pixels, Point, ScrollHandle, Subscription, Task};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

pub(crate) struct ActionsDeps {
    pub(crate) action_repo: Arc<dyn ActionRepo>,
    pub(crate) queue_repo: Arc<dyn QueueRepo>,
    pub(crate) actions_service: Arc<ActionsService>,
    pub(crate) trigger_instance_repo: Arc<dyn TriggerInstanceRepo>,
    pub(crate) script_repo: Arc<dyn ScriptRepo>,
    pub(crate) clip_library: Arc<ClipLibrary>,
    pub(crate) globals_repo: Arc<dyn GlobalsRepo>,
    pub(crate) overlay_repo: Arc<dyn OverlayRepo>,
    pub(crate) speak: Option<SpeakQueueHandle>,
    pub(crate) sub_action_registry: Arc<SubActionRegistry>,
    pub(crate) trigger_registry: Arc<TriggerRegistry>,
    pub(crate) rt_handle: tokio::runtime::Handle,
    pub(crate) bus: Arc<EventBus>,
    pub(crate) scheduler: QueueSchedulerHandle,
}

pub(super) struct ActionTreeState {
    pub(super) width: Pixels,
    pub(super) keys: ListCursor<ActionId>,
    pub(super) scroll: ScrollHandle,
    pub(super) list_load: Loadable<()>,
    pub(super) list_reads: Arc<AtomicU64>,
    pub(super) landed_list_read: u64,
    pub(super) groups: Vec<ActionGroup>,
    pub(super) filter_action_ids: HashMap<ActionsFilter, HashSet<ActionId>>,
    pub(super) filter: ActionsFilter,
    pub(super) search: SearchState,
    pub(super) _search_sub: Subscription,
    pub(super) selected: Option<ActionId>,
    pub(super) hovered: Option<ActionId>,
    pub(super) concurrent_queue_ids: HashSet<String>,
    pub(super) renaming: Option<Renaming>,
    pub(super) pending_delete: Confirm<ActionId>,
    pub(super) delete_scheduled_count: Option<(ActionId, u64)>,
}

impl ActionTreeState {
    pub(super) fn new<V: 'static>(
        search: SearchState,
        search_sub: Subscription,
        selected: Option<ActionId>,
        cx: &mut Context<V>,
    ) -> Self {
        Self {
            width: LEFT_PANEL_W,
            keys: ListCursor::new(cx),
            scroll: ScrollHandle::new(),
            list_load: Loadable::Loading,
            list_reads: Arc::new(AtomicU64::new(0)),
            landed_list_read: 0,
            groups: Vec::new(),
            filter_action_ids: HashMap::new(),
            filter: ActionsFilter::All,
            search,
            _search_sub: search_sub,
            selected,
            hovered: None,
            concurrent_queue_ids: HashSet::new(),
            renaming: None,
            pending_delete: Confirm::default(),
            delete_scheduled_count: None,
        }
    }
}

pub(super) struct ActionMenus {
    pub(super) row_open: Option<ActionId>,
    pub(super) header_open: Option<Point<Pixels>>,
    pub(super) step_open: Option<usize>,
    pub(super) click_pos: Option<Point<Pixels>>,
    pub(super) focus: FocusHandle,
    pub(super) focus_restore: Option<FocusHandle>,
}

impl ActionMenus {
    pub(super) fn new<V: 'static>(cx: &mut Context<V>) -> Self {
        Self {
            row_open: None,
            header_open: None,
            step_open: None,
            click_pos: None,
            focus: cx.focus_handle(),
            focus_restore: None,
        }
    }
}

pub(super) struct StepChoices {
    pub(super) select_options: HashMap<String, Vec<(String, String)>>,
    pub(super) select_options_failed: bool,
    pub(super) builtins: BuiltinRegistry,
    pub(super) collection_options: ChoiceOptions,
    pub(super) collection_options_gen: async_bridge::Generation,
    pub(super) obs_catalog_options_gen: async_bridge::Generation,
    pub(super) choice_fields: Vec<CollectionChoiceField>,
    pub(super) obs_fields: Vec<ObsCatalogField>,
    pub(super) vtube_fields: Vec<VTubeCatalogField>,
    pub(super) _collection_watch: Vec<Task<()>>,
}

impl StepChoices {
    pub(super) fn new() -> Self {
        Self {
            select_options: HashMap::new(),
            select_options_failed: false,
            builtins: BuiltinRegistry::default(),
            collection_options: ChoiceOptions::new(),
            collection_options_gen: async_bridge::Generation::default(),
            obs_catalog_options_gen: async_bridge::Generation::default(),
            choice_fields: Vec::new(),
            obs_fields: Vec::new(),
            vtube_fields: Vec::new(),
            _collection_watch: Vec::new(),
        }
    }
}

pub(super) struct ActionEditorState {
    pub(super) detail: Option<ActionDetail>,
    pub(super) telemetry: Option<ActionTelemetry>,
    pub(super) last_outcome: Option<ExecutionOutcome>,
    pub(super) step_drag: Option<StepDragSession>,
    pub(super) step_drag_focus: FocusHandle,
    pub(super) action_writes: HashMap<ActionId, action_writes::ActionWriteQueue>,
    pub(super) nav_path: Vec<nav::NavFrame>,
    pub(super) case_fields: BTreeMap<(usize, usize), CaseField>,
    pub(super) step_health: Vec<forge_registry::StepHealth>,
}

impl ActionEditorState {
    pub(super) fn new<V: 'static>(cx: &mut Context<V>) -> Self {
        Self {
            detail: None,
            telemetry: None,
            last_outcome: None,
            step_drag: None,
            step_drag_focus: cx.focus_handle(),
            action_writes: HashMap::new(),
            nav_path: Vec::new(),
            case_fields: BTreeMap::new(),
            step_health: Vec::new(),
        }
    }
}

#[derive(Default)]
pub(super) struct ActionModals {
    pub(super) action_form: Option<ActionForm>,
    pub(super) history: Option<HistoryModalHost>,
    pub(super) test_run: Option<Entity<test_run::TestRunModal>>,
    pub(super) _test_run_sub: Option<Subscription>,
    pub(super) sub_form: Option<Entity<sub_action_modal::EditSubActionForm>>,
    pub(super) _sub_form_sub: Option<Subscription>,
    pub(super) grid_picker: Option<GridPickerForm>,
    pub(super) add_trigger: Option<AddTriggerStage>,
    pub(super) pack_export: Option<PackExportHost>,
}
