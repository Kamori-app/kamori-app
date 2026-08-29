//! Versioned PIM operation codec independent from DAV and transport ordering.

use chrono::{LocalResult, NaiveDateTime, TimeZone as _};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PimResourceKind {
    CalendarEvent,
    Task,
    Contact,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "type", content = "value")]
pub enum PimValue {
    Text(String),
    Integer(i64),
    Boolean(bool),
    TextList(Vec<String>),
    Record(BTreeMap<String, String>),
    Records(Vec<BTreeMap<String, String>>),
    #[serde(with = "serde_bytes")]
    Bytes(Vec<u8>),
    Null,
}

pub const CURRENT_PIM_SCHEMA_VERSION: u16 = 2;

const fn legacy_pim_schema_version() -> u16 {
    1
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PimUpsertV1 {
    #[serde(default = "legacy_pim_schema_version")]
    pub schema_version: u16,
    pub resource_kind: PimResourceKind,
    pub resource_id: Uuid,
    /// Operation ids this edit observed. These are semantic causality, not server cursors.
    pub dependencies: Vec<Uuid>,
    /// Field-level mutations emitted by first-party clients.
    pub fields: BTreeMap<String, PimValue>,
    /// Optional lossless iCalendar/vCard source retained by adapter imports.
    #[serde(with = "serde_bytes")]
    pub raw_projection: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PimDeleteV1 {
    #[serde(default = "legacy_pim_schema_version")]
    pub schema_version: u16,
    pub resource_kind: PimResourceKind,
    pub resource_id: Uuid,
    pub dependencies: Vec<Uuid>,
    /// Adapter-local resource name when it differs from the canonical stream id.
    pub projection_resource_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "operation")]
pub enum PimOperationV1 {
    Upsert(PimUpsertV1),
    Delete(PimDeleteV1),
}

impl PimOperationV1 {
    pub fn encode(&self) -> anyhow::Result<Vec<u8>> {
        self.validate()?;
        Ok(rmp_serde::to_vec_named(self)?)
    }

    pub fn decode(bytes: &[u8]) -> anyhow::Result<Self> {
        let operation: Self = rmp_serde::from_slice(bytes)?;
        operation.validate()?;
        Ok(operation)
    }

    /// Validates invariants which every v1 materializer relies on.
    ///
    /// PIM v1 deliberately models a linear edit or an explicit conflict branch.
    /// Multi-parent convergence belongs to the future CRDT payload codec; accepting
    /// more than one parent here would let different clients choose different bases.
    pub fn validate(&self) -> anyhow::Result<()> {
        let (resource_id, dependencies) = match self {
            Self::Upsert(value) => (value.resource_id, value.dependencies.as_slice()),
            Self::Delete(value) => (value.resource_id, value.dependencies.as_slice()),
        };
        let schema_version = match self {
            Self::Upsert(value) => value.schema_version,
            Self::Delete(value) => value.schema_version,
        };
        anyhow::ensure!(
            (1..=CURRENT_PIM_SCHEMA_VERSION).contains(&schema_version),
            "unsupported PIM operation schema version"
        );
        anyhow::ensure!(!resource_id.is_nil(), "PIM resource id must be non-nil");
        anyhow::ensure!(
            dependencies.len() <= 1,
            "PIM v1 operations support at most one parent"
        );
        anyhow::ensure!(
            dependencies.iter().all(|dependency| !dependency.is_nil()),
            "PIM parent id must be non-nil"
        );
        if let Self::Upsert(value) = self
            && !value.raw_projection.is_empty()
        {
            let projection = std::str::from_utf8(&value.raw_projection)
                .map_err(|_| anyhow::anyhow!("raw PIM projection is not UTF-8"))?;
            validate_projection(value.resource_kind, projection)?;
        }
        if let Self::Upsert(value) = self {
            if value.schema_version >= 2 {
                validate_v2_fields(value)?;
            } else {
                for field in ["starts_at", "ends_at"] {
                    if let Some(PimValue::Text(value)) = value.fields.get(field) {
                        validate_compact_utc(value, field)?;
                    }
                }
                if let (Some(PimValue::Text(starts_at)), Some(PimValue::Text(ends_at))) =
                    (value.fields.get("starts_at"), value.fields.get("ends_at"))
                {
                    anyhow::ensure!(
                        ends_at >= starts_at,
                        "PIM event end must not precede its start"
                    );
                }
            }
        }
        Ok(())
    }
}

fn validate_v2_fields(upsert: &PimUpsertV1) -> anyhow::Result<()> {
    validate_optional_text(&upsert.fields, "title", 500)?;
    if let Some(PimValue::Text(title)) = upsert.fields.get("title") {
        anyhow::ensure!(!title.trim().is_empty(), "title must not be blank");
    }
    validate_optional_text(&upsert.fields, "notes", 20_000)?;
    validate_optional_text(&upsert.fields, "location", 2_000)?;
    validate_optional_text(&upsert.fields, "organization", 1_000)?;
    validate_optional_text(&upsert.fields, "job_title", 1_000)?;
    validate_optional_text(&upsert.fields, "url", 4_096)?;
    if let Some(PimValue::Text(value)) = upsert.fields.get("url") {
        anyhow::ensure!(
            !value
                .chars()
                .any(|character| matches!(character, '\r' | '\n' | '\0')),
            "url contains a line break"
        );
    }
    for field in ["completed", "favorite"] {
        match upsert.fields.get(field) {
            None | Some(PimValue::Boolean(_)) | Some(PimValue::Null) => {}
            Some(_) => anyhow::bail!("{field} must be boolean"),
        }
    }
    for field in ["dtstamp", "completed_at"] {
        match upsert.fields.get(field) {
            None | Some(PimValue::Null) => {}
            Some(PimValue::Text(value)) => validate_compact_utc(value, field)?,
            Some(_) => anyhow::bail!("{field} must be a UTC date-time string"),
        }
    }

    if let Some(value) = upsert.fields.get("categories") {
        match value {
            PimValue::TextList(values) => {
                anyhow::ensure!(values.len() <= 50, "categories contains too many values");
                anyhow::ensure!(
                    values
                        .iter()
                        .all(|value| !value.trim().is_empty() && value.chars().count() <= 100),
                    "category values must contain 1 to 100 characters"
                );
            }
            PimValue::Null => {}
            _ => anyhow::bail!("categories must be a text list"),
        }
    }

    if let Some(value) = upsert.fields.get("priority") {
        match value {
            PimValue::Integer(value) => {
                anyhow::ensure!((0..=9).contains(value), "priority must be between 0 and 9")
            }
            PimValue::Null => {}
            _ => anyhow::bail!("priority must be an integer"),
        }
    }
    if let Some(value) = upsert.fields.get("reminder_minutes") {
        match value {
            PimValue::Integer(value) => anyhow::ensure!(
                (0..=40_320).contains(value),
                "reminder_minutes must be between 0 and 40320"
            ),
            PimValue::Null => {}
            _ => anyhow::bail!("reminder_minutes must be an integer"),
        }
    }
    if let Some(value) = upsert.fields.get("recurrence_rule") {
        match value {
            PimValue::Text(value) => {
                anyhow::ensure!(value.len() <= 1_024, "recurrence rule is too long");
                anyhow::ensure!(
                    !value.is_empty()
                        && value.is_ascii()
                        && !value
                            .bytes()
                            .any(|byte| matches!(byte, b'\r' | b'\n' | b':')),
                    "recurrence rule is invalid"
                );
            }
            PimValue::Null => {}
            _ => anyhow::bail!("recurrence_rule must be text"),
        }
    }

    for field in ["starts_at", "ends_at", "due_at"] {
        if let Some(value) = upsert.fields.get(field) {
            validate_temporal_value(value, field)?;
        }
    }
    if let Some(PimValue::Record(starts_at)) = upsert.fields.get("starts_at")
        && let Some(PimValue::Record(ends_at)) = upsert.fields.get("ends_at")
    {
        validate_temporal_order(starts_at, ends_at)?;
    }

    if let Some(value) = upsert.fields.get("birthday") {
        match value {
            PimValue::Text(value) => validate_vcard_birthday(value)?,
            PimValue::Null => {}
            _ => anyhow::bail!("birthday must be a calendar date"),
        }
    }
    if let Some(value) = upsert.fields.get("name") {
        validate_string_record(value, "name", 8, 500)?;
    }
    for field in ["emails", "phones"] {
        validate_labeled_values(upsert.fields.get(field), field)?;
    }
    if let Some(value) = upsert.fields.get("addresses") {
        match value {
            PimValue::Records(values) => {
                anyhow::ensure!(values.len() <= 20, "addresses contains too many values");
                for record in values {
                    anyhow::ensure!(record.len() <= 10, "address contains too many fields");
                    for value in record.values() {
                        anyhow::ensure!(
                            value.chars().count() <= 2_000
                                && !value
                                    .chars()
                                    .any(|character| matches!(character, '\r' | '\n')),
                            "address field is invalid"
                        );
                    }
                }
            }
            PimValue::Null => {}
            _ => anyhow::bail!("addresses must be a record list"),
        }
    }
    Ok(())
}

fn validate_vcard_birthday(value: &str) -> anyhow::Result<()> {
    if let Some(partial_date) = value.strip_prefix("--") {
        let normalized = partial_date.replace('-', "");
        anyhow::ensure!(
            normalized.len() == 4 && normalized.bytes().all(|byte| byte.is_ascii_digit()),
            "birthday must use YYYY-MM-DD or --MM-DD"
        );
        let month: u32 = normalized[0..2].parse()?;
        let day: u32 = normalized[2..4].parse()?;
        validate_date_parts(2000, month, day, "birthday")?;
        return Ok(());
    }
    if value.len() == 8 && value.bytes().all(|byte| byte.is_ascii_digit()) {
        return validate_calendar_date(
            &format!("{}-{}-{}", &value[0..4], &value[4..6], &value[6..8]),
            "birthday",
        );
    }
    validate_calendar_date(value, "birthday")
}

fn validate_optional_text(
    fields: &BTreeMap<String, PimValue>,
    name: &str,
    max_chars: usize,
) -> anyhow::Result<()> {
    match fields.get(name) {
        None | Some(PimValue::Null) => Ok(()),
        Some(PimValue::Text(value)) => {
            anyhow::ensure!(
                value.chars().count() <= max_chars
                    && !value
                        .chars()
                        .any(|character| matches!(character, '\r' | '\0')),
                "{name} is invalid"
            );
            Ok(())
        }
        Some(_) => anyhow::bail!("{name} must be text"),
    }
}

fn validate_string_record(
    value: &PimValue,
    field: &str,
    max_entries: usize,
    max_chars: usize,
) -> anyhow::Result<()> {
    match value {
        PimValue::Record(record) => {
            anyhow::ensure!(
                record.len() <= max_entries,
                "{field} contains too many values"
            );
            anyhow::ensure!(
                record.values().all(|value| {
                    value.chars().count() <= max_chars
                        && !value
                            .chars()
                            .any(|character| matches!(character, '\r' | '\0'))
                }),
                "{field} contains an invalid value"
            );
            Ok(())
        }
        PimValue::Null => Ok(()),
        _ => anyhow::bail!("{field} must be a record"),
    }
}

fn validate_labeled_values(value: Option<&PimValue>, field: &str) -> anyhow::Result<()> {
    let Some(value) = value else {
        return Ok(());
    };
    match value {
        PimValue::Records(values) => {
            anyhow::ensure!(values.len() <= 50, "{field} contains too many values");
            for record in values {
                let value = record.get("value").map(String::as_str).unwrap_or_default();
                let label = record.get("label").map(String::as_str).unwrap_or_default();
                anyhow::ensure!(
                    !value.trim().is_empty()
                        && value.chars().count() <= 4_096
                        && !value
                            .chars()
                            .any(|character| matches!(character, '\r' | '\n' | '\0')),
                    "{field} contains an invalid value"
                );
                anyhow::ensure!(
                    label.chars().count() <= 80
                        && !label
                            .chars()
                            .any(|character| matches!(character, '\r' | '\n' | '\0')),
                    "{field} contains an invalid label"
                );
            }
            Ok(())
        }
        PimValue::Null => Ok(()),
        _ => anyhow::bail!("{field} must be a record list"),
    }
}

fn validate_temporal_value(value: &PimValue, field: &str) -> anyhow::Result<()> {
    match value {
        PimValue::Null => Ok(()),
        PimValue::Record(record) => {
            match record.get("kind").map(String::as_str) {
                Some("date") => validate_calendar_date(
                    record.get("date").map(String::as_str).unwrap_or_default(),
                    field,
                )?,
                Some("utc") => validate_compact_utc(
                    record.get("utc").map(String::as_str).unwrap_or_default(),
                    field,
                )?,
                Some("zoned_datetime") => {
                    validate_compact_local(
                        record.get("local").map(String::as_str).unwrap_or_default(),
                        field,
                    )?;
                    validate_timezone(
                        record
                            .get("timezone")
                            .map(String::as_str)
                            .unwrap_or_default(),
                        field,
                    )?;
                    let utc = record
                        .get("utc")
                        .ok_or_else(|| anyhow::anyhow!("{field} zoned date-time requires utc"))?;
                    validate_compact_utc(utc, field)?;
                    validate_zoned_instant_consistency(record, field)?;
                }
                _ => anyhow::bail!("{field} has an unsupported temporal kind"),
            }
            Ok(())
        }
        _ => anyhow::bail!("{field} must be a temporal record"),
    }
}

fn validate_temporal_order(
    starts_at: &BTreeMap<String, String>,
    ends_at: &BTreeMap<String, String>,
) -> anyhow::Result<()> {
    let start_kind = starts_at.get("kind").map(String::as_str);
    let end_kind = ends_at.get("kind").map(String::as_str);
    anyhow::ensure!(
        start_kind == end_kind,
        "PIM event start and end must use the same temporal kind"
    );
    let comparable = match (start_kind, end_kind) {
        (Some("date"), Some("date")) => starts_at.get("date").zip(ends_at.get("date")),
        (Some("utc"), Some("utc")) => starts_at.get("utc").zip(ends_at.get("utc")),
        (Some("zoned_datetime"), Some("zoned_datetime")) => {
            anyhow::ensure!(
                starts_at.get("timezone") == ends_at.get("timezone"),
                "PIM event start and end must use the same timezone"
            );
            starts_at.get("utc").zip(ends_at.get("utc"))
        }
        _ => None,
    };
    if let Some((start, end)) = comparable {
        anyhow::ensure!(end > start, "PIM event end must be later than its start");
    }
    Ok(())
}

fn validate_zoned_instant_consistency(
    record: &BTreeMap<String, String>,
    field: &str,
) -> anyhow::Result<()> {
    let local = record.get("local").map(String::as_str).unwrap_or_default();
    let timezone = record
        .get("timezone")
        .map(String::as_str)
        .unwrap_or_default();
    let utc = record.get("utc").map(String::as_str).unwrap_or_default();
    let local = NaiveDateTime::parse_from_str(local, "%Y%m%dT%H%M%S")
        .map_err(|_| anyhow::anyhow!("invalid {field} local date-time"))?;
    let timezone: Tz = timezone
        .parse()
        .map_err(|_| anyhow::anyhow!("{field} contains an unknown IANA timezone"))?;
    let supplied = NaiveDateTime::parse_from_str(utc, "%Y%m%dT%H%M%SZ")
        .map_err(|_| anyhow::anyhow!("invalid {field} UTC instant"))?;
    let matches = match timezone.from_local_datetime(&local) {
        LocalResult::Single(value) => value.naive_utc() == supplied,
        LocalResult::Ambiguous(first, second) => {
            first.naive_utc() == supplied || second.naive_utc() == supplied
        }
        LocalResult::None => anyhow::bail!("{field} does not exist in its timezone"),
    };
    anyhow::ensure!(
        matches,
        "{field} local date-time, timezone, and UTC instant disagree"
    );
    Ok(())
}

fn validate_calendar_date(value: &str, field: &str) -> anyhow::Result<()> {
    let bytes = value.as_bytes();
    anyhow::ensure!(
        bytes.len() == 10
            && bytes[4] == b'-'
            && bytes[7] == b'-'
            && bytes
                .iter()
                .enumerate()
                .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit()),
        "{field} must use YYYY-MM-DD"
    );
    validate_date_parts(
        value[0..4].parse()?,
        value[5..7].parse()?,
        value[8..10].parse()?,
        field,
    )
}

