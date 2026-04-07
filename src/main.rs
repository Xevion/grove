use gpui::*;

struct GroveApp;

impl Render for GroveApp {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .bg(rgb(0x1e1e2e))
            .size_full()
            .justify_center()
            .items_center()
            .gap_4()
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(0xcdd6f4))
                    .child("Grove"),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x6c7086))
                    .child("File Manager"),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(900.), px(600.)),
                cx,
            ))),
            titlebar: Some(TitlebarOptions {
                title: Some("Grove".into()),
                ..Default::default()
            }),
            ..Default::default()
        };

        cx.open_window(options, |_window, cx| cx.new(|_| GroveApp))
            .unwrap();

        cx.activate(true);
    });
}
