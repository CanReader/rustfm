use crate::{
    config::SortMode,
    fs_ops::{list_dir, Entry},
};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol};
use std::{
    fs::File,
    io::{BufRead, BufReader, Read},
    path::Path,
};

pub enum Preview {
    Text(Vec<String>),
    Dir(Vec<Entry>),
    Image(StatefulProtocol),
    Binary(String),
    Empty,
    Unreadable(String),
}

const MAX_LINES: usize = 400;
const MAX_BYTES: usize = 64 * 1024;

const IMAGE_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "bmp", "webp", "tiff", "tif", "ico", "avif", "qoi",
];

pub fn generate(path: &Path, show_hidden: bool, picker: Option<&mut Picker>) -> Preview {
    if path.is_dir() {
        return match list_dir(path, show_hidden, SortMode::Name, false, true) {
            Ok(v) if v.is_empty() => Preview::Empty,
            Ok(v) => Preview::Dir(v),
            Err(e) => Preview::Unreadable(e.to_string()),
        };
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
