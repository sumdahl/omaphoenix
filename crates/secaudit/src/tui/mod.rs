//! `secaudit-tui`: the exact Rust port of the Python Textual app of the
//! same name: same screens, keys, layout, colours and messages, sharing its
//! history in `$XDG_STATE_HOME/secaudit`.

mod app;
mod scan;
mod theme;
mod times;
mod view;
mod widgets;

use std::io;
use std::time::Duration;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind,
};
use ratatui::crossterm::execute;
use secaudit_core::store::Store;

use app::{App, Severity, Suspend};

/// Textual's 10 Hz spinner and clock need no faster loop.
const FRAME: Duration = Duration::from_millis(33);

/// Runs the app; `scan` starts a scan right away (`--scan`).
///
/// # Errors
/// If the terminal can't be set up or drawn.
pub fn run(scan: Option<&str>) -> io::Result<()> {
    let store = Store::from_env();
    let _ = std::fs::create_dir_all(store.runs_dir());
    store.prune_incomplete(600);
    let mut app = App::new(store);
    let mut term = ratatui::init();
    let _ = execute!(io::stdout(), EnableMouseCapture);
    if let Some(mode) = scan {
        app.start_scan(mode);
    }
    let result = event_loop(&mut term, &mut app);
    if let app::Screen::Scan(s) = &mut app.screen {
        s.cancel();
    }
    let _ = execute!(io::stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

fn event_loop(term: &mut DefaultTerminal, app: &mut App) -> io::Result<()> {
    let size = term.size()?;
    app.size = (size.width, size.height);
    let mut dirty = true;
    while !app.quit {
        if let Some(work) = app.suspend.take() {
            // Hand the plain terminal over: sudo prompts and fix output need it.
            let _ = execute!(io::stdout(), DisableMouseCapture);
            ratatui::restore();
            match work {
                Suspend::Sudo(mode) => {
                    if scan::ensure_sudo(&mode) {
                        app.launch(&mode);
                    } else {
                        app.notify(
                            "No sudo access, scan cancelled. Quick scan works without sudo.",
                            Severity::Warning,
                            5,
                        );
                    }
                }
                Suspend::Fixes(ns) => app.run_fixes(&ns),
            }
            *term = ratatui::init();
            let _ = execute!(io::stdout(), EnableMouseCapture);
            term.clear()?;
            dirty = true;
        }
        if dirty {
            term.draw(|f| view::draw(f, app))?;
            dirty = false;
        }
        if event::poll(FRAME)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => app.on_key(key),
                Event::Mouse(m) => app.on_mouse(m),
                Event::Resize(w, h) => app.size = (w, h),
                _ => {}
            }
            dirty = true;
        }
        dirty |= app.tick();
    }
    Ok(())
}

