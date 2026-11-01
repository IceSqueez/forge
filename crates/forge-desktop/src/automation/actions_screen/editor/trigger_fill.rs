use super::*;
use crate::app::async_bridge;
use crate::automation::trigger_fill_form::{
    TriggerFillForm, TriggerFillFormEvent, TriggerFillIds, TriggerFillSeed,
};
use crate::shared::option_choices::ChoiceOptions;
use crate::shared::repo_failure::RepoFailure;
use forge_components::tr;
use gpui::{App, Context, Entity, WeakEntity, Window, div};
use std::rc::Rc;

const FILL_IDS: TriggerFillIds = TriggerFillIds {
    field: "actions-trigger-field",
    back: "actions-trigger-fill-back",
    cancel: "actions-trigger-fill-cancel",
    submit: "actions-trigger-fill-submit",
    close: "actions-trigger-fill-close",
    scrim: "actions-trigger-fill-scrim",
};

pub(super) struct TriggerFillDeps {
    pub(super) editor: WeakEntity<ActionEditor>,
    pub(super) trigger_registry: Arc<TriggerRegistry>,
    pub(super) actions_service: Arc<forge_runtime::ActionsService>,
    pub(super) rt_handle: tokio::runtime::Handle,
}

pub(super) enum TriggerFillEvent {
    Cancelled,
    SaveRequested(ActionId),
    Created,
}

pub(super) struct TriggerFillModal {
    deps: TriggerFillDeps,
    action_id: ActionId,
    form: Entity<TriggerFillForm>,
    _form_sub: Subscription,
}

impl EventEmitter<TriggerFillEvent> for TriggerFillModal {}

impl Render for TriggerFillModal {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(self.form.clone())
    }
}

impl TriggerFillModal {
    pub(super) fn new(
        deps: TriggerFillDeps,
        action_id: ActionId,
        kind_id: String,
        cx: &mut Context<Self>,
    ) -> Self {
        let editor = deps.editor.clone();
        let seed = TriggerFillSeed {
            registry: Arc::clone(&deps.trigger_registry),
            kind_id,
            ids: FILL_IDS,
            on_back: Rc::new(move |window, cx| {
                if let Err(closed) =
                    editor.update(cx, |editor, cx| editor.open_trigger_picker(window, cx))
                {
                    tracing::debug!(error = %closed, "the action editor closed under its trigger form");
                }
            }),
        };
        let form = cx.new(|cx| TriggerFillForm::new(seed, cx));
        let form_sub = cx.subscribe(&form, Self::on_form_event);
        Self {
            deps,
            action_id,
            form,
            _form_sub: form_sub,
        }
    }

    fn on_form_event(
        &mut self,
        _form: Entity<TriggerFillForm>,
        event: &TriggerFillFormEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            TriggerFillFormEvent::Cancelled => cx.emit(TriggerFillEvent::Cancelled),
            TriggerFillFormEvent::SubmitRequested => {
                cx.emit(TriggerFillEvent::SaveRequested(self.action_id));
            }
        }
    }

    pub(super) fn request_save(&mut self, cx: &mut Context<Self>) {
        self.form.update(cx, TriggerFillForm::request_submit);
    }

    pub(super) fn save(&mut self, cx: &mut Context<Self>) {
        let Some(instance) = self.form.update(cx, |form, cx| form.instance_to_save(cx)) else {
            return;
        };
        self.form.update(cx, |form, cx| form.set_saving(true, cx));

        let action_id = self.action_id;
        let service = Arc::clone(&self.deps.actions_service);
        async_bridge::run_async(
            &self.deps.rt_handle,
            async move {
                service
                    .create_linked_trigger(action_id, instance)
                    .await
                    .map_err(RepoFailure::from)
            },
            |this, result: Result<(), RepoFailure>, cx| match result {
                Ok(()) => cx.emit(TriggerFillEvent::Created),
                Err(message) => {
                    this.form.update(cx, |form, cx| form.set_saving(false, cx));
                    on_repo_error(&message, cx);
                }
            },
            cx,
        );
    }

    pub(super) fn option_keys(&self, cx: &App) -> Vec<String> {
        self.form.read(cx).option_keys()
    }

    pub(super) fn refresh_choices(&mut self, options: &ChoiceOptions, cx: &mut Context<Self>) {
        let blank = tr!("config_form_choice_any");
        self.form
            .update(cx, |form, cx| form.refresh_choices(options, &blank, cx));
    }
}
