use super::{Accepted, KEEP_RUNS, Meta, Store, accept_key};
use std::collections::BTreeMap;
use std::fs;

fn store() -> (tempfile::TempDir, Store) {
    let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
    let s = Store {
        dir: t.path().join("secaudit"),
    };
    (t, s)
}

const LINES: &str = concat!(
    r#"{"sev":"fail","section":"Secrets & credentials in home","msg":"~/.netrc perms 644 (readable by others, want 600)","detail":"","fix":"chmod 600 ~/.netrc","hint":""}"#,
    "\n",
    r#"{"sev":"warn","section":"Secrets & credentials in home","msg":"~/.zsh_history: 2 line(s) look like secrets/tokens (values not shown)","detail":"","fix":"","hint":""}"#,
    "\n",
    r#"{"sev":"pass","section":"SSH","msg":"sshd not enabled","detail":"","fix":"","hint":""}"#,
    "\n"
);

fn scan(s: &Store, stamp: &str, findings: &str) -> std::path::PathBuf {
    let mut meta = Meta {
        mode: "quick".into(),
        started: 1,
        host: "h".into(),
        ..Meta::default()
    };
    let dir = s.begin(stamp, &meta).unwrap_or_default();
    fs::write(dir.join("findings.jsonl"), findings).ok();
    s.finish(&dir, &mut meta).ok();
    dir
}

#[test]
fn accept_key_masks_numbers_like_python() {
    assert_eq!(
        accept_key("S", "~/.zsh_history: 12 line(s) look like secrets:"),
        "S|~/.zsh_history: # line(s) look like secrets"
    );
    assert_eq!(accept_key("S", "a 1 b 22"), accept_key("S", "a 3 b 4"));
}

#[test]
fn runs_score_with_accepted_risks() {
    let (_t, s) = store();
    scan(&s, "20261010-010000", LINES);
    let runs = s.runs();
    assert_eq!(runs.len(), 1);
    let c = runs[0].counts();
    assert_eq!(
        (c.fail, c.warn, c.pass, c.score(), c.grade()),
        (1, 1, 1, 88, 'B')
    );

    let mut acc = BTreeMap::new();
    let warn = &runs[0].items[1];
    acc.insert(
        warn.accept_key(),
        Accepted {
            section: warn.finding.section.clone(),
            msg: warn.title().into(),
            at: 1,
        },
    );
    s.save_accepted(&acc).ok();
    let c = s.runs()[0].counts();
    assert_eq!((c.warn, c.accepted, c.score()), (0, 1, 90));
}

#[test]
fn keeps_thirty_and_ignores_unfinished() {
    let (_t, s) = store();
    for i in 0..KEEP_RUNS + 2 {
        scan(&s, &format!("20261010-{i:06}"), LINES);
    }
    s.begin("20991231-000000", &Meta::default()).ok();
    let runs = s.runs();
    assert_eq!(runs.len(), KEEP_RUNS);
    assert!(
        runs[0]
            .path
            .ends_with(format!("20261010-{:06}", KEEP_RUNS + 1)),
        "newest first"
    );
}

#[test]
fn compares_and_records_fixes_and_exports() {
    let (_t, s) = store();
    scan(&s, "20261010-000001", LINES);
    let fixed = LINES.lines().skip(1).collect::<Vec<_>>().join("\n");
    scan(&s, "20261010-000002", &fixed);
    let runs = s.runs();
    let resolved: Vec<_> = runs[1]
        .issue_keys()
        .difference(&runs[0].issue_keys())
        .cloned()
        .collect();
    assert_eq!(resolved.len(), 1);
    assert!(resolved[0].2.starts_with("~/.netrc"));

    runs[1].mark_applied(1).ok();
    assert!(runs[1].applied().contains(&1));
    let report = runs[1]
        .export_markdown("2026-10-10 01:00")
        .unwrap_or_default();
    let md = fs::read_to_string(report).unwrap_or_default();
    assert!(
        md.contains("- [ ] **FAIL** ~/.netrc perms 644 (readable by others, want 600)"),
        "{md}"
    );
    assert!(md.contains("  Fix: `chmod 600 ~/.netrc`"));
}
