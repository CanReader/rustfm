# Rustfm

A fast, modern terminal file manager written in Rust.

Rustfm is a keyboard-driven TUI file manager built around a three-pane Miller-column layout. It aims to be responsive on large directories, comfortable for everyday navigation, and flexible enough to replace a graphical file manager for users who live in the terminal. It targets developers, system administrators, and power users who want a single tool for browsing, previewing, and operating on files without leaving the shell.

## Features

### Layout and previews
- **Three-pane Miller-column layout** showing parent directory, current directory, and a live preview of the selected entry.
- **Image previews** rendered directly in the terminal via the Kitty, iTerm2, or Sixel graphics protocols, with a halfblocks fallback so previews work on any terminal. Implemented on top of `ratatui-image`.
- **Text, directory, and binary previews** with automatic binary detection and MIME type information for unknown files.
- **Nerd Font icons** for folders, symlinks, and dozens of file types including Rust, Go, Python, JavaScript/TypeScript, HTML/CSS, Markdown, TOML, YAML, JSON, images, video, audio, archives, PDF, and office documents.

### Navigation and editing
- **Keyboard-driven navigation** using arrow keys and a compact vim-inspired command set: `gg`/`G`, `/`, `n`/`N`, `y`/`d`/`p`, `D`, `r`, `a`/`A`, `.`, `'`.
- **Per-directory cursor memory** — the cursor position is remembered when you leave and return to a directory.
- **Bookmarks** — press `'` followed by a key to jump to any path defined in `config.toml`.
- **Multi-selection** with `<space>`; yank, cut, and delete all operate on the current selection when one exists.
- **Hidden files toggle** via `.`.

### File operations
- **Background file operations** with a live progress bar. Copy, move, and delete run on a dedicated worker thread over mpsc channels so the UI never blocks. A progress gauge appears above the footer while a task is active.
- **Auto-unique destination on paste collisions** — pasting a file that already exists creates `name_1.ext`, `name_2.ext`, and so on.
- **Native trash support** via the `trash` crate, with direct-delete as a fallback. Toggleable through the `use_trash` config option.

### Discovery
- **Sort modes**: name, size, mtime, and extension, each with a reverse toggle. Triggered by `o` followed by `n`/`s`/`t`/`e`/`r`.
- **Live filter** (`f`) — type to narrow the current directory by substring match. Distinct from search.
- **Fuzzy finder overlay** (`Ctrl-F`) — centered popup with subsequence matching, scoring (consecutive-run, word-boundary, and start-of-string bonuses), and matched-character highlighting.
- **Git integration** — shows an `M`/`A`/`D`/`R`/`?`/`!`/`U` column per entry using `git status --porcelain --ignored`, propagated up to parent directories so a folder is marked if any descendant has changed. Colors are theme-driven.

### Opening files
- **Default application system** — Rustfm first consults an internal `[openers]` table in its config file, mapping file extension to a command template (with `{}` as the path placeholder). If no internal mapping exists for a file's extension, it falls back to the OS default handler: `xdg-open` on Linux, `open` on macOS, `start` on Windows. The status bar indicates which opener will be used, for example `opener:internal(rs)` versus `opener:os-default`.

### Appearance and configuration
- **Configurable theme** with 25 named colors covering borders, cursor, entry types, status, git indicators, progress bar, and overlays. Supports named colors and `#rrggbb` hex values.
- **TOML config** at `~/.config/rustfm/config.toml`, auto-generated with sensible defaults on first run.

## Installation

Build from source with Cargo:

```bash
git clone https://github.com/<your-user>/rustfm
cd rustfm
cargo build --release
```

The release binary will be at `target/release/rustfm`. Copy or symlink it somewhere on your `PATH`, for example:

```bash
install -m 755 target/release/rustfm ~/.local/bin/rustfm
```

A Nerd Font is required for the icon glyphs to render correctly in your terminal. Set your terminal font to any Nerd Font patched variant (for example `JetBrainsMono Nerd Font`, `FiraCode Nerd Font`).

## Usage

```bash
rustfm [PATH]
```

The positional `PATH` argument is optional and may be a directory or a file. If it is a directory, Rustfm starts there. If it is a file, Rustfm starts in its parent directory with the cursor on that file. When omitted, Rustfm starts in the current working directory.

## Keybindings

### Navigation
| Key | Action |
|-----|--------|
| `←` | Go to parent directory |
| `→` / `Enter` | Enter directory or open file |
| `↓` | Move cursor down |
| `↑` | Move cursor up |
| `Ctrl-d` / `Ctrl-u` | Page down / page up |
| `gg` | Jump to top |
| `G` | Jump to bottom |
| `'` `<key>` | Jump to bookmark |

