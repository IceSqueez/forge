use forge_components::{ftl_id, tr_find};
use forge_registry::{SubActionRunner, TriggerKindDescriptor};

const NAME_PREFIX: &str = "kind_name_";
const SUMMARY_PREFIX: &str = "kind_summary_";

pub(crate) fn step_name(runner: &dyn SubActionRunner) -> String {
    kind_copy(NAME_PREFIX, runner.id(), runner.label())
}

pub(crate) fn step_summary(runner: &dyn SubActionRunner) -> String {
    kind_copy(SUMMARY_PREFIX, runner.id(), runner.summary())
}

pub(crate) fn trigger_name(descriptor: &dyn TriggerKindDescriptor) -> String {
    kind_copy(NAME_PREFIX, descriptor.id(), descriptor.label())
}

pub(crate) fn trigger_summary(descriptor: &dyn TriggerKindDescriptor) -> String {
    kind_copy(SUMMARY_PREFIX, descriptor.id(), descriptor.summary())
}

pub(crate) fn trigger_instance_name(
    stored: &str,
    descriptor: Option<&dyn TriggerKindDescriptor>,
) -> String {
    match descriptor {
        Some(descriptor) if stored == descriptor.label() => trigger_name(descriptor),
        _ => stored.to_owned(),
    }
}

