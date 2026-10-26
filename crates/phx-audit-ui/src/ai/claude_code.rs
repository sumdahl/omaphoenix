use std::fmt::Write as _;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::time::Instant;

use serde_json::Value;

use super::config::{claude_bin, effort, model_override};
use super::event::{Event, Request, thousands, uuid};
use super::prompts::SYSTEM;

/// Streams one `claude -p` turn; `session: None` starts a new conversation.
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
            message:
                "Claude Code (the claude command) wasn't found. Set OMAPHOENIX_AUDIT_CLAUDE_BIN."
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
