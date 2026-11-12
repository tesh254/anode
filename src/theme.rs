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
        theme.colors.button_primary = theme.colors.primary;
        theme.colors.button_primary_hover = theme.colors.primary_hover;
        theme.colors.button_primary_active = theme.colors.primary_active;
        theme.colors.button_primary_foreground = theme.colors.primary_foreground;
        theme.colors.success = color(0x16a34a, 0x22c55e);
        theme.colors.info = color(0x0d9488, 0x14b8a6);
        theme.colors.ring = theme.colors.primary;
        theme.colors.caret = color(0x0a0a0a, 0xfafafa);
        // A folder tab rises out of its bar and merges with the surface it
        // opens onto, so the selected fill has to be the content colour and
        // the bar a visibly different one. Giving the unselected label the
        // muted foreground carries the same distinction without relying on
        // the fill alone.
        theme.colors.tab_bar = color(0xe5e7eb, 0x161616);
        theme.colors.tab = theme.colors.tab_bar;
        theme.colors.tab_foreground = theme.colors.muted_foreground;
        theme.colors.tab_active = theme.colors.background;
        theme.colors.tab_active_foreground = theme.colors.accent_foreground;
        theme.colors.title_bar = theme.colors.sidebar;
        theme.colors.status_bar = theme.colors.sidebar;
        theme.colors.popover = color(0xffffff, 0x111111);
        theme.colors.popover_foreground = theme.colors.foreground;
    });
}

/// Console surfaces remain dark in both appearances, as in a hardware terminal.
pub fn terminal_background(cx: &App) -> gpui_kit::Hsla {
    use gpui_kit::component::ActiveTheme as _;
    if cx.theme().is_dark() {
        cx.theme().background
    } else {
        rgb(0x171717).into()
    }
}

pub fn terminal_foreground(_cx: &App) -> gpui_kit::Hsla {
    rgb(0xd1d5db).into()
}

pub fn terminal_caret(_cx: &App) -> gpui_kit::Hsla {
    rgb(0xfafafa).into()
}

/// The colours the device log paints its lines in.
///
/// The console keeps a dark background in both appearances, so it cannot read
/// severity from the window theme: a light theme's `danger` is a dark red that
/// would disappear here. These are the same bright values the terminal's ANSI
/// palette uses, which also keeps Output and Terminal looking like one
/// surface.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct ConsolePalette {
    /// Ordinary tool output.
    pub text: gpui_kit::Hsla,
    /// Timestamps and placeholders.
    pub dim: gpui_kit::Hsla,
    /// A step that completed.
    pub ok: gpui_kit::Hsla,
    pub warn: gpui_kit::Hsla,
    pub error: gpui_kit::Hsla,
    /// Anode's own narration, as distinct from a tool's.
    pub info: gpui_kit::Hsla,
}

pub fn console_palette() -> ConsolePalette {
    ConsolePalette {
        text: rgb(0xd1d5db).into(),
        dim: rgb(0x6b7280).into(),
        ok: rgb(0xb5df94).into(),
        warn: rgb(0xffdc97).into(),
        error: rgb(0xff7b86).into(),
        info: rgb(0x83c7ff).into(),
    }
}

/// ANSI colors are terminal data, independent of the application's accent roles.
pub(crate) fn terminal_ansi_color(color: vt100::Color, default: gpui_kit::Hsla) -> gpui_kit::Hsla {
    let value = match color {
        vt100::Color::Default => return default,
        vt100::Color::Rgb(r, g, b) => (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b),
        vt100::Color::Idx(index) => {
            const ANSI: [u32; 16] = [
                0x171717, 0xe06c75, 0x98c379, 0xe5c07b, 0x61afef, 0xc678dd, 0x56b6c2, 0xd1d5db,
                0x6b7280, 0xff7b86, 0xb5df94, 0xffdc97, 0x83c7ff, 0xe09aff, 0x7dd9df, 0xffffff,
            ];
            if index < 16 {
                ANSI[usize::from(index)]
            } else if index < 232 {
                let index = u32::from(index - 16);
                let channel = |value| if value == 0 { 0 } else { 55 + value * 40 };
                (channel(index / 36) << 16) | (channel(index / 6 % 6) << 8) | channel(index % 6)
            } else {
                let gray = 8 + u32::from(index - 232) * 10;
                (gray << 16) | (gray << 8) | gray
            }
        }
    };
    rgb(value).into()
}
