use loom_app::App;
use loom_app::app::{subscription, theme_callback, update, view};

fn main() -> iced::Result {
    tracing_subscriber::fmt().with_env_filter("info").init();
    tracing::info!("streamer-loom starting");
    iced::application("streamer-loom", update, view)
        .subscription(subscription)
        .theme(theme_callback)
        .run_with(|| (App::default(), iced::Task::none()))
}
