use anyhow::Result;
use ratatui_image::picker::Picker;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use crate::{
    background::{Task, TaskMsg, Worker},
    clipboard,
    config::{Config, SortMode},
    fs_ops::Entry,
    fuzzy,
    git::{self, GitInfo},
    metadata,
    opener::Opener,
    panel::{FilePanel, PanelMode},
    preview::{self, Preview},
    sidebar::Sidebar,
    theme::Palette,
};

pub const MAX_PANELS: usize = 4;
const MAX_PROCESSES: usize = 30;

/// Which zone owns navigation keys: `s` sidebar,
/// `p` process bar, `m` metadata; anything else returns to the file panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    FilePanel,
    Sidebar,
    Processes,
    Metadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Prompt(PromptKind),
    /// Typing into the active panel's search bar.
    Search,
    Fuzzy,
    ConfirmDelete {
        permanent: bool,
    },
    /// Quit requested while background tasks are still running.
    ConfirmQuit,
    SortMenu,
    Help,
    Palette,
    Bookmarks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    Rename,
    New,
    GoTo,
    CommitMsg,
    GitCmd,
    Shell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipMode {
    Copy,
    Cut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    Running,
    Successful,
    Failed,
}

/// One entry in the Tasks pane. Created when a task is
/// submitted to the worker; updated from `TaskMsg`s; kept after completion
/// so the user can see history.
pub struct Process {
    pub id: u64,
    pub label: String,
    pub state: ProcessState,
    pub done: u64,
    pub total: u64,
    pub current: String,
    pub started: Instant,
}

pub struct StatusMessage {
    pub text: String,
    pub is_error: bool,
    pub at: Instant,
}

#[derive(Debug, Clone)]
pub struct FuzzyMatch {
    pub index: usize,
    pub score: i32,
    pub match_positions: Vec<usize>,
}

pub struct PdfState {
    pub path: PathBuf,
    pub page: u32,
    pub total: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteAction {
    Yank,
    Cut,
    Paste,
    Delete,
    PermanentDelete,
    Rename,
    NewEntry,
    NewPanel,
    ClosePanel,
    TogglePreview,
    ToggleFooter,
    PinDirectory,
    CopyPath,
    CopyName,
    CopyDir,
    CopyFilesToClipboard,
    ToggleHidden,
    ToggleDiff,
    Refresh,
    SelectAll,
    ClearSelection,
    SortName,
    SortSize,
    SortMtime,
    SortExt,
    SortReverse,
    GitStage,
    GitUnstage,
    GitDiscard,
    GitCommit,
    GitDiff,
    GitRefresh,
    GitRaw,
    GoTo,
    Bookmark,
    Shell,
    OpenWithEditor,
    OpenDirWithEditor,
    Help,
    Quit,
}

pub struct PaletteEntry {
    pub action: PaletteAction,
    pub label: &'static str,
    pub hint: &'static str,
}

pub const PALETTE_ENTRIES: &[PaletteEntry] = &[
    PaletteEntry {
        action: PaletteAction::Yank,
        label: "Copy selection",
        hint: "y",
    },
    PaletteEntry {
        action: PaletteAction::Cut,
        label: "Cut selection",
        hint: "d",
    },
    PaletteEntry {
        action: PaletteAction::Paste,
        label: "Paste into current directory",
        hint: "p",
    },
    PaletteEntry {
        action: PaletteAction::Delete,
        label: "Delete selection (trash)",
        hint: "D",
    },
    PaletteEntry {
        action: PaletteAction::PermanentDelete,
        label: "Delete selection permanently",
        hint: "",
    },
    PaletteEntry {
        action: PaletteAction::Rename,
        label: "Rename current entry",
        hint: "r",
    },
    PaletteEntry {
        action: PaletteAction::NewEntry,
        label: "New file or directory",
        hint: "a",
    },
    PaletteEntry {
        action: PaletteAction::NewPanel,
        label: "Open new file panel",
        hint: "n",
    },
    PaletteEntry {
        action: PaletteAction::ClosePanel,
        label: "Close file panel",
        hint: "w",
    },
    PaletteEntry {
        action: PaletteAction::TogglePreview,
        label: "Toggle preview panel",
        hint: "f",
    },
    PaletteEntry {
        action: PaletteAction::ToggleFooter,
        label: "Toggle footer panels",
        hint: "F",
    },
    PaletteEntry {
        action: PaletteAction::PinDirectory,
        label: "Pin/unpin current directory",
        hint: "P",
    },
    PaletteEntry {
        action: PaletteAction::OpenWithEditor,
        label: "Open file with editor",
        hint: "e",
    },
    PaletteEntry {
        action: PaletteAction::OpenDirWithEditor,
        label: "Open directory with editor",
        hint: "E",
    },
    PaletteEntry {
        action: PaletteAction::CopyPath,
        label: "Copy absolute path to clipboard",
        hint: "C-p",
    },
    PaletteEntry {
        action: PaletteAction::CopyName,
        label: "Copy filename to clipboard",
        hint: "",
    },
    PaletteEntry {
        action: PaletteAction::CopyDir,
        label: "Copy working directory path",
        hint: "c",
    },
    PaletteEntry {
        action: PaletteAction::CopyFilesToClipboard,
        label: "Copy files to system clipboard",
        hint: "",
    },
    PaletteEntry {
        action: PaletteAction::ToggleHidden,
        label: "Toggle hidden files",
        hint: ".",
    },
    PaletteEntry {
        action: PaletteAction::ToggleDiff,
        label: "Toggle git diff preview",
        hint: "z d",
    },
    PaletteEntry {
        action: PaletteAction::Refresh,
        label: "Refresh directory",
        hint: "R",
    },
    PaletteEntry {
        action: PaletteAction::SelectAll,
        label: "Select all entries",
        hint: "A",
    },
    PaletteEntry {
        action: PaletteAction::ClearSelection,
        label: "Clear selection",
        hint: "Esc",
    },
    PaletteEntry {
        action: PaletteAction::SortName,
        label: "Sort by name",
        hint: "o",
    },
    PaletteEntry {
        action: PaletteAction::SortSize,
        label: "Sort by size",
        hint: "o",
    },
    PaletteEntry {
        action: PaletteAction::SortMtime,
        label: "Sort by modified time",
        hint: "o",
    },
    PaletteEntry {
        action: PaletteAction::SortExt,
        label: "Sort by type",
        hint: "o",
    },
    PaletteEntry {
        action: PaletteAction::SortReverse,
        label: "Reverse sort order",
        hint: "o",
    },
    PaletteEntry {
        action: PaletteAction::GitStage,
        label: "Git: stage",
        hint: "z s",
    },
    PaletteEntry {
        action: PaletteAction::GitUnstage,
        label: "Git: unstage",
        hint: "z u",
    },
    PaletteEntry {
        action: PaletteAction::GitDiscard,
        label: "Git: discard worktree changes",
        hint: "z x",
    },
    PaletteEntry {
        action: PaletteAction::GitCommit,
        label: "Git: commit staged changes",
        hint: "z c",
    },
    PaletteEntry {
        action: PaletteAction::GitDiff,
        label: "Git: toggle diff preview",
        hint: "z d",
    },
    PaletteEntry {
        action: PaletteAction::GitRefresh,
        label: "Git: refresh state",
        hint: "z r",
    },
    PaletteEntry {
        action: PaletteAction::GitRaw,
        label: "Git: run arbitrary command",
        hint: "z g",
    },
    PaletteEntry {
        action: PaletteAction::GoTo,
        label: "Go to path…",
        hint: "",
    },
    PaletteEntry {
        action: PaletteAction::Bookmark,
        label: "Open bookmarks…",
        hint: "'",
    },
    PaletteEntry {
        action: PaletteAction::Shell,
        label: "Run shell command",
        hint: "!",
    },
    PaletteEntry {
        action: PaletteAction::Help,
        label: "Open help menu",
        hint: "?",
    },
    PaletteEntry {
        action: PaletteAction::Quit,
        label: "Quit Rustfm",
        hint: "q",
    },
];

pub struct App {
    pub config: Config,
    pub palette: Palette,

    // -- Panels / focus
    pub panels: Vec<FilePanel>,
    pub active_panel: usize,
    pub focus: Focus,
    pub sidebar: Sidebar,
    pub pinned: Vec<String>,

    // -- Preview
    pub preview: Preview,
    pub preview_scroll: u16,
    pub preview_pending: Option<(PathBuf, Instant)>,
    pub pdf_state: Option<PdfState>,
    pub preview_visible: bool,
    /// Debounce timer for preview + metadata regeneration. Generating a
    /// preview can be expensive (syntax highlighting, image decode, dir
    /// listing), so holding j/k must not regenerate on every keypress —
    /// we wait until the cursor rests for a moment.
    pub preview_debounce: Option<Instant>,

