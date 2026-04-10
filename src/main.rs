use gpui::{App, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};
use gpui_component::Root;
use tracing::info;
use tracing_subscriber::EnvFilter;

use torrix::app::{self, ToriixApp};
use torrix::assets::Assets;
use torrix::theme::apply_toriix_theme;

fn init_tracing() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn,torrix=debug"));

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
            gpui_component::init(cx);
            apply_toriix_theme(cx);
            app::register_keybindings(cx);

            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1000.), px(650.)),
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Toriix".into()),
                    ..Default::default()
                }),
                ..Default::default()
            };

            cx.open_window(options, |window, cx| {
                let view = cx.new(ToriixApp::new);
                cx.new(|cx| Root::new(view, window, cx))
            })
            .unwrap();

            cx.activate(true);
        });
}
