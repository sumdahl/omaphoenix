use super::config::home;

fn host() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .unwrap_or_default()
        .trim()
        .to_owned()
}

fn word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Emulates the original's regex lookarounds around `value`.
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

/// Replaces the home path, username and hostname with `~`, `<user>` and `<host>`.
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

#[cfg(test)]
mod tests {
    use super::redact_with;

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
}
