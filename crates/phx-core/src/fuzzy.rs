//! Fuzzy search shared by every list in the apps: subsequence matching scored like fzf.
//!
//! A query is space-separated terms that must all match. A term matches when its characters
//! appear in order; matches score higher when they are consecutive, start a word (after `/ . _ -`
//! or a space, or at a lower→upper case change), or start the text. A term with an uppercase
//! letter is case-sensitive ("smart case").

/// A match: its score (higher is better) and the matched character positions in the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// Relevance; only meaningful compared with other matches of the same query.
    pub score: i64,
    /// Indices (in `chars()`) of the matched characters, ascending.
    pub positions: Vec<usize>,
}

const MATCH: i64 = 16;
const CONSECUTIVE: i64 = 24;
const BOUNDARY: i64 = 32;
const FIRST_CHAR: i64 = 16;
const GAP: i64 = 3;
const LEADING_GAP: i64 = 1;
const END: i64 = 16;

/// Scores `text` against `query`; `None` when some term doesn't match. An empty query matches everything with score 0.
#[must_use]
pub fn score(query: &str, text: &str) -> Option<Match> {
    let chars: Vec<char> = text.chars().collect();
    let mut total = 0;
    let mut positions = Vec::new();
    for (n, term) in query.split_whitespace().enumerate() {
        let m = score_term(term, &chars, n == 0)?;
        total += m.score;
        positions.extend(m.positions);
    }
    positions.sort_unstable();
    positions.dedup();
    // Shorter texts win ties: "nord" over "nord-backup-2019".
    let len_penalty = i64::try_from(chars.len()).unwrap_or(i64::MAX) / 4;
    Some(Match {
        score: total
            - if query.trim().is_empty() {
                0
            } else {
                len_penalty
            },
        positions,
    })
}

/// Indices of `items` matching `query`, best first; ties keep the original order. Empty query: all, in order.
pub fn rank<T>(query: &str, items: &[T], text: impl Fn(&T) -> String) -> Vec<(usize, Match)> {
    let mut hits: Vec<(usize, Match)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, it)| score(query, &text(it)).map(|m| (i, m)))
        .collect();
    hits.sort_by(|a, b| b.1.score.cmp(&a.1.score).then(a.0.cmp(&b.0)));
    hits
}

/// Indices of `items` matching `query`, in their original order (for lists whose order means something).
pub fn filter<T>(query: &str, items: &[T], text: impl Fn(&T) -> String) -> Vec<usize> {
    items
        .iter()
        .enumerate()
        .filter(|(_, it)| score(query, &text(it)).is_some())
        .map(|(i, _)| i)
        .collect()
}

fn fold(c: char, case_sensitive: bool) -> char {
    if case_sensitive {
        c
    } else {
        c.to_lowercase().next().unwrap_or(c)
    }
}

fn is_boundary(chars: &[char], i: usize) -> bool {
    let Some(prev) = i.checked_sub(1).and_then(|p| chars.get(p)) else {
        return true;
    };
    let cur = chars.get(i).copied().unwrap_or(' ');
    matches!(prev, '/' | '.' | '_' | '-' | ' ' | ':' | '@')
        || (prev.is_lowercase() && cur.is_uppercase())
        || (!prev.is_alphanumeric() && cur.is_alphanumeric())
}

/// Best alignment of one term: try every start, take the tightest window from it, score it.
fn score_term(term: &str, chars: &[char], first_term: bool) -> Option<Match> {
    let case_sensitive = term.chars().any(char::is_uppercase);
    let q: Vec<char> = term.chars().map(|c| fold(c, case_sensitive)).collect();
    let t: Vec<char> = chars.iter().map(|&c| fold(c, case_sensitive)).collect();
    let first = *q.first()?;
    let mut best: Option<Match> = None;
    for start in (0..t.len()).filter(|&i| t.get(i) == Some(&first)) {
        let Some(end) = forward_end(&q, &t, start) else {
            break; // later starts can't match either
        };
        let tight = backward_start(&q, &t, end);
        let Some(positions) = forward_positions(&q, &t, tight) else {
            continue;
        };
        let s = alignment_score(chars, &positions, first_term);
        if best.as_ref().is_none_or(|b| s > b.score) {
            best = Some(Match {
                score: s,
                positions,
            });
        }
    }
    best
}

fn forward_end(q: &[char], t: &[char], start: usize) -> Option<usize> {
    let mut qi = 0;
    for (i, c) in t.iter().enumerate().skip(start) {
        if q.get(qi) == Some(c) {
            qi += 1;
            if qi == q.len() {
                return Some(i);
            }
        }
    }
    None
}

fn backward_start(q: &[char], t: &[char], end: usize) -> usize {
    let mut qi = q.len();
    for i in (0..=end).rev() {
        if qi > 0 && q.get(qi - 1) == t.get(i) {
            qi -= 1;
            if qi == 0 {
                return i;
            }
        }
    }
    0
}

