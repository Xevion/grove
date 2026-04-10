use std::borrow::Cow;

use gpui::{AssetSource, SharedString};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "assets"]
struct ToriixAssets;

#[cfg(not(target_family = "wasm"))]
const fn component_assets() -> gpui_component_assets::Assets {
    gpui_component_assets::Assets
}

#[cfg(target_family = "wasm")]
fn component_assets() -> gpui_component_assets::Assets {
    gpui_component_assets::Assets::default()
}

/// Composite asset source: checks Toriix's embedded assets first,
/// then falls back to gpui-component's icon assets.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        if let Some(f) = ToriixAssets::get(path) {
            return Ok(Some(f.data));
        }
        component_assets().load(path)
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        let mut items: Vec<SharedString> = ToriixAssets::iter()
            .filter(|p| p.starts_with(path))
            .map(SharedString::from)
            .collect();
        items.extend(component_assets().list(path)?);
        Ok(items)
    }
}
