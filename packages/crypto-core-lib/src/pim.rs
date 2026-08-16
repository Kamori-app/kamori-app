//! Versioned PIM operation codec independent from DAV and transport ordering.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
        Ok(rmp_serde::from_slice(bytes)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
