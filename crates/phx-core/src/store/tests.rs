use super::{Repo, StoreError};
use std::fs;

fn repo() -> (tempfile::TempDir, Repo) {
    let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
    let r = Repo {
        dir: t.path().join("repo"),
    };
    r.init("").unwrap_or_else(|e| unreachable!("init: {e}"));
    (t, r)
}

#[test]
fn knows_when_commits_still_need_pushing() {
    let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
    let bare = t.path().join("remote.git");
    let ok = std::process::Command::new("git")
        .args(["init", "-q", "--bare", "-b", "main"])
        .arg(&bare)
        .status()
        .is_ok_and(|s| s.success());
    assert!(ok, "bare remote");
    let r = Repo {
        dir: t.path().join("repo"),
    };
    r.init(&bare.to_string_lossy())
        .unwrap_or_else(|e| unreachable!("init: {e}"));
    assert!(!r.unpushed(), "nothing committed yet");
    fs::write(r.dir.join("a"), "1").ok();
    assert!(r.commit("one").unwrap_or(false));
    assert!(r.unpushed(), "never pushed");
    r.push().unwrap_or_else(|e| unreachable!("push: {e}"));
    assert!(!r.unpushed(), "in step after a push");
    fs::write(r.dir.join("a"), "2").ok();
    assert!(r.commit("two").unwrap_or(false));
    assert!(r.unpushed(), "a new commit waits for the next push");
}

#[test]
fn commits_only_changes_and_lists_newest_first() {
    let (_t, r) = repo();
    assert!(
        r.list().unwrap_or_default().is_empty(),
        "empty repo lists nothing"
    );
    fs::write(r.dir.join("a"), "1").ok();
    assert!(r.commit("one").unwrap_or(false));
    assert!(!r.commit("same").unwrap_or(true), "no change, no commit");
    fs::write(r.dir.join("a"), "2").ok();
    assert!(r.commit("two").unwrap_or(false));
    let list = r.list().unwrap_or_default();
    assert_eq!(
        list.iter().map(|s| s.subject.as_str()).collect::<Vec<_>>(),
        ["two", "one"]
    );
    assert_eq!(
        r.resolve("").map(|s| s.subject).ok().as_deref(),
        Some("two")
    );
    assert!(matches!(r.resolve("zzz"), Err(StoreError::NotFound(_))));
}

#[test]
fn extracts_a_snapshot_with_modes_and_links() {
    use std::os::unix::fs::PermissionsExt;
    let (t, r) = repo();
    fs::create_dir_all(r.dir.join("d")).ok();
    fs::write(r.dir.join("d/x.sh"), "echo").ok();
    fs::set_permissions(r.dir.join("d/x.sh"), fs::Permissions::from_mode(0o755)).ok();
    std::os::unix::fs::symlink("d/x.sh", r.dir.join("link")).ok();
    r.commit("s").ok();
    let id = r.resolve("").map(|s| s.id).unwrap_or_default();
    let out = t.path().join("out");
    r.extract(&id, &out)
        .unwrap_or_else(|e| unreachable!("extract: {e}"));
    assert_eq!(
        fs::read_to_string(out.join("d/x.sh")).ok().as_deref(),
        Some("echo")
    );
    assert_eq!(
        fs::metadata(out.join("d/x.sh"))
            .map(|m| m.permissions().mode() & 0o111)
            .ok(),
        Some(0o111)
    );
    assert_eq!(fs::read_link(out.join("link")).ok(), Some("d/x.sh".into()));
}

#[test]
fn untar_refuses_escaping_paths() {
    let mut builder = tar::Builder::new(Vec::new());
    let mut h = tar::Header::new_gnu();
    h.set_size(1);
    h.set_mode(0o644);
    let name = b"../evil";
    if let Some(old) = h.as_old_mut().name.get_mut(..name.len()) {
        old.copy_from_slice(name);
    }
    h.set_cksum();
    builder.append(&h, &b"x"[..]).ok();
    let bytes = builder.into_inner().unwrap_or_default();
    let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
    let r = super::untar(&bytes[..], &t.path().join("out"));
    assert!(matches!(r, Err(StoreError::Traversal(_))), "{r:?}");
    assert!(!t.path().join("evil").exists());
}

#[test]
fn lock_is_exclusive() {
    let (_t, r) = repo();
    let held = r.lock();
    assert!(held.is_ok());
    assert!(matches!(r.lock(), Err(StoreError::Locked)));
    drop(held);
    assert!(r.lock().is_ok());
}
