use ratatui::style::Color;
use serde::{Deserialize, Serialize};

/// Theme definition. Every color is a named TOML field so users
/// can port palettes from other tools with minimal edits. Defaults are Catppuccin
/// Mocha accents on pure black.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    // -- Code preview syntax highlighting (syntect theme name)
    pub code_syntax_highlight: String,

    // -- Gradient used for the logo / progress bars (two colors)
    pub gradient_color: Vec<String>,

    // -- File panel
    pub file_panel_fg: String,
    pub file_panel_bg: String,
    pub file_panel_border: String,
    pub file_panel_border_active: String,
    pub file_panel_top_directory_icon: String,
    pub file_panel_top_path: String,
    pub file_panel_item_selected_fg: String,
    pub file_panel_item_selected_bg: String,

    // -- Footer (processes / metadata / clipboard)
    pub footer_fg: String,
    pub footer_bg: String,
    pub footer_border: String,
    pub footer_border_active: String,

    // -- Sidebar
    pub sidebar_fg: String,
    pub sidebar_bg: String,
    pub sidebar_title: String,
    pub sidebar_border: String,
    pub sidebar_border_active: String,
    pub sidebar_item_selected_fg: String,
    pub sidebar_item_selected_bg: String,
    pub sidebar_divider: String,

    // -- Modals
    pub modal_fg: String,
    pub modal_bg: String,
    pub modal_border_active: String,
    pub modal_cancel_fg: String,
    pub modal_cancel_bg: String,
    pub modal_confirm_fg: String,
    pub modal_confirm_bg: String,

    // -- Help menu
    pub help_menu_hotkey: String,
    pub help_menu_title: String,

    // -- Special
    pub cursor: String,
    pub correct: String,
    pub error: String,
    pub hint: String,
    pub cancel: String,

    // -- Entry kinds (rustfm extension)
    pub directory: String,
    pub symlink: String,
    pub executable: String,
    pub readonly: String,

    // -- Git (rustfm extension)
    pub git_modified: String,
    pub git_added: String,
    pub git_deleted: String,
    pub git_untracked: String,
    pub git_ignored: String,
}

/// Default look: "Catppuccin Black" — Catppuccin Mocha accents on a pure
/// black background. The structural identity (thick focus frame,
/// row-highlight cursor, section headers) is rustfm's own regardless of
/// palette.
impl Default for Theme {
    fn default() -> Self {
        Self {
            code_syntax_highlight: "base16-eighties.dark".into(),
            gradient_color: vec!["#89b4fa".into(), "#cba6f7".into()],

            file_panel_fg: "#cdd6f4".into(),
            file_panel_bg: "#000000".into(),
            file_panel_border: "#45475a".into(),
            file_panel_border_active: "#b4befe".into(),
            file_panel_top_directory_icon: "#a6e3a1".into(),
            file_panel_top_path: "#89b4fa".into(),
            file_panel_item_selected_fg: "#89dceb".into(),
            file_panel_item_selected_bg: "#313244".into(),

            footer_fg: "#cdd6f4".into(),
            footer_bg: "#000000".into(),
            footer_border: "#45475a".into(),
            footer_border_active: "#a6e3a1".into(),

            sidebar_fg: "#a6adc8".into(),
            sidebar_bg: "#000000".into(),
            sidebar_title: "#74c7ec".into(),
            sidebar_border: "#45475a".into(),
            sidebar_border_active: "#f38ba8".into(),
            sidebar_item_selected_fg: "#89dceb".into(),
            sidebar_item_selected_bg: "#313244".into(),
            sidebar_divider: "#585b70".into(),

            modal_fg: "#cdd6f4".into(),
            modal_bg: "#11111b".into(),
            modal_border_active: "#89b4fa".into(),
            modal_cancel_fg: "#11111b".into(),
            modal_cancel_bg: "#eba0ac".into(),
            modal_confirm_fg: "#11111b".into(),
            modal_confirm_bg: "#89dceb".into(),

            help_menu_hotkey: "#89dceb".into(),
            help_menu_title: "#eba0ac".into(),

            cursor: "#f5e0dc".into(),
            correct: "#a6e3a1".into(),
            error: "#f38ba8".into(),
            hint: "#89dceb".into(),
            cancel: "#eba0ac".into(),

            directory: "#89b4fa".into(),
            symlink: "#cba6f7".into(),
            executable: "#a6e3a1".into(),
            readonly: "#6c7086".into(),

            git_modified: "#f9e2af".into(),
            git_added: "#a6e3a1".into(),
            git_deleted: "#f38ba8".into(),
            git_untracked: "#94e2d5".into(),
            git_ignored: "#6c7086".into(),
        }
    }
}

/// Parsed, render-ready colors.
pub struct Palette {
    pub gradient: (Color, Color),

    pub file_panel_fg: Color,
    pub file_panel_bg: Color,
    pub file_panel_border: Color,
    pub file_panel_border_active: Color,
    pub file_panel_top_directory_icon: Color,
    pub file_panel_top_path: Color,
    pub file_panel_item_selected_fg: Color,
    pub file_panel_item_selected_bg: Color,

    pub footer_fg: Color,
    pub footer_bg: Color,
    pub footer_border: Color,
    pub footer_border_active: Color,

    pub sidebar_fg: Color,
    pub sidebar_bg: Color,
    pub sidebar_title: Color,
    pub sidebar_border: Color,
    pub sidebar_border_active: Color,
    pub sidebar_item_selected_fg: Color,
    pub sidebar_item_selected_bg: Color,
    pub sidebar_divider: Color,