### File operations
| Key | Action |
|-----|--------|
| `<space>` | Toggle selection on current entry |
| `y` | Yank (copy) selection or current entry |
| `d` | Cut selection or current entry |
| `p` | Paste into current directory |
| `D` | Delete (to trash, or hard-delete if `use_trash=false`) |
| `r` | Rename current entry |
| `a` | Create new file |
| `A` | Create new directory |

### Modes and overlays
| Key | Action |
|-----|--------|
| `/` | Search forward |
| `n` / `N` | Next / previous search match |
| `f` | Live filter |
| `Ctrl-F` | Fuzzy finder overlay |
| `o` `n`/`s`/`t`/`e` | Sort by name / size / mtime / extension |
| `o r` | Reverse sort |

### Miscellaneous
| Key | Action |
|-----|--------|
| `.` | Toggle hidden files |
| `q` | Quit |

## Configuration

Rustfm reads its configuration from `~/.config/rustfm/config.toml`. On first launch the file is created with sensible defaults.

### Openers

The `[openers]` table maps file extensions to command templates. `{}` is replaced with the absolute path of the file. Rustfm first consults this table; if the file's extension has no match, it falls back to the OS default handler (`xdg-open` on Linux, `open` on macOS, `start` on Windows).

```toml
[openers]
rs   = "nvim {}"
toml = "nvim {}"
md   = "nvim {}"
py   = "nvim {}"
js   = "nvim {}"
ts   = "nvim {}"
html = "nvim {}"
css  = "nvim {}"
json = "nvim {}"
yaml = "nvim {}"
txt  = "nvim {}"

# Files with extensions not listed above are passed to xdg-open / open / start.
```

The default generated config uses `nvim` for text and code files. Change any entry to your editor of choice, or remove it to let the OS default handler take over.

### Bookmarks

```toml
[bookmarks]
h = "/home/you"
d = "/home/you/Downloads"
p = "/home/you/Desktop/Projects"
c = "/home/you/.config"
```

Press `'` followed by the bookmark key to jump.

### Theme

```toml
[theme]
border           = "#3b4252"
border_active    = "#88c0d0"
cursor_bg        = "#4c566a"
cursor_fg        = "#eceff4"
dir_fg           = "#81a1c1"
file_fg          = "#d8dee9"
symlink_fg       = "#b48ead"
exec_fg          = "#a3be8c"
status_fg        = "#e5e9f0"
git_modified     = "#ebcb8b"
git_added        = "#a3be8c"
git_deleted      = "#bf616a"
git_untracked    = "#d08770"
progress_fg      = "#88c0d0"
overlay_bg       = "#2e3440"
```

All 25 theme keys accept either named colors (`"red"`, `"cyan"`, ...) or `#rrggbb` hex values.

### Other options

```toml
use_trash = true   # false = permanent delete on `D`
show_hidden = false
```

## Requirements

- **Rust 1.75+** recommended for building from source.
- **Linux, macOS, or Windows.** Unix platforms are the primary target; Windows is supported on a best-effort basis.
- **`git` binary on `PATH`** for git status integration (optional).
- **A Nerd Font** terminal font for icon glyphs (optional but recommended).
- **`xdg-open` (Linux) / `open` (macOS) / `start` (Windows)** for opening files that have no internal opener mapping (optional).

## Project structure

```
src/
  main.rs         Entry point; CLI parsing, terminal setup and teardown.
  app.rs          Core application state, pane model, selection, cursor memory.
  config.rs       TOML config loading, defaults, openers and bookmarks.
  events.rs       Main event loop and key dispatch.
  fs_ops.rs       Copy, move, delete, rename, create primitives.
  opener.rs       Internal opener lookup and OS-default fallback.
  preview.rs      Text, directory, binary, and image preview builders.
  ui.rs           Ratatui rendering: three panes, icons, status, overlays.
  theme.rs        Theme struct, color parsing, defaults.
  background.rs   Worker thread, mpsc channels, progress reporting.
  git.rs          Git status parsing and propagation to parent folders.
  fuzzy.rs        Subsequence matcher, scoring, match highlighting.
```

## License

Rustfm is released under the MIT License. See `Cargo.toml` for the declaration.

## Contributing

Issues and pull requests are welcome. If you plan a larger change, please open an issue first to discuss the approach. Bug reports are most useful when they include the terminal emulator, font, and a minimal reproduction.
