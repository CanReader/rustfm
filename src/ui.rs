use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use ratatui_image::StatefulImage;
use unicode_width::UnicodeWidthStr;

use crate::{
    app::{
        App, ClipMode, Focus, FuzzyMatch, Mode, Process, ProcessState, PromptKind, PALETTE_ENTRIES,
    },
    config::SortMode,
    fs_ops::Entry,
    git::{GitInfo, GitState},
    panel::PanelMode,
    preview::Preview,
    sidebar::SideItem,
    theme::Palette,
};

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Rustfm's border language: the focused zone gets a thick frame, everything
/// else a thin plain one.
fn border_type(active: bool) -> BorderType {
    if active {
        BorderType::Thick
    } else {
        BorderType::Plain
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();

    // Base background: solid fill unless transparency is enabled.
    if !app.config.transparent_background {
        f.render_widget(
            Block::default().style(Style::default().bg(app.palette.file_panel_bg)),
            area,
        );
    }

    let footer_h = if app.footer_visible && area.height > 20 {
        app.config.footer_height.clamp(8, area.height / 2)
    } else {
        0
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(footer_h)])
        .split(area);

    draw_main_row(f, rows[0], app);
    if footer_h > 0 {
        draw_footer_row(f, rows[1], app);
    }

    draw_status_toast(f, area, app);

    match app.mode {
        Mode::Fuzzy => draw_fuzzy(f, area, app),
        Mode::Palette => draw_palette(f, area, app),
        Mode::Bookmarks => draw_bookmarks(f, area, app),
        Mode::Prompt(kind) => draw_prompt(f, area, app, kind),
        Mode::ConfirmDelete { permanent } => draw_confirm_delete(f, area, app, permanent),
        Mode::ConfirmQuit => draw_confirm_quit(f, area, app),
        Mode::SortMenu => draw_sort_menu(f, area, app),
        Mode::Help => draw_help(f, area, app),
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Main row: sidebar | file panels | preview

fn draw_main_row(f: &mut Frame, area: Rect, app: &mut App) {
    let sidebar_w = if app.config.sidebar_width > 0 && area.width > 45 {
        app.config.sidebar_width.min(area.width / 3)
    } else {
        0
    };
    let remaining = area.width.saturating_sub(sidebar_w);
    let n_panels = app.panels.len() as u16;
    let preview_w = if app.preview_visible {
        if app.config.file_preview_width == 0 {
            remaining / (n_panels + 1)
        } else {
            area.width / app.config.file_preview_width.max(2)
        }
    } else {
        0
    };
    let panels_w = remaining.saturating_sub(preview_w);

    let mut constraints: Vec<Constraint> = Vec::new();
    if sidebar_w > 0 {
        constraints.push(Constraint::Length(sidebar_w));
    }
    let per_panel = panels_w / n_panels.max(1);
    for i in 0..n_panels {
        if i == n_panels - 1 {
            // Last panel absorbs the rounding remainder.
            constraints.push(Constraint::Length(panels_w - per_panel * (n_panels - 1)));
        } else {
            constraints.push(Constraint::Length(per_panel));
        }
    }
    if preview_w > 0 {
        constraints.push(Constraint::Length(preview_w));
    }

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    let mut idx = 0;
    if sidebar_w > 0 {
        draw_sidebar(f, chunks[idx], app);
        idx += 1;
    }
    for i in 0..app.panels.len() {
        draw_file_panel(f, chunks[idx], app, i);
        idx += 1;
    }
    if preview_w > 0 {
        draw_preview(f, chunks[idx], app);
    }
}

// ---------------------------------------------------------------------------
// Sidebar

fn draw_sidebar(f: &mut Frame, area: Rect, app: &mut App) {
    let pal = &app.palette;
    let focused = app.focus == Focus::Sidebar;
    let border = if focused {
        pal.sidebar_border_active
    } else {
        pal.sidebar_border
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(border_type(focused))
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(pal.sidebar_bg));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 3 {
        return;
    }

    // Logo: gear icon + gradient wordmark.
    let title = "rustfm";
    let icon_w = if app.config.nerdfont { 2 } else { 0 };
    let pad = (inner.width as usize).saturating_sub(title.len() + icon_w) / 2;
    let mut spans: Vec<Span> = vec![Span::raw(" ".repeat(pad))];
    if app.config.nerdfont {
        spans.push(Span::styled(
            "\u{e7a8} ", //
            Style::default().fg(pal.gradient.1),
        ));
    }
    let chars: Vec<char> = title.chars().collect();
    for (i, ch) in chars.iter().enumerate() {
        let t = i as f32 / (chars.len().saturating_sub(1).max(1)) as f32;
        spans.push(Span::styled(
            ch.to_string(),
            Style::default()
                .fg(pal.gradient_at(t))
                .add_modifier(Modifier::BOLD),
        ));
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)),
        Rect { height: 1, ..inner },
    );

    let list_area = Rect {
        x: inner.x,
        y: inner.y + 2,
        width: inner.width,
        height: inner.height.saturating_sub(2),
    };
    let h = list_area.height as usize;
    let offset = app.sidebar.cursor.saturating_sub(h.saturating_sub(1));

    let mut lines: Vec<Line> = Vec::new();
    for (i, item) in app.sidebar.items.iter().enumerate().skip(offset).take(h) {
        match item {
            SideItem::Divider(label) => {
                // Section header: small-caps style label, no dashed rule.
                lines.push(Line::from(vec![
                    Span::styled("▪ ", Style::default().fg(pal.sidebar_divider)),
                    Span::styled(
                        label.to_uppercase(),
                        Style::default()
                            .fg(pal.sidebar_title)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]));
            }
            SideItem::Dir { name, icon, .. } => {
                let is_cursor = i == app.sidebar.cursor;
                let name_style = if is_cursor {
                    let mut s = Style::default().fg(pal.sidebar_item_selected_fg);
                    if pal.sidebar_item_selected_bg != pal.sidebar_bg {
                        s = s.bg(pal.sidebar_item_selected_bg);
                    }
                    if focused {
                        s = s.add_modifier(Modifier::BOLD);
                    }
                    s
                } else {
                    Style::default().fg(pal.sidebar_fg)
                };
                // Full-row background highlight is the cursor — no bar glyph.
                let marker = Span::styled(" ", name_style);
                let icon_span = if app.config.nerdfont {
                    Span::styled(format!("{icon} "), name_style)
                } else {
                    Span::raw("")
                };
                let max_w = (list_area.width as usize).saturating_sub(4);
                let text = truncate_end(name, max_w);
                let fill =
                    (list_area.width as usize).saturating_sub(1 + icon_span.width() + text.width());
                lines.push(Line::from(vec![
                    marker,
                    icon_span,
                    Span::styled(text, name_style),
                    Span::styled(" ".repeat(fill), name_style),
                ]));
            }
        }
    }
    f.render_widget(Paragraph::new(lines), list_area);
}

// ---------------------------------------------------------------------------
// File panel

fn draw_file_panel(f: &mut Frame, area: Rect, app: &mut App, panel_idx: usize) {
    let is_active = panel_idx == app.active_panel;
    let focused = is_active && app.focus == Focus::FilePanel;
    let pal = &app.palette;
    let panel = &app.panels[panel_idx];

    let border = if focused {
        pal.file_panel_border_active
    } else {
        pal.file_panel_border
    };

    // Top border title: chevron + shortened path.
    let path_str = shorten_home(&panel.cwd.display().to_string());
    let max_title = (area.width as usize).saturating_sub(6);
    let mut title_spans: Vec<Span> = vec![Span::raw(" ")];
    title_spans.push(Span::styled(
        "❯ ",
        Style::default()
            .fg(pal.file_panel_top_directory_icon)
            .add_modifier(Modifier::BOLD),
    ));
    title_spans.push(Span::styled(
        truncate_start(&path_str, max_title),
        Style::default()
            .fg(pal.file_panel_top_path)
            .add_modifier(Modifier::BOLD),
    ));
    title_spans.push(Span::raw(" "));

    // Bottom border: item counter (right) and mode / sort marker (left).
    let counter = if panel.entries.is_empty() {
        " 0/0 ".to_string()
    } else {
        format!(" {}/{} ", panel.cursor + 1, panel.entries.len())
    };
    let mut bottom_left_spans: Vec<Span> = Vec::new();
    if panel.mode == PanelMode::Select {
        bottom_left_spans.push(Span::styled(
            " Select ",
            Style::default().fg(pal.hint).add_modifier(Modifier::BOLD),
        ));
    } else if is_active {
        let rev = if app.config.sort_reverse {
            "↓"
        } else {
            "↑"
        };
        bottom_left_spans.push(Span::styled(
            format!(" {} {rev} ", app.config.sort.label()),
            Style::default().fg(pal.readonly),
        ));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(border_type(focused))
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(pal.file_panel_bg))
        .title(Line::from(title_spans))
        .title_bottom(Line::from(bottom_left_spans))
        .title_bottom(
            Line::from(Span::styled(counter, Style::default().fg(pal.readonly))).right_aligned(),
        );
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 3 {
        return;
    }

    // Row 0: search bar.
    let search_area = Rect { height: 1, ..inner };
    let searching = panel.searching && is_active && app.mode == Mode::Search;
    let (search_text, search_style) = if searching {
        (app.input.clone(), Style::default().fg(pal.file_panel_fg))
    } else if !panel.search.is_empty() {
        (panel.search.clone(), Style::default().fg(pal.file_panel_fg))
    } else {
        ("Filter".to_string(), Style::default().fg(pal.readonly))
    };
    let icon = if app.config.nerdfont {
        "\u{f002} "
    } else {
        "/ "
    }; //
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {icon}"), Style::default().fg(pal.hint)),
            Span::styled(search_text.clone(), search_style),
        ])),
        search_area,
    );
    if searching {
        let cursor_byte = app.input_cursor.min(app.input.len());
        let w = app.input[..cursor_byte].width() as u16;
        let prefix = 1 + icon.width() as u16;
        f.set_cursor_position((
            (search_area.x + prefix + w).min(search_area.x + search_area.width.saturating_sub(1)),
            search_area.y,
        ));
    }

    // Row 1: dotted rule under the filter bar (rustfm's own accent).
    let divider_area = Rect {
        y: inner.y + 1,
        height: 1,
        ..inner
    };
    f.render_widget(
        Paragraph::new(Span::styled(
            "┄".repeat(inner.width as usize),
            Style::default().fg(pal.file_panel_border),
        )),
        divider_area,
    );

    // Rows 2..: entries.
    let list_area = Rect {
        x: inner.x,
        y: inner.y + 2,
        width: inner.width,
        height: inner.height.saturating_sub(2),
    };
    let rows = list_area.height as usize;
    if is_active {
        app.panel_rows = rows;
    }
    let panel = &mut app.panels[panel_idx];
    panel.clamp_offset(rows);
    let panel = &app.panels[panel_idx];
    let pal = &app.palette;

    if panel.entries.is_empty() {
        let msg = if panel.search.is_empty() {
            "(empty)"
        } else {
            "(no match)"
        };
        f.render_widget(
            Paragraph::new(Span::styled(msg, Style::default().fg(pal.readonly))),
            list_area,
        );
        return;
    }

    let mut lines: Vec<Line> = Vec::with_capacity(rows);
    for (i, entry) in panel
        .entries
        .iter()
        .enumerate()
        .skip(panel.offset)
        .take(rows)
    {
        let is_cursor = i == panel.cursor && is_active;
        let is_selected = panel.selected.contains(&entry.path);

        // The cursor is a full-row background highlight (rustfm's own look),
        // so every span in a cursor row carries the highlight bg.
        let row_bg = if is_cursor && pal.file_panel_item_selected_bg != pal.file_panel_bg {
            Some(pal.file_panel_item_selected_bg)
        } else {
            None
        };
        let with_bg = |mut s: Style| {
            if let Some(bg) = row_bg {
                s = s.bg(bg);
            }
            s
        };

        let mut style = entry_style(entry, pal);
        if is_cursor || is_selected {
            style = Style::default().fg(pal.file_panel_item_selected_fg);
        }
        if is_cursor {
            style = style.add_modifier(Modifier::BOLD);
        }
        style = with_bg(style);

        // Browser-mode selection marker; select mode shows checkboxes.
        let marker = if is_selected && panel.mode == PanelMode::Browser {
            Span::styled("•", with_bg(Style::default().fg(pal.hint)))
        } else {
            Span::styled(" ", with_bg(Style::default()))
        };

        let mut spans: Vec<Span> = vec![marker];
        let mut used = 1usize;

        if panel.mode == PanelMode::Select && app.config.nerdfont {
            let (check, check_color) = if is_selected {
                ("\u{f4a7} ", pal.correct) //
            } else {
                ("\u{e640} ", pal.readonly) //
            };
            spans.push(Span::styled(
                check,
                with_bg(Style::default().fg(check_color)),
            ));
            used += 2;
        } else if panel.mode == PanelMode::Select {
            let check = if is_selected { "[x] " } else { "[ ] " };
            spans.push(Span::styled(
                check,
                with_bg(Style::default().fg(pal.readonly)),
            ));
            used += 4;
        }

        if app.config.nerdfont {
            let (glyph, color) = icon_for(entry);
            spans.push(Span::styled(
                format!("{glyph} "),
                with_bg(
                    Style::default().fg(color.unwrap_or_else(|| {
                        entry_style(entry, pal).fg.unwrap_or(pal.file_panel_fg)
                    })),
                ),
            ));
            used += 3;
        }

        // Git status letter, right-aligned.
        let git_span = git_status_span(entry, app.git.as_ref(), pal);
        let git_w = if git_span.is_some() { 2 } else { 0 };

        let name = if entry.is_dir {
            format!("{}/", entry.name)
        } else {
            entry.name.clone()
        };
        let name_w = (list_area.width as usize).saturating_sub(used + git_w + 1);
        let display = truncate_end(&name, name_w);
        let pad = name_w.saturating_sub(display.width());
        spans.push(Span::styled(display, style));
        // Pad to the panel edge so the highlight covers the whole row.
        spans.push(Span::styled(" ".repeat(pad), with_bg(Style::default())));
        if let Some(mut gs) = git_span {
            gs.style = with_bg(gs.style);
            spans.push(gs);
        }
        spans.push(Span::styled(" ", with_bg(Style::default())));
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), list_area);
}

