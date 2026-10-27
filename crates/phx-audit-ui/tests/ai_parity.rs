//! Prompt and cache-key parity with Manish's Python app. Runs only when
//! `OMAPHOENIX_AUDIT_PY_AI_JSON` points at the output of his functions on the same
//! state (see docs/10); otherwise it has nothing to compare against.

use phx_audit::store::{Store, sev_order};
use phx_audit_ui::ai;

#[test]
fn prompts_and_cache_keys_match_python() {
    let Some(path) = std::env::var_os("OMAPHOENIX_AUDIT_PY_AI_JSON") else {
        return;
    };
    let py: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap_or_default())
            .unwrap_or_default();
    let store = Store::from_env();
    let accepted = store.accepted();
    let mut runs = store.runs();
    let run = runs.first_mut().unwrap_or_else(|| unreachable!("a run"));
    run.recount(&accepted);
    let mut rows: Vec<_> = run
        .items
        .iter()
        .filter(|f| f.is_issue() && !f.accepted)
        .collect();
    rows.sort_by_key(|f| (sev_order(f.finding.sev), f.n));
    let f = rows
        .first()
        .copied()
        .unwrap_or_else(|| unreachable!("an issue"));
    let explain = ai::explain_prompt(f, run);
    let plan = ai::plan_prompt(run);
    assert_eq!(
        explain,
        py["explain_prompt"].as_str().unwrap_or_default(),
        "explain prompt"
    );
    assert_eq!(
        plan,
        py["plan_prompt"].as_str().unwrap_or_default(),
        "plan prompt"
    );
    assert_eq!(
        ai::cache_path(&store.dir, &explain).to_string_lossy(),
        py["explain"].as_str().unwrap_or_default()
    );
    assert_eq!(
        ai::cache_path(&store.dir, &plan).to_string_lossy(),
        py["plan"].as_str().unwrap_or_default()
    );
}
