// Real SQLCipher and native FRB orchestration; only HTTP is synthetic.
use super::*;
use crate::local_bridge_runner::{LocalBridgeConfig, LocalBridgeRunner, RefreshCredentialStore};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use uuid::Uuid;

struct Server {
    endpoint: String,
    requests: Arc<Mutex<Vec<(String, serde_json::Value)>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    fn new() -> Self {
        Self::with_refresh_failure(false)
    }
    fn with_refresh_failure(fail_refresh: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let thread = std::thread::spawn(move || {
            let mut workers = Vec::new();
            while !stopping.load(Ordering::SeqCst) {
                let (mut stream, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(error) => panic!("{error}"),
                };
                // BSD/macOS accepts can inherit the nonblocking listener flag.
                // The worker uses blocking Read; normalize the accepted socket.
                stream.set_nonblocking(false).unwrap();
                let captured = captured.clone();
                workers.push(std::thread::spawn(move || {
                stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                let mut bytes = Vec::new();
                let mut buf = [0; 8192];
                let (end, length) = loop {
                    let n = match stream.read(&mut buf) { Ok(0) | Err(_) => return, Ok(n) => n };
                    bytes.extend_from_slice(&buf[..n]);
                    if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..end]);
                        let length = headers.lines().find_map(|s| s.to_ascii_lowercase().strip_prefix("content-length:").map(|s| s.trim().parse::<usize>().unwrap())).unwrap_or(0);
                        break (end + 4, length);
                    }
                };
                while bytes.len() < end + length { let n=stream.read(&mut buf).unwrap(); assert!(n>0); bytes.extend_from_slice(&buf[..n]); }
                let headers = String::from_utf8_lossy(&bytes[..end]);
                let line = headers.lines().next().unwrap().to_string();
                let body = if length == 0 { serde_json::Value::Null } else { rmp_serde::from_slice(&bytes[end..end+length]).unwrap() };
                captured.lock().unwrap().push((line.clone(), body));
                let refresh = line.contains("/auth/refresh");
                let authorized = (refresh && !fail_refresh) || headers.to_ascii_lowercase().contains("bearer a1") || headers.to_ascii_lowercase().contains("bearer a2");
                let payload = if refresh { serde_json::json!({"access_token":"A1", "refresh_token":"T1"}) }
                    else if line.contains("/devices") { serde_json::json!({"devices":[]}) }
                    else if line.contains("/operations") { serde_json::json!({"operations":[], "next_cursor":0}) }
                    else if line.contains("recovery-key-packages") { serde_json::json!({"packages":[]}) }
                    else { serde_json::json!({"spaces":[]}) };
                let body = rmp_serde::to_vec_named(&payload).unwrap();
                write!(stream,"HTTP/1.1 {}\r\nContent-Length: {}\r\nContent-Type: application/msgpack\r\nConnection: close\r\n\r\n",if authorized {"200 OK"} else {"401 Unauthorized"},body.len()).unwrap();
                stream.write_all(&body).unwrap();
                }));
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });
        Self {
            endpoint,
            requests,
            stop,
            thread: Some(thread),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.thread.take().unwrap().join().unwrap();
    }
}

async fn session() -> crate::frb_generated::RustOpaque<MobileSessionLease> {
    let lease = mobile_new_session_lease();
    mobile_acquire_session_lease(lease.clone()).await;
    // Simulate process loss without logout (which deliberately erases credentials).
    state::MOBILE_RUNNER.lock().await.take();
    state::MOBILE_BRIDGE_RUNTIME.lock().await.last_config = None;
    state::MOBILE_COLLECTION_KEYS.lock().await.clear();
    state::MOBILE_DEVICE_SECRETS.lock().await.take();
    state::set_mobile_refresh_token(None).await;
    lease
}
fn path() -> String {
    std::env::temp_dir()
        .join(format!("mobile-native-{}.sqlite3", Uuid::new_v4()))
        .to_str()
        .unwrap()
        .to_string()
}
async fn configure(
    lease: crate::frb_generated::RustOpaque<MobileSessionLease>,
    server: &Server,
    path: &str,
) -> Result<(), String> {
    mobile_configure_sync(
        lease,
        server.endpoint.clone(),
        path.into(),
        "A0".into(),
        [7; 32],
        None,
        "alice".into(),
        false,
    )
    .await
}

