use chrono::{DateTime, Local};
use humansize::{format_size, BINARY, DECIMAL};
use std::fs;

use crate::fs_ops::Entry;

/// Key/value pairs shown in the Info footer pane.
pub fn collect(entry: &Entry, date_format: &str, use_si: bool) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    out.push(("Name".into(), entry.name.clone()));

    let kind = if entry.is_symlink {
        "symlink".to_string()
    } else if entry.is_dir {
        "directory".to_string()
    } else {
        mime_guess::from_path(&entry.path)
            .first()
            .map(|m| m.to_string())
            .unwrap_or_else(|| "file".into())
    };
    out.push(("Type".into(), kind));

    if entry.is_dir {
        if let Ok(rd) = fs::read_dir(&entry.path) {
            out.push(("Items".into(), rd.count().to_string()));
        }
    } else {
        let human = if use_si {
            format_size(entry.size, DECIMAL)
        } else {
            format_size(entry.size, BINARY)
        };
        out.push(("Size".into(), format!("{human} ({} B)", entry.size)));
    }

    if let Ok(meta) = fs::symlink_metadata(&entry.path) {
        if let Ok(t) = meta.modified() {
            let dt: DateTime<Local> = t.into();
            out.push(("Modified".into(), dt.format(date_format).to_string()));
        }
        if let Ok(t) = meta.accessed() {
            let dt: DateTime<Local> = t.into();
            out.push(("Accessed".into(), dt.format(date_format).to_string()));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            out.push(("Permissions".into(), permission_string(&meta)));
            out.push(("Owner".into(), user_name(meta.uid())));
            out.push(("Group".into(), group_name(meta.gid())));
        }
    }

    if entry.is_symlink {
        if let Ok(target) = fs::read_link(&entry.path) {
            out.push(("Links to".into(), target.display().to_string()));
        }
    }

    out.push(("Path".into(), entry.path.display().to_string()));
    out
}

#[cfg(unix)]
fn permission_string(meta: &fs::Metadata) -> String {
    use std::os::unix::fs::MetadataExt;
    let mode = meta.mode();
    let kind = match mode & 0o170000 {
        0o040000 => 'd',
        0o120000 => 'l',
        0o140000 => 's',
        0o060000 => 'b',
        0o020000 => 'c',
        0o010000 => 'p',
        _ => '-',
    };
    let mut s = String::with_capacity(10);
    s.push(kind);
    for shift in [6u32, 3, 0] {
        let bits = (mode >> shift) & 0o7;
        s.push(if bits & 0o4 != 0 { 'r' } else { '-' });
        s.push(if bits & 0o2 != 0 { 'w' } else { '-' });
        s.push(if bits & 0o1 != 0 { 'x' } else { '-' });
    }
    format!("{s} ({:o})", mode & 0o7777)
}

/// Resolve a uid to a name via /etc/passwd; falls back to the number.
/// Tables are parsed once and cached — metadata refreshes on every cursor
/// rest, and re-reading the files each time is wasted work.
#[cfg(unix)]
fn user_name(uid: u32) -> String {
    static USERS: std::sync::OnceLock<std::collections::HashMap<u32, String>> =
        std::sync::OnceLock::new();
    USERS
        .get_or_init(|| parse_id_db("/etc/passwd"))
        .get(&uid)
        .cloned()
        .unwrap_or_else(|| uid.to_string())
}

#[cfg(unix)]
fn group_name(gid: u32) -> String {
    static GROUPS: std::sync::OnceLock<std::collections::HashMap<u32, String>> =
        std::sync::OnceLock::new();
    GROUPS
        .get_or_init(|| parse_id_db("/etc/group"))
        .get(&gid)
        .cloned()
        .unwrap_or_else(|| gid.to_string())
}

#[cfg(unix)]
fn parse_id_db(db: &str) -> std::collections::HashMap<u32, String> {
    let mut map = std::collections::HashMap::new();
    let Ok(contents) = fs::read_to_string(db) else {
        return map;
    };
    for line in contents.lines() {
        let mut fields = line.split(':');
        let Some(name) = fields.next() else { continue };
        let _pw = fields.next();
        let Some(id) = fields.next().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        map.entry(id).or_insert_with(|| name.to_string());
    }
    map
}