fn validate_compact_local(value: &str, field: &str) -> anyhow::Result<()> {
    let bytes = value.as_bytes();
    anyhow::ensure!(
        bytes.len() == 15
            && bytes[8] == b'T'
            && bytes
                .iter()
                .enumerate()
                .all(|(index, byte)| index == 8 || byte.is_ascii_digit()),
        "{field} must use YYYYMMDDTHHMMSS"
    );
    let year = value[0..4].parse()?;
    let month = value[4..6].parse()?;
    let day = value[6..8].parse()?;
    validate_date_parts(year, month, day, field)?;
    let hour: u32 = value[9..11].parse()?;
    let minute: u32 = value[11..13].parse()?;
    let second: u32 = value[13..15].parse()?;
    anyhow::ensure!(
        hour <= 23 && minute <= 59 && second <= 59,
        "invalid {field}"
    );
    Ok(())
}

fn validate_date_parts(year: u32, month: u32, day: u32, field: &str) -> anyhow::Result<()> {
    let leap_year =
        year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap_year => 29,
        2 => 28,
        _ => 0,
    };
    anyhow::ensure!(year > 0 && day > 0 && day <= max_day, "invalid {field}");
    Ok(())
}

fn validate_timezone(value: &str, field: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        !value.is_empty()
            && value.len() <= 128
            && value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'_' | b'+' | b'-')
            }),
        "{field} contains an invalid timezone"
    );
    Ok(())
}

/// One materialized branch inside a full per-stream checkpoint.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PimSnapshotBranchV2 {
    pub projection_resource_id: String,
    pub head_operation_id: Uuid,
    pub deleted: bool,
    #[serde(with = "serde_bytes")]
    pub materialized_projection: Vec<u8>,
}

/// Full per-stream checkpoint used for epoch rotation and new-device recovery.
/// A stream can contain explicit conflict branches, all of which must survive
/// an epoch rotation.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PimSnapshotV2 {
    pub schema_version: u16,
    pub covers_through_space_seq: u64,
    pub resource_kind: PimResourceKind,
    pub resource_id: Uuid,
    pub branches: Vec<PimSnapshotBranchV2>,
}

/// Minimal version-graph input used by every PIM v1 materializer.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PimBranchNodeV1 {
    pub operation_id: Uuid,
    pub parent_operation_id: Option<Uuid>,
    /// Snapshot frontiers already have an explicit stable projection identity.
    pub seed_projection_resource_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PimBranchAssignmentV1 {
    pub operation_id: Uuid,
    pub projection_resource_id: String,
    pub head: bool,
}

fn conflict_projection_resource_id(
    default_projection_resource_id: &str,
    operation_id: Uuid,
) -> String {
    match default_projection_resource_id.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() && !extension.is_empty() => {
            format!("{stem}.conflict-{operation_id}.{extension}")
        }
        _ => format!("{default_projection_resource_id}.conflict-{operation_id}"),
    }
}

/// Assigns stable branch identities from the complete known single-parent DAG.
/// The result is independent of arrival order. At a fork, the lowest operation
/// id inherits its parent's projection and every sibling starts a stable branch.
pub fn assign_pim_branches(
    default_projection_resource_id: &str,
    nodes: &[PimBranchNodeV1],
) -> anyhow::Result<Vec<PimBranchAssignmentV1>> {
    anyhow::ensure!(
        !default_projection_resource_id.trim().is_empty(),
        "default projection resource id is required"
    );
    let by_id = nodes
        .iter()
        .map(|node| (node.operation_id, node))
        .collect::<HashMap<_, _>>();
    anyhow::ensure!(by_id.len() == nodes.len(), "duplicate PIM operation id");
    anyhow::ensure!(
        nodes.iter().all(|node| !node.operation_id.is_nil()),
        "PIM operation id must be non-nil"
    );

    let mut children = HashMap::<Uuid, Vec<Uuid>>::new();
    let mut roots = Vec::new();
    for node in nodes {
        if let Some(parent) = node.parent_operation_id {
            anyhow::ensure!(
                node.seed_projection_resource_id.is_none(),
                "only a PIM graph root may seed a projection branch id"
            );
            anyhow::ensure!(
                by_id.contains_key(&parent),
                "PIM operation dependency is missing"
            );
            children.entry(parent).or_default().push(node.operation_id);
        } else {
            roots.push(node.operation_id);
        }
    }
    roots.sort_unstable();
    for values in children.values_mut() {
        values.sort_unstable();
    }

    let mut assigned = HashMap::<Uuid, String>::new();
    let mut used_projections = HashSet::<String>::new();
    let mut unseeded_root_index = 0_usize;
    let mut queue = VecDeque::new();
    for root in roots {
        let node = by_id[&root];
        let projection = if let Some(seed) = node.seed_projection_resource_id.as_ref() {
            anyhow::ensure!(!seed.trim().is_empty(), "PIM snapshot branch id is empty");
            seed.clone()
        } else if unseeded_root_index == 0
            && !used_projections.contains(default_projection_resource_id)
        {
            unseeded_root_index += 1;
            default_projection_resource_id.to_string()
        } else {
            unseeded_root_index += 1;
            conflict_projection_resource_id(default_projection_resource_id, root)
        };
        anyhow::ensure!(
            used_projections.insert(projection.clone()),
            "duplicate PIM projection branch id"
        );
        assigned.insert(root, projection);
        queue.push_back(root);
    }

    while let Some(parent) = queue.pop_front() {
        let parent_projection = assigned
            .get(&parent)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("PIM version graph is cyclic"))?;
        for (index, child) in children.get(&parent).into_iter().flatten().enumerate() {
            let projection = if index == 0 {
                parent_projection.clone()
            } else {
                let projection =
                    conflict_projection_resource_id(default_projection_resource_id, *child);
                anyhow::ensure!(
                    used_projections.insert(projection.clone()),
                    "duplicate PIM projection branch id"
                );
                projection
            };
            assigned.insert(*child, projection);
            queue.push_back(*child);
        }
    }
    anyhow::ensure!(assigned.len() == nodes.len(), "PIM version graph is cyclic");

    let mut result = assigned
        .into_iter()
        .map(
            |(operation_id, projection_resource_id)| PimBranchAssignmentV1 {
                operation_id,
                projection_resource_id,
                head: !children.contains_key(&operation_id),
            },
        )
        .collect::<Vec<_>>();
    result.sort_by_key(|assignment| assignment.operation_id);
    Ok(result)
}

