use std::collections::BTreeSet;
use std::time::Duration;

use forge_components::{PushToast, ToastKind, duplicate_message_ids, tr};
use forge_storage::Language;
use forge_types::{COMMON_MESSAGES, MessageCatalog, MessageCatalogError};
use gpui::{AppContext, Context};

use crate::i18n::{app_catalogs, catalog_language};

pub const REGISTERED_CATALOGS: &[MessageCatalog] = &[COMMON_MESSAGES];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogProblem {
    Malformed(MessageCatalogError),
    DuplicateNamespace(&'static str),
    DuplicateMessageId { language: Language, id: String },
}

impl CatalogProblem {
    pub fn diagnostic(&self) -> String {
        match self {
            Self::Malformed(error) => error.to_string(),
            Self::DuplicateNamespace(namespace) => {
                format!("namespace {namespace} registered twice")
            }
            Self::DuplicateMessageId { language, id } => format!("{language}: {id} defined twice"),
        }
    }
}

pub fn fragments(lang: Language) -> impl Iterator<Item = &'static str> {
    REGISTERED_CATALOGS.iter().map(move |catalog| match lang {
        Language::En => catalog.en_ftl,
        Language::Uk => catalog.uk_ftl,
    })
}

pub fn catalog_problems(catalogs: &[MessageCatalog]) -> Vec<CatalogProblem> {
    let mut problems: Vec<CatalogProblem> = catalogs
        .iter()
        .filter_map(|catalog| catalog.check().err())
        .map(CatalogProblem::Malformed)
        .collect();
    let mut namespaces = BTreeSet::new();
    problems.extend(
        catalogs
            .iter()
            .filter(|catalog| !namespaces.insert(catalog.namespace))
            .map(|catalog| CatalogProblem::DuplicateNamespace(catalog.namespace)),
    );
    problems
}

pub fn registered_catalog_problems() -> Vec<CatalogProblem> {
    let mut problems = catalog_problems(REGISTERED_CATALOGS);
    for lang in [Language::En, Language::Uk] {
        problems.extend(
            duplicate_message_ids(catalog_language(lang), &app_catalogs(lang))
                .into_iter()
                .map(|id| CatalogProblem::DuplicateMessageId { language: lang, id }),
        );
    }
    problems
}

pub fn report_catalog_problems<T: 'static>(cx: &mut Context<T>) {
    cx.spawn(async move |_, cx| {
        let problems = cx
            .background_spawn(async { registered_catalog_problems() })
            .await;
        if problems.is_empty() {
            return;
        }
        let listed = problems
            .iter()
            .map(CatalogProblem::diagnostic)
            .collect::<Vec<_>>()
            .join("; ");
        tracing::error!(problems = %listed, "message catalogs conflict");
        cx.update(|cx| {
            cx.push_toast_full(
                ToastKind::Error,
                tr!("message_catalog_conflict", problems = listed),
                None,
                None,
                Duration::ZERO,
            );
        });
    })
    .detach();
}
