use crate::{
    config::SortMode,
    fs_ops::{list_dir, Entry},
    git::{self, GitInfo},
};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol};
use std::{
    fs::File,
    io::{BufRead, BufReader, Read},
    path::Path,
    sync::OnceLock,
};
use syntect::{
    easy::HighlightLines,
    highlighting::{FontStyle, ThemeSet},
    parsing::SyntaxSet,
};

pub fn pdf_page_count(path: &Path) -> u32 {
    let out = std::process::Command::new("pdfinfo").arg(path).output();
    let Ok(out) = out else { return 1 };
    if !out.status.success() {
        return 1;
    }
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        if let Some(rest) = line.strip_prefix("Pages:") {
            return rest.trim().parse().unwrap_or(1);
        }
    }
    1
}

pub fn render_pdf_page(path: &Path, page: u32, picker: Option<&mut Picker>) -> Preview {
    let Some(picker) = picker else {
        return Preview::Binary("pdf (graphics unavailable in terminal)".into());
    };
    let tmp_dir = std::env::temp_dir();
    let prefix = tmp_dir.join(format!("rustfm_pdf_{}", std::process::id()));
    let out_path = tmp_dir.join(format!("rustfm_pdf_{}.png", std::process::id()));
    let _ = std::fs::remove_file(&out_path);

    let page_s = page.to_string();
    let status = std::process::Command::new("pdftoppm")
        .args([
            "-png",
            "-r",
            "200",
            "-f",
            &page_s,
            "-l",
            &page_s,
            "-singlefile",
            "-aa",
            "yes",
            "-aaVector",
            "yes",
        ])
        .arg(path)
        .arg(&prefix)
        .status();
    match status {
        Ok(s) if s.success() && out_path.exists() => {
            let res = image::ImageReader::open(&out_path)
                .and_then(|r| r.with_guessed_format())
                .map_err(|e| format!("pdf open: {e}"))
                .and_then(|r| r.decode().map_err(|e| format!("pdf decode: {e}")));
            let _ = std::fs::remove_file(&out_path);
            match res {
                Ok(img) => Preview::Image(picker.new_resize_protocol(img)),
                Err(e) => Preview::Unreadable(e),
            }
        }
        Ok(_) => Preview::Binary("pdf (pdftoppm failed)".into()),
        Err(_) => Preview::Binary("pdf (install poppler-utils for preview)".into()),
    }
}

pub enum Preview {
    Text(Vec<String>),
    /// Syntax-highlighted source code, pre-styled lines.
    Code(Vec<Line<'static>>),
    Dir(Vec<Entry>),
    Image(StatefulProtocol),
    Diff(Vec<String>),
    Binary(String),
    Empty,
    Unreadable(String),
}

const MAX_LINES: usize = 400;
const MAX_BYTES: usize = 64 * 1024;

const IMAGE_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "bmp", "webp", "tiff", "tif", "ico", "avif", "qoi",
];

fn syntax_set() -> &'static SyntaxSet {
    static SS: OnceLock<SyntaxSet> = OnceLock::new();
    SS.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme_set() -> &'static ThemeSet {
    static TS: OnceLock<ThemeSet> = OnceLock::new();
    TS.get_or_init(ThemeSet::load_defaults)
}