/// Screen tests on ratatui's in-memory backend: keys in, text out.
#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use secaudit_core::store::{Meta, Store};

    use super::app::{App, Modal, Screen};
    use super::view;

    fn screen(app: &mut App, w: u16, h: u16) -> String {
        app.size = (w, h);
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap_or_else(|e| unreachable!("{e}"));
        t.draw(|f| view::draw(f, app))
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

    /// The app as it opens, without the "engine not found" toast (tests
    /// run without the engine binary).
    fn open(store: Store) -> App {
        let mut app = App::new(store);
        app.toasts.clear();
        app
    }

    fn key(app: &mut App, c: KeyCode) {
        app.on_key(KeyEvent::new(c, KeyModifiers::NONE));
    }

    fn store_with_scan() -> (tempfile::TempDir, Store) {
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        let store = Store {
            dir: t.path().join("secaudit"),
        };
        let mut meta = Meta {
            mode: "full".into(),
            started: 1,
            host: "box".into(),
            ..Meta::default()
        };
        let dir = store.begin("20261010-010000", &meta).unwrap_or_default();
        std::fs::write(
            dir.join("findings.jsonl"),
            concat!(
                r#"{"sev":"fail","section":"SSH","msg":"~/.ssh perms 755 (want 700)","detail":"","fix":"chmod 700 ~/.ssh","hint":""}"#, "\n",
                r#"{"sev":"warn","section":"Firewall","msg":"Some ufw rules allow from Anywhere","detail":"22/tcp ALLOW IN Anywhere","fix":"","hint":"Scope rules to your LAN"}"#, "\n",
                r#"{"sev":"pass","section":"SSH","msg":"sshd not enabled","detail":"","fix":"","hint":""}"#, "\n",
            ),
        )
        .ok();
        store.finish(&dir, &mut meta).ok();
        (t, store)
    }

    #[test]
    fn empty_history_shows_the_welcome() {
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        let mut app = open(Store {
            dir: t.path().into(),
        });
        let s = screen(&mut app, 100, 30);
        assert!(
            s.contains("No scans yet")
                && s.contains("Full scan")
                && s.contains("Quick scan")
                && s.contains("Deep scan"),
            "{s}"
        );
        assert!(s.contains(" s Scan  x Fix  space Select"), "footer:\n{s}");
    }

    #[test]
    fn a_scan_lays_out_like_the_original() {
        let (_t, store) = store_with_scan();
        let mut app = open(store);
        let s = screen(&mut app, 120, 36);
        let rows: Vec<&str> = s.lines().collect();
        assert!(
            rows.first()
                .is_some_and(|r| r.starts_with(" ◆ Security Audit")),
            "{s}"
        );
        assert!(
            s.contains("✗ 1 fail") && s.contains("! 1 warn") && s.contains("✓ 1 pass"),
            "stats:\n{s}"
        );
        assert!(s.contains("SCORE TREND"), "trend box at 120 columns:\n{s}");
        assert!(
            rows.get(9).is_some_and(|r| r.starts_with(" SECTIONS")
                && r.contains("Issues 2  Accepted 0  Notes 0  Passed 1  All 3")),
            "{s}"
        );
        assert!(
            s.contains("✗ FAIL   SSH") && s.contains("● fix"),
            "table:\n{s}"
        );
        assert!(
            s.contains("SUGGESTED FIX") && s.contains("chmod 700 ~/.ssh"),
            "detail:\n{s}"
        );
        assert!(
            s.contains("Run fix  x") && s.contains("Accept risk  a"),
            "buttons:\n{s}"
        );

        key(&mut app, KeyCode::Down);
        assert!(
            screen(&mut app, 120, 36).contains("22/tcp ALLOW IN Anywhere"),
            "detail follows the selection"
        );
        key(&mut app, KeyCode::Char('4'));
        assert!(
            screen(&mut app, 120, 36).contains("sshd not enabled"),
            "Passed tab"
        );
        key(&mut app, KeyCode::Char('5'));
        key(&mut app, KeyCode::Char('/'));
        for c in "ufw".chars() {
            key(&mut app, KeyCode::Char(c));
        }
        let s = screen(&mut app, 120, 36);
        assert!(
            s.contains("allow from Anywhere") && !s.contains("sshd not enabled"),
            "search filters:\n{s}"
        );
        assert!(
            s.lines().last().is_some_and(|l| l.starts_with(" / Search")),
            "search footer:\n{s}"
        );
        let narrow = screen(&mut app, 80, 24);
        assert!(
            !narrow.contains("SCORE TREND"),
            "no trend box under 110 columns"
        );
    }

    #[test]
    fn accepting_a_risk_rescores_and_moves_it() {
        let (_t, store) = store_with_scan();
        let mut app = open(store);
        key(&mut app, KeyCode::Char('a'));
        let s = screen(&mut app, 120, 36);
        assert!(
            s.contains("≈ 1 accepted") && s.contains("Accepted 1"),
            "{s}"
        );
        assert!(s.contains("Accepted, no longer scored"), "toast:\n{s}");
        key(&mut app, KeyCode::Char('2'));
        key(&mut app, KeyCode::Char('a'));
        assert!(
            screen(&mut app, 120, 36).contains("Accepted 0"),
            "undo with a again"
        );
    }

    #[test]
    fn marking_and_the_fix_dialog() {
        let (_t, store) = store_with_scan();
        let mut app = open(store);
        key(&mut app, KeyCode::Char(' '));
        assert!(
            screen(&mut app, 120, 36).contains("◉ 1 selected"),
            "selection bar"
        );
        key(&mut app, KeyCode::Char('x'));
        let s = screen(&mut app, 120, 36);
        assert!(
            s.contains("Run this fix?")
                && s.contains("$ chmod 700 ~/.ssh")
                && s.contains("Cancel  n"),
            "{s}"
        );
        key(&mut app, KeyCode::Char('n'));
        assert!(app.modal.is_none() && app.suspend.is_none());
        key(&mut app, KeyCode::Char('s'));
        assert!(matches!(app.modal, Some(Modal::Chooser(0))));
        assert!(screen(&mut app, 120, 36).contains("Start a scan"));
        key(&mut app, KeyCode::Esc);
        assert!(app.modal.is_none());
    }

    #[test]
    fn history_help_and_palette() {
        let (_t, store) = store_with_scan();
        let mut app = open(store);
        key(&mut app, KeyCode::Char('h'));
        assert!(matches!(app.screen, Screen::History(0)));
        let s = screen(&mut app, 120, 36);
        assert!(
            s.contains("First scan on record; nothing to compare against.")
                && s.contains("1 kept (max 30)"),
            "{s}"
        );
        key(&mut app, KeyCode::Esc);
        key(&mut app, KeyCode::Char('?'));
        assert!(
            screen(&mut app, 120, 36).contains("Cursor up"),
            "keys panel"
        );
        key(&mut app, KeyCode::Char('?'));
        app.on_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
        assert!(screen(&mut app, 120, 36).contains("Search for commands"));
        for c in "hist".chars() {
            key(&mut app, KeyCode::Char(c));
        }
        key(&mut app, KeyCode::Enter);
        assert!(
            matches!(app.screen, Screen::History(_)),
            "palette runs Scan history"
        );
        key(&mut app, KeyCode::Char('q'));
        assert!(app.quit);
    }
}
