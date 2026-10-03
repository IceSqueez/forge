use std::sync::Arc;
use std::time::Duration;

use forge_components::{InputEvent, TextInput, tr};
use forge_monobank::MonobankJar;
use forge_storage::{CredentialsRepo, SettingsRepo, has_credentials_for};
use gpui::{AppContext as _, Context, Entity, Subscription, Window, px};

use crate::async_bridge::{self, ErrorSink};
use crate::donation_services::{
    DONATELLO_POLL_INTERVAL_KEY, DonationService, stored_donatello_poll_interval,
};
use crate::presentation::ActivePresentation;

mod problem;
mod render;

pub use problem::TokenProblem;
pub(crate) use render::remove_confirm;

const TOKEN_FONT_SIZE: gpui::Pixels = px(12.0);
const DONATELLO_TOKEN_PAGE: &str = "https://donatello.to/panel/doc-api";
const MONOBANK_TOKEN_PAGE: &str = "https://api.monobank.ua/";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Busy {
    Checking,
    Saving,
    LoadingJars,
    Removing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Success(String),
    Problem(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JarSource {
    Pasted,
    Stored,
}

pub struct DonationSettingsLaunch {
    pub service: DonationService,
    pub credentials: Arc<dyn CredentialsRepo>,
    pub settings: Arc<dyn SettingsRepo>,
    pub rt_handle: tokio::runtime::Handle,
}

pub struct DonationSettingsView {
    launch: DonationSettingsLaunch,
    token: Entity<TextInput>,
    token_revealed: bool,
    has_token: Option<bool>,
    busy: Option<Busy>,
    outcome: Option<Outcome>,
    jars: Vec<MonobankJar>,
    jar_source: Option<JarSource>,
    picked_jar: Option<String>,
    saved_jar: Option<String>,
    poll_interval: Option<Duration>,
    remove_pending: bool,
    _token_sub: Subscription,
}

impl DonationSettingsView {
    pub fn new(launch: DonationSettingsLaunch, cx: &mut Context<Self>) -> Self {
        let palette = cx.palette();
        let token = cx.new(|cx| {
            TextInput::new(tr!("donation_token_placeholder"), cx)
                .with_palette(palette)
                .plain()
                .mono()
                .with_font_size(TOKEN_FONT_SIZE)
                .secure(true)
        });
        let token_sub = cx.subscribe(&token, |this, _input, event: &InputEvent, cx| {
            if let InputEvent::Changed(_) = event {
                this.on_token_edited(cx);
            }
        });
        let mut view = Self {
            launch,
            token,
            token_revealed: false,
            has_token: None,
            busy: None,
            outcome: None,
            jars: Vec::new(),
            jar_source: None,
            picked_jar: None,
            saved_jar: None,
            poll_interval: None,
            remove_pending: false,
            _token_sub: token_sub,
        };
        view.load_stored(cx);
        view
    }

    pub fn service_name(&self) -> &'static str {
        match self.launch.service {
            DonationService::Donatello(_) => forge_donatello::DONATELLO_INTEGRATION.brand_name,
            DonationService::Monobank(_) => forge_monobank::MONOBANK_INTEGRATION.brand_name,
        }
    }

    pub fn is_monobank(&self) -> bool {
        matches!(self.launch.service, DonationService::Monobank(_))
    }

    pub fn focus_token(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.token.update(cx, |input, cx| input.focus(window, cx));
    }

    fn load_stored(&mut self, cx: &mut Context<Self>) {
        let credentials = Arc::clone(&self.launch.credentials);
        let service = self.launch.service.clone();
        let settings = Arc::clone(&self.launch.settings);
        async_bridge::run_async(
            &self.launch.rt_handle,
            async move {
                let present = has_credentials_for(credentials.as_ref(), service.id()).await;
                let jar = match &service {
                    DonationService::Monobank(provider) => {
                        provider.selected_jar_id().await.ok().flatten()
                    }
                    DonationService::Donatello(_) => None,
                };
                let interval = match &service {
                    DonationService::Donatello(_) => Some(
                        stored_donatello_poll_interval(settings.as_ref())
                            .await
                            .map_or(forge_donatello::DEFAULT_POLL_INTERVAL, |stored| {
                                stored.clamp(
                                    forge_donatello::MIN_POLL_INTERVAL,
                                    forge_donatello::MAX_POLL_INTERVAL,
                                )
                            }),
                    ),
                    DonationService::Monobank(_) => None,
                };
                (present, jar, interval)
            },
            |this, (present, jar, interval), cx| {
                match present {
                    Ok(present) => this.has_token = Some(present),
                    Err(error) => {
                        tracing::warn!(error = %error, "could not tell whether a donation token is stored");
                    }
                }
                this.saved_jar = jar;
                this.poll_interval = interval;
                cx.notify();
            },
            cx,
        );
    }

    pub fn draft_token(&self, cx: &gpui::App) -> String {
        self.token.read(cx).content().trim().to_owned()
    }

    pub fn can_check(&self, cx: &gpui::App) -> bool {
        if self.busy.is_some() {
            return false;
        }
        let pasted = !self.draft_token(cx).is_empty();
        if self.is_monobank() {
            pasted || self.has_token == Some(true)
        } else {
            pasted
        }
    }

    pub fn can_save(&self, cx: &gpui::App) -> bool {
        if self.busy.is_some() {
            return false;
        }
        let pasted = !self.draft_token(cx).is_empty();
        if !self.is_monobank() {
            return pasted;
        }
        let Some(picked) = &self.picked_jar else {
            return false;
        };
        let changes_something =
            self.jar_source == Some(JarSource::Pasted) || self.saved_jar.as_ref() != Some(picked);
        match self.jar_source {
            Some(JarSource::Pasted) => pasted && changes_something,
            Some(JarSource::Stored) => self.has_token == Some(true) && changes_something,
            None => false,
        }
    }

    pub fn shown_jar(&self) -> Option<&String> {
        self.picked_jar.as_ref().or(self.saved_jar.as_ref())
    }

    fn on_token_edited(&mut self, cx: &mut Context<Self>) {
        self.outcome = None;
        if self.jar_source == Some(JarSource::Pasted) {
            self.jars.clear();
            self.jar_source = None;
            self.picked_jar = None;
        }
        cx.notify();
    }

    fn toggle_reveal(&mut self, cx: &mut Context<Self>) {
        self.token_revealed = !self.token_revealed;
        let secure = !self.token_revealed;
        self.token
            .update(cx, |input, cx| input.set_secure(secure, cx));
        cx.notify();
    }

    fn open_token_page(&mut self, cx: &mut Context<Self>) {
        let page = if self.is_monobank() {
            MONOBANK_TOKEN_PAGE
        } else {
            DONATELLO_TOKEN_PAGE
        };
        async_bridge::open_external(
            &self.launch.rt_handle,
            page,
            ErrorSink::Toast,
            tr!("integration_open_url_failed"),
            cx,
        );
    }

    fn begin(&mut self, busy: Busy, cx: &mut Context<Self>) {
        self.busy = Some(busy);
        self.outcome = None;
        cx.notify();
    }

    fn fail(&mut self, problem: &TokenProblem, cx: &mut Context<Self>) {
        self.busy = None;
        self.outcome = Some(Outcome::Problem(problem.message(self.service_name())));
        cx.notify();
    }

    fn check_token(&mut self, cx: &mut Context<Self>) {
        if !self.can_check(cx) {
            return;
        }
        match self.launch.service.clone() {
            DonationService::Donatello(provider) => {
                let raw = self.draft_token(cx);
                self.begin(Busy::Checking, cx);
                async_bridge::run_async(
                    &self.launch.rt_handle,
                    async move {
                        provider
                            .verify_token(&raw)
                            .await
                            .map_err(|error| TokenProblem::of_donatello(&error))
                    },
                    |this, result, cx| match result {
                        Ok(account) => {
                            this.busy = None;
                            this.outcome = Some(Outcome::Success(account_line(
                                "donation_token_works",
                                "donation_token_works_as",
                                account.nickname,
                            )));
                            cx.notify();
                        }
                        Err(problem) => this.fail(&problem, cx),
                    },
                    cx,
                );
            }
            DonationService::Monobank(provider) => {
                let raw = self.draft_token(cx);
                let source = if raw.is_empty() {
                    JarSource::Stored
                } else {
                    JarSource::Pasted
                };
                self.begin(Busy::LoadingJars, cx);
                async_bridge::run_async(
                    &self.launch.rt_handle,
                    async move {
                        let listing = match source {
                            JarSource::Pasted => provider.verify_token(&raw).await,
                            JarSource::Stored => provider.stored_jars().await,
                        };
                        listing.map_err(|error| TokenProblem::of_monobank(&error))
                    },
                    move |this, result, cx| match result {
                        Ok(jars) => this.apply_jars(source, jars, cx),
                        Err(problem) => this.fail(&problem, cx),
                    },
                    cx,
                );
            }
        }
    }

    pub fn apply_jars(
        &mut self,
        source: JarSource,
        jars: Vec<MonobankJar>,
        cx: &mut Context<Self>,
    ) {
        self.busy = None;
        self.outcome = jars
            .is_empty()
            .then(|| Outcome::Problem(tr!("donation_jar_none")));
        let keep = self
            .saved_jar
            .clone()
            .filter(|saved| jars.iter().any(|jar| &jar.id == saved));
        let only = match jars.as_slice() {
            [jar] => Some(jar.id.clone()),
            _ => None,
        };
        self.picked_jar = keep.or(only);
        self.jars = jars;
        self.jar_source = Some(source);
        cx.notify();
    }

    fn pick_jar(&mut self, jar_id: String, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        self.picked_jar = Some(jar_id);
        self.outcome = None;
        cx.notify();
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if !self.can_save(cx) {
            return;
        }
        let raw = self.draft_token(cx);
        match self.launch.service.clone() {
            DonationService::Donatello(provider) => {
                self.begin(Busy::Saving, cx);
                async_bridge::run_async(
                    &self.launch.rt_handle,
                    async move {
                        provider
                            .save_token(&raw)
                            .await
                            .map_err(|error| TokenProblem::of_donatello(&error))
                    },
                    |this, result, cx| match result {
                        Ok(account) => {
                            let line = account_line(
                                "donation_token_saved",
                                "donation_token_saved_as",
                                account.nickname,
                            );
                            this.token_saved(line, None, cx);
                        }
                        Err(problem) => this.fail(&problem, cx),
                    },
                    cx,
                );
            }
            DonationService::Monobank(provider) => {
                let Some(jar_id) = self.picked_jar.clone() else {
                    return;
                };
                let source = self.jar_source;
                self.begin(Busy::Saving, cx);
                async_bridge::run_async(
                    &self.launch.rt_handle,
                    async move {
                        let saved = match source {
                            Some(JarSource::Pasted) => provider.save_token(&raw, &jar_id).await,
                            _ => provider.select_jar(&jar_id).await,
                        };
                        saved.map_err(|error| TokenProblem::of_monobank(&error))
                    },
                    |this, result, cx| match result {
                        Ok(jar) => {
                            let line = tr!("donation_jar_saved", jar = jar_title(&jar));
                            this.token_saved(line, Some(jar.id), cx);
                        }
                        Err(problem) => this.fail(&problem, cx),
                    },
                    cx,
                );
            }
        }
    }

    fn token_saved(&mut self, line: String, jar: Option<String>, cx: &mut Context<Self>) {
        self.busy = None;
        self.has_token = Some(true);
        self.outcome = Some(Outcome::Success(line));
        if jar.is_some() {
            self.saved_jar = jar;
            self.picked_jar = None;
            if self.jar_source == Some(JarSource::Pasted) {
                self.jar_source = Some(JarSource::Stored);
            }
        }
        self.token.update(cx, |input, cx| input.clear(cx));
        cx.notify();
    }

    fn request_remove(&mut self, cx: &mut Context<Self>) {
        if self.busy.is_some() || self.has_token != Some(true) {
            return;
        }
        self.remove_pending = true;
        cx.notify();
    }

    pub fn cancel_remove(&mut self, cx: &mut Context<Self>) {
        self.remove_pending = false;
        cx.notify();
    }

    pub fn confirm_remove(&mut self, cx: &mut Context<Self>) {
        if !self.remove_pending {
            return;
        }
        self.remove_pending = false;
        self.begin(Busy::Removing, cx);
        let service = self.launch.service.clone();
        async_bridge::run_async(
            &self.launch.rt_handle,
            async move {
                match service {
                    DonationService::Donatello(provider) => provider
                        .remove_token()
                        .await
                        .map_err(|error| TokenProblem::of_donatello(&error)),
                    DonationService::Monobank(provider) => provider
                        .remove_token()
                        .await
                        .map_err(|error| TokenProblem::of_monobank(&error)),
                }
            },
            |this, result, cx| match result {
                Ok(_) => {
                    this.busy = None;
                    this.has_token = Some(false);
                    this.jars.clear();
                    this.jar_source = None;
                    this.picked_jar = None;
                    this.saved_jar = None;
                    this.outcome = Some(Outcome::Success(tr!("donation_token_removed")));
                    cx.notify();
                }
                Err(problem) => this.fail(&problem, cx),
            },
            cx,
        );
    }

    fn set_poll_interval(&mut self, requested: Duration, cx: &mut Context<Self>) {
        let DonationService::Donatello(provider) = &self.launch.service else {
            return;
        };
        let applied = provider.set_poll_interval(requested);
        self.poll_interval = Some(applied);
        let settings = Arc::clone(&self.launch.settings);
        let secs = applied.as_secs().to_string();
        self.launch.rt_handle.spawn(async move {
            if let Err(error) = settings
                .set_string(DONATELLO_POLL_INTERVAL_KEY, &secs)
                .await
            {
                tracing::warn!(error = %error, "could not keep the Donatello poll interval");
            }
        });
        cx.notify();
    }

    pub fn is_remove_pending(&self) -> bool {
        self.remove_pending
    }
}

pub fn jar_title(jar: &MonobankJar) -> String {
    jar.title
        .clone()
        .unwrap_or_else(|| tr!("donation_jar_untitled"))
}

fn account_line(plain_key: &str, named_key: &str, nickname: Option<String>) -> String {
    match nickname {
        Some(name) => tr!(named_key, name = name),
        None => tr!(plain_key),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use forge_donatello::{DONATELLO_CREDENTIAL_ID, DonatelloConfig, DonatelloProvider};
    use forge_monobank::{MonobankConfig, MonobankProvider, MonobankRateLimits};
    use forge_platform_core::PlatformEndpoints;
    use forge_storage::{CredentialId, StorageError};
    use gpui::TestAppContext;
    use time::OffsetDateTime;

    use super::*;
    use crate::test_support::{install_presentation, runtime, test_backend};

    const SETTLE_ROUNDS: usize = 200;
    const SAVED_JAR: &str = "jar-saved";
    const OTHER_JAR: &str = "jar-other";
    const PASTED: &str = "pasted-token";

    type Build = fn(&Arc<Vault>) -> DonationService;

    #[derive(Default)]
    struct Vault {
        bundles: Mutex<HashMap<String, String>>,
    }

    impl Vault {
        fn holding(id: &str, bundle: &str) -> Self {
            let vault = Self::default();
            vault
                .bundles
                .lock()
                .unwrap()
                .insert(id.to_owned(), bundle.to_owned());
            vault
        }

        fn is_empty(&self) -> bool {
            self.bundles.lock().unwrap().is_empty()
        }
    }

    #[async_trait::async_trait]
    impl CredentialsRepo for Vault {
        async fn store(&self, id: &CredentialId, bundle: &str) -> Result<(), StorageError> {
            self.bundles
                .lock()
                .unwrap()
                .insert(id.as_str().to_owned(), bundle.to_owned());
            Ok(())
        }

        async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
            Ok(self.bundles.lock().unwrap().get(id.as_str()).cloned())
        }

        async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
            Ok(self.bundles.lock().unwrap().remove(id.as_str()).is_some())
        }

        async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
            Ok(self
                .bundles
                .lock()
                .unwrap()
                .keys()
                .map(CredentialId::new)
                .collect())
        }

        async fn last_refresh(
            &self,
            _: &CredentialId,
        ) -> Result<Option<OffsetDateTime>, StorageError> {
            Ok(None)
        }

        async fn mark_refreshed(&self, _: &CredentialId) -> Result<(), StorageError> {
            Ok(())
        }
    }

    fn saved_monobank() -> Vault {
        Vault::holding(
            forge_monobank::MONOBANK_CREDENTIAL_ID,
            &format!(r#"{{"token":"stored-token","jar_id":"{SAVED_JAR}"}}"#),
        )
    }

    fn saved_donatello() -> Vault {
        Vault::holding(DONATELLO_CREDENTIAL_ID, "stored-token")
    }

    fn monobank(vault: &Arc<Vault>) -> DonationService {
        DonationService::Monobank(Arc::new(
            MonobankProvider::new(
                MonobankConfig::new(&PlatformEndpoints::default()),
                Arc::clone(vault) as Arc<dyn CredentialsRepo>,
                MonobankRateLimits::official(),
            )
            .expect("the provider builds offline"),
        ))
    }

    fn donatello(vault: &Arc<Vault>) -> DonationService {
        DonationService::Donatello(Arc::new(
            DonatelloProvider::new(
                DonatelloConfig::new(&PlatformEndpoints::default()),
                Arc::clone(vault) as Arc<dyn CredentialsRepo>,
                forge_donatello::default_rate_limiter(),
            )
            .expect("the provider builds offline"),
        ))
    }

    fn settle_until(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        view: &Entity<DonationSettingsView>,
        done: impl Fn(&DonationSettingsView) -> bool,
    ) -> bool {
        (0..SETTLE_ROUNDS).any(|_| {
            rt.block_on(async { tokio::time::sleep(Duration::from_millis(1)).await });
            cx.run_until_parked();
            view.read_with(cx, |view, _| done(view))
        })
    }

    fn open(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        build: Build,
        vault: &Arc<Vault>,
    ) -> Entity<DonationSettingsView> {
        install_presentation(cx);
        let (settings, _writes) = test_backend();
        let launch = DonationSettingsLaunch {
            service: build(vault),
            credentials: Arc::clone(vault) as Arc<dyn CredentialsRepo>,
            settings: settings as Arc<dyn SettingsRepo>,
            rt_handle: rt.handle().clone(),
        };
        let view = cx.new(|cx| DonationSettingsView::new(launch, cx));
        let loaded = settle_until(cx, rt, &view, |view| view.has_token.is_some());
        assert!(loaded, "the stored token state never loaded");
        view
    }

    fn type_token(cx: &mut TestAppContext, view: &Entity<DonationSettingsView>, text: &str) {
        let token = view.read_with(cx, |view, _| view.token.clone());
        token.update(cx, |input, cx| {
            input.set_content(text.to_owned(), cx);
            cx.emit(InputEvent::Changed(text.to_owned().into()));
        });
        cx.run_until_parked();
    }

    fn jar(id: &str) -> MonobankJar {
        MonobankJar {
            id: id.to_owned(),
            send_id: None,
            title: None,
            currency: None,
            goal: None,
        }
    }

    fn jars(ids: &[&str]) -> Vec<MonobankJar> {
        ids.iter().map(|id| jar(id)).collect()
    }

    #[gpui::test]
    fn the_check_button_needs_a_pasted_token_unless_monobank_already_holds_one(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let cases: [(Build, Vault, &str, bool, &str); 5] = [
            (
                monobank,
                saved_monobank(),
                "",
                true,
                "monobank lists the jars of the saved token",
            ),
            (
                monobank,
                Vault::default(),
                "",
                false,
                "monobank with nothing saved or pasted",
            ),
            (
                monobank,
                Vault::default(),
                PASTED,
                true,
                "monobank with a pasted token",
            ),
            (
                donatello,
                saved_donatello(),
                "",
                false,
                "donatello only checks a pasted token",
            ),
            (
                donatello,
                Vault::default(),
                "  ",
                false,
                "whitespace is not a token",
            ),
        ];

        for (build, vault, field, expected, case) in cases {
            let vault = Arc::new(vault);
            let view = open(cx, &rt, build, &vault);
            type_token(cx, &view, field);

            assert_eq!(
                view.read_with(cx, |view, cx| view.can_check(cx)),
                expected,
                "{case}"
            );
        }
    }

    #[gpui::test]
    fn nothing_can_be_checked_or_saved_while_a_request_runs(cx: &mut TestAppContext) {
        let rt = runtime();
        let view = open(cx, &rt, donatello, &Arc::new(Vault::default()));
        type_token(cx, &view, PASTED);

        view.update(cx, |view, _| view.busy = Some(Busy::Checking));

        assert_eq!(
            view.read_with(cx, |view, cx| (view.can_check(cx), view.can_save(cx))),
            (false, false)
        );
    }

    #[gpui::test]
    fn donatello_can_be_saved_only_with_a_pasted_token(cx: &mut TestAppContext) {
        let rt = runtime();
        let view = open(cx, &rt, donatello, &Arc::new(saved_donatello()));

        for (field, expected) in [("", false), ("   ", false), (PASTED, true)] {
            type_token(cx, &view, field);
            assert_eq!(
                view.read_with(cx, |view, cx| view.can_save(cx)),
                expected,
                "{field:?}"
            );
        }
    }

    #[gpui::test]
    fn monobank_can_be_saved_only_when_it_would_change_the_stored_choice(cx: &mut TestAppContext) {
        let rt = runtime();
        let cases = [
            (
                JarSource::Stored,
                "",
                Some(SAVED_JAR),
                false,
                "the saved jar again",
            ),
            (
                JarSource::Stored,
                "",
                Some(OTHER_JAR),
                true,
                "another jar of the saved token",
            ),
            (JarSource::Stored, "", None, false, "no jar picked"),
            (
                JarSource::Pasted,
                PASTED,
                Some(SAVED_JAR),
                true,
                "a new token for the same jar",
            ),
            (
                JarSource::Pasted,
                "",
                Some(OTHER_JAR),
                false,
                "pasted jars after the field was emptied",
            ),
        ];

        for (source, field, picked, expected, case) in cases {
            let view = open(cx, &rt, monobank, &Arc::new(saved_monobank()));
            type_token(cx, &view, field);
            view.update(cx, |view, cx| {
                view.apply_jars(source, jars(&[SAVED_JAR, OTHER_JAR]), cx);
                view.picked_jar = picked.map(str::to_owned);
            });

            assert_eq!(
                view.read_with(cx, |view, cx| view.can_save(cx)),
                expected,
                "{case}"
            );
        }
    }

    #[gpui::test]
    fn monobank_without_a_saved_token_cannot_save_a_stored_listing(cx: &mut TestAppContext) {
        let rt = runtime();
        let view = open(cx, &rt, monobank, &Arc::new(Vault::default()));

        view.update(cx, |view, cx| {
            view.apply_jars(JarSource::Stored, jars(&[OTHER_JAR]), cx);
        });

        assert!(!view.read_with(cx, |view, cx| view.can_save(cx)));
    }

    #[gpui::test]
    fn loading_jars_preselects_the_saved_jar_or_the_only_one(cx: &mut TestAppContext) {
        let rt = runtime();
        let cases = [
            (
                &[OTHER_JAR, SAVED_JAR][..],
                Some(SAVED_JAR),
                "the saved jar stays selected",
            ),
            (
                &[OTHER_JAR][..],
                Some(OTHER_JAR),
                "a single jar is selected for the user",
            ),
            (
                &[OTHER_JAR, "jar-third"][..],
                None,
                "a choice between unsaved jars is left open",
            ),
        ];

        for (listed, expected, case) in cases {
            let view = open(cx, &rt, monobank, &Arc::new(saved_monobank()));

            view.update(cx, |view, cx| {
                view.apply_jars(JarSource::Stored, jars(listed), cx)
            });

            view.read_with(cx, |view, _| {
                assert_eq!(view.picked_jar.as_deref(), expected, "{case}");
                assert_eq!(view.outcome, None, "{case}");
            });
        }
    }

    #[gpui::test]
    fn an_account_without_jars_is_reported_and_leaves_nothing_selected(cx: &mut TestAppContext) {
        let rt = runtime();
        let view = open(cx, &rt, monobank, &Arc::new(saved_monobank()));
        view.update(cx, |view, _| view.busy = Some(Busy::LoadingJars));

        view.update(cx, |view, cx| {
            view.apply_jars(JarSource::Stored, Vec::new(), cx)
        });

        view.read_with(cx, |view, _| {
            assert_eq!(view.picked_jar, None);
            assert_eq!(view.busy, None);
            assert_eq!(
                view.outcome,
                Some(Outcome::Problem(tr!("donation_jar_none")))
            );
        });
    }

    #[gpui::test]
    fn editing_the_token_drops_jars_loaded_from_the_pasted_one_only(cx: &mut TestAppContext) {
        let rt = runtime();
        for (source, kept) in [(JarSource::Pasted, false), (JarSource::Stored, true)] {
            let view = open(cx, &rt, monobank, &Arc::new(saved_monobank()));
            type_token(cx, &view, PASTED);
            view.update(cx, |view, cx| {
                view.apply_jars(source, jars(&[OTHER_JAR]), cx);
            });

            type_token(cx, &view, "pasted-token-edited");

            view.read_with(cx, |view, _| {
                assert_eq!(!view.jars.is_empty(), kept, "{source:?}");
                assert_eq!(view.picked_jar.is_some(), kept, "{source:?}");
            });
        }
    }

    #[gpui::test]
    fn confirming_a_removal_nobody_asked_for_removes_nothing(cx: &mut TestAppContext) {
        let rt = runtime();
        let vault = Arc::new(saved_donatello());
        let view = open(cx, &rt, donatello, &vault);

        view.update(cx, |view, cx| view.confirm_remove(cx));
        settle_until(cx, &rt, &view, |_| false);

        assert!(!vault.is_empty(), "the stored token was deleted");
        view.read_with(cx, |view, _| {
            assert_eq!(view.has_token, Some(true));
            assert_eq!(view.busy, None);
        });
    }

    #[gpui::test]
    fn a_requested_removal_once_confirmed_forgets_the_token_and_the_jar(cx: &mut TestAppContext) {
        let rt = runtime();
        let vault = Arc::new(saved_monobank());
        let view = open(cx, &rt, monobank, &vault);

        view.update(cx, |view, cx| {
            view.request_remove(cx);
            view.confirm_remove(cx);
        });
        let removed = settle_until(cx, &rt, &view, |view| view.has_token == Some(false));

        assert!(removed, "the removal never finished");
        assert!(vault.is_empty());
        view.read_with(cx, |view, _| assert_eq!(view.shown_jar(), None));
    }
}
