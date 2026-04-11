use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{backend::Backend, Terminal};
use std::time::Duration;

use crate::{
    app::{App, Mode, PromptKind},
    config::SortMode,
    ui,
};

pub fn run_loop<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    let mut pending_g = false;
    let mut pending_git = false;
    let mut pending_cmd = false;
    while !app.quit {
        app.drain_task_messages();
        terminal.draw(|f| ui::draw(f, app))?;
        app.expire_status();

        if !event::poll(Duration::from_millis(120))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => match app.mode {
                Mode::Normal => handle_normal(app, key, &mut pending_g, &mut pending_git, &mut pending_cmd)?,
                Mode::Search => handle_search(app, key)?,
                Mode::Filter => handle_filter(app, key)?,
                Mode::Fuzzy => handle_fuzzy(app, key)?,
                Mode::Sort => handle_sort(app, key)?,
                Mode::ConfirmDelete => handle_confirm_delete(app, key)?,
                Mode::Prompt(kind) => handle_prompt(app, key, kind)?,
            },
            Event::Resize(_, _) => {}
            _ => {}
        }
    }
    Ok(())
}

fn handle_normal(
    app: &mut App,
    key: KeyEvent,
    pending_g: &mut bool,
    pending_git: &mut bool,
    pending_cmd: &mut bool,
) -> Result<()> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);

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
                app.input.clear();
                app.mode = Mode::Prompt(PromptKind::CommitMsg);
            }
            KeyCode::Char('d') => app.toggle_diff_mode(),
            KeyCode::Char('r') => app.refresh()?,
            KeyCode::Char('g') | KeyCode::Char(':') => {
                app.input.clear();
                app.mode = Mode::Prompt(PromptKind::GitCmd);
            }
            _ => {}
        }
        return Ok(());
    }

    match key.code {
        KeyCode::Char('q') => app.quit = true,
        KeyCode::Char('f') if ctrl => {
            app.input.clear();
            app.update_fuzzy();
            app.mode = Mode::Fuzzy;
        }
        KeyCode::Char('a') if ctrl => app.select_all(),
        KeyCode::Char('d') if ctrl => app.move_cursor(10),
        KeyCode::Char('u') if ctrl => app.move_cursor(-10),
        KeyCode::Down => {
            if shift {
                app.range_select(1);
            } else {
                app.select_anchor = None;
                app.move_cursor(1);
            }
        }
        KeyCode::Up => {
            if shift {
                app.range_select(-1);
            } else {
                app.select_anchor = None;
                app.move_cursor(-1);
            }
        }
        KeyCode::Left => app.go_up()?,
        KeyCode::Right | KeyCode::Enter => app.enter()?,
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
        KeyCode::Char(' ') => {
            if ctrl {
                app.toggle_select_no_move();
            } else {
                app.toggle_select();
            }
        }
        KeyCode::Esc => {
            if !app.filter.is_empty() {
                app.clear_filter()?;
            } else {
                app.clear_selection();
            }
        }
        KeyCode::Char('y') => app.yank(),
        KeyCode::Char('d') => app.cut(),
        KeyCode::Char('p') => app.paste()?,
        KeyCode::Char('D') => {
            if app.config.confirm_delete {
                app.mode = Mode::ConfirmDelete;
            } else {
                app.delete_current()?;
            }
        }
        KeyCode::Char('r') => {
            app.input = app.current_entry().map(|e| e.name.clone()).unwrap_or_default();
            app.mode = Mode::Prompt(PromptKind::Rename);
        }
        KeyCode::Char('a') => {
            app.input.clear();
            app.mode = Mode::Prompt(PromptKind::NewFile);
        }
        KeyCode::Char('A') => {
            app.input.clear();
            app.mode = Mode::Prompt(PromptKind::NewDir);
        }
        KeyCode::Char('.') => app.toggle_hidden()?,
        KeyCode::Char('/') => {
            app.input.clear();
            app.mode = Mode::Search;
        }
        KeyCode::Char('f') => {
            app.input = app.filter.clone();
            app.mode = Mode::Filter;
        }
        KeyCode::Char('n') => app.search_next(true),
        KeyCode::Char('N') => app.search_next(false),
        KeyCode::Char(':') => {
            app.input.clear();
            app.mode = Mode::Prompt(PromptKind::GoTo);
        }
        KeyCode::Char('\'') => {
            app.input.clear();
            app.mode = Mode::Prompt(PromptKind::Bookmark);
        }
        KeyCode::Char('o') => app.mode = Mode::Sort,
        KeyCode::Char('R') => app.refresh()?,
        KeyCode::Char('z') => {
            *pending_git = true;
            return Ok(());
        }
        KeyCode::Char(',') => {
            *pending_cmd = true;
            return Ok(());
        }
        KeyCode::Char('!') => {
            app.input.clear();
            app.mode = Mode::Prompt(PromptKind::Shell);
        }
        _ => {}
    }
    *pending_g = false;
    Ok(())
}

