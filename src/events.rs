use anyhow::Result;
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEventKind,
};
use ratatui::{backend::Backend, Terminal};
use std::time::Duration;

use crate::{
    app::{App, Focus, Mode, PromptKind},
    config::SortMode,
    panel::PanelMode,
    ui,
};

pub fn run_loop<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    let mut pending_g = false;
    let mut pending_git = false;
    let mut pending_cmd = false;
    while !app.quit {
        app.drain_task_messages();
        app.tick();
        if app.needs_redraw {
            terminal.clear()?;
            app.needs_redraw = false;
        }
        terminal.draw(|f| ui::draw(f, app))?;
        app.expire_status();

        if !event::poll(Duration::from_millis(120))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => match app.mode {
                Mode::Normal => match app.focus {
                    Focus::FilePanel => handle_file_panel(
                        app,
                        key,
                        &mut pending_g,
                        &mut pending_git,
                        &mut pending_cmd,
                    )?,
                    Focus::Sidebar => handle_sidebar(app, key)?,
                    Focus::Processes => handle_processes(app, key)?,
                    Focus::Metadata => handle_metadata(app, key)?,
                },
                Mode::Search => handle_search(app, key)?,
                Mode::Fuzzy => handle_fuzzy(app, key)?,
                Mode::SortMenu => handle_sort_menu(app, key)?,
                Mode::ConfirmDelete { permanent } => handle_confirm_delete(app, key, permanent)?,
                Mode::ConfirmQuit => handle_confirm_quit(app, key)?,
                Mode::Help => handle_help(app, key)?,
                Mode::Palette => handle_palette(app, key)?,
                Mode::Bookmarks => handle_bookmarks(app, key)?,
                Mode::Prompt(kind) => handle_prompt(app, key, kind)?,
            },
            Event::Mouse(me) => match me.kind {
                MouseEventKind::ScrollDown => {
                    if app.mode == Mode::Normal && app.focus == Focus::FilePanel {
                        app.move_cursor(3);
                    }
                }
                MouseEventKind::ScrollUp
                    if app.mode == Mode::Normal && app.focus == Focus::FilePanel =>
                {
                    app.move_cursor(-3);
                }
                _ => {}
            },
            Event::Resize(_, _) => {}
            _ => {}
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// File panel focus

fn handle_file_panel(
    app: &mut App,
    key: KeyEvent,
    pending_g: &mut bool,
    pending_git: &mut bool,
    pending_cmd: &mut bool,
) -> Result<()> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let select_mode = app.panel().mode == PanelMode::Select;

    if *pending_cmd {
        *pending_cmd = false;
        if let KeyCode::Char(c) = key.code {
            app.run_command_binding(c)?;
        }
        return Ok(());
    }

    if *pending_git {
        *pending_git = false;
        match key.code {
            KeyCode::Char('s') => app.git_stage()?,
            KeyCode::Char('u') => app.git_unstage()?,
            KeyCode::Char('x') => app.git_discard()?,
            KeyCode::Char('c') => {
                app.input_clear();
                app.mode = Mode::Prompt(PromptKind::CommitMsg);
            }
            KeyCode::Char('d') => app.toggle_diff_mode(),
            KeyCode::Char('r') => app.refresh()?,
            KeyCode::Char('g') | KeyCode::Char(':') => {
                app.input_clear();
                app.mode = Mode::Prompt(PromptKind::GitCmd);
            }
            _ => {}
        }
        return Ok(());
    }

    // -- Ctrl combos (main file operations)
    if ctrl {
        match key.code {
            KeyCode::Char('c') => app.yank(),
            KeyCode::Char('x') => app.cut(),
            KeyCode::Char('v') => app.paste()?,
            KeyCode::Char('d') => {
                if app.config.confirm_delete {
                    app.open_confirm_delete(false);
                } else {
                    app.delete_current(false)?;
                }
            }
            KeyCode::Char('n') => {
                app.input_clear();
                app.mode = Mode::Prompt(PromptKind::New);
            }
            KeyCode::Char('r') => app.open_rename_prompt(),
            KeyCode::Char('p') => app.copy_current_path(),
            KeyCode::Char('f') => app.open_fuzzy(),
            KeyCode::Char('a') => app.select_all(),
            // Ctrl+Space: toggle selection without moving (rustfm habit).
            KeyCode::Char(' ') => app.select_toggle_current(),
            _ => {}
        }
        *pending_g = false;
        return Ok(());
    }

    // Shift+↑/↓ in select mode: select while moving (crossterm reports them
    // as Up/Down with SHIFT). Handled before the main match so the plain
    // Up/Down arms don't also fire.
    if select_mode && shift {
        match key.code {
            KeyCode::Down => {
                app.select_move(1);
                return Ok(());
            }
            KeyCode::Up => {
                app.select_move(-1);
                return Ok(());
            }
            _ => {}
        }
    }

    match key.code {
        // -- Basics
        KeyCode::Char('q') => app.request_quit(),
        // Esc backs out of search filter / select mode / selection / focus.
        // It never quits the app — accidental double-Esc must not kill the
        // session (use q to quit).
        KeyCode::Esc => {
            let _ = app.escape_pressed()?;
        }
        KeyCode::Char('?') => {
            app.help_scroll = 0;
            app.mode = Mode::Help;
        }

        // -- Navigation
        KeyCode::Down | KeyCode::Char('j') => app.move_cursor(1),
        KeyCode::Up | KeyCode::Char('k') => app.move_cursor(-1),
        KeyCode::PageDown => app.page_move(1),
        KeyCode::PageUp => app.page_move(-1),
        KeyCode::Home => app.goto_top(),
        KeyCode::End => app.goto_bottom(),
        KeyCode::Left | KeyCode::Char('h') | KeyCode::Backspace => app.go_up()?,
        KeyCode::Right | KeyCode::Char('l') => {
            if select_mode {
                app.select_toggle_current();
            } else {
                app.enter(false)?;
            }
        }
        KeyCode::Enter => {
            if select_mode {
                app.select_toggle_current();
            } else {
                // Shift+Enter forces interactive mode for executables (TUI
                // suspends so the user can read logs). Plain Enter spawns
                // executables detached, like double-clicking in a GUI.
                app.enter(shift)?;
            }
        }
        KeyCode::Char(' ') => {
            if select_mode {
                app.select_toggle_current();
            } else {
                // rustfm habit: space toggles selection and moves down.
                app.select_toggle_current();
                app.move_cursor(1);
            }
        }

        // -- gg / G
        KeyCode::Char('g') => {
            if *pending_g {
                app.goto_top();
                *pending_g = false;
            } else {
                *pending_g = true;
                return Ok(());
            }
        }
        KeyCode::Char('G') => app.goto_bottom(),

        // -- File panels
        KeyCode::Tab => app.next_panel(),
        KeyCode::BackTab => app.prev_panel(),
        KeyCode::Char('L') => app.next_panel(),
        KeyCode::Char('H') => app.prev_panel(),
        KeyCode::Char('n') | KeyCode::Char('N') => app.new_panel(),
        KeyCode::Char('w') => app.close_panel(),

        // -- Focus. Never move focus into a pane that isn't on screen —
        // that reads as "the app froze". The footer auto-opens instead.
        KeyCode::Char('s') => {
            if app.config.sidebar_width > 0 {
                app.focus = Focus::Sidebar;
            } else {
                app.set_status("sidebar is disabled (sidebar_width = 0)".into(), true);
            }
        }
        KeyCode::Char('b') => {
            if !app.footer_visible {
                app.toggle_footer();
            }
            app.focus = Focus::Processes;
        }
        KeyCode::Char('m') => {
            if !app.footer_visible {
                app.toggle_footer();
            }
            app.focus = Focus::Metadata;
        }

        // -- Toggles
        KeyCode::Char('f') => app.toggle_preview(),
        KeyCode::Char('F') => app.toggle_footer(),
        KeyCode::Char('P') => app.toggle_pin(),
        KeyCode::Char('.') => app.toggle_hidden()?,

        // -- Select mode
        KeyCode::Char('v') => app.toggle_select_mode(),
        KeyCode::Char('A') => {
            app.select_all();
        }
        KeyCode::Char('J') => {
            if select_mode {
                app.select_move(1);
            } else {
                app.scroll_preview(1);
            }
        }
        KeyCode::Char('K') => {
            if select_mode {
                app.select_move(-1);
            } else {
                app.scroll_preview(-1);
            }
        }
        KeyCode::Char('}') => app.scroll_preview(10),
        KeyCode::Char('{') => app.scroll_preview(-10),

        // -- Copy / cut / paste — rustfm habits, same as ctrl+c/x/v.
        KeyCode::Char('y') => app.yank(),
        KeyCode::Char('d') => app.cut(),
        KeyCode::Char('p') => app.paste()?,

        // -- Delete: D / delete / ctrl+d all go to trash (with confirm).
        // Permanent deletion lives in the command palette only, so an old
        // "D = delete" habit can never hard-wipe files by accident.
        KeyCode::Delete | KeyCode::Char('D') => {
            if app.config.confirm_delete {
                app.open_confirm_delete(false);
            } else {
                app.delete_current(false)?;
            }
        }

        // -- Create / rename — rustfm habits, same as ctrl+n / ctrl+r.
        KeyCode::Char('a') => {
            app.input_clear();
            app.mode = Mode::Prompt(PromptKind::New);
        }
        KeyCode::Char('r') => app.open_rename_prompt(),
        KeyCode::Char('R') => app.refresh()?,

        // -- Editor / copy helpers
        KeyCode::Char('e') => app.open_with_editor()?,
        KeyCode::Char('E') => app.open_dir_with_editor()?,
        KeyCode::Char('c') => app.copy_cwd(),

        // -- View
        KeyCode::Char('o') => app.open_sort_menu(),

        // -- Search / overlays
        KeyCode::Char('/') => app.open_search(),
        KeyCode::Char(':') => app.open_palette(),
        KeyCode::Char('\'') => app.open_bookmarks(),
        KeyCode::Char('!') => {
            app.input_clear();
            app.mode = Mode::Prompt(PromptKind::Shell);
        }
        KeyCode::Char('z') => {
            *pending_git = true;
            return Ok(());
        }
        KeyCode::Char(',') => {
            *pending_cmd = true;
            return Ok(());
        }
        _ => {}
    }
    *pending_g = false;
    Ok(())
}

