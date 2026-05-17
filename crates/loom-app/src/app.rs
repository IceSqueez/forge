use std::sync::Arc;

use iced::{Element, Length, Subscription, Task, Theme};
use loom_events::EventBus;
use loom_runtime::InMemoryEventBus;
use loom_storage_sqlite::SqliteBackend;
use loom_widgets::{LoomPalette, ThemeId};

use crate::screen::OnboardingStep;
use crate::{Message, Screen, SettingsSection};

pub struct App {
    pub screen: Screen,
    pub theme: Theme,
    pub palette: LoomPalette,
    pub backend: Arc<SqliteBackend>,
    pub bus: Arc<InMemoryEventBus>,
    pub storage_offline: bool,
}

impl App {
    pub fn default_with(
        initial: Screen,
        backend: Arc<SqliteBackend>,
        storage_offline: bool,
    ) -> Self {
        let (theme, palette) = loom_widgets::catppuccin_mocha();
        Self {
            screen: initial,
            theme,
            palette,
            backend,
            bus: Arc::new(InMemoryEventBus::new()),
            storage_offline,
        }
    }
}

#[cfg(test)]
impl Default for App {
    #[allow(clippy::expect_used)]
    fn default() -> Self {
        keyring::use_sample_store(&std::collections::HashMap::new())
            .expect("sample keyring store must initialize");
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime for test");
        let backend = Arc::new(
            rt.block_on(SqliteBackend::open("sqlite::memory:"))
                .expect("in-memory SQLite always opens"),
        );
        let (theme, palette) = loom_widgets::catppuccin_mocha();
        Self {
            screen: Screen::Onboarding(OnboardingStep::Welcome),
            theme,
            palette,
            backend,
            bus: Arc::new(InMemoryEventBus::new()),
            storage_offline: false,
        }
    }
}

pub fn update(app: &mut App, msg: Message) -> Task<Message> {
    match msg {
        Message::Navigate(screen) => {
            app.screen = screen;
            Task::none()
        }
        Message::ThemeChanged(id) => {
            let (theme, palette) = match id {
                ThemeId::CatppuccinMocha => loom_widgets::catppuccin_mocha(),
                ThemeId::TokyoNight => loom_widgets::tokyo_night_storm(),
                ThemeId::Latte => loom_widgets::latte(),
            };
            app.theme = theme;
            app.palette = palette;
            Task::none()
        }
        Message::BusEvent(_) => Task::none(),
        Message::Noop => Task::none(),
    }
}

fn nav_button<'a>(label: &'a str, screen: Screen, palette: &LoomPalette) -> Element<'a, Message> {
    loom_widgets::ghost_button(label, Message::Navigate(screen), palette)
}

fn hub_view(palette: &LoomPalette) -> Element<'static, Message> {
    let hero = loom_widgets::hero_card(
        "Welcome to streamer-loom",
        "0.1.0-alpha.1",
        std::iter::empty::<Element<'static, Message>>(),
        palette,
    );

    let metrics = iced::widget::row![
        loom_widgets::metric_card("Twitch", "disconnected", None::<&str>, palette),
        loom_widgets::metric_card("OBS", "disconnected", None::<&str>, palette),
        loom_widgets::metric_card("Speak Queue", "empty", None::<&str>, palette),
    ]
    .spacing(12);

    let content = loom_widgets::card([hero, metrics.into()], palette);

    iced::widget::container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(16)
        .into()
}

fn settings_section_button<'a>(
    label: &'a str,
    section: SettingsSection,
    active: &SettingsSection,
    palette: &LoomPalette,
) -> Element<'a, Message> {
    if &section == active {
        loom_widgets::primary_button(label, Message::Navigate(Screen::Settings(section)), palette)
    } else {
        loom_widgets::ghost_button(label, Message::Navigate(Screen::Settings(section)), palette)
    }
}

fn settings_diagnostics_pane(palette: &LoomPalette) -> Element<'static, Message> {
    let version = env!("CARGO_PKG_VERSION");
    let metrics = iced::widget::row![
        loom_widgets::metric_card("Build", version, None::<&str>, palette),
        loom_widgets::metric_card("Rust", "1.95.0", None::<&str>, palette),
        loom_widgets::metric_card("OS", std::env::consts::OS, None::<&str>, palette),
    ]
    .spacing(12);

    iced::widget::container(metrics)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(16)
        .into()
}

fn settings_view<'a>(
    section: &'a SettingsSection,
    palette: &'a LoomPalette,
) -> Element<'a, Message> {
    let nav = iced::widget::column![
        settings_section_button("Appearance", SettingsSection::Appearance, section, palette),
        settings_section_button("Language", SettingsSection::Language, section, palette),
        settings_section_button("Shortcuts", SettingsSection::Shortcuts, section, palette),
        settings_section_button(
            "Notifications",
            SettingsSection::Notifications,
            section,
            palette
        ),
        settings_section_button("Scripting", SettingsSection::Scripting, section, palette),
        settings_section_button("Queues", SettingsSection::Queues, section, palette),
        settings_section_button("Storage", SettingsSection::Storage, section, palette),
        settings_section_button("WebSocket", SettingsSection::WebSocket, section, palette),
        settings_section_button("Version", SettingsSection::Version, section, palette),
        settings_section_button(
            "Diagnostics",
            SettingsSection::Diagnostics,
            section,
            palette
        ),
    ]
    .spacing(4)
    .width(Length::Fixed(160.0));

    let pane: Element<'a, Message> = match section {
        SettingsSection::Diagnostics => settings_diagnostics_pane(palette),
        other => {
            let label = format!("Settings · {other:?}");
            iced::widget::container(loom_widgets::empty_state(
                label,
                "Placeholder for alpha-1.",
                None::<(&str, Message)>,
                palette,
            ))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        }
    };

    iced::widget::row![nav, pane].spacing(16).into()
}

