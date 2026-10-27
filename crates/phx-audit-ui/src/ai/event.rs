use std::process::Child;

use serde_json::Value;

/// What a request produces, in order.
pub enum Event {
    /// The model started writing.
    Started,
    /// More answer text.
    Text(String),
    /// Finished: status note, optional warning, the turn to keep, the session id.
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
    pub(super) child: Option<Child>,
}

impl Request {
    /// Stops the request.
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

pub(super) fn uuid() -> String {
    std::fs::read_to_string("/proc/sys/kernel/random/uuid")
        .unwrap_or_default()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::thousands;

    #[test]
    fn groups_thousands() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(1234), "1,234");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }
}
