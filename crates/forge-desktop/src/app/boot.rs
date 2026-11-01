use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use forge_audio::AudioSinkFactory;
use forge_components::{AccentColor, Density, DisplayFormatChoices, ThemeId};
use forge_host::host::{HostInputs, build_host};
use forge_host::host_handles::{BootOutcomes, Host};
use forge_host::platform_identities::PlatformIdentities;
use forge_host::speak_boot::LocalTtsEngine;
use forge_platform_core::PlatformEndpoints;
use forge_runtime::DateNamesHandle;
use forge_storage::settings::core_settings::{ACCENT, FONT_BODY, FONT_MONO, LANGUAGE, THEME};
use forge_storage::{
    CredentialsKeyLoss, DataProvider, DatabaseFileSchema, Language, SettingsCatalog, SettingsRepo,
    StorageError, read_setting, read_setting_or_default, write_setting,
};
use forge_storage_sqlite::SqliteBackend;
use forge_types::Shared;

use crate::app::log_tail::LogTail;
use crate::app::runtime_handles::RuntimeHandles;
use crate::app::stay_awake::StayAwakeSetting;
use crate::app::window_placement::{WINDOW_PLACEMENT, WindowPlacement};
use crate::preferences::settings_catalog::DENSITY;
use crate::shared::repo_failure::RepoFailure;
use crate::tools::overlay_build_failure::OverlayBuildFailure;

pub(crate) enum BootFailure {
    UpgradeRequired { expected: u32, found: u32 },
    PreBaseline { found: u32 },
    Retry { failure: RepoFailure },
}

fn default_db_path() -> PathBuf {
    crate::app::app_paths::app_paths()
        .data_dir()
        .join(forge_storage::DATABASE_FILE_NAME)
}

const APPEARANCE_READ_BUDGET: Duration = Duration::from_secs(2);

#[derive(Default)]
pub(crate) struct PersistedAppearance {
    pub theme: ThemeId,
    pub accent: AccentColor,
    pub density: Density,
    pub body_font: Option<String>,
    pub mono_font: Option<String>,
}

#[derive(Default)]
pub(crate) struct BootPresentation {
    pub appearance: PersistedAppearance,
    pub window_placement: Option<WindowPlacement>,
    pub backend: Option<Arc<dyn DataProvider>>,
}

pub(crate) struct PresentationRead(tokio::task::JoinHandle<BootPresentation>);

impl PresentationRead {
    pub(crate) async fn arrived(self) -> BootPresentation {
        let presentation = self.0.await.unwrap_or_else(|error| {
            tracing::warn!(error = %error, "persisted presentation read stopped");
            BootPresentation::default()
        });
        tracing::info!(
            theme = ?presentation.appearance.theme,
            accent = ?presentation.appearance.accent,
            density = ?presentation.appearance.density,
            "applied persisted presentation"
        );
        presentation
    }
}

pub(crate) struct BootStorage {
    pub presentation: PresentationRead,
    pub opening: BackendOpening,
}

pub(crate) struct BackendOpening(
    tokio::task::JoinHandle<Result<Arc<dyn DataProvider>, BootFailure>>,
);

impl BackendOpening {
    pub(crate) async fn finished(self) -> Result<Arc<dyn DataProvider>, BootFailure> {
        match self.0.await {
            Ok(opened) => opened,
            Err(e) => Err(BootFailure::Retry {
                failure: RepoFailure::startup_stopped(e),
            }),
        }
    }
}

pub(crate) fn open_boot_storage(rt_handle: &tokio::runtime::Handle) -> BootStorage {
    open_boot_storage_with(rt_handle, open_backend())
}

fn open_boot_storage_with(
    rt_handle: &tokio::runtime::Handle,
    open: impl Future<Output = Result<Arc<dyn DataProvider>, BootFailure>> + Send + 'static,
) -> BootStorage {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let opening = BackendOpening(rt_handle.spawn(async move {
        let opened = open.await;
        let presentation = match &opened {
            Ok(backend) => BootPresentation {
                appearance: load_appearance(backend.as_ref()).await,
                window_placement: read_setting_or_default(backend.as_ref(), &WINDOW_PLACEMENT)
                    .await,
                backend: Some(Arc::clone(backend)),
            },
            Err(_) => BootPresentation::default(),
        };
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the window opens with defaults once the read budget runs out"
        )]
        let _ = tx.send(presentation);
        opened
    }));
    let presentation = PresentationRead(rt_handle.spawn(async move {
        tokio::time::timeout(APPEARANCE_READ_BUDGET, rx)
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default()
    }));
    BootStorage {
        presentation,
        opening,
    }
}

