use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FakeInput {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub muted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct FakeObsConfig {
    pub password: Option<String>,
    pub scenes: Vec<String>,
    pub current_scene: Option<String>,
    pub inputs: Vec<FakeInput>,
    pub online_at_boot: bool,
}

impl Default for FakeObsConfig {
    fn default() -> Self {
        Self {
            password: None,
            scenes: vec!["Main".to_owned(), "BRB".to_owned()],
            current_scene: None,
            inputs: vec![FakeInput {
                name: "Mic/Aux".to_owned(),
                kind: "pulse_input_capture".to_owned(),
                muted: false,
            }],
            online_at_boot: true,
        }
    }
}

impl FakeObsConfig {
    pub fn problems(&self) -> Vec<(String, String)> {
        let mut problems = Vec::new();
        if self.scenes.is_empty() {
            problems.push((
                "scenes".to_owned(),
                "needs at least one scene: OBS always has a program scene".to_owned(),
            ));
        }
        for (index, scene) in self.scenes.iter().enumerate() {
            if scene.trim().is_empty() {
                problems.push((format!("scenes[{index}]"), "must not be blank".to_owned()));
            } else if self.scenes[..index].contains(scene) {
                problems.push((
                    format!("scenes[{index}]"),
                    format!("repeats `{scene}`; OBS scene names are unique"),
                ));
            }
        }
        if let Some(current) = &self.current_scene
            && !self.scenes.contains(current)
        {
            problems.push((
                "current_scene".to_owned(),
                format!("names `{current}`, which `scenes` does not list"),
            ));
        }
        for (index, input) in self.inputs.iter().enumerate() {
            if input.name.trim().is_empty() || input.kind.trim().is_empty() {
                problems.push((
                    format!("inputs[{index}]"),
                    "needs a non-blank name and kind".to_owned(),
                ));
            } else if self.inputs[..index]
                .iter()
                .any(|earlier| earlier.name == input.name)
            {
                problems.push((
                    format!("inputs[{index}]"),
                    format!("repeats `{}`; OBS input names are unique", input.name),
                ));
            }
        }
        if self.password.as_deref() == Some("") {
            problems.push((
                "password".to_owned(),
                "must not be empty; omit it to run OBS without authentication".to_owned(),
            ));
        }
        problems
    }

    pub fn declares_scene(&self, scene: &str) -> bool {
        self.scenes.iter().any(|known| known == scene)
    }

    pub fn declares_input(&self, input: &str) -> bool {
        self.inputs.iter().any(|known| known.name == input)
    }
}
