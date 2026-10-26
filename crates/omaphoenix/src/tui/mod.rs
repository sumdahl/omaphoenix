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

pub use state::Section;
use state::{State, Suspend};

/// At most 30 redraws a second.
const FRAME: Duration = Duration::from_millis(33);

/// Runs the TUI, optionally opening on `section`.
pub fn run(app: &App<'_>, section: Option<Section>) -> io::Result<()> {
    let mut st = State::new(app);
    st.start();
    if let Some(s) = section {
        st.section = s;
    }
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
    use phx_audit::store::Store;

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

    #[test]
    fn unified_keymap_confirms_searches_and_names_every_action() {
        use phx_audit::store::Meta;
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        let app = App {
            home: t.path().into(),
            repo: Repo {
                dir: t.path().join("repo"),
            },
            log: &quiet,
            passphrase: None,
        };
        let store = Store {
            dir: t.path().join("audit"),
        };
        let mut meta = Meta {
            mode: "quick".into(),
            started: 1,
            ..Meta::default()
        };
        let dir = store
            .begin("20261010-010000", &meta)
            .unwrap_or_else(|e| unreachable!("{e}"));
        std::fs::write(
            dir.join("findings.jsonl"),
            concat!(
                r#"{"sev":"fail","section":"SSH","msg":"~/.ssh perms 755","detail":"","fix":"chmod 700 ~/.ssh","hint":""}"#,
                "\n",
                r#"{"sev":"warn","section":"Firewall","msg":"ufw allows Anywhere","detail":"","fix":"","hint":"scope it"}"#,
                "\n",
            ),
        )
        .unwrap_or_else(|e| unreachable!("{e}"));
        store
            .finish(&dir, &mut meta)
            .unwrap_or_else(|e| unreachable!("{e}"));
        let mut st = State::with_store(&app, store);

        key(&mut st, KeyCode::Char('3'));
        key(&mut st, KeyCode::Char('x'));
        assert!(st.suspend.is_none(), "x never runs a fix without asking");
        assert!(screen(&st, 100, 24).contains("Run this fix in the terminal?"));
        key(&mut st, KeyCode::Char('n'));
        assert!(
            st.confirm.is_none() && st.suspend.is_none(),
            "anything but y cancels"
        );
        key(&mut st, KeyCode::Char('x'));
        key(&mut st, KeyCode::Char('y'));
        assert!(
            matches!(st.suspend, Some(super::Suspend::Fix(_))),
            "y runs it"
        );
        st.suspend = None;

        key(&mut st, KeyCode::Char('/'));
        for c in "ufw".chars() {
            key(&mut st, KeyCode::Char(c));
        }
        key(&mut st, KeyCode::Enter);
        assert_eq!(st.audit_items().len(), 1, "/ filters the audit list");
        key(&mut st, KeyCode::Esc);
        assert_eq!(st.audit_items().len(), 2, "esc clears the search");

        key(&mut st, KeyCode::Char('?'));
        let s = screen(&st, 100, 30);
        assert!(
            s.contains("Keys: Audit") && s.contains("rescan"),
            "? lists this screen's keys:\n{s}"
        );
        key(&mut st, KeyCode::Esc);
        assert!(!st.help);

        st.on_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL));
        assert!(
            screen(&st, 100, 30).contains("Take snapshot"),
            "palette lists actions"
        );
        for c in "timeline".chars() {
            key(&mut st, KeyCode::Char(c));
        }
        key(&mut st, KeyCode::Enter);
        assert_eq!(st.section, Section::Timeline, "palette runs Go to Timeline");

        key(&mut st, KeyCode::Char('u'));
        assert!(
            st.suspend.is_none(),
            "nothing to undo before setup, and never without asking"
        );
    }

    #[test]
    fn nothing_is_backed_up_until_the_user_chooses_and_new_items_are_asked_about() {
        use phx_contract::selection::Choice;
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        std::fs::create_dir_all(t.path().join("repo/.git")).unwrap_or_else(|e| unreachable!("{e}"));
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
        st.choices = Some(Vec::new());

        key(&mut st, KeyCode::Char('2'));
        key(&mut st, KeyCode::Char('s'));
        assert_eq!(st.section, Section::Backup, "s first asks what to back up");
        assert!(!st.jobs.contains("command"), "no snapshot before a choice");
        let s = screen(&st, 100, 24);
        assert!(s.contains("Choose what OmaPhoenix backs up"), "{s}");
        assert!(
            s.contains("choose what to back up (4)"),
            "the activity line says so:\n{s}"
        );

        let theme = Choice {
            category: "theme".into(),
            items: vec![("nord".into(), true)],
        };
        app.save_choices(&[theme])
            .unwrap_or_else(|e| unreachable!("{e}"));
        st.unreviewed.insert("theme:rose".into());
        key(&mut st, KeyCode::Char('2'));
        key(&mut st, KeyCode::Char('s'));
        assert!(
            !st.jobs.contains("command"),
            "new items are asked about first"
        );
        assert!(screen(&st, 120, 24).contains("1 new item(s) you haven't reviewed"));
        key(&mut st, KeyCode::Char('b'));
        assert_eq!(st.section, Section::Backup, "b opens Backup to review them");
        key(&mut st, KeyCode::Char('2'));
        key(&mut st, KeyCode::Char('s'));
        key(&mut st, KeyCode::Esc);
        assert!(
            st.ask_snapshot.is_none() && !st.jobs.contains("command"),
            "esc cancels"
        );
    }

    #[test]
    fn status_and_backup_wording_match_the_setup() {
        use phx_contract::selection::Choice;
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        let repo = t.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap_or_else(|e| unreachable!("{e}"));
        let app = App {
            home: t.path().into(),
            repo: Repo { dir: repo.clone() },
            log: &quiet,
            passphrase: None,
        };
        let mut st = State::with_store(
            &app,
            Store {
                dir: t.path().join("audit"),
            },
        );
        st.interval = Some("daily");
        st.choices = Some(vec![(
            Choice {
                category: "secret".into(),
                items: vec![(".ssh/id_a".into(), true)],
            },
            vec![String::new()],
        )]);
        st.section = Section::Backup;
        let before = screen(&st, 120, 24);
        assert!(
            before.contains("automatic, daily") && !before.contains("every daily"),
            "{before}"
        );
        assert!(
            before.contains("Settings → e first"),
            "before setup, the hint shows:\n{before}"
        );
        std::fs::create_dir_all(repo.join("secrets")).unwrap_or_else(|e| unreachable!("{e}"));
        std::fs::write(repo.join("secrets/recipient.txt"), "age1x")
            .unwrap_or_else(|e| unreachable!("{e}"));
        let after = screen(&st, 120, 24);
        assert!(
            !after.contains("Settings → e first"),
            "after setup, it doesn't:\n{after}"
        );
    }

    #[test]
    fn backup_search_finds_and_unticks_an_item_across_categories() {
        use phx_contract::selection::Choice;
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        std::fs::create_dir_all(t.path().join("repo/.git")).unwrap_or_else(|e| unreachable!("{e}"));
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
        let mut config: Vec<(String, bool)> = (0..100)
            .map(|i| (format!(".config/app-{i}/settings.conf"), true))
            .collect();
        config.push((".claude.json.tmp.159014.3290931ff618".into(), true));
        let choice = |category: &str, items: Vec<(String, bool)>| {
            let notes = vec![String::new(); items.len()];
            (
                Choice {
                    category: category.into(),
                    items,
                },
                notes,
            )
        };
        st.choices = Some(vec![
            choice("config", config),
            choice("bin", vec![(".local/bin/fpl-engine".into(), true)]),
        ]);
        st.section = Section::Backup;
        key(&mut st, KeyCode::Char('/'));
        assert!(st.searching, "/ searches Backup");
        for c in "claude tmp".chars() {
            key(&mut st, KeyCode::Char(c));
        }
        let hits = st.backup_hits();
        assert_eq!(
            hits.first().map(|h| h.key.as_str()),
            Some("config:.claude.json.tmp.159014.3290931ff618")
        );
        assert_eq!(hits.len(), 1, "both terms must match");
        let s = screen(&st, 120, 24);
        assert!(
            s.contains("1 match(es)") && s.contains("[x] config:.claude.json.tmp"),
            "{s}"
        );
        key(&mut st, KeyCode::Tab);
        let off = |st: &State<'_>| {
            st.choices
                .as_ref()
                .and_then(|c| c.first())
                .and_then(|(c, _)| c.items.last())
                .is_some_and(|(_, on)| !*on)
        };
        assert!(off(&st) && st.dirty, "tab unticks while typing");
        key(&mut st, KeyCode::Enter);
        assert!(
            !st.searching && !st.search.is_empty(),
            "enter keeps the results"
        );
        key(&mut st, KeyCode::Char(' '));
        assert!(!off(&st), "space ticks it again");
        key(&mut st, KeyCode::Char(' '));
        assert!(screen(&st, 120, 24).contains("[ ] config:.claude.json.tmp"));
        key(&mut st, KeyCode::Esc);
        assert!(st.search.is_empty(), "esc clears");
        assert!(
            screen(&st, 120, 24).contains("config"),
            "back to the categories"
        );
        assert!(off(&st), "the choice survives clearing the search");

        key(&mut st, KeyCode::Char('/'));
        for c in "zzzz".chars() {
            key(&mut st, KeyCode::Char(c));
        }
        assert!(screen(&st, 120, 24).contains("No item matches"));
        key(&mut st, KeyCode::Esc);
        key(&mut st, KeyCode::Char('/'));
        for c in "app 9".chars() {
            key(&mut st, KeyCode::Char(c));
        }
        let n = st.backup_hits().len();
        key(&mut st, KeyCode::Enter);
        key(&mut st, KeyCode::Char('a'));
        let unticked = st
            .choices
            .as_ref()
            .and_then(|c| c.first())
            .map_or(0, |(c, _)| c.items.iter().filter(|(_, on)| !*on).count());
        assert_eq!(
            unticked,
            n + 1,
            "a unticks every result (plus the .claude one)"
        );
    }
}
