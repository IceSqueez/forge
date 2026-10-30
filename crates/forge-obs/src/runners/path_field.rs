use forge_registry::runner::SubActionConfig;
use forge_registry::{ConfigProblem, ConfigReason, PickedTextField, TypedField};
use forge_types::{ArgStack, Variant};

use crate::vocabulary::messages;

const HOME_PREFIX: char = '~';
const PARENT_SEGMENT: &str = "..";
const VARIABLE_DELIMITER: char = '%';

fn template_of<'a>(field: &PickedTextField, config: &'a SubActionConfig) -> &'a str {
    match config.get(field.key) {
        Some(Variant::String(text)) => text,
        _ => field.default,
    }
}

fn has_parent_segment(path: &str) -> bool {
    path.split(['/', '\\'])
        .any(|segment| segment == PARENT_SEGMENT)
}

fn is_rooted(path: &str) -> bool {
    path.starts_with(['/', '\\']) || path.as_bytes().get(1) == Some(&b':')
}

fn refuse(field: &PickedTextField, message: forge_types::MessageRef) -> ConfigProblem {
    ConfigProblem::field(field.key, ConfigReason::Coded(message))
}

pub(crate) fn check_template(
    field: &PickedTextField,
    config: &SubActionConfig,
) -> Result<(), ConfigProblem> {
    if template_of(field, config)
        .trim_start()
        .starts_with(HOME_PREFIX)
    {
        return Err(refuse(field, messages::path_home_relative()));
    }
    Ok(())
}

pub(crate) fn read_path(
    field: &PickedTextField,
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<String, ConfigProblem> {
    check_template(field, config)?;
    let resolved = field.read(config, args)?;
    let template = template_of(field, config);
    if resolved != template {
        let escapes_by_parent = has_parent_segment(&resolved) && !has_parent_segment(template);
        let escapes_by_root = template.starts_with(VARIABLE_DELIMITER) && is_rooted(&resolved);
        if escapes_by_parent || escapes_by_root {
            return Err(refuse(field, messages::path_escapes_template()));
        }
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use forge_registry::TextPicker;

    use super::*;

    const PATH: PickedTextField = PickedTextField {
        key: "path",
        label: "Path",
        picker: TextPicker::File,
        default: "",
        required: false,
    };

    fn config_with(template: &str) -> SubActionConfig {
        let mut config = SubActionConfig::new();
        config.insert(PATH.key.to_owned(), Variant::String(template.to_owned()));
        config
    }

    fn resolve(template: &str, value: &str) -> Result<String, ConfigProblem> {
        let args = ArgStack::new().set("v".to_owned(), Variant::String(value.to_owned()));
        read_path(&PATH, &config_with(template), &args)
    }

    fn refusal_name(problem: &ConfigProblem) -> &str {
        assert_eq!(problem.field.as_deref(), Some(PATH.key));
        let ConfigReason::Coded(message) = &problem.reason else {
            panic!("expected a coded refusal");
        };
        message.code().name()
    }

    #[test]
    fn a_home_relative_template_is_refused_because_obs_does_not_expand_it() {
        for template in ["~/clips", "  ~/clips", "~"] {
            let problem = check_template(&PATH, &config_with(template)).unwrap_err();
            assert_eq!(refusal_name(&problem), "path_home_relative", "{template:?}");
        }
    }

    #[test]
    fn a_tilde_inside_a_template_is_an_ordinary_character() {
        assert!(check_template(&PATH, &config_with("/mnt/~clips")).is_ok());
    }

    #[test]
    fn read_path_refuses_a_home_relative_template_before_resolving_it() {
        let problem = resolve("~/%v%", "clips").unwrap_err();

        assert_eq!(refusal_name(&problem), "path_home_relative");
    }

    #[test]
    fn a_variable_that_adds_a_parent_segment_is_refused() {
        for (template, value) in [
            ("/clips/%v%", "../secrets"),
            ("/clips/%v%", "a/../../secrets"),
            (r"C:\clips\%v%", r"..\secrets"),
            ("/clips/%v%", ".."),
        ] {
            let problem = resolve(template, value).unwrap_err();
            assert_eq!(
                refusal_name(&problem),
                "path_escapes_template",
                "{template} with {value}"
            );
        }
    }

    #[test]
    fn a_variable_that_opens_a_template_and_supplies_a_root_is_refused() {
        for value in ["/etc", r"\\server\share", r"C:\Windows", "C:/Windows"] {
            let problem = resolve("%v%/clips", value).unwrap_err();
            assert_eq!(refusal_name(&problem), "path_escapes_template", "{value}");
        }
    }

    #[test]
    fn paths_that_stay_inside_the_template_resolve_to_the_interpolated_text() {
        for (template, value, expected) in [
            ("/clips/%v%", "stream-01", "/clips/stream-01"),
            ("/clips/%v%", "a..b", "/clips/a..b"),
            ("/clips/%v%", "/nested", "/clips//nested"),
            ("%v%/clips", "relative", "relative/clips"),
        ] {
            assert_eq!(
                resolve(template, value).unwrap(),
                expected,
                "{template} with {value:?}"
            );
        }
    }

    #[test]
    fn a_parent_segment_the_author_typed_is_their_own_choice() {
        for (template, value, expected) in [
            ("/clips/../archive", "ignored", "/clips/../archive"),
            ("../clips/%v%", "stream", "../clips/stream"),
            ("../clips/%v%", "../more", "../clips/../more"),
        ] {
            assert_eq!(resolve(template, value).unwrap(), expected, "{template}");
        }
    }
}
