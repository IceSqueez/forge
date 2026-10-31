use super::ScriptEditorView;
use forge_components::tr;
use forge_script::{ScriptError, inert_annotation_lines, validate_syntax};
use gpui::prelude::*;
use gpui::{Context, SharedString};
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ProblemLevel {
    Error,
    Info,
}

pub(super) struct EditorProblem {
    pub(super) level: ProblemLevel,
    pub(super) text: SharedString,
}

fn compile_problem(body: &str) -> Option<String> {
    match validate_syntax(body) {
        Ok(()) => None,
        Err(ScriptError::Compile { reason, .. }) => Some(reason),
        Err(other) => Some(other.to_string()),
    }
}

impl ScriptEditorView {
    pub(super) fn recompute_diagnostics(&mut self, body: &str, cx: &mut Context<Self>) {
        self.compile_error = None;
        self.refresh_annotations(body, cx);
        self.schedule_compile_check(body.to_owned(), Duration::ZERO, cx);
    }

    pub(super) fn schedule_compile_check(
        &mut self,
        body: String,
        debounce: Duration,
        cx: &mut Context<Self>,
    ) {
        self._compile_check = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(debounce).await;
            let compile_error = cx
                .background_spawn(async move { compile_problem(&body) })
                .await;
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
            let _ = this.update(cx, |this, cx| {
                this.compile_error = compile_error.map(SharedString::from);
                this.rebuild_problems();
                cx.notify();
            });
        }));
    }

    pub(super) fn refresh_annotations(&mut self, body: &str, cx: &mut Context<Self>) {
        self.problem_lines = inert_annotation_lines(body);
        self.rebuild_problems();

        let marks = self.problem_lines.clone();
        self.code_input
            .update(cx, |area, cx| area.set_gutter_marks(marks, cx));
    }

    fn rebuild_problems(&mut self) {
        let mut problems = Vec::with_capacity(self.problem_lines.len() + 1);
        if let Some(reason) = &self.compile_error {
            problems.push(EditorProblem {
                level: ProblemLevel::Error,
                text: reason.clone(),
            });
        }
        problems.extend(self.problem_lines.iter().map(|line| {
            EditorProblem {
                level: ProblemLevel::Info,
                text: format!(
                    "Ln {} · {}",
                    line + 1,
                    tr!("script_editor_inert_annotation")
                )
                .into(),
            }
        }));
        self.problems = problems;
    }
}