// ---------------------------------------------------------------------------
// Sidebar / Processes / Metadata focus

fn handle_sidebar(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Char('q') => app.request_quit(),
        KeyCode::Esc | KeyCode::Char('s') | KeyCode::Tab => app.focus = Focus::FilePanel,
        KeyCode::Down | KeyCode::Char('j') => app.sidebar.move_cursor(1),
        KeyCode::Up | KeyCode::Char('k') => app.sidebar.move_cursor(-1),
        KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => app.sidebar_open_selected()?,
        KeyCode::Char('b') => {
            if !app.footer_visible {
                app.toggle_footer();
            }
            app.focus = Focus::Processes;
        }
        KeyCode::Char('m') => {
            if !app.footer_visible {
                app.toggle_footer();
            }
            app.focus = Focus::Metadata;
        }
        KeyCode::Char('?') => {
            app.help_scroll = 0;
            app.mode = Mode::Help;
        }
        _ => {}
    }
    Ok(())
}

fn handle_processes(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Char('q') => app.request_quit(),
        KeyCode::Esc | KeyCode::Char('b') | KeyCode::Tab => app.focus = Focus::FilePanel,
        KeyCode::Down | KeyCode::Char('j') => {
            if app.proc_scroll + 1 < app.processes.len() {
                app.proc_scroll += 1;
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.proc_scroll = app.proc_scroll.saturating_sub(1);
        }
        KeyCode::Char('s') => {
            if app.config.sidebar_width > 0 {
                app.focus = Focus::Sidebar;
            }
        }
        KeyCode::Char('m') => app.focus = Focus::Metadata,
        KeyCode::Char('?') => {
            app.help_scroll = 0;
            app.mode = Mode::Help;
        }
        _ => {}
    }
    Ok(())
}

