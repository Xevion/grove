use gpui::{px, size, App, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowOptions};
use tracing::info;
use tracing_subscriber::EnvFilter;

use grove::app::{self, GroveApp};
use grove::assets::Assets;

fn init_tracing() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn,grove=debug"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();

    info!("tracing initialized");
}

fn main() {
    init_tracing();

    gpui_platform::application()
        .with_assets(Assets)
        .run(|cx: &mut App| {
            app::register_keybindings(cx);

            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1000.), px(650.)),
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Grove".into()),
                    ..Default::default()
                }),
                ..Default::default()
            };

            cx.open_window(options, |_window, cx| cx.new(GroveApp::new))
                .unwrap();

            cx.activate(true);
        });
}
