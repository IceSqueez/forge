use forge_registry::{
    ConfigReason, FieldLiteral, FormField, PickedTextField, TextPicker, TypedField, invalid_id,
};
use forge_types::{ClipId, Variant};

use crate::vocabulary::{config_keys, picker_keys};

pub(super) struct ClipField {
    pub(super) required: bool,
}

impl ClipField {
    const fn as_text(&self) -> PickedTextField {
        PickedTextField {
            key: config_keys::CLIP_ID,
            label: "Clip",
            picker: TextPicker::Options {
                options_key: picker_keys::CLIP_IDS,
            },
            default: "",
            required: self.required,
        }
    }
}

impl TypedField for ClipField {
    type Value = Option<ClipId>;

    fn key(&self) -> &'static str {
        config_keys::CLIP_ID
    }

    fn form_field(&self) -> FormField {
        self.as_text().form_field()
    }

    fn default_value(&self) -> Variant {
        self.as_text().default_value()
    }

    fn literal<'a>(
        &self,
        value: &'a Variant,
    ) -> Result<FieldLiteral<'a, Option<ClipId>>, ConfigReason> {
        match self.as_text().literal(value)? {
            FieldLiteral::Value(text) => self.parse_text(&text).map(FieldLiteral::Value),
            FieldLiteral::Template(text) => Ok(FieldLiteral::Template(text)),
        }
    }

    fn parse_text(&self, text: &str) -> Result<Option<ClipId>, ConfigReason> {
        if text.is_empty() {
            return if self.required {
                Err(ConfigReason::Required)
            } else {
                Ok(None)
            };
        }
        text.parse::<ClipId>().map(Some).map_err(|_| invalid_id())
    }
}
