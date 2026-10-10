use super::git::{https_url, inspect};
use crate::category::{Category, Env};
use std::fs;
use std::path::Path;

#[test]
fn rewrites_ssh_remotes_like_go() {
    assert_eq!(
        https_url("git@github.com-personal:sumdahl/media.git"),
        "https://github.com/sumdahl/media.git"
    );
    assert_eq!(
        https_url("ssh://git@github.com/a/b"),
        "https://github.com/a/b"
    );
    assert_eq!(
        https_url("https://gitlab.com/a/b"),
        "https://gitlab.com/a/b"
    );
}

fn git(dir: &Path, args: &[&str]) {
    let mut cmd = vec![
        "git",
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@t",
        "-c",
        "init.defaultBranch=main",
    ];
    cmd.extend_from_slice(args);
    crate::sys::output(Some(dir), &cmd).unwrap_or_else(|e| unreachable!("{e}"));
}

// A theme, not a plugin: Omarchy's real plugin validator would reject the fake.
#[test]
fn bundled_work_restores_exactly() {
    let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
    let remote = t.path().join("remote.git");
    fs::create_dir_all(&remote).ok();
    git(&remote, &["init", "-q", "--bare"]);
    let home = t.path().join("home");
    let plugin = home.join(".config/omarchy/themes/p");
    fs::create_dir_all(&plugin).ok();
    git(&plugin, &["init", "-q"]);
    fs::write(plugin.join("a.qml"), "pushed").ok();
    git(&plugin, &["add", "-A"]);
    git(&plugin, &["commit", "-q", "-m", "one"]);
    git(
        &plugin,
        &["remote", "add", "origin", &remote.to_string_lossy()],
    );
    git(&plugin, &["push", "-q", "origin", "main"]);
    fs::write(plugin.join("a.qml"), "unpushed").ok();
    git(&plugin, &["commit", "-qam", "two"]);
    fs::write(plugin.join("b.qml"), "uncommitted").ok();
    let st = inspect(&plugin).unwrap_or_else(|| unreachable!("a checkout"));
    assert_eq!((st.unpushed, st.dirty), (1, true));

    let tree = t.path().join("tree");
    let log = |_: &str, _: &str| {};
    let env = Env::new(&home, &tree, &log);
    let cat = super::themes();
    let items = cat.detect(&env).unwrap_or_default();
    assert_eq!(items.first().map(|i| i.note.as_str()), Some("bundle"));
    cat.capture(&env, &items)
        .unwrap_or_else(|e| unreachable!("capture: {e}"));

    let fresh = t.path().join("fresh");
    let env2 = Env::new(&fresh, &tree, &log);
    let res = cat.restore(&env2).unwrap_or_default();
    assert_eq!(res.applied, 1, "{:?}", res.failed);
    let restored = fresh.join(".config/omarchy/themes/p");
    assert_eq!(
        fs::read_to_string(restored.join("a.qml")).ok().as_deref(),
        Some("unpushed")
    );
    assert_eq!(
        fs::read_to_string(restored.join("b.qml")).ok().as_deref(),
        Some("uncommitted")
    );
    let log =
        crate::sys::output(Some(&restored), &["git", "log", "--format=%s"]).unwrap_or_default();
    assert_eq!(
        log, "two\none",
        "history kept, uncommitted work not committed"
    );
}
