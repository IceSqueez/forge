pub const MARKUP_FILE: &str = "index.html";
pub const STYLE_FILE: &str = "overlay.css";
pub const BEHAVIOR_FILE: &str = "overlay.js";

/// The config document is data, so it is regenerated even when every source file is overridden.
pub const CONFIG_FILE: &str = "config.json";

pub const OVERRIDABLE_FILES: &[&str] = &[MARKUP_FILE, STYLE_FILE, BEHAVIOR_FILE];

pub const RESERVED_DIRECTORY: &str = "forge-shared";

/// Generated markup references this by a literal relative path, so a page keeps the runtime it shipped against.
pub const RUNTIME_ASSET: &str = "runtime-v1.js";

pub const RUNTIME_SOURCE: &str = include_str!("../assets/shared/runtime-v1.js");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageAssets {
    pub markup: &'static str,
    pub style: &'static str,
    pub behavior: &'static str,
}

impl PageAssets {
    pub fn files(&self) -> [(&'static str, &'static str); 3] {
        [
            (MARKUP_FILE, self.markup),
            (STYLE_FILE, self.style),
            (BEHAVIOR_FILE, self.behavior),
        ]
    }
}
