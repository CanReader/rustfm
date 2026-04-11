use chrono::{DateTime, Local};
use humansize::{format_size, BINARY};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use ratatui_image::StatefulImage;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use crate::{
    app::{App, FuzzyMatch, Mode, Progress, PromptKind},
    config::SortMode,
    fs_ops::Entry,
    git::GitStatus,
    preview::Preview,
    theme::Palette,
};

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let mut constraints = vec![
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ];
    if app.progress.is_some() {
        constraints.insert(3, Constraint::Length(1));
    }
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    draw_header(f, vertical[0], app);
    draw_panes(f, vertical[1], app);
    draw_info(f, vertical[2], app);

    let mut idx = 3;
    if app.progress.is_some() {
        if let Some(p) = &app.progress {
            draw_progress(f, vertical[idx], p, &app.palette);
        }
        idx += 1;
    }
    draw_footer(f, vertical[idx], app);

    if app.mode == Mode::Fuzzy {
        draw_fuzzy(f, area, app);
    }
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let pal = &app.palette;
    let home = dirs::home_dir().map(|h| h.display().to_string()).unwrap_or_default();
    let cwd_disp = app.cwd.display().to_string();
    let shown = if !home.is_empty() && cwd_disp.starts_with(&home) {
        format!("~{}", &cwd_disp[home.len()..])
    } else {
        cwd_disp
    };
    let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
    let host = hostname();
    let sort_label = match app.config.sort {
        SortMode::Name => "name",
        SortMode::Size => "size",
        SortMode::Mtime => "mtime",
        SortMode::Ext => "ext",
    };
    let rev = if app.config.sort_reverse { "↓" } else { "↑" };

    let mut spans = vec![
        Span::styled(
            format!("{user}@{host}"),
            Style::default().fg(pal.header_user).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            shown,
            Style::default().fg(pal.header_path).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            app.current_entry()
                .map(|e| format!("/{}", e.name))
                .unwrap_or_default(),
            Style::default().fg(pal.file),
        ),
    ];
    if !app.filter.is_empty() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("[filter: {}]", app.filter),
            Style::default().fg(pal.info_accent),
        ));
    }
    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        format!("[{sort_label}{rev}]"),
        Style::default().fg(pal.info_dim),
    ));
    if app.git_root.is_some() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled("⎇ git", Style::default().fg(pal.git_added)));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_panes(f: &mut Frame, area: Rect, app: &mut App) {
    let r = app.config.ratios;
    let total = (r[0] + r[1] + r[2]) as u32;
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(r[0] as u32, total),
            Constraint::Ratio(r[1] as u32, total),
            Constraint::Ratio(r[2] as u32, total),
        ])
        .split(area);

    let parent_area = chunks[0];
    let current_area = chunks[1];
    let preview_area = chunks[2];

    // Disjoint field borrows: no cloning.
    draw_entries(
        f,
        parent_area,
        &app.parent_entries,
        app.parent_cursor,
        &app.selected,
        &app.git_status,
        &app.palette,
        app.config.icons,
        false,
    );
    draw_entries(
        f,
        current_area,
        &app.entries,
        app.cursor,
        &app.selected,
        &app.git_status,
        &app.palette,
        app.config.icons,
        true,
    );
    draw_preview(f, preview_area, &mut app.preview, &app.palette, app.config.icons);
}

