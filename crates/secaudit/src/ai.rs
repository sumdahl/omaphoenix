//! The AI assistant of Manish's `secaudit-tui`, ported: Claude explains a
//! finding, or plans the whole scan. Opt-in: nothing is sent until the user
//! asks and agrees once. The username, hostname and home path are replaced
//! before a request leaves the machine; the scan never collects secret
//! values. Two backends, as in the original: Claude Code (`claude -p` with
//! every tool, MCP server, slash command and hook off) or the Anthropic API.
//!
//! Prompts, redaction, config, key storage and the answer cache match the
//! Python app byte for byte, so the two apps share settings and saved
//! answers. UI-free: answers stream as [`Event`]s over a channel.

use std::fmt::Write as _;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::Sender;
use std::time::Instant;

use secaudit_core::store::{Item, Run, sev_name, sev_order};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// The system prompt, as the original's `AI_SYSTEM`.
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

/// `SECAUDIT_AI_MODEL`, if set.
#[must_use]
pub fn model_override() -> Option<String> {
    std::env::var("SECAUDIT_AI_MODEL").ok()
}

/// The API model: `SECAUDIT_AI_MODEL`, else `claude-opus-5-5`.
#[must_use]
pub fn model() -> String {
    model_override().unwrap_or_else(|| "claude-opus-5-5".into())
}

/// `SECAUDIT_AI_EFFORT`, else `medium`.
#[must_use]
pub fn effort() -> String {
    std::env::var("SECAUDIT_AI_EFFORT").unwrap_or_else(|_| "medium".into())
}

fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}

fn host() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .unwrap_or_default()
        .trim()
        .to_owned()
}

/// Python's `\w`.
fn word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Replaces `value` where the character before passes `ok_before` and the
/// one after passes `ok_after` (the original's lookarounds).
fn replace_bounded(
    text: &str,
    value: &str,
    label: &str,
    ok_before: impl Fn(Option<char>) -> bool,
    ok_after: impl Fn(Option<char>) -> bool,
) -> String {
    if value.is_empty() {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut prev: Option<char> = None;
    while let Some(i) = rest.find(value) {
        let before = rest
            .get(..i)
            .and_then(|b| b.chars().next_back())
            .or(if i == 0 { prev } else { None });
        let after_str = rest.get(i + value.len()..).unwrap_or_default();
        let after = after_str.chars().next();
        let head = rest.get(..i).unwrap_or_default();
        out.push_str(head);
        if ok_before(before) && ok_after(after) {
            out.push_str(label);
            prev = value.chars().next_back();
        } else {
            // Not a match here: keep its first character, look again after it.
            let first = value.chars().next().unwrap_or_default();
            out.push(first);
            prev = Some(first);
            rest = rest.get(i + first.len_utf8()..).unwrap_or_default();
            continue;
        }
        rest = after_str;
    }
    out.push_str(rest);
    out
}

/// `redact()`: the home path becomes `~`, the username `<user>`, the
/// hostname `<host>`, with the original's word boundaries.
#[must_use]
pub fn redact(text: &str) -> String {
    redact_with(
        text,
        &home(),
        &std::env::var("USER").unwrap_or_default(),
        &host(),
    )
}

fn redact_with(text: &str, home: &str, user: &str, host: &str) -> String {
    // HOME(?=/|$|[^\w.-])
    let mut t = replace_bounded(
        text,
        home,
        "~",
        |_| true,
        |a| a.is_none_or(|c| c == '/' || !(word(c) || c == '.' || c == '-')),
    );
    for (value, label) in [(user, "<user>"), (host, "<host>")] {
        // (?<![\w.-])VALUE(?![\w-])
        t = replace_bounded(
            &t,
            value,
            label,
            |b| b.is_none_or(|c| !(word(c) || c == '.' || c == '-')),
            |a| a.is_none_or(|c| !(word(c) || c == '-')),
        );
    }
    t
}

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

/// `finding_block()`.
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

/// `plan_prompt()`.
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

// ---------- config, key, backend ----------

/// `$XDG_CONFIG_HOME/secaudit` (else `~/.config/secaudit`).
#[must_use]
pub fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map_or_else(|| PathBuf::from(home()).join(".config"), PathBuf::from)
        .join("secaudit")
}

