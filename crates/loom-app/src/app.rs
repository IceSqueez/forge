use iced::{Element, Subscription, Task, Theme, widget::text};
use loom_widgets::{LoomPalette, ThemeId};

use crate::{Message, Screen};

pub struct App {
    pub screen: Screen,
    pub theme: Theme,
    pub palette: LoomPalette,
}

impl Default for App {
    fn default() -> Self {
        let (theme, palette) = loom_widgets::catppuccin_mocha();
        Self {
            screen: Screen::Hub,
            theme,
            palette,
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
        Message::Noop => Task::none(),
    }
}

pub fn view(app: &App) -> Element<'_, Message> {
    match &app.screen {
        Screen::Hub => text("streamer-loom hub").into(),
        other => text(format!("placeholder for {other:?}")).into(),
    }
}

pub fn subscription(_app: &App) -> Subscription<Message> {
    Subscription::none()
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
        assert_eq!(app.screen, Screen::Hub);
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