pub fn generate(
    path: &Path,
    show_hidden: bool,
    picker: Option<&mut Picker>,
    git_info: Option<&GitInfo>,
    diff_mode: bool,
    syntax_theme: &str,
) -> Preview {
    if path.is_dir() {
        return match list_dir(path, show_hidden, SortMode::Name, false, true) {
            Ok(v) if v.is_empty() => Preview::Empty,
            Ok(v) => Preview::Dir(v),
            Err(e) => Preview::Unreadable(e.to_string()),
        };
    }

    if diff_mode {
        if let Some(info) = git_info {
            if let Some(fs) = info.status.get(path) {
                if fs.is_dirty() && !fs.is_untracked() {
                    match git::diff_for(&info.root, path) {
                        Ok(lines) if !lines.is_empty() => return Preview::Diff(lines),
                        Ok(_) => {}
                        Err(e) => return Preview::Unreadable(format!("git diff: {e}")),
                    }
                }
            }
        }
    }

    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let ext_l = ext.to_ascii_lowercase();
        if IMAGE_EXTS.contains(&ext_l.as_str()) {
            if let Some(picker) = picker {
                match image::ImageReader::open(path).and_then(|r| r.with_guessed_format()) {
                    Ok(reader) => match reader.decode() {
                        Ok(img) => return Preview::Image(picker.new_resize_protocol(img)),
                        Err(e) => return Preview::Unreadable(format!("image decode: {e}")),
                    },
                    Err(e) => return Preview::Unreadable(format!("image open: {e}")),
                }
            } else {
                return Preview::Binary("image (graphics unavailable in terminal)".into());
            }
        }
        if ext_l == "pdf" {
            return render_pdf_page(path, 1, picker);
        }
    }

    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => return Preview::Unreadable(e.to_string()),
    };
    let meta = match file.metadata() {
        Ok(m) => m,
        Err(e) => return Preview::Unreadable(e.to_string()),
    };
    if meta.len() == 0 {
        return Preview::Empty;
    }
    let reader = BufReader::new(file.take(MAX_BYTES as u64));
    let mut lines: Vec<String> = Vec::with_capacity(64);
    let mut binary = false;
    for (i, line) in reader.lines().enumerate() {
        if i >= MAX_LINES {
            break;
        }
        match line {
            Ok(l) => {
                if l.as_bytes().contains(&0) {
                    binary = true;
                    break;
                }
                lines.push(l);
            }
            Err(_) => {
                binary = true;
                break;
            }
        }
    }
    if binary {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return Preview::Binary(format!("binary file ({}) — {} bytes", mime, meta.len()));
    }

    // Try syntax highlighting; fall back to plain text if the file type is
    // unknown to syntect.
    if let Some(code) = highlight(path, &lines, syntax_theme) {
        return Preview::Code(code);
    }
    Preview::Text(lines)
}

/// Syntax-highlight `lines` with syntect, converting to owned ratatui lines.
/// Returns None when no syntax definition matches the file.
fn highlight(path: &Path, lines: &[String], theme_name: &str) -> Option<Vec<Line<'static>>> {
    let ss = syntax_set();
    let syntax = path
        .extension()
        .and_then(|e| e.to_str())
        .and_then(|ext| ss.find_syntax_by_extension(ext))
        .or_else(|| {
            path.file_name()
                .and_then(|n| n.to_str())
                .and_then(|name| ss.find_syntax_by_extension(name))
        })
        .or_else(|| lines.first().and_then(|l| ss.find_syntax_by_first_line(l)))?;
    // Plain text syntax means nothing to highlight — keep the cheap path.
    if syntax.name == "Plain Text" {
        return None;
    }

    let ts = theme_set();
    let theme = ts
        .themes
        .get(theme_name)
        .or_else(|| ts.themes.get("base16-eighties.dark"))
        .or_else(|| ts.themes.values().next())?;

    let mut hl = HighlightLines::new(syntax, theme);
    let mut out: Vec<Line<'static>> = Vec::with_capacity(lines.len());
    for raw in lines {
        let Ok(ranges) = hl.highlight_line(raw, ss) else {
            out.push(Line::raw(raw.clone()));
            continue;
        };
        let spans: Vec<Span<'static>> = ranges
            .into_iter()
            .map(|(style, text)| {
                let fg = style.foreground;
                let mut s = Style::default().fg(Color::Rgb(fg.r, fg.g, fg.b));
                if style.font_style.contains(FontStyle::BOLD) {
                    s = s.add_modifier(Modifier::BOLD);
                }
                if style.font_style.contains(FontStyle::ITALIC) {
                    s = s.add_modifier(Modifier::ITALIC);
                }
                if style.font_style.contains(FontStyle::UNDERLINE) {
                    s = s.add_modifier(Modifier::UNDERLINED);
                }
                Span::styled(text.to_string(), s)
            })
            .collect();
        out.push(Line::from(spans));
    }
    Some(out)
}