async fn load_appearance(settings: &dyn SettingsRepo) -> PersistedAppearance {
    let theme = stored_or_warn(read_setting(settings, &THEME).await, "theme")
        .and_then(|key| ThemeId::from_storage_key(&key))
        .unwrap_or_default();
    let accent = stored_or_warn(read_setting(settings, &ACCENT).await, "accent")
        .and_then(|key| AccentColor::from_storage_key(&key))
        .unwrap_or_default();
    let density = stored_or_warn(read_setting(settings, &DENSITY).await, "density")
        .and_then(|key| Density::from_storage_key(&key))
        .unwrap_or_default();
    PersistedAppearance {
        theme,
        accent,
        density,
        body_font: stored_or_warn(read_setting(settings, &FONT_BODY).await, "body font"),
        mono_font: stored_or_warn(read_setting(settings, &FONT_MONO).await, "mono font"),
    }
}

fn stored_or_warn<E: std::fmt::Display>(
    read: Result<Option<String>, E>,
    setting: &'static str,
) -> Option<String> {
    read.unwrap_or_else(|error| {
        tracing::warn!(%error, setting, "appearance setting unreadable; the default applies");
        None
    })
}

fn sqlite_url(db_path: &std::path::Path) -> String {
    let escaped = db_path
        .display()
        .to_string()
        .replace('%', "%25")
        .replace('?', "%3F");
    format!("sqlite://{escaped}?mode=rwc")
}

async fn open_backend() -> Result<Arc<dyn DataProvider>, BootFailure> {
    let db_path = default_db_path();
    if let Some(parent) = db_path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return Err(BootFailure::Retry {
            failure: RepoFailure::data_dir_unavailable(parent, &e),
        });
    }
    let url = sqlite_url(&db_path);
    match SqliteBackend::open(&url, crate::app::app_paths::app_paths()).await {
        Ok(backend) => Ok(Arc::new(backend) as Arc<dyn DataProvider>),
        Err(e) => {
            let err: StorageError = e.into();
            Err(match err {
                StorageError::SchemaMismatch { expected, found } => {
                    BootFailure::UpgradeRequired { expected, found }
                }
                StorageError::PreBaselineSchema { found } => BootFailure::PreBaseline { found },
                other => BootFailure::Retry {
                    failure: RepoFailure::from(other),
                },
            })
        }
    }
}

async fn apply_persisted_log_level(repo: &dyn SettingsRepo) {
    if crate::app::log_level::env_overridden() {
        return;
    }
    match forge_storage::diagnostic_log_level(repo).await {
        Ok(level) => {
            if crate::app::log_level::apply(&level) {
                tracing::info!(?level, "log level applied from settings");
            } else {
                tracing::warn!("log filter is not reloadable; keeping the boot level");
            }
        }
        Err(e) => tracing::warn!(error = %e, "could not read the persisted log level"),
    }
}

pub(crate) struct RuntimeInputs {
    pub log_tail: LogTail,
    pub endpoints: PlatformEndpoints,
    pub hotkey_main_thread: forge_hotkey::MainThreadLink,
    pub preopened: Option<Arc<dyn DataProvider>>,
    pub audio_sinks: Arc<dyn AudioSinkFactory>,
    pub local_tts_engines: Vec<LocalTtsEngine>,
    pub platform_identities: PlatformIdentities,
}

pub(crate) fn database_file_schema() -> Arc<dyn DatabaseFileSchema> {
    Arc::new(forge_storage_sqlite::SqliteDatabaseFileSchema)
}

pub(crate) async fn build_runtime(inputs: RuntimeInputs) -> Result<RuntimeHandles, BootFailure> {
    let RuntimeInputs {
        log_tail,
        endpoints,
        hotkey_main_thread,
        preopened,
        audio_sinks,
        local_tts_engines,
        platform_identities,
    } = inputs;
    let backend = match preopened {
        Some(backend) => backend,
        None => open_backend().await?,
    };
    let pre_boot = run_pre_boot(&backend).await;
    let host = build_host(HostInputs {
        provider: Arc::clone(&backend),
        data_dir: crate::app::app_paths::app_paths().data_dir(),
        endpoints,
        hotkey_main_thread,
        audio_sinks,
        local_tts_engines,
        platform_identities,
        anonymous_donor: pre_boot.anonymous_donor.clone(),
        date_names: pre_boot.date_names.clone(),
    })
    .await;
    Ok(present_host(host, backend, log_tail, pre_boot).await)
}