impl PimSnapshotV2 {
    pub const SCHEMA_VERSION: u16 = 2;

    pub fn encode(&self) -> anyhow::Result<Vec<u8>> {
        self.validate()?;
        Ok(rmp_serde::to_vec_named(self)?)
    }

    pub fn decode(bytes: &[u8]) -> anyhow::Result<Self> {
        let snapshot: Self = rmp_serde::from_slice(bytes)?;
        snapshot.validate()?;
        Ok(snapshot)
    }

    fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.schema_version == Self::SCHEMA_VERSION,
            "unsupported PIM snapshot schema version"
        );
        anyhow::ensure!(
            !self.resource_id.is_nil(),
            "snapshot resource id must be non-nil"
        );
        anyhow::ensure!(!self.branches.is_empty(), "snapshot has no branches");
        let mut projection_ids = HashSet::new();
        let mut head_ids = HashSet::new();
        for branch in &self.branches {
            anyhow::ensure!(
                !branch.projection_resource_id.trim().is_empty(),
                "snapshot projection resource id is required"
            );
            anyhow::ensure!(
                !branch.head_operation_id.is_nil(),
                "snapshot head operation id must be non-nil"
            );
            anyhow::ensure!(
                projection_ids.insert(branch.projection_resource_id.as_str()),
                "snapshot projection resource ids must be unique"
            );
            anyhow::ensure!(
                head_ids.insert(branch.head_operation_id),
                "snapshot head operation ids must be unique"
            );
            anyhow::ensure!(
                branch.deleted || !branch.materialized_projection.is_empty(),
                "live snapshot projection is required"
            );
            if !branch.deleted {
                std::str::from_utf8(&branch.materialized_projection)
                    .map_err(|_| anyhow::anyhow!("snapshot projection is not UTF-8"))?;
            }
        }
        Ok(())
    }
}

/// Materializes a lossless DAV projection from a field operation. Existing
/// unknown properties are retained when a first-party edit changes known
/// fields, while adapter imports may supply their complete raw projection.
pub fn materialize_projection(
    upsert: &PimUpsertV1,
    existing: Option<&str>,
) -> anyhow::Result<String> {
    PimOperationV1::Upsert(upsert.clone()).validate()?;
    if !upsert.raw_projection.is_empty() {
        let projection = std::str::from_utf8(&upsert.raw_projection)
            .map_err(|_| anyhow::anyhow!("raw PIM projection is not UTF-8"))?
            .to_string();
        validate_projection(upsert.resource_kind, &projection)?;
        return Ok(projection);
    }
    if let Some(existing) = existing {
        return patch_projection(existing, upsert);
    }
    let uid = upsert.resource_id;
    let title = escape_projection(&text_field(upsert, "title"));
    anyhow::ensure!(!title.is_empty(), "new PIM resource title is required");
    let projection = match upsert.resource_kind {
        PimResourceKind::Contact => {
            let mut lines = vec![
                "BEGIN:VCARD".to_string(),
                "VERSION:4.0".to_string(),
                format!("UID:{uid}"),
                format!("FN:{title}"),
            ];
            if let Some(PimValue::Record(name)) = upsert.fields.get("name") {
                lines.push(render_structured_name(name));
            }
            append_contact_values(&mut lines, upsert, "emails", "email", "EMAIL");
            append_contact_values(&mut lines, upsert, "phones", "phone", "TEL");
            append_addresses(&mut lines, upsert);
            append_text_property(&mut lines, upsert, "organization", "ORG");
            append_text_property(&mut lines, upsert, "job_title", "TITLE");
            append_raw_property(&mut lines, upsert, "birthday", "BDAY");
            append_raw_property(&mut lines, upsert, "url", "URL");
            append_text_property(&mut lines, upsert, "notes", "NOTE");
            append_categories(&mut lines, upsert);
            if matches!(upsert.fields.get("favorite"), Some(PimValue::Boolean(true))) {
                lines.push("X-KAMORI-FAVORITE:TRUE".to_string());
            }
            lines.push("END:VCARD".to_string());
            format!("{}\r\n", lines.join("\r\n"))
        }
        PimResourceKind::CalendarEvent => {
            let dtstamp = required_utc_field(upsert, "dtstamp")?;
            let starts_at = required_temporal_line(upsert, "starts_at", "DTSTART")?;
            let ends_at = optional_temporal_line(upsert, "ends_at", "DTEND")?;
            let mut lines = vec![
                "BEGIN:VCALENDAR".to_string(),
                "VERSION:2.0".to_string(),
                "PRODID:-//Kamori//EN".to_string(),
                "BEGIN:VEVENT".to_string(),
                format!("UID:{uid}"),
                format!("DTSTAMP:{dtstamp}"),
                format!("SUMMARY:{title}"),
                starts_at,
            ];
            if let Some(ends_at) = ends_at {
                lines.push(ends_at);
            }
            append_text_property(&mut lines, upsert, "location", "LOCATION");
            append_text_property(&mut lines, upsert, "notes", "DESCRIPTION");
            append_raw_property(&mut lines, upsert, "recurrence_rule", "RRULE");
            append_categories(&mut lines, upsert);
            append_managed_alarm(&mut lines, upsert, false);
            lines.extend(["END:VEVENT".to_string(), "END:VCALENDAR".to_string()]);
            format!("{}\r\n", lines.join("\r\n"))
        }
        PimResourceKind::Task => {
            let dtstamp = required_utc_field(upsert, "dtstamp")?;
            let completed = matches!(
                upsert.fields.get("completed"),
                Some(PimValue::Boolean(true))
            );
            let mut lines = vec![
                "BEGIN:VCALENDAR".to_string(),
                "VERSION:2.0".to_string(),
                "PRODID:-//Kamori//EN".to_string(),
                "BEGIN:VTODO".to_string(),
                format!("UID:{uid}"),
                format!("DTSTAMP:{dtstamp}"),
                format!("SUMMARY:{title}"),
                format!(
                    "STATUS:{}",
                    if completed {
                        "COMPLETED"
                    } else {
                        "NEEDS-ACTION"
                    }
                ),
            ];
            if let Some(line) = optional_temporal_line(upsert, "starts_at", "DTSTART")? {
                lines.push(line);
            }
            if let Some(line) = optional_temporal_line(upsert, "due_at", "DUE")? {
                lines.push(line);
            }
            append_raw_property(&mut lines, upsert, "completed_at", "COMPLETED");
            append_integer_property(&mut lines, upsert, "priority", "PRIORITY");
            append_text_property(&mut lines, upsert, "notes", "DESCRIPTION");
            append_raw_property(&mut lines, upsert, "recurrence_rule", "RRULE");
            append_categories(&mut lines, upsert);
            append_managed_alarm(&mut lines, upsert, true);
            lines.extend(["END:VTODO".to_string(), "END:VCALENDAR".to_string()]);
            format!("{}\r\n", lines.join("\r\n"))
        }
    };
    validate_projection(upsert.resource_kind, &projection)?;
    Ok(projection)
}

fn append_text_property(
    lines: &mut Vec<String>,
    upsert: &PimUpsertV1,
    field: &str,
    property: &str,
) {
    if let Some(PimValue::Text(value)) = upsert.fields.get(field)
        && !value.is_empty()
    {
        lines.push(fold_content_line(property, &escape_projection(value)));
    }
}

fn append_raw_property(lines: &mut Vec<String>, upsert: &PimUpsertV1, field: &str, property: &str) {
    if let Some(PimValue::Text(value)) = upsert.fields.get(field)
        && !value.is_empty()
    {
        lines.push(fold_content_line(property, value));
    }
}

fn append_integer_property(
    lines: &mut Vec<String>,
    upsert: &PimUpsertV1,
    field: &str,
    property: &str,
) {
    if let Some(PimValue::Integer(value)) = upsert.fields.get(field) {
        lines.push(format!("{property}:{value}"));
    }
}

fn append_categories(lines: &mut Vec<String>, upsert: &PimUpsertV1) {
    if let Some(PimValue::TextList(values)) = upsert.fields.get("categories")
        && !values.is_empty()
    {
        let value = values
            .iter()
            .map(|value| escape_projection(value))
            .collect::<Vec<_>>()
            .join(",");
        lines.push(fold_content_line("CATEGORIES", &value));
    }
}

fn append_managed_alarm(lines: &mut Vec<String>, upsert: &PimUpsertV1, related_to_end: bool) {
    if let Some(PimValue::Integer(minutes)) = upsert.fields.get("reminder_minutes") {
        lines.extend([
            "BEGIN:VALARM".to_string(),
            "ACTION:DISPLAY".to_string(),
            "DESCRIPTION:Kamori reminder".to_string(),
            format!(
                "TRIGGER{}:-PT{minutes}M",
                if related_to_end { ";RELATED=END" } else { "" }
            ),
            "X-KAMORI-MANAGED:TRUE".to_string(),
            "END:VALARM".to_string(),
        ]);
    }
}

fn append_contact_values(
    lines: &mut Vec<String>,
    upsert: &PimUpsertV1,
    plural_field: &str,
    legacy_field: &str,
    property: &str,
) {
    if let Some(PimValue::Records(values)) = upsert.fields.get(plural_field) {
        for record in values {
            let value = record.get("value").map(String::as_str).unwrap_or_default();
            if value.is_empty() {
                continue;
            }
            let label = record.get("label").map(String::as_str).unwrap_or_default();
            let head = preserved_property_head(record, property, label)
                .unwrap_or_else(|| labeled_property_head(property, label));
            lines.push(fold_content_line(&head, &escape_projection(value)));
        }
        return;
    }
    append_text_property(lines, upsert, legacy_field, property);
}

