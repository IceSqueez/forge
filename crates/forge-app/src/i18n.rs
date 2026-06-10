use std::rc::Rc;
use std::sync::Arc;

use fluent::{FluentBundle, FluentResource};
use fluent_langneg::{NegotiationStrategy, negotiate_languages};
use forge_storage::{Language, SettingsRepo};
use unic_langid::LanguageIdentifier;

const EN_MAIN_FTL: &str = include_str!("../locales/en/main.ftl");
const UK_MAIN_FTL: &str = include_str!("../locales/uk/main.ftl");

fn build_bundle(lang: Language) -> Rc<FluentBundle<FluentResource>> {
    let tag = match lang {
        Language::En => "en-US",
        Language::Uk => "uk-UA",
    };
    // invariant: hard-coded BCP-47 tags always parse without error
    let lang_id: LanguageIdentifier = tag.parse().unwrap_or_else(|_| {
        tracing::warn!("BCP-47 parse failed for {tag}; falling back to base tag");
        let base = &tag[..2];
        base.parse().unwrap_or_default()
    });
    let ftl_source = match lang {
        Language::En => EN_MAIN_FTL,
        Language::Uk => UK_MAIN_FTL,
    };

    let mut bundle = FluentBundle::new(vec![lang_id]);
    let resource = match FluentResource::try_new(ftl_source.to_owned()) {
        Ok(r) => r,
        Err((r, errors)) => {
            tracing::warn!(?errors, "FTL parse errors in {:?} bundle", lang);
            r
        }
    };
    if let Err(errors) = bundle.add_resource(resource) {
        tracing::warn!(?errors, "FTL resource add errors for {:?}", lang);
    }
    Rc::new(bundle)
}

/// Detects the OS locale on first run and maps it to a supported `Language`.
///
/// Returns `None` if the OS locale does not negotiate to Ukrainian — in that case the caller
/// should persist and use `Language::En`.
fn negotiate_os_locale() -> Option<Language> {
    let os_locale = sys_locale::get_locale()?;
    let requested: Vec<LanguageIdentifier> = os_locale
        .parse::<LanguageIdentifier>()
        .map(|l| vec![l])
        .unwrap_or_default();
    if requested.is_empty() {
        return None;
    }
    // invariant: "en" and "uk" are valid BCP-47 base tags
    let en: LanguageIdentifier = "en".parse().unwrap_or_default();
    let uk: LanguageIdentifier = "uk".parse().unwrap_or_default();
    let available = vec![en.clone(), uk];
    let negotiated = negotiate_languages(
        &requested,
        &available,
        Some(&en),
        NegotiationStrategy::Filtering,
    );
    match negotiated.first().map(|l| l.to_string()).as_deref() {
        Some("uk") => Some(Language::Uk),
        _ => None,
    }
}

/// Returns `(language_to_use, language_to_persist)`. The second element is `Some` only on first
/// run (no persisted setting yet) — caller must follow up with `SettingsRepo::set_language` to
/// pin the negotiated OS locale.
pub async fn resolve_startup_language(
    settings: Arc<dyn SettingsRepo>,
) -> (Language, Option<Language>) {
    match settings
        .get_string(forge_storage::reserved_keys::LANGUAGE)
        .await
    {
        Ok(Some(s)) => {
            let lang = s.parse::<Language>().unwrap_or(Language::En);
            (lang, None)
        }
        Ok(None) => {
            let detected = negotiate_os_locale().unwrap_or(Language::En);
            (detected, Some(detected))
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to read language setting; defaulting to en");
            (Language::En, None)
        }
    }
}

/// Builds the `FluentBundle` for `lang` and installs it into the `forge-widgets` thread-local.
///
/// Must be called on the main/render thread — the thread-local is per-thread and iced's view
/// loop runs on the main thread.
pub fn install_language(lang: Language) {
    let bundle = build_bundle(lang);
    forge_widgets::install_bundle(bundle);
    let locale_id: &'static str = match lang {
        Language::En => "en",
        Language::Uk => "uk",
    };
    forge_widgets::set_locale_id(locale_id);
}
