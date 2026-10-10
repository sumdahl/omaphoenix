use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::sync::LazyLock;

use regex::bytes::Regex;

/// Vendor token shapes, not keywords: configs legitimately say "password" and "token".
pub static SECRET_SHAPE: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r"gh[pousr]_[A-Za-z0-9]{36,}|github_pat_[A-Za-z0-9_]{40,}|sk-[A-Za-z0-9]{32,}|sk-ant-[A-Za-z0-9_-]{40,}|AKIA[0-9A-Z]{16}|xox[baprs]-[A-Za-z0-9-]{10,}|BEGIN (RSA |OPENSSH |EC |DSA |PGP )?PRIVATE KEY|AIza[0-9A-Za-z_-]{35}|glpat-[A-Za-z0-9_-]{20,}").ok()
});

// A credential-named key with a 16+ char opaque value; wrangler keeps OAuth tokens in plain TOML.
static SECRET_ASSIGNMENT: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r#"(?i)^[ \t]*(export[ \t]+)?"?[a-z0-9_.-]*(token|secret|passw(or)?d|passphrase|api[_-]?key|access[_-]?key|private[_-]?key|client[_-]?secret|credential)s?"?[ \t]*[:=][ \t]*"?([A-Za-z0-9+/=._~-]{16,})"?[ \t]*,?[ \t]*$"#).ok()
});

// Values that are clearly not secrets even though the key name says so.
static NOT_SECRET_VALUE: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r"(?i)^(true|false|null|none|changeme|your[_-]|example|placeholder|xxx+|\$\{|<)")
        .ok()
});

// Code and docs say `token: someVariable`, so the key-name rule is for config and data only.
const CODE_OR_DOC_EXT: &[&str] = &[
    "qml", "js", "mjs", "cjs", "ts", "tsx", "jsx", "py", "go", "rs", "c", "h", "cpp", "java", "kt",
    "rb", "php", "swift", "cs", "md", "markdown", "rst", "adoc", "txt", "html", "css", "scss",
];

/// Longest line examined by the secret scanner; longer ones are scanned in pieces.
const MAX_SCAN_LINE: u64 = 1 << 20;

/// Whether the file holds something shaped like a secret, streamed line by line.
#[must_use]
pub fn has_secret_shape(path: &Path) -> bool {
    let (Some(shape), Some(assign), Some(not_secret)) = (
        SECRET_SHAPE.as_ref(),
        SECRET_ASSIGNMENT.as_ref(),
        NOT_SECRET_VALUE.as_ref(),
    ) else {
        return false;
    };
    let Ok(file) = File::open(path) else {
        return false;
    };
    let mut reader = BufReader::new(file);
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let mut code_or_doc = CODE_OR_DOC_EXT.contains(&ext.as_str());
    let mut line = Vec::new();
    let mut first = true;
    loop {
        line.clear();
        let Ok(n) = reader
            .by_ref()
            .take(MAX_SCAN_LINE)
            .read_until(b'\n', &mut line)
        else {
            return false;
        };
        if n == 0 {
            return false;
        }
        if first {
            code_or_doc |= line.starts_with(b"#!");
            first = false;
        }
        if shape.is_match(&line) {
            return true;
        }
        if !code_or_doc {
            // Like Go's multiline `$`: only the newline is stripped, so CRLF lines never match.
            let trimmed = line.strip_suffix(b"\n").unwrap_or(&line);
            if let Some(m) = assign.captures(trimmed).and_then(|c| c.get(4))
                && !not_secret.is_match(m.as_bytes())
            {
                return true;
            }
        }
    }
}

/// Whether the file starts with the ELF magic: an executable belongs to its installer.
#[must_use]
pub fn is_binary(path: &Path) -> bool {
    let mut magic = [0u8; 4];
    File::open(path)
        .and_then(|mut f| f.read_exact(&mut magic))
        .is_ok()
        && &magic == b"\x7fELF"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"))
    }

    #[test]
    fn patterns_compile() {
        assert!(
            SECRET_SHAPE.is_some() && SECRET_ASSIGNMENT.is_some() && NOT_SECRET_VALUE.is_some()
        );
    }

    #[test]
    fn secret_shapes_like_go() {
        let t = tmp();
        let check = |name: &str, body: &str| {
            let p = t.path().join(name);
            fs::write(&p, body).ok();
            has_secret_shape(&p)
        };
        assert!(check("a.conf", &format!("x = ghp_{}\n", "a".repeat(36))));
        assert!(check(
            "wrangler.toml",
            "oauth_token = \"abcdefghijklmnop1234\"\n"
        ));
        assert!(check(
            "cfg.json",
            "  \"api_key\": \"ABCDEFGHIJKLMNOPQRST\",\n"
        ));
        assert!(!check(
            "hyprlock.conf",
            "placeholder_text = Enter password\n"
        ));
        assert!(!check("x.conf", "token = changeme_please_now_1234\n"));
        assert!(!check("x.conf", "use_token = true\n"));
        assert!(!check(
            "Plugin.qml",
            "property string token: \"abcdefghijklmnop1234\"\n"
        ));
        assert!(!check("run.sh", "#!/bin/sh\nTOKEN=abcdefghijklmnop1234\n"));
        assert!(
            check("k.md", "-----BEGIN OPENSSH PRIVATE KEY-----\n"),
            "vendor shapes are checked in docs too"
        );
    }
}
