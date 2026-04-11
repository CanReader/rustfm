use crate::theme::Theme;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortMode {
    Name,
    Size,
    Mtime,
    Ext,
}

impl Default for SortMode {
    fn default() -> Self {
        SortMode::Name
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub show_hidden: bool,
    pub preview_enabled: bool,
    pub confirm_delete: bool,
    pub use_trash: bool,
    pub ratios: [u16; 3],
    pub date_format: String,
    pub icons: bool,
    pub git_integration: bool,
    pub sort: SortMode,
    pub sort_reverse: bool,
    pub dirs_first: bool,
    pub openers: HashMap<String, String>,
    pub bookmarks: HashMap<String, String>,
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
            preview_enabled: true,
            confirm_delete: true,
            use_trash: true,
            ratios: [1, 2, 3],
            date_format: "%Y-%m-%d %H:%M".into(),
            icons: true,
            git_integration: true,
            sort: SortMode::Name,
            sort_reverse: false,
            dirs_first: true,
            openers,
            bookmarks: HashMap::new(),
            theme: Theme::default(),
        }
    }
}

fn default_editor() -> String {
    std::env::var("EDITOR").unwrap_or_else(|_| "nvim".into())
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        if !path.exists() {
            Self::write_default(&path)?;
            return Ok(Self::default());
        }
        let contents = fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let cfg: Config = toml::from_str(&contents)
            .with_context(|| format!("parsing {}", path.display()))?;
        Ok(cfg)
    }

    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("rustfm")
            .join("config.toml")
    }

    fn write_default(path: &PathBuf) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        let default_contents = r#"# Rustfm configuration
show_hidden = false
preview_enabled = true
confirm_delete = true
use_trash = true
ratios = [1, 2, 3]
date_format = "%Y-%m-%d %H:%M"
icons = true
git_integration = true
sort = "name"          # name | size | mtime | ext
sort_reverse = false
dirs_first = true

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

[theme]
active_border = "cyan"
inactive_border = "darkgray"
cursor_bg = "blue"
cursor_fg = "white"
directory = "blue"
symlink = "magenta"
file = "white"
readonly = "darkgray"
header_user = "green"
header_path = "cyan"
info_size = "yellow"
info_dim = "darkgray"
info_accent = "magenta"
status_ok = "green"
status_err = "red"
git_modified = "yellow"
git_added = "green"
git_deleted = "red"
git_untracked = "cyan"
git_ignored = "darkgray"
progress_bar = "cyan"
progress_bg = "darkgray"
overlay_bg = "black"
overlay_fg = "white"
overlay_match = "yellow"
"#;
        fs::write(path, default_contents).ok();
        Ok(())
    }
}
