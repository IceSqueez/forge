use super::Validator;
use crate::harness::scenario::step::StepAction;

impl<'a> Validator<'a> {
    pub(super) fn require_fake_vtube(&mut self, location: &str) {
        if self.scenario.fakes.vtube.is_none() {
            self.report(
                location,
                "needs a fake VTube Studio: add fixture.vtube and fakes.vtube",
            );
        }
    }

    pub(super) fn check_vtube_action(&mut self, location: &str, action: &StepAction) {
        let vtube = self.scenario.fakes.vtube.as_ref();
        let undeclared = |field: &str, value: &str, list: &str| {
            (
                format!("{location}.{field}"),
                format!("names `{value}`, which fakes.vtube.{list} does not list"),
            )
        };
        let problem = match action {
            StepAction::VtubeOnline {} => {
                let already = self.vtube_online;
                self.vtube_online = true;
                already.then(|| {
                    (
                        location.to_owned(),
                        "the fake VTube Studio is already running; set fakes.vtube.online_at_boot to false to start it from a step".to_owned(),
                    )
                })
            }
            StepAction::VtubeAuthenticated { within_ms } => {
                self.check_within(location, *within_ms);
                None
            }
            StepAction::VtubeHotkey { hotkey }
                if vtube.is_some_and(|v| !v.declares_hotkey(hotkey)) =>
            {
                Some(undeclared("hotkey", hotkey, "models[].hotkeys"))
            }
            StepAction::VtubeModelLoad { model }
                if vtube.is_some_and(|v| !v.declares_model(model)) =>
            {
                Some(undeclared("model", model, "models"))
            }
            StepAction::VtubeItemAdded { file } | StepAction::VtubeItemRemoved { file }
                if vtube.is_some_and(|v| !v.declares_item(file)) =>
            {
                Some(undeclared("file", file, "items"))
            }
            StepAction::VtubeExpression { file, .. }
                if vtube.is_some_and(|v| !v.declares_expression(file)) =>
            {
                Some(undeclared("file", file, "models[].expressions"))
            }
            _ => None,
        };
        if let Some((location, message)) = problem {
            self.report(location, message);
        }
    }
}