fn git_status_span<'a>(entry: &Entry, git: Option<&GitInfo>, pal: &Palette) -> Option<Span<'a>> {
    let fs = git?.status.get(&entry.path)?;
    let (state, ch) = if fs.worktree != GitState::Clean {
        (fs.worktree, fs.worktree.label())
    } else if fs.index != GitState::Clean {
        (fs.index, fs.index.label())
    } else {
        return None;
    };
    let color = state_color(state, pal);
    if ch.trim().is_empty() {
        return None;
    }
    Some(Span::styled(
        format!(" {ch}"),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    ))
}

// ---------------------------------------------------------------------------
// Preview panel

fn draw_preview(f: &mut Frame, area: Rect, app: &mut App) {
    let pal = &app.palette;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(pal.file_panel_border))
        .style(Style::default().bg(pal.file_panel_bg));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let scroll = app.preview_scroll;
    let dim = Style::default().fg(pal.readonly);
    match &mut app.preview {
        Preview::Text(lines) => {
            let joined = lines.join("\n");
            let p = Paragraph::new(joined)
                .style(Style::default().fg(app.palette.file_panel_fg))
                .wrap(Wrap { trim: false })
                .scroll((scroll, 0));
            f.render_widget(p, inner);
        }
        Preview::Code(lines) => {
            let text: Vec<Line> = lines
                .iter()
                .skip(scroll as usize)
                .take(inner.height as usize)
                .cloned()
                .collect();
            f.render_widget(Paragraph::new(text), inner);
        }
        Preview::Dir(entries) => {
            let start = scroll as usize;
            let nerd = app.config.nerdfont;
            let pal = &app.palette;
            let items: Vec<Line> = entries
                .iter()
                .skip(start)
                .take(inner.height as usize)
                .map(|e| {
                    let name = if e.is_dir {
                        format!("{}/", e.name)
                    } else {
                        e.name.clone()
                    };
                    let mut spans = Vec::new();
                    if nerd {
                        let (glyph, color) = icon_for(e);
                        spans.push(Span::styled(
                            format!("{glyph} "),
                            Style::default().fg(color.unwrap_or(pal.file_panel_fg)),
                        ));
                    }
                    spans.push(Span::styled(name, entry_style(e, pal)));
                    Line::from(spans)
                })
                .collect();
            f.render_widget(Paragraph::new(items), inner);
        }
        Preview::Image(proto) => {
            let image_widget = StatefulImage::default();
            f.render_stateful_widget(image_widget, inner, proto);
        }
        Preview::Diff(lines) => {
            let pal = &app.palette;
            let rendered: Vec<Line> = lines
                .iter()
                .skip(scroll as usize)
                .take(inner.height as usize)
                .map(|l| {
                    let color = if l.starts_with("+++") || l.starts_with("---") {
                        pal.readonly
                    } else if l.starts_with('+') {
                        pal.git_added
                    } else if l.starts_with('-') {
                        pal.git_deleted
                    } else if l.starts_with("@@") {
                        pal.hint
                    } else if l.starts_with("diff ") || l.starts_with("index ") {
                        pal.readonly
                    } else {
                        pal.file_panel_fg
                    };
                    Line::from(Span::styled(l.clone(), Style::default().fg(color)))
                })
                .collect();
            f.render_widget(Paragraph::new(rendered), inner);
        }
        Preview::Binary(info) => {
            f.render_widget(Paragraph::new(info.as_str()).style(dim), inner);
        }
        Preview::Empty => {
            f.render_widget(Paragraph::new("(empty)").style(dim), inner);
        }
        Preview::Unreadable(e) => {
            f.render_widget(
                Paragraph::new(format!("unreadable: {e}"))
                    .style(Style::default().fg(app.palette.error)),
                inner,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Footer: Processes | Metadata | Clipboard

fn draw_footer_row(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
        ])
        .split(area);

    draw_processes(f, chunks[0], app);
    draw_metadata(f, chunks[1], app);
    draw_clipboard(f, chunks[2], app);
}

fn footer_block<'a>(
    title: &'a str,
    icon: &'a str,
    active: bool,
    pal: &Palette,
    nerd: bool,
) -> Block<'a> {
    let border = if active {
        pal.footer_border_active
    } else {
        pal.footer_border
    };
    let mut spans = vec![Span::raw(" ")];
    if nerd {
        spans.push(Span::styled(
            format!("{icon} "),
            Style::default().fg(pal.hint),
        ));
    }
    // Small-caps section headers matching the sidebar's style.
    spans.push(Span::styled(
        title,
        Style::default()
            .fg(pal.sidebar_title)
            .add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::raw(" "));
    Block::default()
        .borders(Borders::ALL)
        .border_type(border_type(active))
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(pal.footer_bg))
        .title(Line::from(spans))
}

