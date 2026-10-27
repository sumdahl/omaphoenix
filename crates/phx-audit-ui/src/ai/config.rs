use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;

use serde_json::Value;

/// `OMAPHOENIX_AUDIT_AI_MODEL`, if set.
#[must_use]
pub fn model_override() -> Option<String> {
    std::env::var("OMAPHOENIX_AUDIT_AI_MODEL").ok()
}

/// The API model: `OMAPHOENIX_AUDIT_AI_MODEL`, else `claude-opus-5-5`.
#[must_use]
pub fn model() -> String {
    model_override().unwrap_or_else(|| "claude-opus-5-5".into())
}

/// `OMAPHOENIX_AUDIT_AI_EFFORT`, else `medium`.
#[must_use]
pub fn effort() -> String {
    std::env::var("OMAPHOENIX_AUDIT_AI_EFFORT").unwrap_or_else(|_| "medium".into())
}

pub(super) fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}

/// `$XDG_CONFIG_HOME/omaphoenix-audit` (else `~/.config/omaphoenix-audit`).
#[must_use]
pub fn config_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map_or_else(|| PathBuf::from(home()).join(".config"), PathBuf::from);
    phx_audit::store::renamed_dir(&base)
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

pub(super) fn stored_key() -> Option<String> {
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

/// Env-var or `ant auth login` credentials exist.
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

/// The Claude Code CLI; also checks usual installs since launchers may lack PATH.
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
    let candidates = std::env::var_os("OMAPHOENIX_AUDIT_CLAUDE_BIN")
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
