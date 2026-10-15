use super::*;
use uuid::Uuid;

fn item(file: &str) -> MobilePimItem {
    let payload = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/pim")
            .join(file),
    )
    .unwrap();
    materialize_mobile_item(MaterializedPimBranch {
        space_id: Uuid::from_u128(1),
        logical_resource_id: Uuid::from_u128(2),
        projection_resource_id: file.into(),
        head_operation_id: Uuid::from_u128(3),
        kind: DavResourceKind::Calendar,
        payload: Some(payload),
        deleted: false,
        conflict: false,
    })
    .unwrap()
}

#[test]
fn mobile_read_model_exposes_typed_recurrence_and_original_series_identity() {
    let value = item("zoned-exceptions.ics");
    assert_eq!(value.resource_id, Uuid::from_u128(2).to_string());
    assert_eq!(value.projection_id, "zoned-exceptions.ics");
    assert_eq!(value.title, "Imported zoned master");
    assert_eq!(value.recurrence_exdates.len(), 1);
    assert_eq!(
        value.recurrence_exdates[0].local.as_deref(),
        Some("20261030T193000")
    );
    assert_eq!(
        value.recurrence_exdates[0].timezone.as_deref(),
        Some("America/New_York")
    );
    assert_eq!(value.recurrence_rdates.len(), 3);
    assert_eq!(value.recurrence_overrides.len(), 2);
    let moved = &value.recurrence_overrides[0];
    assert_eq!(
        moved.recurrence_id.local.as_deref(),
        Some("20261106T193000")
    );
    assert_eq!(
        moved.starts_at.as_ref().unwrap().local.as_deref(),
        Some("20261107T203000")
    );
    assert_eq!(
        moved.ends_at.as_ref().unwrap().local.as_deref(),
        Some("20261107T204500")
    );
    assert_eq!(moved.title.as_deref(), Some("Detached override"));
    assert_eq!(
        moved.notes.as_deref(),
        Some("Leave this exception untouched")
    );
    assert!(!moved.cancelled);
    assert!(value.recurrence_overrides[1].cancelled);
    assert!(value.recurrence_unsupported.is_empty());
}

#[test]
fn mobile_read_model_keeps_date_identity_and_flags_unexpanded_advanced_forms() {
    let value = item("all-day-exceptions.ics");
    assert_eq!(value.recurrence_exdates[0].kind, "date");
    assert_eq!(
        value.recurrence_exdates[0].date.as_deref(),
        Some("2026-11-06")
    );
    assert_eq!(
        value.recurrence_overrides[0]
            .starts_at
            .as_ref()
            .unwrap()
            .date
            .as_deref(),
        Some("2026-11-14")
    );
    let advanced = item("unsupported-period-range.ics");
    assert_eq!(
        advanced.recurrence_unsupported,
        ["RDATE;VALUE=PERIOD", "RECURRENCE-ID;RANGE"]
    );
    assert!(advanced.recurrence_rdates.is_empty());
    assert!(advanced.recurrence_overrides.is_empty());
}