fn onboarding_view<'a>(step: &'a OnboardingStep, palette: &'a LoomPalette) -> Element<'a, Message> {
    let step_label = format!("Step: {step:?}");
    let hero = loom_widgets::hero_card(
        "Welcome to streamer-loom",
        "First-run setup",
        std::iter::once(iced::widget::text(step_label).into()),
        palette,
    );

    let buttons = iced::widget::row![
        loom_widgets::ghost_button("Skip", Message::Navigate(Screen::Hub), palette),
        loom_widgets::ghost_button("Next", Message::Navigate(Screen::Hub), palette),
    ]
    .spacing(8);

    let content = loom_widgets::card([hero, buttons.into()], palette);

    iced::widget::container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(16)
        .into()
}

fn coming_soon_view(screen_label: String, palette: &LoomPalette) -> Element<'static, Message> {
    iced::widget::container(loom_widgets::empty_state(
        "Coming soon",
        screen_label,
        None::<(&str, Message)>,
        palette,
    ))
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let palette = &app.palette;

    let nav_items = vec![
        nav_button("Hub", Screen::Hub, palette),
        nav_button("Live Chat", Screen::LiveChat, palette),
        nav_button("Events", Screen::EventFeed, palette),
        nav_button("Globals", Screen::Globals, palette),
        nav_button("Actions", Screen::Actions, palette),
        nav_button("Commands", Screen::Commands, palette),
        nav_button("Platforms", Screen::Platforms, palette),
        nav_button("Integrations", Screen::Integrations, palette),
        nav_button(
            "Settings",
            Screen::Settings(SettingsSection::Appearance),
            palette,
        ),
    ];

    let sidebar = loom_widgets::sidebar(
        vec![loom_widgets::sidebar_section("Main", nav_items, palette)],
        palette,
    );

    let content: Element<'_, Message> = match &app.screen {
        Screen::Hub => hub_view(palette),
        Screen::Settings(section) => settings_view(section, palette),
        Screen::Onboarding(step) => onboarding_view(step, palette),
        other => coming_soon_view(format!("{other:?}"), palette),
    };

    iced::widget::row![sidebar, content].into()
}

pub fn subscription(app: &App) -> Subscription<Message> {
    use iced::advanced::subscription::{EventStream, Hasher, Recipe, from_recipe};
    use iced::futures::StreamExt as _;

    struct BusRecipe(Arc<InMemoryEventBus>);

    impl Recipe for BusRecipe {
        type Output = Message;

        fn hash(&self, state: &mut Hasher) {
            use std::hash::Hash as _;
            (Arc::as_ptr(&self.0) as usize).hash(state);
        }

        fn stream(
            self: Box<Self>,
            _input: EventStream,
        ) -> iced::futures::stream::BoxStream<'static, Self::Output> {
            let bus = self.0;
            iced::stream::channel(
                64,
                |mut tx: iced::futures::channel::mpsc::Sender<Message>| async move {
                    let mut stream = bus.subscribe();
                    loop {
                        if let Ok(event) = stream.recv().await {
                            let _ = tx.try_send(Message::BusEvent(event));
                        }
                    }
                },
            )
            .boxed()
        }
    }

    from_recipe(BusRecipe(app.bus.clone()))
}

pub fn theme_callback(app: &App) -> Theme {
    app.theme.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_widgets::ThemeId;

    #[test]
    fn navigate_updates_screen() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Actions));
        assert_eq!(app.screen, Screen::Actions);
    }

    #[test]
    fn navigate_to_hub_sets_hub_screen() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Logs));
        let _ = update(&mut app, Message::Navigate(Screen::Hub));
        assert_eq!(app.screen, Screen::Hub);
    }

    #[test]
    fn navigate_to_settings_diagnostics() {
        let mut app = App::default();
        let _ = update(
            &mut app,
            Message::Navigate(Screen::Settings(SettingsSection::Diagnostics)),
        );
        assert_eq!(app.screen, Screen::Settings(SettingsSection::Diagnostics));
    }

    #[test]
    fn navigate_to_onboarding_welcome() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Hub));
        let _ = update(
            &mut app,
            Message::Navigate(Screen::Onboarding(OnboardingStep::Welcome)),
        );
        assert_eq!(app.screen, Screen::Onboarding(OnboardingStep::Welcome));
    }

    #[test]
    fn theme_changed_tokyo_night() {
        let mut app = App::default();
        let _ = update(&mut app, Message::ThemeChanged(ThemeId::TokyoNight));
        let _ = theme_callback(&app);
    }

    #[test]
    fn theme_changed_latte() {
        let mut app = App::default();
        let _ = update(&mut app, Message::ThemeChanged(ThemeId::Latte));
        let _ = theme_callback(&app);
    }

    #[test]
    fn noop_does_not_change_screen() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Noop);
        assert_eq!(app.screen, Screen::Onboarding(OnboardingStep::Welcome));
    }

    #[test]
    fn subscription_compiles() {
        let app = App::default();
        let _ = subscription(&app);
    }

    #[test]
    fn view_compiles() {
        let app = App::default();
        let _ = view(&app);
    }
}