fn draw_processes(f: &mut Frame, area: Rect, app: &App) {
    let pal = &app.palette;
    let block = footer_block(
        "TASKS",
        "\u{f0ae}", //
        app.focus == Focus::Processes,
        pal,
        app.config.nerdfont,
    );
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    if app.processes.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(
                " no active tasks",
                Style::default().fg(pal.readonly),
            )),
            inner,
        );
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    for p in app.processes.iter().skip(app.proc_scroll) {
        if lines.len() + 2 > inner.height as usize {
            break;
        }
        lines.push(process_title_line(p, pal));
        lines.push(process_bar_line(p, pal, inner.width as usize));
        if lines.len() < inner.height as usize {
            lines.push(Line::raw(""));
        }
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn process_title_line<'a>(p: &Process, pal: &Palette) -> Line<'a> {
    let (icon, color) = match p.state {
        ProcessState::Running => {
            let frame = (p.started.elapsed().as_millis() / 100) as usize % SPINNER.len();
            (SPINNER[frame].to_string(), pal.hint)
        }
        ProcessState::Successful => ("✓".to_string(), pal.correct),
        ProcessState::Failed => ("✗".to_string(), pal.error),
    };
    let label = if p.current.is_empty() {
        p.label.clone()
    } else {
        format!("{} — {}", p.label, p.current)
    };
    Line::from(vec![
        Span::raw(" "),
        Span::styled(
            icon,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(label, Style::default().fg(pal.footer_fg)),
    ])
}

fn process_bar_line<'a>(p: &Process, pal: &Palette, width: usize) -> Line<'a> {
    let pct = if p.total == 0 {
        1.0
    } else {
        (p.done as f64 / p.total as f64).min(1.0)
    };
    let pct_label = format!(" {:>3}%", (pct * 100.0) as u16);
    let bar_w = width.saturating_sub(pct_label.len() + 2);
    let filled = (bar_w as f64 * pct).round() as usize;
    let mut spans: Vec<Span> = vec![Span::raw(" ")];
    for i in 0..bar_w {
        if i < filled {
            let t = if bar_w <= 1 {
                0.0
            } else {
                i as f32 / (bar_w - 1) as f32
            };
            spans.push(Span::styled("▰", Style::default().fg(pal.gradient_at(t))));
        } else {
            spans.push(Span::styled("▱", Style::default().fg(pal.footer_border)));
        }
    }
    spans.push(Span::styled(pct_label, Style::default().fg(pal.footer_fg)));
    Line::from(spans)
}

fn draw_metadata(f: &mut Frame, area: Rect, app: &App) {
    let pal = &app.palette;
    let block = footer_block(
        "INFO",
        "\u{f05a}", //
        app.focus == Focus::Metadata,
        pal,
        app.config.nerdfont,
    );
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    if app.metadata.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(
                " nothing selected",
                Style::default().fg(pal.readonly),
            )),
            inner,
        );
        return;
    }

    let key_w = 12usize;
    let val_w = (inner.width as usize).saturating_sub(key_w + 3);
    let lines: Vec<Line> = app
        .metadata
        .iter()
        .skip(app.meta_scroll)
        .take(inner.height as usize)
        .map(|(k, v)| {
            Line::from(vec![
                Span::styled(format!(" {k:<key_w$} "), Style::default().fg(pal.hint)),
                Span::styled(truncate_end(v, val_w), Style::default().fg(pal.footer_fg)),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_clipboard(f: &mut Frame, area: Rect, app: &App) {
    let pal = &app.palette;
    let block = footer_block(
        "CLIPBOARD",
        "\u{f0ea}", //
        false,
        pal,
        app.config.nerdfont,
    );
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    if app.clipboard.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(
                " no content in clipboard",
                Style::default().fg(pal.readonly),
            )),
            inner,
        );
        return;
    }

    let mode = match app.clip_mode {
        ClipMode::Copy => ("Copy", pal.correct),
        ClipMode::Cut => ("Cut", pal.cancel),
    };
    let mut lines: Vec<Line> = vec![Line::from(vec![
        Span::raw(" "),
        Span::styled(
            mode.0,
            Style::default().fg(mode.1).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" — {} item(s)", app.clipboard.len()),
            Style::default().fg(pal.readonly),
        ),
    ])];
    let max_items = (inner.height as usize).saturating_sub(1);
    for (i, p) in app.clipboard.iter().enumerate() {
        if i + 1 >= max_items && app.clipboard.len() > max_items {
            lines.push(Line::from(Span::styled(
                format!(" … and {} more", app.clipboard.len() - i),
                Style::default().fg(pal.readonly),
            )));
            break;
        }
        if i >= max_items {
            break;
        }
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| p.display().to_string());
        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                truncate_end(&name, (inner.width as usize).saturating_sub(3)),
                Style::default().fg(pal.footer_fg),
            ),
        ]));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

