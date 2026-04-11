use anyhow::{bail, Context, Result};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitState {
    Clean,
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    Untracked,
    Ignored,
    Conflict,
}

impl GitState {
    pub fn label(self) -> &'static str {
        match self {
            GitState::Clean => " ",
            GitState::Modified => "M",
            GitState::Added => "A",
            GitState::Deleted => "D",
            GitState::Renamed => "R",
            GitState::Copied => "C",
            GitState::Untracked => "?",
            GitState::Ignored => "!",
            GitState::Conflict => "U",
        }
    }

    pub fn severity(self) -> u8 {
        match self {
            GitState::Conflict => 9,
            GitState::Deleted => 8,
            GitState::Modified => 7,
            GitState::Renamed => 6,
            GitState::Added => 5,
            GitState::Copied => 4,
            GitState::Untracked => 3,
            GitState::Ignored => 1,
            GitState::Clean => 0,
        }
    }

    pub fn is_dirty(self) -> bool {
        !matches!(self, GitState::Clean | GitState::Ignored)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FileStatus {
    pub index: GitState,
    pub worktree: GitState,
}

impl FileStatus {
    pub fn clean() -> Self {
        Self {
            index: GitState::Clean,
            worktree: GitState::Clean,
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.index.is_dirty() || self.worktree.is_dirty()
    }

    pub fn is_untracked(&self) -> bool {
        matches!(self.index, GitState::Untracked) || matches!(self.worktree, GitState::Untracked)
    }

    pub fn merged_with(self, other: FileStatus) -> FileStatus {
        FileStatus {
            index: if other.index.severity() > self.index.severity() {
                other.index
            } else {
                self.index
            },
            worktree: if other.worktree.severity() > self.worktree.severity() {
                other.worktree
            } else {
                self.worktree
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct GitInfo {
    pub root: PathBuf,
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub stash_count: u32,
    pub staged: u32,
    pub unstaged: u32,
    pub untracked: u32,
    pub conflicts: u32,
    pub status: HashMap<PathBuf, FileStatus>,
}

pub fn fetch(cwd: &Path) -> Option<GitInfo> {
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

    let parsed = parse_status_branch(&root);
    let stash_count = count_stashes(&root);

    Some(GitInfo {
        root,
        branch: parsed.branch,
        upstream: parsed.upstream,
        ahead: parsed.ahead,
        behind: parsed.behind,
        stash_count,
        staged: parsed.staged,
        unstaged: parsed.unstaged,
        untracked: parsed.untracked,
        conflicts: parsed.conflicts,
        status: parsed.status,
    })
}

struct ParsedStatus {
    branch: Option<String>,
    upstream: Option<String>,
    ahead: u32,
    behind: u32,
    staged: u32,
    unstaged: u32,
    untracked: u32,
    conflicts: u32,
    status: HashMap<PathBuf, FileStatus>,
}

fn parse_status_branch(root: &Path) -> ParsedStatus {
    let mut result = ParsedStatus {
        branch: None,
        upstream: None,
        ahead: 0,
        behind: 0,
        staged: 0,
        unstaged: 0,
        untracked: 0,
        conflicts: 0,
        status: HashMap::new(),
    };
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("status")
        .arg("--branch")
        .arg("--porcelain=v1")
        .arg("--ignored")
        .output();
    let Ok(out) = out else {
        return result;
    };
    if !out.status.success() {
        return result;
    }
    let text = String::from_utf8_lossy(&out.stdout);

    for (i, line) in text.lines().enumerate() {
        if i == 0 && line.starts_with("## ") {
            parse_branch_header(&line[3..], &mut result);
            continue;
        }
        if line.len() < 3 {
            continue;
        }
        let bytes = line.as_bytes();
        let x = bytes[0] as char;
        let y = bytes[1] as char;
        let rest = &line[3..];
        let path_str = match rest.split_once(" -> ") {
            Some((_, new)) => new,
            None => rest,
        };
        let fs = if x == '?' && y == '?' {
            result.untracked += 1;
            FileStatus {
                index: GitState::Untracked,
                worktree: GitState::Untracked,
            }
        } else if x == '!' && y == '!' {
            FileStatus {
                index: GitState::Ignored,
                worktree: GitState::Ignored,
            }
        } else {
            let idx = map_code(x);
            let wt = map_code(y);
            if matches!(idx, GitState::Conflict) || matches!(wt, GitState::Conflict) {
                result.conflicts += 1;
            } else {
                if idx.is_dirty() {
                    result.staged += 1;
                }
                if wt.is_dirty() {
                    result.unstaged += 1;
                }
            }
            FileStatus {
                index: idx,
                worktree: wt,
            }
        };
        let abs = root.join(path_str.trim_end_matches('/'));
        let merged = match result.status.get(&abs) {
            Some(existing) => existing.merged_with(fs),
            None => fs,
        };
        result.status.insert(abs.clone(), merged);

        // Propagate severity up to ancestor directories.
        let mut parent = abs.parent();
        while let Some(p) = parent {
            if p == root {
                break;
            }
            let merged = match result.status.get(p) {
                Some(existing) => existing.merged_with(fs),
                None => fs,
            };
            result.status.insert(p.to_path_buf(), merged);
            parent = p.parent();
        }
    }
    result
}

fn parse_branch_header(rest: &str, result: &mut ParsedStatus) {
    // Possible forms:
    //   main...origin/main [ahead 1, behind 2]
    //   main...origin/main
    //   main
    //   HEAD (no branch)
    //   No commits yet on main
    if let Some(name) = rest.strip_prefix("No commits yet on ") {
        result.branch = Some(name.to_string());
        return;
    }
    let (branch_part, bracket) = match rest.find(" [") {
        Some(pos) if rest.ends_with(']') => (&rest[..pos], Some(&rest[pos + 2..rest.len() - 1])),
        _ => (rest, None),
    };
    if let Some((b, u)) = branch_part.split_once("...") {
        result.branch = Some(b.to_string());
        result.upstream = Some(u.to_string());
    } else {
        result.branch = Some(branch_part.to_string());
    }
    if let Some(br) = bracket {
        for part in br.split(", ") {
            if let Some(n) = part.strip_prefix("ahead ") {
                result.ahead = n.parse().unwrap_or(0);
            } else if let Some(n) = part.strip_prefix("behind ") {
                result.behind = n.parse().unwrap_or(0);
            } else if let Some(n) = part.strip_prefix("gone ") {
                let _ = n;
            }
        }
    }
}

fn map_code(c: char) -> GitState {
    match c {
        ' ' => GitState::Clean,
        'M' => GitState::Modified,
        'A' => GitState::Added,
        'D' => GitState::Deleted,
        'R' => GitState::Renamed,
        'C' => GitState::Copied,
        'U' => GitState::Conflict,
        '?' => GitState::Untracked,
        '!' => GitState::Ignored,
        _ => GitState::Clean,
    }
}

fn count_stashes(root: &Path) -> u32 {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("rev-list")
        .arg("--count")
        .arg("--walk-reflogs")
        .arg("refs/stash")
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .trim()
            .parse()
            .unwrap_or(0),
        _ => 0,
    }
}

pub fn stage(root: &Path, paths: &[PathBuf]) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(root).arg("add").arg("--");
    for p in paths {
        cmd.arg(p);
    }
    let out = cmd.output().context("git add")?;
    if !out.status.success() {
        bail!(
            "git add: {}",
            String::from_utf8_lossy(&out.stderr).trim().to_string()
        );
    }
    Ok(())
}

pub fn unstage(root: &Path, paths: &[PathBuf]) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(root)
        .arg("restore")
        .arg("--staged")
        .arg("--");
    for p in paths {
        cmd.arg(p);
    }
    let out = cmd.output().context("git restore --staged")?;
    if !out.status.success() {
        bail!(
            "git restore --staged: {}",
            String::from_utf8_lossy(&out.stderr).trim().to_string()
        );
    }
    Ok(())
}

pub fn discard(root: &Path, paths: &[PathBuf]) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(root).arg("restore").arg("--");
    for p in paths {
        cmd.arg(p);
    }
    let out = cmd.output().context("git restore")?;
    if !out.status.success() {
        bail!(
            "git restore: {}",
            String::from_utf8_lossy(&out.stderr).trim().to_string()
        );
    }
    Ok(())
}

pub fn commit(root: &Path, message: &str) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("commit")
        .arg("-m")
        .arg(message)
        .output()
        .context("git commit")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("{}", err.lines().next().unwrap_or("commit failed"));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let first = stdout
        .lines()
        .next()
        .unwrap_or("committed")
        .trim()
        .to_string();
    Ok(first)
}

pub fn diff_for(root: &Path, path: &Path) -> Result<Vec<String>> {
    let worktree = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("--no-pager")
        .arg("diff")
        .arg("--no-color")
        .arg("HEAD")
        .arg("--")
        .arg(path)
        .output()
        .context("git diff HEAD")?;
    let mut lines: Vec<String> = String::from_utf8_lossy(&worktree.stdout)
        .lines()
        .map(|s| s.to_string())
        .collect();
    if lines.is_empty() {
        let cached = Command::new("git")
            .arg("-C")
            .arg(root)
            .arg("--no-pager")
            .arg("diff")
            .arg("--no-color")
            .arg("--cached")
            .arg("--")
            .arg(path)
            .output()
            .context("git diff --cached")?;
        lines = String::from_utf8_lossy(&cached.stdout)
            .lines()
            .map(|s| s.to_string())
            .collect();
    }
    Ok(lines)
}

pub fn recent_log(root: &Path, path: Option<&Path>, limit: u32) -> Vec<String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(root)
        .arg("--no-pager")
        .arg("log")
        .arg(format!("-n{limit}"))
        .arg("--pretty=format:%h %s");
    if let Some(p) = path {
        cmd.arg("--").arg(p);
    }
    match cmd.output() {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(|s| s.to_string())
            .collect(),
        _ => Vec::new(),
    }
}
