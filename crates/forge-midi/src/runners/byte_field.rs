use forge_registry::{
    ConfigReason, FieldLiteral, FormField, IntegerEditor, IntegerField, TypedField, ValueBounds,
};
use forge_types::{SubActionConfig, Variant};

pub(crate) struct ByteField {
    pub(crate) key: &'static str,
    pub(crate) label: &'static str,
    pub(crate) default: i64,
    pub(crate) max: i64,
    pub(crate) seeds_default: bool,
}

impl ByteField {
    const fn as_integer(&self) -> IntegerField {
        IntegerField {
            key: self.key,
            label: self.label,
            editor: IntegerEditor::Number,
            default: self.default,
            bounds: ValueBounds {
                min: Some(0),
                max: Some(self.max),
            },
        }
    }

    fn narrowed(&self, value: i64) -> Result<u8, ConfigReason> {
        u8::try_from(value).map_err(|_| ConfigReason::OutOfRange(self.as_integer().bounds))
    }
}

impl TypedField for ByteField {
    type Value = u8;

    fn key(&self) -> &'static str {
        self.key
    }

    fn form_field(&self) -> FormField {
        self.as_integer().form_field()
    }

    fn default_value(&self) -> Variant {
        self.as_integer().default_value()
    }

    fn write_default(&self, config: &mut SubActionConfig) {
        if self.seeds_default {
            config.insert(self.key().to_owned(), self.default_value());
        }
    }

    fn literal<'a>(&self, value: &'a Variant) -> Result<FieldLiteral<'a, u8>, ConfigReason> {
        Ok(match self.as_integer().literal(value)? {
            FieldLiteral::Value(number) => FieldLiteral::Value(self.narrowed(number)?),
            FieldLiteral::Template(text) => FieldLiteral::Template(text),
        })
    }

    fn parse_text(&self, text: &str) -> Result<u8, ConfigReason> {
        self.narrowed(self.as_integer().parse_text(text)?)
    }
}
