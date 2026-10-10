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
        include_str!("../../../../tests/fixtures/go/selection/default.conf")
    );
}

#[test]
fn selected_and_machine_agree_with_go() {
    let sets: Vec<RuleSet> = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/go/selection/selected.json"
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
        "../../../../tests/fixtures/go/selection/build.json"
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
