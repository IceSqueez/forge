use forge_registry::{ConfigReason, FieldLiteral, FormField, IntegerField, TypedField};
use forge_types::Variant;

pub(super) struct VolumeField {
    pub(super) integer: IntegerField,
}

impl VolumeField {
    fn whole_number(value: f64) -> Result<i64, ConfigReason> {
        if value.is_finite() && value.fract() == 0.0 {
            Ok(value as i64)
        } else {
            Err(ConfigReason::NotANumber)
        }
    }
}

impl TypedField for VolumeField {
    type Value = i64;

    fn key(&self) -> &'static str {
        self.integer.key()
    }

    fn form_field(&self) -> FormField {
        self.integer.form_field()
    }

    fn default_value(&self) -> Variant {
        self.integer.default_value()
    }

    fn literal<'a>(&self, value: &'a Variant) -> Result<FieldLiteral<'a, i64>, ConfigReason> {
        match value {
            Variant::Float(number) => {
                let whole = Self::whole_number(*number)?;
                match self.integer.literal(&Variant::Int(whole))? {
                    FieldLiteral::Value(bounded) => Ok(FieldLiteral::Value(bounded)),
                    FieldLiteral::Template(_) => Err(ConfigReason::NotANumber),
                }
            }
            other => self.integer.literal(other),
        }
    }

    fn parse_text(&self, text: &str) -> Result<i64, ConfigReason> {
        self.integer.parse_text(text)
    }
}
