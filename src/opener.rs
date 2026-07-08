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

/// How the user wants an executable to launch. `Background` is the default
/// "double-click" behaviour: spawn detached, no TUI takeover, no waiting.
/// `Interactive` keeps the legacy stdio-inherited behaviour so the user can
/// see logs, prompts, or errors before returning to the file manager.
///
/// This only meaningfully affects executables. Internal opener mappings and
/// the OS-default fallback ignore it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenMode {
    Background,
    Interactive,
}

pub enum OpenOutcome {
    /// Ran an internal command that needs a terminal (spawned foreground).
    Internal,
    /// Handed off to the OS default opener in the background.
    OsDefault,
    /// Spawned an executable detached. No TTY interaction; child outlives us.
    BackgroundSpawned,
}

impl<'a> Opener<'a> {
    pub fn new(table: &'a HashMap<String, String>) -> Self {
        Self { table }
    }

    pub fn lookup(&self, path: &Path) -> Option<&String> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        self.table.get(&ext)
    }

    /// True when opening this path under `mode` will inherit the terminal —
    /// i.e. when the caller must suspend the TUI before invoking `open` and
    /// force a redraw afterwards. Caller probes this *before* `open` because
    /// the suspend/resume dance has to wrap the call.
    pub fn will_take_tty(&self, path: &Path, mode: OpenMode) -> bool {
        if self.lookup(path).is_some() {
            return true;
        }
        if is_executable(path) && matches!(mode, OpenMode::Interactive) {
            return true;
        }
        false
    }

    /// Primary entry point. Consults internal map first; otherwise, if the
    /// file has its executable bit set, run it (interactive) or spawn it
    /// detached (background); otherwise delegate to the OS default handler.
    pub fn open(&self, path: &Path, mode: OpenMode) -> Result<OpenOutcome> {
        if let Some(template) = self.lookup(path) {
            run_template(template, path)?;
            return Ok(OpenOutcome::Internal);
        }
        if is_executable(path) {
            return match mode {
                OpenMode::Interactive => {
                    run_executable_interactive(path)?;
                    Ok(OpenOutcome::Internal)
                }
                OpenMode::Background => {
                    spawn_executable_detached(path)?;
                    Ok(OpenOutcome::BackgroundSpawned)
                }
            };
        }
        open::that_detached(path).context("OS default opener failed")?;
        Ok(OpenOutcome::OsDefault)
    }
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && (m.permissions().mode() & 0o111) != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            matches!(
                e.to_ascii_lowercase().as_str(),
                "exe" | "bat" | "cmd" | "com"
            )
        })
        .unwrap_or(false)
}

fn run_executable_interactive(path: &Path) -> Result<()> {
    let status = Command::new(path)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("executing {}", path.display()))?;
    // open_current has already left the alternate screen, so anything the
    // program printed is on the real terminal. Pause so the user can read
    // it before Rustfm's TUI redraws over the output on return.
    use std::io::{Read, Write};
    let mut out = std::io::stdout();
    let _ = writeln!(out);
    let _ = write!(out, "[press Enter to return]");
    let _ = out.flush();
    let _ = std::io::stdin().read(&mut [0u8; 1]);
    if !status.success() {
        anyhow::bail!("exited with {status}");
    }
    Ok(())
}

#[cfg(unix)]
fn spawn_executable_detached(path: &Path) -> Result<()> {
    use std::os::unix::process::CommandExt;
    let mut cmd = Command::new(path);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // Detach from rustfm's controlling terminal and process group via setsid
    // so that SIGHUP on terminal close and SIGINT on Ctrl-C don't propagate
    // to the child, and the child can outlive rustfm cleanly. Direct extern
    // declaration avoids pulling in the libc crate as a dependency.
    unsafe {
        cmd.pre_exec(|| {
            extern "C" {
                fn setsid() -> i32;
            }
            setsid();
            Ok(())
        });
    }
    cmd.spawn()
        .with_context(|| format!("spawning {}", path.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn spawn_executable_detached(path: &Path) -> Result<()> {
    Command::new(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("spawning {}", path.display()))?;
    Ok(())
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
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || "/._-+=@".contains(c))
    {
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
