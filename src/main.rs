mod app;
mod assets;
mod fs;
mod icons;
mod model;
mod theme;
pub(crate) mod ui;

use gpui::{
    px, size, App, AppContext, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions,
};
use tracing::info;
use tracing_subscriber::EnvFilter;

use app::GroveApp;
use assets::Assets;

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

    Application::new().with_assets(Assets).run(|cx: &mut App| {
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
