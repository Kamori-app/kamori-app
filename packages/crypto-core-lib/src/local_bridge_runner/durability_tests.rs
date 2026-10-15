//! Process-crash coverage, not a claim of physical power-loss durability.
use super::*;
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

const KEY: &str = "synthetic-crash-cache-key";
const SPACE: Uuid = Uuid::from_u128(41);
const RESOURCE: Uuid = Uuid::from_u128(42);
const ATTEMPT: Uuid = Uuid::from_u128(43);

fn config(path: PathBuf, endpoint: &str, refresh: &str) -> LocalBridgeConfig {
    LocalBridgeConfig::new(path, endpoint, "A0")
        .with_sqlite_key(KEY)
        .with_refresh_token(refresh)
        .with_device_identity(LocalDeviceIdentity {
            device_id: Uuid::from_u128(44),
            signing_private_key: [7; 32],
        })
}

// Invoked in a fresh OS process by the parent test. Never exits normally after
// the acknowledgement: the parent kills it without running Rust destructors.
#[tokio::test]
async fn crash_child() {
    let Ok(path) = std::env::var("KAMORI_DURABILITY_CHILD_PATH") else {
        return;
    };
    let endpoint = std::env::var("KAMORI_DURABILITY_CHILD_ENDPOINT").unwrap();
    let mode = std::env::var("KAMORI_DURABILITY_CHILD_MODE").unwrap();
    if mode == "pending" {
        let store = RefreshCredentialStore::start_session(
            &path,
            KEY.into(),
            &endpoint,
            "alice",
            "A0",
            "T0",
        )
        .unwrap();
        assert_eq!(store.begin_with_fallback("T0", ATTEMPT).unwrap(), ATTEMPT);
        // Keep the SQLCipher database open across the following committed writes.
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection.pragma_update(None, "key", KEY).unwrap();
        connection
            .query_row("SELECT count(*) FROM sqlite_master", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap();
        let runner = LocalBridgeRunner::new(config(PathBuf::from(&path), &endpoint, "T0")).unwrap();
        runner
            .register_collection_key_epoch_from(SPACE.to_string(), 1, [9; 32], 0)
            .await
            .unwrap();
        runner
            .upsert_pim_item(
                SPACE,
                RESOURCE,
                PimResourceKind::Task,
                BTreeMap::from([(
                    "title".into(),
                    PimValue::Text("Committed before SIGKILL".into()),
                )]),
            )
            .await
            .unwrap();
        assert_eq!(
            runner.state.cache.list_queued_operations().unwrap().len(),
            1
        );
        // Pull through the real verifier/materializer without retrying the
        // deliberately unavailable upload, retaining the local durable outbox.
        runner
            .state
            .sync_space(SPACE, &BTreeMap::from([(1, [9; 32])]))
            .await
            .unwrap();
        assert_eq!(runner.space_cursor(SPACE).await.unwrap(), 73);
        println!("committed:{mode}");
        std::io::stdout().flush().unwrap();
        loop {
            std::thread::park();
        }
    } else {
        let store =
            RefreshCredentialStore::open(&path, KEY.into(), &endpoint, "T0", "alice").unwrap();
        assert_eq!(store.begin("T0").unwrap(), ATTEMPT);
        store.commit("T0", "A1", "T1").unwrap();
        store.commit("T1", "A2", "T2").unwrap();
        println!("committed:{mode}");
        std::io::stdout().flush().unwrap();
        loop {
            std::thread::park();
        }
    }
}

struct KillOnDrop(Child);
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn crash_after_commit(path: &std::path::Path, endpoint: &str, mode: &str) {
    let mut child = KillOnDrop(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "local_bridge_runner::durability_tests::crash_child",
                "--nocapture",
            ])
            .env("KAMORI_DURABILITY_CHILD_PATH", path)
            .env("KAMORI_DURABILITY_CHILD_ENDPOINT", endpoint)
            .env("KAMORI_DURABILITY_CHILD_MODE", mode)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let reader = BufReader::new(child.0.stdout.take().unwrap());
    let expected = format!("committed:{mode}");
    assert!(
        reader
            .lines()
            .map(|line| line.unwrap())
            .any(|line| line == expected),
        "child must acknowledge the committed production write before it is killed"
    );
    child.0.kill().unwrap();
    let status = child.0.wait().unwrap();
    assert!(!status.success());
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(status.signal(), Some(9), "actual abrupt SIGKILL");
    }
}

