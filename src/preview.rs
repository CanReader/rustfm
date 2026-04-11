use crate::{
    config::SortMode,
    fs_ops::{list_dir, Entry},
    git::{self, GitInfo},
};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol};
use std::{
    fs::File,
    io::{BufRead, BufReader, Read},
    path::Path,
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
        .args(["-png", "-r", "200", "-f", &page_s, "-l", &page_s, "-singlefile", "-aa", "yes", "-aaVector", "yes"])
        .arg(path)
        .arg(&prefix)
        .status();
    match status {
        Ok(s) if s.success() && out_path.exists() => {
            let res = image::ImageReader::open(&out_path)
                .and_then(|r| Ok(r.with_guessed_format()?))
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

pub fn generate(
    path: &Path,
    show_hidden: bool,
    picker: Option<&mut Picker>,
    git_info: Option<&GitInfo>,
    diff_mode: bool,
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
                match image::ImageReader::open(path).and_then(|r| Ok(r.with_guessed_format()?)) {
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
        return Preview::Binary(format!(
            "binary file ({}) — {} bytes",
            mime,
            meta.len()
        ));
    }
    Preview::Text(lines)
}