    // -- Footer
    pub footer_visible: bool,
    pub processes: Vec<Process>,
    pub proc_scroll: usize,
    pub metadata: Vec<(String, String)>,
    pub meta_scroll: usize,

    // -- Clipboard
    pub clipboard: Vec<PathBuf>,
    pub clip_mode: ClipMode,

    // -- Modes / overlays
    pub mode: Mode,
    pub input: String,
    pub input_cursor: usize,
    pub status: Option<StatusMessage>,
    pub fuzzy_matches: Vec<FuzzyMatch>,
    pub fuzzy_cursor: usize,
    pub palette_matches: Vec<FuzzyMatch>,
    pub palette_cursor: usize,
    pub bookmarks_view: Vec<(String, String)>,
    pub bookmarks_cursor: usize,
    pub bookmarks_adding: bool,
    pub sort_cursor: usize,
    pub help_scroll: u16,
    /// Which button is highlighted in the confirm-delete modal.
    pub confirm_yes: bool,
    /// Targets snapshotted when the confirm-delete modal opened. The entry
    /// list can be refreshed underneath the modal by a finishing background
    /// task; acting on a live cursor index then would delete the wrong file.
    pub confirm_targets: Vec<PathBuf>,
    /// Path snapshotted when the rename prompt opened, for the same reason.
    pub rename_target: Option<PathBuf>,

    // -- Misc
    pub show_hidden: bool,
    pub quit: bool,
    pub picker: Option<Picker>,
    pub git: Option<GitInfo>,
    pub diff_mode: bool,
    pub worker: Worker,
    pub next_task_id: u64,
    /// Visible rows in a file panel body, updated during draw; used for
    /// page-up/page-down.
    pub panel_rows: usize,
    /// Set after returning from an external program (editor, pager) that took
    /// over the terminal. The event loop clears ratatui's diff buffer before
    /// the next draw so the whole UI repaints, not just the diff against a
    /// stale cached frame.
    pub needs_redraw: bool,
}

impl App {
    pub fn new(start: PathBuf, config: Config, picker: Option<Picker>) -> Result<Self> {
        let mut palette = Palette::from_theme(&config.theme);
        // Transparent mode: no pane paints a background, the terminal's own
        // (possibly translucent) background shows through everywhere. The
        // cursor-row highlight and modal backgrounds are kept for
        // readability.
        if config.transparent_background {
            use ratatui::style::Color;
            palette.file_panel_bg = Color::Reset;
            palette.sidebar_bg = Color::Reset;
            palette.footer_bg = Color::Reset;
        }
        let show_hidden = config.show_hidden;
        let worker = Worker::spawn();
        let pinned = Config::load_pinned();
        let sidebar = Sidebar::build(&pinned);
        let preview_visible = config.default_open_file_preview;
        let footer_visible = config.show_footer;
        let mut app = Self {
            palette,
            show_hidden,
            config,
            panels: vec![FilePanel::new(start)],
            active_panel: 0,
            focus: Focus::FilePanel,
            sidebar,
            pinned,
            preview: Preview::Empty,
            preview_scroll: 0,
            preview_pending: None,
            pdf_state: None,
            preview_visible,
            preview_debounce: None,
            footer_visible,
            processes: Vec::new(),
            proc_scroll: 0,
            metadata: Vec::new(),
            meta_scroll: 0,
            clipboard: Vec::new(),
            clip_mode: ClipMode::Copy,
            mode: Mode::Normal,
            input: String::new(),
            input_cursor: 0,
            status: None,
            fuzzy_matches: Vec::new(),
            fuzzy_cursor: 0,
            palette_matches: Vec::new(),
            palette_cursor: 0,
            bookmarks_view: Vec::new(),
            bookmarks_cursor: 0,
            bookmarks_adding: false,
            sort_cursor: 0,
            help_scroll: 0,
            confirm_yes: false,
            confirm_targets: Vec::new(),
            rename_target: None,
            quit: false,
            picker,
            git: None,
            diff_mode: false,
            worker,
            next_task_id: 1,
            panel_rows: 20,
            needs_redraw: false,
        };
        app.refresh()?;
        Ok(app)
    }

    // ------------------------------------------------------------------
    // Panel access

    pub fn panel(&self) -> &FilePanel {
        &self.panels[self.active_panel]
    }

    pub fn panel_mut(&mut self) -> &mut FilePanel {
        let i = self.active_panel;
        &mut self.panels[i]
    }

    pub fn cwd(&self) -> PathBuf {
        self.panel().cwd.clone()
    }

    pub fn current_entry(&self) -> Option<&Entry> {
        self.panel().current_entry()
    }

    // ------------------------------------------------------------------
    // Refresh / reload

    pub fn refresh(&mut self) -> Result<()> {
        let (show_hidden, sort, rev, dirs_first) = (
            self.show_hidden,
            self.config.sort,
            self.config.sort_reverse,
            self.config.dirs_first,
        );
        for p in &mut self.panels {
            p.refresh(show_hidden, sort, rev, dirs_first);
        }
        if self.config.git_integration {
            self.reload_git();
        } else {
            self.git = None;
        }
        self.schedule_preview_update();
        Ok(())
    }

    /// Regenerate preview + metadata once the cursor rests (see
    /// `preview_debounce`). The pending update fires from `tick()`.
    fn schedule_preview_update(&mut self) {
        self.preview_debounce = Some(Instant::now());
    }

    /// Refresh panel listings and git state WITHOUT touching the preview.
    /// Used after `!`/`z g` commands whose captured output is being shown
    /// in the preview pane — a full refresh would clobber it.
    fn refresh_entries_only(&mut self) {
        let (show_hidden, sort, rev, dirs_first) = (
            self.show_hidden,
            self.config.sort,
            self.config.sort_reverse,
            self.config.dirs_first,
        );
        for p in &mut self.panels {
            p.refresh(show_hidden, sort, rev, dirs_first);
        }
        if self.config.git_integration {
            self.reload_git();
        }
    }

    /// Put the cursor on `path` in the active panel if present.
    pub fn focus_file(&mut self, path: &Path) {
        if let Some(pos) = self.panel().entries.iter().position(|e| e.path == path) {
            let cwd = self.cwd();
            let panel = self.panel_mut();
            panel.cursor = pos;
            panel.cursor_memory.insert(cwd, pos);
            self.on_cursor_moved();
        }
    }

    fn reload_git(&mut self) {
        self.git = git::fetch(&self.panel().cwd);
    }

    pub fn reload_preview(&mut self) {
        self.preview_scroll = 0;
        self.pdf_state = None;
        if !self.preview_visible {
            self.preview = Preview::Empty;
            self.preview_pending = None;
            return;
        }
        let path = self.current_entry().map(|e| e.path.clone());
        let Some(p) = path else {
            self.preview = Preview::Empty;
            self.preview_pending = None;
            return;
        };
        let is_pdf = p
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("pdf"))
            .unwrap_or(false);
        if is_pdf {
            self.preview = Preview::Text(vec!["loading pdf…".into()]);
            self.preview_pending = Some((p, Instant::now()));
            return;
        }
        self.preview_pending = None;
        self.preview = preview::generate(
            &p,
            self.show_hidden,
            self.picker.as_mut(),
            self.git.as_ref(),
            self.diff_mode,
            &self.config.theme.code_syntax_highlight,
        );
    }

    pub fn update_metadata(&mut self) {
        self.meta_scroll = 0;
        self.metadata = match self.current_entry() {
            Some(e) => {
                let mut md =
                    metadata::collect(e, &self.config.date_format, self.config.file_size_use_si);
                if let Some(info) = self.git.as_ref() {
                    if let Some(fs) = info.status.get(&e.path) {
                        md.push((
                            "Git".into(),
                            format!("{}{}", fs.index.label(), fs.worktree.label()),
                        ));
                    }
                    // Repo summary, previously shown in the old header bar.
                    let mut branch = info.branch.clone().unwrap_or_else(|| "HEAD".into());
                    if let Some(up) = info.upstream.as_deref() {
                        branch.push_str(&format!(" → {up}"));
                    }
                    if info.ahead > 0 {
                        branch.push_str(&format!(" ↑{}", info.ahead));
                    }
                    if info.behind > 0 {
                        branch.push_str(&format!(" ↓{}", info.behind));
                    }
                    md.push(("Branch".into(), branch));
                    let mut changes = Vec::new();
                    if info.staged > 0 {
                        changes.push(format!("●{} staged", info.staged));
                    }
                    if info.unstaged > 0 {
                        changes.push(format!("✚{} unstaged", info.unstaged));
                    }
                    if info.untracked > 0 {
                        changes.push(format!("?{} untracked", info.untracked));
                    }
                    if info.conflicts > 0 {
                        changes.push(format!("‼{} conflicts", info.conflicts));
                    }
                    if info.stash_count > 0 {
                        changes.push(format!("⚑{} stashed", info.stash_count));
                    }
                    if !changes.is_empty() {
                        md.push(("Changes".into(), changes.join("  ")));
                    }
                }
                md
            }
            None => Vec::new(),
        };
    }

