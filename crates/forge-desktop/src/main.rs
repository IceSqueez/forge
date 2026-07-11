use forge_components::hello_label;
use gpui::{
    App, Application, Bounds, Context, SharedString, TitlebarOptions, Window, WindowBounds,
    WindowOptions, div, prelude::*, px, rgb, size,
};

struct HelloForge {
    text: SharedString,
}

impl Render for HelloForge {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .justify_center()
            .items_center()
            .gap_2()
            .bg(rgb(0x1e1e2e))
            .text_color(rgb(0xcdd6f4))
            .text_xl()
            .child(self.text.clone())
            .child(hello_label())
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(720.0), px(480.0)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some(SharedString::from("hello forge")),
                ..Default::default()
            }),
            app_id: Some("forge-desktop".to_owned()),
            ..Default::default()
        };

        match cx.open_window(options, |_, cx| {
            cx.new(|_| HelloForge {
                text: SharedString::from("hello forge"),
            })
        }) {
            Ok(_) => cx.activate(true),
            Err(err) => eprintln!("forge-desktop: failed to open window: {err}"),
        }
    });
}
