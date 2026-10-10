use std::path::PathBuf;

use secaudit_core::store::{Item, sev_name, sev_order};
use slint::ComponentHandle;

use super::{State, int, local, model, state};
use crate::ui::{App, Past, Row};

/// Tabs, as `in_tab()` in the terminal app.
pub(crate) fn in_tab(tab: i32, f: &Item) -> bool {
    let sev = sev_name(f.finding.sev);
    match tab {
        0 => f.is_issue() && !f.accepted,
        1 => f.accepted,
        2 => matches!(sev, "info" | "skip"),
        3 => sev == "pass",
        _ => true,
    }
}

pub(crate) fn refresh(app: &App) {
    let shared = state();
    let mut s = shared.borrow_mut();
    let Some(run) = s.run() else {
        app.set_has_run(false);
        app.set_history(model(Vec::new()));
        return;
    };
    let c = run.counts();
    app.set_has_run(true);
    let latest = s.current == 0;
    app.set_when(
        format!(
            "{} · {} scan{}",
            local(run.meta.started, "%a %-d %b %H:%M"),
            run.meta.mode,
            if latest { "" } else { " (older scan)" }
        )
        .into(),
    );
    let score = c.score();
    app.set_score(i32::try_from(score).unwrap_or(0));
    app.set_grade(c.grade().to_string().into());
    #[allow(clippy::cast_precision_loss, reason = "a score of 0-100")]
    app.set_ring(score as f32);
    app.set_fails(int(c.fail));
    app.set_warns(int(c.warn));
    app.set_passes(int(c.pass));
    app.set_notes(int(c.info + c.skip));
    app.set_accepted(int(c.accepted));
    app.set_tabs(model(vec![
        format!("Issues {}", c.fail + c.warn).into(),
        format!("Accepted {}", c.accepted).into(),
        format!("Notes {}", c.info + c.skip).into(),
        format!("Passed {}", c.pass).into(),
        format!("All {}", run.items.len()).into(),
    ]));
    let since = match s.runs.get(s.current + 1) {
        Some(prev) => {
            let (a, b) = (run.issue_keys(), prev.issue_keys());
            format!(
                "Since the previous scan: {} new · {} resolved",
                a.difference(&b).count(),
                b.difference(&a).count()
            )
        }
        None => "First scan on record".into(),
    };
    app.set_since(since.into());

    let q = app.get_query().to_lowercase();
    let tab = app.get_tab();
    let applied = run.applied();
    let mut items: Vec<&Item> = run
        .items
        .iter()
        .filter(|f| in_tab(tab, f))
        .filter(|f| {
            let fd = &f.finding;
            q.is_empty()
                || format!(
                    "{}\n{}\n{}\n{}\n{}",
                    fd.msg, fd.section, fd.detail, fd.fix, fd.hint
                )
                .to_lowercase()
                .contains(&q)
        })
        .collect();
    items.sort_by_key(|f| (sev_order(f.finding.sev), f.n));
    let rows: Vec<Row> = items
        .iter()
        .map(|f| Row {
            sev: sev_name(f.finding.sev).into(),
            title: f.title().into(),
            section: f.finding.section.as_str().into(),
            detail: f.finding.detail.as_str().into(),
            fix: f.finding.fix.as_str().into(),
            hint: f.finding.hint.as_str().into(),
            accepted: f.accepted,
            applied: applied.contains(&f.n),
        })
        .collect();
    s.shown = items.iter().map(|f| f.n).collect();
    let n = rows.len();
    app.set_rows(model(rows));
    app.set_sel(app.get_sel().min(int(n.saturating_sub(1))).max(0));

    set_history(app, &s);
}

pub(crate) fn set_history(app: &App, state: &State) {
    let history: Vec<Past> = state
        .runs
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let c = r.counts();
            let change = match state.runs.get(i + 1) {
                Some(p) => {
                    let (a, b) = (r.issue_keys(), p.issue_keys());
                    format!(
                        "+{} new  −{} resolved",
                        a.difference(&b).count(),
                        b.difference(&a).count()
                    )
                }
                None => "first scan".into(),
            };
            Past {
                label: local(r.meta.started, "%a %-d %b %H:%M").into(),
                score: i32::try_from(c.score()).unwrap_or(0),
                grade: c.grade().to_string().into(),
                mode: r.meta.mode.as_str().into(),
                change: change.into(),
            }
        })
        .collect();
    app.set_history(model(history));
}

pub(crate) fn selected(app: &App) -> Option<(PathBuf, Item)> {
    let st = state();
    let s = st.borrow();
    let n = *s.shown.get(usize::try_from(app.get_sel()).ok()?)?;
    let run = s.run()?;
    Some((
        run.path.clone(),
        run.items.iter().find(|i| i.n == n)?.clone(),
    ))
}

pub(crate) fn wire(app: &App) {
    let weak = app.as_weak();
    app.on_set_tab(move |t| {
        if let Some(app) = weak.upgrade() {
            app.set_tab(t);
            app.set_sel(0);
            refresh(&app);
        }
    });
    let weak = app.as_weak();
    app.on_search(move |q| {
        if let Some(app) = weak.upgrade() {
            app.set_query(q);
            app.set_sel(0);
            refresh(&app);
        }
    });
    let weak = app.as_weak();
    app.on_open_run(move |i| {
        if let Some(app) = weak.upgrade() {
            state().borrow_mut().current = usize::try_from(i).unwrap_or(0);
            app.set_sel(0);
            refresh(&app);
        }
    });
}
