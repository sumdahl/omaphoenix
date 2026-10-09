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

/// Screen tests on ratatui's in-memory backend at 80×24 (DESIGN.md §12):
/// keys in, text out, no terminal needed.
#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use secaudit_core::store::{Meta, Store};

    use super::app::{App, Screen};
    use super::view;

    fn screen(app: &App) -> String {
        let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap_or_else(|e| unreachable!("{e}"));
        t.draw(|f| view::draw(f, app))
            .unwrap_or_else(|e| unreachable!("{e}"));
        let buf = t.backend().buffer().clone();
        (0..24)
            .map(|y| {
                (0..80)
                    .map(|x| buf.cell((x, y)).map_or(" ", |c| c.symbol()))
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
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
            mode: "quick".into(),
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
    fn empty_history_invites_a_scan() {
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        let app = App::with_store(Store {
            dir: t.path().into(),
        });
        let s = screen(&app);
        assert!(
            s.contains("No scans yet.") && s.contains("Press s to scan"),
            "{s}"
        );
    }

    #[test]
    fn a_scan_shows_score_tabs_and_details_at_80x24() {
        let (_t, store) = store_with_scan();
        let mut app = App::with_store(store);
        let s = screen(&app);
        assert!(
            s.contains("Score 88 B"),
            "score on the top bar at 80 columns:\n{s}"
        );
        assert!(
            s.contains("1 Issues 2") && s.contains("4 Passed 1"),
            "tabs with counts:\n{s}"
        );
        assert!(
            s.contains("✕ FAIL") && s.contains("~/.ssh perms 755"),
            "worst first:\n{s}"
        );
        assert!(
            s.contains("$ chmod 700 ~/.ssh"),
            "selected finding's fix:\n{s}"
        );

        key(&mut app, KeyCode::Down);
        assert!(
            screen(&app).contains("Scope rules to your LAN"),
            "detail follows the selection"
        );

        key(&mut app, KeyCode::Char('4'));
        assert!(screen(&app).contains("sshd not enabled"), "Passed tab");

        key(&mut app, KeyCode::Char('5'));
        key(&mut app, KeyCode::Char('/'));
        for c in "ufw".chars() {
            key(&mut app, KeyCode::Char(c));
        }
        let s = screen(&app);
        assert!(
            s.contains("allow from Anywhere") && !s.contains("sshd not enabled"),
            "search filters:\n{s}"
        );
    }

    #[test]
    fn accepting_a_risk_rescores_and_moves_it() {
        let (_t, store) = store_with_scan();
        let mut app = App::with_store(store);
        key(&mut app, KeyCode::Char('a'));
        let s = screen(&app);
        assert!(
            s.contains("Score 98 A") && s.contains("2 Accepted 1"),
            "{s}"
        );
        key(&mut app, KeyCode::Char('2'));
        key(&mut app, KeyCode::Char('a'));
        assert!(screen(&app).contains("Score 88 B"), "undo with a again");
    }

    #[test]
    fn history_and_help_screens_open_and_close() {
        let (_t, store) = store_with_scan();
        let mut app = App::with_store(store);
        key(&mut app, KeyCode::Char('h'));
        assert!(matches!(app.screen, Screen::History { .. }));
        assert!(screen(&app).contains("First scan on record"));
        key(&mut app, KeyCode::Esc);
        key(&mut app, KeyCode::Char('?'));
        assert!(screen(&app).contains("Keys"));
        key(&mut app, KeyCode::Char('x'));
        assert!(matches!(app.screen, Screen::Main));
        key(&mut app, KeyCode::Char('q'));
        assert!(app.quit);
    }
}
