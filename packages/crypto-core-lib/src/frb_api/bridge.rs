use std::collections::BTreeMap;

use crate::{
    local_bridge_runner::{
        DavResourceKind, LocalBridgeConfig, LocalBridgeRunner, MaterializedPimBranch,
    },
    pim::{PimResourceKind, PimValue},
};

use super::{
    state::{
        MOBILE_BRIDGE_RUNTIME, MOBILE_COLLECTION_KEYS, MOBILE_DEVICE_SECRETS,
        MOBILE_REFRESH_TOKEN, MOBILE_RUNNER, MOBILE_RUNTIME_LEASE, MOBILE_SYNC_STARTS,
    },
    types::{
        MobileDeviceSecrets, MobileLabeledValue, MobilePimDraft, MobilePimItem,
        MobilePimTemporal, MobilePostalAddress, MobileSyncConfig,
    },
};

pub(super) async fn mobile_runner() -> Result<LocalBridgeRunner, String> {
    if let Some(runner) = MOBILE_RUNNER.lock().await.clone() {
        return Ok(runner);
    }
    let config = MOBILE_BRIDGE_RUNTIME
        .lock()
        .await
        .last_config
        .clone()
        .ok_or_else(|| "mobile sync has not been configured yet".to_string())?;
    let refresh_token = MOBILE_REFRESH_TOKEN.lock().await.clone();
    let mut local_config = LocalBridgeConfig::new(
        config.sqlite_path,
        config.cloud_base_url,
        config.access_token,
    )
    .with_sqlite_key(hex::encode(config.sqlite_key));
    if let Some(refresh_token) = refresh_token {
        local_config = local_config.with_refresh_token(refresh_token);
    }
    if let Some(device) = MOBILE_DEVICE_SECRETS.lock().await.clone() {
        let device_id = uuid::Uuid::parse_str(&device.device_id)
            .map_err(|error| format!("invalid persisted mobile device id: {error}"))?;
        local_config = local_config.with_device_identity(
            crate::local_bridge_runner::LocalDeviceIdentity {
                device_id,
                signing_private_key: device.signing_private_key,
            },
        );
    }
    let runner = LocalBridgeRunner::new(local_config).map_err(|error| error.to_string())?;
    for (collection_id, (key_epoch, cmk)) in MOBILE_COLLECTION_KEYS.lock().await.clone() {
        let sync_start_seq = MOBILE_SYNC_STARTS
            .lock()
            .await
            .get(&collection_id)
            .copied()
            .unwrap_or(0);
        runner
            .register_collection_key_epoch_from(
                collection_id,
                key_epoch,
                cmk,
                sync_start_seq,
            )
            .await
            .map_err(|error| error.to_string())?;
    }
    *MOBILE_RUNNER.lock().await = Some(runner.clone());
    Ok(runner)
}

async fn persist_runner_refresh_token(runner: &LocalBridgeRunner) {
    if let Some(config) = MOBILE_BRIDGE_RUNTIME.lock().await.last_config.as_mut() {
        config.access_token = runner.current_access_token().await;
    }
    super::state::set_mobile_refresh_token(runner.current_refresh_token().await).await;
}

pub(super) async fn mobile_configure_sync_impl(
    cloud_base_url: String,
    sqlite_path: String,
    access_token: String,
    sqlite_key: [u8; 32],
    device: Option<MobileDeviceSecrets>,
) -> Result<(), String> {
    let _lease = MOBILE_RUNTIME_LEASE.lock().await;
    let cloud_base_url = crate::local_bridge_runner::normalize_cloud_base_url(&cloud_base_url)
        .map_err(|error| error.to_string())?;
    if cloud_base_url.trim().is_empty()
        || sqlite_path.trim().is_empty()
        || access_token.trim().is_empty()
    {
        return Err("sync configuration fields must not be empty".to_string());
    }
    let mut runtime = MOBILE_BRIDGE_RUNTIME.lock().await;
    runtime.last_config = Some(MobileSyncConfig {
        cloud_base_url,
        sqlite_path,
        access_token,
        sqlite_key,
    });
    drop(runtime);
    MOBILE_RUNNER.lock().await.take();
    if let Some(device) = device {
        *MOBILE_DEVICE_SECRETS.lock().await = Some(device);
    }
    mobile_runner().await.map(|_| ())
}