// ---------------------------------------------------------------------------
// Status toast

fn draw_status_toast(f: &mut Frame, area: Rect, app: &App) {
    let Some(s) = &app.status else { return };
    let pal = &app.palette;
    let color = if s.is_error { pal.error } else { pal.correct };
    let text = format!(" {} ", s.text);
    let w = (text.width() as u16 + 2).min(area.width.saturating_sub(2));
    let rect = Rect {
        x: area.x + 1,
        y: area.y + area.height.saturating_sub(1),
        width: w,
        height: 1,
    };
    f.render_widget(Clear, rect);
    f.render_widget(
        Paragraph::new(Span::styled(
            truncate_end(&text, w as usize),
            Style::default()
                .fg(color)
                .bg(pal.modal_bg)
                .add_modifier(Modifier::BOLD),
        )),
        rect,
    );
}

// ---------------------------------------------------------------------------
// Modals

fn modal_rect(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width.saturating_sub(4));
    let h = h.min(area.height.saturating_sub(2));
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

fn modal_block<'a>(title: Line<'a>, pal: &Palette) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(pal.modal_border_active))
        .style(Style::default().bg(pal.modal_bg).fg(pal.modal_fg))
        .title(title)
}

fn draw_prompt(f: &mut Frame, area: Rect, app: &App, kind: PromptKind) {
    let pal = &app.palette;
    let (title, hint): (&str, &str) = match kind {
        PromptKind::Rename => ("Rename", "new name"),
        PromptKind::New => ("Create", "name — end with / for a directory"),
        PromptKind::GoTo => ("Go to path", "~/some/where"),
        PromptKind::CommitMsg => ("Git commit", "commit message"),
        PromptKind::GitCmd => ("Git command", "e.g. log --oneline -5"),
        PromptKind::Shell => ("Shell command", "runs in current directory"),
    };
    let popup = modal_rect(area, 64, 5);
    f.render_widget(Clear, popup);
    let block = modal_block(
        Line::from(vec![
            Span::raw(" "),
            Span::styled(
                title,
                Style::default().fg(pal.hint).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ]),
        pal,
    );
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("❯ ", Style::default().fg(pal.cursor)),
            Span::styled(app.input.clone(), Style::default().fg(pal.modal_fg)),
        ])),
        rows[0],
    );
    f.render_widget(
        Paragraph::new(Span::styled(hint, Style::default().fg(pal.readonly))),
        rows[2],
    );

    let cursor_byte = app.input_cursor.min(app.input.len());
    let w = app.input[..cursor_byte].width() as u16;
    f.set_cursor_position((
        (rows[0].x + 2 + w).min(rows[0].x + rows[0].width.saturating_sub(1)),
        rows[0].y,
    ));
}

