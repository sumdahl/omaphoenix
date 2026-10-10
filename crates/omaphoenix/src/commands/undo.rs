use phx_core::App;

use crate::out::say;

pub fn run(app: &App<'_>, dry_run: bool) -> Result<(), String> {
    let rep = app.undo(dry_run).map_err(|e| e.to_string())?;
    let verb = if dry_run { "undo (dry run)" } else { "undo" };
    say(&format!(
        "{verb} of the restore of snapshot {}",
        rep.snapshot
    ));
    rep.reverted.iter().for_each(|r| say(&format!("  ✓ {r}")));
    rep.kept.iter().for_each(|k| say(&format!("  · {k}")));
    rep.failed.iter().for_each(|f| say(&format!("  ✗ {f}")));
    for (mgr, pkgs) in &rep.packages {
        say(&format!(
            "\n{} package(s) were installed by the restore ({mgr}). To remove them:",
            pkgs.len()
        ));
        if mgr == "pkg-repo" || mgr == "pkg-aur" {
            say(&format!("  sudo pacman -Rns {}", pkgs.join(" ")));
        } else {
            say(&format!("  {}   ({mgr})", pkgs.join(" ")));
        }
    }
    if !dry_run && !rep.reverted.is_empty() {
        say(&format!(
            "\nAnything the restore created is in {}",
            rep.trash.display()
        ));
    }
    if !rep.failed.is_empty() {
        return Err(format!(
            "{} change(s) could not be reversed; run undo again after fixing them",
            rep.failed.len()
        ));
    }
    Ok(())
}
