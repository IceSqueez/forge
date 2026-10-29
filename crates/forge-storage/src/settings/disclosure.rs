use forge_types::redaction::STAMP;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingDisclosure {
    Verbatim,
    Kind,
    Presence,
    Withheld,
}

impl SettingDisclosure {
    pub fn render(self, value: &str) -> Option<String> {
        match self {
            Self::Verbatim => Some(value.to_owned()),
            Self::Kind => Some(shape_of(value)),
            Self::Presence => Some(if value.is_empty() { "unset" } else { "set" }.to_owned()),
            Self::Withheld => None,
        }
    }
}

fn shape_of(value: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(value) {
        Ok(serde_json::Value::Array(items)) => format!("list({})", items.len()),
        Ok(serde_json::Value::Object(fields)) => format!("map({})", fields.len()),
        _ => format!("{STAMP} len={}>", value.chars().count()),
    }
}

#[cfg(test)]
mod tests {
    use super::SettingDisclosure;

    const SECRET: &str = "nova_the_broadcaster";

    #[test]
    fn a_kind_class_discloses_only_the_container_shape() {
        for (value, expected) in [
            (r#"["a.com","b.com"]"#, "list(2)"),
            ("[]", "list(0)"),
            (r#"{"a":1,"b":2}"#, "map(2)"),
            ("{}", "map(0)"),
            (SECRET, "<redacted len=20>"),
            ("Привіт", "<redacted len=6>"),
            ("", "<redacted len=0>"),
        ] {
            assert_eq!(
                SettingDisclosure::Kind.render(value).as_deref(),
                Some(expected),
                "value {value:?}",
            );
        }
    }

    #[test]
    fn a_presence_class_discloses_only_whether_a_value_exists() {
        for (value, expected) in [("", "unset"), (" ", "set"), (SECRET, "set")] {
            assert_eq!(
                SettingDisclosure::Presence.render(value).as_deref(),
                Some(expected),
                "value {value:?}",
            );
        }
    }

    #[test]
    fn verbatim_shows_the_value_and_withheld_shows_nothing() {
        assert_eq!(
            (
                SettingDisclosure::Verbatim.render(SECRET),
                SettingDisclosure::Withheld.render(SECRET),
            ),
            (Some(SECRET.to_owned()), None)
        );
    }
}