    /// Called on cursor movement: preview + metadata follow the cursor
    /// (debounced, so holding a movement key stays smooth).
    pub fn on_cursor_moved(&mut self) {
        self.schedule_preview_update();
    }

    pub fn tick(&mut self) {
        // Debounced preview/metadata regeneration after cursor movement.
        if let Some(at) = self.preview_debounce {
            if at.elapsed() >= Duration::from_millis(80) {
                self.preview_debounce = None;
                self.reload_preview();
                self.update_metadata();
            }
        }

        let Some((path, at)) = self.preview_pending.clone() else {
            return;
        };
        if at.elapsed() < Duration::from_millis(150) {
            return;
        }
        if self.current_entry().map(|e| e.path != path).unwrap_or(true) {
            self.preview_pending = None;
            return;
        }
        let total = preview::pdf_page_count(&path).max(1);
        self.preview = preview::render_pdf_page(&path, 1, self.picker.as_mut());
        self.pdf_state = Some(PdfState {
            path,
            page: 1,
            total,
        });
        self.preview_pending = None;
    }

    // ------------------------------------------------------------------
    // Navigation

    pub fn move_cursor(&mut self, delta: i64) {
        self.panel_mut().move_cursor(delta);
        self.on_cursor_moved();
    }

    pub fn goto_top(&mut self) {
        self.panel_mut().goto_top();
        self.on_cursor_moved();
    }

    pub fn goto_bottom(&mut self) {
        self.panel_mut().goto_bottom();
        self.on_cursor_moved();
    }

    pub fn page_move(&mut self, dir: i64) {
        let page = self.panel_rows.max(1) as i64;
        self.move_cursor(dir * page);
    }

    pub fn enter(&mut self, interactive: bool) -> Result<()> {
        let Some(entry) = self.current_entry().cloned() else {
            return Ok(());
        };
        if entry.is_dir {
            self.set_panel_cwd(entry.path)?;
        } else {
            self.open_current(interactive)?;
        }
        Ok(())
    }

    pub fn set_panel_cwd(&mut self, path: PathBuf) -> Result<()> {
        {
            let panel = self.panel_mut();
            panel.cwd = path;
            panel.cursor = 0;
            panel.offset = 0;
            panel.search.clear();
            panel.selected.clear();
            panel.mode = PanelMode::Browser;
        }
        self.refresh()
    }

