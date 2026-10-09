//! `secaudit tui`: scan, review, accept risks, apply fixes, compare scans.
//! The Rust successor of the Python `secaudit-tui`, sharing its history.

mod app;
mod scan;
mod theme;
mod view;

use std::io;
use std::time::Duration;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};

use app::{App, Suspend};

/// At most 30 redraws a second (DESIGN.md §12).
const FRAME: Duration = Duration::from_millis(33);

pub fn run(scan: Option<&str>) -> io::Result<()> {
    let mut app = App::new();
    let mut term = ratatui::init();
    if let Some(mode) = scan {
        app.start_scan(mode);
    }
    let result = event_loop(&mut term, &mut app);
    ratatui::restore();
    result
}

fn event_loop(term: &mut DefaultTerminal, app: &mut App) -> io::Result<()> {
    let mut dirty = true;
    let mut last_second = std::time::Instant::now();
    while !app.quit {
        if let Some(work) = app.suspend.take() {
            // Hand the plain terminal over: sudo prompts and fix output need it.
            ratatui::restore();
            match work {
                Suspend::Sudo(mode) => {
                    if scan::ensure_sudo(&mode) {
                        app.launch(&mode);
                    } else {
                        app.notify(
                            "No sudo access, scan cancelled. Quick scan works without sudo.",
                        );
                    }
                }
                Suspend::Fixes(ns) => app.run_fixes(&ns),
            }
            *term = ratatui::init();
            term.clear()?;
            dirty = true;
        }
        if dirty {
            term.draw(|f| view::draw(f, app))?;
            dirty = false;
        }
        if event::poll(FRAME)? {
            // Any event redraws: a key press, a resize.
            if let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                app.on_key(key);
            }
            dirty = true;
        }
        dirty |= app.tick();
        // A running scan's clock ticks once a second.
        if matches!(app.screen, app::Screen::Scan(_))
            && last_second.elapsed() >= Duration::from_secs(1)
        {
            dirty = true;
            last_second = std::time::Instant::now();
        }
    }
    Ok(())
}