#[tokio::test]
async fn native_restore_rejects_token_older_than_predecessor_before_network() {
    let lease = session().await;
    let server = Server::new();
    let path = path();
    let runner = LocalBridgeRunner::new(
        LocalBridgeConfig::new(path.clone(), server.endpoint.clone(), "A0")
            .with_sqlite_key(hex::encode([7; 32]))
            .with_refresh_token("T0"),
    )
    .unwrap();
    runner
        .persist_rotated_credentials("T0".into(), "A1".into(), "T1".into())
        .await
        .unwrap();
    runner
        .persist_rotated_credentials("T1".into(), "A2".into(), "T2".into())
        .await
        .unwrap();
    drop(runner);
    mobile_import_refresh_token(lease.clone(), "T0".into(), Uuid::new_v4().to_string())
        .await
        .unwrap();
    let result = configure(lease.clone(), &server, &path).await;
    assert!(
        result.is_err(),
        "unsupported T0 must require fresh signin before provisioning"
    );
    assert!(server.requests.lock().unwrap().is_empty());
    let store = RefreshCredentialStore::open(
        path.clone(),
        hex::encode([7; 32]),
        &server.endpoint,
        "T2",
        "alice",
    )
    .unwrap();
    assert_eq!(
        store.recover("T2").unwrap(),
        Some(("A2".into(), "T2".into()))
    );
    drop(lease);
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn native_cold_configure_then_import_refreshes_with_imported_attempt() {
    let lease = session().await;
    let server = Server::new();
    let path = path();
    let request = Uuid::new_v4();
    configure(lease.clone(), &server, &path).await.unwrap();
    mobile_import_refresh_token(lease.clone(), "T0".into(), request.to_string())
        .await
        .unwrap();
    mobile_register_collection_key(lease.clone(), Uuid::new_v4().to_string(), 1, 0, [8; 32])
        .await
        .unwrap();
    let result = mobile_sync_now(lease.clone()).await;
    assert!(result.is_ok(), "cold native sync must refresh: {result:?}");
    let requests = server.requests.lock().unwrap().clone();
    let refresh = requests
        .iter()
        .find(|(line, _)| line.contains("/auth/refresh"))
        .expect("refresh request");
    assert_eq!(refresh.1["refresh_token"], "T0");
    assert_eq!(refresh.1["rotation_request_id"], request.to_string());
    drop(requests);
    assert_eq!(
        mobile_export_refresh_token(lease.clone())
            .await
            .unwrap()
            .as_deref(),
        Some("T1")
    );
    assert!(
        mobile_export_refresh_rotation_request_id(lease.clone())
            .await
            .unwrap()
            .is_some()
    );
    state::MOBILE_RUNNER.lock().await.take();
    drop(lease);
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn native_predecessor_restore_reconciles_globals_and_pending_attempt() {
    let lease = session().await;
    let server = Server::new();
    let path = path();
    let store = RefreshCredentialStore::start_session(
        path.clone(),
        hex::encode([7; 32]),
        &server.endpoint,
        "alice",
        "A0",
        "T0",
    )
    .unwrap();
    store.commit("T0", "A1", "T1").unwrap();
    let pending = store.begin("T1").unwrap();
    mobile_import_refresh_token(lease.clone(), "T0".into(), Uuid::new_v4().to_string())
        .await
        .unwrap();
    configure(lease.clone(), &server, &path).await.unwrap();
    assert_eq!(
        mobile_export_refresh_token(lease.clone())
            .await
            .unwrap()
            .as_deref(),
        Some("T1")
    );
    assert_eq!(
        mobile_export_refresh_rotation_request_id(lease.clone())
            .await
            .unwrap(),
        Some(pending.to_string())
    );
    let provisioned = mobile_provision_device_and_spaces(
        lease.clone(),
        server.endpoint.clone(),
        "A0".into(),
        [9; 32],
        "android".into(),
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(provisioned.access_token, "A1");
    assert!(
        !server
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|(line, _)| line.contains("/auth/refresh"))
    );
    state::MOBILE_RUNNER.lock().await.take();
    drop(lease);
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn native_pending_attempt_survives_import_failure_and_direct_transport_retry() {
    let lease = session().await;
    let server = Server::with_refresh_failure(true);
    let path = path();
    let store = RefreshCredentialStore::start_session(
        path.clone(),
        hex::encode([7; 32]),
        &server.endpoint,
        "alice",
        "A0",
        "T0",
    )
    .unwrap();
    let pending = store.begin("T0").unwrap();
    // A lost native response leaves a durable attempt distinct from the OS copy.
    mobile_import_refresh_token(lease.clone(), "T0".into(), Uuid::new_v4().to_string())
        .await
        .unwrap();
    configure(lease.clone(), &server, &path).await.unwrap();
    mobile_register_collection_key(lease.clone(), Uuid::new_v4().to_string(), 1, 0, [8; 32])
        .await
        .unwrap();
    let sync_error = mobile_sync_now(lease.clone()).await.unwrap_err();
    assert!(
        sync_error.contains("unauthorized and refresh failed"),
        "{sync_error}"
    );
    assert_eq!(
        mobile_export_refresh_token(lease.clone())
            .await
            .unwrap()
            .as_deref(),
        Some("T0")
    );
    assert_eq!(
        mobile_export_refresh_rotation_request_id(lease.clone())
            .await
            .unwrap(),
        Some(pending.to_string())
    );
    let direct = transport::refresh_mobile_tokens(&server.endpoint, "T0").await;
    assert!(direct.unwrap_err().contains("401 Unauthorized"));
    let requests = server.requests.lock().unwrap().clone();
    let refreshes: Vec<_> = requests
        .iter()
        .filter(|(line, _)| line.contains("/auth/refresh"))
        .collect();
    assert_eq!(refreshes.len(), 2, "captured requests: {requests:?}");
    for (_, body) in refreshes {
        assert_eq!(body["refresh_token"], "T0");
        assert_eq!(body["rotation_request_id"], pending.to_string());
    }
    drop(requests);
    state::MOBILE_RUNNER.lock().await.take();
    drop(lease);
    std::fs::remove_file(path).unwrap();
}

fn encrypted_cache(path: &str) -> rusqlite::Connection {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.pragma_update(None, "key", hex::encode([7; 32]))
        .unwrap();
    conn
}

#[tokio::test]
async fn native_fresh_signin_replaces_only_credentials_preserving_pim_and_cursor() {
    let lease = session().await;
    let server = Server::new();
    let path = path();
    let store = RefreshCredentialStore::start_session(
        path.clone(),
        hex::encode([7; 32]),
        &server.endpoint,
        "alice",
        "A0",
        "T0",
    )
    .unwrap();
    store.commit("T0", "A1", "T1").unwrap();
    store.commit("T1", "A2", "T2").unwrap();
    encrypted_cache(&path).execute_batch("INSERT INTO contacts VALUES('space','alice.vcf','BEGIN:VCARD','etag',1000); INSERT INTO sync_cursors VALUES('space:space',23);").unwrap();
    mobile_import_refresh_token(lease.clone(), "T0".into(), Uuid::new_v4().to_string())
        .await
        .unwrap();
    assert!(configure(lease.clone(), &server, &path).await.is_err());
    // Successful password signin discards the old in-memory configuration.
    state::MOBILE_BRIDGE_RUNTIME.lock().await.last_config = None;
    let fresh_id = Uuid::new_v4();
    mobile_import_refresh_token(lease.clone(), "U0".into(), fresh_id.to_string())
        .await
        .unwrap();
    mobile_configure_sync(
        lease.clone(),
        server.endpoint.clone(),
        path.clone(),
        "B0".into(),
        [7; 32],
        None,
        "alice".into(),
        true,
    )
    .await
    .unwrap();
    assert_eq!(
        mobile_export_refresh_token(lease.clone())
            .await
            .unwrap()
            .as_deref(),
        Some("U0")
    );
    assert_eq!(
        mobile_export_refresh_rotation_request_id(lease.clone())
            .await
            .unwrap(),
        Some(fresh_id.to_string())
    );
    assert_eq!(
        store.recover("U0").unwrap(),
        Some(("B0".into(), "U0".into()))
    );
    assert!(store.recover("T2").unwrap().is_none());
    let conn = encrypted_cache(&path);
    assert_eq!(
        conn.query_row("SELECT vcard FROM contacts", [], |row| row
            .get::<_, String>(0))
            .unwrap(),
        "BEGIN:VCARD"
    );
    assert_eq!(
        conn.query_row(
            "SELECT last_seq_id FROM sync_cursors WHERE scope='space:space'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        23
    );
    drop(conn);
    assert!(server.requests.lock().unwrap().is_empty());
    state::MOBILE_RUNNER.lock().await.take();
    drop(lease);
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn native_restore_rejects_account_or_endpoint_mismatch_without_replacing_cache() {
    let lease = session().await;
    let server = Server::new();
    let path = path();
    let store = RefreshCredentialStore::start_session(
        path.clone(),
        hex::encode([7; 32]),
        &server.endpoint,
        "alice",
        "A1",
        "T1",
    )
    .unwrap();
    mobile_import_refresh_token(lease.clone(), "T1".into(), Uuid::new_v4().to_string())
        .await
        .unwrap();
    for (endpoint, account) in [
        (server.endpoint.clone(), "bob"),
        ("http://127.0.0.1:1".into(), "alice"),
    ] {
        assert!(
            mobile_configure_sync(
                lease.clone(),
                endpoint,
                path.clone(),
                "A1".into(),
                [7; 32],
                None,
                account.into(),
                false
            )
            .await
            .is_err()
        );
        assert_eq!(
            store.recover("T1").unwrap(),
            Some(("A1".into(), "T1".into()))
        );
    }
    assert!(server.requests.lock().unwrap().is_empty());
    state::MOBILE_RUNNER.lock().await.take();
    drop(lease);
    std::fs::remove_file(path).unwrap();
}
