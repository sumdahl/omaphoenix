//! A port of Go's `path.Match`; parity with Go matters more than a glob crate's semantics.

/// Whether `name` matches `pattern`; `None` for a malformed pattern (Go's `ErrBadPattern`).
#[must_use]
pub fn path_match(pattern: &str, name: &str) -> Option<bool> {
    let mut pattern = pattern.as_bytes();
    let mut name = name.as_bytes();
    'pattern: while !pattern.is_empty() {
        let (star, chunk, rest) = scan_chunk(pattern);
        pattern = rest;
        if star && chunk.is_empty() {
            // A trailing `*` matches the rest unless it crosses a `/`.
            return Some(!name.contains(&b'/'));
        }
        let (t, ok) = match_chunk(chunk, name)?;
        if ok && (t.is_empty() || !pattern.is_empty()) {
            name = t;
            continue;
        }
        if star {
            // Retry the chunk further along, never skipping past a `/`.
            let mut i = 0;
            while i < name.len() && name.get(i) != Some(&b'/') {
                let (t, ok) = match_chunk(chunk, name.get(i + 1..).unwrap_or_default())?;
                if ok {
                    if pattern.is_empty() && !t.is_empty() {
                        i += 1;
                        continue;
                    }
                    name = t;
                    continue 'pattern;
                }
                i += 1;
            }
        }
        // No match; still report a malformed remainder as malformed, as Go does.
        while !pattern.is_empty() {
            let (_, chunk, rest) = scan_chunk(pattern);
            pattern = rest;
            match_chunk(chunk, b"")?;
        }
        return Some(false);
    }
    Some(name.is_empty())
}

/// Splits off leading `*`s and the literal chunk up to the next unbracketed `*`.
fn scan_chunk(mut pattern: &[u8]) -> (bool, &[u8], &[u8]) {
    let mut star = false;
    while let Some((b'*', rest)) = pattern.split_first() {
        pattern = rest;
        star = true;
    }
    let mut in_range = false;
    let mut i = 0;
    while let Some(&c) = pattern.get(i) {
        match c {
            b'\\' if i + 1 < pattern.len() => i += 1,
            b'[' => in_range = true,
            b']' => in_range = false,
            b'*' if !in_range => break,
            _ => {}
        }
        i += 1;
    }
    let (chunk, rest) = pattern.split_at(i);
    (star, chunk, rest)
}

/// Matches `chunk` against the start of `s`: (rest, matched), `None` when malformed.
fn match_chunk<'a>(mut chunk: &[u8], mut s: &'a [u8]) -> Option<(&'a [u8], bool)> {
    let mut failed = false;
    while let Some(&c) = chunk.first() {
        if !failed && s.is_empty() {
            failed = true;
        }
        match c {
            b'[' => {
                let mut r = '\0';
                if !failed {
                    let (ch, n) = decode(s);
                    r = ch;
                    s = s.get(n..).unwrap_or_default();
                }
                chunk = chunk.get(1..).unwrap_or_default();
                let negated = chunk.first() == Some(&b'^');
                if negated {
                    chunk = chunk.get(1..).unwrap_or_default();
                }
                let mut matched = false;
                let mut ranges = 0;
                loop {
                    if chunk.first() == Some(&b']') && ranges > 0 {
                        chunk = chunk.get(1..).unwrap_or_default();
                        break;
                    }
                    let (lo, rest) = get_esc(chunk)?;
                    chunk = rest;
                    let mut hi = lo;
                    if chunk.first() == Some(&b'-') {
                        let (h, rest) = get_esc(chunk.get(1..).unwrap_or_default())?;
                        hi = h;
                        chunk = rest;
                    }
                    if lo <= r && r <= hi {
                        matched = true;
                    }
                    ranges += 1;
                }
                if matched == negated {
                    failed = true;
                }
            }
            b'?' => {
                if !failed {
                    if s.first() == Some(&b'/') {
                        failed = true;
                    }
                    let (_, n) = decode(s);
                    s = s.get(n..).unwrap_or_default();
                }
                chunk = chunk.get(1..).unwrap_or_default();
            }
            _ => {
                let lit = if c == b'\\' {
                    chunk = chunk.get(1..).unwrap_or_default();
                    *chunk.first()?
                } else {
                    c
                };
                if !failed {
                    if s.first() != Some(&lit) {
                        failed = true;
                    }
                    s = s.get(1..).unwrap_or_default();
                }
                chunk = chunk.get(1..).unwrap_or_default();
            }
        }
    }
    Some(if failed { (&[][..], false) } else { (s, true) })
}

/// One possibly escaped character inside `[…]`; `None` if malformed or unterminated.
fn get_esc(mut chunk: &[u8]) -> Option<(char, &[u8])> {
    match chunk.first() {
        None | Some(b'-' | b']') => return None,
        Some(b'\\') => {
            chunk = chunk.get(1..).unwrap_or_default();
            chunk.first()?;
        }
        Some(_) => {}
    }
    let (r, n) = decode(chunk);
    if r == char::REPLACEMENT_CHARACTER && n == 1 {
        return None;
    }
    let rest = chunk.get(n..).unwrap_or_default();
    if rest.is_empty() {
        return None;
    }
    Some((r, rest))
}

/// The first UTF-8 character like Go's `utf8.DecodeRune`: invalid bytes give U+FFFD, length 1.
fn decode(s: &[u8]) -> (char, usize) {
    for n in 1..=s.len().min(4) {
        if let Some(c) = s
            .get(..n)
            .and_then(|b| std::str::from_utf8(b).ok())
            .and_then(|t| t.chars().next())
        {
            return (c, n);
        }
    }
    (char::REPLACEMENT_CHARACTER, 1)
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Case {
        pattern: String,
        name: String,
        r#match: bool,
        bad_pattern: bool,
    }

    /// Every one of Go's answers, including which patterns are malformed.
    #[test]
    fn agrees_with_go_path_match() {
        let cases: Vec<Case> = serde_json::from_str(include_str!(
            "../../../tests/fixtures/go/selection/path-match.json"
        ))
        .unwrap_or_default();
        assert!(cases.len() > 1000, "fixture loaded");
        let wrong: Vec<String> = cases
            .iter()
            .filter(|c| {
                let got = super::path_match(&c.pattern, &c.name);
                let want = if c.bad_pattern && !c.r#match {
                    None
                } else {
                    Some(c.r#match)
                };
                // Go may report a match and an error together; only the match matters.
                got.unwrap_or(false) != c.r#match || (want.is_none() && got.is_some())
            })
            .map(|c| {
                format!(
                    "{:?} vs {:?}: go={} bad={}",
                    c.pattern, c.name, c.r#match, c.bad_pattern
                )
            })
            .collect();
        assert!(
            wrong.is_empty(),
            "{} mismatches:\n{}",
            wrong.len(),
            wrong.join("\n")
        );
    }
}
