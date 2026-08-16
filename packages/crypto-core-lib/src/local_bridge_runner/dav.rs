use super::{
    DavResourceKind, LocalBridgeState, LocalResource, PutResult, UpsertOutcome, now_unix_ms,
    types::DavChange,
};
use anyhow::{Context, Result, anyhow};
use axum::{
    body::Bytes,
    extract::State,
    http::{
        HeaderMap, HeaderValue, Method, StatusCode, Uri,
        header::{
            ALLOW, AUTHORIZATION, CONTENT_TYPE, ETAG, IF_MATCH, IF_NONE_MATCH, LOCATION,
            WWW_AUTHENTICATE,
        },
    },
    response::{IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use quick_xml::{Reader, events::Event};
use std::{collections::BTreeMap, sync::Arc};
use tracing::error;

const MAX_REPORT_BYTES: usize = 1024 * 1024;
const PRINCIPAL_PATH: &str = "/principals/kamori/";

#[derive(Clone, Debug)]
struct ParsedDavPath {
    kind: DavResourceKind,
    collection_id: String,
    resource_id: Option<String>,
}

#[derive(Clone, Debug)]
enum DavTarget {
    Root,
    Principal,
    Home(DavResourceKind),
    Collection(ParsedDavPath),
    Resource(ParsedDavPath),
    WellKnown(DavResourceKind),
}

#[derive(Debug, PartialEq, Eq)]
enum DavReport {
    Query,
    MultiGet(Vec<String>),
    SyncCollection(Option<String>),
}

fn parse_dav_target(path: &str) -> Result<DavTarget> {
    let segments = path
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    match segments.as_slice() {
        [] => Ok(DavTarget::Root),
        [".well-known", "caldav"] => Ok(DavTarget::WellKnown(DavResourceKind::Calendar)),
        [".well-known", "carddav"] => Ok(DavTarget::WellKnown(DavResourceKind::Contact)),
        ["principals", "kamori"] => Ok(DavTarget::Principal),
        [scope] => DavResourceKind::from_route_segment(scope)
            .filter(|kind| *kind != DavResourceKind::Note)
            .map(DavTarget::Home)
            .ok_or_else(|| anyhow!("unsupported DAV path")),
        [scope, collection_id] => {
            let kind = DavResourceKind::from_route_segment(scope)
                .filter(|kind| *kind != DavResourceKind::Note)
                .ok_or_else(|| anyhow!("unsupported DAV scope"))?;
            Ok(DavTarget::Collection(ParsedDavPath {
                kind,
                collection_id: (*collection_id).to_string(),
                resource_id: None,
            }))
        }
        [scope, collection_id, resource_id] => {
            let kind = DavResourceKind::from_route_segment(scope)
                .filter(|kind| *kind != DavResourceKind::Note)
                .ok_or_else(|| anyhow!("unsupported DAV scope"))?;
            Ok(DavTarget::Resource(ParsedDavPath {
                kind,
                collection_id: (*collection_id).to_string(),
                resource_id: Some((*resource_id).to_string()),
            }))
        }
        _ => Err(anyhow!("unsupported DAV path")),
    }
}

/// Dispatches the authenticated localhost CalDAV/CardDAV projection.
pub(crate) async fn dav_dispatch(
    State(state): State<Arc<LocalBridgeState>>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let path = uri.path().to_string();
    if !is_authorized(&state, &headers) {
        let mut response = (StatusCode::UNAUTHORIZED, "DAV credentials required").into_response();
        response.headers_mut().insert(
            WWW_AUTHENTICATE,
            HeaderValue::from_static("Basic realm=\"Kamori local DAV\", charset=\"UTF-8\""),
        );
        return response;
    }
    if matches!(
        parse_dav_target(&path),
        Ok(DavTarget::WellKnown(DavResourceKind::Calendar))
    ) {
        return redirect_response("/caldav/");
    }
    if matches!(
        parse_dav_target(&path),
        Ok(DavTarget::WellKnown(DavResourceKind::Contact))
    ) {
        return redirect_response("/carddav/");
    }
    let result = match method.as_str() {
        "PROPFIND" => handle_propfind(state, path.clone(), &headers).await,
        "REPORT" => handle_report(state, path.clone(), body).await,
        "PUT" => handle_put(state, path.clone(), headers, body).await,
        "DELETE" => handle_delete(state, path.clone(), headers).await,
        "GET" => handle_get(state, path.clone(), false).await,
        "HEAD" => handle_get(state, path.clone(), true).await,
        "OPTIONS" => Ok(handle_options()),
        "MKCOL" | "MKCALENDAR" | "PROPPATCH" => Ok((
            StatusCode::FORBIDDEN,
            "collections are created in Kamori, not through the local DAV projection",
        )
            .into_response()),
        _ => Ok((StatusCode::METHOD_NOT_ALLOWED, "Method Not Allowed").into_response()),
    };

    match result {
        Ok(response) => response,
        Err(err) => {
            error!(error = ?err, path = %path, method = %method, "DAV handler failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal local DAV error",
            )
                .into_response()
        }
    }
}

fn is_authorized(state: &LocalBridgeState, headers: &HeaderMap) -> bool {
    let Some((username, password)) = state.dav_credentials.as_ref() else {
        return false;
    };
    let expected = format!(
        "Basic {}",
        STANDARD.encode(format!("{username}:{password}"))
    );
    let Some(actual) = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    actual.len() == expected.len()
        && actual
            .as_bytes()
            .iter()
            .zip(expected.as_bytes())
            .fold(0_u8, |difference, (left, right)| {
                difference | (left ^ right)
            })
            == 0
}

async fn handle_propfind(
    state: Arc<LocalBridgeState>,
    path: String,
    headers: &HeaderMap,
) -> Result<Response> {
    let Ok(target) = parse_dav_target(&path) else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    if let DavTarget::WellKnown(kind) = target {
        return Ok(redirect_response(kind.home_path()));
    }
    let depth = match parse_depth(headers) {
        Ok(depth) => depth,
        Err(_) => {
            return Ok((
                StatusCode::FORBIDDEN,
                "the local DAV projection supports Depth 0 and 1 only",
            )
                .into_response());
        }
    };
    let xml = match target {
        DavTarget::Root => root_multistatus(),
        DavTarget::Principal => principal_multistatus(),
        DavTarget::Home(kind) => {
            let collections = if depth == 1 {
                state.registered_collection_ids().await
            } else {
                Vec::new()
            };
            home_multistatus(kind, &collections)
        }
        DavTarget::Collection(parsed) => {
            if !collection_exists(&state, &parsed.collection_id).await {
                return Ok(StatusCode::NOT_FOUND.into_response());
            }
            let resources = if depth == 1 {
                state
                    .list_resources(parsed.kind, parsed.collection_id.clone())
                    .await?
            } else {
                Vec::new()
            };
            let revision = state
                .latest_dav_revision(parsed.kind, parsed.collection_id.clone())
                .await?;
            collection_multistatus(&parsed, &resources, revision, depth == 1, false)
        }
        DavTarget::Resource(parsed) => {
            if !collection_exists(&state, &parsed.collection_id).await {
                return Ok(StatusCode::NOT_FOUND.into_response());
            }
            let resource = state
                .get_resource(
                    parsed.kind,
                    parsed.collection_id.clone(),
                    parsed.resource_id.clone().expect("resource target"),
                )
                .await?;
            let Some(resource) = resource else {
                return Ok(StatusCode::NOT_FOUND.into_response());
            };
            resource_multistatus(&resource, false)
        }
        DavTarget::WellKnown(_) => unreachable!(),
    };
    Ok(xml_response(StatusCode::MULTI_STATUS, xml))
}

async fn handle_report(
    state: Arc<LocalBridgeState>,
    path: String,
    body: Bytes,
) -> Result<Response> {
    let Ok(DavTarget::Collection(parsed)) = parse_dav_target(&path) else {
        return Ok((StatusCode::BAD_REQUEST, "REPORT requires a collection URL").into_response());
    };
    if !collection_exists(&state, &parsed.collection_id).await {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    let report = match parse_report(&body) {
        Ok(report) => report,
        Err(_) => return Ok((StatusCode::BAD_REQUEST, "invalid DAV REPORT XML").into_response()),
    };
    let current_revision = state
        .latest_dav_revision(parsed.kind, parsed.collection_id.clone())
        .await?;
    let xml = match report {
        DavReport::Query => {
            let resources = state
                .list_resources(parsed.kind, parsed.collection_id.clone())
                .await?;
            collection_multistatus(&parsed, &resources, current_revision, false, true)
        }
        DavReport::MultiGet(hrefs) => {
            let mut resources = Vec::new();
            let mut missing = Vec::new();
            for href in hrefs {
                let Some(resource_id) = resource_id_from_href(&parsed, &href) else {
                    missing.push(href);
                    continue;
                };
                match state
                    .get_resource(parsed.kind, parsed.collection_id.clone(), resource_id)
                    .await?
                {
                    Some(resource) => resources.push(resource),
                    None => missing.push(href),
                }
            }
            multiget_xml(&resources, &missing)
        }
        DavReport::SyncCollection(token) => {
            let since = match token
                .as_deref()
                .map(|value| parse_sync_token(&parsed, value))
                .transpose()
            {
                Ok(revision) => revision.unwrap_or(0),
                Err(_) => return Ok(valid_sync_token_error()),
            };
            if since > current_revision {
                return Ok(valid_sync_token_error());
            }
            if token.is_none() || since == 0 {
                let resources = state
                    .list_resources(parsed.kind, parsed.collection_id.clone())
                    .await?;
                sync_multistatus(&parsed, &resources, &[], current_revision)
            } else {
                let changes = state
                    .list_dav_changes_since(parsed.kind, parsed.collection_id.clone(), since)
                    .await?;
                sync_changes_multistatus(&state, &parsed, changes, current_revision).await?
            }
        }
    };
    Ok(xml_response(StatusCode::MULTI_STATUS, xml))
}

async fn handle_get(
    state: Arc<LocalBridgeState>,
    path: String,
    head_only: bool,
) -> Result<Response> {
    let Ok(DavTarget::Resource(parsed)) = parse_dav_target(&path) else {
        return Ok((StatusCode::BAD_REQUEST, "resource URL is required for GET").into_response());
    };
    if !collection_exists(&state, &parsed.collection_id).await {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    let maybe = state
        .get_resource(
            parsed.kind,
            parsed.collection_id,
            parsed.resource_id.expect("resource target"),
        )
        .await?;
    let Some(resource) = maybe else {
        return Ok((StatusCode::NOT_FOUND, "resource not found").into_response());
    };
    let mut response = if head_only {
        StatusCode::OK.into_response()
    } else {
        (StatusCode::OK, resource.payload).into_response()
    };
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static(resource.kind.content_type()),
    );
    insert_etag(&mut response, &resource.etag);
    Ok(response)
}

async fn handle_put(
    state: Arc<LocalBridgeState>,
    path: String,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response> {
    let Ok(DavTarget::Resource(parsed)) = parse_dav_target(&path) else {
        return Ok((StatusCode::BAD_REQUEST, "resource URL is required for PUT").into_response());
    };
    if !collection_exists(&state, &parsed.collection_id).await {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    let resource_id = parsed.resource_id.expect("resource target");
    let payload = String::from_utf8(body.to_vec()).context("PUT payload must be UTF-8")?;
    if validate_payload(parsed.kind, &payload).is_err() {
        return Ok((
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "payload is not a complete vCard/iCalendar resource",
        )
            .into_response());
    }
    let existing = state
        .get_resource(
            parsed.kind,
            parsed.collection_id.clone(),
            resource_id.clone(),
        )
        .await?;
    let requires_absence = headers
        .get(IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim() == "*");
    if let Some(existing) = existing {
        if requires_absence {
            return Ok((StatusCode::PRECONDITION_FAILED, "resource already exists").into_response());
        }
        let Some(expected) = parse_etag_header(&headers) else {
            return Ok((
                StatusCode::PRECONDITION_REQUIRED,
                "If-Match is required when replacing a DAV resource",
            )
                .into_response());
        };
        if expected != "*" && expected != existing.etag {
            return Ok((StatusCode::PRECONDITION_FAILED, "ETag changed").into_response());
        }
    } else if parse_etag_header(&headers).as_deref() == Some("*") {
        return Ok((StatusCode::PRECONDITION_FAILED, "resource does not exist").into_response());
    }

    let put = state
        .put_resource_and_push(
            parsed.kind,
            parsed.collection_id,
            resource_id,
            payload,
            now_unix_ms(),
        )
        .await?;
    let status = match put.outcome {
        UpsertOutcome::Inserted => StatusCode::CREATED,
        UpsertOutcome::Updated => StatusCode::NO_CONTENT,
        UpsertOutcome::IgnoredStale => StatusCode::CONFLICT,
    };
    let mut response = (status, render_put_message(&put)).into_response();
    insert_etag(&mut response, &put.etag);
    Ok(response)
}

async fn handle_delete(
    state: Arc<LocalBridgeState>,
    path: String,
    headers: HeaderMap,
) -> Result<Response> {
    let Ok(DavTarget::Resource(parsed)) = parse_dav_target(&path) else {
        return Ok((
            StatusCode::BAD_REQUEST,
            "resource URL is required for DELETE",
        )
            .into_response());
    };
    if !collection_exists(&state, &parsed.collection_id).await {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    let resource_id = parsed.resource_id.expect("resource target");
    let Some(existing) = state
        .get_resource(
            parsed.kind,
            parsed.collection_id.clone(),
            resource_id.clone(),
        )
        .await?
    else {
        return Ok((StatusCode::NOT_FOUND, "resource not found").into_response());
    };
    let Some(expected) = parse_etag_header(&headers) else {
        return Ok((
            StatusCode::PRECONDITION_REQUIRED,
            "If-Match is required when deleting a DAV resource",
        )
            .into_response());
    };
    if expected != "*" && expected != existing.etag {
        return Ok((StatusCode::PRECONDITION_FAILED, "ETag changed").into_response());
    }
    state
        .delete_resource_and_push(parsed.kind, parsed.collection_id, resource_id)
        .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

fn handle_options() -> Response {
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(
        ALLOW,
        HeaderValue::from_static("OPTIONS, PROPFIND, REPORT, GET, HEAD, PUT, DELETE"),
    );
    response.headers_mut().insert(
        "dav",
        HeaderValue::from_static("1, 3, calendar-access, addressbook, sync-collection"),
    );
    response
}

fn parse_depth(headers: &HeaderMap) -> Result<u8> {
    match headers
        .get("depth")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
    {
        None | Some("0") => Ok(0),
        Some("1") => Ok(1),
        _ => Err(anyhow!("only DAV Depth 0 and 1 are supported")),
    }
}

fn parse_report(body: &[u8]) -> Result<DavReport> {
    if body.is_empty() || body.len() > MAX_REPORT_BYTES {
        return Err(anyhow!(
            "REPORT XML body must contain 1 to {MAX_REPORT_BYTES} bytes"
        ));
    }
    let mut reader = Reader::from_reader(body);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut root = None;
    let mut capture = None::<String>;
    let mut captured = String::new();
    let mut hrefs = Vec::new();
    let mut sync_token = None;
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(start) => {
                let local = String::from_utf8_lossy(start.local_name().as_ref()).into_owned();
                root.get_or_insert_with(|| local.clone());
                if local == "href" || local == "sync-token" {
                    capture = Some(local);
                    captured.clear();
                }
            }
            Event::Text(text) if capture.is_some() => {
                let decoded = text.decode()?;
                captured.push_str(&quick_xml::escape::unescape(&decoded)?);
            }
            Event::CData(text) if capture.is_some() => captured.push_str(&text.decode()?),
            Event::End(end) => {
                let local = String::from_utf8_lossy(end.local_name().as_ref()).into_owned();
                if capture.as_deref() == Some(local.as_str()) {
                    let value = captured.trim().to_string();
                    if local == "href" && !value.is_empty() {
                        hrefs.push(value);
                    } else if local == "sync-token" && !value.is_empty() {
                        sync_token = Some(value);
                    }
                    capture = None;
                    captured.clear();
                }
            }
            Event::DocType(_) => return Err(anyhow!("DOCTYPE is not allowed in DAV requests")),
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    match root.as_deref() {
        Some("calendar-query" | "addressbook-query") => Ok(DavReport::Query),
        Some("calendar-multiget" | "addressbook-multiget") => Ok(DavReport::MultiGet(hrefs)),
        Some("sync-collection") => Ok(DavReport::SyncCollection(sync_token)),
        _ => Err(anyhow!("unsupported DAV REPORT type")),
    }
}

fn parse_sync_token(path: &ParsedDavPath, token: &str) -> Result<u64> {
    let expected_prefix = format!(
        "http://kamori.app/ns/sync/{}/{}/",
        path.kind.route_prefix(),
        path.collection_id
    );
    token
        .strip_prefix(&expected_prefix)
        .ok_or_else(|| anyhow!("sync token belongs to another collection"))?
        .parse::<u64>()
        .context("invalid sync token revision")
}

fn sync_token(path: &ParsedDavPath, revision: u64) -> String {
    format!(
        "http://kamori.app/ns/sync/{}/{}/{revision}",
        path.kind.route_prefix(),
        path.collection_id
    )
}

async fn sync_changes_multistatus(
    state: &LocalBridgeState,
    path: &ParsedDavPath,
    changes: Vec<DavChange>,
    current_revision: u64,
) -> Result<String> {
    let mut latest = BTreeMap::<String, DavChange>::new();
    for change in changes {
        let should_replace = latest
            .get(&change.resource_id)
            .is_none_or(|previous| change.revision > previous.revision);
        if should_replace {
            latest.insert(change.resource_id.clone(), change);
        }
    }
    let mut resources = Vec::new();
    let mut deleted = Vec::new();
    for (resource_id, change) in latest {
        if change.deleted {
            deleted.push(resource_id);
        } else if let Some(resource) = state
            .get_resource(path.kind, path.collection_id.clone(), resource_id.clone())
            .await?
        {
            resources.push(resource);
        } else {
            deleted.push(resource_id);
        }
    }
    Ok(sync_multistatus(
        path,
        &resources,
        &deleted,
        current_revision,
    ))
}

async fn collection_exists(state: &LocalBridgeState, collection_id: &str) -> bool {
    state
        .registered_collection_ids()
        .await
        .iter()
        .any(|candidate| candidate == collection_id)
}

fn validate_payload(kind: DavResourceKind, payload: &str) -> Result<()> {
    let valid = match kind {
        DavResourceKind::Contact => {
            payload.contains("BEGIN:VCARD") && payload.contains("END:VCARD")
        }
        DavResourceKind::Calendar => {
            payload.contains("BEGIN:VCALENDAR") && payload.contains("END:VCALENDAR")
        }
        DavResourceKind::Note => false,
    };
    if valid {
        Ok(())
    } else {
        Err(anyhow!(
            "payload is not a complete vCard/iCalendar resource"
        ))
    }
}

fn resource_id_from_href(path: &ParsedDavPath, href: &str) -> Option<String> {
    let href_path = url::Url::parse(href)
        .ok()
        .map(|url| url.path().to_string())
        .unwrap_or_else(|| href.split(['?', '#']).next().unwrap_or(href).to_string());
    let prefix = format!("/{}/{}/", path.kind.route_prefix(), path.collection_id);
    let resource_id = href_path.strip_prefix(&prefix)?;
    if resource_id.is_empty() || resource_id.contains('/') {
        None
    } else {
        Some(resource_id.to_string())
    }
}

fn root_multistatus() -> String {
    let mut xml = xml_open();
    xml.push_str("<d:response><d:href>/</d:href><d:propstat><d:prop>");
    xml.push_str("<d:resourcetype><d:collection/></d:resourcetype>");
    xml.push_str(&format!(
        "<d:current-user-principal><d:href>{PRINCIPAL_PATH}</d:href></d:current-user-principal>"
    ));
    xml.push_str(ok_propstat_end());
    xml.push_str(xml_close());
    xml
}

fn principal_multistatus() -> String {
    let mut xml = xml_open();
    xml.push_str(&format!(
        "<d:response><d:href>{PRINCIPAL_PATH}</d:href><d:propstat><d:prop>"
    ));
    xml.push_str("<d:resourcetype><d:principal/></d:resourcetype>");
    xml.push_str("<d:displayname>Kamori</d:displayname>");
    xml.push_str("<c:calendar-home-set><d:href>/caldav/</d:href></c:calendar-home-set>");
    xml.push_str(
        "<card:addressbook-home-set><d:href>/carddav/</d:href></card:addressbook-home-set>",
    );
    xml.push_str(ok_propstat_end());
    xml.push_str(xml_close());
    xml
}

fn home_multistatus(kind: DavResourceKind, collections: &[String]) -> String {
    let mut xml = xml_open();
    let home = kind.home_path();
    xml.push_str(&format!(
        "<d:response><d:href>{home}</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype><d:displayname>Kamori</d:displayname>"
    ));
    xml.push_str(ok_propstat_end());
    for collection in collections {
        let parsed = ParsedDavPath {
            kind,
            collection_id: collection.clone(),
            resource_id: None,
        };
        push_collection_response(&mut xml, &parsed, 0);
    }
    xml.push_str(xml_close());
    xml
}

fn collection_multistatus(
    path: &ParsedDavPath,
    resources: &[LocalResource],
    revision: u64,
    include_collection_members: bool,
    include_payload: bool,
) -> String {
    let mut xml = xml_open();
    push_collection_response(&mut xml, path, revision);
    if include_collection_members || include_payload {
        for resource in resources {
            push_resource_response(&mut xml, resource, include_payload);
        }
    }
    xml.push_str(xml_close());
    xml
}

fn resource_multistatus(resource: &LocalResource, include_payload: bool) -> String {
    let mut xml = xml_open();
    push_resource_response(&mut xml, resource, include_payload);
    xml.push_str(xml_close());
    xml
}

fn multiget_xml(resources: &[LocalResource], missing: &[String]) -> String {
    let mut xml = xml_open();
    for resource in resources {
        push_resource_response(&mut xml, resource, true);
    }
    for href in missing {
        xml.push_str(&format!(
            "<d:response><d:href>{}</d:href><d:status>HTTP/1.1 404 Not Found</d:status></d:response>",
            xml_escape(href)
        ));
    }
    xml.push_str(xml_close());
    xml
}

fn sync_multistatus(
    path: &ParsedDavPath,
    resources: &[LocalResource],
    deleted: &[String],
    revision: u64,
) -> String {
    let mut xml = xml_open();
    for resource in resources {
        push_resource_response(&mut xml, resource, false);
    }
    for resource_id in deleted {
        let href = resource_href(path.kind, &path.collection_id, resource_id);
        xml.push_str(&format!(
            "<d:response><d:href>{href}</d:href><d:status>HTTP/1.1 404 Not Found</d:status></d:response>"
        ));
    }
    xml.push_str(&format!(
        "<d:sync-token>{}</d:sync-token>",
        xml_escape(&sync_token(path, revision))
    ));
    xml.push_str(xml_close());
    xml
}

fn push_collection_response(xml: &mut String, path: &ParsedDavPath, revision: u64) {
    let href = format!(
        "/{}/{}/",
        path.kind.route_prefix(),
        xml_escape(&path.collection_id)
    );
    xml.push_str(&format!(
        "<d:response><d:href>{href}</d:href><d:propstat><d:prop><d:displayname>{}</d:displayname>",
        xml_escape(&path.collection_id)
    ));
    match path.kind {
        DavResourceKind::Calendar => {
            xml.push_str("<d:resourcetype><d:collection/><c:calendar/></d:resourcetype>");
            xml.push_str("<c:supported-calendar-component-set><c:comp name=\"VEVENT\"/><c:comp name=\"VTODO\"/></c:supported-calendar-component-set>");
        }
        DavResourceKind::Contact => {
            xml.push_str("<d:resourcetype><d:collection/><card:addressbook/></d:resourcetype>");
        }
        DavResourceKind::Note => return,
    }
    xml.push_str("<d:supported-report-set><d:supported-report><d:report><d:sync-collection/></d:report></d:supported-report>");
    if path.kind == DavResourceKind::Calendar {
        xml.push_str("<d:supported-report><d:report><c:calendar-query/></d:report></d:supported-report><d:supported-report><d:report><c:calendar-multiget/></d:report></d:supported-report>");
    } else {
        xml.push_str("<d:supported-report><d:report><card:addressbook-query/></d:report></d:supported-report><d:supported-report><d:report><card:addressbook-multiget/></d:report></d:supported-report>");
    }
    xml.push_str("</d:supported-report-set>");
    xml.push_str(&format!(
        "<d:sync-token>{}</d:sync-token><cs:getctag>{revision}</cs:getctag>",
        xml_escape(&sync_token(path, revision))
    ));
    xml.push_str(ok_propstat_end());
}

fn push_resource_response(xml: &mut String, resource: &LocalResource, include_payload: bool) {
    let href = resource_href(
        resource.kind,
        &resource.collection_id,
        &resource.resource_id,
    );
    xml.push_str(&format!(
        "<d:response><d:href>{href}</d:href><d:propstat><d:prop><d:getetag>\"{}\"</d:getetag><d:getcontenttype>{}</d:getcontenttype>",
        xml_escape(&resource.etag),
        resource.kind.content_type()
    ));
    if include_payload {
        let payload = xml_escape(&resource.payload);
        match resource.kind {
            DavResourceKind::Calendar => {
                xml.push_str(&format!("<c:calendar-data>{payload}</c:calendar-data>"));
            }
            DavResourceKind::Contact => {
                xml.push_str(&format!("<card:address-data>{payload}</card:address-data>"));
            }
            DavResourceKind::Note => {}
        }
    }
    xml.push_str(ok_propstat_end());
}

fn resource_href(kind: DavResourceKind, collection_id: &str, resource_id: &str) -> String {
    format!(
        "/{}/{}/{}",
        kind.route_prefix(),
        xml_escape(collection_id),
        xml_escape(resource_id)
    )
}

fn xml_open() -> String {
    r#"<?xml version="1.0" encoding="utf-8"?><d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav" xmlns:card="urn:ietf:params:xml:ns:carddav" xmlns:cs="http://calendarserver.org/ns/">"#.to_string()
}

fn xml_close() -> &'static str {
    "</d:multistatus>"
}

fn ok_propstat_end() -> &'static str {
    "</d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>"
}

fn xml_response(status: StatusCode, xml: String) -> Response {
    let mut response = (status, xml).into_response();
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/xml; charset=utf-8"),
    );
    response
}

fn valid_sync_token_error() -> Response {
    xml_response(
        StatusCode::CONFLICT,
        r#"<?xml version="1.0" encoding="utf-8"?><d:error xmlns:d="DAV:"><d:valid-sync-token/></d:error>"#.to_string(),
    )
}

fn redirect_response(location: &str) -> Response {
    let mut response = StatusCode::MOVED_PERMANENTLY.into_response();
    response.headers_mut().insert(
        LOCATION,
        HeaderValue::from_str(location).unwrap_or(HeaderValue::from_static("/")),
    );
    response
}

fn insert_etag(response: &mut Response, etag: &str) {
    response.headers_mut().insert(
        ETAG,
        HeaderValue::from_str(&format!("\"{etag}\""))
            .unwrap_or(HeaderValue::from_static("\"invalid-etag\"")),
    );
}

fn render_put_message(put: &PutResult) -> String {
    if put.outcome == UpsertOutcome::IgnoredStale {
        "local cache rejected an older projection".to_string()
    } else if put.cloud_pushed {
        format!(
            "stored locally and pushed to cloud (space_seq={})",
            put.cloud_space_seq.unwrap_or(0)
        )
    } else {
        "stored locally, cloud push pending".to_string()
    }
}

fn parse_etag_header(headers: &HeaderMap) -> Option<String> {
    headers
        .get(IF_MATCH)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .map(|value| value.trim_start_matches("W/").trim_matches('"').to_string())
}

fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

trait DavKindPath {
    fn home_path(self) -> &'static str;
}

impl DavKindPath for DavResourceKind {
    fn home_path(self) -> &'static str {
        match self {
            Self::Calendar => "/caldav/",
            Self::Contact => "/carddav/",
            Self::Note => "/",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_discovery_and_resource_paths() {
        assert!(matches!(
            parse_dav_target("/.well-known/caldav").expect("parse"),
            DavTarget::WellKnown(DavResourceKind::Calendar)
        ));
        let DavTarget::Resource(parsed) =
            parse_dav_target("/carddav/team/alice.vcf").expect("parse")
        else {
            panic!("expected resource");
        };
        assert_eq!(parsed.kind, DavResourceKind::Contact);
        assert_eq!(parsed.collection_id, "team");
        assert_eq!(parsed.resource_id.as_deref(), Some("alice.vcf"));
    }

    #[test]
    fn report_parser_is_namespace_prefix_independent() {
        let report = parse_report(
            br#"<x:calendar-multiget xmlns:x="urn:ietf:params:xml:ns:caldav" xmlns:z="DAV:"><z:href>/caldav/a/one.ics</z:href></x:calendar-multiget>"#,
        )
        .expect("parse report");
        assert_eq!(
            report,
            DavReport::MultiGet(vec!["/caldav/a/one.ics".to_string()])
        );
    }

    #[test]
    fn sync_tokens_are_bound_to_collection_and_kind() {
        let path = ParsedDavPath {
            kind: DavResourceKind::Calendar,
            collection_id: "space".to_string(),
            resource_id: None,
        };
        let token = sync_token(&path, 42);
        assert_eq!(parse_sync_token(&path, &token).expect("token"), 42);
        assert!(parse_sync_token(&path, "http://kamori.app/ns/sync/carddav/space/42").is_err());
    }

    #[test]
    fn etag_parser_accepts_quoted_weak_and_wildcard_values() {
        let mut headers = HeaderMap::new();
        headers.insert(IF_MATCH, HeaderValue::from_static("W/\"abc\""));
        assert_eq!(parse_etag_header(&headers).as_deref(), Some("abc"));
        headers.insert(IF_MATCH, HeaderValue::from_static("*"));
        assert_eq!(parse_etag_header(&headers).as_deref(), Some("*"));
    }

    #[test]
    fn rejects_doctype_and_oversized_reports() {
        assert!(parse_report(b"<!DOCTYPE x><x/>").is_err());
        assert!(parse_report(&vec![b'x'; MAX_REPORT_BYTES + 1]).is_err());
    }
}