fn handle_metadata(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Char('q') => app.request_quit(),
        KeyCode::Esc | KeyCode::Char('m') | KeyCode::Tab => app.focus = Focus::FilePanel,
        KeyCode::Down | KeyCode::Char('j') => {
            if app.meta_scroll + 1 < app.metadata.len() {
                app.meta_scroll += 1;
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.meta_scroll = app.meta_scroll.saturating_sub(1);
        }
        KeyCode::Char('s') => {
            if app.config.sidebar_width > 0 {
                app.focus = Focus::Sidebar;
            }
        }
        KeyCode::Char('b') => app.focus = Focus::Processes,
        KeyCode::Char('?') => {
            app.help_scroll = 0;
            app.mode = Mode::Help;
        }
        _ => {}
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Typing modes

fn edit_input(app: &mut App, key: KeyEvent) -> bool {
    // Ctrl/Alt chords are commands, never literal input — without this,
    // Ctrl+C in a rename prompt types a literal 'c' into the filename.
    if key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        return false;
    }
    match key.code {
        KeyCode::Left => {
            app.input_left();
            true
        }
        KeyCode::Right => {
            app.input_right();
            true
        }
        KeyCode::Home => {
            app.input_home();
            true
        }
        KeyCode::End => {
            app.input_end();
            true
        }
        KeyCode::Backspace => {
            app.input_backspace();
            true
        }
        KeyCode::Delete => {
            app.input_delete();
            true
        }
        KeyCode::Char(c) => {
            app.input_insert(c);
            true
        }
        _ => false,
    }
}

fn is_cancel(key: &KeyEvent) -> bool {
    key.code == KeyCode::Esc
        || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
}

fn handle_search(app: &mut App, key: KeyEvent) -> Result<()> {
    if is_cancel(&key) {
        return app.close_search(false);
    }
    match key.code {
        KeyCode::Enter => app.close_search(true)?,
        KeyCode::Down => app.move_cursor(1),
        KeyCode::Up => app.move_cursor(-1),
        _ => {
            if edit_input(app, key) {
                app.apply_search_live()?;
            }
        }
    }
    Ok(())
}

fn handle_fuzzy(app: &mut App, key: KeyEvent) -> Result<()> {
    if is_cancel(&key) {
        app.mode = Mode::Normal;
        app.input_clear();
        app.fuzzy_matches.clear();
        app.fuzzy_cursor = 0;
        return Ok(());
    }
    match key.code {
        KeyCode::Enter => {
            let sel = app.fuzzy_cursor;
            app.accept_fuzzy(sel);
            app.mode = Mode::Normal;
            app.input_clear();
            app.fuzzy_matches.clear();
            app.fuzzy_cursor = 0;
        }
        KeyCode::Up => app.fuzzy_move(-1),
        KeyCode::Down => app.fuzzy_move(1),
        _ => {
            if edit_input(app, key) {
                app.fuzzy_cursor = 0;
                app.update_fuzzy();
            }
        }
    }
    Ok(())
}

fn handle_sort_menu(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('o') | KeyCode::Char('q') => app.mode = Mode::Normal,
        KeyCode::Down | KeyCode::Char('j') => {
            if app.sort_cursor < 3 {
                app.sort_cursor += 1;
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.sort_cursor = app.sort_cursor.saturating_sub(1);
        }
        KeyCode::Char('R') | KeyCode::Char('r') => app.toggle_sort_reverse()?,
        KeyCode::Enter => {
            let mode = match app.sort_cursor {
                0 => SortMode::Name,
                1 => SortMode::Size,
                2 => SortMode::Mtime,
                _ => SortMode::Ext,
            };
            app.set_sort(mode)?;
            app.mode = Mode::Normal;
        }
        _ => {}
    }
    Ok(())
}

fn handle_confirm_delete(app: &mut App, key: KeyEvent, permanent: bool) -> Result<()> {
    if is_cancel(&key) {
        app.confirm_targets.clear();
        app.mode = Mode::Normal;
        return Ok(());
    }
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            app.delete_current(permanent)?;
            app.mode = Mode::Normal;
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            app.confirm_targets.clear();
            app.mode = Mode::Normal;
        }
        KeyCode::Left | KeyCode::Right | KeyCode::Tab => app.confirm_yes = !app.confirm_yes,
        KeyCode::Enter => {
            if app.confirm_yes {
                app.delete_current(permanent)?;
            } else {
                app.confirm_targets.clear();
            }
            app.mode = Mode::Normal;
        }
        _ => {}
    }
    Ok(())
}

