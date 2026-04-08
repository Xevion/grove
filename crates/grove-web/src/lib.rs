use gpui::{
    div, px, rgb, App, AppContext, Context, InteractiveElement, IntoElement, ParentElement, Render,
    Styled, Window, WindowOptions,
};
use wasm_bindgen::prelude::*;

const BG_BASE: u32 = 0x1e1e2e;
const BG_SURFACE: u32 = 0x313244;
const TEXT_PRIMARY: u32 = 0xcdd6f4;
const TEXT_MUTED: u32 = 0xa6adc8;
const BORDER_COLOR: u32 = 0x45475a;

struct MockEntry {
    name: &'static str,
    is_dir: bool,
    size: &'static str,
}

const MOCK_ENTRIES: &[MockEntry] = &[
    MockEntry { name: "src", is_dir: true, size: "\u{2014}" },
    MockEntry { name: "assets", is_dir: true, size: "\u{2014}" },
    MockEntry { name: "target", is_dir: true, size: "\u{2014}" },
    MockEntry { name: ".gitignore", is_dir: false, size: "12 B" },
    MockEntry { name: "Cargo.lock", is_dir: false, size: "24.3 KB" },
    MockEntry { name: "Cargo.toml", is_dir: false, size: "412 B" },
    MockEntry { name: "CLAUDE.md", is_dir: false, size: "1.8 KB" },
    MockEntry { name: "README.md", is_dir: false, size: "2.1 KB" },
];

struct GroveWebDemo;

impl GroveWebDemo {
    fn new(_cx: &mut Context<Self>) -> Self {
        Self
    }

    fn render_header(&self) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .px(px(12.))
            .py(px(8.))
            .bg(rgb(BG_SURFACE))
            .border_b_1()
            .border_color(rgb(BORDER_COLOR))
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(TEXT_PRIMARY))
                    .child("Grove \u{2014} WASM Demo"),
            )
    }

    fn render_row(&self, entry: &MockEntry) -> impl IntoElement {
        let icon = if entry.is_dir { "\u{1F4C1}" } else { "\u{1F4C4}" };

        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(12.))
            .py(px(4.))
            .hover(|s| s.bg(rgb(BG_SURFACE)))
            .child(div().text_sm().child(icon))
            .child(
                div()
                    .flex_1()
                    .text_sm()
                    .text_color(rgb(TEXT_PRIMARY))
                    .child(entry.name),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(TEXT_MUTED))
                    .child(entry.size),
            )
    }
}

impl Render for GroveWebDemo {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let mut content = div().flex().flex_col().flex_1().min_h_0();

        for entry in MOCK_ENTRIES {
            content = content.child(self.render_row(entry));
        }

        div()
            .flex()
            .flex_col()
            .bg(rgb(BG_BASE))
            .text_color(rgb(TEXT_PRIMARY))
            .size_full()
            .child(self.render_header())
            .child(content)
    }
}

#[wasm_bindgen]
pub fn run() -> Result<(), JsValue> {
    #[cfg(target_family = "wasm")]
    gpui_platform::web_init();

    #[cfg(target_family = "wasm")]
    let app = {
        use gpui::Application;
        use std::rc::Rc;

        let app = gpui_platform::single_threaded_web();

        // Leak the inner Rc<AppCell> to keep the application alive after run() returns.
        // WASM's async event loop means run() returns before the app is done.
        struct WasmApp(Rc<gpui::AppCell>);
        let wasm_app = unsafe { std::mem::transmute::<Application, WasmApp>(app) };
        std::mem::forget(wasm_app.0.clone());
        unsafe { std::mem::transmute::<WasmApp, Application>(wasm_app) }
    };

    #[cfg(not(target_family = "wasm"))]
    let app = gpui_platform::application();

    app.run(|cx: &mut App| {
        let options = WindowOptions::default();

        cx.open_window(options, |_window, cx| cx.new(GroveWebDemo::new))
            .expect("failed to open window");
        cx.activate(true);
    });

    Ok(())
}
