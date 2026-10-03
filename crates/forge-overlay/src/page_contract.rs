use crate::assets::RUNTIME_ASSET;
use crate::config::{
    ACCENT, FONT, MARGIN_BOTTOM, MARGIN_LEFT, MARGIN_RIGHT, MARGIN_TOP, POSITION, TEXT_SIZE,
};
use crate::descriptor::{ConfigSection, OverlayKindDescriptor};
use crate::metrics::{
    MARGIN_BOTTOM_PROPERTY, MARGIN_LEFT_PROPERTY, MARGIN_RIGHT_PROPERTY, MARGIN_TOP_PROPERTY,
    TEXT_SIZE_PROPERTY,
};

pub const PAGE_NAMESPACE: &str = "forge";
pub const BIND_ATTRIBUTE: &str = "data-bind";
pub const HIDDEN_CLASS: &str = "hidden";

pub const READY_FUNCTION: &str = "ready";
pub const CONTENT_FUNCTION: &str = "content";
pub const SET_FUNCTION: &str = "set";
pub const SHOW_FUNCTION: &str = "show";
pub const SOUND_FUNCTION: &str = "sound";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageFunction {
    pub name: &'static str,
    pub calls: &'static [&'static str],
    pub callback: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BindRule {
    pub attribute: &'static str,
    pub writer: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeProperty {
    pub property: &'static str,
    pub config_key: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeAttribute {
    pub element: &'static str,
    pub attribute: &'static str,
    pub config_key: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageContract {
    pub runtime_asset: &'static str,
    pub namespace: &'static str,
    pub functions: &'static [PageFunction],
    pub bind: BindRule,
    pub custom_properties: &'static [RuntimeProperty],
    pub body_attribute: RuntimeAttribute,
    pub hidden_class: &'static str,
}

impl PageContract {
    pub fn applies_config_key(&self, key: &str) -> bool {
        self.body_attribute.config_key == key
            || self
                .custom_properties
                .iter()
                .any(|property| property.config_key == key)
    }
}

pub const PAGE_CONTRACT: PageContract = PageContract {
    runtime_asset: RUNTIME_ASSET,
    namespace: PAGE_NAMESPACE,
    functions: &[
        PageFunction {
            name: READY_FUNCTION,
            calls: &["forge.ready(callback)"],
            callback: Some("callback(config)"),
        },
        PageFunction {
            name: CONTENT_FUNCTION,
            calls: &["forge.content(callback)"],
            callback: Some("callback(values, durationMs)"),
        },
        PageFunction {
            name: SET_FUNCTION,
            calls: &["forge.set(name, text)"],
            callback: None,
        },
        PageFunction {
            name: SHOW_FUNCTION,
            calls: &["forge.show(target)", "forge.show(target, ms)"],
            callback: None,
        },
        PageFunction {
            name: SOUND_FUNCTION,
            calls: &["forge.sound(name)"],
            callback: None,
        },
    ],
    bind: BindRule {
        attribute: BIND_ATTRIBUTE,
        writer: SET_FUNCTION,
    },
    custom_properties: &[
        RuntimeProperty {
            property: "--accent",
            config_key: ACCENT,
        },
        RuntimeProperty {
            property: "--font",
            config_key: FONT,
        },
        RuntimeProperty {
            property: TEXT_SIZE_PROPERTY,
            config_key: TEXT_SIZE,
        },
        RuntimeProperty {
            property: MARGIN_TOP_PROPERTY,
            config_key: MARGIN_TOP,
        },
        RuntimeProperty {
            property: MARGIN_RIGHT_PROPERTY,
            config_key: MARGIN_RIGHT,
        },
        RuntimeProperty {
            property: MARGIN_BOTTOM_PROPERTY,
            config_key: MARGIN_BOTTOM,
        },
        RuntimeProperty {
            property: MARGIN_LEFT_PROPERTY,
            config_key: MARGIN_LEFT,
        },
    ],
    body_attribute: RuntimeAttribute {
        element: "body",
        attribute: "data-position",
        config_key: POSITION,
    },
    hidden_class: HIDDEN_CLASS,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LookConfigKey {
    pub key: &'static str,
    pub section: ConfigSection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookContract {
    pub kind_id: String,
    pub content_keys: Vec<&'static str>,
    pub config_keys: Vec<LookConfigKey>,
}

pub fn look_contract(descriptor: &dyn OverlayKindDescriptor) -> LookContract {
    let mut content_keys = Vec::new();
    let mut config_keys = Vec::new();
    for sectioned in descriptor.look_fields() {
        let key = sectioned.field.key();
        match sectioned.section {
            ConfigSection::Content => content_keys.push(key),
            section if !PAGE_CONTRACT.applies_config_key(key) => {
                config_keys.push(LookConfigKey { key, section });
            }
            _ => {}
        }
    }
    LookContract {
        kind_id: descriptor.id().to_owned(),
        content_keys,
        config_keys,
    }
}
