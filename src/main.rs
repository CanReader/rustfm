mod app;
mod background;
mod clipboard;
mod config;
mod events;
mod fs_ops;
mod fuzzy;
mod git;
mod opener;
mod preview;
mod theme;
mod ui;

use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{env, io, path::PathBuf, process};

use crate::{app::App, config::Config};

fn main() {
    if let Err(e) = run() {
        eprintln!("rustfm: {e:#}");
        process::exit(1);
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let start_path = match args.get(1).map(PathBuf::from) {
        Some(p) if p.exists() => p.canonicalize()?,
        _ => env::current_dir()?,
    };

    let config = Config::load().unwrap_or_else(|e| {
        eprintln!("rustfm: config error: {e}, using defaults");
        Config::default()
    });

    // Image preview picker must be created BEFORE raw mode is entered because
    // `from_query_stdio` reads a response from the terminal on stdin.
    let picker = ratatui_image::picker::Picker::from_query_stdio()
        .ok()
        .or_else(|| Some(ratatui_image::picker::Picker::from_fontsize((8, 16))));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(start_path, config, picker)?;
    let result = events::run_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}
