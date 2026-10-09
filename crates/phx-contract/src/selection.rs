//! `selection.conf`: which detected items a snapshot captures.
//!
//! One rule per line, `category:pattern`, with a leading `!` to exclude.
//! Patterns are Go `path.Match` globs over the item id; a bare `*` means the
//! whole category, including ids with `/`. The last matching rule wins, so a
//! broad include followed by narrow excludes reads top to bottom. An
//! unescaped `#` starts a comment anywhere on a line; `\#` is a literal `#`.
//!
//! Go wrote ids containing `#` unescaped, so they couldn't be excluded
//! (Go repo `docs/10-known-issues.md`, K1). Rust escapes them. Files Go wrote read the
//! same here, because Go never wrote `\#`.

use std::collections::BTreeMap;

use crate::glob::path_match;

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

/// Categories never wildcarded: a new key or secret must not start travelling
/// in snapshots without the user ticking it.
const OPT_IN: &[&str] = &["secret"];

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

    /// Hardware-specific patterns per category, from `machine:<category>:<glob>`
    /// lines; such items are skipped when restoring onto other hardware.
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

/// Writes rules that select exactly the ticked items. Ordinary categories
/// become `cat:*` plus an exclude per unticked item, so items that appear
/// later are captured by default; opt-in categories list each ticked item.
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

/// Quotes glob metacharacters, and `#` so it isn't read as a comment, so an
/// id matches only itself.
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

#[cfg(test)]
mod tests {
    use super::{Choice, DEFAULT, Rules, build};
    use proptest::prelude::*;
    use serde::Deserialize;
    use std::collections::BTreeMap;

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Case {
        category: String,
        #[serde(rename = "ID")]
        id: String,
        selected: bool,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct RuleSet {
        name: String,
        text: String,
        machine: BTreeMap<String, Vec<String>>,
        cases: Vec<Case>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct GoChoice {
        category: String,
        #[serde(rename = "IDs")]
        ids: Option<Vec<String>>,
        selected: Option<Vec<bool>>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct BuildCase {
        choices: Option<Vec<GoChoice>>,
        output: String,
    }

    #[test]
    fn default_matches_go() {
        assert_eq!(
            DEFAULT,
            include_str!("../../../tests/fixtures/go/selection/default.conf")
        );
    }

    #[test]
    fn selected_and_machine_agree_with_go() {
        let sets: Vec<RuleSet> = serde_json::from_str(include_str!(
            "../../../tests/fixtures/go/selection/selected.json"
        ))
        .unwrap_or_default();
        assert!(sets.len() >= 8, "fixture loaded");
        for set in sets {
            let rules = Rules::parse(&set.text);
            for c in &set.cases {
                assert_eq!(
                    rules.selected(&c.category, &c.id),
                    c.selected,
                    "{}: {}:{:?}",
                    set.name,
                    c.category,
                    c.id
                );
            }
            let machine: BTreeMap<String, Vec<String>> = rules
                .machine()
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v.into_iter().map(str::to_owned).collect()))
                .collect();
            assert_eq!(machine, set.machine, "{}", set.name);
        }
    }

    #[test]
    fn build_matches_go_byte_for_byte() {
        let cases: Vec<BuildCase> = serde_json::from_str(include_str!(
            "../../../tests/fixtures/go/selection/build.json"
        ))
        .unwrap_or_default();
        assert!(cases.len() >= 6, "fixture loaded");
        for case in cases {
            let choices: Vec<Choice> = case
                .choices
                .unwrap_or_default()
                .into_iter()
                .map(|g| Choice {
                    category: g.category,
                    items: g
                        .ids
                        .unwrap_or_default()
                        .into_iter()
                        .zip(g.selected.unwrap_or_default())
                        .collect(),
                })
                .collect();
            assert_eq!(build(&choices), case.output);
        }
    }

    /// Go couldn't exclude ids containing `#`; Rust can.
    #[test]
    fn ids_with_hash_can_be_excluded() {
        let text = build(&[Choice {
            category: "config".into(),
            items: vec![("#emacs#".into(), false), (".zshrc".into(), true)],
        }]);
        assert!(text.contains("!config:\\#emacs\\#\n"), "{text}");
        let rules = Rules::parse(&text);
        assert!(!rules.selected("config", "#emacs#"));
        assert!(rules.selected("config", ".zshrc"));
    }

    /// Go silently dropped a ticked secret with `#` in its name (Go K1).
    #[test]
    fn ticked_secret_with_hash_is_captured() {
        let text = build(&[Choice {
            category: "secret".into(),
            items: vec![(".ssh/#work#_key".into(), true), (".ssh/id".into(), false)],
        }]);
        let rules = Rules::parse(&text);
        assert!(rules.selected("secret", ".ssh/#work#_key"));
        assert!(!rules.selected("secret", ".ssh/id"));
    }

    #[test]
    fn comments_still_strip() {
        let rules = Rules::parse("config:*  # all\n!config:a\\\\# backslash, then a comment\n");
        assert!(!rules.selected("config", "a\\"));
        assert!(rules.selected("config", "b"));
    }

    /// Ids the format can represent: no line breaks or edge whitespace.
    fn id() -> impl Strategy<Value = String> {
        "[^\\r\\n]{1,24}".prop_filter("no edge whitespace", |s| s.trim() == s)
    }

    proptest! {
        /// Build∘Parse: whatever the user ticks is exactly what gets captured.
        #[test]
        fn build_then_parse_selects_exactly_the_ticked_items(
            cat in prop::sample::select(vec!["config", "plugin", "secret", "bin"]),
            items in prop::collection::btree_map(id(), any::<bool>(), 0..8),
        ) {
            let items: Vec<(String, bool)> = items.into_iter().collect();
            let rules = Rules::parse(&build(&[Choice { category: cat.into(), items: items.clone() }]));
            for (id, on) in &items {
                prop_assert_eq!(rules.selected(cat, id), *on, "id {:?}", id);
            }
        }
    }
}
