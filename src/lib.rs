pub mod app;
pub mod assets;
pub mod fs;
pub mod icons;
pub mod model;
pub mod theme;
pub mod ui;

#[cfg(target_family = "wasm")]
use gpui::{App, AppContext, WindowOptions};
#[cfg(target_family = "wasm")]
use wasm_bindgen::prelude::*;

#[cfg(target_family = "wasm")]
use app::GroveApp;
#[cfg(target_family = "wasm")]
use assets::Assets;

/// Show the HTML error overlay when WebGPU/window init fails.
#[cfg(target_family = "wasm")]
fn show_wasm_error(msg: &str) {
    let Some(window) = web_sys::window() else { return };
    let Some(document) = window.document() else { return };

    if let Some(loading) = document.get_element_by_id("loading") {
        let _ = loading.dyn_ref::<web_sys::HtmlElement>()
            .map(|el| el.style().set_property("display", "none"));
    }
    if let Some(error_div) = document.get_element_by_id("error") {
        let _ = error_div.dyn_ref::<web_sys::HtmlElement>()
            .map(|el| el.style().set_property("display", "flex"));
    }
    if let Some(error_msg) = document.get_element_by_id("error-message") {
        error_msg.set_text_content(Some(msg));
    }
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub fn run() -> Result<(), JsValue> {
    gpui_platform::web_init();

    // Application::run() consumes self and clones the inner Rc into the
    // platform callback. In WASM, run() returns immediately (async event
    // loop), and the callback's Rc clone keeps the AppCell alive. No leak
    // hack needed — the consumed Application drops its Rc (refcount 2→1)
    // while the closure retains the other.
    gpui_platform::single_threaded_web()
        .with_assets(Assets)
        .run(|cx: &mut App| {
        app::register_keybindings(cx);

        match cx.open_window(WindowOptions::default(), |_window, cx| {
            cx.new(GroveApp::new)
        }) {
            Ok(_) => cx.activate(true),
            Err(e) => {
                show_wasm_error(&format!("{e:#}"));
                cx.quit();
            }
        }
    });

    Ok(())
}
