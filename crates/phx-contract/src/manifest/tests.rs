use super::{Manifest, ManifestError};

const FIXTURES: [(&str, &str); 3] = [
    (
        "full",
        include_str!("../../../../tests/fixtures/go/manifest/full.json"),
    ),
    (
        "minimal",
        include_str!("../../../../tests/fixtures/go/manifest/minimal.json"),
    ),
    (
        "nil-items",
        include_str!("../../../../tests/fixtures/go/manifest/nil-items.json"),
    ),
];

// Writing Go's manifests back gives the same JSON, except a `null` item map comes back as `{}`.
#[test]
fn go_manifests_round_trip() {
    for (name, json) in FIXTURES {
        let m = Manifest::parse(json).unwrap_or_else(|e| unreachable!("{name}: {e}"));
        let mut want: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
        if want["items"].is_null() {
            want["items"] = serde_json::json!({});
        }
        // An empty list is written as null, as Go writes a nil one.
        if let Some(items) = want["items"].as_object_mut() {
            for v in items.values_mut() {
                if v.as_array().is_some_and(Vec::is_empty) {
                    *v = serde_json::Value::Null;
                }
            }
        }
        assert_eq!(serde_json::to_value(&m).ok(), Some(want), "{name}");
    }
}

#[test]
fn reads_go_fields() {
    let m = Manifest::parse(FIXTURES[0].1).unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(m.hardware, "Dell Inc. Latitude 7430");
    assert_eq!(
        m.items["plugin"][0].note,
        "https://github.com/sumdahl/media@3f2a1c9"
    );
    assert_eq!(
        m.items["config"][2].id, "a<b>&c \"q\" ünï",
        "Go's \\u003c escapes decode"
    );
    assert!(m.items["etc"].is_empty());
}

#[test]
fn refuses_newer_format_and_tolerates_new_fields() {
    assert!(matches!(
        Manifest::parse(r#"{"format":2}"#),
        Err(ManifestError::NewerFormat(2))
    ));
    assert!(Manifest::parse(r#"{"format":1,"audit":null,"future":[1]}"#).is_ok());
    assert!(
        Manifest::parse(r#"{"items":{}}"#).is_err(),
        "format is required"
    );
}