fn preserved_property_head(
    record: &BTreeMap<String, String>,
    property: &str,
    label: &str,
) -> Option<String> {
    let head = record.get("raw_head")?;
    if head
        .chars()
        .any(|character| matches!(character, '\r' | '\n' | '\0'))
        || property_name(head).ok()?.as_str() != property
        || !projection_label(head)
            .trim()
            .eq_ignore_ascii_case(label.trim())
    {
        return None;
    }
    Some(head.clone())
}

fn labeled_property_head(property: &str, label: &str) -> String {
    if label.is_empty() {
        return property.to_string();
    }
    let normalized = label.trim().to_ascii_uppercase();
    if matches!(
        normalized.as_str(),
        "HOME" | "WORK" | "CELL" | "MOBILE" | "FAX" | "OTHER"
    ) {
        let normalized = if normalized == "MOBILE" {
            "CELL"
        } else {
            normalized.as_str()
        };
        format!("{property};TYPE={normalized}")
    } else {
        let escaped = label.replace('\\', "\\\\").replace('"', "\\\"");
        format!("{property};X-KAMORI-LABEL=\"{escaped}\"")
    }
}

fn render_structured_name(name: &BTreeMap<String, String>) -> String {
    let part = |key: &str| escape_projection(name.get(key).map(String::as_str).unwrap_or_default());
    fold_content_line(
        "N",
        &format!(
            "{};{};{};{};{}",
            part("family"),
            part("given"),
            part("middle"),
            part("prefix"),
            part("suffix")
        ),
    )
}

fn append_addresses(lines: &mut Vec<String>, upsert: &PimUpsertV1) {
    let Some(PimValue::Records(addresses)) = upsert.fields.get("addresses") else {
        return;
    };
    for address in addresses {
        let part =
            |key: &str| escape_projection(address.get(key).map(String::as_str).unwrap_or_default());
        let label = address.get("label").map(String::as_str).unwrap_or_default();
        let head = preserved_property_head(address, "ADR", label)
            .unwrap_or_else(|| labeled_property_head("ADR", label));
        lines.push(fold_content_line(
            &head,
            &format!(
                "{};{};{};{};{};{};{}",
                part("po_box"),
                part("extended"),
                part("street"),
                part("locality"),
                part("region"),
                part("postal_code"),
                part("country")
            ),
        ));
    }
}

fn required_temporal_line(
    upsert: &PimUpsertV1,
    field: &str,
    property: &str,
) -> anyhow::Result<String> {
    optional_temporal_line(upsert, field, property)?
        .ok_or_else(|| anyhow::anyhow!("new {:?} requires {field}", upsert.resource_kind))
}

fn optional_temporal_line(
    upsert: &PimUpsertV1,
    field: &str,
    property: &str,
) -> anyhow::Result<Option<String>> {
    match upsert.fields.get(field) {
        None | Some(PimValue::Null) => Ok(None),
        Some(PimValue::Text(value)) => {
            validate_compact_utc(value, field)?;
            Ok(Some(format!("{property}:{value}")))
        }
        Some(PimValue::Record(record)) => temporal_content_line(property, record).map(Some),
        Some(_) => anyhow::bail!("{field} must be a temporal value"),
    }
}

fn temporal_content_line(
    property: &str,
    record: &BTreeMap<String, String>,
) -> anyhow::Result<String> {
    match record.get("kind").map(String::as_str) {
        Some("date") => {
            let date = record.get("date").map(String::as_str).unwrap_or_default();
            validate_calendar_date(date, property)?;
            Ok(format!("{property};VALUE=DATE:{}", date.replace('-', "")))
        }
        Some("utc") => {
            let utc = record.get("utc").map(String::as_str).unwrap_or_default();
            validate_compact_utc(utc, property)?;
            Ok(format!("{property}:{utc}"))
        }
        Some("zoned_datetime") => {
            let local = record.get("local").map(String::as_str).unwrap_or_default();
            let timezone = record
                .get("timezone")
                .map(String::as_str)
                .unwrap_or_default();
            validate_compact_local(local, property)?;
            validate_timezone(timezone, property)?;
            Ok(format!("{property};TZID={timezone}:{local}"))
        }
        _ => anyhow::bail!("{property} has an unsupported temporal kind"),
    }
}

fn text_field(upsert: &PimUpsertV1, name: &str) -> String {
    match upsert.fields.get(name) {
        Some(PimValue::Text(value)) => value.clone(),
        _ => String::new(),
    }
}

fn validate_compact_utc(value: &str, field: &str) -> anyhow::Result<()> {
    let bytes = value.as_bytes();
    anyhow::ensure!(
        bytes.len() == 16
            && bytes[8] == b'T'
            && bytes[15] == b'Z'
            && bytes
                .iter()
                .enumerate()
                .all(|(index, byte)| index == 8 || index == 15 || byte.is_ascii_digit()),
        "{field} must use compact UTC format YYYYMMDDTHHMMSSZ"
    );
    let year: u32 = value[0..4].parse()?;
    let month: u32 = value[4..6].parse()?;
    let day: u32 = value[6..8].parse()?;
    validate_date_parts(year, month, day, field)?;
    let hour: u32 = value[9..11].parse()?;
    let minute: u32 = value[11..13].parse()?;
    let second: u32 = value[13..15].parse()?;
    anyhow::ensure!(
        hour <= 23 && minute <= 59 && second <= 59,
        "invalid {field}"
    );
    Ok(())
}

fn required_utc_field(upsert: &PimUpsertV1, name: &str) -> anyhow::Result<String> {
    let value = match upsert.fields.get(name) {
        Some(PimValue::Text(value)) => value.clone(),
        _ => anyhow::bail!("new {:?} requires {name}", upsert.resource_kind),
    };
    validate_compact_utc(&value, name)?;
    Ok(value)
}

fn escape_projection(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace(',', "\\,")
        .replace(';', "\\;")
}

#[derive(Clone, Debug)]
struct ProjectionLine {
    raw: String,
    head: String,
    value: String,
    property: String,
    component_id: Option<usize>,
    opened_component_id: Option<usize>,
}

#[derive(Clone, Debug)]
struct ProjectionComponent {
    name: String,
    parent: Option<usize>,
}

#[derive(Clone, Debug)]
struct ParsedProjection {
    lines: Vec<ProjectionLine>,
    components: Vec<ProjectionComponent>,
}

fn content_separator(line: &str) -> Option<usize> {
    let mut quoted = false;
    let mut escaped = false;
    for (index, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            ':' if !quoted => return Some(index),
            _ => {}
        }
    }
    None
}

fn property_name(head: &str) -> anyhow::Result<String> {
    let token = head
        .split(';')
        .next()
        .unwrap_or_default()
        .rsplit('.')
        .next()
        .unwrap_or_default();
    anyhow::ensure!(
        !token.is_empty()
            && token
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'),
        "invalid PIM content-line property name"
    );
    Ok(token.to_ascii_uppercase())
}

fn parse_projection(payload: &str) -> anyhow::Result<ParsedProjection> {
    anyhow::ensure!(!payload.contains('\0'), "PIM projection contains NUL");
    let normalized = payload.replace("\r\n", "\n");
    anyhow::ensure!(
        !normalized.contains('\r'),
        "PIM projection contains a bare carriage return"
    );
    let mut logical = Vec::<(String, String)>::new();
    for physical in normalized.split('\n') {
        if physical.is_empty() && logical.is_empty() {
            continue;
        }
        if physical.starts_with(' ') || physical.starts_with('\t') {
            let Some((raw, unfolded)) = logical.last_mut() else {
                anyhow::bail!("PIM projection starts with a folded continuation");
            };
            raw.push_str("\r\n");
            raw.push_str(physical);
            unfolded.push_str(&physical[1..]);
        } else if !physical.is_empty() {
            logical.push((physical.to_string(), physical.to_string()));
        }
    }
    anyhow::ensure!(!logical.is_empty(), "PIM projection is empty");

    let mut components = Vec::<ProjectionComponent>::new();
    let mut stack = Vec::<usize>::new();
    let mut lines = Vec::with_capacity(logical.len());
    for (raw, unfolded) in logical {
        let separator = content_separator(&unfolded)
            .ok_or_else(|| anyhow::anyhow!("PIM content line has no value separator"))?;
        let head = unfolded[..separator].to_string();
        let value = unfolded[separator + 1..].to_string();
        let property = property_name(&head)?;
        let component_id = stack.last().copied();
        let mut opened_component_id = None;
        if property == "BEGIN" {
            let name = value.trim().to_ascii_uppercase();
            anyhow::ensure!(
                !name.is_empty()
                    && name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'),
                "invalid PIM component name"
            );
            let id = components.len();
            components.push(ProjectionComponent {
                name,
                parent: component_id,
            });
            stack.push(id);
            opened_component_id = Some(id);
        } else if property == "END" {
            let Some(open_id) = stack.pop() else {
                anyhow::bail!("PIM projection has an unmatched END");
            };
            anyhow::ensure!(
                components[open_id].name.eq_ignore_ascii_case(value.trim()),
                "PIM projection closes the wrong component"
            );
        }
        lines.push(ProjectionLine {
            raw,
            head,
            value,
            property,
            component_id,
            opened_component_id,
        });
    }
    anyhow::ensure!(stack.is_empty(), "PIM projection has an unclosed component");
    Ok(ParsedProjection { lines, components })
}

impl ParsedProjection {
    fn component_ids(&self, name: &str) -> Vec<usize> {
        self.components
            .iter()
            .enumerate()
            .filter_map(|(id, component)| component.name.eq_ignore_ascii_case(name).then_some(id))
            .collect()
    }

    fn property_values(&self, component_id: usize, name: &str) -> Vec<&str> {
        self.lines
            .iter()
            .filter(|line| {
                line.component_id == Some(component_id) && line.property.eq_ignore_ascii_case(name)
            })
            .map(|line| line.value.as_str())
            .collect()
    }

    fn primary_component(&self, kind: PimResourceKind) -> anyhow::Result<usize> {
        let name = match kind {
            PimResourceKind::Contact => "VCARD",
            PimResourceKind::CalendarEvent => "VEVENT",
            PimResourceKind::Task => "VTODO",
        };
        let ids = self.component_ids(name);
        anyhow::ensure!(!ids.is_empty(), "PIM projection has no {name} component");
        if kind == PimResourceKind::Contact {
            anyhow::ensure!(ids.len() == 1, "vCard projection must contain one VCARD");
        }
        Ok(ids
            .iter()
            .copied()
            .find(|id| self.property_values(*id, "RECURRENCE-ID").is_empty())
            .unwrap_or(ids[0]))
    }
}

