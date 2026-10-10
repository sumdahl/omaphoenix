use std::fmt::Write as _;

use phx_core::store::{Repo, Snapshot};
use phx_core::{App, RestoreOptions, StageReport};

use crate::out::say;

#[allow(
    clippy::too_many_arguments,
    clippy::fn_params_excessive_bools,
    reason = "the command's flags, passed straight through"
)]
pub fn run(
    app: &App<'_>,
    items: Vec<String>,
    at: String,
    dry_run: bool,
    only: Vec<String>,
    all_hardware: bool,
    no_restart: bool,
    from: &str,
) -> Result<(), String> {
    if !app.repo.exists() {
        if from.is_empty() {
            return Err("no local snapshot repo; pass --from <git url>".into());
        }
        Repo::clone_from(from, &app.repo.dir).map_err(|e| e.to_string())?;
    }
    let opt = RestoreOptions {
        at,
        dry_run,
        no_restart,
        all_hardware,
        only,
        items,
    };
    let (snap, reports) = app.restore(&opt).map_err(|e| e.to_string())?;
    print_report(&snap, &reports, dry_run)
}

fn print_report(snap: &Snapshot, reports: &[StageReport], dry: bool) -> Result<(), String> {
    say(&format!(
        "\n{} of snapshot {} ({})",
        if dry { "dry run" } else { "restore" },
        snap.id,
        snap.time
    ));
    let (mut failed, mut deferred) = (0, 0);
    for r in reports {
        let mark = if !r.ok() {
            failed += 1;
            "✗"
        } else if r.result.deferred.is_empty() {
            "✓"
        } else {
            "⏸"
        };
        deferred += r.result.deferred.len();
        let mut line = format!(
            "{mark} {:<9} applied {} · already present {}",
            r.category, r.result.applied, r.result.present
        );
        if !r.result.planned.is_empty() {
            let _ = write!(line, " · planned {}", r.result.planned.len());
        }
        say(&line);
        for (i, p) in r.result.planned.iter().enumerate() {
            if i == 15 {
                say(&format!("    … {} more", r.result.planned.len() - 15));
                break;
            }
            say(&format!("    + {p}"));
        }
        r.result
            .failed
            .iter()
            .for_each(|f| say(&format!("    ✗ {f}")));
        r.result
            .deferred
            .iter()
            .for_each(|d| say(&format!("    ⏸ {d}")));
        r.skipped
            .iter()
            .for_each(|s| say(&format!("    · skipped, other hardware: {s}")));
        if let Some(e) = &r.err {
            say(&format!("    ✗ {e}"));
        }
    }
    if deferred > 0 {
        say(&format!(
            "\n⏸ {deferred} item(s) were skipped for now because their download was too slow.\n  Everything else is restored. Run `omaphoenix restore` again later to finish them;\n  it continues where this run stopped."
        ));
    }
    if failed > 0 {
        return Err(format!("{failed} stage(s) failed"));
    }
    Ok(())
}