    pub modal_fg: Color,
    pub modal_bg: Color,
    pub modal_border_active: Color,
    pub modal_cancel_fg: Color,
    pub modal_cancel_bg: Color,
    pub modal_confirm_fg: Color,
    pub modal_confirm_bg: Color,

    pub help_menu_hotkey: Color,
    pub help_menu_title: Color,

    pub cursor: Color,
    pub correct: Color,
    pub error: Color,
    pub hint: Color,
    pub cancel: Color,

    pub directory: Color,
    pub symlink: Color,
    pub executable: Color,
    pub readonly: Color,

    pub git_modified: Color,
    pub git_added: Color,
    pub git_deleted: Color,
    pub git_untracked: Color,
    pub git_ignored: Color,
}

impl Palette {
    pub fn from_theme(t: &Theme) -> Self {
        let c = |s: &str, fallback: Color| parse_color(s).unwrap_or(fallback);
        let g0 = t
            .gradient_color
            .first()
            .map(String::as_str)
            .unwrap_or("#89b4fa");
        let g1 = t
            .gradient_color
            .get(1)
            .map(String::as_str)
            .unwrap_or("#cba6f7");
        Self {
            gradient: (
                c(g0, Color::Rgb(0x89, 0xb4, 0xfa)),
                c(g1, Color::Rgb(0xcb, 0xa6, 0xf7)),
            ),

            file_panel_fg: c(&t.file_panel_fg, Color::Gray),
            file_panel_bg: c(&t.file_panel_bg, Color::Reset),
            file_panel_border: c(&t.file_panel_border, Color::DarkGray),
            file_panel_border_active: c(&t.file_panel_border_active, Color::Cyan),
            file_panel_top_directory_icon: c(&t.file_panel_top_directory_icon, Color::Green),
            file_panel_top_path: c(&t.file_panel_top_path, Color::Blue),
            file_panel_item_selected_fg: c(&t.file_panel_item_selected_fg, Color::LightBlue),
            file_panel_item_selected_bg: c(&t.file_panel_item_selected_bg, Color::Reset),

            footer_fg: c(&t.footer_fg, Color::Gray),
            footer_bg: c(&t.footer_bg, Color::Reset),
            footer_border: c(&t.footer_border, Color::DarkGray),
            footer_border_active: c(&t.footer_border_active, Color::Green),

            sidebar_fg: c(&t.sidebar_fg, Color::Gray),
            sidebar_bg: c(&t.sidebar_bg, Color::Reset),
            sidebar_title: c(&t.sidebar_title, Color::Cyan),
            sidebar_border: c(&t.sidebar_border, Color::Reset),
            sidebar_border_active: c(&t.sidebar_border_active, Color::Red),
            sidebar_item_selected_fg: c(&t.sidebar_item_selected_fg, Color::LightBlue),
            sidebar_item_selected_bg: c(&t.sidebar_item_selected_bg, Color::Reset),
            sidebar_divider: c(&t.sidebar_divider, Color::DarkGray),

            modal_fg: c(&t.modal_fg, Color::Gray),
            modal_bg: c(&t.modal_bg, Color::Black),
            modal_border_active: c(&t.modal_border_active, Color::Gray),
            modal_cancel_fg: c(&t.modal_cancel_fg, Color::Black),
            modal_cancel_bg: c(&t.modal_cancel_bg, Color::Red),
            modal_confirm_fg: c(&t.modal_confirm_fg, Color::Black),
            modal_confirm_bg: c(&t.modal_confirm_bg, Color::Cyan),

            help_menu_hotkey: c(&t.help_menu_hotkey, Color::Cyan),
            help_menu_title: c(&t.help_menu_title, Color::Red),

            cursor: c(&t.cursor, Color::White),
            correct: c(&t.correct, Color::Green),
            error: c(&t.error, Color::Red),
            hint: c(&t.hint, Color::Cyan),
            cancel: c(&t.cancel, Color::Red),

            directory: c(&t.directory, Color::Blue),
            symlink: c(&t.symlink, Color::Magenta),
            executable: c(&t.executable, Color::Green),
            readonly: c(&t.readonly, Color::DarkGray),

            git_modified: c(&t.git_modified, Color::Yellow),
            git_added: c(&t.git_added, Color::Green),
            git_deleted: c(&t.git_deleted, Color::Red),
            git_untracked: c(&t.git_untracked, Color::Cyan),
            git_ignored: c(&t.git_ignored, Color::DarkGray),
        }
    }

    /// Linear interpolation across the theme gradient — used for the logo
    /// text and task progress bars.
    pub fn gradient_at(&self, t: f32) -> Color {
        let (a, b) = (self.gradient.0, self.gradient.1);
        let (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) = (a, b) else {
            return a;
        };
        let t = t.clamp(0.0, 1.0);
        let lerp = |x: u8, y: u8| -> u8 { (x as f32 + (y as f32 - x as f32) * t).round() as u8 };
        Color::Rgb(lerp(r1, r2), lerp(g1, g2), lerp(b1, b2))
    }
}

pub fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim().to_ascii_lowercase();
    if s.is_empty() {
        return None;
    }
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(Color::Rgb(r, g, b));
        }
    }
    Some(match s.as_str() {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "gray" | "grey" => Color::Gray,
        "darkgray" | "darkgrey" => Color::DarkGray,
        "lightred" => Color::LightRed,
        "lightgreen" => Color::LightGreen,
        "lightyellow" => Color::LightYellow,
        "lightblue" => Color::LightBlue,
        "lightmagenta" => Color::LightMagenta,
        "lightcyan" => Color::LightCyan,
        "white" => Color::White,
        "reset" => Color::Reset,
        _ => return None,
    })
}
