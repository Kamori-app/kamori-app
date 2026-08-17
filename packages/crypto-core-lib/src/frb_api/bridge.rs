use std::collections::BTreeMap;

use crate::{
    local_bridge_runner::{DavResourceKind, LocalBridgeConfig, LocalBridgeRunner, LocalResource},
    pim::{PimResourceKind, PimValue},
};

use super::{
    state::{
        MOBILE_BRIDGE_RUNTIME, MOBILE_COLLECTION_KEYS, MOBILE_DEVICE_SECRETS, MOBILE_REFRESH_TOKEN,
    },
    types::{MobileDeviceSecrets, MobilePimItem, MobileSyncConfig},
};

async fn mobile_runner() -> Result<LocalBridgeRunner, String> {
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
        runner
            .register_collection_key_epoch(collection_id, key_epoch, cmk)
            .await;
    }
    Ok(runner)
}

async fn persist_runner_refresh_token(runner: &LocalBridgeRunner) {
    super::state::set_mobile_refresh_token(runner.current_refresh_token().await).await;
}

pub(super) async fn mobile_configure_sync_impl(
    cloud_base_url: String,
    sqlite_path: String,
    access_token: String,
    sqlite_key: [u8; 32],
    device: Option<MobileDeviceSecrets>,
) -> Result<(), String> {
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
    if let Some(device) = device {
        *MOBILE_DEVICE_SECRETS.lock().await = Some(device);
    }
    Ok(())
}

pub(super) async fn mobile_sync_now_impl() -> Result<u64, String> {
    if MOBILE_COLLECTION_KEYS.lock().await.is_empty() {
        return Err("no collection keys are registered for sync".to_string());
    }
    let runner = mobile_runner().await?;
    let synced = runner.sync_once().await.map_err(|error| error.to_string())?;
    persist_runner_refresh_token(&runner).await;
    Ok(synced)
}

fn projection_field(payload: &str, name: &str) -> Option<String> {
    payload.lines().find_map(|line| {
        line.strip_prefix(name)
            .map(|value| value.trim_end_matches('\r').to_string())
    })
}

fn unescape_projection(value: String) -> String {
    value
        .replace("\\n", "\n")
        .replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\\\", "\\")
}

fn materialize_mobile_item(resource: LocalResource) -> Option<MobilePimItem> {
    let resource_id = resource.resource_id.split('.').next()?;
    let resource_id = uuid::Uuid::parse_str(resource_id).ok()?.to_string();
    let (resource_kind, title) = match resource.kind {
        DavResourceKind::Contact => (
            "contact",
            projection_field(&resource.payload, "FN:").unwrap_or_default(),
        ),
        DavResourceKind::Calendar if resource.payload.contains("BEGIN:VTODO") => (
            "task",
            projection_field(&resource.payload, "SUMMARY:").unwrap_or_default(),
        ),
        DavResourceKind::Calendar => (
            "calendar_event",
            projection_field(&resource.payload, "SUMMARY:").unwrap_or_default(),
        ),
        DavResourceKind::Note => return None,
    };
    Some(MobilePimItem {
        space_id: resource.collection_id,
        resource_id,
        resource_kind: resource_kind.to_string(),
        title: unescape_projection(title),
        completed: projection_field(&resource.payload, "STATUS:")
            .is_some_and(|status| status == "COMPLETED"),
        email: projection_field(&resource.payload, "EMAIL:").map(unescape_projection),
        phone: projection_field(&resource.payload, "TEL:").map(unescape_projection),
        starts_at: projection_field(&resource.payload, "DTSTART:"),
        ends_at: projection_field(&resource.payload, "DTEND:"),
        conflict: resource.resource_id.contains(".conflict-"),
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
        for kind in [DavResourceKind::Contact, DavResourceKind::Calendar] {
            let resources = runner
                .list_cached_resources(space_id, kind)
                .await
                .map_err(|error| error.to_string())?;
            items.extend(resources.into_iter().filter_map(materialize_mobile_item));
        }
    }
    items.sort_by_key(|item| item.title.to_lowercase());
    Ok(items)
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn mobile_upsert_pim_item_impl(
    space_id: String,
    resource_id: Option<String>,
    resource_kind: String,
    title: String,
    completed: bool,
    email: Option<String>,
    phone: Option<String>,
    starts_at: Option<String>,
    ends_at: Option<String>,
) -> Result<MobilePimItem, String> {
    let space_id = uuid::Uuid::parse_str(&space_id)
        .map_err(|error| format!("invalid security-space id: {error}"))?;
    let resource_id = resource_id
        .map(|value| uuid::Uuid::parse_str(&value))
        .transpose()
        .map_err(|error| format!("invalid PIM resource id: {error}"))?
        .unwrap_or_else(uuid::Uuid::new_v4);
    let kind = parse_pim_kind(&resource_kind)?;
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
    runner
        .upsert_pim_item(space_id, resource_id, kind, fields)
        .await
        .map_err(|error| error.to_string())?;
    persist_runner_refresh_token(&runner).await;
    Ok(MobilePimItem {
        space_id: space_id.to_string(),
        resource_id: resource_id.to_string(),
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
    resource_kind: String,
) -> Result<(), String> {
    let space_id = uuid::Uuid::parse_str(&space_id)
        .map_err(|error| format!("invalid security-space id: {error}"))?;
    let resource_id = uuid::Uuid::parse_str(&resource_id)
        .map_err(|error| format!("invalid PIM resource id: {error}"))?;
    let kind = parse_pim_kind(&resource_kind)?;
    let runner = mobile_runner().await?;
    runner
        .delete_pim_item(space_id, resource_id, kind)
        .await
        .map_err(|error| error.to_string())?;
    persist_runner_refresh_token(&runner).await;
    Ok(())
}

pub(super) async fn mobile_register_collection_key_impl(
    collection_id: String,
    key_epoch: u32,
    cmk: [u8; 32],
) -> Result<(), String> {
    if key_epoch == 0 {
        return Err("key_epoch must be positive".to_string());
    }
    MOBILE_COLLECTION_KEYS
        .lock()
        .await
        .insert(collection_id, (key_epoch, cmk));
    Ok(())
}

pub(super) async fn mobile_unregister_collection_key_impl(
    collection_id: String,
) -> Result<(), String> {
    MOBILE_COLLECTION_KEYS.lock().await.remove(&collection_id);
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
