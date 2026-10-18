#[path = "support/pim_corpus.rs"]
mod corpus;

use crypto_core_lib::pim::{
    PimOperationV1, PimUpsertV1, PimValue, materialize_projection, projection_fields,
};
use std::collections::BTreeMap;
use uuid::Uuid;

#[test]
fn imported_corpus_matches_canonical_fields_and_lossless_partial_edits() {
    for (fixture, source) in corpus::corpus() {
        let imported = PimUpsertV1 {
            schema_version: 2,
            resource_kind: fixture.kind,
            resource_id: Uuid::new_v4(),
            dependencies: vec![],
            fields: BTreeMap::new(),
            raw_projection: source.as_bytes().to_vec(),
        };
        let encoded = PimOperationV1::Upsert(imported).encode().unwrap();
        let PimOperationV1::Upsert(imported) = PimOperationV1::decode(&encoded).unwrap() else {
            panic!("upsert");
        };
        let materialized = materialize_projection(&imported, None).unwrap();
        assert_eq!(materialized, source);
        corpus::assert_fields(&fixture, &materialized);
        let edit = PimUpsertV1 {
            raw_projection: vec![],
            fields: corpus::edit_fields("First-party title", "First-party note"),
            dependencies: vec![Uuid::new_v4()],
            ..imported
        };
        let edited = materialize_projection(&edit, Some(&materialized)).unwrap();
        corpus::assert_partial_edit(
            &fixture,
            &source,
            &edited,
            "First-party title",
            "First-party note",
        );
        assert_eq!(
            materialize_projection(&edit, Some(&edited)).unwrap(),
            edited,
            "repeated edits must not duplicate exception components"
        );
    }
}

#[test]
fn reminder_removal_only_removes_the_master_kamori_alarm() {
    let (fixture, source) = corpus::corpus()
        .into_iter()
        .find(|(fixture, _)| fixture.file == "zoned-exceptions.ics")
        .unwrap();
    let edit = PimUpsertV1 {
        schema_version: 2,
        resource_kind: fixture.kind,
        resource_id: Uuid::new_v4(),
        dependencies: vec![],
        fields: BTreeMap::from([("reminder_minutes".into(), PimValue::Null)]),
        raw_projection: vec![],
    };
    let edited = materialize_projection(&edit, Some(&source)).unwrap();
    let managed = "BEGIN:VALARM\r\nX-KAMORI-MANAGED:TRUE\r\nTRIGGER:-PT17M\r\nACTION:DISPLAY\r\nDESCRIPTION:Managed reminder\r\nEND:VALARM\r\n";
    assert_eq!(source.matches(managed).count(), 1);
    assert_eq!(edited, source.replacen(managed, "", 1));
    assert!(
        !projection_fields(&edited, fixture.kind)
            .unwrap()
            .contains_key("reminder_minutes")
    );
}
