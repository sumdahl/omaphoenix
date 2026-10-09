//! The OmaPhoenix TUI: the joint product's six sections (DESIGN.md §4) in
//! the terminal (§12). It only renders: every action is a call into the
//! engine, and anything that prompts is handed the plain terminal.

mod state;
mod theme;
mod view;

use std::io::{self, Write};
use std::time::Duration;

use phx_core::App;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};

use state::{State, Suspend};

/// At most 30 redraws a second.
const FRAME: Duration = Duration::from_millis(33);

pub fn run(app: &App<'_>) -> io::Result<()> {
    let mut st = State::new(app);
    st.start();
    let mut term = ratatui::init();
    let result = event_loop(&mut term, &mut st);
    ratatui::restore();
    result
}

fn event_loop(term: &mut DefaultTerminal, st: &mut State<'_>) -> io::Result<()> {
    let mut dirty = true;
    while !st.quit {
        if let Some(work) = st.suspend.take() {
            ratatui::restore();
            hand_over(work);
            *term = ratatui::init();
            term.clear()?;
            *st = State::new(st.app);
            st.start();
            dirty = true;
        }
        if dirty {
            term.draw(|f| view::draw(f, st))?;
            dirty = false;
        }
        if event::poll(FRAME)? {
            if let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                st.on_key(key);
            }
            dirty = true;
        }
        dirty |= st.tick();
    }
    Ok(())
}

/// Runs work that needs the plain terminal, then waits for Enter.
fn hand_over(work: Suspend) {
    let mut out = io::stdout();
    let status = match work {
        Suspend::Cli(args) => std::env::current_exe().and_then(|exe| {
            phx_core::sys::command(&exe.to_string_lossy())
                .args(&args)
                .status()
        }),
        Suspend::Fix(fix) => {
            let _ = writeln!(out, "\n  \x1b[2m$ {fix}\x1b[0m");
            phx_core::sys::command("bash").arg("-c").arg(&fix).status()
        }
        Suspend::SecauditTui => {
            if phx_core::sys::has("secaudit") {
                return drop(phx_core::sys::command("secaudit").arg("tui").status());
            }
            let _ = writeln!(
                out,
                "\n  The Security Audit app (secaudit) is not installed; OmaPhoenix's Audit section covers the quick scan."
            );
            Ok(std::process::ExitStatus::default())
        }
    };
    let ok = status.is_ok_and(|s| s.success());
    let _ = write!(
        out,
        "\n  {}\n  Press Enter to return to OmaPhoenix… ",
        if ok { "✓ done" } else { "✗ did not finish" }
    );
    let _ = out.flush();
    let mut line = String::new();
    let _ = io::stdin().read_line(&mut line);
}
