use super::{Choice, OPT_IN};

/// Rules selecting exactly the ticked items; new items in ordinary categories default to captured.
#[must_use]
pub fn build(choices: &[Choice]) -> String {
    let mut out = String::from(
        "# omaphoenix selection rules — one per line, last match wins.
#   category:pattern     include (glob over the item id)
#   !category:pattern    exclude
# Written by the omaphoenix TUI; hand edits are kept only until the next save.
",
    );
    for c in choices {
        let cat = c.category.as_str();
        let line = |out: &mut String, bang: &str, pattern: &str| {
            for part in [bang, cat, ":", pattern, "\n"] {
                out.push_str(part);
            }
        };
        if OPT_IN.contains(&cat) {
            c.items
                .iter()
                .filter(|(_, on)| *on)
                .for_each(|(id, _)| line(&mut out, "", &escape(id)));
        } else if !c.items.is_empty() && c.items.iter().all(|(_, on)| !on) {
            line(&mut out, "!", "*");
        } else {
            line(&mut out, "", "*");
            c.items
                .iter()
                .filter(|(_, on)| !on)
                .for_each(|(id, _)| line(&mut out, "!", &escape(id)));
        }
    }
    out
}

// Quotes glob metacharacters and `#`, so an id matches only itself.
fn escape(id: &str) -> String {
    let mut s = String::with_capacity(id.len());
    for c in id.chars() {
        if matches!(c, '\\' | '*' | '?' | '[' | '#') {
            s.push('\\');
        }
        s.push(c);
    }
    s
}
