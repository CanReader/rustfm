use anyhow::Result;
use ratatui_image::picker::Picker;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use crate::{
    background::{Task, TaskMsg, Worker},
    clipboard,
    config::{Config, SortMode},
    fs_ops::{self, Entry},
    fuzzy,
    git::{self, GitInfo},
    opener::Opener,
    preview::{self, Preview},
    theme::Palette,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Prompt(PromptKind),
    Search,
    Filter,
    Fuzzy,
    ConfirmDelete,
    Sort,
    Palette,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteAction {
    Yank,
    Cut,
    Paste,
    Delete,
    Rename,
    NewEntry,
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
    Quit,
}

pub struct PaletteEntry {
    pub action: PaletteAction,
    pub label: &'static str,
    pub hint: &'static str,
}

pub const PALETTE_ENTRIES: &[PaletteEntry] = &[
    PaletteEntry { action: PaletteAction::Yank, label: "Yank (copy) selection", hint: "y" },
    PaletteEntry { action: PaletteAction::Cut, label: "Cut selection", hint: "d" },
    PaletteEntry { action: PaletteAction::Paste, label: "Paste into current directory", hint: "p" },
    PaletteEntry { action: PaletteAction::Delete, label: "Delete selection", hint: "D" },
    PaletteEntry { action: PaletteAction::Rename, label: "Rename current entry", hint: "r" },
    PaletteEntry { action: PaletteAction::NewEntry, label: "New file or directory", hint: "a" },
    PaletteEntry { action: PaletteAction::CopyPath, label: "Copy absolute path to clipboard", hint: "" },
    PaletteEntry { action: PaletteAction::CopyName, label: "Copy filename to clipboard", hint: "" },
    PaletteEntry { action: PaletteAction::CopyDir, label: "Copy current directory path to clipboard", hint: "" },
    PaletteEntry { action: PaletteAction::CopyFilesToClipboard, label: "Copy files to system clipboard (paste in other apps)", hint: "" },
    PaletteEntry { action: PaletteAction::ToggleHidden, label: "Toggle hidden files", hint: "." },
    PaletteEntry { action: PaletteAction::ToggleDiff, label: "Toggle git diff preview", hint: "z d" },
    PaletteEntry { action: PaletteAction::Refresh, label: "Refresh directory", hint: "R" },
    PaletteEntry { action: PaletteAction::SelectAll, label: "Select all entries", hint: "C-a" },
    PaletteEntry { action: PaletteAction::ClearSelection, label: "Clear selection", hint: "Esc" },
    PaletteEntry { action: PaletteAction::SortName, label: "Sort by name", hint: "o n" },
    PaletteEntry { action: PaletteAction::SortSize, label: "Sort by size", hint: "o s" },
    PaletteEntry { action: PaletteAction::SortMtime, label: "Sort by modified time", hint: "o t" },
    PaletteEntry { action: PaletteAction::SortExt, label: "Sort by extension", hint: "o e" },
    PaletteEntry { action: PaletteAction::SortReverse, label: "Reverse sort order", hint: "o r" },
    PaletteEntry { action: PaletteAction::GitStage, label: "Git: stage", hint: "z s" },
    PaletteEntry { action: PaletteAction::GitUnstage, label: "Git: unstage", hint: "z u" },
    PaletteEntry { action: PaletteAction::GitDiscard, label: "Git: discard worktree changes", hint: "z x" },
    PaletteEntry { action: PaletteAction::GitCommit, label: "Git: commit staged changes", hint: "z c" },
    PaletteEntry { action: PaletteAction::GitDiff, label: "Git: toggle diff preview", hint: "z d" },
    PaletteEntry { action: PaletteAction::GitRefresh, label: "Git: refresh state", hint: "z r" },
    PaletteEntry { action: PaletteAction::GitRaw, label: "Git: run arbitrary command", hint: "z g" },
    PaletteEntry { action: PaletteAction::GoTo, label: "Go to path…", hint: "" },
    PaletteEntry { action: PaletteAction::Bookmark, label: "Jump to bookmark", hint: "'" },
    PaletteEntry { action: PaletteAction::Shell, label: "Run shell command", hint: "!" },
    PaletteEntry { action: PaletteAction::Quit, label: "Quit Rustfm", hint: "q" },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    Rename,
    New,
    GoTo,
    Bookmark,
    CommitMsg,
    GitCmd,
    Shell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipMode {
    Copy,
    Cut,
}

pub struct StatusMessage {
    pub text: String,
    pub is_error: bool,
    pub at: Instant,
}

pub struct Progress {
    pub id: u64,
    pub label: String,
    pub total: u64,
    pub done: u64,
    pub current: String,
}

pub struct PaneState {
    pub cursor: HashMap<PathBuf, usize>,
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

pub struct App {
    pub config: Config,
    pub palette: Palette,
    pub cwd: PathBuf,
    pub entries: Vec<Entry>,
    pub cursor: usize,
    pub parent_entries: Vec<Entry>,
    pub parent_cursor: usize,
    pub preview: Preview,
    pub preview_scroll: u16,
    pub preview_pending: Option<(PathBuf, Instant)>,
    pub pdf_state: Option<PdfState>,
    pub pane_state: PaneState,
    pub selected: HashSet<PathBuf>,
    pub select_anchor: Option<usize>,
    pub clipboard: Vec<PathBuf>,
    pub clip_mode: ClipMode,
    pub mode: Mode,
    pub input: String,
    pub input_cursor: usize,
    pub search_query: String,
    pub filter: String,
    pub fuzzy_matches: Vec<FuzzyMatch>,
    pub status: Option<StatusMessage>,
    pub show_hidden: bool,
    pub quit: bool,
    pub picker: Option<Picker>,
    pub git: Option<GitInfo>,
    pub diff_mode: bool,
    pub worker: Worker,
    pub next_task_id: u64,
    pub progress: Option<Progress>,
    pub palette_matches: Vec<FuzzyMatch>,
    pub palette_cursor: usize,
    /// Set after returning from an external program (editor, pager) that took
    /// over the terminal. The event loop clears ratatui's diff buffer before
    /// the next draw so the whole UI repaints, not just the diff against a
    /// stale cached frame.
    pub needs_redraw: bool,
}

impl App {
    pub fn new(start: PathBuf, config: Config, picker: Option<Picker>) -> Result<Self> {
        let palette = Palette::from_theme(&config.theme);
        let show_hidden = config.show_hidden;
        let worker = Worker::spawn();
        let mut app = Self {
            palette,
            show_hidden,
            config,
            cwd: start,
            entries: Vec::new(),
            cursor: 0,
            parent_entries: Vec::new(),
            parent_cursor: 0,
            preview: Preview::Empty,
            preview_scroll: 0,
            preview_pending: None,
            pdf_state: None,
            pane_state: PaneState { cursor: HashMap::new() },
            selected: HashSet::new(),
            select_anchor: None,
            clipboard: Vec::new(),
            clip_mode: ClipMode::Copy,
            mode: Mode::Normal,
            input: String::new(),
            input_cursor: 0,
            search_query: String::new(),
            filter: String::new(),
            fuzzy_matches: Vec::new(),
            status: None,
            quit: false,
            picker,
            git: None,
            diff_mode: false,
            worker,
            next_task_id: 1,
            progress: None,
            palette_matches: Vec::new(),
            palette_cursor: 0,
            needs_redraw: false,
        };
        app.refresh()?;
        Ok(app)
    }

    pub fn refresh(&mut self) -> Result<()> {
        let all = fs_ops::list_dir(
            &self.cwd,
            self.show_hidden,
            self.config.sort,
            self.config.sort_reverse,
            self.config.dirs_first,
        )?;
        self.entries = if self.filter.is_empty() {
            all
        } else {
            let needle = self.filter.to_ascii_lowercase();
            all.into_iter()
                .filter(|e| e.name_lower.contains(&needle))
                .collect()
        };
        if self.cursor >= self.entries.len() {
            self.cursor = self.entries.len().saturating_sub(1);
        }
        if let Some(saved) = self.pane_state.cursor.get(&self.cwd) {
            if *saved < self.entries.len() {
                self.cursor = *saved;
            }
        }
        self.reload_parent();
        if self.config.git_integration {
            self.reload_git();
        } else {
            self.git = None;
        }
        self.reload_preview();
        Ok(())
    }

    fn reload_parent(&mut self) {
        if let Some(parent) = self.cwd.parent() {
            self.parent_entries = fs_ops::list_dir(
                parent,
                self.show_hidden,
                self.config.sort,
                self.config.sort_reverse,
                self.config.dirs_first,
            )
            .unwrap_or_default();
            self.parent_cursor = self
                .parent_entries
                .iter()
                .position(|e| e.path == self.cwd)
                .unwrap_or(0);
        } else {
            self.parent_entries.clear();
            self.parent_cursor = 0;
        }
    }

    fn reload_git(&mut self) {
        self.git = git::fetch(&self.cwd);
    }

    fn reload_preview(&mut self) {
        self.preview_scroll = 0;
        self.pdf_state = None;
        let path = self.entries.get(self.cursor).map(|e| e.path.clone());
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
        );
    }

    pub fn tick(&mut self) {
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
        self.pdf_state = Some(PdfState { path, page: 1, total });
        self.preview_pending = None;
    }

    pub fn scroll_preview(&mut self, delta: i32) {
        if let Some(st) = self.pdf_state.as_mut() {
            let new = (st.page as i32 + delta).clamp(1, st.total as i32) as u32;
            if new == st.page {
                return;
            }
            st.page = new;
            let path = st.path.clone();
            self.preview = preview::render_pdf_page(&path, new, self.picker.as_mut());
            self.set_status(format!("pdf page {}/{}", new, self.pdf_state.as_ref().unwrap().total), false);
            return;
        }
        let new = (self.preview_scroll as i32 + delta).max(0) as u16;
        self.preview_scroll = new;
    }

    pub fn toggle_diff_mode(&mut self) {
        self.diff_mode = !self.diff_mode;
        self.reload_preview();
        let msg = if self.diff_mode { "diff preview on" } else { "diff preview off" };
        self.set_status(msg.into(), false);
    }

    fn git_targets(&self) -> Vec<PathBuf> {
        if self.selected.is_empty() {
            self.current_entry().map(|e| vec![e.path.clone()]).unwrap_or_default()
        } else {
            self.selected.iter().cloned().collect()
        }
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
        let d = shell_quote(&self.cwd.display().to_string());
        let s = if self.selected.is_empty() {
            f.clone()
        } else {
            let mut items: Vec<String> = self
                .selected
                .iter()
                .map(|p| shell_quote(&p.display().to_string()))
                .collect();
            items.sort();
            items.join(" ")
        };
        template
            .replace("{f}", &f)
            .replace("{n}", &n)
            .replace("{d}", &d)
            .replace("{s}", &s)
    }

    fn run_shell_cmd(&mut self, cmd: &str) -> Result<()> {
        use std::process::Command;

        let out = Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .current_dir(&self.cwd)
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
                let _ = self.reload_git_only();
            }
            Err(e) => {
                self.set_status(format!("spawn failed: {e}"), true);
            }
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
            .unwrap_or_else(|| self.cwd.clone());
        match git::run_raw(&cwd, &args) {
            Ok(lines) => {
                let summary = format!("git {}", trimmed);
                self.preview = Preview::Text(if lines.is_empty() {
                    vec!["(no output)".into()]
                } else {
                    lines
                });
                self.set_status(summary, false);
                let _ = self.reload_git_only();
            }
            Err(e) => self.set_status(format!("git failed: {e}"), true),
        }
        Ok(())
    }

    fn reload_git_only(&mut self) -> Result<()> {
        if self.config.git_integration {
            self.reload_git();
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

    pub fn current_entry(&self) -> Option<&Entry> {
        self.entries.get(self.cursor)
    }

    pub fn move_cursor(&mut self, delta: i64) {
        if self.entries.is_empty() {
            return;
        }
        let len = self.entries.len() as i64;
        let new = (self.cursor as i64 + delta).clamp(0, len - 1) as usize;
        self.cursor = new;
        self.pane_state.cursor.insert(self.cwd.clone(), new);
        self.reload_preview();
    }

    pub fn goto_top(&mut self) {
        self.cursor = 0;
        self.pane_state.cursor.insert(self.cwd.clone(), 0);
        self.reload_preview();
    }

    pub fn goto_bottom(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.cursor = self.entries.len() - 1;
        self.pane_state.cursor.insert(self.cwd.clone(), self.cursor);
        self.reload_preview();
    }

    pub fn enter(&mut self) -> Result<()> {
        let Some(entry) = self.current_entry().cloned() else { return Ok(()) };
        if entry.is_dir {
            self.cwd = entry.path;
            self.cursor = 0;
            self.filter.clear();
            self.refresh()?;
        } else {
            self.open_current()?;
        }
        Ok(())
    }

    pub fn go_up(&mut self) -> Result<()> {
        if let Some(parent) = self.cwd.parent().map(PathBuf::from) {
            let old = self.cwd.clone();
            self.cwd = parent;
            self.filter.clear();
            self.refresh()?;
            if let Some(pos) = self.entries.iter().position(|e| e.path == old) {
                self.cursor = pos;
                self.pane_state.cursor.insert(self.cwd.clone(), pos);
                self.reload_preview();
            }
        }
        Ok(())
    }

    pub fn open_current(&mut self) -> Result<()> {
        let Some(entry) = self.current_entry().cloned() else { return Ok(()) };
        if entry.is_dir {
            return Ok(());
        }
        let opener = Opener::new(&self.config.openers);
        let outcome = {
            use crossterm::{
                event::DisableMouseCapture,
                execute,
                terminal::{disable_raw_mode, LeaveAlternateScreen},
            };
            let _ = disable_raw_mode();
            let _ = execute!(std::io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
            let res = opener.open(&entry.path);
            use crossterm::{
                event::EnableMouseCapture,
                terminal::{enable_raw_mode, EnterAlternateScreen},
            };
            let _ = enable_raw_mode();
            let _ = execute!(std::io::stdout(), EnterAlternateScreen, EnableMouseCapture);
            res
        };
        match outcome {
            Ok(crate::opener::OpenOutcome::Internal) => {
                self.set_status(format!("opened {}", entry.name), false);
            }
            Ok(crate::opener::OpenOutcome::OsDefault) => {
                self.set_status(format!("opened {} (OS default)", entry.name), false);
            }
            Err(e) => self.set_status(format!("open failed: {e}"), true),
        }
        self.needs_redraw = true;
        Ok(())
    }

    pub fn toggle_select(&mut self) {
        if let Some(entry) = self.current_entry().cloned() {
            if !self.selected.remove(&entry.path) {
                self.selected.insert(entry.path);
            }
        }
        self.select_anchor = None;
        self.move_cursor(1);
    }

    pub fn toggle_select_no_move(&mut self) {
        if let Some(entry) = self.current_entry().cloned() {
            if !self.selected.remove(&entry.path) {
                self.selected.insert(entry.path);
            }
        }
        self.select_anchor = None;
    }

    pub fn select_all(&mut self) {
        for e in &self.entries {
            self.selected.insert(e.path.clone());
        }
        self.select_anchor = None;
    }

    pub fn range_select(&mut self, delta: i64) {
        if self.entries.is_empty() {
            return;
        }
        if self.select_anchor.is_none() {
            self.select_anchor = Some(self.cursor);
            if let Some(e) = self.entries.get(self.cursor) {
                self.selected.insert(e.path.clone());
            }
        }
        self.move_cursor(delta);
        let anchor = self.select_anchor.unwrap_or(self.cursor);
        let (lo, hi) = if anchor <= self.cursor {
            (anchor, self.cursor)
        } else {
            (self.cursor, anchor)
        };
        for i in lo..=hi {
            if let Some(e) = self.entries.get(i) {
                self.selected.insert(e.path.clone());
            }
        }
    }

    pub fn clear_selection(&mut self) {
        self.selected.clear();
        self.select_anchor = None;
    }

    pub fn toggle_hidden(&mut self) -> Result<()> {
        self.show_hidden = !self.show_hidden;
        self.refresh()
    }

    pub fn yank(&mut self) {
        self.stash_clipboard(ClipMode::Copy);
        let n = self.clipboard.len();
        let sys = self.push_clipboard_files();
        self.set_status(format!("yanked {n} item(s){sys}"), false);
    }

    pub fn cut(&mut self) {
        self.stash_clipboard(ClipMode::Cut);
        let n = self.clipboard.len();
        let sys = self.push_clipboard_files();
        self.set_status(format!("cut {n} item(s){sys}"), false);
    }

    /// Best-effort push of the current internal clipboard to the OS clipboard
    /// as a `text/uri-list` payload. Returns a short status suffix such as
    /// " → wl-copy" on success, or " (no clipboard tool)" on failure — both
    /// forms are appended to the main yank/cut status line so the user sees
    /// whether the system clipboard actually received the files.
    fn push_clipboard_files(&self) -> String {
        if self.clipboard.is_empty() {
            return String::new();
        }
        match clipboard::copy_files(&self.clipboard) {
            Ok(tool) => format!(" → {tool}"),
            Err(e) => format!(" ({e})"),
        }
    }

    pub fn copy_current_path(&mut self) {
        let Some(entry) = self.current_entry().cloned() else { return };
        let text = entry.path.display().to_string();
        match clipboard::copy_text(&text) {
            Ok(tool) => self.set_status(format!("copied path → {tool}"), false),
            Err(e) => self.set_status(format!("copy failed: {e}"), true),
        }
    }

    pub fn copy_current_name(&mut self) {
        let Some(entry) = self.current_entry().cloned() else { return };
        match clipboard::copy_text(&entry.name) {
            Ok(tool) => self.set_status(format!("copied name → {tool}"), false),
            Err(e) => self.set_status(format!("copy failed: {e}"), true),
        }
    }

    pub fn copy_cwd(&mut self) {
        let text = self.cwd.display().to_string();
        match clipboard::copy_text(&text) {
            Ok(tool) => self.set_status(format!("copied cwd → {tool}"), false),
            Err(e) => self.set_status(format!("copy failed: {e}"), true),
        }
    }

    /// Push the current selection (or entry under cursor) to the OS clipboard
    /// without touching the internal yank buffer. Useful for sharing files
    /// with other apps without disturbing an in-progress cut/paste flow.
    pub fn copy_selection_to_os_clipboard(&mut self) {
        let items: Vec<PathBuf> = if self.selected.is_empty() {
            self.current_entry().map(|e| vec![e.path.clone()]).unwrap_or_default()
        } else {
            self.selected.iter().cloned().collect()
        };
        if items.is_empty() {
            return;
        }
        match clipboard::copy_files(&items) {
            Ok(tool) => self.set_status(format!("copied {} item(s) → {tool}", items.len()), false),
            Err(e) => self.set_status(format!("copy failed: {e}"), true),
        }
    }

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
            self.palette_matches.sort_by(|a, b| b.score.cmp(&a.score));
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
                    self.mode = Mode::ConfirmDelete;
                } else {
                    self.delete_current()?;
                }
            }
            PaletteAction::Rename => {
                let name = self.current_entry().map(|e| e.name.clone()).unwrap_or_default();
                self.input_set(name);
                self.mode = Mode::Prompt(PromptKind::Rename);
                return Ok(Some(PromptKind::Rename));
            }
            PaletteAction::NewEntry => {
                self.input_clear();
                self.mode = Mode::Prompt(PromptKind::New);
                return Ok(Some(PromptKind::New));
            }
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
                self.input_clear();
                self.mode = Mode::Prompt(PromptKind::Bookmark);
                return Ok(Some(PromptKind::Bookmark));
            }
            PaletteAction::Shell => {
                self.input_clear();
                self.mode = Mode::Prompt(PromptKind::Shell);
                return Ok(Some(PromptKind::Shell));
            }
            PaletteAction::Quit => self.quit = true,
        }
        Ok(None)
    }

    fn stash_clipboard(&mut self, mode: ClipMode) {
        let items: Vec<PathBuf> = if self.selected.is_empty() {
            self.current_entry().map(|e| vec![e.path.clone()]).unwrap_or_default()
        } else {
            self.selected.iter().cloned().collect()
        };
        self.clipboard = items;
        self.clip_mode = mode;
        self.selected.clear();
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_task_id;
        self.next_task_id += 1;
        id
    }

    pub fn paste(&mut self) -> Result<()> {
        if self.clipboard.is_empty() {
            self.set_status("clipboard empty".into(), true);
            return Ok(());
        }
        let sources = self.clipboard.clone();
        let id = self.next_id();
        let task = match self.clip_mode {
            ClipMode::Copy => Task::Copy { id, sources, dest_dir: self.cwd.clone() },
            ClipMode::Cut => Task::Move { id, sources, dest_dir: self.cwd.clone() },
        };
        self.worker.submit(task);
        if self.clip_mode == ClipMode::Cut {
            self.clipboard.clear();
        }
        Ok(())
    }

    pub fn delete_current(&mut self) -> Result<()> {
        let targets: Vec<PathBuf> = if self.selected.is_empty() {
            self.current_entry().map(|e| vec![e.path.clone()]).unwrap_or_default()
        } else {
            self.selected.iter().cloned().collect()
        };
        if targets.is_empty() {
            return Ok(());
        }
        self.selected.clear();
        let id = self.next_id();
        self.worker.submit(Task::Delete {
            id,
            targets,
            use_trash: self.config.use_trash,
        });
        Ok(())
    }

    pub fn rename_current(&mut self, new_name: &str) -> Result<()> {
        let Some(entry) = self.current_entry().cloned() else { return Ok(()) };
        let dst = self.cwd.join(new_name);
        if dst.exists() {
            self.set_status("target exists".into(), true);
            return Ok(());
        }
        match fs_ops::move_path(&entry.path, &dst) {
            Ok(_) => {
                self.refresh()?;
                self.set_status(format!("renamed → {new_name}"), false);
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
        let dst = self.cwd.join(name);
        let result = if is_dir {
            fs_ops::create_dir(&dst).map(|_| format!("mkdir {name}/"))
        } else {
            fs_ops::create_file(&dst).map(|_| format!("touch {name}"))
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
            self.cwd = canon;
        } else if let Some(parent) = canon.parent() {
            self.cwd = parent.to_path_buf();
            self.filter.clear();
            self.refresh()?;
            if let Some(pos) = self.entries.iter().position(|e| e.path == canon) {
                self.cursor = pos;
                self.reload_preview();
            }
            return Ok(());
        }
        self.cursor = 0;
        self.filter.clear();
        self.refresh()
    }

    pub fn jump_bookmark(&mut self, key: &str) -> Result<()> {
        if let Some(path) = self.config.bookmarks.get(key).cloned() {
            self.goto_path(&path)?;
        } else {
            self.set_status(format!("no bookmark '{key}'"), true);
        }
        Ok(())
    }

    pub fn apply_search(&mut self) {
        let q = self.search_query.to_ascii_lowercase();
        if q.is_empty() {
            return;
        }
        if let Some(pos) = self
            .entries
            .iter()
            .position(|e| e.name_lower.contains(&q))
        {
            self.cursor = pos;
            self.reload_preview();
        } else {
            self.set_status(format!("no match: {q}"), true);
        }
    }

    pub fn search_next(&mut self, forward: bool) {
        if self.search_query.is_empty() || self.entries.is_empty() {
            return;
        }
        let q = self.search_query.to_ascii_lowercase();
        let len = self.entries.len();
        let start = self.cursor;
        for i in 1..=len {
            let idx = if forward {
                (start + i) % len
            } else {
                (start + len - i) % len
            };
            if self.entries[idx].name_lower.contains(&q) {
                self.cursor = idx;
                self.reload_preview();
                return;
            }
        }
    }

    pub fn apply_filter(&mut self) -> Result<()> {
        self.filter = self.input.clone();
        self.cursor = 0;
        self.refresh()
    }

    pub fn clear_filter(&mut self) -> Result<()> {
        if self.filter.is_empty() {
            return Ok(());
        }
        self.filter.clear();
        self.refresh()
    }

    pub fn update_fuzzy(&mut self) {
        self.fuzzy_matches.clear();
        if self.input.is_empty() {
            for (i, _) in self.entries.iter().enumerate() {
                self.fuzzy_matches.push(FuzzyMatch {
                    index: i,
                    score: 0,
                    match_positions: Vec::new(),
                });
            }
            return;
        }
        for (i, e) in self.entries.iter().enumerate() {
            if let Some((score, positions)) = fuzzy::score(&self.input, &e.name) {
                self.fuzzy_matches.push(FuzzyMatch {
                    index: i,
                    score,
                    match_positions: positions,
                });
            }
        }
        self.fuzzy_matches.sort_by(|a, b| b.score.cmp(&a.score));
    }

    pub fn accept_fuzzy(&mut self, selection: usize) {
        if let Some(m) = self.fuzzy_matches.get(selection) {
            self.cursor = m.index;
            self.pane_state.cursor.insert(self.cwd.clone(), self.cursor);
            self.reload_preview();
        }
    }

    pub fn set_sort(&mut self, mode: SortMode) -> Result<()> {
        self.config.sort = mode;
        self.refresh()
    }

    pub fn toggle_sort_reverse(&mut self) -> Result<()> {
        self.config.sort_reverse = !self.config.sort_reverse;
        self.refresh()
    }

    pub fn drain_task_messages(&mut self) {
        while let Ok(msg) = self.worker.msg_rx.try_recv() {
            match msg {
                TaskMsg::Start { id, label, total } => {
                    self.progress = Some(Progress {
                        id,
                        label,
                        total,
                        done: 0,
                        current: String::new(),
                    });
                }
                TaskMsg::Progress { id, done, current } => {
                    if let Some(p) = self.progress.as_mut() {
                        if p.id == id {
                            p.done = done;
                            p.current = current;
                        }
                    }
                }
                TaskMsg::Done { id, ok, errs } => {
                    let label = self
                        .progress
                        .as_ref()
                        .filter(|p| p.id == id)
                        .map(|p| p.label.clone())
                        .unwrap_or_else(|| "task".into());
                    self.progress = None;
                    let _ = self.refresh();
                    self.set_status(format!("{label}: {ok} ok, {errs} failed"), errs > 0);
                }
            }
        }
    }

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

fn shell_quote(s: &str) -> String {
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
