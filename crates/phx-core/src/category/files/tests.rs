use super::{copy_tree, same_file};
use crate::category::Env;
use std::fs;

fn tmp() -> tempfile::TempDir {
    tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"))
}

#[test]
fn same_file_streams() {
    let t = tmp();
    let (a, b, c) = (t.path().join("a"), t.path().join("b"), t.path().join("c"));
    let big = "x".repeat(200_000);
    fs::write(&a, &big).ok();
    fs::write(&b, &big).ok();
    fs::write(&c, format!("{}y", &big[1..])).ok();
    assert!(same_file(&a, &b));
    assert!(!same_file(&a, &c));
    assert!(!same_file(&a, &t.path().join("missing")));
}

#[test]
fn copy_tree_skips_junk_secrets_and_big_files() {
    let t = tmp();
    let src = t.path().join("src");
    for (p, body) in [
        ("keep.conf", "a"),
        ("sub/x.conf", "b"),
        (".git/HEAD", "c"),
        ("leak.conf", "k = ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
    ] {
        let f = src.join(p);
        fs::create_dir_all(f.parent().unwrap_or(&src)).ok();
        fs::write(&f, body).ok();
    }
    fs::write(src.join("big.bin"), vec![0u8; 2048]).ok();
    let logs = std::cell::RefCell::new(Vec::new());
    let log = |l: &str, m: &str| logs.borrow_mut().push(format!("{l} {m}"));
    let env = Env::new(t.path(), t.path(), &log);
    let dst = t.path().join("dst");
    let n = copy_tree(&env, &src, &dst, 1024, &|_, _, _| false).unwrap_or(0);
    assert_eq!(n, 2);
    assert!(
        dst.join("sub/x.conf").exists()
            && !dst.join(".git").exists()
            && !dst.join("leak.conf").exists()
    );
    assert!(!dst.join("big.bin").exists());
    assert_eq!(logs.borrow().len(), 2, "{:?}", logs.borrow());
}

#[test]
fn a_snapshot_moves_unchanged_files_back_and_copies_changed_ones() {
    let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
    let (src, repo, prev) = (
        t.path().join("src"),
        t.path().join("repo"),
        t.path().join("prev"),
    );
    for d in [&src, &repo, &prev] {
        fs::create_dir_all(d).unwrap_or_else(|e| unreachable!("{e}"));
    }
    fs::write(src.join("same"), "kept").ok();
    fs::write(src.join("changed"), "new!").ok();
    super::copy_file(&src.join("same"), &prev.join("same"), 0o644)
        .unwrap_or_else(|e| unreachable!("{e}"));
    fs::write(prev.join("changed"), "old").ok();
    let inode = |p: &std::path::Path| {
        std::os::unix::fs::MetadataExt::ino(
            &fs::metadata(p).unwrap_or_else(|e| unreachable!("{e}")),
        )
    };
    let reused = inode(&prev.join("same"));

    super::set_reuse(Some((repo.clone(), prev.clone())));
    super::copy_file(&src.join("same"), &repo.join("same"), 0o644)
        .unwrap_or_else(|e| unreachable!("{e}"));
    super::copy_file(&src.join("changed"), &repo.join("changed"), 0o644)
        .unwrap_or_else(|e| unreachable!("{e}"));
    super::set_reuse(None);

    assert_eq!(
        inode(&repo.join("same")),
        reused,
        "unchanged: the old copy moved back, not rewritten"
    );
    assert!(!prev.join("same").exists());
    assert_eq!(
        fs::read_to_string(repo.join("changed")).unwrap_or_default(),
        "new!",
        "changed: copied"
    );
    assert!(
        prev.join("changed").exists(),
        "the stale copy is left behind to be dropped"
    );
}

#[test]
fn copy_tree_copies_a_single_file() {
    let t = tmp();
    let src = t.path().join("brave-flags.conf");
    fs::write(&src, "--x").ok();
    let log = |_: &str, _: &str| {};
    let env = Env::new(t.path(), t.path(), &log);
    let dst = t.path().join("out/brave-flags.conf");
    assert_eq!(
        copy_tree(&env, &src, &dst, 1024, &|_, _, _| false).ok(),
        Some(1)
    );
    assert_eq!(fs::read_to_string(&dst).ok().as_deref(), Some("--x"));
}
