# Rustfm

A fast, modern terminal file manager written in Rust: sidebar with pinned dirs and disks, multiple file panels, live preview, and a footer with Tasks, Info, and Clipboard panes. Ships with a **"Catppuccin Black"** default theme (Mocha accents on pure black), optional background transparency, a thick frame marking the focused panel, and a full-row highlight cursor.

```
┌─ sidebar ─┐┏ ❯ ~/path ━━━━━━━━━━━━━━━━┓┌ ❯ ~/path ───────────────┐┌──────────────────────┐
│  rustfm  ││   Filter                 ┃│   Filter                ││ fn main() {          │
│           │┃┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┃│┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄││     ...syntax-       │
│ 󰋜 Home    │┃▓▓src/▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓┃│   Documents/            ││     highlighted...   │
│ 󰉍 Downloads┃   Cargo.toml          M  ┃│   notes.txt             ││ }                    │
│ ▪ PINNED  │┃   README.md              ┃│                         ││                      │
│ ▪ DISKS   │┃                          ┃│                         ││                      │
│ 󰋊 root    │┃                          ┃│                         ││                      │
└───────────┘┗ Name ↑ ━━━━━━━━━━━━ 1/3 ━┛└──────────────── 1/2 ────┘└──────────────────────┘
┌  TASKS ──────────────┐┌  INFO ─────────────────────────┐┌  CLIPBOARD ──────────────────┐
│ ✓ Copy 2 items       ││ Name        Cargo.toml         ││ Copy — 2 item(s)             │
│ ▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰ 100% ││ Size        1.2 KiB            ││ README.md                    │
│                      ││ Permissions -rw-r--r-- (644)   ││ notes.txt                    │
└──────────────────────┘└────────────────────────────────┘└──────────────────────────────┘
```

## Features

### Layout
- **Sidebar** with well-known home directories, a **Pinned** section (`P` pins/unpins the current directory), and a **Disks** section listing mounted volumes.
- **Multiple file panels** side by side — `n` opens a new panel, `w` closes it, `tab`/`L`/`H` cycle between them. Each panel keeps its own location, cursor, search filter, and selection.
- **Preview panel** with syntax-highlighted code (via syntect), images (Kitty/iTerm2/Sixel with halfblocks fallback), PDFs (page-by-page via poppler), directories, and git diffs. Toggle with `f`.
- **Footer** with three panes — toggle with `F`:
  - **Tasks** — background copy/move/delete operations with live progress bars, spinners, and success/failure history.
  - **Info** — name, type, size, dates, permissions, owner/group, git status, and repo branch/changes summary for the file under the cursor.
  - **Clipboard** — the current copy/cut buffer contents.
- **Focus zones** — `s` sidebar, `b` task bar, `m` info; `esc`/`tab` returns to the file panel.

### Navigation and selection
- Arrow keys or `j`/`k`, `h`/`l`, `gg`/`G`, `pgup`/`pgdn`, per-directory cursor memory, mouse-wheel scrolling.
- **Search bar** in each panel (`/`) — live substring filter.
- **Fuzzy finder** (`ctrl+f`) — subsequence matching with scored, highlighted results.
- **Select mode** (`v`) — checkbox-style multi-selection: `enter`/`space` toggles, `shift+↑↓`/`J`/`K` selects while moving, `A` selects all.
- **Bookmarks** (`'`) and a **command palette** (`:`) with fuzzy search over every action.

### File operations
- Two key styles work everywhere — vim-flavored and ctrl-based: `y`/`ctrl+c` copy, `d`/`ctrl+x` cut, `p`/`ctrl+v` paste, `D`/`delete`/`ctrl+d` trash, `a`/`ctrl+n` create (trailing `/` for directory), `r`/`ctrl+r` rename. Permanent deletion is available from the command palette only, so a habitual `D` can never hard-wipe files.
- All transfers run on a background worker thread and report into the Processes pane; the UI never blocks.
- Paste collisions auto-create `name_1.ext`, `name_2.ext`, …
- Native **trash** support with Confirm/Cancel button modals.
- System clipboard integration: copied files land on the OS clipboard as `text/uri-list` (pasteable in graphical file managers and browsers), `ctrl+p` copies the path, `c` copies the working directory.