fn draw_entries(
    f: &mut Frame,
    area: Rect,
    entries: &[Entry],
    cursor: usize,
    selected: &HashSet<PathBuf>,
    git: &HashMap<PathBuf, GitStatus>,
    pal: &Palette,
    icons_enabled: bool,
    active: bool,
) {
    let items: Vec<ListItem> = entries
        .iter()
        .map(|e| {
            let icon = if icons_enabled { icon_for(e) } else { "" };
            let is_sel = selected.contains(&e.path);
            let mut style = entry_style(e, pal);
            if is_sel {
                style = style.add_modifier(Modifier::REVERSED);
            }
            let marker = if is_sel { "*" } else { " " };
            let (git_marker, git_color) = match git.get(&e.path) {
                Some(s) => (s.label(), git_color(*s, pal)),
                None => (" ", Color::Reset),
            };
            let name = if e.is_dir {
                format!("{}/", e.name)
            } else {
                e.name.clone()
            };
            ListItem::new(Line::from(vec![
                Span::raw(marker),
                Span::styled(git_marker, Style::default().fg(git_color)),
                Span::raw(" "),
                Span::raw(icon),
                Span::raw(" "),
                Span::styled(name, style),
            ]))
        })
        .collect();

    let border_style = if active {
        Style::default().fg(pal.active_border)
    } else {
        Style::default().fg(pal.inactive_border)
    };
    let block = Block::default().borders(Borders::ALL).border_style(border_style);

    let mut state = ListState::default();
    if !entries.is_empty() {
        state.select(Some(cursor.min(entries.len() - 1)));
    }
    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(pal.cursor_bg)
                .fg(pal.cursor_fg)
                .add_modifier(Modifier::BOLD),
        );
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_preview(f: &mut Frame, area: Rect, preview: &mut Preview, pal: &Palette, icons_enabled: bool) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(pal.inactive_border))
        .title(" preview ");
    let inner = block.inner(area);
    f.render_widget(block, area);

    match preview {
        Preview::Text(lines) => {
            let joined = lines
                .iter()
                .take(inner.height as usize)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            let p = Paragraph::new(joined).wrap(Wrap { trim: false });
            f.render_widget(p, inner);
        }
        Preview::Dir(entries) => {
            let items: Vec<ListItem> = entries
                .iter()
                .take(inner.height as usize)
                .map(|e| {
                    let icon = if icons_enabled { icon_for(e) } else { "" };
                    let name = if e.is_dir {
                        format!("{}/", e.name)
                    } else {
                        e.name.clone()
                    };
                    ListItem::new(Line::from(vec![
                        Span::raw(icon),
                        Span::raw(" "),
                        Span::styled(name, entry_style(e, pal)),
                    ]))
                })
                .collect();
            f.render_widget(List::new(items), inner);
        }
        Preview::Image(proto) => {
            let image_widget = StatefulImage::default();
            f.render_stateful_widget(image_widget, inner, proto);
        }
        Preview::Binary(info) => {
            f.render_widget(
                Paragraph::new(info.as_str()).style(Style::default().fg(pal.info_dim)),
                inner,
            );
        }
        Preview::Empty => {
            f.render_widget(
                Paragraph::new("(empty)").style(Style::default().fg(pal.info_dim)),
                inner,
            );
        }
        Preview::Unreadable(e) => {
            f.render_widget(
                Paragraph::new(format!("unreadable: {e}")).style(Style::default().fg(pal.status_err)),
                inner,
            );
        }
    }
}

