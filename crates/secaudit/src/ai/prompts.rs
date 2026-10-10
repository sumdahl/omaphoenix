use secaudit_core::store::{Item, Run, sev_name, sev_order};

use super::redact::redact;

/// The system prompt.
pub const SYSTEM: &str = "You are a security engineer helping the owner of a personal Linux workstation understand results from secaudit, a read-only audit tool. The machine runs Arch Linux with the Omarchy desktop (Hyprland on Wayland, systemd, pacman and the AUR, ufw firewall, LUKS disk encryption). The owner is a developer who also works in DevSecOps: be technically precise, but explain the underlying concepts plainly, as you would to a capable engineer who doesn't specialise in this particular area.

Ground what you say in the findings you are given. Usernames, the hostname and the home directory are redacted as <user>, <host> and ~. Don't assume facts about the machine that the findings don't show; when the right action depends on something you can't see, say what to check and how.

When you suggest commands:
- Prefer the smallest change that resolves the issue, and say when a command needs sudo.
- If secaudit's suggested fix is right, build on it. If it is risky, incomplete or wrong, say so directly and explain why.
- Don't suggest switching off a security control just to clear a finding unless you label it clearly as a trade-off.
- Include a direct way to confirm the fix worked.

Write Markdown for a terminal: short paragraphs, ## headings, fenced code blocks for commands, no tables.";

const EXPLAIN: &str = "Explain this secaudit finding and how to deal with it.

{finding}

Use these sections:
## What this means
The issue in plain language, including the concept behind it (what the setting, permission, service or file does).
## Why it matters
A realistic way it could be abused on a personal workstation, and how serious it is here compared with a server. If the risk is low, say so plainly.
## How to fix it
Step by step, with commands and what each one does.
## Trade-offs
What could break or change for the owner, if anything.
## Verify
A direct check that the fix worked.

Keep it under about 450 words.";

const PLAN: &str = "Here are the open issues from a secaudit scan of this workstation ({mode} scan, kernel {kernel}, score {score}/100).

<open_issues>
{issues}
</open_issues>
<accepted_risks>
{accepted}
</accepted_risks>

Give the owner an action plan:
## Overall picture
Two or three sentences on the security posture in plain terms.
## Do first
The changes that reduce the most risk for the least effort, in order, with the commands to run.
## Worth doing
Lower-priority hardening, briefly.
## Reasonable to accept
Issues that are often fine on a personal dev workstation, and why, so the owner can decide to accept them.
## Investigate
Anything that could indicate compromise or misconfiguration and needs a closer look, with how to check. Say \"nothing stands out\" if that's the case.

Keep it under about 650 words.";

fn label(sev: &str) -> &'static str {
    match sev {
        "fail" => "FAIL",
        "warn" => "WARN",
        "pass" => "PASS",
        "skip" => "SKIP",
        _ => "NOTE",
    }
}

fn kernel(run: &Run) -> &str {
    if run.meta.kernel.is_empty() {
        "?"
    } else {
        &run.meta.kernel
    }
}

/// One finding and its scan context, redacted.
#[must_use]
pub fn finding_block(f: &Item, run: &Run) -> String {
    let fd = &f.finding;
    let mut parts = vec![
        "<finding>".to_owned(),
        format!("severity: {}", label(sev_name(fd.sev))),
        format!("section: {}", fd.section),
        format!("message: {}", f.title()),
    ];
    if !fd.detail.is_empty() {
        parts.push("details:".into());
        parts.push(fd.detail.clone());
    }
    if !fd.fix.is_empty() {
        parts.push(format!("secaudit suggested fix: {}", fd.fix));
    }
    if !fd.hint.is_empty() {
        parts.push(format!("secaudit advice: {}", fd.hint));
    }
    parts.push("</finding>".into());
    let scope = if run.meta.mode == "quick" {
        "as the user; root-only checks were skipped"
    } else {
        "as root"
    };
    parts.push(format!(
        "<context>scan: {} ({scope}); kernel {}</context>",
        run.meta.mode,
        kernel(run)
    ));
    redact(&parts.join("\n"))
}

/// The explain prompt for one finding.
#[must_use]
pub fn explain_prompt(f: &Item, run: &Run) -> String {
    EXPLAIN.replace("{finding}", &finding_block(f, run))
}

/// The action-plan prompt for a whole scan.
#[must_use]
pub fn plan_prompt(run: &Run) -> String {
    let line = |f: &&Item| {
        let fd = &f.finding;
        let extra = if !fd.fix.is_empty() {
            format!(" | fix: {}", fd.fix)
        } else if !fd.hint.is_empty() {
            format!(" | advice: {}", fd.hint)
        } else {
            String::new()
        };
        let detail = if fd.detail.is_empty() {
            String::new()
        } else {
            format!(
                " | details: {}",
                fd.detail.lines().take(6).collect::<Vec<_>>().join(" / ")
            )
        };
        format!(
            "- [{}] {}: {}{detail}{extra}",
            sev_name(fd.sev).to_uppercase(),
            fd.section,
            f.title()
        )
    };
    let mut issues: Vec<&Item> = run
        .items
        .iter()
        .filter(|f| f.is_issue() && !f.accepted)
        .collect();
    issues.sort_by_key(|f| sev_order(f.finding.sev));
    let accepted: Vec<&Item> = run.items.iter().filter(|f| f.accepted).collect();
    let or_none = |s: String| if s.is_empty() { "none".to_owned() } else { s };
    redact(
        &PLAN
            .replace("{mode}", &run.meta.mode)
            .replace("{kernel}", kernel(run))
            .replace("{score}", &run.counts().score().to_string())
            .replace(
                "{issues}",
                &or_none(issues.iter().map(line).collect::<Vec<_>>().join("\n")),
            )
            .replace(
                "{accepted}",
                &or_none(
                    accepted
                        .iter()
                        .map(|f| format!("- {}: {}", f.finding.section, f.title()))
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
            ),
    )
}