pub(super) async fn mobile_sync_now_impl() -> Result<u64, String> {
    let _lease = MOBILE_RUNTIME_LEASE.lock().await;
    if MOBILE_COLLECTION_KEYS.lock().await.is_empty() {
        return Err("no collection keys are registered for sync".to_string());
    }
    let runner = mobile_runner().await?;
    let result = runner.sync_once().await.map_err(|error| error.to_string());
    persist_runner_refresh_token(&runner).await;
    result
}

fn materialize_mobile_item(branch: MaterializedPimBranch) -> Option<MobilePimItem> {
    if branch.deleted {
        return None;
    }
    let payload = branch.payload?;
    let parsed_kind = match branch.kind {
        DavResourceKind::Contact => PimResourceKind::Contact,
        DavResourceKind::Calendar => {
            crate::pim::validate_dav_projection(false, &payload).ok()?
        }
        DavResourceKind::Note => return None,
    };
    let resource_kind = match parsed_kind {
        PimResourceKind::Contact => "contact",
        PimResourceKind::Task => "task",
        PimResourceKind::CalendarEvent => "calendar_event",
    };
    let fields = crate::pim::projection_fields(&payload, parsed_kind).ok()?;
    let title = pim_text(&fields, "title").unwrap_or_default();
    Some(MobilePimItem {
        space_id: branch.space_id.to_string(),
        resource_id: branch.logical_resource_id.to_string(),
        projection_id: branch.projection_resource_id,
        head_operation_id: branch.head_operation_id.to_string(),
        resource_kind: resource_kind.to_string(),
        title,
        completed: pim_boolean(&fields, "completed"),
        completed_at: pim_text(&fields, "completed_at"),
        notes: pim_text(&fields, "notes"),
        starts_at: pim_temporal(&fields, "starts_at"),
        ends_at: pim_temporal(&fields, "ends_at"),
        due_at: pim_temporal(&fields, "due_at"),
        priority: pim_integer(&fields, "priority").unwrap_or(0),
        location: pim_text(&fields, "location"),
        recurrence_rule: pim_text(&fields, "recurrence_rule"),
        reminder_minutes: pim_integer(&fields, "reminder_minutes"),
        categories: pim_text_list(&fields, "categories"),
        name_prefix: pim_record_value(&fields, "name", "prefix"),
        given_name: pim_record_value(&fields, "name", "given"),
        middle_name: pim_record_value(&fields, "name", "middle"),
        family_name: pim_record_value(&fields, "name", "family"),
        name_suffix: pim_record_value(&fields, "name", "suffix"),
        emails: pim_labeled_values(&fields, "emails"),
        phones: pim_labeled_values(&fields, "phones"),
        addresses: pim_addresses(&fields),
        organization: pim_text(&fields, "organization"),
        job_title: pim_text(&fields, "job_title"),
        birthday: pim_text(&fields, "birthday"),
        url: pim_text(&fields, "url"),
        favorite: pim_boolean(&fields, "favorite"),
        conflict: branch.conflict,
    })
}

fn pim_text(fields: &BTreeMap<String, PimValue>, name: &str) -> Option<String> {
    match fields.get(name) {
        Some(PimValue::Text(value)) => Some(value.clone()),
        _ => None,
    }
}

fn pim_boolean(fields: &BTreeMap<String, PimValue>, name: &str) -> bool {
    matches!(fields.get(name), Some(PimValue::Boolean(true)))
}

fn pim_integer(fields: &BTreeMap<String, PimValue>, name: &str) -> Option<i64> {
    match fields.get(name) {
        Some(PimValue::Integer(value)) => Some(*value),
        _ => None,
    }
}

fn pim_text_list(fields: &BTreeMap<String, PimValue>, name: &str) -> Vec<String> {
    match fields.get(name) {
        Some(PimValue::TextList(values)) => values.clone(),
        _ => Vec::new(),
    }
}

fn pim_record_value(
    fields: &BTreeMap<String, PimValue>,
    name: &str,
    key: &str,
) -> String {
    match fields.get(name) {
        Some(PimValue::Record(record)) => record.get(key).cloned().unwrap_or_default(),
        _ => String::new(),
    }
}

fn pim_temporal(fields: &BTreeMap<String, PimValue>, name: &str) -> Option<MobilePimTemporal> {
    let PimValue::Record(record) = fields.get(name)? else {
        return None;
    };
    Some(MobilePimTemporal {
        kind: record.get("kind").cloned().unwrap_or_default(),
        date: record.get("date").cloned(),
        utc: record.get("utc").cloned(),
        local: record.get("local").cloned(),
        timezone: record.get("timezone").cloned(),
    })
}