fn handle_confirm_quit(app: &mut App, key: KeyEvent) -> Result<()> {
    if is_cancel(&key) {
        app.mode = Mode::Normal;
        return Ok(());
    }
    match key.code {
        // A second `q` confirms — quitting is what the user just asked for.
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Char('q') => app.quit = true,
        KeyCode::Char('n') | KeyCode::Char('N') => app.mode = Mode::Normal,
        KeyCode::Left | KeyCode::Right | KeyCode::Tab => app.confirm_yes = !app.confirm_yes,
        KeyCode::Enter => {
            if app.confirm_yes {
                app.quit = true;
            } else {
                app.mode = Mode::Normal;
            }
        }
        _ => {}
    }
    Ok(())
}

fn handle_help(app: &mut App, key: KeyEvent) -> Result<()> {
    let max = ui::help_entries().len() as u16;
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => app.mode = Mode::Normal,
        KeyCode::Down | KeyCode::Char('j') => {
            app.help_scroll = (app.help_scroll + 1).min(max.saturating_sub(5));
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.help_scroll = app.help_scroll.saturating_sub(1);
        }
        KeyCode::PageDown => {
            app.help_scroll = (app.help_scroll + 10).min(max.saturating_sub(5));
        }
        KeyCode::PageUp => {
            app.help_scroll = app.help_scroll.saturating_sub(10);
        }
        _ => {}
    }
    Ok(())
}

