use std::borrow::Cow;

use gpui::{AssetSource, SharedString};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "assets"]
struct GroveAssets;

/// Composite asset source: checks Grove's embedded assets first,
/// then falls back to gpui-component's icon assets.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        if let Some(f) = GroveAssets::get(path) {
            return Ok(Some(f.data));
        }
        gpui_component_assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        let mut items: Vec<SharedString> = GroveAssets::iter()
            .filter(|p| p.starts_with(path))
            .map(SharedString::from)
            .collect();
        items.extend(gpui_component_assets::Assets.list(path)?);
        Ok(items)
    }
}
