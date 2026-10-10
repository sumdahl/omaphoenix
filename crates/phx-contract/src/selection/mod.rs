//! `selection.conf`: `[!]category:glob` rules, last match wins; an unescaped `#` starts a comment.

mod build;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use crate::glob::path_match;

pub use build::build;

/// The file `omaphoenix init` writes.
pub const DEFAULT: &str = "# omaphoenix selection rules — one per line, last match wins.
#   category:pattern     include (glob over the item id)
#   !category:pattern    exclude
pkg-repo:*
pkg-aur:*
pkg-other:*
plugin:*
theme:*
config:*
local:*
bin:*
etc:*
service:*
# secret:.ssh/*          added by 'omaphoenix secrets init' (encrypted)
";

// Never wildcarded: a new key must not start travelling in snapshots unticked.
pub(super) const OPT_IN: &[&str] = &["secret"];

#[derive(Clone, Debug, PartialEq, Eq)]
struct Rule {
    exclude: bool,
    category: Box<str>,
    pattern: Box<str>,
}

/// Parsed selection rules.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rules {
    rules: Vec<Rule>,
}

impl Rules {
    /// Parses rules. Lines without a `:` are ignored, as in Go.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let rules = text
            .lines()
            .filter_map(|line| {
                let line = strip_comment(line).trim();
                let (exclude, line) = match line.strip_prefix('!') {
                    Some(rest) => (true, rest),
                    None => (false, line),
                };
                let (category, pattern) = line.split_once(':')?;
                Some(Rule {
                    exclude,
                    category: category.into(),
                    pattern: pattern.into(),
                })
            })
            .collect();
        Self { rules }
    }

    /// Whether item `id` in `category` is captured.
    #[must_use]
    pub fn selected(&self, category: &str, id: &str) -> bool {
        self.rules
            .iter()
            .rev()
            .find(|r| {
                &*r.category == category
                    && (&*r.pattern == "*"
                        || &*r.pattern == id
                        || path_match(&r.pattern, id) == Some(true))
            })
            .is_some_and(|r| !r.exclude)
    }

    /// Hardware-specific patterns per category, from `machine:<category>:<glob>` lines.
    #[must_use]
    pub fn machine(&self) -> BTreeMap<&str, Vec<&str>> {
        let mut out: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for r in self
            .rules
            .iter()
            .filter(|r| &*r.category == "machine" && !r.exclude)
        {
            if let Some((cat, pat)) = r.pattern.split_once(':') {
                out.entry(cat).or_default().push(pat);
            }
        }
        out
    }
}

/// One category's items and whether each is ticked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    /// Category name.
    pub category: String,
    /// Item ids, each paired with whether it is captured.
    pub items: Vec<(String, bool)>,
}

/// The line up to its first unescaped `#`.
fn strip_comment(line: &str) -> &str {
    let mut escaped = false;
    for (i, b) in line.bytes().enumerate() {
        match b {
            _ if escaped => escaped = false,
            b'\\' => escaped = true,
            b'#' => return line.get(..i).unwrap_or_default(),
            _ => {}
        }
    }
    line
}
