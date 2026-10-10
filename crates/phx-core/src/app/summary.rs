//! What a snapshot changed, for its commit message.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use phx_contract::Manifest;

const LISTED: usize = 20;

/// `snapshot <time> · +1 service · -1 config`, then the items, one per line; `time` stays first.
pub(super) fn commit_message(time: &str, before: Option<&Manifest>, after: &Manifest) -> String {
    let Some(before) = before else {
        return format!("snapshot {time} · first snapshot");
    };
    let (old, new) = (ids(before), ids(after));
    let added: Vec<&String> = new.difference(&old).collect();
    let removed: Vec<&String> = old.difference(&new).collect();
    let mut counts: BTreeMap<(&str, char), usize> = BTreeMap::new();
    for (sign, list) in [('+', &added), ('-', &removed)] {
        for id in list {
            let category = id.split_once(':').map_or(id.as_str(), |(c, _)| c);
            *counts.entry((category, sign)).or_default() += 1;
        }
    }
    let mut subject = format!("snapshot {time}");
    if counts.is_empty() {
        subject.push_str(" · files changed");
    }
    for ((category, sign), n) in &counts {
        let _ = write!(subject, " · {sign}{n} {category}");
    }
    let lines: Vec<String> = added
        .iter()
        .map(|id| format!("+ {id}"))
        .chain(removed.iter().map(|id| format!("- {id}")))
        .collect();
    if lines.is_empty() {
        return subject;
    }
    let mut body: Vec<String> = lines.iter().take(LISTED).cloned().collect();
    if lines.len() > LISTED {
        body.push(format!("… and {} more", lines.len() - LISTED));
    }
    format!("{subject}\n\n{}", body.join("\n"))
}

/// The part of a snapshot subject after its time, e.g. `+1 service · -1 config`.
#[must_use]
pub fn changes(subject: &str) -> &str {
    subject.split_once(" · ").map_or("", |(_, rest)| rest)
}

fn ids(m: &Manifest) -> BTreeSet<String> {
    m.items
        .iter()
        .flat_map(|(c, items)| items.iter().map(move |i| format!("{c}:{}", i.id)))
        .collect()
}

#[cfg(test)]
mod tests {
    use phx_contract::Manifest;

    fn manifest(json: &str) -> Manifest {
        serde_json::from_str(json).unwrap_or_else(|e| unreachable!("{e}"))
    }

    #[test]
    fn names_what_was_added_and_removed() {
        let before = manifest(r#"{"format":1,"items":{"config":[{"id":".config/a"}]}}"#);
        let after = manifest(
            r#"{"format":1,"items":{"config":[{"id":".config/b"}],"service":[{"id":"user/t.timer"}]}}"#,
        );
        let msg = super::commit_message("2026-10-10T15:00:00+05:45", Some(&before), &after);
        let subject = msg.lines().next().unwrap_or_default();
        assert_eq!(
            subject,
            "snapshot 2026-10-10T15:00:00+05:45 · +1 config · -1 config · +1 service"
        );
        assert!(
            msg.contains("+ config:.config/b") && msg.contains("- config:.config/a"),
            "{msg}"
        );
        assert_eq!(
            super::changes(subject),
            "+1 config · -1 config · +1 service"
        );
        assert_eq!(
            super::commit_message("t", None, &after),
            "snapshot t · first snapshot"
        );
        assert_eq!(
            super::commit_message("t", Some(&after), &after),
            "snapshot t · files changed"
        );
    }
}
