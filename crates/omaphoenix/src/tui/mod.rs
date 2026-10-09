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

/// Screen tests on ratatui's in-memory backend (DESIGN.md §12): every
/// section reachable by keys and readable at 80×24, nothing panics when the
/// terminal is tiny.
#[cfg(test)]
mod tests {
    use phx_core::App;
    use phx_core::store::Repo;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use secaudit_core::store::Store;

    use super::state::{Section, State};
    use super::view;

    fn quiet(_: &str, _: &str) {}

    fn screen(st: &State<'_>, w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap_or_else(|e| unreachable!("{e}"));
        t.draw(|f| view::draw(f, st))
            .unwrap_or_else(|e| unreachable!("{e}"));
        let buf = t.backend().buffer().clone();
        (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| buf.cell((x, y)).map_or(" ", |c| c.symbol()))
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn key(st: &mut State<'_>, c: KeyCode) {
        st.on_key(KeyEvent::new(c, KeyModifiers::NONE));
    }

    #[test]
    fn every_section_by_key_at_80x24_and_tiny() {
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        let app = App {
            home: t.path().into(),
            repo: Repo {
                dir: t.path().join("repo"),
            },
            log: &quiet,
            passphrase: None,
        };
        let mut st = State::with_store(
            &app,
            Store {
                dir: t.path().join("audit"),
            },
        );
        let s = screen(&st, 80, 24);
        assert!(
            s.contains("1 Pulse") && s.contains("6 Settings"),
            "all six sections fit at 80 columns:\n{s}"
        );
        assert!(
            s.contains("Not set up: nothing is protected yet"),
            "honest when nothing is protected:\n{s}"
        );
        assert!(
            s.contains("Set up snapshots"),
            "the primary action follows state:\n{s}"
        );

        let expect = [
            ('2', Section::Timeline, "No snapshots yet"),
            ('3', Section::Audit, "No audit yet."),
            ('5', Section::Migrate, "Migrate to a new machine"),
            ('6', Section::Settings, "(not set up: i)"),
        ];
        for (k, section, text) in expect {
            key(&mut st, KeyCode::Char(k));
            assert_eq!(st.section, section);
            let s = screen(&st, 80, 24);
            assert!(s.contains(text), "section {k}:\n{s}");
            let _ = screen(&st, 30, 8); // tiny terminal: must not panic
        }
        key(&mut st, KeyCode::Tab);
        assert_eq!(st.section, Section::Pulse, "tab wraps around");
        key(&mut st, KeyCode::Char('q'));
        assert!(st.quit);
    }
}