fn kind_copy(prefix: &str, kind_id: &str, english: &str) -> String {
    tr_find(&format!("{prefix}{}", ftl_id(kind_id)), None).unwrap_or_else(|| english.to_owned())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::rc::Rc;

    use fluent::{FluentBundle, FluentResource};
    use forge_components::install_bundle;
    use forge_registry::TriggerCategory;
    use forge_storage::Language;

    use super::*;
    use crate::i18n::install_language;
    use crate::integrations::tests::every_registry_entry;
    use crate::test_support::{Declares, StubTrigger};

    const EN_FTL: &str = include_str!("../locales/en/main.ftl");
    const UK_FTL: &str = include_str!("../locales/uk/main.ftl");
    const STUB_KIND: &str = "demo.chat.message";
    const STUB_LABEL: &str = "Chat Message";
    const STUB_FTL: &str = "\
kind_name_demo_chat_message = Повідомлення в чаті
kind_summary_demo_chat_message = Хтось написав у чат
";
    const UK_NAMES_SHARED_WITH_EN: &[&str] = &[
        "core.http.delete",
        "core.http.get",
        "core.http.patch",
        "core.http.post",
        "core.http.put",
        "midi.input.control_change",
        "midi.input.note_off",
        "midi.input.note_on",
        "midi.input.pitch_bend",
        "midi.input.program_change",
        "youtube.chat.super_chat",
        "youtube.chat.super_sticker",
    ];

    fn install_ftl(source: &str) {
        let mut bundle = FluentBundle::new(vec!["uk-UA".parse().unwrap()]);
        bundle
            .add_resource(FluentResource::try_new(source.to_owned()).unwrap())
            .unwrap();
        install_bundle(Rc::new(bundle));
    }

    fn stub(id: &str) -> StubTrigger {
        StubTrigger::new(id, STUB_LABEL, TriggerCategory::Chat, Declares::Nothing)
    }

    fn catalog_key(prefix: &str, kind_id: &str) -> String {
        format!("{prefix}{}", ftl_id(kind_id))
    }

    #[test]
    fn kind_copy_reads_the_dotted_kind_id_entry_for_name_and_summary() {
        install_ftl(STUB_FTL);
        let descriptor = stub(STUB_KIND);
        assert_eq!(
            (trigger_name(&descriptor), trigger_summary(&descriptor)),
            (
                "Повідомлення в чаті".to_owned(),
                "Хтось написав у чат".to_owned()
            )
        );
    }

    #[test]
    fn kind_copy_falls_back_to_the_descriptor_english_when_the_entry_is_missing() {
        install_ftl(STUB_FTL);
        let descriptor = stub("demo.chat.other");
        assert_eq!(
            (trigger_name(&descriptor), trigger_summary(&descriptor)),
            (STUB_LABEL.to_owned(), STUB_LABEL.to_owned())
        );
    }

    #[test]
    fn trigger_instance_name_translates_only_a_name_still_equal_to_the_default_label() {
        install_ftl(STUB_FTL);
        let descriptor = stub(STUB_KIND);
        for (stored, expected) in [
            (STUB_LABEL, "Повідомлення в чаті"),
            ("Raid shoutout", "Raid shoutout"),
            ("chat message", "chat message"),
            ("", ""),
        ] {
            assert_eq!(
                trigger_instance_name(stored, Some(&descriptor)),
                expected,
                "stored {stored:?}"
            );
        }
    }

    #[test]
    fn trigger_instance_name_keeps_the_stored_name_when_the_kind_is_unregistered() {
        install_ftl(STUB_FTL);
        assert_eq!(trigger_instance_name(STUB_LABEL, None), STUB_LABEL);
    }

    struct ResolvedKind {
        kind: &'static str,
        id: String,
        english_name: String,
        english_summary: String,
        name: String,
        summary: String,
    }

    impl ResolvedKind {
        fn describe(&self) -> String {
            format!("{} {}", self.kind, self.id)
        }
    }

    async fn every_kind_resolved_in(language: Language) -> Vec<ResolvedKind> {
        let (triggers, sub_actions) = every_registry_entry().await;
        install_language(language);
        let mut kinds: Vec<ResolvedKind> = triggers
            .all()
            .map(|descriptor| ResolvedKind {
                kind: "trigger",
                id: descriptor.id().to_owned(),
                english_name: descriptor.label().to_owned(),
                english_summary: descriptor.summary().to_owned(),
                name: trigger_name(descriptor),
                summary: trigger_summary(descriptor),
            })
            .collect();
        kinds.extend(sub_actions.all().map(|runner| ResolvedKind {
            kind: "step",
            id: runner.id().to_owned(),
            english_name: runner.label().to_owned(),
            english_summary: runner.summary().to_owned(),
            name: step_name(runner),
            summary: step_summary(runner),
        }));
        kinds
    }

    #[tokio::test]
    async fn every_registered_kind_resolves_in_en_to_exactly_its_descriptor_name_and_summary() {
        let kinds = every_kind_resolved_in(Language::En).await;
        let drifted: Vec<String> = kinds
            .iter()
            .filter(|kind| kind.name != kind.english_name || kind.summary != kind.english_summary)
            .map(|kind| {
                format!(
                    "{}: {:?} / {:?} -> {:?} / {:?}",
                    kind.describe(),
                    kind.english_name,
                    kind.english_summary,
                    kind.name,
                    kind.summary
                )
            })
            .collect();
        assert!(
            drifted.is_empty(),
            "{} en copy drifted from code: {drifted:#?}",
            drifted.len()
        );
    }

    #[tokio::test]
    async fn every_registered_kind_has_a_non_empty_uk_name_and_summary_entry() {
        let kinds = every_kind_resolved_in(Language::Uk).await;
        let missing: Vec<String> = kinds
            .iter()
            .flat_map(|kind| {
                [NAME_PREFIX, SUMMARY_PREFIX].map(|prefix| (kind, catalog_key(prefix, &kind.id)))
            })
            .filter(|(_, key)| tr_find(key, None).is_none_or(|value| value.trim().is_empty()))
            .map(|(kind, key)| format!("{}: {key}", kind.describe()))
            .collect();
        assert!(
            missing.is_empty(),
            "{} entries without uk copy: {missing:#?}",
            missing.len()
        );
    }

    #[tokio::test]
    async fn every_registered_kind_shows_a_uk_name_that_differs_from_english_unless_allow_listed() {
        let kinds = every_kind_resolved_in(Language::Uk).await;
        let untranslated: Vec<String> = kinds
            .iter()
            .filter(|kind| !UK_NAMES_SHARED_WITH_EN.contains(&kind.id.as_str()))
            .filter(|kind| kind.name == kind.english_name)
            .map(|kind| format!("{}: {:?}", kind.describe(), kind.name))
            .collect();
        assert!(
            untranslated.is_empty(),
            "{} uk names equal to English: {untranslated:#?}",
            untranslated.len()
        );
    }

    #[tokio::test]
    async fn every_kind_copy_entry_names_a_registered_kind() {
        let (triggers, sub_actions) = every_registry_entry().await;
        let registered: BTreeSet<String> = triggers
            .all()
            .map(|descriptor| descriptor.id().to_owned())
            .chain(sub_actions.all().map(|runner| runner.id().to_owned()))
            .flat_map(|kind_id| {
                [NAME_PREFIX, SUMMARY_PREFIX].map(|prefix| catalog_key(prefix, &kind_id))
            })
            .collect();
        let orphans: Vec<&str> = [EN_FTL, UK_FTL]
            .into_iter()
            .flat_map(str::lines)
            .filter(|line| line.starts_with(NAME_PREFIX) || line.starts_with(SUMMARY_PREFIX))
            .filter_map(|line| line.split_once(" = ").map(|(key, _)| key.trim_end()))
            .filter(|key| !registered.contains(*key))
            .collect();
        assert!(
            orphans.is_empty(),
            "{} kind copy entries name no registered kind: {orphans:#?}",
            orphans.len()
        );
    }
}