/// Validates one complete iCalendar/vCard projection and its primary kind.
pub fn validate_projection(kind: PimResourceKind, payload: &str) -> anyhow::Result<()> {
    let parsed = parse_projection(payload)?;
    let roots = parsed
        .components
        .iter()
        .enumerate()
        .filter_map(|(id, component)| component.parent.is_none().then_some((id, component)))
        .collect::<Vec<_>>();
    anyhow::ensure!(
        roots.len() == 1,
        "PIM projection must have one root component"
    );
    let expected_root = if kind == PimResourceKind::Contact {
        "VCARD"
    } else {
        "VCALENDAR"
    };
    anyhow::ensure!(
        roots[0].1.name == expected_root,
        "PIM projection has the wrong root component"
    );
    let root_versions = parsed.property_values(roots[0].0, "VERSION");
    anyhow::ensure!(
        root_versions.len() == 1,
        "PIM projection requires one VERSION"
    );
    if kind != PimResourceKind::Contact {
        anyhow::ensure!(
            root_versions[0].trim() == "2.0",
            "iCalendar VERSION must be 2.0"
        );
        let events = parsed.component_ids("VEVENT");
        let tasks = parsed.component_ids("VTODO");
        anyhow::ensure!(
            events.is_empty() || tasks.is_empty(),
            "one iCalendar resource cannot mix VEVENT and VTODO"
        );
    }
    let target_name = match kind {
        PimResourceKind::Contact => "VCARD",
        PimResourceKind::CalendarEvent => "VEVENT",
        PimResourceKind::Task => "VTODO",
    };
    let targets = parsed.component_ids(target_name);
    anyhow::ensure!(!targets.is_empty(), "PIM projection has no {target_name}");
    let mut uid = None::<String>;
    for component_id in targets {
        let values = parsed.property_values(component_id, "UID");
        anyhow::ensure!(values.len() == 1, "each PIM component requires one UID");
        let candidate = values[0].trim();
        anyhow::ensure!(!candidate.is_empty(), "PIM UID must not be empty");
        if let Some(uid) = uid.as_deref() {
            anyhow::ensure!(uid == candidate, "recurrence components must share one UID");
        } else {
            uid = Some(candidate.to_string());
        }
    }
    Ok(())
}

/// Validates an adapter upload whose calendar subtype is selected by content.
pub fn validate_dav_projection(is_contact: bool, payload: &str) -> anyhow::Result<PimResourceKind> {
    if is_contact {
        validate_projection(PimResourceKind::Contact, payload)?;
        return Ok(PimResourceKind::Contact);
    }
    let parsed = parse_projection(payload)?;
    let kind = if !parsed.component_ids("VEVENT").is_empty() {
        PimResourceKind::CalendarEvent
    } else if !parsed.component_ids("VTODO").is_empty() {
        PimResourceKind::Task
    } else {
        anyhow::bail!("iCalendar resource contains neither VEVENT nor VTODO");
    };
    validate_projection(kind, payload)?;
    Ok(kind)
}

/// Reads an unfolded primary-component property while accepting parameters and
/// grouped vCard property names.
pub fn projection_property(
    payload: &str,
    kind: PimResourceKind,
    name: &str,
) -> anyhow::Result<Option<String>> {
    let parsed = parse_projection(payload)?;
    let component = parsed.primary_component(kind)?;
    Ok(parsed
        .property_values(component, name)
        .first()
        .map(|value| (*value).to_string()))
}

/// Extracts Kamori's typed PIM fields from the primary iCalendar/vCard
/// component while keeping the complete source projection authoritative for
/// unknown properties.
pub fn projection_fields(
    payload: &str,
    kind: PimResourceKind,
) -> anyhow::Result<BTreeMap<String, PimValue>> {
    let parsed = parse_projection(payload)?;
    let component = parsed.primary_component(kind)?;
    let mut fields = BTreeMap::new();
    let title_property = if kind == PimResourceKind::Contact {
        "FN"
    } else {
        "SUMMARY"
    };
    if let Some(line) = parsed.property_lines(component, title_property).first() {
        fields.insert(
            "title".to_string(),
            PimValue::Text(unescape_projection_text(&line.value)),
        );
    }
    for (field, property) in [
        ("starts_at", "DTSTART"),
        ("ends_at", "DTEND"),
        ("due_at", "DUE"),
    ] {
        if let Some(line) = parsed.property_lines(component, property).first() {
            fields.insert(field.to_string(), temporal_from_projection(line)?);
        }
    }
    if kind == PimResourceKind::Task {
        fields.insert(
            "completed".to_string(),
            PimValue::Boolean(
                parsed
                    .property_values(component, "STATUS")
                    .first()
                    .is_some_and(|status| status.eq_ignore_ascii_case("COMPLETED")),
            ),
        );
        if let Some(value) = parsed.property_values(component, "COMPLETED").first() {
            fields.insert(
                "completed_at".to_string(),
                PimValue::Text((*value).to_string()),
            );
        }
        if let Some(value) = parsed.property_values(component, "PRIORITY").first()
            && let Ok(priority) = value.parse::<i64>()
        {
            fields.insert("priority".to_string(), PimValue::Integer(priority));
        }
    }
    let text_properties: &[(&str, &str)] = if kind == PimResourceKind::Contact {
        &[
            ("organization", "ORG"),
            ("job_title", "TITLE"),
            ("birthday", "BDAY"),
            ("url", "URL"),
            ("notes", "NOTE"),
        ]
    } else {
        &[
            ("location", "LOCATION"),
            ("notes", "DESCRIPTION"),
            ("recurrence_rule", "RRULE"),
        ]
    };
    for (field, property) in text_properties {
        if let Some(value) = parsed.property_values(component, property).first() {
            let value = if *property == "RRULE" {
                (*value).to_string()
            } else {
                unescape_projection_text(value)
            };
            fields.insert((*field).to_string(), PimValue::Text(value));
        }
    }
    if let Some(value) = parsed.property_values(component, "CATEGORIES").first() {
        fields.insert(
            "categories".to_string(),
            PimValue::TextList(split_projection_value(value, ',')),
        );
    }
    if kind == PimResourceKind::Contact {
        let emails = labeled_projection_records(&parsed, component, "EMAIL");
        let phones = labeled_projection_records(&parsed, component, "TEL");
        if let Some(first) = emails.first().and_then(|value| value.get("value")) {
            fields.insert("email".to_string(), PimValue::Text(first.clone()));
        }
        if let Some(first) = phones.first().and_then(|value| value.get("value")) {
            fields.insert("phone".to_string(), PimValue::Text(first.clone()));
        }
        if !emails.is_empty() {
            fields.insert("emails".to_string(), PimValue::Records(emails));
        }
        if !phones.is_empty() {
            fields.insert("phones".to_string(), PimValue::Records(phones));
        }
        if let Some(line) = parsed.property_lines(component, "N").first() {
            let values = split_projection_value(&line.value, ';');
            fields.insert(
                "name".to_string(),
                PimValue::Record(BTreeMap::from([
                    (
                        "family".to_string(),
                        values.first().cloned().unwrap_or_default(),
                    ),
                    (
                        "given".to_string(),
                        values.get(1).cloned().unwrap_or_default(),
                    ),
                    (
                        "middle".to_string(),
                        values.get(2).cloned().unwrap_or_default(),
                    ),
                    (
                        "prefix".to_string(),
                        values.get(3).cloned().unwrap_or_default(),
                    ),
                    (
                        "suffix".to_string(),
                        values.get(4).cloned().unwrap_or_default(),
                    ),
                ])),
            );
        }
        let addresses = parsed
            .property_lines(component, "ADR")
            .into_iter()
            .map(|line| {
                let values = split_projection_value(&line.value, ';');
                BTreeMap::from([
                    ("label".to_string(), projection_label(&line.head)),
                    ("raw_head".to_string(), line.head.clone()),
                    (
                        "po_box".to_string(),
                        values.first().cloned().unwrap_or_default(),
                    ),
                    (
                        "extended".to_string(),
                        values.get(1).cloned().unwrap_or_default(),
                    ),
                    (
                        "street".to_string(),
                        values.get(2).cloned().unwrap_or_default(),
                    ),
                    (
                        "locality".to_string(),
                        values.get(3).cloned().unwrap_or_default(),
                    ),
                    (
                        "region".to_string(),
                        values.get(4).cloned().unwrap_or_default(),
                    ),
                    (
                        "postal_code".to_string(),
                        values.get(5).cloned().unwrap_or_default(),
                    ),
                    (
                        "country".to_string(),
                        values.get(6).cloned().unwrap_or_default(),
                    ),
                ])
            })
            .collect::<Vec<_>>();
        if !addresses.is_empty() {
            fields.insert("addresses".to_string(), PimValue::Records(addresses));
        }
        fields.insert(
            "favorite".to_string(),
            PimValue::Boolean(
                parsed
                    .property_values(component, "X-KAMORI-FAVORITE")
                    .first()
                    .is_some_and(|value| value.eq_ignore_ascii_case("TRUE")),
            ),
        );
    }
    if let Some(minutes) = managed_alarm_minutes(&parsed, component) {
        fields.insert("reminder_minutes".to_string(), PimValue::Integer(minutes));
    }
    Ok(fields)
}

impl ParsedProjection {
    fn property_lines(&self, component_id: usize, name: &str) -> Vec<&ProjectionLine> {
        self.lines
            .iter()
            .filter(|line| {
                line.component_id == Some(component_id) && line.property.eq_ignore_ascii_case(name)
            })
            .collect()
    }
}

fn projection_parameter(head: &str, name: &str) -> Option<String> {
    split_projection_head(head)
        .into_iter()
        .skip(1)
        .find_map(|parameter| {
            let (parameter_name, value) = parameter.split_once('=')?;
            parameter_name.eq_ignore_ascii_case(name).then(|| {
                value
                    .trim_matches('"')
                    .replace("\\\"", "\"")
                    .replace("\\\\", "\\")
            })
        })
}

fn split_projection_head(head: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    let mut escaped = false;

    for (index, character) in head.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' && quoted {
            escaped = true;
            continue;
        }
        if character == '"' {
            quoted = !quoted;
        } else if character == ';' && !quoted {
            parts.push(&head[start..index]);
            start = index + character.len_utf8();
        }
    }
    parts.push(&head[start..]);
    parts
}

fn projection_label(head: &str) -> String {
    projection_parameter(head, "X-KAMORI-LABEL")
        .or_else(|| {
            projection_parameter(head, "TYPE")
                .and_then(|value| value.split(',').next().map(str::to_ascii_lowercase))
        })
        .unwrap_or_default()
}

fn labeled_projection_records(
    parsed: &ParsedProjection,
    component: usize,
    property: &str,
) -> Vec<BTreeMap<String, String>> {
    parsed
        .property_lines(component, property)
        .into_iter()
        .map(|line| {
            BTreeMap::from([
                ("label".to_string(), projection_label(&line.head)),
                ("value".to_string(), unescape_projection_text(&line.value)),
                ("raw_head".to_string(), line.head.clone()),
            ])
        })
        .collect()
}

