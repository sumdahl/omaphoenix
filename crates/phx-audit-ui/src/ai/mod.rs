//! AI assistant; prompts, redaction and cache stay byte-identical to the Python app.

mod api;
mod cache;
mod claude_code;
mod config;
mod event;
mod prompts;
mod redact;

pub use api::ask_api;
pub use cache::cache_path;
pub use claude_code::ask_claude_code;
pub use config::{
    backend, claude_bin, config_dir, effort, env_credentials, has_stored_key, key_file, model,
    model_override, ready, save_consent, store_key,
};
pub use event::{Event, Request, thousands};
pub use prompts::{SYSTEM, explain_prompt, finding_block, plan_prompt};
pub use redact::redact;
