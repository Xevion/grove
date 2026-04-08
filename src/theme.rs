use gpui::{App, rgb};
use gpui_component::{Theme, ThemeMode};

pub const BG_BASE: u32 = 0x1e_1e2e;
pub const BG_SURFACE: u32 = 0x31_3244;
pub const BG_HOVER: u32 = 0x2a_2b3d;
pub const BG_SELECTED: u32 = 0x36_3a4f;
pub const BG_SELECTED_HOVER: u32 = 0x3e_4263;
pub const TEXT_PRIMARY: u32 = 0xcd_d6f4;
pub const TEXT_SECONDARY: u32 = 0xa6_adc8;
pub const TEXT_MUTED: u32 = 0x6c_7086;
pub const ACCENT: u32 = 0x89_b4fa;
pub const SIDEBAR_BG: u32 = 0x18_1825;
pub const BORDER_COLOR: u32 = 0x31_3244;
pub const BORDER_SUBTLE: u32 = 0x45_475a;
pub const BORDER_INTERACTIVE: u32 = 0x58_5b70;

/// Set gpui-component's theme to dark mode and override colors to match Grove's palette.
pub fn apply_grove_theme(cx: &mut App) {
    Theme::change(ThemeMode::Dark, None, cx);

    let theme = Theme::global_mut(cx);
    let c = &mut theme.colors;

    c.background = rgb(BG_BASE).into();
    c.foreground = rgb(TEXT_PRIMARY).into();
    c.border = rgb(BORDER_INTERACTIVE).into();
    c.muted = rgb(BG_SURFACE).into();
    c.muted_foreground = rgb(TEXT_MUTED).into();
    c.accent = rgb(BG_SELECTED).into();
    c.accent_foreground = rgb(TEXT_PRIMARY).into();
    c.secondary = rgb(BG_HOVER).into();
    c.secondary_active = rgb(BG_SELECTED).into();
    c.secondary_foreground = rgb(TEXT_SECONDARY).into();
    c.secondary_hover = rgb(BG_SELECTED_HOVER).into();

    // Table-specific
    c.table = rgb(BG_BASE).into();
    c.table_head = rgb(BG_SURFACE).into();
    c.table_head_foreground = rgb(TEXT_MUTED).into();
    c.table_hover = rgb(BG_HOVER).into();
    c.table_active = rgb(BG_SELECTED).into();
    c.table_active_border = rgb(ACCENT).into();
    c.table_even = rgb(BG_BASE).into();
    c.table_row_border = rgb(BORDER_COLOR).into();

    // Disable active_highlight overlay (covers text). With it off, selected rows
    // use bg(accent) directly on the row div instead of an absolute overlay.
    theme.list.active_highlight = false;
}
