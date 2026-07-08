use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use crate::{
    config::SortMode,
    fs_ops::{self, Entry},
};

/// Panel mode: `Browser` for normal navigation, `Select`
/// for multi-item selection (entered with `v`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelMode {
    Browser,
    Select,
}

/// One file panel. Several can sit side by side (`n` to open,
/// `w` to close, `tab`/`L`/`H` to cycle); each keeps its own location,
/// cursor, search filter, and selection.
pub struct FilePanel {
    pub cwd: PathBuf,
    pub entries: Vec<Entry>,
    pub cursor: usize,
    pub offset: usize,
    pub search: String,
    pub searching: bool,
    pub mode: PanelMode,
    pub selected: HashSet<PathBuf>,
    /// Remembered cursor position per directory so going back restores it.
    pub cursor_memory: HashMap<PathBuf, usize>,
}

impl FilePanel {
    pub fn new(cwd: PathBuf) -> Self {
        Self {
            cwd,
            entries: Vec::new(),
            cursor: 0,
            offset: 0,
            search: String::new(),
            searching: false,
            mode: PanelMode::Browser,
            selected: HashSet::new(),
            cursor_memory: HashMap::new(),
        }
    }

    pub fn refresh(&mut self, show_hidden: bool, sort: SortMode, reverse: bool, dirs_first: bool) {
        let all =
            fs_ops::list_dir(&self.cwd, show_hidden, sort, reverse, dirs_first).unwrap_or_default();
        self.entries = if self.search.is_empty() {
            all
        } else {
            let needle = self.search.to_ascii_lowercase();
            all.into_iter()
                .filter(|e| e.name_lower.contains(&needle))
                .collect()
        };
        if self.cursor >= self.entries.len() {
            self.cursor = self.entries.len().saturating_sub(1);
        }
        if self.search.is_empty() {
            if let Some(saved) = self.cursor_memory.get(&self.cwd) {
                if *saved < self.entries.len() {
                    self.cursor = *saved;
                }
            }
        }
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
        self.cursor_memory.insert(self.cwd.clone(), new);
    }

    pub fn goto_top(&mut self) {
        self.cursor = 0;
        self.cursor_memory.insert(self.cwd.clone(), 0);
    }

    pub fn goto_bottom(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.cursor = self.entries.len() - 1;
        self.cursor_memory.insert(self.cwd.clone(), self.cursor);
    }

    /// Keep the cursor row inside the visible window of `height` rows.
    pub fn clamp_offset(&mut self, height: usize) {
        if height == 0 {
            return;
        }
        if self.cursor < self.offset {
            self.offset = self.cursor;
        }
        if self.cursor >= self.offset + height {
            self.offset = self.cursor + 1 - height;
        }
        let max_offset = self.entries.len().saturating_sub(height);
        if self.offset > max_offset {
            self.offset = max_offset;
        }
    }

    pub fn toggle_select_current(&mut self) {
        if let Some(e) = self.current_entry().cloned() {
            if !self.selected.remove(&e.path) {
                self.selected.insert(e.path);
            }
        }
    }

    pub fn select_all(&mut self) {
        for e in &self.entries {
            self.selected.insert(e.path.clone());
        }
    }

    /// Targets for a file operation: the selection if non-empty, otherwise
    /// the entry under the cursor.
    pub fn targets(&self) -> Vec<PathBuf> {
        if self.selected.is_empty() {
            self.current_entry()
                .map(|e| vec![e.path.clone()])
                .unwrap_or_default()
        } else {
            let mut v: Vec<PathBuf> = self.selected.iter().cloned().collect();
            v.sort();
            v
        }
    }
}
