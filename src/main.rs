mod app;
mod background;
mod clipboard;
mod config;
mod events;
mod fs_ops;
mod fuzzy;
mod git;
mod metadata;
mod opener;
mod panel;
mod preview;
mod sidebar;
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
    let arg_path = match args.get(1).map(PathBuf::from) {
        Some(p) if p.exists() => Some(p.canonicalize()?),
        _ => None,
    };
    // A file argument opens its parent directory with the cursor on the
    // file; a directory argument opens that directory.
    let (start_path, focus) = match arg_path {
        Some(p) if p.is_dir() => (p, None),
        Some(p) => {
            let parent = p.parent().map(PathBuf::from).unwrap_or(env::current_dir()?);
            (parent, Some(p))
        }
        None => (env::current_dir()?, None),
    };

    let config = Config::load().unwrap_or_else(|e| {
        eprintln!("rustfm: config error: {e}, using defaults");
        Config::default()
    });

    // Image preview picker must be created BEFORE raw mode is entered because
    // `from_query_stdio` reads a response from the terminal on stdin.
    //
    // Under tmux/screen the capability query may never be answered; the
    // reader ratatui-image leaves behind then blocks on stdin forever and
    // steals every subsequent keystroke from crossterm, freezing the UI.
    // Skip the query there and fall back to half-block rendering.
    let in_multiplexer = env::var_os("TMUX").is_some()
        || env::var("TERM")
            .map(|t| t.starts_with("screen") || t.starts_with("tmux"))
            .unwrap_or(false);
    let picker = if in_multiplexer {
        Some(ratatui_image::picker::Picker::from_fontsize((8, 16)))
    } else {
        ratatui_image::picker::Picker::from_query_stdio()
            .ok()
            .or_else(|| Some(ratatui_image::picker::Picker::from_fontsize((8, 16))))
    };

    // Install a panic hook BEFORE entering raw mode / alt screen so that
    // any panic anywhere (during draw, in the event loop, in a worker we
    // join later, …) restores the terminal before the default hook prints
    // the panic message. Without this, a panic mid-draw leaves the user's
    // shell in raw mode + alt screen and they have to blindly type `reset`.
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        original_hook(info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(start_path, config, picker)?;
    if let Some(file) = focus {
        app.focus_file(&file);
    }
    let result = events::run_loop(&mut terminal, &mut app);

    // Best-effort cleanup. A failure restoring one piece of state should not
    // prevent us from restoring the rest, so we use `let _` and only return
    // the original loop's error to the caller.
    let _ = disable_raw_mode();
    let _ = execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    );
    let _ = terminal.show_cursor();

    result
}