fn split_projection_value(value: &str, separator: char) -> Vec<String> {
    let mut values = Vec::new();
    let mut current = String::new();
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        if character == '\\' {
            match characters.next() {
                Some('n' | 'N') => current.push('\n'),
                Some(next) => current.push(next),
                None => current.push('\\'),
            }
        } else if character == separator {
            values.push(std::mem::take(&mut current));
        } else {
            current.push(character);
        }
    }
    values.push(current);
    values
}

fn temporal_from_projection(line: &ProjectionLine) -> anyhow::Result<PimValue> {
    if projection_parameter(&line.head, "VALUE")
        .is_some_and(|value| value.eq_ignore_ascii_case("DATE"))
    {
        anyhow::ensure!(line.value.len() == 8, "invalid all-day projection value");
        return Ok(PimValue::Record(BTreeMap::from([
            ("kind".to_string(), "date".to_string()),
            (
                "date".to_string(),
                format!(
                    "{}-{}-{}",
                    &line.value[0..4],
                    &line.value[4..6],
                    &line.value[6..8]
                ),
            ),
        ])));
    }
    if let Some(timezone) = projection_parameter(&line.head, "TZID") {
        return Ok(PimValue::Record(BTreeMap::from([
            ("kind".to_string(), "zoned_datetime".to_string()),
            ("local".to_string(), line.value.clone()),
            ("timezone".to_string(), timezone),
        ])));
    }
    Ok(PimValue::Record(BTreeMap::from([
        ("kind".to_string(), "utc".to_string()),
        ("utc".to_string(), line.value.clone()),
    ])))
}

fn managed_alarm_minutes(parsed: &ParsedProjection, component: usize) -> Option<i64> {
    parsed
        .components
        .iter()
        .enumerate()
        .filter(|(_, candidate)| candidate.parent == Some(component) && candidate.name == "VALARM")
        .find_map(|(alarm_id, _)| {
            let managed = parsed
                .property_values(alarm_id, "X-KAMORI-MANAGED")
                .first()
                .is_some_and(|value| value.eq_ignore_ascii_case("TRUE"));
            if !managed {
                return None;
            }
            parsed
                .property_values(alarm_id, "TRIGGER")
                .first()
                .and_then(|value| value.strip_prefix("-PT"))
                .and_then(|value| value.strip_suffix('M'))
                .and_then(|value| value.parse().ok())
        })
}

/// Decodes RFC text escaping without treating parameters as part of the value.
pub fn unescape_projection_text(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }
        match chars.next() {
            Some('n' | 'N') => output.push('\n'),
            Some(next @ ('\\' | ',' | ';')) => output.push(next),
            Some(next) => {
                output.push('\\');
                output.push(next);
            }
            None => output.push('\\'),
        }
    }
    output
}

fn fold_content_line(head: &str, value: &str) -> String {
    let content = format!("{head}:{value}");
    let mut output = String::with_capacity(content.len());
    let mut width = 0usize;
    for ch in content.chars() {
        let bytes = ch.len_utf8();
        if width > 0 && width.saturating_add(bytes) > 75 {
            output.push_str("\r\n ");
            width = 1;
        }
        output.push(ch);
        width = width.saturating_add(bytes);
    }
    output
}

fn patch_projection(existing: &str, upsert: &PimUpsertV1) -> anyhow::Result<String> {
    let replacements = projection_replacements(upsert)?;
    let legacy_single_edits = if upsert.resource_kind == PimResourceKind::Contact {
        [("email", "EMAIL"), ("phone", "TEL")]
            .into_iter()
            .filter_map(|(field, property)| {
                (!upsert.fields.contains_key(&format!("{field}s")))
                    .then(|| upsert.fields.get(field))
                    .flatten()
                    .map(|value| (property.to_string(), value.clone()))
            })
            .collect::<BTreeMap<_, _>>()
    } else {
        BTreeMap::new()
    };
    let alarm_changed = upsert.fields.contains_key("reminder_minutes");
    if replacements.is_empty() && legacy_single_edits.is_empty() && !alarm_changed {
        validate_projection(upsert.resource_kind, existing)?;
        return Ok(existing.to_string());
    }

    let parsed = parse_projection(existing)?;
    let primary_component = parsed.primary_component(upsert.resource_kind)?;
    let managed_alarm_ids = parsed
        .components
        .iter()
        .enumerate()
        .filter_map(|(id, component)| {
            (component.parent == Some(primary_component)
                && component.name == "VALARM"
                && parsed
                    .property_values(id, "X-KAMORI-MANAGED")
                    .iter()
                    .any(|value| value.eq_ignore_ascii_case("TRUE")))
            .then_some(id)
        })
        .collect::<HashSet<_>>();

    let mut output_lines = Vec::with_capacity(parsed.lines.len() + replacements.len() * 2 + 6);
    let mut legacy_applied = HashSet::new();
    let primary_name = parsed.components[primary_component].name.as_str();
    for line in parsed.lines {
        if alarm_changed
            && (line
                .component_id
                .is_some_and(|id| managed_alarm_ids.contains(&id))
                || line
                    .opened_component_id
                    .is_some_and(|id| managed_alarm_ids.contains(&id)))
        {
            continue;
        }
        let primary_end = line.component_id == Some(primary_component)
            && line.property == "END"
            && line.value.eq_ignore_ascii_case(primary_name);
        if primary_end {
            for values in replacements.values() {
                output_lines.extend(values.iter().cloned());
            }
            if alarm_changed {
                append_managed_alarm(
                    &mut output_lines,
                    upsert,
                    upsert.resource_kind == PimResourceKind::Task,
                );
            }
            for (property, value) in &legacy_single_edits {
                if !legacy_applied.contains(property)
                    && let PimValue::Text(value) = value
                    && !value.is_empty()
                {
                    output_lines.push(fold_content_line(property, &escape_projection(value)));
                }
            }
        }
        if line.component_id == Some(primary_component)
            && replacements.contains_key(line.property.as_str())
        {
            continue;
        }
        if line.component_id == Some(primary_component)
            && let Some(value) = legacy_single_edits.get(line.property.as_str())
            && legacy_applied.insert(line.property.clone())
        {
            if let PimValue::Text(value) = value
                && !value.is_empty()
            {
                output_lines.push(fold_content_line(&line.head, &escape_projection(value)));
            }
            continue;
        }
        output_lines.push(line.raw);
    }
    let mut output = output_lines.join("\r\n");
    output.push_str("\r\n");
    validate_projection(upsert.resource_kind, &output)?;
    Ok(output)
}

fn projection_replacements(upsert: &PimUpsertV1) -> anyhow::Result<BTreeMap<String, Vec<String>>> {
    let mut replacements = BTreeMap::<String, Vec<String>>::new();
    let title_property = if upsert.resource_kind == PimResourceKind::Contact {
        "FN"
    } else {
        "SUMMARY"
    };
    replace_text_field(&mut replacements, upsert, "title", title_property);

    match upsert.resource_kind {
        PimResourceKind::Contact => {
            replace_record_field(
                &mut replacements,
                upsert,
                "name",
                "N",
                render_structured_name,
            );
            replace_contact_values(&mut replacements, upsert, "emails", "EMAIL");
            replace_contact_values(&mut replacements, upsert, "phones", "TEL");
            if upsert.fields.contains_key("addresses") {
                let mut lines = Vec::new();
                append_addresses(&mut lines, upsert);
                replacements.insert("ADR".to_string(), lines);
            }
            replace_text_field(&mut replacements, upsert, "organization", "ORG");
            replace_text_field(&mut replacements, upsert, "job_title", "TITLE");
            replace_raw_field(&mut replacements, upsert, "birthday", "BDAY");
            replace_raw_field(&mut replacements, upsert, "url", "URL");
            replace_text_field(&mut replacements, upsert, "notes", "NOTE");
            if let Some(value) = upsert.fields.get("favorite") {
                let lines = match value {
                    PimValue::Boolean(true) => vec!["X-KAMORI-FAVORITE:TRUE".to_string()],
                    PimValue::Boolean(false) | PimValue::Null => Vec::new(),
                    _ => anyhow::bail!("favorite must be boolean"),
                };
                replacements.insert("X-KAMORI-FAVORITE".to_string(), lines);
            }
        }
        PimResourceKind::CalendarEvent => {
            replace_temporal_field(&mut replacements, upsert, "starts_at", "DTSTART")?;
            replace_temporal_field(&mut replacements, upsert, "ends_at", "DTEND")?;
            replace_text_field(&mut replacements, upsert, "location", "LOCATION");
            replace_text_field(&mut replacements, upsert, "notes", "DESCRIPTION");
            replace_raw_field(&mut replacements, upsert, "recurrence_rule", "RRULE");
        }
        PimResourceKind::Task => {
            replace_temporal_field(&mut replacements, upsert, "starts_at", "DTSTART")?;
            replace_temporal_field(&mut replacements, upsert, "due_at", "DUE")?;
            replace_raw_field(&mut replacements, upsert, "completed_at", "COMPLETED");
            replace_integer_field(&mut replacements, upsert, "priority", "PRIORITY")?;
            replace_text_field(&mut replacements, upsert, "notes", "DESCRIPTION");
            replace_raw_field(&mut replacements, upsert, "recurrence_rule", "RRULE");
            if let Some(value) = upsert.fields.get("completed") {
                let lines = match value {
                    PimValue::Boolean(completed) => vec![format!(
                        "STATUS:{}",
                        if *completed {
                            "COMPLETED"
                        } else {
                            "NEEDS-ACTION"
                        }
                    )],
                    PimValue::Null => Vec::new(),
                    _ => anyhow::bail!("completed must be boolean"),
                };
                replacements.insert("STATUS".to_string(), lines);
            }
        }
    }
    if upsert.fields.contains_key("categories") {
        let mut lines = Vec::new();
        append_categories(&mut lines, upsert);
        replacements.insert("CATEGORIES".to_string(), lines);
    }
    if let Some(PimValue::Text(value)) = upsert.fields.get("dtstamp") {
        replacements.insert("DTSTAMP".to_string(), vec![format!("DTSTAMP:{value}")]);
    }
    Ok(replacements)
}

fn replace_text_field(
    replacements: &mut BTreeMap<String, Vec<String>>,
    upsert: &PimUpsertV1,
    field: &str,
    property: &str,
) {
    if let Some(value) = upsert.fields.get(field) {
        let lines = match value {
            PimValue::Text(value) if !value.is_empty() => {
                vec![fold_content_line(property, &escape_projection(value))]
            }
            _ => Vec::new(),
        };
        replacements.insert(property.to_string(), lines);
    }
}

fn replace_raw_field(
    replacements: &mut BTreeMap<String, Vec<String>>,
    upsert: &PimUpsertV1,
    field: &str,
    property: &str,
) {
    if let Some(value) = upsert.fields.get(field) {
        let lines = match value {
            PimValue::Text(value) if !value.is_empty() => vec![fold_content_line(property, value)],
            _ => Vec::new(),
        };
        replacements.insert(property.to_string(), lines);
    }
}

