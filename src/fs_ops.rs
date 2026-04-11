use crate::config::SortMode;
use anyhow::{Context, Result};
use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub name_lower: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub readonly: bool,
    pub ext_lower: Option<String>,
}

impl Entry {
    pub fn from_path(path: PathBuf) -> Result<Self> {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        let meta = fs::symlink_metadata(&path)
            .with_context(|| format!("stat {}", path.display()))?;
        let ft = meta.file_type();
        let is_symlink = ft.is_symlink();
        let is_dir = if is_symlink {
            fs::metadata(&path).map(|m| m.is_dir()).unwrap_or(false)
        } else {
            ft.is_dir()
        };
        let ext_lower = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase());
        let name_lower = name.to_ascii_lowercase();
        Ok(Self {
            name,
            name_lower,
            path,
            is_dir,
            is_symlink,
            size: meta.len(),
            modified: meta.modified().ok(),
            readonly: meta.permissions().readonly(),
            ext_lower,
        })
    }

    pub fn is_hidden(&self) -> bool {
        self.name.starts_with('.')
    }
}

pub fn list_dir(
    path: &Path,
    show_hidden: bool,
    sort: SortMode,
    reverse: bool,
    dirs_first: bool,
) -> Result<Vec<Entry>> {
    let mut entries = Vec::new();
    let rd = match fs::read_dir(path) {
        Ok(rd) => rd,
        Err(_) => return Ok(entries),
    };
    for dirent in rd.flatten() {
        if let Ok(e) = Entry::from_path(dirent.path()) {
            if !show_hidden && e.is_hidden() {
                continue;
            }
            entries.push(e);
        }
    }
    sort_entries(&mut entries, sort, reverse, dirs_first);
    Ok(entries)
}

pub fn sort_entries(entries: &mut [Entry], mode: SortMode, reverse: bool, dirs_first: bool) {
    entries.sort_by(|a, b| {
        if dirs_first {
            match (a.is_dir, b.is_dir) {
                (true, false) => return std::cmp::Ordering::Less,
                (false, true) => return std::cmp::Ordering::Greater,
                _ => {}
            }
        }
        let ord = match mode {
            SortMode::Name => a.name_lower.cmp(&b.name_lower),
            SortMode::Size => a.size.cmp(&b.size),
            SortMode::Mtime => a.modified.cmp(&b.modified),
            SortMode::Ext => a.ext_lower.cmp(&b.ext_lower).then(a.name_lower.cmp(&b.name_lower)),
        };
        if reverse {
            ord.reverse()
        } else {
            ord
        }
    });
}

pub fn copy_path(src: &Path, dst: &Path) -> Result<()> {
    if src.is_dir() {
        copy_dir_all(src, dst)?;
    } else {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).ok();
        }
        fs::copy(src, dst)?;
    }
    Ok(())
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if ft.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else {
            fs::copy(entry.path(), dst_path)?;
        }
    }
    Ok(())
}

pub fn move_path(src: &Path, dst: &Path) -> Result<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).ok();
    }
    if fs::rename(src, dst).is_err() {
        copy_path(src, dst)?;
        delete_path(src, false)?;
    }
    Ok(())
}

pub fn delete_path(path: &Path, use_trash: bool) -> Result<()> {
    if use_trash {
        trash::delete(path).context("moving to trash")?;
        return Ok(());
    }
    if path.is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub fn unique_destination(dir: &Path, name: &str) -> PathBuf {
    let mut dst = dir.join(name);
    if !dst.exists() {
        return dst;
    }
    let (stem, ext) = split_name(name);
    for i in 1..10_000 {
        let candidate = if ext.is_empty() {
            format!("{stem}_{i}")
        } else {
            format!("{stem}_{i}.{ext}")
        };
        dst = dir.join(candidate);
        if !dst.exists() {
            return dst;
        }
    }
    dst
}

fn split_name(name: &str) -> (String, String) {
    match name.rfind('.') {
        Some(i) if i > 0 => (name[..i].into(), name[i + 1..].into()),
        _ => (name.into(), String::new()),
    }
}

pub fn create_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    Ok(())
}

pub fn create_file(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    fs::OpenOptions::new().create_new(true).write(true).open(path)?;
    Ok(())
}
