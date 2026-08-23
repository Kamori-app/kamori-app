//! Versioned PIM operation codec independent from DAV and transport ordering.

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
    #[serde(with = "serde_bytes")]
    Bytes(Vec<u8>),
    Null,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PimUpsertV1 {
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
            for field in ["dtstamp", "starts_at", "ends_at"] {
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
        Ok(())
    }
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
        Ok(rmp_serde::to_vec_named(self)?)
    }

    pub fn decode(bytes: &[u8]) -> anyhow::Result<Self> {
        let snapshot: Self = rmp_serde::from_slice(bytes)?;
        anyhow::ensure!(
            snapshot.schema_version == Self::SCHEMA_VERSION,
            "unsupported PIM snapshot schema version"
        );
        anyhow::ensure!(
            !snapshot.resource_id.is_nil(),
            "snapshot resource id must be non-nil"
        );
        anyhow::ensure!(!snapshot.branches.is_empty(), "snapshot has no branches");
        let mut projection_ids = HashSet::new();
        let mut head_ids = HashSet::new();
        for branch in &snapshot.branches {
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
        Ok(snapshot)
    }
}

/// Materializes a lossless DAV projection from a field operation. Existing
/// unknown properties are retained when a first-party edit changes known
/// fields, while adapter imports may supply their complete raw projection.
pub fn materialize_projection(
    upsert: &PimUpsertV1,
    existing: Option<&str>,
) -> anyhow::Result<String> {
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
            for (field, property) in [("email", "EMAIL"), ("phone", "TEL")] {
                let value = escape_projection(&text_field(upsert, field));
                if !value.is_empty() {
                    lines.push(format!("{property}:{value}"));
                }
            }
            lines.push("END:VCARD".to_string());
            format!("{}\r\n", lines.join("\r\n"))
        }
        PimResourceKind::CalendarEvent => {
            let dtstamp = required_utc_field(upsert, "dtstamp")?;
            let starts_at = required_utc_field(upsert, "starts_at")?;
            let ends_at = optional_utc_field(upsert, "ends_at")?;
            if let Some(ends_at) = ends_at.as_deref() {
                anyhow::ensure!(
                    ends_at >= starts_at.as_str(),
                    "event end precedes its start"
                );
            }
            let mut lines = vec![
                "BEGIN:VCALENDAR".to_string(),
                "VERSION:2.0".to_string(),
                "PRODID:-//Kamori//EN".to_string(),
                "BEGIN:VEVENT".to_string(),
                format!("UID:{uid}"),
                format!("DTSTAMP:{dtstamp}"),
                format!("SUMMARY:{title}"),
                format!("DTSTART:{starts_at}"),
            ];
            if let Some(ends_at) = ends_at {
                lines.push(format!("DTEND:{ends_at}"));
            }
            lines.extend(["END:VEVENT".to_string(), "END:VCALENDAR".to_string()]);
            format!("{}\r\n", lines.join("\r\n"))
        }
        PimResourceKind::Task => {
            let dtstamp = required_utc_field(upsert, "dtstamp")?;
            let completed = matches!(
                upsert.fields.get("completed"),
                Some(PimValue::Boolean(true))
            );
            format!(
                "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Kamori//EN\r\nBEGIN:VTODO\r\nUID:{uid}\r\nDTSTAMP:{dtstamp}\r\nSUMMARY:{title}\r\nSTATUS:{}\r\nEND:VTODO\r\nEND:VCALENDAR\r\n",
                if completed {
                    "COMPLETED"
                } else {
                    "NEEDS-ACTION"
                },
            )
        }
    };
    validate_projection(upsert.resource_kind, &projection)?;
    Ok(projection)
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

fn optional_utc_field(upsert: &PimUpsertV1, name: &str) -> anyhow::Result<Option<String>> {
    match upsert.fields.get(name) {
        None | Some(PimValue::Null) => Ok(None),
        Some(PimValue::Text(value)) => {
            validate_compact_utc(value, name)?;
            Ok(Some(value.clone()))
        }
        Some(_) => anyhow::bail!("{name} must be a UTC date-time string"),
    }
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

    fn end_line_index(&self, component_id: usize) -> anyhow::Result<usize> {
        let name = &self.components[component_id].name;
        self.lines
            .iter()
            .position(|line| {
                line.component_id == Some(component_id)
                    && line.property == "END"
                    && line.value.eq_ignore_ascii_case(name)
            })
            .ok_or_else(|| anyhow::anyhow!("PIM component has no END line"))
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
    let mut edits = Vec::<(&str, String)>::new();
    if let Some(PimValue::Text(value)) = upsert.fields.get("title") {
        edits.push((
            if upsert.resource_kind == PimResourceKind::Contact {
                "FN"
            } else {
                "SUMMARY"
            },
            escape_projection(value),
        ));
    }
    for (field, property) in [
        ("email", "EMAIL"),
        ("phone", "TEL"),
        ("starts_at", "DTSTART"),
        ("ends_at", "DTEND"),
    ] {
        if let Some(PimValue::Text(value)) = upsert.fields.get(field) {
            edits.push((property, escape_projection(value)));
        }
    }
    if let Some(PimValue::Boolean(completed)) = upsert.fields.get("completed") {
        edits.push((
            "STATUS",
            if *completed {
                "COMPLETED"
            } else {
                "NEEDS-ACTION"
            }
            .to_string(),
        ));
    }
    if edits.is_empty() {
        validate_projection(upsert.resource_kind, existing)?;
        return Ok(existing.to_string());
    }

    let mut parsed = parse_projection(existing)?;
    let primary_component = parsed.primary_component(upsert.resource_kind)?;
    let mut applied = vec![false; edits.len()];
    for line in &mut parsed.lines {
        if line.component_id != Some(primary_component) {
            continue;
        }
        if let Some((index, (_, value))) = edits
            .iter()
            .enumerate()
            .find(|(index, (name, _))| !applied[*index] && line.property.eq_ignore_ascii_case(name))
        {
            line.raw = fold_content_line(&line.head, value);
            line.value = value.clone();
            applied[index] = true;
        }
    }
    let mut insertion_index = parsed.end_line_index(primary_component)?;
    for (index, (name, value)) in edits.iter().enumerate() {
        if applied[index] {
            continue;
        }
        parsed.lines.insert(
            insertion_index,
            ProjectionLine {
                raw: fold_content_line(name, value),
                head: (*name).to_string(),
                value: value.clone(),
                property: (*name).to_string(),
                component_id: Some(primary_component),
            },
        );
        insertion_index += 1;
    }
    let mut output = parsed
        .lines
        .into_iter()
        .map(|line| line.raw)
        .collect::<Vec<_>>()
        .join("\r\n");
    output.push_str("\r\n");
    validate_projection(upsert.resource_kind, &output)?;
    Ok(output)
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

        assert!(PimSnapshotV2::decode(&snapshot.encode().expect("encode")).is_err());
    }

    #[test]
    fn rejects_multi_parent_v1_operation() {
        let operation = PimOperationV1::Delete(PimDeleteV1 {
            resource_kind: PimResourceKind::Task,
            resource_id: Uuid::from_u128(9),
            dependencies: vec![Uuid::from_u128(7), Uuid::from_u128(8)],
            projection_resource_id: None,
        });
        let encoded = operation.encode().expect("encode unsupported operation");

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
}
