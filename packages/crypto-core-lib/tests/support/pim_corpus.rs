use crypto_core_lib::pim::{PimResourceKind, PimValue, projection_fields, projection_property};
use serde::Deserialize;
use std::{collections::BTreeMap, path::Path};

#[derive(Deserialize)]
pub struct Fixture {
    pub file: String,
    pub kind: PimResourceKind,
    pub uid: String,
    pub fields: BTreeMap<String, PimValue>,
}

pub fn corpus() -> Vec<(Fixture, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/pim");
    let fixtures: Vec<Fixture> =
        serde_json::from_str(&std::fs::read_to_string(root.join("corpus.json")).unwrap()).unwrap();
    fixtures
        .into_iter()
        .map(|fixture| {
            let source = std::fs::read_to_string(root.join(&fixture.file)).unwrap();
            (fixture, source)
        })
        .collect()
}

pub fn assert_fields(fixture: &Fixture, source: &str) {
    let fields = projection_fields(source, fixture.kind).unwrap();
    for (name, expected) in &fixture.fields {
        assert_eq!(fields.get(name), Some(expected), "{}: {name}", fixture.file);
    }
    assert_eq!(
        projection_property(source, fixture.kind, "UID")
            .unwrap()
            .as_deref(),
        Some(fixture.uid.as_str())
    );
}

pub fn edit_fields(title: &str, notes: &str) -> BTreeMap<String, PimValue> {
    BTreeMap::from([
        ("title".into(), PimValue::Text(title.into())),
        ("notes".into(), PimValue::Text(notes.into())),
    ])
}

// Remove only the two exact, known managed lines. Byte equality of everything
// remaining catches changed folding, parameters, alarms, lost/duplicated overrides,
// or any other unmanaged mutation without reproducing the production parser.
pub fn assert_partial_edit(
    fixture: &Fixture,
    original: &str,
    edited: &str,
    title: &str,
    notes: &str,
) {
    let fields = projection_fields(edited, fixture.kind).unwrap();
    assert_eq!(fields.get("title"), Some(&PimValue::Text(title.into())));
    assert_eq!(fields.get("notes"), Some(&PimValue::Text(notes.into())));
    let (title_property, notes_property) = if fixture.kind == PimResourceKind::Contact {
        ("FN", "NOTE")
    } else {
        ("SUMMARY", "DESCRIPTION")
    };
    let PimValue::Text(old_title) = &fixture.fields["title"] else {
        panic!("title");
    };
    let PimValue::Text(old_notes) = &fixture.fields["notes"] else {
        panic!("notes");
    };
    let unchanged = original
        .replace(&format!("{title_property}:{old_title}\r\n"), "")
        .replace(&format!("{notes_property}:{old_notes}\r\n"), "");
    let actual = edited
        .replace(&format!("{title_property}:{title}\r\n"), "")
        .replace(&format!("{notes_property}:{notes}\r\n"), "");
    assert_eq!(
        actual, unchanged,
        "{} unmanaged bytes changed",
        fixture.file
    );
    assert_eq!(
        projection_property(edited, fixture.kind, "UID")
            .unwrap()
            .as_deref(),
        Some(fixture.uid.as_str())
    );
}