fn handle_search(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
            app.input.clear();
        }
        KeyCode::Enter => {
            app.search_query = std::mem::take(&mut app.input);
            app.apply_search();
            app.mode = Mode::Normal;
        }
        KeyCode::Backspace => {
            app.input.pop();
        }
        KeyCode::Char(c) => app.input.push(c),
        _ => {}
    }
    Ok(())
}

fn handle_filter(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
            app.input.clear();
        }
        KeyCode::Enter => {
            app.apply_filter()?;
            app.mode = Mode::Normal;
            app.input.clear();
        }
        KeyCode::Backspace => {
            app.input.pop();
            app.apply_filter()?;
        }
        KeyCode::Char(c) => {
            app.input.push(c);
            app.apply_filter()?;
        }
        _ => {}
    }
    Ok(())
}

fn handle_fuzzy(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
            app.input.clear();
            app.fuzzy_matches.clear();
        }
        KeyCode::Enter => {
            app.accept_fuzzy(0);
            app.mode = Mode::Normal;
            app.input.clear();
            app.fuzzy_matches.clear();
        }
        KeyCode::Backspace => {
            app.input.pop();
            app.update_fuzzy();
        }
        KeyCode::Char(c) => {
            app.input.push(c);
            app.update_fuzzy();
        }
        _ => {}
    }
    Ok(())
}

fn handle_sort(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Char('n') => app.set_sort(SortMode::Name)?,
        KeyCode::Char('s') => app.set_sort(SortMode::Size)?,
        KeyCode::Char('t') => app.set_sort(SortMode::Mtime)?,
        KeyCode::Char('e') => app.set_sort(SortMode::Ext)?,
        KeyCode::Char('r') => app.toggle_sort_reverse()?,
        _ => {}
    }
    app.mode = Mode::Normal;
    Ok(())
}

fn handle_confirm_delete(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            app.delete_current()?;
            app.mode = Mode::Normal;
        }
        _ => app.mode = Mode::Normal,
    }
    Ok(())
}

fn handle_prompt(app: &mut App, key: KeyEvent, kind: PromptKind) -> Result<()> {
    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
            app.input.clear();
        }
        KeyCode::Enter => {
            let input = std::mem::take(&mut app.input);
            app.mode = Mode::Normal;
            if input.is_empty() {
                return Ok(());
            }
            match kind {
                PromptKind::Rename => app.rename_current(&input)?,
                PromptKind::NewFile => app.make_file(&input)?,
                PromptKind::NewDir => app.make_dir(&input)?,
                PromptKind::GoTo => app.goto_path(&input)?,
                PromptKind::Bookmark => app.jump_bookmark(&input)?,
                PromptKind::CommitMsg => app.git_commit(&input)?,
                PromptKind::GitCmd => app.run_git_cmd(&input)?,
                PromptKind::Shell => app.run_shell_raw(&input)?,
            }
        }
        KeyCode::Backspace => {
            app.input.pop();
        }
        KeyCode::Char(c) => app.input.push(c),
        _ => {}
    }
    Ok(())
}