fn replace_integer_field(
    replacements: &mut BTreeMap<String, Vec<String>>,
    upsert: &PimUpsertV1,
    field: &str,
    property: &str,
) -> anyhow::Result<()> {
    if let Some(value) = upsert.fields.get(field) {
        let lines = match value {
            PimValue::Integer(value) => vec![format!("{property}:{value}")],
            PimValue::Null => Vec::new(),
            _ => anyhow::bail!("{field} must be an integer"),
        };
        replacements.insert(property.to_string(), lines);
    }
    Ok(())
}

fn replace_temporal_field(
    replacements: &mut BTreeMap<String, Vec<String>>,
    upsert: &PimUpsertV1,
    field: &str,
    property: &str,
) -> anyhow::Result<()> {
    if upsert.fields.contains_key(field) {
        let lines = optional_temporal_line(upsert, field, property)?
            .into_iter()
            .collect();
        replacements.insert(property.to_string(), lines);
    }
    Ok(())
}

fn replace_record_field(
    replacements: &mut BTreeMap<String, Vec<String>>,
    upsert: &PimUpsertV1,
    field: &str,
    property: &str,
    render: fn(&BTreeMap<String, String>) -> String,
) {
    if let Some(value) = upsert.fields.get(field) {
        let lines = match value {
            PimValue::Record(value) => vec![render(value)],
            _ => Vec::new(),
        };
        replacements.insert(property.to_string(), lines);
    }
}

