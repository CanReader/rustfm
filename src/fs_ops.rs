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
    pub is_exec: bool,
    pub ext_lower: Option<String>,
}

impl Entry {
    pub fn from_path(path: PathBuf) -> Result<Self> {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        let meta =
            fs::symlink_metadata(&path).with_context(|| format!("stat {}", path.display()))?;
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
        #[cfg(unix)]
        let is_exec = {
            use std::os::unix::fs::PermissionsExt;
            !is_dir && (meta.permissions().mode() & 0o111) != 0
        };
        #[cfg(not(unix))]
        let is_exec = false;
        Ok(Self {
            name,
            name_lower,
            path,
            is_dir,
            is_symlink,
            size: meta.len(),
            modified: meta.modified().ok(),
            readonly: meta.permissions().readonly(),
            is_exec,
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
            SortMode::Ext => a
                .ext_lower
                .cmp(&b.ext_lower)
                .then(a.name_lower.cmp(&b.name_lower)),
        };
        if reverse {
            ord.reverse()
        } else {
            ord
        }
    });
}

/// Recursively copies `src` to `dst`, preserving symlinks (does not follow
/// them) and continuing past per-entry errors. The returned vector contains
/// human-readable messages for each entry that failed; an empty vector means
/// a fully clean copy. Top-level failures (e.g. unable to create the root
/// destination directory) still propagate as `Err`.
pub fn copy_path(src: &Path, dst: &Path) -> Result<Vec<String>> {
    let meta = fs::symlink_metadata(src).with_context(|| format!("stat {}", src.display()))?;
    let ft = meta.file_type();
    let mut errs = Vec::new();
    if ft.is_symlink() {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).ok();
        }
        copy_symlink(src, dst)
            .with_context(|| format!("symlink {} -> {}", src.display(), dst.display()))?;
    } else if ft.is_dir() {
        // Refuse to recurse a directory into a destination that lives
        // inside the source. Without this guard, paste-into-self (e.g.
        // yank /foo, navigate to /foo, paste — dest becomes /foo/foo_1)
        // would enumerate the freshly-created destination as part of the
        // source's children and copy it back into itself, fanning out
        // until the disk fills.
        if dst_is_inside_src(src, dst) {
            anyhow::bail!(
                "refusing to copy {} into a subdirectory of itself ({})",
                src.display(),
                dst.display()
            );
        }
        copy_dir_recursive(src, dst, &mut errs)?;
    } else {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).ok();
        }
        fs::copy(src, dst)
            .with_context(|| format!("copy {} -> {}", src.display(), dst.display()))?;
    }
    Ok(errs)
}

/// Returns true if `dst` is `src` itself or a path inside `src`. We
/// canonicalize both sides because either could contain `..`, symlinks, or
/// distinct-but-equivalent forms (`/tmp` vs `/private/tmp` on macOS).
/// `dst` may not exist yet, so we canonicalize its existing ancestor and
/// compare.
fn dst_is_inside_src(src: &Path, dst: &Path) -> bool {
    let Ok(src_abs) = src.canonicalize() else {
        return false;
    };
    let mut probe = dst.to_path_buf();
    let dst_abs = loop {
        if let Ok(p) = probe.canonicalize() {
            break p;
        }
        match probe.parent() {
            Some(p) if p != probe.as_path() => probe = p.to_path_buf(),
            _ => return false,
        }
    };
    dst_abs == src_abs || dst_abs.starts_with(&src_abs)
}

fn copy_dir_recursive(src: &Path, dst: &Path, errs: &mut Vec<String>) -> Result<()> {
    fs::create_dir_all(dst).with_context(|| format!("mkdir {}", dst.display()))?;
    let rd = match fs::read_dir(src) {
        Ok(rd) => rd,
        Err(e) => {
            errs.push(format!("read {}: {e}", src.display()));
            return Ok(());
        }
    };
    for entry in rd {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                errs.push(format!("read entry in {}: {e}", src.display()));
                continue;
            }
        };
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(e) => {
                errs.push(format!("type {}: {e}", entry.path().display()));
                continue;
            }
        };
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if ft.is_symlink() {
            if let Err(e) = copy_symlink(&src_path, &dst_path) {
                errs.push(format!("symlink {}: {e}", src_path.display()));
            }
        } else if ft.is_dir() {
            if let Err(e) = copy_dir_recursive(&src_path, &dst_path, errs) {
                errs.push(format!("dir {}: {e}", src_path.display()));
            }
        } else if ft.is_file() {
            if let Err(e) = fs::copy(&src_path, &dst_path) {
                errs.push(format!("file {}: {e}", src_path.display()));
            }
        } else {
            errs.push(format!("skip special {}", src_path.display()));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn copy_symlink(src: &Path, dst: &Path) -> Result<()> {
    use std::os::unix::fs::symlink;
    let target = fs::read_link(src)?;
    // If something already exists at dst (the worker passes a unique
    // destination, but a recursive copy may revisit), refuse rather than
    // dereference-and-clobber.
    if dst.symlink_metadata().is_ok() {
        anyhow::bail!("destination already exists");
    }
    symlink(target, dst)?;
    Ok(())
}

#[cfg(not(unix))]
fn copy_symlink(_src: &Path, _dst: &Path) -> Result<()> {
    anyhow::bail!("symlink copy not supported on this platform")
}

/// Moves `src` to `dst`. Tries `rename(2)` first; if that fails (typically
/// EXDEV on cross-device moves) falls back to a tolerant copy followed by a
/// delete of the source. The source is only deleted if the copy was fully
/// clean — partial copies leave the source intact so no data is lost.
///
/// If the copy succeeds but the source-delete fails (e.g. permission denied
/// on a child file), we surface that as an `errs` entry rather than
/// returning `Err`: the destination is already populated, and `Err` would
/// make the caller treat the whole move as a failure and discard the
/// duplication-warning the user needs to see.
pub fn move_path(src: &Path, dst: &Path) -> Result<Vec<String>> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).ok();
    }
    if fs::rename(src, dst).is_ok() {
        return Ok(Vec::new());
    }
    let mut errs = copy_path(src, dst)?;
    if errs.is_empty() {
        if let Err(e) = delete_path(src, false) {
            errs.push(format!(
                "copied to {} but could not remove source {}: {e}",
                dst.display(),
                src.display()
            ));
        }
    }
    Ok(errs)
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
    fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)?;
    Ok(())
}