/// Where a pasted API key is kept (mode 600).
#[must_use]
pub fn key_file() -> PathBuf {
    config_dir().join("anthropic-api-key")
}

fn load_config() -> serde_json::Map<String, Value> {
    std::fs::read_to_string(config_dir().join("config.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

/// Records consent and the chosen backend, keeping other settings.
///
/// # Errors
/// If the config can't be written.
pub fn save_consent(backend: &str) -> std::io::Result<()> {
    let mut c = load_config();
    c.insert("ai_consent".into(), Value::Bool(true));
    c.insert("ai_backend".into(), Value::String(backend.into()));
    std::fs::create_dir_all(config_dir())?;
    let text = serde_json::to_string_pretty(&Value::Object(c)).unwrap_or_default();
    std::fs::write(config_dir().join("config.json"), text)
}

fn stored_key() -> Option<String> {
    std::fs::read_to_string(key_file())
        .ok()
        .map(|k| k.trim().to_owned())
        .filter(|k| !k.is_empty())
}

/// Saves a pasted key, readable only by the user.
///
/// # Errors
/// If the file can't be written.
pub fn store_key(key: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(config_dir())?;
    let path = key_file();
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)?;
    writeln!(f, "{}", key.trim())?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
}

/// Credentials found without a pasted key: env vars or an `ant auth login` profile.
#[must_use]
pub fn env_credentials() -> bool {
    std::env::var_os("ANTHROPIC_API_KEY").is_some_and(|v| !v.is_empty())
        || std::env::var_os("ANTHROPIC_AUTH_TOKEN").is_some_and(|v| !v.is_empty())
        || PathBuf::from(home()).join(".config/anthropic").is_dir()
}

/// Whether a pasted key is already saved.
#[must_use]
pub fn has_stored_key() -> bool {
    stored_key().is_some()
}

/// The Claude Code CLI; launchers often run without the shell's PATH, so the
/// usual installs are checked too.
#[must_use]
pub fn claude_bin() -> Option<PathBuf> {
    let home = PathBuf::from(home());
    let on_path = std::env::var_os("PATH")
        .map(|p| {
            std::env::split_paths(&p)
                .map(|d| d.join("claude"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut mise: Vec<PathBuf> = std::fs::read_dir(home.join(".local/share/mise/installs/claude"))
        .map(|rd| rd.flatten().map(|e| e.path().join("claude")).collect())
        .unwrap_or_default();
    mise.sort();
    mise.reverse();
    let candidates = std::env::var_os("SECAUDIT_CLAUDE_BIN")
        .map(PathBuf::from)
        .into_iter()
        .chain(on_path)
        .chain([
            home.join(".local/bin/claude"),
            home.join(".claude/local/claude"),
        ])
        .chain(mise);
    candidates.into_iter().find(|c| {
        std::fs::metadata(c).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    })
}

/// `claude-code` or `api`.
#[must_use]
pub fn backend() -> &'static str {
    if load_config().get("ai_backend").and_then(Value::as_str) == Some("api") {
        return "api";
    }
    if claude_bin().is_some() {
        "claude-code"
    } else {
        "api"
    }
}

/// Consent given, and the backend can run.
#[must_use]
pub fn ready() -> bool {
    load_config().get("ai_consent").and_then(Value::as_bool) == Some(true)
        && (backend() == "claude-code" || env_credentials() || stored_key().is_some())
}

/// The saved answer for `prompt` (shared with the Python app).
#[must_use]
pub fn cache_path(state: &Path, prompt: &str) -> PathBuf {
    let digest = Sha256::digest(format!("{}\0{SYSTEM}\0{prompt}", model()).as_bytes());
    let hex = digest.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    });
    state
        .join("ai-cache")
        .join(format!("{}.md", hex.get(..24).unwrap_or_default()))
}

// ---------- asking ----------

/// What a request produces, in order.
pub enum Event {
    /// The model started writing (status: "Writing…").
    Started,
    /// More answer text.
    Text(String),
    /// Finished: the status note, a warning appended to it, and the
    /// assistant turn to keep for follow-ups (API backend).
    #[allow(missing_docs, reason = "described on the variant")]
    Done {
        note: String,
        warning: Option<String>,
        turn: Option<Value>,
        session: Option<String>,
    },
    /// Failed: the message and whether `ctrl+r` may help.
    #[allow(missing_docs, reason = "described on the variant")]
    Error { message: String, retry: bool },
}

/// A request in flight; `cancel` stops it.
pub struct Request {
    child: Option<Child>,
}

impl Request {
    /// Stops the request (going back mid-answer).
    pub fn cancel(&mut self) {
        if let Some(c) = &mut self.child {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

/// `1,234`.
#[must_use]
pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A random v4 UUID from the kernel.
fn uuid() -> String {
    std::fs::read_to_string("/proc/sys/kernel/random/uuid")
        .unwrap_or_default()
        .trim()
        .to_owned()
}

/// One turn from `claude -p`, streaming. `session` continues a conversation;
/// `None` starts one (its id comes back in [`Event::Done`]).
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "mirrors ask_claude_code() in the original"
)]
pub fn ask_claude_code(
    cwd: &Path,
    prompt: &str,
    session: Option<&str>,
    tx: Sender<Event>,
) -> Request {
    let Some(exe) = claude_bin() else {
        let _ = tx.send(Event::Error {
            message: "Claude Code (the claude command) wasn't found. Set SECAUDIT_CLAUDE_BIN."
                .into(),
            retry: false,
        });
        return Request { child: None };
    };
    let new_session = session.is_none();
    let id = session.map_or_else(uuid, str::to_owned);
    let mut cmd = Command::new(exe);
    cmd.args([
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
    ])
    // Answer only: no tools, MCP servers, slash commands or hooks.
    .args([
        "--tools",
        "",
        "--strict-mcp-config",
        "--disable-slash-commands",
    ])
    .args(["--settings", r#"{"disableAllHooks": true}"#])
    .args(["--system-prompt", SYSTEM, "--effort", &effort()])
    .args([
        if new_session {
            "--session-id"
        } else {
            "--resume"
        },
        &id,
    ]);
    if let Some(m) = model_override() {
        cmd.args(["--model", &m]);
    }
    let _ = std::fs::create_dir_all(cwd);
    let spawned = cmd
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match spawned {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send(Event::Error {
                message: format!("Couldn't start Claude Code: {e}"),
                retry: false,
            });
            return Request { child: None };
        }
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(prompt.as_bytes());
    }
    let (stdout, stderr) = (child.stdout.take(), child.stderr.take());
    std::thread::spawn(move || {
        let started = Instant::now();
        let mut model = String::new();
        let mut result: Option<Value> = None;
        let mut wrote = false;
        for line in stdout
            .into_iter()
            .flat_map(|o| BufReader::new(o).lines().map_while(Result::ok))
        {
            let Ok(ev) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            match ev.get("type").and_then(Value::as_str) {
                Some("stream_event") => {
                    let e = &ev["event"];
                    if e["type"] == "content_block_delta" && e["delta"]["type"] == "text_delta" {
                        if !wrote {
                            wrote = true;
                            let _ = tx.send(Event::Started);
                        }
                        let _ = tx.send(Event::Text(
                            e["delta"]["text"].as_str().unwrap_or_default().to_owned(),
                        ));
                    }
                }
                Some("system") if ev["subtype"] == "init" => {
                    ev["model"]
                        .as_str()
                        .unwrap_or_default()
                        .clone_into(&mut model);
                }
                Some("result") => result = Some(ev),
                _ => {}
            }
        }
        let mut err = String::new();
        if let Some(mut e) = stderr {
            let _ = e.read_to_string(&mut err);
        }
        let failed = result
            .as_ref()
            .is_none_or(|r| r["is_error"].as_bool() == Some(true));
        if failed {
            let last = err.trim().lines().last().unwrap_or_default().to_owned();
            let mut msg = result
                .as_ref()
                .and_then(|r| r["result"].as_str().map(str::to_owned))
                .filter(|m| !m.is_empty())
                .unwrap_or(if last.is_empty() {
                    "exit status 1".into()
                } else {
                    last
                });
            if msg.to_lowercase().contains("login") {
                msg = "Claude Code isn't logged in. Run claude in a terminal and sign in, then try again.".into();
            }
            let _ = tx.send(Event::Error {
                message: msg,
                retry: true,
            });
            return;
        }
        let out = result
            .as_ref()
            .and_then(|r| r["usage"]["output_tokens"].as_u64());
        let mut note = format!("{}s · Claude Code", started.elapsed().as_secs());
        if !model.is_empty() {
            let _ = write!(note, " ({model})");
        }
        if let Some(o) = out.filter(|o| *o > 0) {
            let _ = write!(note, " · {} tokens", thousands(o));
        }
        let _ = tx.send(Event::Done {
            note,
            warning: None,
            turn: None,
            session: Some(id),
        });
    });
    Request { child: Some(child) }
}

/// One turn from the Messages API, streaming, over `curl` (no HTTP stack in
/// the binary; the key travels in a header file, never on the command line).
/// `messages` is the whole conversation so far, ending with the question.
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "one request and its event stream, as the SDK call in the original"
)]
pub fn ask_api(messages: &[Value], tx: Sender<Event>) -> Request {
    let base =
        std::env::var("ANTHROPIC_BASE_URL").unwrap_or_else(|_| "https://api.anthropic.com".into());
    let auth = if let Some(k) = std::env::var("ANTHROPIC_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
    {
        format!("x-api-key: {k}")
    } else if let Some(t) = std::env::var("ANTHROPIC_AUTH_TOKEN")
        .ok()
        .filter(|k| !k.is_empty())
    {
        format!("authorization: Bearer {t}")
    } else if let Some(k) = stored_key() {
        format!("x-api-key: {k}")
    } else {
        let _ = tx.send(Event::Error {
            message: "No API key: set ANTHROPIC_API_KEY or paste one in the setup dialog.".into(),
            retry: false,
        });
        return Request { child: None };
    };
    let body = json!({
        "model": model(),
        "max_tokens": 16000,
        "system": SYSTEM,
        "messages": messages,
        "output_config": {"effort": effort()},
        "fallbacks": "default",
        "stream": true,
    });
    // Headers and body in private temp files: the key stays off the process list.
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join(format!("secaudit-ai-{}", uuid()));
    let setup = std::fs::create_dir_all(&dir)
        .and_then(|()| std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)))
        .and_then(|()| {
            std::fs::write(
                dir.join("headers"),
                format!("{auth}\nanthropic-version: 2023-06-01\nanthropic-beta: server-side-fallback-2026-07-01\ncontent-type: application/json\n"),
            )
        })
        .and_then(|()| std::fs::write(dir.join("body"), body.to_string()));
    if let Err(e) = setup {
        let _ = std::fs::remove_dir_all(&dir);
        let _ = tx.send(Event::Error {
            message: e.to_string(),
            retry: false,
        });
        return Request { child: None };
    }
    let spawned = Command::new("curl")
        .args(["-sS", "-N", "-X", "POST"])
        .arg("-H")
        .arg(format!("@{}", dir.join("headers").display()))
        .arg("--data-binary")
        .arg(format!("@{}", dir.join("body").display()))
        .args(["-w", "\n__STATUS__ %{http_code}\n"])
        .arg(format!("{base}/v1/messages"))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = spawned else {
        {
            let _ = std::fs::remove_dir_all(&dir);
            let _ = tx.send(Event::Error {
                message: "curl is needed to reach the Anthropic API.".into(),
                retry: false,
            });
            return Request { child: None };
        }
    };
    let stdout = child.stdout.take();
    std::thread::spawn(move || {
        let started = Instant::now();
        let mut blocks: Vec<Value> = Vec::new();
        let (mut model_used, mut stop) = (String::new(), String::new());
        let (mut tokens_in, mut tokens_out) = (0u64, 0u64);
        let mut status = 0u16;
        let mut other = String::new();
        let mut wrote = false;
        for line in stdout
            .into_iter()
            .flat_map(|o| BufReader::new(o).lines().map_while(Result::ok))
        {
            // Clean up the key as soon as curl has read it.
            if dir.exists() {
                let _ = std::fs::remove_dir_all(&dir);
            }
            if let Some(code) = line.strip_prefix("__STATUS__ ") {
                status = code.trim().parse().unwrap_or(0);
                continue;
            }
            let Some(data) = line.strip_prefix("data: ") else {
                other.push_str(&line);
                continue;
            };
            let Ok(ev) = serde_json::from_str::<Value>(data) else {
                continue;
            };
            match ev["type"].as_str() {
                Some("message_start") => {
                    ev["message"]["model"]
                        .as_str()
                        .unwrap_or_default()
                        .clone_into(&mut model_used);
                    tokens_in = ev["message"]["usage"]["input_tokens"].as_u64().unwrap_or(0);
                }
                Some("content_block_start") => blocks.push(ev["content_block"].clone()),
                Some("content_block_delta") => {
                    let d = &ev["delta"];
                    let Some(b) = blocks.last_mut() else { continue };
                    match d["type"].as_str() {
                        Some("text_delta") => {
                            let t = d["text"].as_str().unwrap_or_default();
                            if !wrote {
                                wrote = true;
                                let _ = tx.send(Event::Started);
                            }
                            let _ = tx.send(Event::Text(t.to_owned()));
                            b["text"] = Value::String(format!(
                                "{}{t}",
                                b["text"].as_str().unwrap_or_default()
                            ));
                        }
                        Some("thinking_delta") => {
                            let t = d["thinking"].as_str().unwrap_or_default();
                            b["thinking"] = Value::String(format!(
                                "{}{t}",
                                b["thinking"].as_str().unwrap_or_default()
                            ));
                        }
                        Some("signature_delta") => b["signature"] = d["signature"].clone(),
                        _ => {}
                    }
                }
                Some("message_delta") => {
                    ev["delta"]["stop_reason"]
                        .as_str()
                        .unwrap_or_default()
                        .clone_into(&mut stop);
                    tokens_out = ev["usage"]["output_tokens"].as_u64().unwrap_or(tokens_out);
                }
                Some("error") => other = ev.to_string(),
                _ => {}
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
        let error_msg = || {
            serde_json::from_str::<Value>(&other)
                .ok()
                .and_then(|v| v["error"]["message"].as_str().map(str::to_owned))
                .unwrap_or_else(|| other.clone())
        };
        let fail = |message: String, retry: bool| {
            let _ = tx.send(Event::Error { message, retry });
        };
        match status {
            200 => {}
            0 => {
                return fail(
                    "Couldn't reach the Anthropic API. Check your connection.".into(),
                    true,
                );
            }
            401 => {
                return fail(
                    format!(
                        "The API key was rejected. Remove it with: rm {}",
                        key_file().display().to_string().replacen(&home(), "~", 1)
                    ),
                    false,
                );
            }
            403 => {
                return fail(
                    "This API key can't use this model. Set SECAUDIT_AI_MODEL to one it can use."
                        .into(),
                    false,
                );
            }
            404 => {
                return fail(
                    format!("Model {} wasn't found. Check SECAUDIT_AI_MODEL.", model()),
                    false,
                );
            }
            429 => return fail("Rate limited by the API. Wait a moment.".into(), true),
            code => return fail(format!("API error {code}: {}", error_msg()), true),
        }
        if stop == "refusal" {
            return fail(
                "Claude declined to answer this one. Try rephrasing the question.".into(),
                true,
            );
        }
        let note = format!(
            "{}s · {} in / {} out tokens",
            started.elapsed().as_secs(),
            thousands(tokens_in),
            thousands(tokens_out)
        );
        let mut warning = None;
        if stop == "max_tokens" {
            warning = Some("  ·  answer was cut off, ask it to continue".to_owned());
        }
        let note = if !model_used.is_empty() && model_used != model() {
            format!("{note}  ·  answered by {model_used}")
        } else {
            note
        };
        let turn = json!({"role": "assistant", "content": blocks});
        let _ = tx.send(Event::Done {
            note,
            warning,
            turn: Some(turn),
            session: None,
        });
    });
    Request { child: Some(child) }
}

#[cfg(test)]
mod tests {
    use super::{redact_with, thousands};

    #[test]
    fn redacts_with_the_originals_boundaries() {
        let r = |t: &str| redact_with(t, "/home/sam", "sam", "box");
        assert_eq!(r("/home/sam/.ssh/id_ed25519"), "~/.ssh/id_ed25519");
        assert_eq!(r("/home/sam"), "~");
        assert_eq!(
            r("/home/samuel/x"),
            "/home/samuel/x",
            "a longer name is not this user"
        );
        assert_eq!(r("docker group: sam"), "docker group: <user>");
        assert_eq!(r("user sam-dev and sam.x"), "user sam-dev and <user>.x");
        assert_eq!(r("host box, boxes, mybox"), "host <host>, boxes, mybox");
    }

    #[test]
    fn groups_thousands() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(1234), "1,234");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }
}