fn handle_palette(app: &mut App, key: KeyEvent) -> Result<()> {
    if is_cancel(&key) {
        app.mode = Mode::Normal;
        app.input_clear();
        app.palette_matches.clear();
        app.palette_cursor = 0;
        return Ok(());
    }
    match key.code {
        KeyCode::Enter => {
            app.accept_palette()?;
        }
        KeyCode::Up => app.palette_move(-1),
        KeyCode::Down => app.palette_move(1),
        _ => {
            if edit_input(app, key) {
                app.palette_cursor = 0;
                app.update_palette();
            }
        }
    }
    Ok(())
}

fn handle_bookmarks(app: &mut App, key: KeyEvent) -> Result<()> {
    if app.bookmarks_adding {
        match key.code {
            KeyCode::Esc => {
                app.bookmarks_adding = false;
            }
            KeyCode::Char(c) => {
                app.bookmarks_finish_add(c)?;
            }
            _ => {}
        }
        return Ok(());
    }
    match key.code {
        KeyCode::Esc => app.bookmarks_close(),
        KeyCode::Up => app.bookmarks_move(-1),
        KeyCode::Down => app.bookmarks_move(1),
        KeyCode::Enter => app.bookmarks_accept()?,
        KeyCode::Char('a') => app.bookmarks_start_add(),
        // Delete is 'x' only — 'd' stays free for quick-jumping to a
        // bookmark bound to 'd' (deleting also persists immediately, so a
        // shadowed quick-jump key would destroy bookmarks by surprise).
        KeyCode::Char('x') => app.bookmarks_delete_current()?,
        // Quick-jump: any other character matches a bookmark key directly.
        KeyCode::Char(c) => {
            let key_str = c.to_string();
            if app.config.bookmarks.contains_key(&key_str) {
                app.bookmarks_close();
                app.jump_bookmark(&key_str)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn handle_prompt(app: &mut App, key: KeyEvent, kind: PromptKind) -> Result<()> {
    if is_cancel(&key) {
        app.mode = Mode::Normal;
        app.input_clear();
        app.rename_target = None;
        return Ok(());
    }
    match key.code {
        KeyCode::Enter => {
            let input = app.input_take();
            app.mode = Mode::Normal;
            if input.is_empty() {
                return Ok(());
            }
            match kind {
                PromptKind::Rename => app.rename_current(&input)?,
                PromptKind::New => app.make_entry(&input)?,
                PromptKind::GoTo => app.goto_path(&input)?,
                PromptKind::CommitMsg => app.git_commit(&input)?,
                PromptKind::GitCmd => app.run_git_cmd(&input)?,
                PromptKind::Shell => app.run_shell_raw(&input)?,
            }
        }
        _ => {
            edit_input(app, key);
        }
    }
    Ok(())
}