fn draw_confirm_delete(f: &mut Frame, area: Rect, app: &App, permanent: bool) {
    let pal = &app.palette;
    // Show the snapshot taken when the modal opened — the live entry list
    // may have been refreshed underneath us.
    let targets = &app.confirm_targets;
    let n = targets.len();
    let what = if n == 1 {
        targets
            .first()
            .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "1 item".into())
    } else {
        format!("{n} items")
    };
    let title = if permanent {
        " Delete permanently "
    } else {
        " Move to trash "
    };
    let popup = modal_rect(area, 52, 7);
    f.render_widget(Clear, popup);
    let block = modal_block(
        Line::from(Span::styled(
            title,
            Style::default().fg(pal.error).add_modifier(Modifier::BOLD),
        )),
        pal,
    );
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let msg = if permanent {
        format!("Permanently delete {what}? This cannot be undone.")
    } else {
        format!("Move {what} to trash?")
    };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);
    f.render_widget(
        Paragraph::new(msg)
            .style(Style::default().fg(pal.modal_fg))
            .wrap(Wrap { trim: true }),
        rows[0],
    );

    // Confirm/Cancel buttons.
    let confirm_style = if app.confirm_yes {
        Style::default()
            .fg(pal.modal_confirm_fg)
            .bg(pal.modal_confirm_bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(pal.modal_fg)
    };
    let cancel_style = if !app.confirm_yes {
        Style::default()
            .fg(pal.modal_cancel_fg)
            .bg(pal.modal_cancel_bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(pal.modal_fg)
    };
    let buttons = Line::from(vec![
        Span::styled("  Confirm (y)  ", confirm_style),
        Span::raw("   "),
        Span::styled("  Cancel (n)  ", cancel_style),
    ])
    .centered();
    f.render_widget(Paragraph::new(buttons), rows[2]);
}

fn draw_confirm_quit(f: &mut Frame, area: Rect, app: &App) {
    let pal = &app.palette;
    let running = app
        .processes
        .iter()
        .filter(|p| p.state == crate::app::ProcessState::Running)
        .count();
    let popup = modal_rect(area, 52, 7);
    f.render_widget(Clear, popup);
    let block = modal_block(
        Line::from(Span::styled(
            " Quit ",
            Style::default().fg(pal.error).add_modifier(Modifier::BOLD),
        )),
        pal,
    );
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);
    f.render_widget(
        Paragraph::new(format!(
            "{running} file operation(s) still running. Quit anyway? Unfinished transfers will be aborted."
        ))
        .style(Style::default().fg(pal.modal_fg))
        .wrap(Wrap { trim: true }),
        rows[0],
    );

    let confirm_style = if app.confirm_yes {
        Style::default()
            .fg(pal.modal_confirm_fg)
            .bg(pal.modal_confirm_bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(pal.modal_fg)
    };
    let cancel_style = if !app.confirm_yes {
        Style::default()
            .fg(pal.modal_cancel_fg)
            .bg(pal.modal_cancel_bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(pal.modal_fg)
    };
    let buttons = Line::from(vec![
        Span::styled("  Quit (y)  ", confirm_style),
        Span::raw("   "),
        Span::styled("  Stay (n)  ", cancel_style),
    ])
    .centered();
    f.render_widget(Paragraph::new(buttons), rows[2]);
}

fn draw_sort_menu(f: &mut Frame, area: Rect, app: &App) {
    let pal = &app.palette;
    let popup = modal_rect(area, 36, 9);
    f.render_widget(Clear, popup);
    let block = modal_block(
        Line::from(Span::styled(
            " Sort by ",
            Style::default().fg(pal.hint).add_modifier(Modifier::BOLD),
        )),
        pal,
    );
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let options = [
        SortMode::Name,
        SortMode::Size,
        SortMode::Mtime,
        SortMode::Ext,
    ];
    let mut lines: Vec<Line> = Vec::new();
    for (i, opt) in options.iter().enumerate() {
        let is_cursor = i == app.sort_cursor;
        let is_current = *opt == app.config.sort;
        let marker = if is_cursor {
            Span::styled("▍", Style::default().fg(pal.cursor))
        } else {
            Span::raw(" ")
        };
        let bullet = if is_current { "● " } else { "○ " };
        let style = if is_cursor {
            Style::default()
                .fg(pal.file_panel_item_selected_fg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(pal.modal_fg)
        };
        lines.push(Line::from(vec![
            marker,
            Span::styled(bullet, Style::default().fg(pal.hint)),
            Span::styled(opt.label().to_string(), style),
        ]));
    }
    lines.push(Line::raw(""));
    let rev = if app.config.sort_reverse { "on" } else { "off" };
    lines.push(Line::from(Span::styled(
        format!(" R reverse: {rev} · enter apply · esc close"),
        Style::default().fg(pal.readonly),
    )));
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_help(f: &mut Frame, area: Rect, app: &App) {
    let pal = &app.palette;
    let popup = modal_rect(area, 74, area.height.saturating_sub(4));
    f.render_widget(Clear, popup);
    let block = modal_block(
        Line::from(Span::styled(
            " Help — hotkeys ",
            Style::default()
                .fg(pal.help_menu_title)
                .add_modifier(Modifier::BOLD),
        )),
        pal,
    );
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let entries = help_entries();
    let lines: Vec<Line> = entries
        .iter()
        .map(|(key, desc)| {
            if key.is_empty() {
                Line::from(Span::styled(
                    desc.to_string(),
                    Style::default()
                        .fg(pal.help_menu_title)
                        .add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from(vec![
                    Span::styled(
                        format!("  {key:<18}"),
                        Style::default().fg(pal.help_menu_hotkey),
                    ),
                    Span::styled(desc.to_string(), Style::default().fg(pal.modal_fg)),
                ])
            }
        })
        .collect();
    let para = Paragraph::new(lines).scroll((app.help_scroll, 0));
    f.render_widget(para, inner);
}

pub fn help_entries() -> Vec<(&'static str, &'static str)> {
    vec![
        ("", "General"),
        ("q / esc", "quit"),
        ("?", "toggle this help menu"),
        (":", "command palette"),
        ("!", "run shell command"),
        (",  then key", "run bound command"),
        ("", ""),
        ("", "Navigation"),
        ("↑↓ / j k", "move cursor"),
        ("pgup / pgdn", "page up / down"),
        ("g g / G", "go to top / bottom"),
        ("h / ← / bksp", "parent directory"),
        ("l / → / enter", "open file or directory"),
        ("shift+enter", "open executable interactively"),
        ("/", "search in panel (live filter)"),
        ("ctrl+f", "fuzzy jump in panel"),
        ("'", "bookmarks"),
        ("", ""),
        ("", "Panels"),
        ("n", "new file panel"),
        ("w", "close file panel"),
        ("tab / L", "next file panel"),
        ("H / shift+←", "previous file panel"),
        ("s", "focus sidebar"),
        ("b", "focus process bar"),
        ("m", "focus metadata"),
        ("f", "toggle preview panel"),
        ("F", "toggle footer"),
        ("P", "pin / unpin current directory"),
        ("", ""),
        ("", "File operations"),
        ("y / ctrl+c", "copy selection"),
        ("d / ctrl+x", "cut selection"),
        ("p / ctrl+v", "paste into current directory"),
        ("D / del / ctrl+d", "delete (trash)"),
        ("a / ctrl+n", "create file (end with / for dir)"),
        ("r / ctrl+r", "rename"),
        ("R", "refresh directory"),
        ("ctrl+p", "copy file path"),
        ("c", "copy working directory path"),
        ("e / E", "open file / directory with editor"),
        ("", ""),
        ("", "Selection"),
        ("space", "toggle selection (and move down)"),
        ("ctrl+space", "toggle selection in place"),
        ("v", "select mode (checkboxes)"),
        ("J K / shift+↑↓", "select while moving (select mode)"),
        ("A / ctrl+a", "select all"),
        ("", ""),
        ("", "View"),
        ("o", "sort options (R inside reverses)"),
        (".", "toggle hidden files"),
        ("J / K, { / }", "scroll preview / pdf pages"),
        ("", ""),
        ("", "Git"),
        ("z s / z u", "stage / unstage"),
        ("z x", "discard changes"),
        ("z c", "commit"),
        ("z d", "toggle diff preview"),
        ("z g", "raw git command"),
    ]
}

// ---------------------------------------------------------------------------
// Fuzzy / palette / bookmarks overlays

fn draw_fuzzy(f: &mut Frame, area: Rect, app: &App) {
    let pal = &app.palette;
    let popup = modal_rect(area, 72, 20);
    f.render_widget(Clear, popup);
    let block = modal_block(
        Line::from(Span::styled(
            " Fuzzy jump ",
            Style::default().fg(pal.hint).add_modifier(Modifier::BOLD),
        )),
        pal,
    );
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    let query = Paragraph::new(Line::from(vec![
        Span::styled("❯ ", Style::default().fg(pal.cursor)),
        Span::styled(app.input.clone(), Style::default().fg(pal.modal_fg)),
    ]));
    f.render_widget(query, layout[0]);

    let list_h = layout[1].height as usize;
    let offset = app.fuzzy_cursor.saturating_sub(list_h.saturating_sub(1));
    let entries = &app.panel().entries;

    let items: Vec<ListItem> = app
        .fuzzy_matches
        .iter()
        .skip(offset)
        .take(list_h)
        .filter_map(|m: &FuzzyMatch| {
            // Entries can shrink under the overlay (background task
            // refresh); never index past the current list.
            let entry = entries.get(m.index)?;
            let mut spans = Vec::new();
            let positions: std::collections::HashSet<usize> =
                m.match_positions.iter().copied().collect();
            for (i, c) in entry.name.chars().enumerate() {
                let style = if positions.contains(&i) {
                    Style::default().fg(pal.hint).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(pal.modal_fg)
                };
                spans.push(Span::styled(c.to_string(), style));
            }
            Some(ListItem::new(Line::from(spans)))
        })
        .collect();

    let list = List::new(items).highlight_style(
        Style::default()
            .fg(pal.file_panel_item_selected_fg)
            .add_modifier(Modifier::BOLD),
    );
    let mut state = ListState::default();
    if !app.fuzzy_matches.is_empty() {
        state.select(Some(app.fuzzy_cursor.saturating_sub(offset)));
    }
    f.render_stateful_widget(list, layout[1], &mut state);
}

fn draw_palette(f: &mut Frame, area: Rect, app: &App) {
    let pal = &app.palette;
    let popup = modal_rect(area, 72, 20);
    f.render_widget(Clear, popup);
    let block = modal_block(
        Line::from(Span::styled(
            " Command palette ",
            Style::default().fg(pal.hint).add_modifier(Modifier::BOLD),
        )),
        pal,
    );
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    let query = Paragraph::new(Line::from(vec![
        Span::styled("❯ ", Style::default().fg(pal.cursor)),
        Span::styled(app.input.clone(), Style::default().fg(pal.modal_fg)),
    ]));
    f.render_widget(query, layout[0]);

    let list_h = layout[1].height as usize;
    let offset = app.palette_cursor.saturating_sub(list_h.saturating_sub(1));
    let hint_w = 8usize;
    let label_w = (layout[1].width as usize).saturating_sub(hint_w + 1);

    let items: Vec<ListItem> = app
        .palette_matches
        .iter()
        .skip(offset)
        .take(list_h)
        .map(|m: &FuzzyMatch| {
            let entry = &PALETTE_ENTRIES[m.index];
            let positions: std::collections::HashSet<usize> =
                m.match_positions.iter().copied().collect();
            let mut spans = Vec::new();
            for (i, c) in entry.label.chars().enumerate() {
                let style = if positions.contains(&i) {
                    Style::default().fg(pal.hint).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(pal.modal_fg)
                };
                spans.push(Span::styled(c.to_string(), style));
            }
            let label_len = entry.label.chars().count();
            if label_len < label_w {
                spans.push(Span::raw(" ".repeat(label_w - label_len)));
            }
            if !entry.hint.is_empty() {
                spans.push(Span::styled(
                    format!("  {}", entry.hint),
                    Style::default().fg(pal.help_menu_hotkey),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items).highlight_style(
        Style::default()
            .fg(pal.file_panel_item_selected_fg)
            .add_modifier(Modifier::BOLD),
    );
    let mut state = ListState::default();
    if !app.palette_matches.is_empty() {
        state.select(Some(app.palette_cursor.saturating_sub(offset)));
    }
    f.render_stateful_widget(list, layout[1], &mut state);
}

fn draw_bookmarks(f: &mut Frame, area: Rect, app: &App) {
    let pal = &app.palette;
    let popup = modal_rect(area, 72, 20);
    f.render_widget(Clear, popup);
    let title = if app.bookmarks_adding {
        " Bookmarks — press a key to bind current dir "
    } else {
        " Bookmarks "
    };
    let block = modal_block(
        Line::from(Span::styled(
            title,
            Style::default().fg(pal.hint).add_modifier(Modifier::BOLD),
        )),
        pal,
    );
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    if app.bookmarks_view.is_empty() {
        let msg = if app.bookmarks_adding {
            "press a single key to bind to the current directory…"
        } else {
            "(no bookmarks)  press 'a' to add one for the current directory"
        };
        let p = Paragraph::new(msg)
            .style(Style::default().fg(pal.readonly))
            .wrap(Wrap { trim: false });
        f.render_widget(p, inner);
        return;
    }

    let list_h = inner.height as usize;
    let offset = app
        .bookmarks_cursor
        .saturating_sub(list_h.saturating_sub(1));
    let key_w = 4usize;
    let path_w = (inner.width as usize).saturating_sub(key_w + 2);

    let items: Vec<ListItem> = app
        .bookmarks_view
        .iter()
        .skip(offset)
        .take(list_h)
        .map(|(k, v)| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{k:<key_w$} "),
                    Style::default()
                        .fg(pal.help_menu_hotkey)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(truncate_start(v, path_w), Style::default().fg(pal.modal_fg)),
            ]))
        })
        .collect();

    let list = List::new(items).highlight_style(
        Style::default()
            .fg(pal.file_panel_item_selected_fg)
            .add_modifier(Modifier::BOLD),
    );
    let mut state = ListState::default();
    state.select(Some(app.bookmarks_cursor.saturating_sub(offset)));
    f.render_stateful_widget(list, inner, &mut state);
}

// ---------------------------------------------------------------------------
// Helpers

fn entry_style(e: &Entry, pal: &Palette) -> Style {
    if e.is_symlink {
        Style::default().fg(pal.symlink)
    } else if e.is_dir {
        Style::default()
            .fg(pal.directory)
            .add_modifier(Modifier::BOLD)
    } else if e.is_exec {
        Style::default().fg(pal.executable)
    } else if e.readonly {
        Style::default().fg(pal.readonly)
    } else {
        Style::default().fg(pal.file_panel_fg)
    }
}

fn state_color(state: GitState, pal: &Palette) -> Color {
    match state {
        GitState::Clean => Color::Reset,
        GitState::Modified => pal.git_modified,
        GitState::Added | GitState::Copied => pal.git_added,
        GitState::Renamed => pal.git_added,
        GitState::Deleted => pal.git_deleted,
        GitState::Conflict => pal.git_deleted,
        GitState::Untracked => pal.git_untracked,
        GitState::Ignored => pal.git_ignored,
    }
}

fn shorten_home(path: &str) -> String {
    let home = dirs::home_dir()
        .map(|h| h.display().to_string())
        .unwrap_or_default();
    if !home.is_empty() && path.starts_with(&home) {
        format!("~{}", &path[home.len()..])
    } else {
        path.to_string()
    }
}

/// Truncate keeping the END of the string (for paths): `…me/dir/sub`.
fn truncate_start(s: &str, max_w: usize) -> String {
    if s.width() <= max_w {
        return s.to_string();
    }
    let mut out: Vec<char> = Vec::new();
    let mut w = 0usize;
    for c in s.chars().rev() {
        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if w + cw + 1 > max_w {
            break;
        }
        w += cw;
        out.push(c);
    }
    let tail: String = out.into_iter().rev().collect();
    format!("…{tail}")
}

/// Truncate keeping the START of the string (for names): `long-nam…`.
fn truncate_end(s: &str, max_w: usize) -> String {
    if s.width() <= max_w {
        return s.to_string();
    }
    let mut out = String::new();
    let mut w = 0usize;
    for c in s.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if w + cw + 1 > max_w {
            break;
        }
        w += cw;
        out.push(c);
    }
    format!("{out}…")
}

/// Nerd Font icon + optional brand color per file type.
fn icon_for(e: &Entry) -> (&'static str, Option<Color>) {
    // Directories and symlinks return None so the icon inherits the theme's
    // entry color; only file types keep their brand colors.
    if e.is_symlink {
        return ("\u{f481}", None);
    }
    if e.is_dir {
        return ("\u{f07b}", None);
    }
    match e.ext_lower.as_deref() {
        Some("rs") => ("\u{e7a8}", Some(Color::Rgb(0xf7, 0x76, 0x48))),
        Some("go") => ("\u{e627}", Some(Color::Rgb(0x00, 0xad, 0xd8))),
        Some("py") => ("\u{e73c}", Some(Color::Rgb(0xff, 0xd4, 0x3b))),
        Some("js") | Some("mjs") | Some("cjs") => ("\u{e74e}", Some(Color::Rgb(0xf7, 0xdf, 0x1e))),
        Some("ts") | Some("tsx") => ("\u{e628}", Some(Color::Rgb(0x31, 0x78, 0xc6))),
        Some("jsx") => ("\u{e7ba}", Some(Color::Rgb(0x61, 0xda, 0xfb))),
        Some("html") | Some("htm") => ("\u{e736}", Some(Color::Rgb(0xe3, 0x4c, 0x26))),
        Some("css") | Some("scss") | Some("sass") => {
            ("\u{e749}", Some(Color::Rgb(0x15, 0x72, 0xb6)))
        }
        Some("md") | Some("markdown") => ("\u{f48a}", Some(Color::Rgb(0x9e, 0x9e, 0x9e))),
        Some("toml") => ("\u{e6b2}", Some(Color::Rgb(0x9c, 0x42, 0x21))),
        Some("yaml") | Some("yml") => ("\u{e6a8}", Some(Color::Rgb(0xcb, 0x17, 0x1e))),
        Some("json") => ("\u{e60b}", Some(Color::Rgb(0xcb, 0xcb, 0x41))),
        Some("lock") => ("\u{f023}", Some(Color::Rgb(0x6c, 0x70, 0x86))),
        Some("sh") | Some("bash") | Some("zsh") | Some("fish") => {
            ("\u{f489}", Some(Color::Rgb(0xa6, 0xe3, 0xa1)))
        }
        Some("c") | Some("h") => ("\u{e61e}", Some(Color::Rgb(0x55, 0x99, 0xd6))),
        Some("cpp") | Some("cc") | Some("hpp") | Some("hh") => {
            ("\u{e61d}", Some(Color::Rgb(0x00, 0x59, 0x9c)))
        }
        Some("java") => ("\u{e738}", Some(Color::Rgb(0xea, 0x2d, 0x2e))),
        Some("rb") => ("\u{e739}", Some(Color::Rgb(0xcc, 0x34, 0x2d))),
        Some("php") => ("\u{e73d}", Some(Color::Rgb(0x77, 0x7b, 0xb4))),
        Some("lua") => ("\u{e620}", Some(Color::Rgb(0x00, 0x00, 0x80))),
        Some("png") | Some("jpg") | Some("jpeg") | Some("gif") | Some("webp") | Some("bmp")
        | Some("tiff") | Some("ico") | Some("svg") | Some("avif") => {
            ("\u{f1c5}", Some(Color::Rgb(0xa6, 0xd1, 0x89)))
        }
        Some("mp4") | Some("mkv") | Some("webm") | Some("mov") | Some("avi") => {
            ("\u{f1c8}", Some(Color::Rgb(0xfd, 0x97, 0x1f)))
        }
        Some("mp3") | Some("flac") | Some("wav") | Some("ogg") | Some("m4a") => {
            ("\u{f1c7}", Some(Color::Rgb(0xef, 0x94, 0xd5)))
        }
        Some("zip") | Some("tar") | Some("gz") | Some("xz") | Some("zst") | Some("bz2")
        | Some("7z") | Some("rar") => ("\u{f1c6}", Some(Color::Rgb(0xec, 0xd2, 0x8b))),
        Some("pdf") => ("\u{f1c1}", Some(Color::Rgb(0xb3, 0x0b, 0x00))),
        Some("doc") | Some("docx") => ("\u{f1c2}", Some(Color::Rgb(0x28, 0x5f, 0xf5))),
        Some("xls") | Some("xlsx") | Some("csv") => {
            ("\u{f1c3}", Some(Color::Rgb(0x20, 0x74, 0x45)))
        }
        Some("txt") | Some("log") => ("\u{f15c}", None),
        Some("conf") | Some("ini") | Some("cfg") => ("\u{e615}", None),
        Some("git") | Some("gitignore") | Some("gitconfig") => {
            ("\u{e702}", Some(Color::Rgb(0xf1, 0x4e, 0x32)))
        }
        _ => ("\u{f15b}", None),
    }
}
