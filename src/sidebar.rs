use std::path::PathBuf;

/// One row in the sidebar.
pub enum SideItem {
    /// Section divider rendered as `─ label ────`.
    Divider(&'static str),
    Dir {
        name: String,
        path: PathBuf,
        icon: &'static str,
    },
}

impl SideItem {
    pub fn is_dir(&self) -> bool {
        matches!(self, SideItem::Dir { .. })
    }
}

/// Sidebar: well-known home directories, then a "Pinned"
/// section, then a "Disks" section listing mounted volumes.
pub struct Sidebar {
    pub items: Vec<SideItem>,
    pub cursor: usize,
}

impl Sidebar {
    pub fn build(pinned: &[String]) -> Self {
        let mut items: Vec<SideItem> = Vec::new();

        let well_known: [(Option<PathBuf>, &str, &str); 7] = [
            (dirs::home_dir(), "\u{f015}", "Home"),          //
            (dirs::desktop_dir(), "\u{f108}", "Desktop"),    //
            (dirs::document_dir(), "\u{f0f6}", "Documents"), //
            (dirs::download_dir(), "\u{f019}", "Downloads"), //
            (dirs::audio_dir(), "\u{f001}", "Music"),        //
            (dirs::picture_dir(), "\u{f03e}", "Pictures"),   //
            (dirs::video_dir(), "\u{f03d}", "Videos"),       //
        ];
        for (path, icon, name) in well_known {
            if let Some(p) = path {
                if p.is_dir() {
                    items.push(SideItem::Dir {
                        name: name.to_string(),
                        path: p,
                        icon,
                    });
                }
            }
        }

        items.push(SideItem::Divider("Pinned"));
        for raw in pinned {
            let expanded = expand_tilde(raw);
            let path = PathBuf::from(&expanded);
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| expanded.clone());
            items.push(SideItem::Dir {
                name,
                path,
                icon: "\u{f08d}", //
            });
        }

        items.push(SideItem::Divider("Disks"));
        for (name, path) in disks() {
            items.push(SideItem::Dir {
                name,
                path,
                icon: "\u{f0a0}", //
            });
        }

        let mut sb = Self { items, cursor: 0 };
        // Land the cursor on the first real directory.
        if sb
            .items
            .get(sb.cursor)
            .map(|i| !i.is_dir())
            .unwrap_or(false)
        {
            sb.move_cursor(1);
        }
        sb
    }

    pub fn current(&self) -> Option<(&str, &PathBuf)> {
        match self.items.get(self.cursor) {
            Some(SideItem::Dir { name, path, .. }) => Some((name.as_str(), path)),
            _ => None,
        }
    }

    /// Move the cursor up/down, skipping dividers.
    pub fn move_cursor(&mut self, delta: i64) {
        if self.items.is_empty() {
            return;
        }
        let len = self.items.len() as i64;
        let step = if delta >= 0 { 1i64 } else { -1i64 };
        let mut pos = self.cursor as i64;
        let mut remaining = delta.abs();
        while remaining > 0 {
            let mut next = pos + step;
            while next >= 0 && next < len {
                if self.items[next as usize].is_dir() {
                    break;
                }
                next += step;
            }
            if next < 0 || next >= len {
                break;
            }
            pos = next;
            remaining -= 1;
        }
        self.cursor = pos as usize;
    }
}

fn expand_tilde(input: &str) -> String {
    if let Some(rest) = input.strip_prefix('~') {
        if let Some(home) = dirs::home_dir() {
            return format!("{}{}", home.display(), rest);
        }
    }
    input.into()
}

/// Mounted volumes for the sidebar "Disks" section.
#[cfg(windows)]
fn disks() -> Vec<(String, PathBuf)> {
    // Probe drive letters; std has no volume enumeration API.
    let mut out = Vec::new();
    for letter in b'A'..=b'Z' {
        let root = format!("{}:\\", letter as char);
        if PathBuf::from(&root).is_dir() {
            out.push((format!("{}:", letter as char), PathBuf::from(root)));
        }
    }
    out
}

/// Mounted volumes for the sidebar "Disks" section. Parses /proc/mounts and
/// keeps real block-device filesystems, skipping boot/efi partitions.
#[cfg(not(windows))]
fn disks() -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    let Ok(contents) = std::fs::read_to_string("/proc/mounts") else {
        // Non-Linux fallback: just the filesystem root.
        return vec![("root".into(), PathBuf::from("/"))];
    };
    for line in contents.lines() {
        let mut parts = line.split_whitespace();
        let (Some(device), Some(mount)) = (parts.next(), parts.next()) else {
            continue;
        };
        if !device.starts_with("/dev/") {
            continue;
        }
        let mount = decode_mount(mount);
        if mount.starts_with("/boot") || mount.starts_with("/efi") {
            continue;
        }
        if out.iter().any(|(_, p)| p == &PathBuf::from(&mount)) {
            continue;
        }
        let name = if mount == "/" {
            "root".to_string()
        } else {
            PathBuf::from(&mount)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| mount.clone())
        };
        out.push((name, PathBuf::from(mount)));
    }
    if out.is_empty() {
        out.push(("root".into(), PathBuf::from("/")));
    }
    // Root first, then alphabetical.
    out.sort_by(|a, b| {
        let a_root = a.1 == *"/";
        let b_root = b.1 == *"/";
        b_root
            .cmp(&a_root)
            .then(a.0.to_lowercase().cmp(&b.0.to_lowercase()))
    });
    out
}

/// /proc/mounts escapes spaces as \040, tabs as \011, etc. Decode at the
/// byte level so multi-byte UTF-8 in mount names survives.
#[cfg(not(windows))]
fn decode_mount(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len() {
            if let Ok(code) = u8::from_str_radix(&s[i + 1..i + 4], 8) {
                out.push(code);
                i += 4;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