pub(crate) struct PreBoot {
    settings_catalog: Arc<SettingsCatalog>,
    credentials_key_loss: Option<CredentialsKeyLoss>,
    stay_awake: StayAwakeSetting,
    startup_language: Language,
    startup_display_formats: DisplayFormatChoices,
    date_names: DateNamesHandle,
    pub(crate) anonymous_donor: Shared<String>,
}

pub(crate) async fn run_pre_boot(backend: &Arc<dyn DataProvider>) -> PreBoot {
    let settings_repo: Arc<dyn SettingsRepo> = Arc::clone(backend) as Arc<dyn SettingsRepo>;
    let credentials_key_loss =
        match forge_storage::take_credentials_key_loss(settings_repo.as_ref()).await {
            Ok(loss) => loss,
            Err(e) => {
                tracing::warn!(error = %e, "could not read the credentials key loss record");
                None
            }
        };
    apply_persisted_log_level(settings_repo.as_ref()).await;
    let settings_catalog =
        Arc::new(crate::preferences::settings_catalog::registered_settings_catalog());

    let (startup_language, persist) =
        crate::app::i18n::resolve_startup_language(Arc::clone(&settings_repo)).await;
    if let Some(detected) = persist
        && let Err(e) =
            write_setting(settings_repo.as_ref(), &LANGUAGE, &detected.to_string()).await
    {
        tracing::warn!(error = %e, "failed to persist detected locale");
    }
    let stay_awake = crate::app::stay_awake::start_stay_awake(
        Arc::clone(&settings_repo),
        crate::app::i18n::message_in(startup_language, crate::app::stay_awake::HOLD_REASON_KEY),
    )
    .await;
    let startup_display_formats =
        crate::app::i18n::resolve_startup_display_formats(settings_repo.as_ref(), startup_language)
            .await;
    if let Err(e) = forge_host::os_adapters::SystemNotifyPort::adopt_app_identity() {
        tracing::warn!(error = %e, "desktop notifications keep the default app identity");
    }
    let date_names = DateNamesHandle::new(crate::app::i18n::date_names_language(startup_language));
    let anonymous_donor = Shared::new(crate::app::i18n::message_in(
        startup_language,
        crate::app::i18n::ANONYMOUS_DONOR_KEY,
    ));
    PreBoot {
        settings_catalog,
        credentials_key_loss,
        stay_awake,
        startup_language,
        startup_display_formats,
        date_names,
        anonymous_donor,
    }
}

pub(crate) async fn present_host(
    host: Host,
    backend: Arc<dyn DataProvider>,
    log_tail: LogTail,
    pre_boot: PreBoot,
) -> RuntimeHandles {
    let Host {
        core,
        automation,
        integrations,
        speech,
        audio,
        overlays,
        server,
        outcomes:
            BootOutcomes {
                warnings: boot_warnings,
                server_unavailable,
                overlay_pass,
                integration_states,
            },
        stop,
    } = host;
    let first_run = crate::app::first_run::resolve_first_run(
        backend.as_ref() as &dyn SettingsRepo,
        &integration_states,
    )
    .await;
    RuntimeHandles {
        rt_handle: tokio::runtime::Handle::current(),
        log_tail,
        backend,
        settings_catalog: pre_boot.settings_catalog,
        startup_language: pre_boot.startup_language,
        startup_display_formats: pre_boot.startup_display_formats,
        anonymous_donor: pre_boot.anonymous_donor,
        date_names: pre_boot.date_names,
        credentials_key_loss: pre_boot.credentials_key_loss,
        stay_awake: pre_boot.stay_awake,
        overlay_build_failure: OverlayBuildFailure::from_outcome(&overlay_pass),
        boot_warnings,
        server_unavailable: server_unavailable.map(RepoFailure::from),
        first_run,
        core,
        automation,
        integrations,
        speech,
        audio,
        overlays,
        server,
        stop,
    }
}

#[cfg(test)]
mod tests {
    use forge_components::{AccentColor, Density, ThemeId};
    use forge_storage::settings::core_settings::{ACCENT, FONT_BODY, FONT_MONO, THEME};
    use forge_storage::write_setting;

    use crate::preferences::settings_catalog::DENSITY;

