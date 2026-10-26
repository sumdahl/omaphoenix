//! Clipboard and text helpers.

use std::process::Command;

/// `copy_to_clipboard()`: wl-copy, else OSC 52.
pub(super) fn copy_to_clipboard(text: &str) {
    use std::io::Write;
    let piped = Command::new("wl-copy")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()
        .and_then(|mut c| {
            c.stdin.take()?.write_all(text.as_bytes()).ok()?;
            c.wait().ok()
        })
        .is_some_and(|s| s.success());
    if !piped {
        let b64 = base64(text.as_bytes());
        let _ = write!(std::io::stdout(), "\x1b]52;c;{b64}\x07");
    }
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [
            chunk.first().copied().unwrap_or(0),
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(
                    *T.get(((n >> (18 - 6 * i)) & 63) as usize).unwrap_or(&b'='),
                ));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Removes ANSI colour codes, as the log screen does.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_matches_rfc() {
        assert_eq!(super::base64(b"hi"), "aGk=");
        assert_eq!(super::base64(b"abc"), "YWJj");
    }
}