fn pim_labeled_values(
    fields: &BTreeMap<String, PimValue>,
    name: &str,
) -> Vec<MobileLabeledValue> {
    match fields.get(name) {
        Some(PimValue::Records(records)) => records
            .iter()
            .filter_map(|record| {
                let value = record.get("value")?.clone();
                Some(MobileLabeledValue {
                    label: record.get("label").cloned().unwrap_or_default(),
                    value,
                    raw_head: record.get("raw_head").cloned(),
                })
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn pim_addresses(fields: &BTreeMap<String, PimValue>) -> Vec<MobilePostalAddress> {
    match fields.get("addresses") {
        Some(PimValue::Records(records)) => records
            .iter()
            .map(|record| MobilePostalAddress {
                label: record.get("label").cloned().unwrap_or_default(),
                raw_head: record.get("raw_head").cloned(),
                po_box: record.get("po_box").cloned().unwrap_or_default(),
                extended: record.get("extended").cloned().unwrap_or_default(),
                street: record.get("street").cloned().unwrap_or_default(),
                locality: record.get("locality").cloned().unwrap_or_default(),
                region: record.get("region").cloned().unwrap_or_default(),
                postal_code: record.get("postal_code").cloned().unwrap_or_default(),
                country: record.get("country").cloned().unwrap_or_default(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn parse_pim_kind(value: &str) -> Result<PimResourceKind, String> {
    match value.trim() {
        "calendar_event" => Ok(PimResourceKind::CalendarEvent),
        "task" => Ok(PimResourceKind::Task),
        "contact" => Ok(PimResourceKind::Contact),
        _ => Err("resource kind must be calendar_event, task, or contact".to_string()),
    }
}

fn optional_text_value(value: &Option<String>) -> PimValue {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| PimValue::Text(value.to_string()))
        .unwrap_or(PimValue::Null)
}

fn mobile_temporal_value(value: &MobilePimTemporal) -> PimValue {
    let mut record = BTreeMap::from([("kind".to_string(), value.kind.clone())]);
    for (name, value) in [
        ("date", &value.date),
        ("utc", &value.utc),
        ("local", &value.local),
        ("timezone", &value.timezone),
    ] {
        if let Some(value) = value {
            record.insert(name.to_string(), value.clone());
        }
    }
    PimValue::Record(record)
}

fn draft_fields(draft: &MobilePimDraft, title: &str) -> BTreeMap<String, PimValue> {
    let temporal = |value: &Option<MobilePimTemporal>| {
        value
            .as_ref()
            .map(mobile_temporal_value)
            .unwrap_or(PimValue::Null)
    };
    let labeled_values = |values: &[MobileLabeledValue]| {
        PimValue::Records(
            values
                .iter()
                .filter(|value| !value.value.trim().is_empty())
                .map(|value| {
                    let mut record = BTreeMap::from([
                        ("label".to_string(), value.label.trim().to_string()),
                        ("value".to_string(), value.value.trim().to_string()),
                    ]);
                    if let Some(raw_head) = &value.raw_head {
                        record.insert("raw_head".to_string(), raw_head.clone());
                    }
                    record
                })
                .collect(),
        )
    };
    let addresses = draft
        .addresses
        .iter()
        .map(|address| {
            let mut record = BTreeMap::from([
                ("label".to_string(), address.label.trim().to_string()),
                ("po_box".to_string(), address.po_box.trim().to_string()),
                ("extended".to_string(), address.extended.trim().to_string()),
                ("street".to_string(), address.street.trim().to_string()),
                ("locality".to_string(), address.locality.trim().to_string()),
                ("region".to_string(), address.region.trim().to_string()),
                ("postal_code".to_string(), address.postal_code.trim().to_string()),
                ("country".to_string(), address.country.trim().to_string()),
            ]);
            if let Some(raw_head) = &address.raw_head {
                record.insert("raw_head".to_string(), raw_head.clone());
            }
            record
        })
        .filter(|address| {
            ["po_box", "extended", "street", "locality", "region", "postal_code", "country"]
                .iter()
                .any(|key| address.get(*key).is_some_and(|value| !value.is_empty()))
        })
        .collect();
    BTreeMap::from([
        ("title".to_string(), PimValue::Text(title.to_string())),
        ("completed_at".to_string(), optional_text_value(&draft.completed_at)),
        ("notes".to_string(), optional_text_value(&draft.notes)),
        ("starts_at".to_string(), temporal(&draft.starts_at)),
        ("ends_at".to_string(), temporal(&draft.ends_at)),
        ("due_at".to_string(), temporal(&draft.due_at)),
        ("priority".to_string(), PimValue::Integer(draft.priority)),
        ("location".to_string(), optional_text_value(&draft.location)),
        (
            "recurrence_rule".to_string(),
            optional_text_value(&draft.recurrence_rule),
        ),
        (
            "reminder_minutes".to_string(),
            draft
                .reminder_minutes
                .map(PimValue::Integer)
                .unwrap_or(PimValue::Null),
        ),
        (
            "categories".to_string(),
            PimValue::TextList(
                draft
                    .categories
                    .iter()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                    .collect(),
            ),
        ),
        (
            "name".to_string(),
            PimValue::Record(BTreeMap::from([
                ("prefix".to_string(), draft.name_prefix.trim().to_string()),
                ("given".to_string(), draft.given_name.trim().to_string()),
                ("middle".to_string(), draft.middle_name.trim().to_string()),
                ("family".to_string(), draft.family_name.trim().to_string()),
                ("suffix".to_string(), draft.name_suffix.trim().to_string()),
            ])),
        ),
        ("emails".to_string(), labeled_values(&draft.emails)),
        ("phones".to_string(), labeled_values(&draft.phones)),
        ("addresses".to_string(), PimValue::Records(addresses)),
        (
            "organization".to_string(),
            optional_text_value(&draft.organization),
        ),
        ("job_title".to_string(), optional_text_value(&draft.job_title)),
        ("birthday".to_string(), optional_text_value(&draft.birthday)),
        ("url".to_string(), optional_text_value(&draft.url)),
        ("favorite".to_string(), PimValue::Boolean(draft.favorite)),
    ])
}

pub(super) async fn mobile_list_pim_items_impl() -> Result<Vec<MobilePimItem>, String> {
    let _lease = MOBILE_RUNTIME_LEASE.lock().await;
    let runner = mobile_runner().await?;
    let mut items = Vec::new();
    let space_ids = MOBILE_COLLECTION_KEYS
        .lock()
        .await
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    for space_id in space_ids {
        let space_id = uuid::Uuid::parse_str(&space_id)
            .map_err(|error| format!("invalid security-space id: {error}"))?;
        let branches = runner
            .list_materialized_pim_branches(space_id)
            .await
            .map_err(|error| error.to_string())?;
        items.extend(branches.into_iter().filter_map(materialize_mobile_item));
    }
    items.sort_by_key(|item| item.title.to_lowercase());
    Ok(items)
}

pub(super) async fn mobile_upsert_pim_item_impl(
    draft: MobilePimDraft,
) -> Result<MobilePimItem, String> {
    let _lease = MOBILE_RUNTIME_LEASE.lock().await;
    let space_id = uuid::Uuid::parse_str(&draft.space_id)
        .map_err(|error| format!("invalid security-space id: {error}"))?;
    let resource_id = draft.resource_id
        .as_deref()
        .map(uuid::Uuid::parse_str)
        .transpose()
        .map_err(|error| format!("invalid PIM resource id: {error}"))?
        .unwrap_or_else(uuid::Uuid::new_v4);
    let kind = parse_pim_kind(&draft.resource_kind)?;
    let expected_head = draft.head_operation_id
        .as_deref()
        .map(uuid::Uuid::parse_str)
        .transpose()
        .map_err(|error| format!("invalid PIM branch head: {error}"))?;
    if draft.projection_id.is_some() != expected_head.is_some() {
        return Err("projection_id and head_operation_id must be supplied together".to_string());
    }
    let title = draft.title.trim().to_string();
    if title.is_empty() || title.chars().count() > 500 {
        return Err("title must contain 1 to 500 characters".to_string());
    }
    let runner = mobile_runner().await?;
    let existing_fields = if let Some(projection_id) = draft.projection_id.as_deref() {
        runner
            .list_materialized_pim_branches(space_id)
            .await
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|branch| branch.projection_resource_id == projection_id)
            .and_then(|branch| branch.payload)
            .map(|payload| crate::pim::projection_fields(&payload, kind))
            .transpose()
            .map_err(|error| error.to_string())?
    } else {
        None
    };
    let mut fields = draft_fields(&draft, &title);
    match kind {
        PimResourceKind::Task => {
            fields.insert("completed".to_string(), PimValue::Boolean(draft.completed));
        }
        PimResourceKind::Contact => {}
        PimResourceKind::CalendarEvent => {
            draft.starts_at
                .as_ref()
                .ok_or_else(|| "calendar event start is required".to_string())?;
            if draft.projection_id.is_none() {
                draft.ends_at
                    .as_ref()
                    .ok_or_else(|| "new calendar event end is required".to_string())?;
            }
        }
    }
    if kind != PimResourceKind::Contact {
        fields.insert(
            "dtstamp".to_string(),
            PimValue::Text(
                crate::local_bridge_runner::current_ical_utc()
                    .map_err(|error| error.to_string())?,
            ),
        );
    }
    if let Some(existing_fields) = existing_fields.as_ref() {
        fields.retain(|name, value| {
            name == "dtstamp"
                || match existing_fields.get(name) {
                    Some(existing) => existing != value,
                    None => !matches!(value, PimValue::Null),
                }
        });
    }
    let result = match (draft.projection_id.as_ref(), expected_head) {
        (Some(projection_id), Some(expected_head)) => runner
            .upsert_pim_branch(
                space_id,
                resource_id,
                projection_id.clone(),
                expected_head,
                kind,
                fields.clone(),
            )
            .await,
        (None, None) => runner
            .upsert_pim_item(space_id, resource_id, kind, fields.clone())
            .await,
        _ => unreachable!("paired branch fields were validated"),
    };
    persist_runner_refresh_token(&runner).await;
    let head_operation_id = result.map_err(|error| error.to_string())?;
    runner
        .list_materialized_pim_branches(space_id)
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|branch| branch.head_operation_id == head_operation_id)
        .and_then(materialize_mobile_item)
        .ok_or_else(|| "saved PIM branch could not be materialized".to_string())
}

pub(super) async fn mobile_delete_pim_item_impl(
    space_id: String,
    resource_id: String,
    projection_id: String,
    head_operation_id: String,
    resource_kind: String,
) -> Result<(), String> {
    let _lease = MOBILE_RUNTIME_LEASE.lock().await;
    let space_id = uuid::Uuid::parse_str(&space_id)
        .map_err(|error| format!("invalid security-space id: {error}"))?;
    let resource_id = uuid::Uuid::parse_str(&resource_id)
        .map_err(|error| format!("invalid PIM resource id: {error}"))?;
    let kind = parse_pim_kind(&resource_kind)?;
    let head_operation_id = uuid::Uuid::parse_str(&head_operation_id)
        .map_err(|error| format!("invalid PIM branch head: {error}"))?;
    let runner = mobile_runner().await?;
    let result = runner
        .delete_pim_branch(
            space_id,
            resource_id,
            projection_id,
            head_operation_id,
            kind,
        )
        .await;
    persist_runner_refresh_token(&runner).await;
    result.map_err(|error| error.to_string())?;
    Ok(())
}

pub(super) async fn mobile_register_collection_key_impl(
    collection_id: String,
    key_epoch: u32,
    sync_start_seq: u64,
    cmk: [u8; 32],
) -> Result<(), String> {
    let _lease = MOBILE_RUNTIME_LEASE.lock().await;
    if key_epoch == 0 {
        return Err("key_epoch must be positive".to_string());
    }
    MOBILE_COLLECTION_KEYS
        .lock()
        .await
        .insert(collection_id.clone(), (key_epoch, cmk));
    MOBILE_SYNC_STARTS
        .lock()
        .await
        .insert(collection_id.clone(), sync_start_seq);
    let runner = { MOBILE_RUNNER.lock().await.clone() };
    if let Some(runner) = runner {
        runner
            .register_collection_key_epoch_from(collection_id, key_epoch, cmk, sync_start_seq)
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(super) async fn mobile_unregister_collection_key_impl(
    collection_id: String,
) -> Result<(), String> {
    let _lease = MOBILE_RUNTIME_LEASE.lock().await;
    MOBILE_COLLECTION_KEYS.lock().await.remove(&collection_id);
    MOBILE_SYNC_STARTS.lock().await.remove(&collection_id);
    let runner = { MOBILE_RUNNER.lock().await.clone() };
    if let Some(runner) = runner {
        let _ = runner.unregister_collection_key(&collection_id).await;
    }
    Ok(())
}
