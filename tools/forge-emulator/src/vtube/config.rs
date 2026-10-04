use serde::{Deserialize, Serialize};

use super::protocol::EXPRESSION_FILE_SUFFIX;

pub const DEFAULT_TOKEN: &str = "emulator-vtube-token";
const MAX_TOKEN_CHARS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FakeModel {
    pub name: String,
    #[serde(default)]
    pub hotkeys: Vec<String>,
    #[serde(default)]
    pub expressions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct FakeVTubeConfig {
    pub token: String,
    pub approve_token_requests: bool,
    pub models: Vec<FakeModel>,
    pub current_model: Option<String>,
    pub items: Vec<String>,
    pub parameters: Vec<String>,
    pub face_found: bool,
    pub online_at_boot: bool,
}

impl Default for FakeVTubeConfig {
    fn default() -> Self {
        Self {
            token: DEFAULT_TOKEN.to_owned(),
            approve_token_requests: true,
            models: vec![FakeModel {
                name: "Emulator Avatar".to_owned(),
                hotkeys: vec!["Wave".to_owned(), "Blush".to_owned()],
                expressions: vec!["Blush.exp3.json".to_owned(), "Smile.exp3.json".to_owned()],
            }],
            current_model: None,
            items: vec!["emulator_star.png".to_owned()],
            parameters: ["FaceAngleX", "FaceAngleY", "MouthOpen", "MouthSmile"]
                .map(str::to_owned)
                .to_vec(),
            face_found: true,
            online_at_boot: true,
        }
    }
}

impl FakeVTubeConfig {
    pub fn problems(&self) -> Vec<(String, String)> {
        let mut problems = Vec::new();
        if self.token.trim().is_empty() {
            problems.push(("token".to_owned(), "must not be blank".to_owned()));
        } else if !self.token.is_ascii() || self.token.len() > MAX_TOKEN_CHARS {
            problems.push((
                "token".to_owned(),
                format!("must be ASCII and at most {MAX_TOKEN_CHARS} characters, as VTube Studio tokens are"),
            ));
        }
        for (index, model) in self.models.iter().enumerate() {
            let location = format!("models[{index}]");
            if model.name.trim().is_empty() {
                problems.push((format!("{location}.name"), "must not be blank".to_owned()));
            } else if self.models[..index]
                .iter()
                .any(|earlier| earlier.name == model.name)
            {
                problems.push((
                    format!("{location}.name"),
                    format!("repeats `{}`; the fake names each model once", model.name),
                ));
            }
            unique_names(
                &mut problems,
                &format!("{location}.hotkeys"),
                &model.hotkeys,
            );
            unique_names(
                &mut problems,
                &format!("{location}.expressions"),
                &model.expressions,
            );
            for (position, file) in model.expressions.iter().enumerate() {
                if !file.ends_with(EXPRESSION_FILE_SUFFIX) {
                    problems.push((
                        format!("{location}.expressions[{position}]"),
                        format!("`{file}` must end in {EXPRESSION_FILE_SUFFIX}"),
                    ));
                }
            }
        }
        if let Some(current) = &self.current_model
            && !self.declares_model(current)
        {
            problems.push((
                "current_model".to_owned(),
                format!("names `{current}`, which `models` does not list"),
            ));
        }
        unique_names(&mut problems, "items", &self.items);
        unique_names(&mut problems, "parameters", &self.parameters);
        problems
    }

    pub fn declares_model(&self, name: &str) -> bool {
        self.models.iter().any(|model| model.name == name)
    }

    pub fn declares_hotkey(&self, hotkey: &str) -> bool {
        self.models
            .iter()
            .any(|model| model.hotkeys.iter().any(|known| known == hotkey))
    }

    pub fn declares_expression(&self, file: &str) -> bool {
        self.models
            .iter()
            .any(|model| model.expressions.iter().any(|known| known == file))
    }

    pub fn declares_item(&self, file: &str) -> bool {
        self.items.iter().any(|known| known == file)
    }
}

fn unique_names(problems: &mut Vec<(String, String)>, location: &str, names: &[String]) {
    for (index, name) in names.iter().enumerate() {
        if name.trim().is_empty() {
            problems.push((
                format!("{location}[{index}]"),
                "must not be blank".to_owned(),
            ));
        } else if names[..index].contains(name) {
            problems.push((format!("{location}[{index}]"), format!("repeats `{name}`")));
        }
    }
}
