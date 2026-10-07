use super::*;
use forge_components::{
    Density, FONT_SM, FONT_XS, ForgePalette, Icon, Spacing, body_family, icon, spacing, tr,
};
use forge_types::SubActionStep;
use gpui::{AnyElement, Context, div};

mod cards;
mod export;
mod header;
mod select_options;
mod stats;
mod step_controls;
mod step_list;
mod step_picker;
mod step_presentation;
mod sub_form;
mod trigger_fill;
mod trigger_list;
mod trigger_picker;

pub(crate) use step_presentation::parse_variable_segments;
pub(super) use step_presentation::{step_glyph, sub_action_summary, sub_category_color};

use cards::inline_warning_card;

impl ScreenActionsView {
    pub(super) fn current_chain(&self) -> Vec<SubActionStep> {
        match &self.detail {
            Some(detail) => nav::resolve_chain(&detail.action.sub_actions, &self.nav_path),
            None => Vec::new(),
        }
    }

    pub(super) fn render_editor_pane(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match (self.selected, self.detail.as_ref()) {
            (Some(sel), Some(detail)) if detail.action.id == sel => {
                self.render_editor(detail, palette, cx)
            }
            (Some(_), _) => self.render_loading(palette),
            (None, _) => self.render_empty(palette),
        }
    }

    fn render_empty(&self, palette: &ForgePalette) -> AnyElement {
        let placeholder = div()
            .flex()
            .flex_col()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(icon(Icon::Bolt, EMPTY_GLYPH, palette.text_faint))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_secondary)
                    .child(tr!("actions_detail_empty_title")),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!("actions_detail_empty_hint")),
            );

        div()
            .flex_1()
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .child(placeholder)
            .into_any_element()
    }

    fn render_loading(&self, palette: &ForgePalette) -> AnyElement {
        div()
            .flex_1()
            .h_full()
            .py(spacing(Spacing::Md, Density::Cozy))
            .px(spacing(Spacing::Lg, Density::Cozy))
            .font_family(body_family())
            .text_size(FONT_SM)
            .text_color(palette.text_muted)
            .child(tr!("action_editor_loading"))
            .into_any_element()
    }

    pub(super) fn overlay_order_at_risk(&self, action: &Action) -> bool {
        self.concurrent_queue_ids
            .contains(&action.queue_id.to_string())
            && analyzer::sends_order_sensitive_overlay(
                &action.sub_actions,
                &self.sub_action_registry,
                &|identity| self.overlay_schema.is_order_sensitive(identity),
            )
    }

    fn render_order_warning(&self, action: &Action, palette: &ForgePalette) -> Option<AnyElement> {
        self.overlay_order_at_risk(action).then(|| {
            inline_warning_card(
                tr!("action_editor_overlay_order_warning"),
                tr!("action_editor_overlay_order_warning_hint"),
                palette,
            )
        })
    }

    fn render_editor(
        &self,
        detail: &ActionDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, Density::Cozy))
            .child(self.render_editor_header(detail, palette, cx))
            .children(self.render_order_warning(&detail.action, palette))
            .child(self.render_triggers_section(detail, palette, cx))
            .child(self.render_sub_actions_section(detail, palette, cx));

        div()
            .id("actions-editor-scroll")
            .flex_1()
            .h_full()
            .overflow_y_scroll()
            .py(PANE_PAD_V)
            .px(PANE_PAD_H)
            .child(body)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use forge_components::ThemeId;
    use forge_runtime::{ActionCancelRegistry, QueueScheduler, spawn_action_engine};
    use forge_soundboard::ClipLibrary;
    use forge_storage::globals::MockGlobalsRepo;
    use forge_storage::queue::MockQueueRepo;
    use forge_storage::script::MockScriptRepo;
    use forge_storage::soundboard::MockSoundboardClipsRepo;
    use forge_storage::{
        DataProvider, MockMediaRepo, MockTriggerInstanceRepo, SoundboardClipsRepo, StoredClip,
    };

    use super::*;
    use crate::presentation::Presentation;
    use crate::test_support::{
        StubActions, StubEventLog, StubHistory, StubOverlays, stub_catalog, test_backend_with_media,
    };

    pub(super) fn view_with(
        cx: &mut gpui::TestAppContext,
        rt: &tokio::runtime::Runtime,
        clips: Vec<StoredClip>,
        media: MockMediaRepo,
        sub_actions: SubActionRegistry,
        action_repo: Arc<dyn ActionRepo>,
    ) -> Entity<ScreenActionsView> {
        cx.update(|cx| {
            cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
        });
        let mut queues = MockQueueRepo::new();
        queues.expect_list().returning(|| Ok(Vec::new()));
        let queue_repo: Arc<dyn QueueRepo> = Arc::new(queues);
        let mut triggers = MockTriggerInstanceRepo::new();
        triggers.expect_list_all().returning(|| Ok(Vec::new()));
        let trigger_repo: Arc<dyn TriggerInstanceRepo> = Arc::new(triggers);
        let mut scripts = MockScriptRepo::new();
        scripts.expect_list().returning(|| Ok(Vec::new()));
        let mut globals = MockGlobalsRepo::new();
        globals.expect_list().returning(|| Ok(Vec::new()));
        let mut clip_repo = MockSoundboardClipsRepo::new();
        clip_repo.expect_list().returning(move || Ok(clips.clone()));
        let clip_repo: Arc<dyn SoundboardClipsRepo> = Arc::new(clip_repo);
        let service = Arc::new(ActionsService::new(
            Arc::clone(&action_repo),
            Arc::clone(&queue_repo),
            Arc::new(StubHistory),
            Arc::clone(&trigger_repo),
            Arc::clone(&clip_repo),
        ));
        let (backend, _writes) = test_backend_with_media(Arc::new(media));
        let (bus, scheduler) = rt.block_on(async {
            let bus = EventBus::new(Arc::new(StubEventLog));
            let engine = spawn_action_engine(
                Arc::clone(&bus),
                stub_catalog(),
                Arc::new(StubActions),
                Arc::new(StubHistory),
                Arc::new(SubActionRegistry::new()),
                Arc::new(ActionCancelRegistry::new()),
            );
            let scheduler = QueueScheduler::spawn(engine, Arc::clone(&bus), Vec::new());
            (bus, scheduler)
        });
        let handle = rt.handle().clone();
        cx.update(|cx| {
            cx.new(|cx| {
                ScreenActionsView::new(
                    action_repo,
                    queue_repo,
                    service,
                    trigger_repo,
                    Arc::new(scripts),
                    Arc::new(ClipLibrary::new(clip_repo, backend.media_repo())),
                    Arc::new(globals),
                    Arc::clone(&backend) as Arc<dyn SettingsRepo>,
                    Arc::new(StubOverlays),
                    Arc::new(OverlayKindRegistry::new()),
                    None,
                    None,
                    Arc::new(sub_actions),
                    Arc::new(TriggerRegistry::new()),
                    handle,
                    bus,
                    scheduler,
                    None,
                    cx,
                )
            })
        })
    }
}