fn replace_contact_values(
    replacements: &mut BTreeMap<String, Vec<String>>,
    upsert: &PimUpsertV1,
    plural_field: &str,
    property: &str,
) {
    if upsert.fields.contains_key(plural_field) {
        let mut lines = Vec::new();
        append_contact_values(&mut lines, upsert, plural_field, "", property);
        replacements.insert(property.to_string(), lines);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_assignment_is_arrival_order_independent() {
        let root = Uuid::from_u128(10);
        let lower_child = Uuid::from_u128(20);
        let higher_child = Uuid::from_u128(30);
        let grandchild = Uuid::from_u128(40);
        let nodes = vec![
            PimBranchNodeV1 {
                operation_id: root,
                parent_operation_id: None,
                seed_projection_resource_id: None,
            },
            PimBranchNodeV1 {
                operation_id: higher_child,
                parent_operation_id: Some(root),
                seed_projection_resource_id: None,
            },
            PimBranchNodeV1 {
                operation_id: grandchild,
                parent_operation_id: Some(higher_child),
                seed_projection_resource_id: None,
            },
            PimBranchNodeV1 {
                operation_id: lower_child,
                parent_operation_id: Some(root),
                seed_projection_resource_id: None,
            },
        ];
        let mut reversed = nodes.clone();
        reversed.reverse();

        let expected = assign_pim_branches("event.ics", &nodes).expect("assign branches");
        assert_eq!(
            assign_pim_branches("event.ics", &reversed).expect("assign reversed branches"),
            expected
        );
        let assignments = expected
            .into_iter()
            .map(|assignment| (assignment.operation_id, assignment))
            .collect::<HashMap<_, _>>();
        assert_eq!(
            assignments[&lower_child].projection_resource_id,
            "event.ics"
        );
        assert_eq!(
            assignments[&higher_child].projection_resource_id,
            format!("event.conflict-{higher_child}.ics")
        );
        assert_eq!(
            assignments[&grandchild].projection_resource_id,
            assignments[&higher_child].projection_resource_id
        );
        assert!(assignments[&lower_child].head);
        assert!(assignments[&grandchild].head);
        assert!(!assignments[&root].head);
    }

    #[test]
    fn branch_assignment_rejects_incomplete_or_cyclic_graphs() {
        let missing = vec![PimBranchNodeV1 {
            operation_id: Uuid::from_u128(2),
            parent_operation_id: Some(Uuid::from_u128(1)),
            seed_projection_resource_id: None,
        }];
        assert!(assign_pim_branches("contact.vcf", &missing).is_err());

        let cycle = vec![
            PimBranchNodeV1 {
                operation_id: Uuid::from_u128(1),
                parent_operation_id: Some(Uuid::from_u128(2)),
                seed_projection_resource_id: None,
            },
            PimBranchNodeV1 {
                operation_id: Uuid::from_u128(2),
                parent_operation_id: Some(Uuid::from_u128(1)),
                seed_projection_resource_id: None,
            },
        ];
        assert!(assign_pim_branches("contact.vcf", &cycle).is_err());
    }

    #[test]
    fn field_operation_msgpack_roundtrip_is_stable() {
        let operation = PimOperationV1::Upsert(PimUpsertV1 {
            schema_version: 1,
            resource_kind: PimResourceKind::Task,
            resource_id: Uuid::from_u128(7),
            dependencies: vec![Uuid::from_u128(5)],
            fields: BTreeMap::from([
                ("completed".to_string(), PimValue::Boolean(false)),
                ("title".to_string(), PimValue::Text("Ship MVP".to_string())),
            ]),
            raw_projection: Vec::new(),
        });
        let encoded = operation.encode().expect("encode");
        assert_eq!(PimOperationV1::decode(&encoded).expect("decode"), operation);
        assert_eq!(operation.encode().expect("encode twice"), encoded);
    }

    #[test]
    fn rejects_non_utf8_raw_projection() {
        let operation = PimOperationV1::Upsert(PimUpsertV1 {
            schema_version: 1,
            resource_kind: PimResourceKind::Contact,
            resource_id: Uuid::from_u128(7),
            dependencies: vec![],
            fields: BTreeMap::new(),
            raw_projection: vec![0xff],
        });

        assert!(operation.validate().is_err());
    }

    #[test]
    fn delete_preserves_adapter_resource_identity() {
        let operation = PimOperationV1::Delete(PimDeleteV1 {
            schema_version: 1,
            resource_kind: PimResourceKind::Contact,
            resource_id: Uuid::from_u128(9),
            dependencies: vec![Uuid::from_u128(8)],
            projection_resource_id: Some("client-generated-name.vcf".to_string()),
        });
        let encoded = operation.encode().expect("encode");
        assert_eq!(PimOperationV1::decode(&encoded).expect("decode"), operation);
    }

    #[test]
    fn snapshot_msgpack_roundtrip_is_stable() {
        let snapshot = PimSnapshotV2 {
            schema_version: PimSnapshotV2::SCHEMA_VERSION,
            covers_through_space_seq: 42,
            resource_kind: PimResourceKind::CalendarEvent,
            resource_id: Uuid::from_u128(5),
            branches: vec![PimSnapshotBranchV2 {
                projection_resource_id: "event.ics".to_string(),
                head_operation_id: Uuid::from_u128(4),
                deleted: false,
                materialized_projection: b"BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n".to_vec(),
            }],
        };
        let encoded = snapshot.encode().expect("encode snapshot");
        assert_eq!(PimSnapshotV2::decode(&encoded).expect("decode"), snapshot);
    }

    #[test]
    fn snapshot_preserves_all_conflict_branches() {
        let snapshot = PimSnapshotV2 {
            schema_version: PimSnapshotV2::SCHEMA_VERSION,
            covers_through_space_seq: 42,
            resource_kind: PimResourceKind::Contact,
            resource_id: Uuid::from_u128(5),
            branches: vec![
                PimSnapshotBranchV2 {
                    projection_resource_id: "contact.vcf".to_string(),
                    head_operation_id: Uuid::from_u128(4),
                    deleted: false,
                    materialized_projection: b"BEGIN:VCARD\r\nEND:VCARD\r\n".to_vec(),
                },
                PimSnapshotBranchV2 {
                    projection_resource_id: "contact.conflict.vcf".to_string(),
                    head_operation_id: Uuid::from_u128(6),
                    deleted: false,
                    materialized_projection: b"BEGIN:VCARD\r\nFN:Conflict\r\nEND:VCARD\r\n"
                        .to_vec(),
                },
            ],
        };

        let decoded = PimSnapshotV2::decode(&snapshot.encode().expect("encode"))
            .expect("decode multi-branch snapshot");
        assert_eq!(decoded.branches, snapshot.branches);
    }

    #[test]
    fn snapshot_rejects_duplicate_branch_identity() {
        let branch = PimSnapshotBranchV2 {
            projection_resource_id: "contact.vcf".to_string(),
            head_operation_id: Uuid::from_u128(4),
            deleted: true,
            materialized_projection: Vec::new(),
        };
        let snapshot = PimSnapshotV2 {
            schema_version: PimSnapshotV2::SCHEMA_VERSION,
            covers_through_space_seq: 42,
            resource_kind: PimResourceKind::Contact,
            resource_id: Uuid::from_u128(5),
            branches: vec![branch.clone(), branch],
        };

        assert!(snapshot.encode().is_err());
    }

    #[test]
    fn rejects_multi_parent_v1_operation() {
        let operation = PimOperationV1::Delete(PimDeleteV1 {
            schema_version: 1,
            resource_kind: PimResourceKind::Task,
            resource_id: Uuid::from_u128(9),
            dependencies: vec![Uuid::from_u128(7), Uuid::from_u128(8)],
            projection_resource_id: None,
        });
        assert!(operation.encode().is_err());
        let encoded = rmp_serde::to_vec_named(&operation).expect("serialize invalid fixture");

        let error = PimOperationV1::decode(&encoded).expect_err("reject multiple parents");
        assert!(error.to_string().contains("at most one parent"));
    }

    #[test]
    fn contact_edit_preserves_parameters_groups_folding_and_unknown_properties() {
        let existing = concat!(
            "BEGIN:VCARD\r\n",
            "VERSION:4.0\r\n",
            "UID:contact-1\r\n",
            "FN:Alice Example\r\n",
            "item1.EMAIL;TYPE=work;PREF=1:old@example.com\r\n",
            "NOTE:This unknown field is folded and must remain\r\n",
            " exactly as imported\r\n",
            "X-APP-CUSTOM;FLAG=YES:opaque\r\n",
            "END:VCARD\r\n",
        );
        let upsert = PimUpsertV1 {
            schema_version: 1,
            resource_kind: PimResourceKind::Contact,
            resource_id: Uuid::from_u128(1),
            dependencies: vec![],
            fields: BTreeMap::from([(
                "email".to_string(),
                PimValue::Text("new@example.com".to_string()),
            )]),
            raw_projection: Vec::new(),
        };

        let patched = materialize_projection(&upsert, Some(existing)).expect("materialize");
        assert!(patched.contains("item1.EMAIL;TYPE=work;PREF=1:new@example.com\r\n"));
        assert!(patched.contains(
            "NOTE:This unknown field is folded and must remain\r\n exactly as imported\r\n"
        ));
        assert!(patched.contains("X-APP-CUSTOM;FLAG=YES:opaque\r\n"));
        validate_projection(PimResourceKind::Contact, &patched).expect("valid patched vCard");
    }

    #[test]
    fn event_edit_changes_master_without_overwriting_recurrence_exception() {
        let existing = concat!(
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//EN\r\n",
            "BEGIN:VEVENT\r\nUID:event-1\r\nSUMMARY:Master\r\nEND:VEVENT\r\n",
            "BEGIN:VEVENT\r\nUID:event-1\r\nRECURRENCE-ID:20260823T120000Z\r\n",
            "SUMMARY:Exception\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        );
        let upsert = PimUpsertV1 {
            schema_version: 1,
            resource_kind: PimResourceKind::CalendarEvent,
            resource_id: Uuid::from_u128(2),
            dependencies: vec![],
            fields: BTreeMap::from([(
                "title".to_string(),
                PimValue::Text("Updated master".to_string()),
            )]),
            raw_projection: Vec::new(),
        };

        let patched = materialize_projection(&upsert, Some(existing)).expect("materialize");
        assert!(patched.contains("SUMMARY:Updated master"));
        assert!(patched.contains("SUMMARY:Exception"));
        validate_projection(PimResourceKind::CalendarEvent, &patched)
            .expect("valid recurrence resource");
    }

    #[test]
    fn property_reader_unfolds_and_accepts_parameters() {
        let card = concat!(
            "BEGIN:VCARD\r\nVERSION:4.0\r\nUID:contact-1\r\n",
            "FN:Alice Ex\r\n ample\r\n",
            "EMAIL;TYPE=work:alice@example.com\r\nEND:VCARD\r\n",
        );
        assert_eq!(
            projection_property(card, PimResourceKind::Contact, "FN").unwrap(),
            Some("Alice Example".to_string())
        );
        assert_eq!(
            projection_property(card, PimResourceKind::Contact, "EMAIL").unwrap(),
            Some("alice@example.com".to_string())
        );
    }

    #[test]
    fn validator_rejects_mismatched_or_missing_uids() {
        let mixed = concat!(
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\n",
            "BEGIN:VEVENT\r\nUID:first\r\nEND:VEVENT\r\n",
            "BEGIN:VEVENT\r\nUID:second\r\nEND:VEVENT\r\n",
            "END:VCALENDAR\r\n",
        );
        assert!(validate_projection(PimResourceKind::CalendarEvent, mixed).is_err());
        assert!(
            validate_projection(
                PimResourceKind::Contact,
                "BEGIN:VCARD\r\nVERSION:4.0\r\nFN:No UID\r\nEND:VCARD\r\n",
            )
            .is_err()
        );
    }

    #[test]
    fn rich_event_roundtrips_typed_time_recurrence_and_managed_alarm() {
        let temporal = BTreeMap::from([
            ("kind".to_string(), "zoned_datetime".to_string()),
            ("local".to_string(), "20260828T183000".to_string()),
            ("timezone".to_string(), "Asia/Tbilisi".to_string()),
            ("utc".to_string(), "20260828T143000Z".to_string()),
        ]);
        let ending = BTreeMap::from([
            ("kind".to_string(), "zoned_datetime".to_string()),
            ("local".to_string(), "20260828T193000".to_string()),
            ("timezone".to_string(), "Asia/Tbilisi".to_string()),
            ("utc".to_string(), "20260828T153000Z".to_string()),
        ]);
        let upsert = PimUpsertV1 {
            schema_version: CURRENT_PIM_SCHEMA_VERSION,
            resource_kind: PimResourceKind::CalendarEvent,
            resource_id: Uuid::from_u128(42),
            dependencies: Vec::new(),
            fields: BTreeMap::from([
                (
                    "title".to_string(),
                    PimValue::Text("Release review".to_string()),
                ),
                (
                    "dtstamp".to_string(),
                    PimValue::Text("20260828T120000Z".to_string()),
                ),
                ("starts_at".to_string(), PimValue::Record(temporal)),
                ("ends_at".to_string(), PimValue::Record(ending)),
                (
                    "recurrence_rule".to_string(),
                    PimValue::Text("FREQ=WEEKLY;BYDAY=FR".to_string()),
                ),
                ("reminder_minutes".to_string(), PimValue::Integer(15)),
                (
                    "categories".to_string(),
                    PimValue::TextList(vec!["Work".to_string(), "Release".to_string()]),
                ),
            ]),
            raw_projection: Vec::new(),
        };

        PimOperationV1::Upsert(upsert.clone())
            .validate()
            .expect("valid rich event");
        let projection = materialize_projection(&upsert, None).expect("materialize event");
        assert!(projection.contains("DTSTART;TZID=Asia/Tbilisi:20260828T183000"));
        assert!(projection.contains("RRULE:FREQ=WEEKLY;BYDAY=FR"));
        assert!(projection.contains("TRIGGER:-PT15M"));
        let fields = projection_fields(&projection, PimResourceKind::CalendarEvent)
            .expect("extract rich fields");
        assert_eq!(
            fields.get("recurrence_rule"),
            Some(&PimValue::Text("FREQ=WEEKLY;BYDAY=FR".to_string()))
        );
        assert_eq!(fields.get("reminder_minutes"), Some(&PimValue::Integer(15)));
    }

    #[test]
    fn rich_contact_replaces_managed_values_but_preserves_unknown_properties() {
        let existing = concat!(
            "BEGIN:VCARD\r\nVERSION:4.0\r\nUID:contact-1\r\n",
            "FN:Alice Example\r\nEMAIL;TYPE=home:old@example.com\r\n",
            "X-VENDOR-IDENTITY:keep-me\r\nEND:VCARD\r\n",
        );
        let upsert = PimUpsertV1 {
            schema_version: CURRENT_PIM_SCHEMA_VERSION,
            resource_kind: PimResourceKind::Contact,
            resource_id: Uuid::from_u128(1),
            dependencies: Vec::new(),
            fields: BTreeMap::from([
                (
                    "emails".to_string(),
                    PimValue::Records(vec![
                        BTreeMap::from([
                            ("label".to_string(), "work".to_string()),
                            ("value".to_string(), "alice@work.example".to_string()),
                        ]),
                        BTreeMap::from([
                            ("label".to_string(), "personal;secure".to_string()),
                            ("value".to_string(), "alice@example.com".to_string()),
                        ]),
                    ]),
                ),
                (
                    "organization".to_string(),
                    PimValue::Text("Kamori".to_string()),
                ),
            ]),
            raw_projection: Vec::new(),
        };

        PimOperationV1::Upsert(upsert.clone())
            .validate()
            .expect("valid rich contact");
        let projection = materialize_projection(&upsert, Some(existing)).expect("patch contact");
        assert!(projection.contains("EMAIL;TYPE=WORK:alice@work.example"));
        assert!(projection.contains("EMAIL;X-KAMORI-LABEL=\"personal;secure\":alice@example.com"));
        assert!(projection.contains("X-VENDOR-IDENTITY:keep-me"));
        let fields =
            projection_fields(&projection, PimResourceKind::Contact).expect("extract rich contact");
        assert!(
            matches!(fields.get("emails"), Some(PimValue::Records(values)) if values.len() == 2)
        );
        assert!(matches!(
            fields.get("emails"),
            Some(PimValue::Records(values))
                if values[1].get("label") == Some(&"personal;secure".to_string())
        ));
    }

    #[test]
    fn contact_edit_preserves_grouped_parameters_and_address_components() {
        let existing = concat!(
            "BEGIN:VCARD\r\nVERSION:4.0\r\nUID:contact-1\r\n",
            "FN:Alice Example\r\nitem1.EMAIL;TYPE=work;PREF=1:old@example.com\r\n",
            "ADR;TYPE=home:PO Box 1;Floor 2;1 Rustaveli Ave;Tbilisi;;0108;Georgia\r\n",
            "END:VCARD\r\n",
        );
        let extracted = projection_fields(existing, PimResourceKind::Contact)
            .expect("extract lossless contact fields");
        let mut emails = match extracted.get("emails").cloned() {
            Some(PimValue::Records(values)) => values,
            _ => panic!("missing emails"),
        };
        emails[0].insert("value".to_string(), "new@example.com".to_string());
        let upsert = PimUpsertV1 {
            schema_version: CURRENT_PIM_SCHEMA_VERSION,
            resource_kind: PimResourceKind::Contact,
            resource_id: Uuid::from_u128(1),
            dependencies: Vec::new(),
            fields: BTreeMap::from([
                ("emails".to_string(), PimValue::Records(emails)),
                (
                    "addresses".to_string(),
                    extracted.get("addresses").cloned().expect("addresses"),
                ),
            ]),
            raw_projection: Vec::new(),
        };

        let projection = materialize_projection(&upsert, Some(existing)).expect("patch contact");
        assert!(projection.contains("item1.EMAIL;TYPE=work;PREF=1:new@example.com"));
        assert!(
            projection
                .contains("ADR;TYPE=home:PO Box 1;Floor 2;1 Rustaveli Ave;Tbilisi;;0108;Georgia")
        );
    }

    #[test]
    fn task_reminders_are_relative_to_due_time() {
        let upsert = PimUpsertV1 {
            schema_version: CURRENT_PIM_SCHEMA_VERSION,
            resource_kind: PimResourceKind::Task,
            resource_id: Uuid::from_u128(7),
            dependencies: Vec::new(),
            fields: BTreeMap::from([
                ("title".to_string(), PimValue::Text("Ship beta".to_string())),
                (
                    "dtstamp".to_string(),
                    PimValue::Text("20260828T120000Z".to_string()),
                ),
                (
                    "due_at".to_string(),
                    PimValue::Record(BTreeMap::from([
                        ("kind".to_string(), "utc".to_string()),
                        ("utc".to_string(), "20260828T140000Z".to_string()),
                    ])),
                ),
                ("completed".to_string(), PimValue::Boolean(false)),
                ("reminder_minutes".to_string(), PimValue::Integer(15)),
            ]),
            raw_projection: Vec::new(),
        };

        let projection = materialize_projection(&upsert, None).expect("materialize task");
        assert!(projection.contains("TRIGGER;RELATED=END:-PT15M"));
    }

    #[test]
    fn v2_validation_rejects_injection_and_impossible_dates() {
        let operation = |field: &str, value: PimValue| {
            PimOperationV1::Upsert(PimUpsertV1 {
                schema_version: CURRENT_PIM_SCHEMA_VERSION,
                resource_kind: PimResourceKind::Contact,
                resource_id: Uuid::from_u128(1),
                dependencies: Vec::new(),
                fields: BTreeMap::from([
                    ("title".to_string(), PimValue::Text("Alice".to_string())),
                    (field.to_string(), value),
                ]),
                raw_projection: Vec::new(),
            })
        };
        assert!(
            operation(
                "url",
                PimValue::Text("https://example.com\r\nX-INJECTED:true".to_string())
            )
            .validate()
            .is_err()
        );
        assert!(
            operation("birthday", PimValue::Text("2026-02-29".to_string()))
                .validate()
                .is_err()
        );
    }
}
