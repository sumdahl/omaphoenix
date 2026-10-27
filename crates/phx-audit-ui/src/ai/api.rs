use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::time::Instant;

use serde_json::{Value, json};

use super::config::{effort, home, key_file, model, stored_key};
use super::event::{Event, Request, thousands, uuid};
use super::prompts::SYSTEM;

/// Streams one Messages API turn over `curl`; the key goes in a header file, never argv.
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
        .join(format!("omaphoenix-audit-ai-{}", uuid()));
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
                    "This API key can't use this model. Set OMAPHOENIX_AUDIT_AI_MODEL to one it can use."
                        .into(),
                    false,
                );
            }
            404 => {
                return fail(
                    format!(
                        "Model {} wasn't found. Check OMAPHOENIX_AUDIT_AI_MODEL.",
                        model()
                    ),
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