fn forward_positions(q: &[char], t: &[char], start: usize) -> Option<Vec<usize>> {
    let mut out = Vec::with_capacity(q.len());
    let mut qi = 0;
    for (i, c) in t.iter().enumerate().skip(start) {
        if qi < q.len() && q.get(qi) == Some(c) {
            out.push(i);
            qi += 1;
        }
    }
    (qi == q.len()).then_some(out)
}

fn alignment_score(chars: &[char], positions: &[usize], first_term: bool) -> i64 {
    let as_i64 = |n: usize| i64::try_from(n).unwrap_or(i64::MAX);
    let mut s = 0;
    for (k, &p) in positions.iter().enumerate() {
        s += MATCH;
        if is_boundary(chars, p) {
            s += BOUNDARY;
        }
        match k.checked_sub(1).and_then(|j| positions.get(j)) {
            Some(&prev) if prev + 1 == p => s += CONSECUTIVE,
            Some(&prev) => s -= GAP * as_i64(p - prev - 1).min(10),
            None => {
                if p == 0 {
                    s += FIRST_CHAR;
                }
                if first_term {
                    s -= LEADING_GAP * as_i64(p).min(20);
                }
            }
        }
    }
    // Matching up to the end names the item itself ("…/fpl-engine"), not a folder on its way.
    if positions.last().is_some_and(|&p| p + 1 == chars.len()) {
        s += END;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::{filter, rank, score};

    fn names(q: &str, items: &[&str]) -> Vec<String> {
        rank(q, items, |s| (*s).to_owned())
            .into_iter()
            .filter_map(|(i, _)| items.get(i).map(|s| (*s).to_owned()))
            .collect()
    }

    #[test]
    fn matches_letters_in_order_any_case() {
        assert!(score("cjt", ".claude.json.tmp.159014").is_some());
        assert!(
            score("CLAUDE", ".claude.json").is_none(),
            "uppercase makes it case-sensitive"
        );
        assert!(score("Claude", "Claude Code").is_some());
        assert!(score("tjc", ".claude.json.tmp").is_none(), "order matters");
        assert!(score("xyz", "hypr").is_none());
    }

    #[test]
    fn every_term_must_match() {
        assert!(score("claude tmp", ".claude.json.tmp.1").is_some());
        assert!(score("claude zzz", ".claude.json.tmp.1").is_none());
        assert_eq!(score("", "anything").map(|m| m.score), Some(0));
        assert_eq!(score("   ", "anything").map(|m| m.score), Some(0));
    }

    #[test]
    fn ranks_word_starts_and_runs_first() {
        let items = [
            ".config/hypr/hyprland.conf",
            ".config/hyprland-mimeapps.list",
            ".local/share/hpr",
            ".config/hypr",
        ];
        assert_eq!(
            names("hypr", &items).first().map(String::as_str),
            Some(".config/hypr")
        );
        let items = [
            "config:.config/kitty/kitty.conf",
            "config:.claude.json.tmp.159014",
            "config:.config/code-flags.conf",
        ];
        assert_eq!(
            names("claude tmp", &items),
            ["config:.claude.json.tmp.159014"]
        );
        assert_eq!(
            names("cjt", &items).first().map(String::as_str),
            Some("config:.claude.json.tmp.159014")
        );
        let items = [
            "service:user/fpl-engine-gw7-audit.timer",
            "bin:.local/bin/fpl-engine",
            "theme:nord",
        ];
        assert_eq!(
            names("fpl eng", &items).first().map(String::as_str),
            Some("bin:.local/bin/fpl-engine")
        );
    }

    #[test]
    fn positions_point_at_the_matched_letters() {
        let m = score("hyp", ".config/hypr").unwrap_or_else(|| unreachable!());
        assert_eq!(m.positions, [8, 9, 10]);
        let m = score("né", "café né").unwrap_or_else(|| unreachable!());
        assert_eq!(m.positions, [5, 6], "unicode by character, not byte");
    }

    #[test]
    fn filter_keeps_the_list_order() {
        let snaps = [
            "884ae58 -1 config",
            "c528752 +1 config",
            "8a67d3e first snapshot",
        ];
        assert_eq!(filter("config", &snaps, |s| (*s).to_owned()), [0, 1]);
        assert_eq!(filter("", &snaps, |s| (*s).to_owned()), [0, 1, 2]);
    }

    #[test]
    fn stays_fast_on_a_thousand_long_items() {
        let items: Vec<String> = (0..1000)
            .map(|i| format!("config:.config/app-{i}/very/long/path/to/settings/file-{i}.conf"))
            .collect();
        let t = std::time::Instant::now();
        for q in ["conf", "app 99", "vlpts", "zzz"] {
            let _ = rank(q, &items, Clone::clone);
        }
        assert!(
            t.elapsed() < std::time::Duration::from_secs(2),
            "{:?}",
            t.elapsed()
        );
    }
}