fn draw_info(f: &mut Frame, area: Rect, app: &App) {
    let pal = &app.palette;
    let Some(entry) = app.current_entry() else {
        f.render_widget(Paragraph::new(""), area);
        return;
    };
    let size = if entry.is_dir {
        "<dir>".into()
    } else {
        format_size(entry.size, BINARY)
    };
    let modified = entry
        .modified
        .and_then(|t| {
            let dt: DateTime<Local> = t.into();
            Some(dt.format(&app.config.date_format).to_string())
        })
        .unwrap_or_default();
    let opener_hint = opener_label(app, entry);
    let counts = format!(
        "{}/{}  sel:{}  clip:{}",
        app.cursor + 1,
        app.entries.len().max(1),
        app.selected.len(),
        app.clipboard.len()
    );
    let line = Line::from(vec![
        Span::styled(size, Style::default().fg(pal.info_size)),
        Span::raw("  "),
        Span::styled(modified, Style::default().fg(pal.info_dim)),
        Span::raw("  "),
        Span::styled(opener_hint, Style::default().fg(pal.info_accent)),
        Span::raw("  "),
        Span::styled(counts, Style::default().fg(pal.info_dim)),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn opener_label(app: &App, entry: &Entry) -> String {
    if entry.is_dir {
        return "dir".into();
    }
    let ext = entry.ext_lower.clone().unwrap_or_default();
    if app.config.openers.contains_key(&ext) {
        format!("opener:internal({ext})")
    } else {
        "opener:os-default".into()
    }
}

fn draw_progress(f: &mut Frame, area: Rect, p: &Progress, pal: &Palette) {
    let ratio = if p.total == 0 {
        0.0
    } else {
        (p.done as f64 / p.total as f64).min(1.0)
    };
    let label = format!("{} {}/{}  {}", p.label, p.done, p.total, p.current);
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(pal.progress_bar).bg(pal.progress_bg))
        .ratio(ratio)
        .label(label);
    f.render_widget(gauge, area);
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let pal = &app.palette;
    let text: Line = match app.mode {
        Mode::Normal => {
            if let Some(s) = &app.status {
                let color = if s.is_error { pal.status_err } else { pal.status_ok };
                Line::from(Span::styled(s.text.clone(), Style::default().fg(color)))
            } else {
                Line::from(Span::styled(
                    "q quit  ←↓↑→ nav  <enter> open  <space> sel  y/d/p yank/cut/paste  D del  r rename  a/A new  . hidden  / search  f filter  C-f fuzzy  o sort  R refresh  ' mark",
                    Style::default().fg(pal.info_dim),
                ))
            }
        }
        Mode::Search => Line::from(vec![Span::raw("/"), Span::raw(app.input.clone())]),
        Mode::Filter => Line::from(vec![
            Span::styled("filter:", Style::default().fg(pal.info_accent)),
            Span::raw(" "),
            Span::raw(app.input.clone()),
        ]),
        Mode::Fuzzy => Line::from(Span::styled(
            "fuzzy: <esc> cancel  <cr> jump",
            Style::default().fg(pal.info_dim),
        )),
        Mode::Sort => Line::from(Span::styled(
            "sort: n=name s=size t=mtime e=ext r=reverse",
            Style::default().fg(pal.info_dim),
        )),
        Mode::ConfirmDelete => Line::from(Span::styled(
            "delete selection? [y/N]",
            Style::default().fg(pal.status_err).add_modifier(Modifier::BOLD),
        )),
        Mode::Prompt(kind) => {
            let label = match kind {
                PromptKind::Rename => "rename:",
                PromptKind::NewFile => "new file:",
                PromptKind::NewDir => "new dir:",
                PromptKind::GoTo => "cd:",
                PromptKind::Bookmark => "bookmark:",
            };
            Line::from(vec![
                Span::styled(label, Style::default().fg(pal.info_size)),
                Span::raw(" "),
                Span::raw(app.input.clone()),
            ])
        }
    };
    f.render_widget(Paragraph::new(text), area);
}

fn draw_fuzzy(f: &mut Frame, area: Rect, app: &App) {
    let w = area.width.saturating_sub(8).min(80);
    let h = area.height.saturating_sub(4).min(20);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(x, y, w, h);
    f.render_widget(Clear, popup);

    let pal = &app.palette;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(pal.active_border))
        .title(" fuzzy — type to filter, <enter> to jump ");
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    let query = Paragraph::new(format!("> {}", app.input)).style(Style::default().fg(pal.overlay_fg));
    f.render_widget(query, layout[0]);

    let items: Vec<ListItem> = app
        .fuzzy_matches
        .iter()
        .take(layout[1].height as usize)
        .map(|m: &FuzzyMatch| {
            let entry = &app.entries[m.index];
            let mut spans = Vec::new();
            let positions: std::collections::HashSet<usize> =
                m.match_positions.iter().copied().collect();
            for (i, c) in entry.name.chars().enumerate() {
                let style = if positions.contains(&i) {
                    Style::default().fg(pal.overlay_match).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(pal.overlay_fg)
                };
                spans.push(Span::styled(c.to_string(), style));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items).highlight_style(
        Style::default()
            .bg(pal.cursor_bg)
            .fg(pal.cursor_fg)
            .add_modifier(Modifier::BOLD),
    );
    let mut state = ListState::default();
    if !app.fuzzy_matches.is_empty() {
        state.select(Some(0));
    }
    f.render_stateful_widget(list, layout[1], &mut state);
}

fn entry_style(e: &Entry, pal: &Palette) -> Style {
    if e.is_symlink {
        Style::default().fg(pal.symlink)
    } else if e.is_dir {
        Style::default().fg(pal.directory).add_modifier(Modifier::BOLD)
    } else if e.readonly {
        Style::default().fg(pal.readonly)
    } else {
        Style::default().fg(pal.file)
    }
}

fn git_color(status: GitStatus, pal: &Palette) -> Color {
    match status {
        GitStatus::Modified => pal.git_modified,
        GitStatus::Added | GitStatus::Renamed => pal.git_added,
        GitStatus::Deleted | GitStatus::Conflict => pal.git_deleted,
        GitStatus::Untracked => pal.git_untracked,
        GitStatus::Ignored => pal.git_ignored,
    }
}

// Nerd Font icons. Requires a Nerd Font patched terminal font.
fn icon_for(e: &Entry) -> &'static str {
    if e.is_symlink {
        return "\u{f481}";
    }
    if e.is_dir {
        return "\u{f07b}";
    }
    match e.ext_lower.as_deref() {
        Some("rs") => "\u{e7a8}",
        Some("go") => "\u{e627}",
        Some("py") => "\u{e73c}",
        Some("js") | Some("mjs") | Some("cjs") => "\u{e74e}",
        Some("ts") | Some("tsx") => "\u{e628}",
        Some("jsx") => "\u{e7ba}",
        Some("html") | Some("htm") => "\u{e736}",
        Some("css") | Some("scss") | Some("sass") => "\u{e749}",
        Some("md") | Some("markdown") => "\u{f48a}",
        Some("toml") => "\u{e6b2}",
        Some("yaml") | Some("yml") => "\u{e6a8}",
        Some("json") => "\u{e60b}",
        Some("lock") => "\u{f023}",
        Some("sh") | Some("bash") | Some("zsh") | Some("fish") => "\u{f489}",
        Some("c") | Some("h") => "\u{e61e}",
        Some("cpp") | Some("cc") | Some("hpp") | Some("hh") => "\u{e61d}",
        Some("java") => "\u{e738}",
        Some("rb") => "\u{e739}",
        Some("php") => "\u{e73d}",
        Some("lua") => "\u{e620}",
        Some("png") | Some("jpg") | Some("jpeg") | Some("gif") | Some("webp")
        | Some("bmp") | Some("tiff") | Some("ico") | Some("svg") | Some("avif") => "\u{f1c5}",
        Some("mp4") | Some("mkv") | Some("webm") | Some("mov") | Some("avi") => "\u{f1c8}",
        Some("mp3") | Some("flac") | Some("wav") | Some("ogg") | Some("m4a") => "\u{f1c7}",
        Some("zip") | Some("tar") | Some("gz") | Some("xz") | Some("zst") | Some("bz2")
        | Some("7z") | Some("rar") => "\u{f1c6}",
        Some("pdf") => "\u{f1c1}",
        Some("doc") | Some("docx") => "\u{f1c2}",
        Some("xls") | Some("xlsx") | Some("csv") => "\u{f1c3}",
        Some("txt") | Some("log") => "\u{f15c}",
        Some("conf") | Some("ini") | Some("cfg") => "\u{e615}",
        Some("git") | Some("gitignore") | Some("gitconfig") => "\u{e702}",
        _ => "\u{f15b}",
    }
}

fn hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "host".into())
}
