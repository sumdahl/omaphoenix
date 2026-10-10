use super::{CONTRACT_SCHEMA, ContractError, Finding, MAX_LINE, Severity, Target};

const LEGACY: &str = include_str!("../../../../tests/fixtures/secaudit/legacy.jsonl");
const V1: &str = include_str!("../../../../tests/fixtures/secaudit/v1.jsonl");

fn parse(line: &str) -> Finding {
    match Finding::parse_line(line) {
        Ok(f) => f,
        Err(e) => unreachable!("fixture line must parse: {e}: {line}"),
    }
}

/// Every fixture parses, and re-serialising gives the same JSON value.
#[test]
fn fixtures_parse_and_round_trip() {
    for line in LEGACY.lines().chain(V1.lines()) {
        let f = parse(line);
        let again = serde_json::to_value(&f).ok();
        let orig: Option<serde_json::Value> = serde_json::from_str(line).ok();
        let mut orig = orig.unwrap_or_default();
        // Legacy lines gain `schema: 0` when written back; nothing else changes.
        if f.schema == 0 {
            orig["schema"] = 0.into();
        }
        // An empty `targets` is omitted on write.
        if orig
            .get("targets")
            .is_some_and(|t| t == &serde_json::json!([]))
        {
            orig.as_object_mut().map(|o| o.remove("targets"));
        }
        assert_eq!(again, Some(orig), "{line}");
    }
}

#[test]
fn legacy_lines_are_schema_zero_without_id() {
    let f = parse(LEGACY.lines().next().unwrap_or_default());
    assert_eq!((f.schema, f.id, f.sev), (0, None, Severity::Warn));
    assert!(f.targets.is_empty());
}

#[test]
fn v1_targets_keep_their_kind() {
    let kinds: Vec<Vec<Target>> = V1.lines().map(|l| parse(l).targets).collect();
    assert_eq!(
        kinds[1],
        vec![
            Target::Unit {
                name: "user/sync.service".into()
            },
            Target::File {
                path: "/home/u/.config/systemd/user/sync.service".into()
            },
        ]
    );
    assert_eq!(kinds[3], vec![Target::None]);
}

#[test]
fn refuses_a_newer_schema() {
    let line = format!(
        r#"{{"schema":{},"id":"a.b","sev":"pass","section":"s","msg":"m"}}"#,
        CONTRACT_SCHEMA + 1
    );
    assert!(matches!(
        Finding::parse_line(&line),
        Err(ContractError::NewerSchema(2))
    ));
}

#[test]
fn v1_requires_a_valid_id() {
    for id in [
        None,
        Some(""),
        Some("nodot"),
        Some("Secrets.x"),
        Some("a..b"),
        Some("a.b-c"),
    ] {
        let id_field = id.map_or(String::new(), |i| format!(r#""id":"{i}","#));
        let line = format!(r#"{{"schema":1,{id_field}"sev":"pass","section":"s","msg":"m"}}"#);
        assert!(
            matches!(Finding::parse_line(&line), Err(ContractError::BadId(_))),
            "{line}"
        );
    }
    for id in ["a.b", "secrets.env_readable", "net.v6.ra_2"] {
        let line = format!(r#"{{"schema":1,"id":"{id}","sev":"pass","section":"s","msg":"m"}}"#);
        assert!(Finding::parse_line(&line).is_ok(), "{line}");
    }
}

#[test]
fn rejects_unknown_severity_and_target_kind() {
    assert!(Finding::parse_line(r#"{"sev":"bad","section":"s","msg":"m"}"#).is_err());
    let line = r#"{"schema":1,"id":"a.b","sev":"pass","section":"s","msg":"m","targets":[{"kind":"disk"}]}"#;
    assert!(Finding::parse_line(line).is_err());
}

#[test]
fn bounds_line_length() {
    let line = format!(
        r#"{{"sev":"pass","section":"s","msg":"{}"}}"#,
        "x".repeat(MAX_LINE)
    );
    assert!(matches!(
        Finding::parse_line(&line),
        Err(ContractError::TooLong(_))
    ));
}
