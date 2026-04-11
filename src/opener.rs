use anyhow::{Context, Result};
use std::{
    collections::HashMap,
    path::Path,
    process::{Command, Stdio},
};

/// Opens a path using the internal default-app map first,
/// falling back to the OS default handler if no internal match exists.
pub struct Opener<'a> {
    pub table: &'a HashMap<String, String>,
}

pub enum OpenOutcome {
    /// Ran an internal command that needs a terminal (spawned foreground).
    Internal,
    /// Handed off to the OS default opener in the background.
    OsDefault,
}

impl<'a> Opener<'a> {
    pub fn new(table: &'a HashMap<String, String>) -> Self {
        Self { table }
    }

    pub fn lookup(&self, path: &Path) -> Option<&String> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        self.table.get(&ext)
    }

    /// Primary entry point. Consults internal map first; otherwise delegates to the OS.
    pub fn open(&self, path: &Path) -> Result<OpenOutcome> {
        if let Some(template) = self.lookup(path) {
            run_template(template, path)?;
            return Ok(OpenOutcome::Internal);
        }
        open::that_detached(path).context("OS default opener failed")?;
        Ok(OpenOutcome::OsDefault)
    }
}

fn run_template(template: &str, path: &Path) -> Result<()> {
    let path_str = path.to_string_lossy().to_string();
    let rendered = if template.contains("{}") {
        template.replace("{}", &shell_escape(&path_str))
    } else {
        format!("{template} {}", shell_escape(&path_str))
    };
    let mut parts = shell_split(&rendered);
    if parts.is_empty() {
        anyhow::bail!("empty opener command");
    }
    let program = parts.remove(0);
    let status = Command::new(&program)
        .args(&parts)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("spawning {program}"))?;
    if !status.success() {
        anyhow::bail!("{program} exited with {status}");
    }
    Ok(())
}

fn shell_escape(s: &str) -> String {
    if s.chars().all(|c| c.is_ascii_alphanumeric() || "/._-+=@".contains(c)) {
        s.into()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

fn shell_split(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '\\' if !in_single => {
                if let Some(next) = chars.next() {
                    cur.push(next);
                }
            }
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
