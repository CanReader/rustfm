use anyhow::Result;
use ratatui_image::picker::Picker;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use crate::{
    background::{Task, TaskMsg, Worker},
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    Rename,
    NewFile,
    NewDir,
    GoTo,
    Bookmark,
    CommitMsg,
    GitCmd,
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

pub struct App {
    pub config: Config,
    pub palette: Palette,
    pub cwd: PathBuf,
    pub entries: Vec<Entry>,
    pub cursor: usize,
    pub parent_entries: Vec<Entry>,
    pub parent_cursor: usize,
    pub preview: Preview,
    pub pane_state: PaneState,
    pub selected: HashSet<PathBuf>,
    pub select_anchor: Option<usize>,
    pub clipboard: Vec<PathBuf>,
    pub clip_mode: ClipMode,
    pub mode: Mode,
    pub input: String,
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
            pane_state: PaneState { cursor: HashMap::new() },
            selected: HashSet::new(),
            select_anchor: None,
            clipboard: Vec::new(),
            clip_mode: ClipMode::Copy,
            mode: Mode::Normal,
            input: String::new(),
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
        let path = self.entries.get(self.cursor).map(|e| e.path.clone());
        self.preview = match path {
            Some(p) => preview::generate(
                &p,
                self.show_hidden,
                self.picker.as_mut(),
                self.git.as_ref(),
                self.diff_mode,
            ),
            None => Preview::Empty,
        };
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
        self.set_status(format!("yanked {} item(s)", self.clipboard.len()), false);
    }

    pub fn cut(&mut self) {
        self.stash_clipboard(ClipMode::Cut);
        self.set_status(format!("cut {} item(s)", self.clipboard.len()), false);
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

    pub fn make_dir(&mut self, name: &str) -> Result<()> {
        let dst = self.cwd.join(name);
        match fs_ops::create_dir(&dst) {
            Ok(_) => {
                self.refresh()?;
                self.set_status(format!("mkdir {name}"), false);
            }
            Err(e) => self.set_status(format!("mkdir failed: {e}"), true),
        }
        Ok(())
    }

    pub fn make_file(&mut self, name: &str) -> Result<()> {
        let dst = self.cwd.join(name);
        match fs_ops::create_file(&dst) {
            Ok(_) => {
                self.refresh()?;
                self.set_status(format!("touch {name}"), false);
            }
            Err(e) => self.set_status(format!("touch failed: {e}"), true),
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
