use crate::theme::Theme;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum SortMode {
    #[default]
    Name,
    Size,
    Mtime,
    Ext,
}

impl SortMode {
    pub fn label(&self) -> &'static str {
        match self {
            SortMode::Name => "Name",
            SortMode::Size => "Size",
            SortMode::Mtime => "Date Modified",
            SortMode::Ext => "Type",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    // -- Behaviour
    pub show_hidden: bool,
    pub confirm_delete: bool,
    pub use_trash: bool,
    pub git_integration: bool,
    pub sort: SortMode,
    pub sort_reverse: bool,
    pub dirs_first: bool,
    pub date_format: String,
    pub file_size_use_si: bool,

    // -- Editor ($EDITOR fallback when empty)
    pub editor: String,

    // -- Layout
    pub sidebar_width: u16,
    pub file_preview_width: u16,
    pub default_open_file_preview: bool,
    pub show_footer: bool,
    pub footer_height: u16,
    pub transparent_background: bool,
    pub nerdfont: bool,

    // -- Tables
    pub openers: HashMap<String, String>,
    pub bookmarks: HashMap<String, String>,
    pub commands: HashMap<String, String>,
    pub theme: Theme,
}

impl Default for Config {
    fn default() -> Self {
        let mut openers = HashMap::new();
        for ext in [
            "txt", "md", "rs", "go", "py", "js", "ts", "json", "toml", "yaml", "yml", "c", "cpp",
            "h", "sh", "conf", "log", "lock",
        ] {
            openers.insert(ext.into(), default_editor());
        }
        Self {
            show_hidden: false,
            confirm_delete: true,
            use_trash: true,
            git_integration: true,
            sort: SortMode::Name,
            sort_reverse: false,
            dirs_first: true,
            date_format: "%Y-%m-%d %H:%M".into(),
            file_size_use_si: false,
            editor: String::new(),
            sidebar_width: 20,
            file_preview_width: 0,
            default_open_file_preview: true,
            show_footer: true,
            footer_height: 12,
            transparent_background: false,
            nerdfont: true,
            openers,
            bookmarks: HashMap::new(),
            commands: HashMap::new(),
            theme: Theme::default(),
        }
    }
}

fn default_editor() -> String {
    std::env::var("EDITOR").unwrap_or_else(|_| "nvim".into())
}

impl Config {
    /// Resolved editor command: config value, then $EDITOR, then nvim.
    pub fn editor_cmd(&self) -> String {
        if !self.editor.trim().is_empty() {
            return self.editor.clone();
        }
        default_editor()
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        if !path.exists() {
            Self::write_default(&path)?;
            return Ok(Self::default());
        }
        let contents =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let cfg: Config =
            toml::from_str(&contents).with_context(|| format!("parsing {}", path.display()))?;
        Ok(cfg)
    }

    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("rustfm")
            .join("config.toml")
    }