### Git integration
- Per-entry status letters (index + worktree) rendered in the panel, propagated to parent directories.
- Branch, upstream, ahead/behind, staged/unstaged/untracked/conflict/stash summary in the Metadata pane.
- `z`-menu: stage (`s`), unstage (`u`), discard (`x`), commit (`c`), diff preview (`d`), refresh (`r`), raw git command (`g`).

### Opening files
- `enter`/`l` opens with the internal `[openers]` table first, then the OS default handler; executables spawn detached (`shift+enter` runs them interactively).
- `e` opens the file in your editor, `E` opens the directory (`editor` config key, falling back to `$EDITOR`).
- `!` runs a shell command (the TUI suspends for interactive programs), `,` + key runs user-defined command bindings with `{f}`/`{d}`/`{s}`/`{n}` placeholders.

## Installation

```bash
git clone https://github.com/<your-user>/rustfm
cd rustfm
cargo build --release
install -m 755 target/release/rustfm ~/.local/bin/rustfm
```

A [Nerd Font](https://www.nerdfonts.com/) is required for icons (set `nerdfont = false` in the config otherwise). PDF previews need `poppler-utils` (`pdftoppm`/`pdfinfo`).

## Usage

```bash
rustfm [PATH]
```

`PATH` may be a directory (start there) or a file (start in its parent with the cursor on it). Defaults to the current working directory.

## Hotkeys

Press `?` inside Rustfm for the full list:

| Key | Action |
|-----|--------|
| `↑↓` / `j k` | move cursor |
| `h` / `←` / `backspace` | parent directory |
| `l` / `→` / `enter` | open directory / file |
| `/` | search (live filter) |
| `ctrl+f` | fuzzy jump |
| `n` / `w` | new / close file panel |
| `tab` / `L` / `H` | cycle file panels |
| `s` / `b` / `m` | focus sidebar / process bar / metadata |
| `f` / `F` | toggle preview / footer |
| `P` | pin current directory |
| `y` / `d` / `p` (or `ctrl+c/x/v`) | copy / cut / paste |
| `D` / `del` / `ctrl+d` | delete to trash (permanent delete: palette) |
| `a` / `ctrl+n` | create (end with `/` for a directory) |
| `r` / `ctrl+r` | rename |
| `R` | refresh |
| `space` | toggle selection and move down |
| `ctrl+p` / `c` | copy file path / working directory |
| `e` / `E` | open file / dir in editor |
| `v` | select mode (`A` all, `J`/`K` extend) |
| `o` | sort menu (`R` inside reverses) |
| `.` | toggle hidden files |
| `J` / `K`, `{` / `}` | scroll preview / PDF pages |
| `:` / `'` / `!` / `z` | palette / bookmarks / shell / git menu |
| `?` | help |
| `q` | quit (`esc` only backs out of modes) |

## Configuration

TOML config at `~/.config/rustfm/config.toml`, auto-generated on first run. Highlights:

- `sidebar_width`, `file_preview_width`, `footer_height`, `show_footer`, `default_open_file_preview` — layout.
- `editor` — used by `e`/`E` (falls back to `$EDITOR`).
- `[openers]` — extension → command template (`{}` is the path).
- `[commands]` — `,`-prefixed key bindings for shell commands.
- `transparent_background` — when `true`, no pane paints a background and the terminal's own (possibly translucent) background shows through; the cursor-row highlight and modals stay opaque for readability.
- `[theme]` — "Catppuccin Black" by default (Mocha accents on pure black). Every color is a named field taking `#rrggbb` or a named color, so any palette (Gruvbox, Nord, …) is a quick edit away. `code_syntax_highlight` selects the syntect theme for code previews.

Pinned directories are stored separately in `~/.config/rustfm/pinned.toml`.
