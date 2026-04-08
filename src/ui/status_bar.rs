use std::num::NonZeroUsize;

use gpui::{
    div, font, hsla, px, rgb, Context, IntoElement, ParentElement, Pixels, Styled, TextRun,
    Window,
};
use lru::LruCache;

use crate::app::GroveApp;
use crate::theme::{BG_SURFACE, BORDER_COLOR, TEXT_MUTED};

// Must match the font_family set on the status bar container
const STATUS_FONT: &str = "sans-serif";

// text_xs = rems(0.75); at default 16px/rem → 12px
const STATUS_FONT_PX: f32 = 12.0;

// Horizontal padding: px_3 = 12px each side = 24px total
const STATUS_PADDING_PX: f32 = 24.0;

const SEP: &str = "  ·  ";

const MEASURE_CACHE_CAP: usize = 256;

/// Cached text measurement. Keyed on (text, `font_size_bits`) to avoid float hashing.
pub struct TextMeasureCache {
    inner: LruCache<(String, u32), Pixels>,
}

impl TextMeasureCache {
    pub fn new() -> Self {
        Self {
            inner: LruCache::new(NonZeroUsize::new(MEASURE_CACHE_CAP).unwrap()),
        }
    }

    fn measure(&mut self, window: &Window, text: &str) -> Pixels {
        let key = (text.to_string(), STATUS_FONT_PX.to_bits());
        if let Some(&cached) = self.inner.get(&key) {
            return cached;
        }
        let run = TextRun {
            len: text.len(),
            font: font(STATUS_FONT),
            color: hsla(0.0, 0.0, 0.0, 1.0),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let width = window
            .text_system()
            .shape_line(text.to_string().into(), px(STATUS_FONT_PX), &[run], None)
            .width;
        self.inner.put(key, width);
        width
    }
}

/// Truncates `name` to fit within `max_px`, preserving the file extension.
///
/// Uses binary search over character count with exact pixel measurement
/// from the text shaper — no character-width guessing.
///
/// `budget.jpeg` at 60px → `bud….jpeg`
/// `noext` at 30px → `no…`
fn smart_truncate_px(
    cache: &mut TextMeasureCache,
    window: &Window,
    name: &str,
    max_px: Pixels,
) -> String {
    if cache.measure(window, name) <= max_px {
        return name.to_string();
    }

    let (stem, ext) = match name.rfind('.') {
        Some(dot) if dot > 0 => (&name[..dot], &name[dot..]),
        _ => (name, ""),
    };

    let suffix = format!("…{ext}");
    let suffix_px = cache.measure(window, &suffix);
    if suffix_px >= max_px {
        return "…".to_string();
    }

    let stem_chars: Vec<char> = stem.chars().collect();
    let mut lo = 1usize;
    let mut hi = stem_chars.len();
    let mut best = 0usize;

    while lo <= hi {
        let mid = usize::midpoint(lo, hi);
        let candidate: String = stem_chars[..mid]
            .iter()
            .copied()
            .chain(suffix.chars())
            .collect();
        if cache.measure(window, &candidate) <= max_px {
            best = mid;
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
    }

    if best == 0 {
        return "…".to_string();
    }

    let truncated_stem: String = stem_chars[..best].iter().collect();
    format!("{truncated_stem}…{ext}")
}

/// Cache key for the truncation result itself.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct TruncationKey {
    selected_index: usize,
    viewport_width_bits: u32,
    show_hidden: bool,
    entry_count: usize,
}

impl GroveApp {
    pub(crate) fn render_status_bar(
        &mut self,
        window: &Window,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let total = self.visible_entries.len();
        let hidden_count = self.entries.len().saturating_sub(total);

        let item_text = match total {
            0 => "Empty directory".to_string(),
            1 => "1 item".to_string(),
            n => format!("{n} items"),
        };

        let hidden_text = if !self.show_hidden && hidden_count > 0 {
            format!(" ({hidden_count} hidden)")
        } else {
            String::new()
        };

        let left_str = format!("{item_text}{hidden_text}");

        let right = if let Some(vi) = self.selected_index {
            if let Some(&ei) = self.visible_entries.get(vi) {
                let entry = &self.entries[ei];
                let viewport_width = window.viewport_size().width;

                let key = TruncationKey {
                    selected_index: vi,
                    viewport_width_bits: f32::from(viewport_width).to_bits(),
                    show_hidden: self.show_hidden,
                    entry_count: self.entries.len(),
                };

                let (display_name, size_str) = if self.truncation_cache.as_ref() == Some(&key) {
                    // Cache hit — reuse previous result
                    self.truncation_result.clone()
                } else {
                    let cache = &mut self.measure_cache;
                    let available_px = viewport_width - px(STATUS_PADDING_PX);
                    let left_px = cache.measure(window, &left_str);
                    let sep_px = cache.measure(window, SEP);

                    let size_str = if entry.is_dir {
                        String::new()
                    } else {
                        format!(" — {}", entry.size_display)
                    };
                    let size_px = cache.measure(window, &size_str);

                    let name_budget_px = (available_px - left_px - sep_px - size_px).max(px(0.0));
                    let display_name =
                        smart_truncate_px(cache, window, &entry.name, name_budget_px);

                    self.truncation_cache = Some(key);
                    self.truncation_result = (display_name.clone(), size_str.clone());
                    (display_name, size_str)
                };

                let mut group = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .flex_none()
                    .child(SEP)
                    .child(display_name);

                if !size_str.is_empty() {
                    group = group.child(size_str);
                }

                group
            } else {
                div()
            }
        } else {
            div()
        };

        div()
            .flex()
            .flex_row()
            .items_center()
            .px_3()
            .py_1()
            .bg(rgb(BG_SURFACE))
            .border_t_1()
            .border_color(rgb(BORDER_COLOR))
            .text_xs()
            .text_color(rgb(TEXT_MUTED))
            .child(div().flex_none().child(left_str))
            .child(div().flex_1())
            .child(right)
    }
}
