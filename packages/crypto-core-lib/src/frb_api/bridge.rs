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
    types::{MobileDeviceSecrets, MobilePimItem, MobileSyncConfig},
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
    let title_property = if parsed_kind == PimResourceKind::Contact {
        "FN"
    } else {
        "SUMMARY"
    };
    let field = |name: &str| {
        crate::pim::projection_property(&payload, parsed_kind, name)
            .ok()
            .flatten()
    };
    let title = field(title_property).unwrap_or_default();
    Some(MobilePimItem {
        space_id: branch.space_id.to_string(),
        resource_id: branch.logical_resource_id.to_string(),
        projection_id: branch.projection_resource_id,
        head_operation_id: branch.head_operation_id.to_string(),
        resource_kind: resource_kind.to_string(),
        title: crate::pim::unescape_projection_text(&title),
        completed: field("STATUS")
            .is_some_and(|status| status == "COMPLETED"),
        email: field("EMAIL").map(|value| crate::pim::unescape_projection_text(&value)),
        phone: field("TEL").map(|value| crate::pim::unescape_projection_text(&value)),
        starts_at: field("DTSTART"),
        ends_at: field("DTEND"),
        conflict: branch.conflict,
    })
}

fn parse_pim_kind(value: &str) -> Result<PimResourceKind, String> {
    match value.trim() {
        "calendar_event" => Ok(PimResourceKind::CalendarEvent),
        "task" => Ok(PimResourceKind::Task),
        "contact" => Ok(PimResourceKind::Contact),
        _ => Err("resource kind must be calendar_event, task, or contact".to_string()),
    }
}

fn validate_compact_utc(value: &str, field: &str) -> Result<(), String> {
    let bytes = value.as_bytes();
    let digits_are_valid = bytes.len() == 16
        && bytes[8] == b'T'
        && bytes[15] == b'Z'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 8 | 15) || byte.is_ascii_digit());
    if !digits_are_valid {
        return Err(format!(
            "{field} must use the UTC format YYYYMMDDTHHMMSSZ"
        ));
    }

    let parse = |range: std::ops::Range<usize>| {
        value[range]
            .parse::<u32>()
            .map_err(|_| format!("invalid {field}"))
    };
    let year = parse(0..4)?;
    let month = parse(4..6)?;
    let day = parse(6..8)?;
    let hour = parse(9..11)?;
    let minute = parse(11..13)?;
    let second = parse(13..15)?;
    let leap_year = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap_year => 29,
        2 => 28,
        _ => 0,
    };
    if year == 0 || day == 0 || day > max_day || hour > 23 || minute > 59 || second > 59 {
        return Err(format!("{field} is not a valid UTC timestamp"));
    }
    Ok(())
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

#[allow(clippy::too_many_arguments)]
pub(super) async fn mobile_upsert_pim_item_impl(
    space_id: String,
    resource_id: Option<String>,
    projection_id: Option<String>,
    head_operation_id: Option<String>,
    resource_kind: String,
    title: String,
    completed: bool,
    email: Option<String>,
    phone: Option<String>,
    starts_at: Option<String>,
    ends_at: Option<String>,
) -> Result<MobilePimItem, String> {
    let _lease = MOBILE_RUNTIME_LEASE.lock().await;
    let space_id = uuid::Uuid::parse_str(&space_id)
        .map_err(|error| format!("invalid security-space id: {error}"))?;
    let resource_id = resource_id
        .map(|value| uuid::Uuid::parse_str(&value))
        .transpose()
        .map_err(|error| format!("invalid PIM resource id: {error}"))?
        .unwrap_or_else(uuid::Uuid::new_v4);
    let kind = parse_pim_kind(&resource_kind)?;
    let expected_head = head_operation_id
        .map(|value| uuid::Uuid::parse_str(&value))
        .transpose()
        .map_err(|error| format!("invalid PIM branch head: {error}"))?;
    if projection_id.is_some() != expected_head.is_some() {
        return Err("projection_id and head_operation_id must be supplied together".to_string());
    }
    let title = title.trim().to_string();
    if title.is_empty() || title.chars().count() > 500 {
        return Err("title must contain 1 to 500 characters".to_string());
    }
    let mut fields = BTreeMap::from([("title".to_string(), PimValue::Text(title.clone()))]);
    match kind {
        PimResourceKind::Task => {
            fields.insert("completed".to_string(), PimValue::Boolean(completed));
        }
        PimResourceKind::Contact => {
            fields.insert(
                "email".to_string(),
                PimValue::Text(email.clone().unwrap_or_default()),
            );
            fields.insert(
                "phone".to_string(),
                PimValue::Text(phone.clone().unwrap_or_default()),
            );
        }
        PimResourceKind::CalendarEvent => {
            let starts_at_value = starts_at
                .as_deref()
                .ok_or_else(|| "calendar event start is required".to_string())?;
            let ends_at_value = ends_at
                .as_deref()
                .ok_or_else(|| "calendar event end is required".to_string())?;
            validate_compact_utc(starts_at_value, "calendar event start")?;
            validate_compact_utc(ends_at_value, "calendar event end")?;
            if ends_at_value < starts_at_value {
                return Err("calendar event end must not be before its start".to_string());
            }
            fields.insert(
                "starts_at".to_string(),
                PimValue::Text(starts_at.clone().unwrap_or_default()),
            );
            fields.insert(
                "ends_at".to_string(),
                PimValue::Text(ends_at.clone().unwrap_or_default()),
            );
        }
    }
    let runner = mobile_runner().await?;
    let result = match (projection_id.as_ref(), expected_head) {
        (Some(projection_id), Some(expected_head)) => runner
            .upsert_pim_branch(
                space_id,
                resource_id,
                projection_id.clone(),
                expected_head,
                kind,
                fields,
            )
            .await,
        (None, None) => runner
            .upsert_pim_item(space_id, resource_id, kind, fields)
            .await,
        _ => unreachable!("paired branch fields were validated"),
    };
    persist_runner_refresh_token(&runner).await;
    let head_operation_id = result.map_err(|error| error.to_string())?;
    Ok(MobilePimItem {
        space_id: space_id.to_string(),
        resource_id: resource_id.to_string(),
        projection_id: projection_id.unwrap_or_else(|| match kind {
            PimResourceKind::Contact => format!("{resource_id}.vcf"),
            PimResourceKind::CalendarEvent | PimResourceKind::Task => {
                format!("{resource_id}.ics")
            }
        }),
        head_operation_id: head_operation_id.to_string(),
        resource_kind,
        title,
        completed,
        email,
        phone,
        starts_at,
        ends_at,
        conflict: false,
    })
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

#[cfg(test)]
mod tests {
    use super::validate_compact_utc;

    #[test]
    fn accepts_real_compact_utc_timestamp() {
        assert!(validate_compact_utc("20260228T235959Z", "start").is_ok());
        assert!(validate_compact_utc("20240229T000000Z", "start").is_ok());
    }

    #[test]
    fn rejects_impossible_or_non_utc_timestamp() {
        assert!(validate_compact_utc("20260229T000000Z", "start").is_err());
        assert!(validate_compact_utc("20260101T250000Z", "start").is_err());
        assert!(validate_compact_utc("2026-01-01T00:00:00Z", "start").is_err());
    }
}