    fn pinned_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("rustfm")
            .join("pinned.toml")
    }

    /// Pinned directories shown in the sidebar ("Pinned"
    /// section). Stored in their own file so toggling a pin never touches
    /// the hand-edited main config.
    pub fn load_pinned() -> Vec<String> {
        #[derive(Deserialize, Default)]
        struct Pinned {
            #[serde(default)]
            pinned: Vec<String>,
        }
        let Ok(contents) = fs::read_to_string(Self::pinned_path()) else {
            return Vec::new();
        };
        toml::from_str::<Pinned>(&contents)
            .map(|p| p.pinned)
            .unwrap_or_default()
    }

    pub fn save_pinned(pinned: &[String]) -> Result<()> {
        let path = Self::pinned_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        let mut out = String::from("# Rustfm pinned directories (sidebar)\npinned = [\n");
        for p in pinned {
            out.push_str(&format!("    \"{}\",\n", toml_escape(p)));
        }
        out.push_str("]\n");
        fs::write(&path, out).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    /// Persists the `[bookmarks]` table in the user's config file in place,
    /// preserving every other section, comment, and value. Uses a naive
    /// section-replacement strategy rather than a full TOML round-trip so
    /// hand-written comments elsewhere in the file aren't clobbered.
    pub fn save_bookmarks(bookmarks: &HashMap<String, String>) -> Result<()> {
        let path = Self::config_path();
        if !path.exists() {
            Self::write_default(&path)?;
        }
        let original =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let body = serialize_bookmarks_body(bookmarks);
        let new_contents = replace_toml_section(&original, "bookmarks", &body);
        let tmp = path.with_extension("toml.tmp");
        if let Some(parent) = tmp.parent() {
            fs::create_dir_all(parent).ok();
        }
        fs::write(&tmp, new_contents).with_context(|| format!("writing {}", tmp.display()))?;
        fs::rename(&tmp, &path)
            .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
        Ok(())
    }

    fn write_default(path: &PathBuf) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        let default_contents = r##"# Rustfm configuration
show_hidden = false
confirm_delete = true
use_trash = true
git_integration = true
sort = "name"          # name | size | mtime | ext
sort_reverse = false
dirs_first = true
date_format = "%Y-%m-%d %H:%M"
file_size_use_si = false

# Editor used by `e` / `E`. Empty means $EDITOR.
editor = ""

# -- Layout
# Sidebar width; 0 hides the sidebar entirely.
sidebar_width = 20
# Preview panel width will be 1/n of total width. 0: same width as a file panel.
file_preview_width = 0
default_open_file_preview = true
# Bottom bar with Processes / Metadata / Clipboard (toggle at runtime with F).
show_footer = true
footer_height = 12
transparent_background = false
# Nerd Font icons (requires a patched font).
nerdfont = true

# Internal opener table: extension -> command template.
# {} is replaced by the file path. If omitted, path is appended.
# Rustfm consults this map FIRST; if no match, it falls back to the OS default app.
[openers]
txt = "nvim {}"
md = "nvim {}"
rs = "nvim {}"
go = "nvim {}"
py = "nvim {}"
toml = "nvim {}"
yaml = "nvim {}"
yml = "nvim {}"
json = "nvim {}"
conf = "nvim {}"
sh = "nvim {}"
c = "nvim {}"
cpp = "nvim {}"
h = "nvim {}"
lock = "nvim {}"
png = "feh {}"
jpg = "feh {}"
jpeg = "feh {}"
gif = "feh {}"
mp4 = "mpv {}"
mkv = "mpv {}"
webm = "mpv {}"
mp3 = "mpv {}"
flac = "mpv {}"
pdf = "zathura {}"

[bookmarks]
h = "~"
c = "~/.config"

# Key-bound shell commands. Press `,` then the key to run.
# Placeholders: {f} current file, {d} current dir, {s} selection (space-joined),
#               {n} current file name (basename). Paths are shell-quoted.
# The TUI is suspended while the command runs, then restored.
[commands]
e = "nvim {f}"
g = "lazygit"
t = "htop"

# "Catppuccin Black" — Catppuccin Mocha accents on a pure black background.
# Field names are stable across releases, so you can paste
# a saved theme in here. Set transparent_background = true (above) to let your
# terminal's background show through instead.
[theme]
code_syntax_highlight = "base16-eighties.dark"
gradient_color = ["#89b4fa", "#cba6f7"]
file_panel_fg = "#cdd6f4"
file_panel_bg = "#000000"
file_panel_border = "#45475a"
file_panel_border_active = "#b4befe"
file_panel_top_directory_icon = "#a6e3a1"
file_panel_top_path = "#89b4fa"
file_panel_item_selected_fg = "#89dceb"
file_panel_item_selected_bg = "#313244"
footer_fg = "#cdd6f4"
footer_bg = "#000000"
footer_border = "#45475a"
footer_border_active = "#a6e3a1"
sidebar_fg = "#a6adc8"
sidebar_bg = "#000000"
sidebar_title = "#74c7ec"
sidebar_border = "#45475a"
sidebar_border_active = "#f38ba8"
sidebar_item_selected_fg = "#89dceb"
sidebar_item_selected_bg = "#313244"
sidebar_divider = "#585b70"
modal_fg = "#cdd6f4"
modal_bg = "#11111b"
modal_border_active = "#89b4fa"
modal_cancel_fg = "#11111b"
modal_cancel_bg = "#eba0ac"
modal_confirm_fg = "#11111b"
modal_confirm_bg = "#89dceb"
help_menu_hotkey = "#89dceb"
help_menu_title = "#eba0ac"
cursor = "#f5e0dc"
correct = "#a6e3a1"
error = "#f38ba8"
hint = "#89dceb"
cancel = "#eba0ac"
directory = "#89b4fa"
symlink = "#cba6f7"
executable = "#a6e3a1"
readonly = "#6c7086"
git_modified = "#f9e2af"
git_added = "#a6e3a1"
git_deleted = "#f38ba8"
git_untracked = "#94e2d5"
git_ignored = "#6c7086"
"##;
        fs::write(path, default_contents).ok();
        Ok(())
    }
}

fn serialize_bookmarks_body(map: &HashMap<String, String>) -> String {
    let mut entries: Vec<(&String, &String)> = map.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    let mut out = String::new();
    for (k, v) in entries {
        let key_repr = if !k.is_empty()
            && k.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            k.clone()
        } else {
            format!("\"{}\"", toml_escape(k))
        };
        out.push_str(&format!("{key_repr} = \"{}\"\n", toml_escape(v)));
    }
    out
}

fn toml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

fn replace_toml_section(original: &str, section: &str, body: &str) -> String {
    let header = format!("[{section}]");
    let lines: Vec<&str> = original.lines().collect();
    let header_idx = lines.iter().position(|l| l.trim() == header);

    match header_idx {
        Some(start) => {
            let end = lines[start + 1..]
                .iter()
                .position(|l| l.trim_start().starts_with('[') && l.trim_end().ends_with(']'))
                .map(|i| start + 1 + i)
                .unwrap_or(lines.len());
            let mut out = String::new();
            for line in &lines[..=start] {
                out.push_str(line);
                out.push('\n');
            }
            out.push_str(body);
            if end < lines.len() {
                out.push('\n');
                for line in &lines[end..] {
                    out.push_str(line);
                    out.push('\n');
                }
            }
            // Preserve the trailing-newline policy of the original file.
            if !original.ends_with('\n') {
                out.pop();
            }
            out
        }
        None => {
            let mut out = original.trim_end_matches('\n').to_string();
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str(&header);
            out.push('\n');
            out.push_str(body);
            if original.ends_with('\n') && !out.ends_with('\n') {
                out.push('\n');
            }
            out
        }
    }
}