#[tokio::test]
async fn encrypted_commits_survive_sigkill_and_reject_unsupported_old_credentials() {
    let directory = std::env::temp_dir().join(format!("kamori-crash-{}", Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("synthetic.sqlite3");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let probe = listener.try_clone().unwrap();
    let requests = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let captured = requests.clone();
    let stopped = stop.clone();
    let server = std::thread::spawn(move || {
        let mut uploaded = None;
        while !stopped.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut socket, _)) => {
                    socket.set_nonblocking(false).unwrap();
                    socket
                        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                        .unwrap();
                    let mut bytes = Vec::new();
                    let mut buffer = [0; 8192];
                    let end = loop {
                        let count = socket.read(&mut buffer).unwrap();
                        assert!(count > 0);
                        bytes.extend_from_slice(&buffer[..count]);
                        if let Some(end) = bytes.windows(4).position(|chunk| chunk == b"\r\n\r\n") {
                            break end + 4;
                        }
                    };
                    let headers = String::from_utf8_lossy(&bytes[..end]).into_owned();
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|value| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    while bytes.len() < end + length {
                        let count = socket.read(&mut buffer).unwrap();
                        assert!(count > 0);
                        bytes.extend_from_slice(&buffer[..count]);
                    }
                    captured.fetch_add(1, Ordering::SeqCst);
                    let (status, body) = if headers.starts_with("POST /operations ") {
                        #[derive(Deserialize)]
                        struct Append {
                            envelope: OperationEnvelopeV1,
                        }
                        let request = Append::deserialize(
                            &mut rmp_serde::Deserializer::new(&bytes[end..end + length])
                                .with_human_readable(),
                        )
                        .unwrap();
                        uploaded = Some(request.envelope);
                        ("503 Service Unavailable", Vec::new())
                    } else if headers.starts_with(&format!("GET /spaces/{SPACE}/devices ")) {
                        #[derive(Serialize)]
                        struct Devices {
                            devices: Vec<cloud::CloudSpaceDevice>,
                        }
                        (
                            "200 OK",
                            msgpack(&Devices {
                                devices: vec![cloud::CloudSpaceDevice {
                                    device_id: Uuid::from_u128(44),
                                    signing_public_key: SigningKey::from_bytes(&[7; 32])
                                        .verifying_key()
                                        .to_bytes()
                                        .to_vec(),
                                }],
                            }),
                        )
                    } else {
                        assert!(headers.starts_with("GET /operations?"));
                        let after = headers.contains("since=73&");
                        assert!(after || headers.contains("since=0&"));
                        #[derive(Serialize)]
                        struct Page {
                            operations: Vec<cloud::CloudStoredOperation>,
                            next_cursor: u64,
                        }
                        (
                            "200 OK",
                            msgpack(&Page {
                                operations: if after {
                                    vec![]
                                } else {
                                    vec![cloud::CloudStoredOperation {
                                        space_seq: 73,
                                        received_at_unix_ms: 1,
                                        envelope: uploaded
                                            .clone()
                                            .expect("local operation reached peer"),
                                    }]
                                },
                                // A stale subsequent page must never move applied cursor backwards.
                                next_cursor: if after { 12 } else { 73 },
                            }),
                        )
                    };
                    write!(socket, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: application/msgpack\r\nConnection: close\r\n\r\n", body.len()).unwrap();
                    socket.write_all(&body).unwrap();
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(2))
                }
                Err(error) => panic!("{error}"),
            }
        }
    });
    crash_after_commit(&path, &endpoint, "pending");
    assert_eq!(
        requests.load(Ordering::SeqCst),
        4,
        "one upload, device directory and two cursor pages"
    );
    assert_ne!(&std::fs::read(&path).unwrap()[..16], b"SQLite format 3\0");
    let plaintext = rusqlite::Connection::open(&path).unwrap();
    assert!(
        plaintext
            .query_row("SELECT count(*) FROM sqlite_master", [], |row| row
                .get::<_, i64>(0))
            .is_err()
    );
    drop(plaintext);
    let store = RefreshCredentialStore::open(&path, KEY.into(), &endpoint, "T0", "alice").unwrap();
    assert_eq!(
        store.begin_with_fallback("T0", Uuid::new_v4()).unwrap(),
        ATTEMPT
    );
    assert!(RefreshCredentialStore::open(&path, KEY.into(), &endpoint, "T0", "bob").is_err());
    assert!(
        RefreshCredentialStore::open(&path, KEY.into(), "http://127.0.0.1:9", "T0", "alice")
            .is_err()
    );
    let runner = LocalBridgeRunner::new(config(path.clone(), &endpoint, "T0")).unwrap();
    let resources = runner
        .list_cached_resources(SPACE, DavResourceKind::Calendar)
        .await
        .unwrap();
    assert_eq!(resources.len(), 1);
    assert!(
        resources[0]
            .payload
            .contains("SUMMARY:Committed before SIGKILL")
    );
    let queued = runner.state.cache.list_queued_operations().unwrap();
    assert_eq!(queued.len(), 1);
    println!(
        "durability: pending={ATTEMPT}, queued={}, applied_cursor=73",
        queued[0].client_op_id
    );
    let operation = PimOperationV1::decode(&queued[0].open(&[9; 32]).unwrap()).unwrap();
    assert!(matches!(operation, PimOperationV1::Upsert(ref item) if item.resource_id == RESOURCE));
    assert_eq!(runner.space_cursor(SPACE).await.unwrap(), 73);
    runner
        .register_collection_key_epoch_from(SPACE.to_string(), 1, [9; 32], 12)
        .await
        .unwrap();
    assert_eq!(runner.space_cursor(SPACE).await.unwrap(), 73);
    drop(runner);
    drop(store);
    crash_after_commit(&path, &endpoint, "rotated");
    assert!(LocalBridgeRunner::new(config(path.clone(), &endpoint, "T0")).is_err());
    stop.store(true, Ordering::SeqCst);
    server.join().unwrap();
    assert_eq!(
        probe.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock,
        "no unsupported-token connection may be queued for acceptance"
    );
    assert_eq!(
        requests.load(Ordering::SeqCst),
        4,
        "unsupported secure-store token must emit zero additional requests"
    );
    let store = RefreshCredentialStore::open(&path, KEY.into(), &endpoint, "T1", "alice").unwrap();
    assert_eq!(
        store.recover("T1").unwrap(),
        Some(("A2".into(), "T2".into()))
    );
    let runner = LocalBridgeRunner::new(config(path.clone(), &endpoint, "T2")).unwrap();
    assert_eq!(runner.space_cursor(SPACE).await.unwrap(), 73);
    assert_eq!(
        runner.state.cache.list_queued_operations().unwrap()[0].client_op_id,
        queued[0].client_op_id
    );
    drop(runner);
    drop(store);
    std::fs::remove_dir_all(directory).unwrap();
}

fn msgpack(value: &impl Serialize) -> Vec<u8> {
    let mut bytes = Vec::new();
    value
        .serialize(
            &mut rmp_serde::Serializer::new(&mut bytes)
                .with_struct_map()
                .with_human_readable(),
        )
        .unwrap();
    bytes
}