    use super::{
        APPEARANCE_READ_BUDGET, BackendOpening, BootFailure, BootPresentation, PresentationRead,
        load_appearance, open_boot_storage_with, sqlite_url,
    };
    use crate::test_support::{test_backend, worker_runtime};
    use forge_storage::DataProvider;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn sqlite_url_escapes_only_the_characters_the_url_parser_would_consume() {
        for (path, expected) in [
            ("/data/forge.db", "sqlite:///data/forge.db?mode=rwc"),
            (
                "/data/100%/forge.db",
                "sqlite:///data/100%25/forge.db?mode=rwc",
            ),
            (
                "/data/why?/forge.db",
                "sqlite:///data/why%3F/forge.db?mode=rwc",
            ),
            (
                "/data/%3F/forge.db",
                "sqlite:///data/%253F/forge.db?mode=rwc",
            ),
            ("/дані/forge.db", "sqlite:///дані/forge.db?mode=rwc"),
        ] {
            assert_eq!(sqlite_url(std::path::Path::new(path)), expected);
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_database_under_a_directory_named_with_url_characters_opens_at_that_exact_path() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("100%25 why?");
        std::fs::create_dir(&directory).unwrap();
        let database = directory.join("forge.db");

        let opened = forge_storage_sqlite::SqliteBackend::open_for_test(
            &sqlite_url(&database),
            [0; 32],
            root.path().join("media"),
            None,
        )
        .await;

        assert!(opened.is_ok(), "{:?}", opened.err());
        assert!(database.is_file());
    }

    #[tokio::test]
    async fn the_stored_theme_accent_density_and_both_fonts_come_back_in_their_own_slots() {
        let (backend, _writes) = test_backend();
        write_setting(
            backend.as_ref(),
            &THEME,
            &Some(ThemeId::TokyoNight.storage_key().to_owned()),
        )
        .await
        .unwrap();
        write_setting(
            backend.as_ref(),
            &ACCENT,
            &Some(AccentColor::Green.storage_key().to_owned()),
        )
        .await
        .unwrap();
        write_setting(
            backend.as_ref(),
            &DENSITY,
            &Some(Density::Spacious.storage_key().to_owned()),
        )
        .await
        .unwrap();
        write_setting(backend.as_ref(), &FONT_BODY, &Some("Inter".to_owned()))
            .await
            .unwrap();
        write_setting(backend.as_ref(), &FONT_MONO, &Some("Iosevka".to_owned()))
            .await
            .unwrap();

        let appearance = load_appearance(backend.as_ref()).await;

        assert_eq!(
            (
                appearance.theme,
                appearance.accent,
                appearance.density,
                appearance.body_font.as_deref(),
                appearance.mono_font.as_deref()
            ),
            (
                ThemeId::TokyoNight,
                AccentColor::Green,
                Density::Spacious,
                Some("Inter"),
                Some("Iosevka")
            )
        );
    }

    #[tokio::test]
    async fn unrecognised_or_missing_appearance_settings_fall_back_to_the_defaults() {
        let (backend, _writes) = test_backend();
        backend.seed_raw(&THEME, "solarized");
        backend.seed_raw(&ACCENT, "teal");
        backend.seed_raw(&DENSITY, "roomy");

        let appearance = load_appearance(backend.as_ref()).await;

        assert_eq!(
            (
                appearance.theme,
                appearance.accent,
                appearance.density,
                appearance.body_font,
                appearance.mono_font
            ),
            (
                ThemeId::default(),
                AccentColor::Mauve,
                Density::default(),
                None,
                None
            )
        );
    }

    #[tokio::test]
    async fn a_store_without_an_accent_boots_with_mauve() {
        let (backend, _writes) = test_backend();

        let appearance = load_appearance(backend.as_ref()).await;

        assert_eq!(appearance.accent, AccentColor::Mauve);
    }

    fn opened_backend() -> Arc<dyn DataProvider> {
        test_backend().0
    }

    #[tokio::test]
    async fn a_slow_open_is_waited_for_and_hands_over_the_backend_it_opened() {
        let backend = opened_backend();
        let handed = Arc::clone(&backend);
        let (finish_open, open_gate) = tokio::sync::oneshot::channel::<()>();
        let opening = BackendOpening(tokio::spawn(async move {
            open_gate.await.ok();
            Ok(handed)
        }));

        let waiting = tokio::spawn(opening.finished());
        tokio::task::yield_now().await;
        let still_waiting = !waiting.is_finished();
        finish_open.send(()).unwrap();
        let finished = waiting.await.unwrap();

        assert!(still_waiting);
        assert!(finished.is_ok_and(|opened| Arc::ptr_eq(&opened, &backend)));
    }

    #[tokio::test]
    async fn an_upgrade_failure_passes_through_with_both_versions() {
        let opening = BackendOpening(tokio::spawn(async {
            Err(BootFailure::UpgradeRequired {
                expected: 9,
                found: 4,
            })
        }));

        let failure = opening.finished().await.err();

        assert!(matches!(
            failure,
            Some(BootFailure::UpgradeRequired {
                expected: 9,
                found: 4
            })
        ));
    }

    #[tokio::test]
    async fn a_pre_baseline_failure_passes_through_with_its_version() {
        let opening = BackendOpening(tokio::spawn(async {
            Err(BootFailure::PreBaseline { found: 3 })
        }));

        let failure = opening.finished().await.err();

        assert!(matches!(
            failure,
            Some(BootFailure::PreBaseline { found: 3 })
        ));
    }

    #[tokio::test]
    async fn a_retryable_failure_passes_through_with_its_reason() {
        let opening = BackendOpening(tokio::spawn(async {
            Err(BootFailure::Retry {
                failure: crate::shared::repo_failure::RepoFailure::startup_stopped("disk gone"),
            })
        }));

        let failure = opening.finished().await.err();

        assert!(matches!(
            failure,
            Some(BootFailure::Retry { failure }) if failure.to_string() == "disk gone"
        ));
    }

    #[tokio::test]
    async fn an_open_that_died_reads_as_a_retryable_failure() {
        let dying = tokio::spawn(std::future::pending());
        dying.abort();

        let failure = BackendOpening(dying).finished().await.err();

        assert!(matches!(failure, Some(BootFailure::Retry { .. })));
    }

    #[test]
    fn boot_storage_runs_the_open_once_and_hands_that_backend_to_the_waiter() {
        let rt = worker_runtime();
        let backend = opened_backend();
        let handed = Arc::clone(&backend);
        let opens = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&opens);

        let storage = open_boot_storage_with(rt.handle(), async move {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(handed)
        });
        let finished = rt.block_on(storage.opening.finished());

        assert_eq!(
            (
                opens.load(Ordering::SeqCst),
                finished.is_ok_and(|opened| Arc::ptr_eq(&opened, &backend))
            ),
            (1, true)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn the_window_opens_with_defaults_once_the_read_budget_runs_out() {
        let started = tokio::time::Instant::now();
        let storage = open_boot_storage_with(
            &tokio::runtime::Handle::current(),
            std::future::pending::<Result<Arc<dyn DataProvider>, BootFailure>>(),
        );

        let presentation = storage.presentation.arrived().await;

        assert_eq!(
            (
                presentation.backend.is_some(),
                presentation.window_placement.is_some(),
                presentation.appearance.theme,
                started.elapsed(),
            ),
            (false, false, ThemeId::default(), APPEARANCE_READ_BUDGET)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn an_open_slower_than_the_read_budget_still_hands_over_its_backend() {
        let backend = opened_backend();
        let handed = Arc::clone(&backend);
        let storage = open_boot_storage_with(&tokio::runtime::Handle::current(), async move {
            tokio::time::sleep(APPEARANCE_READ_BUDGET * 2).await;
            Ok(handed)
        });

        let early_backend = storage.presentation.arrived().await.backend.is_some();
        let finished = storage.opening.finished().await;

        assert_eq!(
            (
                early_backend,
                finished.is_ok_and(|opened| Arc::ptr_eq(&opened, &backend))
            ),
            (false, true)
        );
    }

    #[tokio::test]
    async fn a_presentation_read_that_died_arrives_as_the_defaults() {
        let dying = tokio::spawn(std::future::pending::<BootPresentation>());
        dying.abort();

        let presentation = PresentationRead(dying).arrived().await;

        assert!(presentation.backend.is_none());
    }

    #[test]
    fn a_fresh_store_boots_into_handles_that_open_the_welcome_screen_with_nothing_to_report() {
        if !forge_host::test_support::runs_here_in_isolation(
            "app::boot::tests::a_fresh_store_boots_into_handles_that_open_the_welcome_screen_with_nothing_to_report",
        ) {
            return;
        }
        forge_host::test_support::on_app_runtime(async {
            let handles = crate::test_support::booted_runtime_handles().await;

            assert_eq!(
                (
                    handles.first_run,
                    handles.server_unavailable.is_none(),
                    handles.overlay_build_failure.is_none(),
                ),
                (crate::app::first_run::FirstRun::Welcome, true, true)
            );
        });
    }

    #[test]
    fn boot_storage_passes_a_failed_open_through_without_a_backend() {
        let rt = worker_runtime();

        let storage = open_boot_storage_with(rt.handle(), async {
            Err(BootFailure::PreBaseline { found: 3 })
        });
        let early_backend = rt
            .block_on(storage.presentation.arrived())
            .backend
            .is_some();
        let failure = rt.block_on(storage.opening.finished()).err();

        assert!(!early_backend);
        assert!(matches!(
            failure,
            Some(BootFailure::PreBaseline { found: 3 })
        ));
    }
}
