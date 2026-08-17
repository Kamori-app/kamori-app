#![cfg(all(feature = "local-bridge", feature = "http-reqwest"))]

use std::path::{Path, PathBuf};

use crypto_core_lib::local_bridge_runner::{
    LocalBridgeConfig, LocalBridgeRunner, LocalDeviceIdentity,
};
use reqwest::{
    Client, Method, Response, StatusCode,
    header::{ALLOW, CONTENT_TYPE, ETAG, IF_MATCH, IF_NONE_MATCH, LOCATION, WWW_AUTHENTICATE},
    redirect::Policy,
};
use uuid::Uuid;

const DAV_USERNAME: &str = "kamori-dav";
const DAV_PASSWORD: &str = "local-only-test-secret";
const CALENDAR_V1: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:event-1\r\nSUMMARY:Initial\r\nDTSTART:20260817T120000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
const CALENDAR_V2: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:event-1\r\nSUMMARY:Updated\r\nDTSTART:20260817T120000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
const CONTACT: &str =
    "BEGIN:VCARD\r\nVERSION:4.0\r\nUID:contact-1\r\nFN:Ada Lovelace\r\nEND:VCARD\r\n";

struct TestBridge {
    runner: LocalBridgeRunner,
    client: Client,
    base_url: String,
    space_id: Uuid,
    database_path: PathBuf,
}

impl TestBridge {
    async fn start() -> Self {
        let database_path = temporary_database_path();
        let space_id = Uuid::new_v4();
        let config = LocalBridgeConfig::new(
            database_path.clone(),
            "http://127.0.0.1:9",
            "offline-test-access-token",
        )
        .with_bind_addr("127.0.0.1:0".parse().expect("loopback address"))
        .with_device_identity(LocalDeviceIdentity {
            device_id: Uuid::new_v4(),
            signing_private_key: [7_u8; 32],
        })
        .with_dav_credentials(DAV_USERNAME, DAV_PASSWORD);
        let runner = LocalBridgeRunner::new(config).expect("create local bridge");
        runner
            .register_collection_key_epoch(space_id.to_string(), 1, [11_u8; 32])
            .await;
        runner.start().await.expect("start local DAV bridge");
        let address = runner.local_addr().await.expect("listener address");
        assert_ne!(address.port(), 0, "the OS must allocate an ephemeral port");
        let client = Client::builder()
            .redirect(Policy::none())
            .build()
            .expect("build HTTP client");
        Self {
            runner,
            client,
            base_url: format!("http://{address}"),
            space_id,
            database_path,
        }
    }

    fn request(&self, method: &str, path: &str) -> reqwest::RequestBuilder {
        self.client
            .request(dav_method(method), format!("{}{path}", self.base_url))
            .basic_auth(DAV_USERNAME, Some(DAV_PASSWORD))
    }

    async fn send(&self, method: &str, path: &str) -> Response {
        self.request(method, path)
            .send()
            .await
            .expect("send DAV request")
    }

    async fn stop(self) {
        self.runner.stop().await.expect("stop local DAV bridge");
        assert!(self.runner.local_addr().await.is_none());
        cleanup_database(&self.database_path);
    }
}

