use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
    Ignored,
    Conflict,
}

impl GitStatus {
    pub fn label(self) -> &'static str {
        match self {
            GitStatus::Modified => "M",
            GitStatus::Added => "A",
            GitStatus::Deleted => "D",
            GitStatus::Renamed => "R",
            GitStatus::Untracked => "?",
            GitStatus::Ignored => "!",
            GitStatus::Conflict => "U",
        }
    }
}

/// Returns (repo_root, status_map-by-absolute-path) if `cwd` is inside a git repo.
pub fn status_for(cwd: &Path) -> Option<(PathBuf, HashMap<PathBuf, GitStatus>)> {
    let root_out = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .arg("rev-parse")
        .arg("--show-toplevel")
        .output()
        .ok()?;
    if !root_out.status.success() {
        return None;
    }
    let root = PathBuf::from(
        String::from_utf8_lossy(&root_out.stdout)
            .trim()
            .to_string(),
    );
    if root.as_os_str().is_empty() {
        return None;
    }

    let status_out = Command::new("git")
        .arg("-C")
        .arg(&root)
        .arg("status")
        .arg("--porcelain")
        .arg("--ignored")
        .output()
        .ok()?;
    if !status_out.status.success() {
        return Some((root, HashMap::new()));
    }

    let mut map: HashMap<PathBuf, GitStatus> = HashMap::new();
    for line in String::from_utf8_lossy(&status_out.stdout).lines() {
        if line.len() < 3 {
            continue;
        }
        let code = &line[..2];
        let rest = &line[3..];
        // Rename entries look like "R  old -> new"; take the new path.
        let relative = if let Some((_, new)) = rest.split_once(" -> ") {
            new
        } else {
            rest
        };
        let status = match code {
            "??" => GitStatus::Untracked,
            "!!" => GitStatus::Ignored,
            s if s.contains('U') || s == "AA" || s == "DD" => GitStatus::Conflict,
            s if s.starts_with('R') || s.contains('R') => GitStatus::Renamed,
            s if s.starts_with('A') || s.contains('A') => GitStatus::Added,
            s if s.starts_with('D') || s.contains('D') => GitStatus::Deleted,
            s if s.starts_with('M') || s.contains('M') => GitStatus::Modified,
            _ => continue,
        };
        let abs = root.join(relative);
        map.insert(abs.clone(), status);
        // Propagate status up to containing directories so folder entries show a marker.
        let mut parent = abs.parent();
        while let Some(p) = parent {
            if p == root {
                break;
            }
            map.entry(p.to_path_buf()).or_insert(status);
            parent = p.parent();
        }
    }
    Some((root, map))
}
