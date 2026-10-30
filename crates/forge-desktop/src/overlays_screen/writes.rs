use std::sync::Arc;

use forge_components::{ToastKind, tr};
use forge_overlay::MediaIssue;
use forge_runtime::{OverlaySampleSource, OverlayServiceHandle};
use forge_storage::{OverlayConfig, OverlayDefinition, OverlayId};
use gpui::Context;

use super::OverlaysView;
use crate::async_bridge;
use crate::repo_failure::RepoFailure;
use forge_components::PushToast;

#[derive(Default)]
pub(super) struct Regenerated {
    failure: Option<String>,
    missing: Vec<String>,
    media_issues: Vec<MediaIssue>,
}

pub(super) async fn regenerate(
    service: &OverlayServiceHandle,
    samples: &dyn OverlaySampleSource,
    id: &OverlayId,
) -> Regenerated {
    match service.materialize(id, samples).await {
        Ok(report) => Regenerated {
            failure: None,
            missing: report.missing_overrides,
            media_issues: report.media_issues,
        },
        Err(error) => Regenerated {
            failure: Some(RepoFailure::from(error).text()),
            missing: Vec::new(),
            media_issues: Vec::new(),
        },
    }
}

impl OverlaysView {
    pub(super) fn apply_regenerated(
        &mut self,
        id: &OverlayId,
        regenerated: Regenerated,
        cx: &mut Context<Self>,
    ) {
        self.note_missing_overrides(regenerated.missing);
        self.set_media_issues(id, regenerated.media_issues, cx);
        self.invalidate_source(cx);
        if let Some(message) = regenerated.failure {
            self.report(&message, cx);
        }
    }

    pub(super) fn save_config(
        &mut self,
        id: OverlayId,
        config: OverlayConfig,
        cx: &mut Context<Self>,
    ) {
        let service = self.handles.service.clone();
        let samples = Arc::clone(&self.handles.samples);
        let target = id.clone();
        async_bridge::run_async(
            &self.handles.rt_handle,
            async move {
                let stored = service
                    .update_config(&id, &config)
                    .await
                    .map_err(RepoFailure::from)?;
                if !stored {
                    return Ok((false, Regenerated::default()));
                }
                Ok((true, regenerate(&service, samples.as_ref(), &id).await))
            },
            move |this, result: Result<(bool, Regenerated), RepoFailure>, cx| match result {
                Ok((true, regenerated)) => {
                    this.apply_regenerated(&target, regenerated, cx);
                    this.load(cx);
                }
                Ok((false, _)) => this.report(&tr!("overlays_toast_missing"), cx),
                Err(message) => this.report(&message.text(), cx),
            },
            cx,
        );
    }

    pub(super) fn create(&mut self, display_name: String, kind_id: String, cx: &mut Context<Self>) {
        let Some(schema_version) = self
            .handles
            .kinds
            .get(&kind_id)
            .map(|descriptor| descriptor.config_schema_version())
        else {
            self.report(&tr!("overlays_toast_unknown_type"), cx);
            return;
        };
        if forge_storage::is_token_shaped(&display_name) {
            self.report(&tr!("overlays_toast_token_shaped_name"), cx);
            return;
        }
        self.close_form(cx);

        let repo = Arc::clone(&self.handles.repo);
        let service = self.handles.service.clone();
        let samples = Arc::clone(&self.handles.samples);
        async_bridge::run_async(
            &self.handles.rt_handle,
            async move {
                let definition = repo
                    .create(&display_name, &kind_id, schema_version)
                    .await
                    .map_err(RepoFailure::from)?;
                let regenerated = regenerate(&service, samples.as_ref(), &definition.id).await;
                Ok((definition, regenerated))
            },
            |this, result: Result<(OverlayDefinition, Regenerated), RepoFailure>, cx| match result {
                Ok((definition, regenerated)) => {
                    let created = definition.id;
                    this.registry.selected = Some(created.clone());
                    cx.push_toast(ToastKind::Success, tr!("overlays_toast_created"));
                    this.apply_regenerated(&created, regenerated, cx);
                    this.load(cx);
                }
                Err(message) => this.report(&message.text(), cx),
            },
            cx,
        );
    }

    pub(super) fn rename(&mut self, id: OverlayId, display_name: String, cx: &mut Context<Self>) {
        if forge_storage::is_token_shaped(&display_name) {
            self.report(&tr!("overlays_toast_token_shaped_name"), cx);
            return;
        }
        self.close_form(cx);
        let service = self.handles.service.clone();
        let samples = Arc::clone(&self.handles.samples);
        let target = id.clone();
        async_bridge::run_async(
            &self.handles.rt_handle,
            async move {
                let renamed = service
                    .rename(&id, &display_name)
                    .await
                    .map_err(RepoFailure::from)?;
                if !renamed {
                    return Ok((false, Regenerated::default()));
                }
                Ok((true, regenerate(&service, samples.as_ref(), &id).await))
            },
            move |this, result: Result<(bool, Regenerated), RepoFailure>, cx| match result {
                Ok((true, regenerated)) => {
                    cx.push_toast(ToastKind::Success, tr!("overlays_toast_renamed"));
                    this.apply_regenerated(&target, regenerated, cx);
                    this.load(cx);
                }
                Ok((false, _)) => this.report(&tr!("overlays_toast_missing"), cx),
                Err(message) => this.report(&message.text(), cx),
            },
            cx,
        );
    }
}