#[tokio::test]
async fn enforces_loopback_authentication_and_supported_methods() {
    let missing_credentials_path = temporary_database_path();
    let missing_credentials = LocalBridgeRunner::new(
        LocalBridgeConfig::new(
            missing_credentials_path.clone(),
            "http://127.0.0.1:9",
            "test-token",
        )
        .with_bind_addr("127.0.0.1:0".parse().expect("loopback address")),
    )
    .expect("create bridge without credentials");
    assert!(missing_credentials.start().await.is_err());
    cleanup_database(&missing_credentials_path);

    let public_bind_path = temporary_database_path();
    let public_bind = LocalBridgeRunner::new(
        LocalBridgeConfig::new(public_bind_path.clone(), "http://127.0.0.1:9", "test-token")
            .with_bind_addr("0.0.0.0:0".parse().expect("public address"))
            .with_dav_credentials(DAV_USERNAME, DAV_PASSWORD),
    )
    .expect("create bridge with public bind");
    assert!(public_bind.start().await.is_err());
    cleanup_database(&public_bind_path);

    let bridge = TestBridge::start().await;
    assert_eq!(bridge.runner.bind_addr().port(), 0);

    let unauthenticated = bridge
        .client
        .request(dav_method("OPTIONS"), format!("{}/", bridge.base_url))
        .send()
        .await
        .expect("send unauthenticated request");
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
    assert!(
        unauthenticated
            .headers()
            .get(WWW_AUTHENTICATE)
            .expect("auth challenge")
            .to_str()
            .expect("valid challenge")
            .contains("Kamori local DAV")
    );

    let wrong_password = bridge
        .client
        .request(dav_method("OPTIONS"), format!("{}/", bridge.base_url))
        .basic_auth(DAV_USERNAME, Some("wrong"))
        .send()
        .await
        .expect("send wrong-password request");
    assert_eq!(wrong_password.status(), StatusCode::UNAUTHORIZED);

    let options = bridge.send("OPTIONS", "/").await;
    assert_eq!(options.status(), StatusCode::NO_CONTENT);
    let allow = options
        .headers()
        .get(ALLOW)
        .expect("Allow header")
        .to_str()
        .expect("valid Allow header");
    assert!(allow.contains("PROPFIND"));
    assert!(allow.contains("DELETE"));
    let dav = options
        .headers()
        .get("dav")
        .expect("DAV capability header")
        .to_str()
        .expect("valid DAV header");
    assert!(dav.contains("calendar-access"));
    assert!(dav.contains("addressbook"));
    assert!(dav.contains("sync-collection"));

    for method in ["MKCOL", "MKCALENDAR", "PROPPATCH"] {
        assert_eq!(
            bridge.send(method, "/caldav/").await.status(),
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(
        bridge.send("PATCH", "/").await.status(),
        StatusCode::METHOD_NOT_ALLOWED
    );

    bridge.stop().await;
}

#[tokio::test]
async fn discovers_principal_homes_and_registered_collections() {
    let bridge = TestBridge::start().await;

    for (well_known, home) in [
        ("/.well-known/caldav", "/caldav/"),
        ("/.well-known/carddav", "/carddav/"),
    ] {
        let response = bridge.send("GET", well_known).await;
        assert_eq!(response.status(), StatusCode::MOVED_PERMANENTLY);
        assert_eq!(
            response
                .headers()
                .get(LOCATION)
                .expect("redirect location")
                .to_str()
                .expect("valid location"),
            home
        );
    }

    let root = propfind(&bridge, "/", "0").await;
    assert!(root.contains("/principals/kamori/"));
    let principal = propfind(&bridge, "/principals/kamori/", "0").await;
    assert!(principal.contains("/caldav/"));
    assert!(principal.contains("/carddav/"));

    for home in ["/caldav/", "/carddav/"] {
        let body = propfind(&bridge, home, "1").await;
        assert!(body.contains(&bridge.space_id.to_string()));
    }

    let collection = propfind(&bridge, &format!("/caldav/{}/", bridge.space_id), "0").await;
    assert!(collection.contains("calendar-query"));
    assert!(collection.contains("sync-token"));

    let invalid_depth = bridge
        .request("PROPFIND", "/")
        .header("depth", "infinity")
        .send()
        .await
        .expect("send invalid Depth request");
    assert_eq!(invalid_depth.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        bridge
            .send("PROPFIND", &format!("/caldav/{}/", Uuid::new_v4()))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );

    bridge.stop().await;
}

#[tokio::test]
async fn applies_calendar_preconditions_reports_and_sync_tombstones() {
    let bridge = TestBridge::start().await;
    let collection_path = format!("/caldav/{}/", bridge.space_id);
    let resource_path = format!("{collection_path}event-1.ics");

    let invalid = bridge
        .request("PUT", &resource_path)
        .body("not a calendar")
        .send()
        .await
        .expect("send invalid calendar");
    assert_eq!(invalid.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

    let created = bridge
        .request("PUT", &resource_path)
        .header(IF_NONE_MATCH, "*")
        .body(CALENDAR_V1)
        .send()
        .await
        .expect("create calendar resource");
    assert_eq!(created.status(), StatusCode::CREATED);
    let first_etag = header_string(&created, ETAG);

    let get = bridge.send("GET", &resource_path).await;
    assert_eq!(get.status(), StatusCode::OK);
    assert_eq!(header_string(&get, ETAG), first_etag);
    assert!(header_string(&get, CONTENT_TYPE).starts_with("text/calendar"));
    assert_eq!(get.text().await.expect("calendar body"), CALENDAR_V1);

    let head = bridge.send("HEAD", &resource_path).await;
    assert_eq!(head.status(), StatusCode::OK);
    assert_eq!(header_string(&head, ETAG), first_etag);
    assert!(head.bytes().await.expect("HEAD body").is_empty());

    let duplicate_create = bridge
        .request("PUT", &resource_path)
        .header(IF_NONE_MATCH, "*")
        .body(CALENDAR_V2)
        .send()
        .await
        .expect("repeat create");
    assert_eq!(duplicate_create.status(), StatusCode::PRECONDITION_FAILED);

    let unguarded_update = bridge
        .request("PUT", &resource_path)
        .body(CALENDAR_V2)
        .send()
        .await
        .expect("send unguarded update");
    assert_eq!(unguarded_update.status(), StatusCode::PRECONDITION_REQUIRED);
    let stale_update = bridge
        .request("PUT", &resource_path)
        .header(IF_MATCH, "\"stale\"")
        .body(CALENDAR_V2)
        .send()
        .await
        .expect("send stale update");
    assert_eq!(stale_update.status(), StatusCode::PRECONDITION_FAILED);

    let sync_before_update = sync_report(&bridge, &collection_path, None).await;
    assert!(sync_before_update.0.contains("event-1.ics"));

    let updated = bridge
        .request("PUT", &resource_path)
        .header(IF_MATCH, first_etag.as_str())
        .body(CALENDAR_V2)
        .send()
        .await
        .expect("update calendar resource");
    assert_eq!(updated.status(), StatusCode::NO_CONTENT);
    let second_etag = header_string(&updated, ETAG);
    assert_ne!(second_etag, first_etag);

    let query = bridge
        .request("REPORT", &collection_path)
        .body(
            r#"<c:calendar-query xmlns:c="urn:ietf:params:xml:ns:caldav" xmlns:d="DAV:"><d:prop><d:getetag/><c:calendar-data/></d:prop></c:calendar-query>"#,
        )
        .send()
        .await
        .expect("calendar query");
    assert_eq!(query.status(), StatusCode::MULTI_STATUS);
    assert!(
        query
            .text()
            .await
            .expect("query body")
            .contains("SUMMARY:Updated")
    );

    let multiget = bridge
        .request("REPORT", &collection_path)
        .body(format!(
            r#"<c:calendar-multiget xmlns:c="urn:ietf:params:xml:ns:caldav" xmlns:d="DAV:"><d:href>{resource_path}</d:href><d:href>{collection_path}missing.ics</d:href></c:calendar-multiget>"#
        ))
        .send()
        .await
        .expect("calendar multiget");
    assert_eq!(multiget.status(), StatusCode::MULTI_STATUS);
    let multiget_body = multiget.text().await.expect("multiget body");
    assert!(multiget_body.contains("SUMMARY:Updated"));
    assert!(multiget_body.contains("missing.ics"));
    assert!(multiget_body.contains("404 Not Found"));

    let sync_after_update =
        sync_report(&bridge, &collection_path, Some(&sync_before_update.1)).await;
    assert!(sync_after_update.0.contains("event-1.ics"));
    assert_ne!(sync_after_update.1, sync_before_update.1);

    let unguarded_delete = bridge.send("DELETE", &resource_path).await;
    assert_eq!(unguarded_delete.status(), StatusCode::PRECONDITION_REQUIRED);
    let stale_delete = bridge
        .request("DELETE", &resource_path)
        .header(IF_MATCH, "\"stale\"")
        .send()
        .await
        .expect("send stale delete");
    assert_eq!(stale_delete.status(), StatusCode::PRECONDITION_FAILED);
    let deleted = bridge
        .request("DELETE", &resource_path)
        .header(IF_MATCH, second_etag.as_str())
        .send()
        .await
        .expect("delete calendar resource");
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        bridge.send("GET", &resource_path).await.status(),
        StatusCode::NOT_FOUND
    );

    let sync_after_delete =
        sync_report(&bridge, &collection_path, Some(&sync_after_update.1)).await;
    assert!(sync_after_delete.0.contains("event-1.ics"));
    assert!(sync_after_delete.0.contains("404 Not Found"));

    let invalid_token = sync_report_response(
        &bridge,
        &collection_path,
        Some("http://kamori.app/ns/sync/caldav/not-this-collection/1"),
    )
    .await;
    assert_eq!(invalid_token.status(), StatusCode::CONFLICT);
    assert!(
        invalid_token
            .text()
            .await
            .expect("invalid-token body")
            .contains("valid-sync-token")
    );

    bridge.stop().await;
}

#[tokio::test]
async fn projects_carddav_resources_and_reports() {
    let bridge = TestBridge::start().await;
    let collection_path = format!("/carddav/{}/", bridge.space_id);
    let resource_path = format!("{collection_path}contact-1.vcf");

    let created = bridge
        .request("PUT", &resource_path)
        .header(IF_NONE_MATCH, "*")
        .body(CONTACT)
        .send()
        .await
        .expect("create contact");
    assert_eq!(created.status(), StatusCode::CREATED);

    let get = bridge.send("GET", &resource_path).await;
    assert_eq!(get.status(), StatusCode::OK);
    assert!(header_string(&get, CONTENT_TYPE).starts_with("text/vcard"));
    assert_eq!(get.text().await.expect("vCard body"), CONTACT);

    for report in [
        r#"<card:addressbook-query xmlns:card="urn:ietf:params:xml:ns:carddav" xmlns:d="DAV:"><d:prop><d:getetag/><card:address-data/></d:prop></card:addressbook-query>"#.to_string(),
        format!(r#"<card:addressbook-multiget xmlns:card="urn:ietf:params:xml:ns:carddav" xmlns:d="DAV:"><d:href>{resource_path}</d:href></card:addressbook-multiget>"#),
    ] {
        let response = bridge
            .request("REPORT", &collection_path)
            .body(report)
            .send()
            .await
            .expect("send CardDAV report");
        assert_eq!(response.status(), StatusCode::MULTI_STATUS);
        assert!(
            response
                .text()
                .await
                .expect("CardDAV report body")
                .contains("FN:Ada Lovelace")
        );
    }

    bridge.stop().await;
}

async fn propfind(bridge: &TestBridge, path: &str, depth: &str) -> String {
    let response = bridge
        .request("PROPFIND", path)
        .header("depth", depth)
        .body(r#"<d:propfind xmlns:d="DAV:"><d:allprop/></d:propfind>"#)
        .send()
        .await
        .expect("send PROPFIND");
    assert_eq!(response.status(), StatusCode::MULTI_STATUS);
    assert!(header_string(&response, CONTENT_TYPE).starts_with("application/xml"));
    response.text().await.expect("PROPFIND body")
}

async fn sync_report(
    bridge: &TestBridge,
    collection_path: &str,
    token: Option<&str>,
) -> (String, String) {
    let response = sync_report_response(bridge, collection_path, token).await;
    assert_eq!(response.status(), StatusCode::MULTI_STATUS);
    let body = response.text().await.expect("sync report body");
    let token = xml_element(&body, "d:sync-token").expect("sync token in response");
    (body, token)
}

async fn sync_report_response(
    bridge: &TestBridge,
    collection_path: &str,
    token: Option<&str>,
) -> Response {
    let token = token.unwrap_or_default();
    bridge
        .request("REPORT", collection_path)
        .body(format!(
            r#"<d:sync-collection xmlns:d="DAV:"><d:sync-token>{token}</d:sync-token><d:sync-level>1</d:sync-level><d:prop><d:getetag/></d:prop></d:sync-collection>"#
        ))
        .send()
        .await
        .expect("send sync-collection REPORT")
}

fn xml_element(xml: &str, name: &str) -> Option<String> {
    let start_marker = format!("<{name}>");
    let end_marker = format!("</{name}>");
    let start = xml.find(&start_marker)? + start_marker.len();
    let end = xml[start..].find(&end_marker)? + start;
    Some(xml[start..end].to_string())
}

fn header_string(response: &Response, name: reqwest::header::HeaderName) -> String {
    response
        .headers()
        .get(name)
        .expect("response header")
        .to_str()
        .expect("valid response header")
        .to_string()
}

fn dav_method(name: &str) -> Method {
    Method::from_bytes(name.as_bytes()).expect("valid DAV method")
}

fn temporary_database_path() -> PathBuf {
    std::env::temp_dir().join(format!("kamori-dav-conformance-{}.sqlite3", Uuid::new_v4()))
}

fn cleanup_database(database_path: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let candidate = PathBuf::from(format!("{}{suffix}", database_path.display()));
        if let Err(error) = std::fs::remove_file(&candidate) {
            assert_eq!(
                error.kind(),
                std::io::ErrorKind::NotFound,
                "remove test database {}: {error}",
                candidate.display()
            );
        }
    }
}