    pub fn go_up(&mut self) -> Result<()> {
        let old = self.cwd();
        if let Some(parent) = old.parent().map(PathBuf::from) {
            self.set_panel_cwd(parent)?;
            let pos = self.panel().entries.iter().position(|e| e.path == old);
            if let Some(pos) = pos {
                let cwd = self.cwd();
                let panel = self.panel_mut();
                panel.cursor = pos;
                panel.cursor_memory.insert(cwd, pos);
                self.on_cursor_moved();
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // File panels (n new, w close, tab/L next, H prev)

    pub fn new_panel(&mut self) {
        if self.panels.len() >= MAX_PANELS {
            self.set_status(format!("max {MAX_PANELS} panels"), true);
            return;
        }
        let cwd = self.cwd();
        self.panels
            .insert(self.active_panel + 1, FilePanel::new(cwd));
        self.active_panel += 1;
        let _ = self.refresh();
    }

    pub fn close_panel(&mut self) {
        if self.panels.len() <= 1 {
            self.set_status("cannot close the last panel".into(), true);
            return;
        }
        self.panels.remove(self.active_panel);
        if self.active_panel >= self.panels.len() {
            self.active_panel = self.panels.len() - 1;
        }
        let _ = self.refresh();
    }

    pub fn next_panel(&mut self) {
        self.active_panel = (self.active_panel + 1) % self.panels.len();
        self.reload_git_only();
        self.on_cursor_moved();
    }

    pub fn prev_panel(&mut self) {
        self.active_panel = (self.active_panel + self.panels.len() - 1) % self.panels.len();
        self.reload_git_only();
        self.on_cursor_moved();
    }

    fn reload_git_only(&mut self) {
        if self.config.git_integration {
            self.reload_git();
        }
    }

    // ------------------------------------------------------------------
    // Sidebar

    pub fn sidebar_open_selected(&mut self) -> Result<()> {
        let Some((_, path)) = self.sidebar.current() else {
            return Ok(());
        };
        let path = path.clone();
        if !path.is_dir() {
            self.set_status(format!("not a directory: {}", path.display()), true);
            return Ok(());
        }
        self.focus = Focus::FilePanel;
        self.set_panel_cwd(path)
    }

    /// `P`: pin or unpin the current working directory.
    pub fn toggle_pin(&mut self) {
        let stored = path_for_storage(&self.cwd());
        if let Some(pos) = self.pinned.iter().position(|p| p == &stored) {
            self.pinned.remove(pos);
            self.set_status(format!("unpinned {stored}"), false);
        } else {
            self.pinned.push(stored.clone());
            self.set_status(format!("pinned {stored}"), false);
        }
        if let Err(e) = Config::save_pinned(&self.pinned) {
            self.set_status(format!("pin save failed: {e}"), true);
        }
        self.sidebar = Sidebar::build(&self.pinned);
    }

    // ------------------------------------------------------------------
    // Preview

    pub fn scroll_preview(&mut self, delta: i32) {
        if let Some(st) = self.pdf_state.as_mut() {
            let new = (st.page as i32 + delta).clamp(1, st.total as i32) as u32;
            if new == st.page {
                return;
            }
            st.page = new;
            let path = st.path.clone();
            self.preview = preview::render_pdf_page(&path, new, self.picker.as_mut());
            self.set_status(
                format!(
                    "pdf page {}/{}",
                    new,
                    self.pdf_state.as_ref().unwrap().total
                ),
                false,
            );
            return;
        }
        let max = self.preview_len().saturating_sub(1) as i32;
        let new = (self.preview_scroll as i32 + delta).clamp(0, max.max(0)) as u16;
        self.preview_scroll = new;
    }

    /// Line count of the current preview content, for scroll clamping.
    fn preview_len(&self) -> usize {
        match &self.preview {
            Preview::Text(lines) => lines.len(),
            Preview::Code(lines) => lines.len(),
            Preview::Dir(entries) => entries.len(),
            Preview::Diff(lines) => lines.len(),
            _ => 0,
        }
    }

    pub fn toggle_preview(&mut self) {
        self.preview_visible = !self.preview_visible;
        self.reload_preview();
    }

    pub fn toggle_footer(&mut self) {
        self.footer_visible = !self.footer_visible;
        // Never leave keyboard focus inside a pane that is no longer drawn.
        if !self.footer_visible && matches!(self.focus, Focus::Processes | Focus::Metadata) {
            self.focus = Focus::FilePanel;
        }
    }

    pub fn toggle_diff_mode(&mut self) {
        self.diff_mode = !self.diff_mode;
        self.reload_preview();
        let msg = if self.diff_mode {
            "diff preview on"
        } else {
            "diff preview off"
        };
        self.set_status(msg.into(), false);
    }

    // ------------------------------------------------------------------
    // Selection (select mode)

    pub fn toggle_select_mode(&mut self) {
        let panel = self.panel_mut();
        panel.mode = match panel.mode {
            PanelMode::Browser => PanelMode::Select,
            PanelMode::Select => {
                panel.selected.clear();
                PanelMode::Browser
            }
        };
    }

    pub fn select_toggle_current(&mut self) {
        self.panel_mut().toggle_select_current();
    }

    /// Shift+↑/↓ (or J/K) in select mode: select while moving.
    pub fn select_move(&mut self, delta: i64) {
        {
            let panel = self.panel_mut();
            if let Some(e) = panel.current_entry().cloned() {
                panel.selected.insert(e.path);
            }
            panel.move_cursor(delta);
            if let Some(e) = panel.current_entry().cloned() {
                panel.selected.insert(e.path);
            }
        }
        self.on_cursor_moved();
    }

    pub fn select_all(&mut self) {
        self.panel_mut().select_all();
    }

    pub fn clear_selection(&mut self) {
        self.panel_mut().selected.clear();
    }

    pub fn toggle_hidden(&mut self) -> Result<()> {
        self.show_hidden = !self.show_hidden;
        self.refresh()
    }

    // ------------------------------------------------------------------
    // Clipboard (copy / cut / paste)

    pub fn yank(&mut self) {
        if !self.stash_clipboard(ClipMode::Copy) {
            self.set_status("nothing to copy".into(), true);
            return;
        }
        let n = self.clipboard.len();
        let sys = self.push_clipboard_files();
        self.set_status(format!("copied {n} item(s){sys}"), false);
    }

    pub fn cut(&mut self) {
        if !self.stash_clipboard(ClipMode::Cut) {
            self.set_status("nothing to cut".into(), true);
            return;
        }
        let n = self.clipboard.len();
        let sys = self.push_clipboard_files();
        self.set_status(format!("cut {n} item(s){sys}"), false);
    }

    /// Returns false when there was nothing to stash (empty dir, no cursor).
    fn stash_clipboard(&mut self, mode: ClipMode) -> bool {
        let items = self.panel().targets();
        if items.is_empty() {
            return false;
        }
        self.clipboard = items;
        self.clip_mode = mode;
        let panel = self.panel_mut();
        panel.selected.clear();
        panel.mode = PanelMode::Browser;
        true
    }

    /// Best-effort push of the current internal clipboard to the OS clipboard
    /// as a `text/uri-list` payload. Returns a short status suffix such as
    /// " → wl-copy" on success, or " (no clipboard tool)" on failure.
    fn push_clipboard_files(&self) -> String {
        if self.clipboard.is_empty() {
            return String::new();
        }
        match clipboard::copy_files(&self.clipboard) {
            Ok(tool) => format!(" → {tool}"),
            Err(e) => format!(" ({e})"),
        }
    }

    pub fn paste(&mut self) -> Result<()> {
        if self.clipboard.is_empty() {
            self.set_status("clipboard empty".into(), true);
            return Ok(());
        }
        let sources = self.clipboard.clone();
        let id = self.next_id();
        let n = sources.len();
        let (task, verb) = match self.clip_mode {
            ClipMode::Copy => (
                Task::Copy {
                    id,
                    sources,
                    dest_dir: self.cwd(),
                },
                "Copy",
            ),
            ClipMode::Cut => (
                Task::Move {
                    id,
                    sources,
                    dest_dir: self.cwd(),
                },
                "Move",
            ),
        };
        self.push_process(id, format!("{verb} {}", count_label(n)), n as u64);
        self.worker.submit(task);
        if self.clip_mode == ClipMode::Cut {
            self.clipboard.clear();
        }
        Ok(())
    }

    /// Open the confirm-delete modal, snapshotting the targets so a
    /// background refresh under the modal can't shift what gets deleted.
    pub fn open_confirm_delete(&mut self, permanent: bool) {
        let targets = self.panel().targets();
        if targets.is_empty() {
            self.set_status("nothing to delete".into(), true);
            return;
        }
        self.confirm_targets = targets;
        self.confirm_yes = false;
        self.mode = Mode::ConfirmDelete { permanent };
    }

    /// Quit, or ask for confirmation first when background tasks are still
    /// running (quitting would kill a half-finished copy/move/delete).
    pub fn request_quit(&mut self) {
        let busy = self
            .processes
            .iter()
            .any(|p| p.state == ProcessState::Running);
        if busy {
            self.confirm_yes = false;
            self.mode = Mode::ConfirmQuit;
        } else {
            self.quit = true;
        }
    }

    pub fn delete_current(&mut self, permanent: bool) -> Result<()> {
        // Prefer the snapshot taken when the confirm modal opened.
        let targets = if self.confirm_targets.is_empty() {
            self.panel().targets()
        } else {
            std::mem::take(&mut self.confirm_targets)
        };
        if targets.is_empty() {
            return Ok(());
        }
        {
            let panel = self.panel_mut();
            panel.selected.clear();
            panel.mode = PanelMode::Browser;
        }
        let use_trash = self.config.use_trash && !permanent;
        let id = self.next_id();
        let n = targets.len();
        let verb = if use_trash { "Trash" } else { "Delete" };
        self.push_process(id, format!("{verb} {}", count_label(n)), n as u64);
        self.worker.submit(Task::Delete {
            id,
            targets,
            use_trash,
        });
        Ok(())
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_task_id;
        self.next_task_id += 1;
        id
    }

    fn push_process(&mut self, id: u64, label: String, total: u64) {
        self.processes.insert(
            0,
            Process {
                id,
                label,
                state: ProcessState::Running,
                done: 0,
                total,
                current: String::new(),
                started: Instant::now(),
            },
        );
        self.processes.truncate(MAX_PROCESSES);
        self.proc_scroll = 0;
    }

    pub fn drain_task_messages(&mut self) {
        while let Ok(msg) = self.worker.msg_rx.try_recv() {
            match msg {
                TaskMsg::Start { id, total, .. } => {
                    if let Some(p) = self.processes.iter_mut().find(|p| p.id == id) {
                        p.total = total.max(1);
                    }
                }
                TaskMsg::Progress { id, done, current } => {
                    if let Some(p) = self.processes.iter_mut().find(|p| p.id == id) {
                        p.done = done;
                        p.current = current;
                    }
                }
                TaskMsg::Done {
                    id,
                    ok,
                    errs,
                    first_error,
                } => {
                    let mut label = String::from("task");
                    if let Some(p) = self.processes.iter_mut().find(|p| p.id == id) {
                        p.done = p.total;
                        p.state = if errs == 0 {
                            ProcessState::Successful
                        } else {
                            ProcessState::Failed
                        };
                        p.current.clear();
                        label = p.label.clone();
                    }
                    let _ = self.refresh();
                    // The refresh may have changed panel entries while the
                    // fuzzy overlay is open — rebuild its matches so stored
                    // indices never point past the new entry list.
                    if self.mode == Mode::Fuzzy {
                        self.update_fuzzy();
                    }
                    if errs > 0 {
                        let mut text = format!("{label}: {ok} ok, {errs} failed");
                        if let Some(err) = first_error {
                            text.push_str(" — ");
                            let trimmed: String = err.chars().take(120).collect();
                            text.push_str(&trimmed);
                        }
                        self.set_status(text, true);
                    } else {
                        self.set_status(format!("{label}: done"), false);
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Create / rename / goto

    /// Open the rename prompt for the entry under the cursor, snapshotting
    /// its path so a background refresh can't shift the rename target.
    pub fn open_rename_prompt(&mut self) {
        let Some(entry) = self.current_entry().cloned() else {
            self.set_status("nothing to rename".into(), true);
            return;
        };
        self.rename_target = Some(entry.path.clone());
        self.input_set(entry.name);
        self.mode = Mode::Prompt(PromptKind::Rename);
    }

    pub fn rename_current(&mut self, new_name: &str) -> Result<()> {
        let src = match self.rename_target.take() {
            Some(p) => p,
            None => match self.current_entry() {
                Some(e) => e.path.clone(),
                None => return Ok(()),
            },
        };
        let dst = self.cwd().join(new_name);
        if dst == src {
            return Ok(());
        }
        if dst.exists() {
            self.set_status("target exists".into(), true);
            return Ok(());
        }
        match crate::fs_ops::move_path(&src, &dst) {
            Ok(inner) => {
                self.refresh()?;
                if inner.is_empty() {
                    self.set_status(format!("renamed → {new_name}"), false);
                } else {
                    let first = inner.into_iter().next().unwrap_or_default();
                    self.set_status(
                        format!("renamed → {new_name} (with errors — {first})"),
                        true,
                    );
                }
            }
            Err(e) => self.set_status(format!("rename failed: {e}"), true),
        }
        Ok(())
    }

    pub fn make_entry(&mut self, raw: &str) -> Result<()> {
        let is_dir = raw.ends_with('/');
        let name = raw.trim_end_matches('/');
        if name.is_empty() {
            return Ok(());
        }
        let dst = self.cwd().join(name);
        let result = if is_dir {
            crate::fs_ops::create_dir(&dst).map(|_| format!("mkdir {name}/"))
        } else {
            crate::fs_ops::create_file(&dst).map(|_| format!("touch {name}"))
        };
        match result {
            Ok(msg) => {
                self.refresh()?;
                self.set_status(msg, false);
            }
            Err(e) => {
                let verb = if is_dir { "mkdir" } else { "touch" };
                self.set_status(format!("{verb} failed: {e}"), true);
            }
        }
        Ok(())
    }

    pub fn goto_path(&mut self, raw: &str) -> Result<()> {
        let expanded = expand_tilde(raw);
        let path = Path::new(&expanded);
        if !path.exists() {
            self.set_status(format!("no such path: {raw}"), true);
            return Ok(());
        }
        let canon = path.canonicalize()?;
        if canon.is_dir() {
            self.set_panel_cwd(canon)?;
        } else if let Some(parent) = canon.parent().map(PathBuf::from) {
            self.set_panel_cwd(parent)?;
            let pos = self.panel().entries.iter().position(|e| e.path == canon);
            if let Some(pos) = pos {
                self.panel_mut().cursor = pos;
                self.on_cursor_moved();
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Open / editor

    pub fn open_current(&mut self, interactive: bool) -> Result<()> {
        let Some(entry) = self.current_entry().cloned() else {
            return Ok(());
        };
        if entry.is_dir {
            return Ok(());
        }
        let opener = Opener::new(&self.config.openers);
        let mode = if interactive {
            crate::opener::OpenMode::Interactive
        } else {
            crate::opener::OpenMode::Background
        };
        // Decide whether the open path will hijack the terminal *before*
        // calling open(). Background-spawned executables and OS-default
        // handoffs don't touch the TTY, so we can keep drawing the TUI.
        let take_tty = opener.will_take_tty(&entry.path, mode);
        let outcome = if take_tty {
            let _guard = TuiSuspend::new();
            opener.open(&entry.path, mode)
        } else {
            opener.open(&entry.path, mode)
        };
        match outcome {
            Ok(crate::opener::OpenOutcome::Internal) => {
                self.set_status(format!("opened {}", entry.name), false);
            }
            Ok(crate::opener::OpenOutcome::OsDefault) => {
                self.set_status(format!("opened {} (OS default)", entry.name), false);
            }
            Ok(crate::opener::OpenOutcome::BackgroundSpawned) => {
                self.set_status(format!("launched {} in background", entry.name), false);
            }
            Err(e) => self.set_status(format!("open failed: {e}"), true),
        }
        if take_tty {
            self.needs_redraw = true;
        }
        Ok(())
    }

    /// `e`: open the file under the cursor in the editor.
    pub fn open_with_editor(&mut self) -> Result<()> {
        let Some(entry) = self.current_entry().cloned() else {
            return Ok(());
        };
        let editor = self.config.editor_cmd();
        self.run_tty_command(&format!(
            "{editor} {}",
            shell_quote(&entry.path.display().to_string())
        ))
    }

    /// `E`: open the current directory in the editor.
    pub fn open_dir_with_editor(&mut self) -> Result<()> {
        let editor = self.config.editor_cmd();
        let cwd = self.cwd().display().to_string();
        self.run_tty_command(&format!("{editor} {}", shell_quote(&cwd)))
    }

    /// Run a command with the TUI suspended and the terminal inherited.
    fn run_tty_command(&mut self, cmd: &str) -> Result<()> {
        use std::process::{Command, Stdio};
        let cwd = self.cwd();
        let status = {
            let _guard = TuiSuspend::new();
            Command::new("sh")
                .arg("-c")
                .arg(cmd)
                .current_dir(&cwd)
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
        };
        self.needs_redraw = true;
        match status {
            Ok(s) if s.success() => {}
            Ok(s) => self.set_status(format!("exit {}: {cmd}", s.code().unwrap_or(-1)), true),
            Err(e) => self.set_status(format!("spawn failed: {e}"), true),
        }
        self.reload_git_only();
        self.refresh()
    }

    // ------------------------------------------------------------------
    // Copy path helpers

    pub fn copy_current_path(&mut self) {
        let Some(entry) = self.current_entry().cloned() else {
            return;
        };
        let text = entry.path.display().to_string();
        match clipboard::copy_text(&text) {
            Ok(tool) => self.set_status(format!("copied path → {tool}"), false),
            Err(e) => self.set_status(format!("copy failed: {e}"), true),
        }
    }

    pub fn copy_current_name(&mut self) {
        let Some(entry) = self.current_entry().cloned() else {
            return;
        };
        match clipboard::copy_text(&entry.name) {
            Ok(tool) => self.set_status(format!("copied name → {tool}"), false),
            Err(e) => self.set_status(format!("copy failed: {e}"), true),
        }
    }

    pub fn copy_cwd(&mut self) {
        let text = self.cwd().display().to_string();
        match clipboard::copy_text(&text) {
            Ok(tool) => self.set_status(format!("copied cwd → {tool}"), false),
            Err(e) => self.set_status(format!("copy failed: {e}"), true),
        }
    }

    pub fn copy_selection_to_os_clipboard(&mut self) {
        let items = self.panel().targets();
        if items.is_empty() {
            return;
        }
        match clipboard::copy_files(&items) {
            Ok(tool) => self.set_status(format!("copied {} item(s) → {tool}", items.len()), false),
            Err(e) => self.set_status(format!("copy failed: {e}"), true),
        }
    }

    // ------------------------------------------------------------------
    // Git

    fn git_targets(&self) -> Vec<PathBuf> {
        self.panel().targets()
    }

    pub fn git_stage(&mut self) -> Result<()> {
        let Some(info) = self.git.as_ref() else {
            self.set_status("not in a git repo".into(), true);
            return Ok(());
        };
        let root = info.root.clone();
        let targets = self.git_targets();
        if targets.is_empty() {
            return Ok(());
        }
        match git::stage(&root, &targets) {
            Ok(_) => {
                self.set_status(format!("staged {}", targets.len()), false);
                self.refresh()?;
            }
            Err(e) => self.set_status(format!("stage failed: {e}"), true),
        }
        Ok(())
    }

    pub fn git_unstage(&mut self) -> Result<()> {
        let Some(info) = self.git.as_ref() else {
            self.set_status("not in a git repo".into(), true);
            return Ok(());
        };
        let root = info.root.clone();
        let targets = self.git_targets();
        if targets.is_empty() {
            return Ok(());
        }
        match git::unstage(&root, &targets) {
            Ok(_) => {
                self.set_status(format!("unstaged {}", targets.len()), false);
                self.refresh()?;
            }
            Err(e) => self.set_status(format!("unstage failed: {e}"), true),
        }
        Ok(())
    }

    pub fn git_discard(&mut self) -> Result<()> {
        let Some(info) = self.git.as_ref() else {
            self.set_status("not in a git repo".into(), true);
            return Ok(());
        };
        let root = info.root.clone();
        let targets = self.git_targets();
        if targets.is_empty() {
            return Ok(());
        }
        match git::discard(&root, &targets) {
            Ok(_) => {
                self.set_status(format!("discarded {}", targets.len()), false);
                self.refresh()?;
            }
            Err(e) => self.set_status(format!("discard failed: {e}"), true),
        }
        Ok(())
    }

    pub fn git_commit(&mut self, msg: &str) -> Result<()> {
        let Some(info) = self.git.as_ref() else {
            self.set_status("not in a git repo".into(), true);
            return Ok(());
        };
        let root = info.root.clone();
        match git::commit(&root, msg) {
            Ok(line) => {
                self.set_status(line, false);
                self.refresh()?;
            }
            Err(e) => self.set_status(format!("commit failed: {e}"), true),
        }
        Ok(())
    }

    pub fn run_git_cmd(&mut self, raw: &str) -> Result<()> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        let args: Vec<String> = shell_words(trimmed);
        if args.is_empty() {
            return Ok(());
        }
        let cwd = self
            .git
            .as_ref()
            .map(|g| g.root.clone())
            .unwrap_or_else(|| self.cwd());

        // Some git subcommands need a real terminal: `commit` without -m
        // launches the editor, `rebase -i` opens the todo list, `add -p`
        // is interactive, `log` invokes the pager, etc. Capturing them via
        // Command::output() would either hang on missing stdin or strip
        // the pager. Suspend the TUI for these the same way we do for `!`.
        if git_args_need_tty(&args) {
            use std::process::{Command, Stdio};
            let status = {
                let _guard = TuiSuspend::new();
                let mut cmd = Command::new("git");
                cmd.arg("-C").arg(&cwd);
                for a in &args {
                    cmd.arg(a);
                }
                let status = cmd
                    .stdin(Stdio::inherit())
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .status();
                pause_for_enter();
                status
            };
            self.needs_redraw = true;

            match status {
                Ok(s) if s.success() => self.set_status(format!("git {trimmed}"), false),
                Ok(s) => self.set_status(
                    format!("git exit {}: {trimmed}", s.code().unwrap_or(-1)),
                    true,
                ),
                Err(e) => self.set_status(format!("git spawn failed: {e}"), true),
            }
            self.reload_git_only();
            let _ = self.refresh();
            return Ok(());
        }

        match git::run_raw(&cwd, &args) {
            Ok(lines) => {
                let summary = format!("git {}", trimmed);
                self.preview = Preview::Text(if lines.is_empty() {
                    vec!["(no output)".into()]
                } else {
                    lines
                });
                self.set_status(summary, false);
                // git may have touched the worktree (checkout, stash, …).
                self.refresh_entries_only();
            }
            Err(e) => self.set_status(format!("git failed: {e}"), true),
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Shell

    pub fn run_shell_raw(&mut self, raw: &str) -> Result<()> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        self.run_shell_cmd(trimmed)
    }

    pub fn run_command_binding(&mut self, key: char) -> Result<()> {
        let tmpl = match self.config.commands.get(&key.to_string()).cloned() {
            Some(t) => t,
            None => {
                self.set_status(format!("no command bound to '{key}'"), true);
                return Ok(());
            }
        };
        let expanded = self.expand_command(&tmpl);
        self.run_shell_cmd(&expanded)
    }

    fn expand_command(&self, template: &str) -> String {
        let current = self.current_entry().map(|e| e.path.clone());
        let f = current
            .as_ref()
            .map(|p| shell_quote(&p.display().to_string()))
            .unwrap_or_default();
        let n = current
            .as_ref()
            .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
            .map(|s| shell_quote(&s))
            .unwrap_or_default();
        let d = shell_quote(&self.cwd().display().to_string());
        let targets = self.panel().targets();
        let s = if targets.is_empty() {
            f.clone()
        } else {
            targets
                .iter()
                .map(|p| shell_quote(&p.display().to_string()))
                .collect::<Vec<_>>()
                .join(" ")
        };
        template
            .replace("{f}", &f)
            .replace("{n}", &n)
            .replace("{d}", &d)
            .replace("{s}", &s)
    }

    fn run_shell_cmd(&mut self, cmd: &str) -> Result<()> {
        use std::process::{Command, Stdio};

        // Programs that need a real TTY (full-screen editors, pagers, TUI
        // apps, REPLs). Running these via Command::output() — which captures
        // stdout/stderr and gives the child a closed stdin — leaves them
        // wedged with no UI. We suspend the TUI and inherit the real
        // terminal for these commands.
        if cmd_is_interactive(cmd) {
            let status = {
                let _guard = TuiSuspend::new();
                let status = Command::new("sh")
                    .arg("-c")
                    .arg(cmd)
                    .current_dir(self.cwd())
                    .stdin(Stdio::inherit())
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .status();
                pause_for_enter();
                status
            };
            self.needs_redraw = true;

            match status {
                Ok(s) if s.success() => self.set_status(format!("ran: {cmd}"), false),
                Ok(s) => self.set_status(format!("exit {}: {cmd}", s.code().unwrap_or(-1)), true),
                Err(e) => self.set_status(format!("spawn failed: {e}"), true),
            }
            self.reload_git_only();
            let _ = self.refresh();
            return Ok(());
        }

        let out = Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .current_dir(self.cwd())
            .output();

        match out {
            Ok(o) => {
                let mut lines: Vec<String> = Vec::new();
                lines.push(format!("$ {cmd}"));
                lines.push(String::new());
                for l in String::from_utf8_lossy(&o.stdout).lines() {
                    lines.push(l.to_string());
                }
                let stderr = String::from_utf8_lossy(&o.stderr);
                if !stderr.trim().is_empty() {
                    if lines.last().map(|l| !l.is_empty()).unwrap_or(false) {
                        lines.push(String::new());
                    }
                    for l in stderr.lines() {
                        lines.push(l.to_string());
                    }
                }
                if lines.len() == 2 {
                    lines.push("(no output)".into());
                }
                let ok = o.status.success();
                let code = o.status.code().unwrap_or(-1);
                self.preview = Preview::Text(lines);
                let label = if ok {
                    format!("ran: {cmd}")
                } else {
                    format!("exit {code}: {cmd}")
                };
                self.set_status(label, !ok);
                // The command may have created/removed files — refresh the
                // listings, but keep the captured output in the preview.
                self.refresh_entries_only();
            }
            Err(e) => {
                self.set_status(format!("spawn failed: {e}"), true);
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Search bar (`/`)

    pub fn open_search(&mut self) {
        let existing = self.panel().search.clone();
        self.input_set(existing);
        self.panel_mut().searching = true;
        self.mode = Mode::Search;
    }

    pub fn apply_search_live(&mut self) -> Result<()> {
        let q = self.input.clone();
        {
            let panel = self.panel_mut();
            panel.search = q;
            panel.cursor = 0;
            panel.offset = 0;
        }
        let (show_hidden, sort, rev, dirs_first) = (
            self.show_hidden,
            self.config.sort,
            self.config.sort_reverse,
            self.config.dirs_first,
        );
        self.panel_mut().refresh(show_hidden, sort, rev, dirs_first);
        self.on_cursor_moved();
        Ok(())
    }

    pub fn close_search(&mut self, keep: bool) -> Result<()> {
        self.panel_mut().searching = false;
        self.mode = Mode::Normal;
        if !keep {
            self.input_clear();
            self.panel_mut().search.clear();
            let (show_hidden, sort, rev, dirs_first) = (
                self.show_hidden,
                self.config.sort,
                self.config.sort_reverse,
                self.config.dirs_first,
            );
            self.panel_mut().refresh(show_hidden, sort, rev, dirs_first);
            self.on_cursor_moved();
        } else {
            self.input_clear();
        }
        Ok(())
    }

    /// Esc in normal mode: clear search filter first, then selection.
    pub fn escape_pressed(&mut self) -> Result<bool> {
        if !self.panel().search.is_empty() {
            self.panel_mut().search.clear();
            let (show_hidden, sort, rev, dirs_first) = (
                self.show_hidden,
                self.config.sort,
                self.config.sort_reverse,
                self.config.dirs_first,
            );
            self.panel_mut().refresh(show_hidden, sort, rev, dirs_first);
            self.on_cursor_moved();
            return Ok(true);
        }
        if self.panel().mode == PanelMode::Select {
            self.toggle_select_mode();
            return Ok(true);
        }
        if !self.panel().selected.is_empty() {
            self.clear_selection();
            return Ok(true);
        }
        if self.focus != Focus::FilePanel {
            self.focus = Focus::FilePanel;
            return Ok(true);
        }
        Ok(false)
    }

    // ------------------------------------------------------------------
    // Sort menu

    pub fn open_sort_menu(&mut self) {
        self.sort_cursor = match self.config.sort {
            SortMode::Name => 0,
            SortMode::Size => 1,
            SortMode::Mtime => 2,
            SortMode::Ext => 3,
        };
        self.mode = Mode::SortMenu;
    }

    pub fn set_sort(&mut self, mode: SortMode) -> Result<()> {
        self.config.sort = mode;
        self.refresh()
    }

    pub fn toggle_sort_reverse(&mut self) -> Result<()> {
        self.config.sort_reverse = !self.config.sort_reverse;
        self.refresh()
    }

    // ------------------------------------------------------------------
    // Fuzzy finder

    pub fn open_fuzzy(&mut self) {
        self.input_clear();
        self.fuzzy_cursor = 0;
        self.update_fuzzy();
        self.mode = Mode::Fuzzy;
    }

    pub fn update_fuzzy(&mut self) {
        self.fuzzy_matches.clear();
        let entries = &self.panels[self.active_panel].entries;
        if self.input.is_empty() {
            for (i, _) in entries.iter().enumerate() {
                self.fuzzy_matches.push(FuzzyMatch {
                    index: i,
                    score: 0,
                    match_positions: Vec::new(),
                });
            }
        } else {
            for (i, e) in entries.iter().enumerate() {
                if let Some((score, positions)) = fuzzy::score(&self.input, &e.name) {
                    self.fuzzy_matches.push(FuzzyMatch {
                        index: i,
                        score,
                        match_positions: positions,
                    });
                }
            }
            self.fuzzy_matches
                .sort_by_key(|m| std::cmp::Reverse(m.score));
        }
        if self.fuzzy_cursor >= self.fuzzy_matches.len() {
            self.fuzzy_cursor = self.fuzzy_matches.len().saturating_sub(1);
        }
    }

    pub fn fuzzy_move(&mut self, delta: i64) {
        if self.fuzzy_matches.is_empty() {
            return;
        }
        let len = self.fuzzy_matches.len() as i64;
        let new = (self.fuzzy_cursor as i64 + delta).clamp(0, len - 1) as usize;
        self.fuzzy_cursor = new;
    }

    pub fn accept_fuzzy(&mut self, selection: usize) {
        if let Some(m) = self.fuzzy_matches.get(selection) {
            let idx = m.index;
            let cwd = self.cwd();
            let panel = self.panel_mut();
            panel.cursor = idx;
            panel.cursor_memory.insert(cwd, idx);
            self.on_cursor_moved();
        }
    }

    // ------------------------------------------------------------------
    // Command palette

    pub fn open_palette(&mut self) {
        self.input_clear();
        self.palette_cursor = 0;
        self.update_palette();
        self.mode = Mode::Palette;
    }

    pub fn update_palette(&mut self) {
        self.palette_matches.clear();
        if self.input.is_empty() {
            for (i, _) in PALETTE_ENTRIES.iter().enumerate() {
                self.palette_matches.push(FuzzyMatch {
                    index: i,
                    score: 0,
                    match_positions: Vec::new(),
                });
            }
        } else {
            for (i, e) in PALETTE_ENTRIES.iter().enumerate() {
                if let Some((score, positions)) = fuzzy::score(&self.input, e.label) {
                    self.palette_matches.push(FuzzyMatch {
                        index: i,
                        score,
                        match_positions: positions,
                    });
                }
            }
            self.palette_matches
                .sort_by_key(|m| std::cmp::Reverse(m.score));
        }
        if self.palette_cursor >= self.palette_matches.len() {
            self.palette_cursor = self.palette_matches.len().saturating_sub(1);
        }
    }

    pub fn palette_move(&mut self, delta: i64) {
        if self.palette_matches.is_empty() {
            return;
        }
        let len = self.palette_matches.len() as i64;
        let new = (self.palette_cursor as i64 + delta).clamp(0, len - 1) as usize;
        self.palette_cursor = new;
    }

    pub fn accept_palette(&mut self) -> Result<Option<PromptKind>> {
        let Some(m) = self.palette_matches.get(self.palette_cursor) else {
            return Ok(None);
        };
        let Some(entry) = PALETTE_ENTRIES.get(m.index) else {
            return Ok(None);
        };
        let action = entry.action;
        self.mode = Mode::Normal;
        self.input_clear();
        self.palette_matches.clear();
        self.palette_cursor = 0;
        self.run_palette_action(action)
    }

    fn run_palette_action(&mut self, action: PaletteAction) -> Result<Option<PromptKind>> {
        match action {
            PaletteAction::Yank => self.yank(),
            PaletteAction::Cut => self.cut(),
            PaletteAction::Paste => self.paste()?,
            PaletteAction::Delete => {
                if self.config.confirm_delete {
                    self.open_confirm_delete(false);
                } else {
                    self.delete_current(false)?;
                }
            }
            PaletteAction::PermanentDelete => {
                self.open_confirm_delete(true);
            }
            PaletteAction::Rename => {
                self.open_rename_prompt();
                return Ok(Some(PromptKind::Rename));
            }
            PaletteAction::NewEntry => {
                self.input_clear();
                self.mode = Mode::Prompt(PromptKind::New);
                return Ok(Some(PromptKind::New));
            }
            PaletteAction::NewPanel => self.new_panel(),
            PaletteAction::ClosePanel => self.close_panel(),
            PaletteAction::TogglePreview => self.toggle_preview(),
            PaletteAction::ToggleFooter => self.toggle_footer(),
            PaletteAction::PinDirectory => self.toggle_pin(),
            PaletteAction::OpenWithEditor => self.open_with_editor()?,
            PaletteAction::OpenDirWithEditor => self.open_dir_with_editor()?,
            PaletteAction::CopyPath => self.copy_current_path(),
            PaletteAction::CopyName => self.copy_current_name(),
            PaletteAction::CopyDir => self.copy_cwd(),
            PaletteAction::CopyFilesToClipboard => self.copy_selection_to_os_clipboard(),
            PaletteAction::ToggleHidden => self.toggle_hidden()?,
            PaletteAction::ToggleDiff => self.toggle_diff_mode(),
            PaletteAction::Refresh => self.refresh()?,
            PaletteAction::SelectAll => self.select_all(),
            PaletteAction::ClearSelection => self.clear_selection(),
            PaletteAction::SortName => self.set_sort(SortMode::Name)?,
            PaletteAction::SortSize => self.set_sort(SortMode::Size)?,
            PaletteAction::SortMtime => self.set_sort(SortMode::Mtime)?,
            PaletteAction::SortExt => self.set_sort(SortMode::Ext)?,
            PaletteAction::SortReverse => self.toggle_sort_reverse()?,
            PaletteAction::GitStage => self.git_stage()?,
            PaletteAction::GitUnstage => self.git_unstage()?,
            PaletteAction::GitDiscard => self.git_discard()?,
            PaletteAction::GitCommit => {
                self.input_clear();
                self.mode = Mode::Prompt(PromptKind::CommitMsg);
                return Ok(Some(PromptKind::CommitMsg));
            }
            PaletteAction::GitDiff => self.toggle_diff_mode(),
            PaletteAction::GitRefresh => self.refresh()?,
            PaletteAction::GitRaw => {
                self.input_clear();
                self.mode = Mode::Prompt(PromptKind::GitCmd);
                return Ok(Some(PromptKind::GitCmd));
            }
            PaletteAction::GoTo => {
                self.input_clear();
                self.mode = Mode::Prompt(PromptKind::GoTo);
                return Ok(Some(PromptKind::GoTo));
            }
            PaletteAction::Bookmark => {
                self.open_bookmarks();
            }
            PaletteAction::Shell => {
                self.input_clear();
                self.mode = Mode::Prompt(PromptKind::Shell);
                return Ok(Some(PromptKind::Shell));
            }
            PaletteAction::Help => {
                self.help_scroll = 0;
                self.mode = Mode::Help;
            }
            PaletteAction::Quit => self.request_quit(),
        }
        Ok(None)
    }

    // ------------------------------------------------------------------
    // Bookmarks

    pub fn jump_bookmark(&mut self, key: &str) -> Result<()> {
        if let Some(path) = self.config.bookmarks.get(key).cloned() {
            self.goto_path(&path)?;
        } else {
            self.set_status(format!("no bookmark '{key}'"), true);
        }
        Ok(())
    }

    pub fn open_bookmarks(&mut self) {
        self.bookmarks_refresh_view();
        self.bookmarks_cursor = 0;
        self.bookmarks_adding = false;
        self.mode = Mode::Bookmarks;
    }

    fn bookmarks_refresh_view(&mut self) {
        let mut v: Vec<(String, String)> = self
            .config
            .bookmarks
            .iter()
            .map(|(k, val)| (k.clone(), val.clone()))
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        self.bookmarks_view = v;
        if self.bookmarks_cursor >= self.bookmarks_view.len() {
            self.bookmarks_cursor = self.bookmarks_view.len().saturating_sub(1);
        }
    }

    pub fn bookmarks_move(&mut self, delta: i64) {
        if self.bookmarks_view.is_empty() {
            return;
        }
        let len = self.bookmarks_view.len() as i64;
        let new = (self.bookmarks_cursor as i64 + delta).clamp(0, len - 1) as usize;
        self.bookmarks_cursor = new;
    }

    pub fn bookmarks_accept(&mut self) -> Result<()> {
        let Some((_key, path)) = self.bookmarks_view.get(self.bookmarks_cursor).cloned() else {
            return Ok(());
        };
        self.mode = Mode::Normal;
        self.bookmarks_adding = false;
        self.goto_path(&path)
    }

    pub fn bookmarks_start_add(&mut self) {
        self.bookmarks_adding = true;
    }

    pub fn bookmarks_finish_add(&mut self, key: char) -> Result<()> {
        self.bookmarks_adding = false;
        if !is_valid_bookmark_key(key) {
            self.set_status(format!("invalid bookmark key: {key:?}"), true);
            return Ok(());
        }
        let key_str = key.to_string();
        let path = path_for_storage(&self.cwd());
        let overwrite = self.config.bookmarks.contains_key(&key_str);
        self.config.bookmarks.insert(key_str.clone(), path.clone());
        match Config::save_bookmarks(&self.config.bookmarks) {
            Ok(_) => {
                let verb = if overwrite { "updated" } else { "added" };
                self.set_status(format!("{verb} bookmark '{key_str}' → {path}"), false);
            }
            Err(e) => {
                self.set_status(format!("save failed: {e}"), true);
            }
        }
        self.bookmarks_refresh_view();
        if let Some(pos) = self.bookmarks_view.iter().position(|(k, _)| k == &key_str) {
            self.bookmarks_cursor = pos;
        }
        Ok(())
    }

    pub fn bookmarks_delete_current(&mut self) -> Result<()> {
        let Some((key, _)) = self.bookmarks_view.get(self.bookmarks_cursor).cloned() else {
            return Ok(());
        };
        self.config.bookmarks.remove(&key);
        match Config::save_bookmarks(&self.config.bookmarks) {
            Ok(_) => self.set_status(format!("removed bookmark '{key}'"), false),
            Err(e) => self.set_status(format!("save failed: {e}"), true),
        }
        self.bookmarks_refresh_view();
        Ok(())
    }

    pub fn bookmarks_close(&mut self) {
        self.mode = Mode::Normal;
        self.bookmarks_adding = false;
    }

    // ------------------------------------------------------------------
    // Input line editing

    pub fn input_clear(&mut self) {
        self.input.clear();
        self.input_cursor = 0;
    }

    pub fn input_set(&mut self, s: String) {
        self.input_cursor = s.len();
        self.input = s;
    }

    pub fn input_take(&mut self) -> String {
        self.input_cursor = 0;
        std::mem::take(&mut self.input)
    }

    pub fn input_insert(&mut self, c: char) {
        self.input.insert(self.input_cursor, c);
        self.input_cursor += c.len_utf8();
    }

    pub fn input_backspace(&mut self) {
        if self.input_cursor == 0 {
            return;
        }
        let mut new_cursor = self.input_cursor - 1;
        while new_cursor > 0 && !self.input.is_char_boundary(new_cursor) {
            new_cursor -= 1;
        }
        self.input.replace_range(new_cursor..self.input_cursor, "");
        self.input_cursor = new_cursor;
    }

    pub fn input_delete(&mut self) {
        if self.input_cursor >= self.input.len() {
            return;
        }
        let mut end = self.input_cursor + 1;
        while end < self.input.len() && !self.input.is_char_boundary(end) {
            end += 1;
        }
        self.input.replace_range(self.input_cursor..end, "");
    }

    pub fn input_left(&mut self) {
        if self.input_cursor == 0 {
            return;
        }
        let mut c = self.input_cursor - 1;
        while c > 0 && !self.input.is_char_boundary(c) {
            c -= 1;
        }
        self.input_cursor = c;
    }

    pub fn input_right(&mut self) {
        if self.input_cursor >= self.input.len() {
            return;
        }
        let mut c = self.input_cursor + 1;
        while c < self.input.len() && !self.input.is_char_boundary(c) {
            c += 1;
        }
        self.input_cursor = c;
    }

    pub fn input_home(&mut self) {
        self.input_cursor = 0;
    }

    pub fn input_end(&mut self) {
        self.input_cursor = self.input.len();
    }

    // ------------------------------------------------------------------
    // Status

    pub fn set_status(&mut self, text: String, is_error: bool) {
        self.status = Some(StatusMessage {
            text,
            is_error,
            at: Instant::now(),
        });
    }

    pub fn expire_status(&mut self) {
        if let Some(s) = &self.status {
            if s.at.elapsed() > Duration::from_secs(5) {
                self.status = None;
            }
        }
    }
}

/// RAII guard that suspends the ratatui TUI (leaves alt screen, disables raw
/// mode) for the lifetime of an external full-terminal program, restoring
/// everything on drop even if the callee errors.
struct TuiSuspend;

impl TuiSuspend {
    fn new() -> Self {
        use crossterm::{
            event::DisableMouseCapture,
            execute,
            terminal::{disable_raw_mode, LeaveAlternateScreen},
        };
        let _ = disable_raw_mode();
        let _ = execute!(std::io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        TuiSuspend
    }
}

impl Drop for TuiSuspend {
    fn drop(&mut self) {
        use crossterm::{
            event::EnableMouseCapture,
            execute,
            terminal::{enable_raw_mode, EnterAlternateScreen},
        };
        let _ = enable_raw_mode();
        let _ = execute!(std::io::stdout(), EnterAlternateScreen, EnableMouseCapture);
    }
}

/// Pause so the user can read whatever the program printed before the TUI
/// redraws over it.
fn pause_for_enter() {
    use std::io::{Read, Write};
    let mut out = std::io::stdout();
    let _ = writeln!(out);
    let _ = write!(out, "[press Enter to return]");
    let _ = out.flush();
    let _ = std::io::stdin().read(&mut [0u8; 1]);
}

fn count_label(n: usize) -> String {
    if n == 1 {
        "1 item".into()
    } else {
        format!("{n} items")
    }
}

pub fn shell_quote(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-./=+:@%".contains(c))
    {
        return s.to_string();
    }
    let escaped = s.replace('\'', r"'\''");
    format!("'{escaped}'")
}

fn shell_words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escape = false;
    for c in s.chars() {
        if escape {
            cur.push(c);
            escape = false;
            continue;
        }
        match c {
            '\\' if !in_single => escape = true,
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            c if c.is_whitespace() && !in_single && !in_double => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn expand_tilde(input: &str) -> String {
    if let Some(rest) = input.strip_prefix('~') {
        if let Some(home) = dirs::home_dir() {
            return format!("{}{}", home.display(), rest);
        }
    }
    input.into()
}

fn is_valid_bookmark_key(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+')
}

/// Convert an absolute path into the form we want stored on disk: replace
/// the user's home directory with `~` so the saved config remains portable
/// across hosts and users. Falls back to the raw absolute path otherwise.
fn path_for_storage(p: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(rest) = p.strip_prefix(&home) {
            if rest.as_os_str().is_empty() {
                return "~".into();
            }
            return format!("~/{}", rest.display());
        }
    }
    p.display().to_string()
}

/// Returns true if `cmd` looks like it needs a real terminal (full-screen
/// editor, pager, TUI app, REPL, or anything that reads from stdin without
/// `<` redirection). Detection is by program-name match on the first word
/// of the *effective* command — we strip leading `sudo`/`env`-style wrappers
/// and look at the next token.
fn cmd_is_interactive(cmd: &str) -> bool {
    let tokens = cmd.split_whitespace();
    let mut prog: Option<&str> = None;
    for tok in tokens {
        if tok.contains('=') && !tok.starts_with('=') {
            continue;
        }
        match tok {
            "sudo" | "doas" | "env" | "nice" | "ionice" | "nohup" | "stdbuf" | "command" => {
                continue;
            }
            _ => {
                prog = Some(tok);
                break;
            }
        }
    }
    let Some(prog) = prog else { return false };
    let basename = prog.rsplit('/').next().unwrap_or(prog);
    matches!(
        basename,
        "vi" | "vim"
            | "nvim"
            | "nano"
            | "emacs"
            | "helix"
            | "hx"
            | "kak"
            | "micro"
            | "less"
            | "more"
            | "most"
            | "bat"
            | "htop"
            | "btop"
            | "top"
            | "iotop"
            | "iftop"
            | "atop"
            | "lazygit"
            | "tig"
            | "gitui"
            | "ranger"
            | "lf"
            | "nnn"
            | "yazi"
            | "broot"
            | "mc"
            | "tmux"
            | "screen"
            | "byobu"
            | "fzf"
            | "sk"
            | "skim"
            | "python"
            | "python2"
            | "python3"
            | "ipython"
            | "node"
            | "irb"
            | "ghci"
            | "psql"
            | "mysql"
            | "sqlite3"
            | "redis-cli"
            | "mongo"
            | "mongosh"
            | "ssh"
            | "telnet"
            | "mosh"
            | "watch"
            | "tail"
            | "man"
            | "info"
    )
}

/// Returns true if a `git <args>` invocation will block waiting for a
/// terminal: spawns $EDITOR (commit/tag without `-m`/`-F`, `rebase -i`,
/// `add -p`, etc.) or otherwise needs an interactive stdin.
fn git_args_need_tty(args: &[String]) -> bool {
    let sub = args
        .iter()
        .find(|a| !a.starts_with('-') && !a.contains('='))
        .map(|s| s.as_str())
        .unwrap_or("");
    let has_msg_flag = args.iter().any(|a| {
        a == "-m"
            || a == "-F"
            || a == "--no-edit"
            || a.starts_with("--message")
            || a.starts_with("--file")
    });
    let has_interactive_flag = args
        .iter()
        .any(|a| a == "-i" || a == "--interactive" || a == "-p" || a == "--patch");
    match sub {
        "commit" | "tag" => !has_msg_flag,
        "rebase" | "cherry-pick" | "revert" | "merge" | "add" | "checkout" | "reset"
        | "restore" | "stash" => has_interactive_flag,
        _ => false,
    }
}
