use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// Copies a list of file paths to the system clipboard as a `text/uri-list`
/// payload so that file managers (Nautilus, Dolphin, Finder) and browsers can
/// paste the actual files, not just their string paths. Falls back to plain
/// newline-separated paths on tools that don't negotiate MIME types.
///
/// Returns the name of the backend tool used, or an error string describing
/// why the copy failed (no tool on PATH, spawn error, non-zero exit).
pub fn copy_files(paths: &[impl AsRef<Path>]) -> Result<&'static str, String> {
    if paths.is_empty() {
        return Err("nothing to copy".into());
    }
    let uri_list: String = paths
        .iter()
        .map(|p| path_to_uri(p.as_ref()))
        .collect::<Vec<_>>()
        .join("\n");
    let plain: String = paths
        .iter()
        .map(|p| p.as_ref().display().to_string())
        .collect::<Vec<_>>()
        .join("\n");

    let on_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();

    if on_wayland && which("wl-copy") {
        run_with_stdin("wl-copy", &["--type", "text/uri-list"], uri_list.as_bytes())?;
        return Ok("wl-copy");
    }
    if which("xclip") {
        run_with_stdin(
            "xclip",
            &["-selection", "clipboard", "-t", "text/uri-list"],
            uri_list.as_bytes(),
        )?;
        return Ok("xclip");
    }
    if which("wl-copy") {
        run_with_stdin("wl-copy", &["--type", "text/uri-list"], uri_list.as_bytes())?;
        return Ok("wl-copy");
    }
    if which("xsel") {
        run_with_stdin("xsel", &["--clipboard", "--input"], plain.as_bytes())?;
        return Ok("xsel");
    }
    if which("pbcopy") {
        run_with_stdin("pbcopy", &[], plain.as_bytes())?;
        return Ok("pbcopy");
    }
    if which("clip.exe") {
        run_with_stdin("clip.exe", &[], plain.as_bytes())?;
        return Ok("clip.exe");
    }
    Err("no clipboard tool on PATH (install wl-clipboard, xclip, xsel, or pbcopy)".into())
}

/// Copies plain UTF-8 text to the system clipboard. Used for actions like
/// "copy path" or "copy filename" that want a shell-friendly string rather
/// than a file handle.
pub fn copy_text(text: &str) -> Result<&'static str, String> {
    let on_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    if on_wayland && which("wl-copy") {
        run_with_stdin("wl-copy", &[], text.as_bytes())?;
        return Ok("wl-copy");
    }
    if which("xclip") {
        run_with_stdin("xclip", &["-selection", "clipboard"], text.as_bytes())?;
        return Ok("xclip");
    }
    if which("wl-copy") {
        run_with_stdin("wl-copy", &[], text.as_bytes())?;
        return Ok("wl-copy");
    }
    if which("xsel") {
        run_with_stdin("xsel", &["--clipboard", "--input"], text.as_bytes())?;
        return Ok("xsel");
    }
    if which("pbcopy") {
        run_with_stdin("pbcopy", &[], text.as_bytes())?;
        return Ok("pbcopy");
    }
    if which("clip.exe") {
        run_with_stdin("clip.exe", &[], text.as_bytes())?;
        return Ok("clip.exe");
    }
    Err("no clipboard tool on PATH".into())
}

fn path_to_uri(p: &Path) -> String {
    let s = p.display().to_string();
    let mut out = String::from("file://");
    for b in s.bytes() {
        match b {
            b'/' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b if b.is_ascii_alphanumeric() => out.push(b as char),
            b => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn run_with_stdin(prog: &str, args: &[&str], data: &[u8]) -> Result<(), String> {
    let mut child = Command::new(prog)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{prog}: spawn failed: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(data)
            .map_err(|e| format!("{prog}: write failed: {e}"))?;
    }
    let status = child
        .wait()
        .map_err(|e| format!("{prog}: wait failed: {e}"))?;
    if !status.success() {
        return Err(format!("{prog} exited with {:?}", status.code()));
    }
    Ok(())
}

fn which(prog: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| {
        let candidate = dir.join(prog);
        candidate.is_file()
    })
}
