use std::fs;
use std::path::Path;

use super::git::git;
use super::{GitRef, GitState};
use crate::category::Error;
use crate::category::files::SECRET_SHAPE;
use crate::sys;

const TIP_REF: &str = "refs/omaphoenix/tip";
const IDENTITY: &[&str] = &[
    "-c",
    "user.name=omaphoenix",
    "-c",
    "user.email=omaphoenix@localhost",
];

/// Bundles the work the remote lacks; refuses secret-shaped changes so the caller copies instead.
pub(super) fn capture_work(
    dir: &Path,
    bundle: &Path,
    st: &GitState,
    git_ref: &mut GitRef,
) -> Result<(), Error> {
    let mut tip = git_ref.commit.clone();
    if st.dirty {
        let index = tempfile::NamedTempFile::new()?.into_temp_path();
        let index_s = index.to_string_lossy().into_owned();
        fs::remove_file(&index)?;
        let idx = [("GIT_INDEX_FILE", index_s.as_str())];
        sys::output_env(Some(dir), &idx, &["git", "add", "-A"])?;
        let tree = sys::output_env(Some(dir), &idx, &["git", "write-tree"])?;
        // Dates pinned to HEAD's so unchanged work hashes the same every run.
        let date = git(dir, &["show", "-s", "--format=%cI", &git_ref.commit])?;
        let pinned = [
            ("GIT_AUTHOR_DATE", date.as_str()),
            ("GIT_COMMITTER_DATE", date.as_str()),
        ];
        let mut argv = vec!["git"];
        argv.extend_from_slice(IDENTITY);
        argv.extend_from_slice(&[
            "commit-tree",
            &tree,
            "-p",
            &git_ref.commit,
            "-m",
            "omaphoenix: uncommitted work",
        ]);
        let work = sys::output_env(Some(dir), &pinned, &argv)?;
        git_ref.work.clone_from(&work);
        tip = work;
    }
    let diff = git(dir, &["log", "-p", "--format=", &tip, "--not", "--remotes"])?;
    if SECRET_SHAPE
        .as_ref()
        .is_some_and(|re| re.is_match(diff.as_bytes()))
    {
        return Err(Error::Other(
            "its local changes contain something shaped like a secret".into(),
        ));
    }
    git(dir, &["update-ref", TIP_REF, &tip])?;
    let result = (|| -> Result<(), Error> {
        if let Some(parent) = bundle.parent() {
            fs::create_dir_all(parent)?;
        }
        let bundle_s = bundle.to_string_lossy();
        // One pack thread keeps the bundle bytes identical for identical commits.
        git(
            dir,
            &[
                "-c",
                "pack.threads=1",
                "bundle",
                "create",
                "-q",
                &bundle_s,
                TIP_REF,
                "--not",
                "--remotes",
            ],
        )?;
        if let Ok(b) = git(dir, &["symbolic-ref", "-q", "--short", "HEAD"]) {
            git_ref.branch = b;
        }
        Ok(())
    })();
    let _ = git(dir, &["update-ref", "-d", TIP_REF]);
    result
}

/// Replays a bundle into a fresh clone, leaving uncommitted work uncommitted.
pub(super) fn apply_work(clone: &Path, r: &GitRef) -> Result<(), Error> {
    git(
        clone,
        &["fetch", "-q", &r.bundle, &format!("{TIP_REF}:{TIP_REF}")],
    )
    .map_err(|e| Error::Other(format!("bundle: {e}")))?;
    let result = (|| -> Result<(), Error> {
        if r.branch.is_empty() {
            git(clone, &["checkout", "-q", &r.commit])?;
        } else {
            git(clone, &["checkout", "-q", "-B", &r.branch, &r.commit])?;
        }
        if r.work.is_empty() {
            return Ok(());
        }
        git(clone, &["checkout", &r.work, "--", "."])?;
        let deleted = sys::lines(
            Some(clone),
            &[
                "git",
                "diff",
                "--name-only",
                "--diff-filter=D",
                &r.commit,
                &r.work,
            ],
        )?;
        for f in deleted {
            let _ = fs::remove_file(clone.join(f));
        }
        git(clone, &["reset", "-q"])?;
        Ok(())
    })();
    let _ = git(clone, &["update-ref", "-d", TIP_REF]);
    result
}

pub(super) fn bundle_name(id: &str) -> String {
    format!("{}.bundle", id.replace('/', "_"))
}
