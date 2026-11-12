//! Electron palette: neutral gray surfaces, green commands and teal device accents.
use gpui_kit::component::Theme;
use gpui_kit::{rgb, App};

pub fn apply(cx: &mut App) {
    Theme::update(cx, |theme| {
        let dark = theme.is_dark();
        let color = |light, dark_color| rgb(if dark { dark_color } else { light }).into();
        theme.colors.background = color(0xffffff, 0x0a0a0a);
        theme.colors.foreground = color(0x374151, 0xd1d5db);
        theme.colors.sidebar = color(0xf3f4f6, 0x0f0f0f);
        theme.colors.sidebar_foreground = theme.colors.foreground;
        theme.colors.muted = color(0xf3f4f6, 0x171717);
        theme.colors.muted_foreground = color(0x6b7280, 0x9ca3af);
        theme.colors.accent = color(0xe5e7eb, 0x242424);
        theme.colors.accent_foreground = color(0x111827, 0xffffff);
        theme.colors.border = color(0xe5e7eb, 0x242424);
        theme.colors.input = color(0xd1d5db, 0x2a2a2a);
        theme.colors.primary = rgb(0x77a666).into();
        theme.colors.primary_hover = rgb(0x648c56).into();
        theme.colors.primary_active = rgb(0x527647).into();
        theme.colors.primary_foreground = rgb(0x0a0a0a).into();
        theme.colors.success = color(0x16a34a, 0x22c55e);
        theme.colors.info = color(0x0d9488, 0x14b8a6);
        theme.colors.ring = theme.colors.primary;
        theme.colors.caret = theme.colors.primary;
        theme.colors.tab = theme.colors.sidebar;
        theme.colors.tab_bar = theme.colors.sidebar;
        theme.colors.tab_active = theme.colors.accent;
        theme.colors.tab_active_foreground = theme.colors.accent_foreground;
        theme.colors.title_bar = theme.colors.sidebar;
        theme.colors.status_bar = theme.colors.sidebar;
        theme.colors.popover = color(0xffffff, 0x111111);
        theme.colors.popover_foreground = theme.colors.foreground;
    });
}
