use ratatui::style::Color;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    pub active_border: String,
    pub inactive_border: String,
    pub cursor_bg: String,
    pub cursor_fg: String,
    pub directory: String,
    pub symlink: String,
    pub file: String,
    pub readonly: String,
    pub header_user: String,
    pub header_path: String,
    pub info_size: String,
    pub info_dim: String,
    pub info_accent: String,
    pub status_ok: String,
    pub status_err: String,
    pub git_modified: String,
    pub git_added: String,
    pub git_deleted: String,
    pub git_untracked: String,
    pub git_ignored: String,
    pub progress_bar: String,
    pub progress_bg: String,
    pub overlay_bg: String,
    pub overlay_fg: String,
    pub overlay_match: String,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            active_border: "cyan".into(),
            inactive_border: "darkgray".into(),
            cursor_bg: "blue".into(),
            cursor_fg: "white".into(),
            directory: "blue".into(),
            symlink: "magenta".into(),
            file: "white".into(),
            readonly: "darkgray".into(),
            header_user: "green".into(),
            header_path: "cyan".into(),
            info_size: "yellow".into(),
            info_dim: "darkgray".into(),
            info_accent: "magenta".into(),
            status_ok: "green".into(),
            status_err: "red".into(),
            git_modified: "yellow".into(),
            git_added: "green".into(),
            git_deleted: "red".into(),
            git_untracked: "cyan".into(),
            git_ignored: "darkgray".into(),
            progress_bar: "cyan".into(),
            progress_bg: "darkgray".into(),
            overlay_bg: "black".into(),
            overlay_fg: "white".into(),
            overlay_match: "yellow".into(),
        }
    }
}

pub struct Palette {
    pub active_border: Color,
    pub inactive_border: Color,
    pub cursor_bg: Color,
    pub cursor_fg: Color,
    pub directory: Color,
    pub symlink: Color,
    pub file: Color,
    pub readonly: Color,
    pub header_user: Color,
    pub header_path: Color,
    pub info_size: Color,
    pub info_dim: Color,
    pub info_accent: Color,
    pub status_ok: Color,
    pub status_err: Color,
    pub git_modified: Color,
    pub git_added: Color,
    pub git_deleted: Color,
    pub git_untracked: Color,
    pub git_ignored: Color,
    pub progress_bar: Color,
    pub progress_bg: Color,
    pub overlay_bg: Color,
    pub overlay_fg: Color,
    pub overlay_match: Color,
}

impl Palette {
    pub fn from_theme(t: &Theme) -> Self {
        Self {
            active_border: parse_color(&t.active_border).unwrap_or(Color::Cyan),
            inactive_border: parse_color(&t.inactive_border).unwrap_or(Color::DarkGray),
            cursor_bg: parse_color(&t.cursor_bg).unwrap_or(Color::Blue),
            cursor_fg: parse_color(&t.cursor_fg).unwrap_or(Color::White),
            directory: parse_color(&t.directory).unwrap_or(Color::Blue),
            symlink: parse_color(&t.symlink).unwrap_or(Color::Magenta),
            file: parse_color(&t.file).unwrap_or(Color::White),
            readonly: parse_color(&t.readonly).unwrap_or(Color::DarkGray),
            header_user: parse_color(&t.header_user).unwrap_or(Color::Green),
            header_path: parse_color(&t.header_path).unwrap_or(Color::Cyan),
            info_size: parse_color(&t.info_size).unwrap_or(Color::Yellow),
            info_dim: parse_color(&t.info_dim).unwrap_or(Color::DarkGray),
            info_accent: parse_color(&t.info_accent).unwrap_or(Color::Magenta),
            status_ok: parse_color(&t.status_ok).unwrap_or(Color::Green),
            status_err: parse_color(&t.status_err).unwrap_or(Color::Red),
            git_modified: parse_color(&t.git_modified).unwrap_or(Color::Yellow),
            git_added: parse_color(&t.git_added).unwrap_or(Color::Green),
            git_deleted: parse_color(&t.git_deleted).unwrap_or(Color::Red),
            git_untracked: parse_color(&t.git_untracked).unwrap_or(Color::Cyan),
            git_ignored: parse_color(&t.git_ignored).unwrap_or(Color::DarkGray),
            progress_bar: parse_color(&t.progress_bar).unwrap_or(Color::Cyan),
            progress_bg: parse_color(&t.progress_bg).unwrap_or(Color::DarkGray),
            overlay_bg: parse_color(&t.overlay_bg).unwrap_or(Color::Black),
            overlay_fg: parse_color(&t.overlay_fg).unwrap_or(Color::White),
            overlay_match: parse_color(&t.overlay_match).unwrap_or(Color::Yellow),
        }
    }
}

fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim().to_ascii_lowercase();
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
