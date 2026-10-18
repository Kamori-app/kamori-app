//! Real persistence schedules: removing the account lock or final generation
//! check must allow a stale capability to survive one of these orderings.
use super::*;
use crate::features::auth::{
    dto::{
        PasskeyAddFinishRequest, PasskeyAddFinishResponse, PasskeyAddStartRequest,
        PasskeyAddStartResponse, PasskeyLoginFinishRequest, PasskeyLoginFinishResponse,
        ReauthAction,
    },
    repositories::update_user_password_file_and_revoke_refresh_sessions,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use openssl::{
    bn::BigNumContext,
    ec::{EcGroup, EcKey, PointConversionForm},
    hash::MessageDigest,
    nid::Nid,
    pkey::{PKey, Private},
    sign::Signer,
};

type Tx = sqlx::Transaction<'static, sqlx::Postgres>;

async fn backend_pid(tx: &mut Tx) -> i32 {
    sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut **tx)
        .await
        .unwrap()
}

// Require the intended production statement and its exact blocking backend.
// Polling only observes PostgreSQL's lock graph; it does not order the schedule.
async fn wait_for_lock<T>(
    pool: &PgPool,
    blocker: i32,
    query: &str,
    task: &tokio::task::JoinHandle<T>,
) -> i32 {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let row: Option<(i32, String, String)> = sqlx::query_as(
                "SELECT pid, wait_event, query FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid)) AND wait_event_type = 'Lock' AND query LIKE $2")
                .bind(blocker).bind(format!("%{query}%")).fetch_optional(pool).await.unwrap();
            if let Some((pid, event, statement)) = row {
                eprintln!("auth persistence lock: backend={pid} blocker={blocker} event={event} statement={statement}");
                return pid;
            }
            assert!(!task.is_finished(), "operation completed before expected lock: {query}");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("production operation reaches conflicting PostgreSQL lock")
}

async fn account_id(app: &TestApp, username: &str) -> Uuid {
    sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.state.pool)
        .await
        .unwrap()
}

async fn generation(app: &TestApp, user_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT auth_generation FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&app.state.pool)
        .await
        .unwrap()
}

async fn snapshot(app: &TestApp, user_id: Uuid) -> serde_json::Value {
    sqlx::query_scalar("SELECT jsonb_build_object('user', to_jsonb(u), 'sessions', (SELECT jsonb_agg(to_jsonb(r) ORDER BY id) FROM refresh_tokens r WHERE user_id = u.id), 'passkeys', (SELECT jsonb_agg(to_jsonb(p) ORDER BY id) FROM user_passkeys p WHERE user_id = u.id)) FROM users u WHERE id = $1")
        .bind(user_id).fetch_one(&app.state.pool).await.unwrap()
}

async fn stage_reset(app: &TestApp, user_id: Uuid, record: &[u8]) -> Tx {
    let mut tx = app.state.pool.begin().await.unwrap();
    update_user_password_file_and_revoke_refresh_sessions(&mut tx, user_id, record, &[13; 64])
        .await
        .unwrap();
    tx
}

fn spawn_reset(app: &TestApp, user_id: Uuid, record: Vec<u8>) -> tokio::task::JoinHandle<()> {
    let pool = app.state.pool.clone();
    tokio::spawn(async move {
        let mut tx = pool.begin().await.unwrap();
        update_user_password_file_and_revoke_refresh_sessions(&mut tx, user_id, &record, &[13; 64])
            .await
            .unwrap();
        tx.commit().await.unwrap();
    })
}

async fn lock_session(app: &TestApp, signin: &SigninArtifacts) -> Tx {
    let session_id = app
        .state
        .validate_token(&signin.access_token)
        .unwrap()
        .session_id
        .unwrap();
    let mut tx = app.state.pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM refresh_tokens WHERE id = $1 FOR UPDATE")
        .bind(session_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx
}

fn bearer(token: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
    );
    headers
}

async fn assert_revoked_access(app: &TestApp, token: &str) {
    let error = crate::features::common::authorize_enrollment_principal(&app.state, &bearer(token))
        .await
        .unwrap_err();
    assert_eq!(error.0, StatusCode::UNAUTHORIZED);
}

async fn refresh(app: &TestApp, request: &RefreshRequest) -> Response {
    post_msgpack(&app.app, "/auth/refresh", request, &HeaderMap::new()).await
}

#[tokio::test]
async fn refresh_commit_and_password_reset_serialize_in_both_orders() {
    let app = setup_test_app().await;
    let unrelated = format!("it-refresh-unrelated-{}", Uuid::new_v4());
    register_user(&app, &unrelated, "P@ssword123!").await;
    let unrelated_signin =
        signin_user(&app, &unrelated, "P@ssword123!", SigninTransport::Body).await;
    let unrelated_id = account_id(&app, &unrelated).await;
    let before = snapshot(&app, unrelated_id).await;

    for reset_first in [true, false] {
        let username = format!("it-refresh-order-{}", Uuid::new_v4());
        register_user(&app, &username, "P@ssword123!").await;
        let signin = signin_user(&app, &username, "P@ssword123!", SigninTransport::Body).await;
        let user_id = account_id(&app, &username).await;
        let registration =
            prepare_signup_request(&app, &username, "New-P@ssword123!", Uuid::new_v4()).await;
        let request = RefreshRequest {
            refresh_token: signin.refresh_token.clone(),
            rotation_request_id: Uuid::new_v4(),
        };
        let router = app.app.clone();
        let pending = request.clone();
        let mut blocker = if reset_first {
            stage_reset(&app, user_id, &registration.opaque_finish_request).await
        } else {
            lock_session(&app, &signin).await
        };
        let blocker_pid = backend_pid(&mut blocker).await;
        let finishing = tokio::spawn(async move {
            post_msgpack(&router, "/auth/refresh", &pending, &HeaderMap::new()).await
        });
        let refresh_pid = wait_for_lock(
            &app.state.pool,
            blocker_pid,
            if reset_first {
                "FROM users"
            } else {
                "FROM refresh_tokens"
            },
            &finishing,
        )
        .await;
        let resetting = if reset_first {
            None
        } else {
            let reset = spawn_reset(&app, user_id, registration.opaque_finish_request);
            wait_for_lock(
                &app.state.pool,
                refresh_pid,
                "UPDATE users SET auth_generation",
                &reset,
            )
            .await;
            Some(reset)
        };
        blocker.commit().await.unwrap();
        let response = finishing.await.unwrap();
        if let Some(reset) = resetting {
            reset.await.unwrap();
        }
        if reset_first {
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        } else {
            let (status, _, body) = split_response(response).await;
            assert_eq!(status, StatusCode::OK);
            let rotated: RefreshResponse = decode_msgpack(&body);
            assert_revoked_access(&app, &rotated.access_token).await;
            let row: (bool, Option<Uuid>, Option<Uuid>) = sqlx::query_as("SELECT revoked_at IS NOT NULL, replaced_by_token_id, rotation_request_id FROM refresh_tokens WHERE id = $1")
                .bind(app.state.validate_token(&signin.access_token).unwrap().session_id.unwrap()).fetch_one(&app.state.pool).await.unwrap();
            assert_eq!(
                row,
                (
                    true,
                    rotated.refresh_token_id,
                    Some(request.rotation_request_id)
                )
            );
            assert_eq!(
                refresh(
                    &app,
                    &RefreshRequest {
                        refresh_token: rotated.refresh_token,
                        rotation_request_id: Uuid::new_v4()
                    }
                )
                .await
                .status(),
                StatusCode::UNAUTHORIZED
            );
        }
        assert_eq!(generation(&app, user_id).await, 1);
        assert_revoked_access(&app, &signin.access_token).await;
        assert_eq!(
            refresh(&app, &request).await.status(),
            StatusCode::UNAUTHORIZED,
            "exact retry cannot resurrect a reset session"
        );
        assert_eq!(active_session_count(&app, &username).await, 0);

        // A current-generation rotation and its exact retry return one replacement.
        let fresh = signin_user(&app, &username, "New-P@ssword123!", SigninTransport::Body).await;
        let request = RefreshRequest {
            refresh_token: fresh.refresh_token,
            rotation_request_id: Uuid::new_v4(),
        };
        let (status, _, body) = split_response(refresh(&app, &request).await).await;
        assert_eq!(status, StatusCode::OK);
        let first: RefreshResponse = decode_msgpack(&body);
        let (status, _, body) = split_response(refresh(&app, &request).await).await;
        assert_eq!(status, StatusCode::OK);
        let retry: RefreshResponse = decode_msgpack(&body);
        assert_eq!(retry.refresh_token, first.refresh_token);
        assert_eq!(retry.refresh_token_id, first.refresh_token_id);
        assert_eq!(active_session_count(&app, &username).await, 1);
        assert_eq!(snapshot(&app, unrelated_id).await, before);
    }
    crate::features::common::authorize_enrollment_principal(
        &app.state,
        &bearer(&unrelated_signin.access_token),
    )
    .await
    .unwrap();
    app.shutdown().await;
}

/// Minimal software authenticator: fresh ES256 key, discoverable user handle,
/// UV+UP flags, none attestation and real SHA-256/ECDSA assertion signatures.
/// CBOR is deliberately limited to this fixed, documented fixture shape.
struct SoftwareAuthenticator {
    key: PKey<Private>,
    credential_id: Vec<u8>,
    user_id: Uuid,
}

impl SoftwareAuthenticator {
    fn new(user_id: Uuid) -> Self {
        let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1).unwrap();
        Self {
            key: PKey::from_ec_key(EcKey::generate(&group).unwrap()).unwrap(),
            credential_id: Uuid::new_v4().as_bytes().to_vec(),
            user_id,
        }
    }

    fn client_data(&self, app: &TestApp, challenge: &[u8], kind: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"type": kind, "challenge": URL_SAFE_NO_PAD.encode(challenge), "origin": app.state.config.webauthn_rp_origin, "crossOrigin": false})).unwrap()
    }

    fn auth_data(&self, app: &TestApp, flags: u8, counter: u32) -> Vec<u8> {
        let mut data = Sha256::digest(app.state.config.webauthn_rp_id.as_bytes()).to_vec();
        data.push(flags);
        data.extend_from_slice(&counter.to_be_bytes());
        data
    }

    fn registration(&self, app: &TestApp, challenge: &[u8]) -> Vec<u8> {
        let ec = self.key.ec_key().unwrap();
        let point = ec
            .public_key()
            .to_bytes(
                ec.group(),
                PointConversionForm::UNCOMPRESSED,
                &mut BigNumContext::new().unwrap(),
            )
            .unwrap();
        // COSE EC2 key: {1:2, 3:-7, -1:1, -2:x, -3:y}.
        let mut cose = hex::decode("a5010203262001215820").unwrap();
        cose.extend_from_slice(&point[1..33]);
        cose.extend_from_slice(&[0x22, 0x58, 0x20]);
        cose.extend_from_slice(&point[33..65]);
        let mut data = self.auth_data(app, 0x45, 0); // UP | UV | AT
        data.extend_from_slice(&[0; 16]); // AAGUID
        data.extend_from_slice(&(self.credential_id.len() as u16).to_be_bytes());
        data.extend_from_slice(&self.credential_id);
        data.extend_from_slice(&cose);
        // {"fmt":"none", "attStmt":{}, "authData": h'...'}.
        let mut attestation =
            hex::decode("a363666d74646e6f6e656761747453746d74a068617574684461746158").unwrap();
        attestation.push(u8::try_from(data.len()).unwrap());
        attestation.extend_from_slice(&data);
        serde_json::to_vec(&serde_json::json!({"id": URL_SAFE_NO_PAD.encode(&self.credential_id), "rawId": URL_SAFE_NO_PAD.encode(&self.credential_id), "type":"public-key", "response": {"attestationObject": URL_SAFE_NO_PAD.encode(attestation), "clientDataJSON": URL_SAFE_NO_PAD.encode(self.client_data(app, challenge, "webauthn.create"))}, "clientExtensionResults": {"credProps": {"rk": true}}})).unwrap()
    }

    fn assertion(&self, app: &TestApp, challenge: &[u8]) -> Vec<u8> {
        let client_data = self.client_data(app, challenge, "webauthn.get");
        let data = self.auth_data(app, 0x05, 1); // UP | UV
        let mut signer = Signer::new(MessageDigest::sha256(), &self.key).unwrap();
        signer.update(&data).unwrap();
        signer.update(&Sha256::digest(&client_data)).unwrap();
        let signature = signer.sign_to_vec().unwrap();
        serde_json::to_vec(&serde_json::json!({"id": URL_SAFE_NO_PAD.encode(&self.credential_id), "rawId": URL_SAFE_NO_PAD.encode(&self.credential_id), "type":"public-key", "response": {"authenticatorData": URL_SAFE_NO_PAD.encode(data), "clientDataJSON": URL_SAFE_NO_PAD.encode(client_data), "signature": URL_SAFE_NO_PAD.encode(signature), "userHandle": URL_SAFE_NO_PAD.encode(self.user_id.as_bytes())}, "clientExtensionResults": {}})).unwrap()
    }
}

async fn prepare_passkey(
    app: &TestApp,
    headers: &HeaderMap,
    password: &str,
    authenticator: &SoftwareAuthenticator,
) -> PasskeyAddFinishRequest {
    let proof = reauth_proof(app, headers, password, ReauthAction::SecuritySettings).await;
    let start_request = PasskeyAddStartRequest {
        reauth_token: proof,
    };
    let response = post_msgpack(&app.app, "/auth/passkey/add/start", &start_request, headers).await;
    let (status, _, body) = split_response(response).await;
    assert_eq!(status, StatusCode::OK);
    let started: PasskeyAddStartResponse = decode_msgpack(&body);
    assert_eq!(
        post_msgpack(&app.app, "/auth/passkey/add/start", &start_request, headers)
            .await
            .status(),
        StatusCode::UNAUTHORIZED,
        "reauth proof is one-use"
    );
    PasskeyAddFinishRequest {
        flow_id: started.flow_id,
        credential: authenticator.registration(app, &started.challenge),
        encrypted_name: vec![17; 49],
    }
}

async fn passkey_count(app: &TestApp, user_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM user_passkeys WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(&app.state.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn passkey_registration_commit_rejects_reset_generation() {
    let app = setup_test_app().await;
    let unrelated = format!("it-passkey-unrelated-{}", Uuid::new_v4());
    register_user(&app, &unrelated, "P@ssword123!").await;
    signin_user(&app, &unrelated, "P@ssword123!", SigninTransport::Body).await;
    let unrelated_id = account_id(&app, &unrelated).await;
    let before = snapshot(&app, unrelated_id).await;
    for reset_first in [true, false] {
        let username = format!("it-passkey-order-{}", Uuid::new_v4());
        register_user(&app, &username, "P@ssword123!").await;
        let signin = signin_user(&app, &username, "P@ssword123!", SigninTransport::Body).await;
        let headers = enroll_signin(&app, &signin).await;
        let user_id = account_id(&app, &username).await;
        let authenticator = SoftwareAuthenticator::new(user_id);
        let pending = prepare_passkey(&app, &headers, "P@ssword123!", &authenticator).await;
        let registration =
            prepare_signup_request(&app, &username, "New-P@ssword123!", Uuid::new_v4()).await;
        let mut blocker = if reset_first {
            stage_reset(&app, user_id, &registration.opaque_finish_request).await
        } else {
            lock_session(&app, &signin).await
        };
        let blocker_pid = backend_pid(&mut blocker).await;
        let router = app.app.clone();
        let finish_request = pending.clone();
        let finish_headers = headers.clone();
        let finishing = tokio::spawn(async move {
            post_msgpack(
                &router,
                "/auth/passkey/add/finish",
                &finish_request,
                &finish_headers,
            )
            .await
        });
        let finish_pid = wait_for_lock(
            &app.state.pool,
            blocker_pid,
            if reset_first {
                "SELECT auth_generation FROM users"
            } else {
                "SELECT id FROM refresh_tokens"
            },
            &finishing,
        )
        .await;
        let resetting = if reset_first {
            None
        } else {
            let reset = spawn_reset(&app, user_id, registration.opaque_finish_request);
            wait_for_lock(
                &app.state.pool,
                finish_pid,
                "UPDATE users SET auth_generation",
                &reset,
            )
            .await;
            Some(reset)
        };
        blocker.commit().await.unwrap();
        let response = finishing.await.unwrap();
        if let Some(reset) = resetting {
            reset.await.unwrap();
        }
        if reset_first {
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            assert_eq!(passkey_count(&app, user_id).await, 0);
        } else {
            let (status, _, body) = split_response(response).await;
            assert_eq!(status, StatusCode::OK);
            let added: PasskeyAddFinishResponse = decode_msgpack(&body);
            assert_eq!(added.passkey.credential_id, authenticator.credential_id);
            assert_eq!(
                passkey_count(&app, user_id).await,
                1,
                "password reset retains a previously committed passkey"
            );
        }
        assert_eq!(generation(&app, user_id).await, 1);
        assert_eq!(active_session_count(&app, &username).await, 0);
        assert_revoked_access(&app, &signin.access_token).await;
        assert!(
            app.state
                .state_store
                .get(&format!("auth:passkey-enrollment:{}", pending.flow_id))
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            app.state
                .state_store
                .get(&format!("passkey:reg:flow:{}", pending.flow_id))
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            post_msgpack(&app.app, "/auth/passkey/add/finish", &pending, &headers)
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );

        let fresh = signin_user(&app, &username, "New-P@ssword123!", SigninTransport::Body).await;
        let fresh_headers = enroll_signin(&app, &fresh).await;
        // A fresh session cannot reuse the consumed pre-reset enrollment.
        assert_eq!(
            post_msgpack(
                &app.app,
                "/auth/passkey/add/finish",
                &pending,
                &fresh_headers
            )
            .await
            .status(),
            StatusCode::UNAUTHORIZED
        );
        if reset_first {
            let current =
                prepare_passkey(&app, &fresh_headers, "New-P@ssword123!", &authenticator).await;
            let response = post_msgpack(
                &app.app,
                "/auth/passkey/add/finish",
                &current,
                &fresh_headers,
            )
            .await;
            let (status, _, body) = split_response(response).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(
                decode_msgpack::<PasskeyAddFinishResponse>(&body)
                    .passkey
                    .credential_id,
                authenticator.credential_id
            );
            assert_eq!(
                post_msgpack(
                    &app.app,
                    "/auth/passkey/add/finish",
                    &current,
                    &fresh_headers
                )
                .await
                .status(),
                StatusCode::UNAUTHORIZED,
                "registration completion is one-use, not exact-retryable"
            );
        }
        assert_eq!(passkey_count(&app, user_id).await, 1);
        // The retained/current passkey performs real new authentication at generation 1.
        let started = crate::features::auth::services::passkey_login_start(&app.state)
            .await
            .unwrap();
        let request = PasskeyLoginFinishRequest {
            flow_id: started.flow_id,
            credential: authenticator.assertion(&app, &started.challenge),
        };
        let (status, _, body) = split_response(
            post_msgpack(
                &app.app,
                "/auth/passkey/login/finish",
                &request,
                &HeaderMap::new(),
            )
            .await,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let login: PasskeyLoginFinishResponse = decode_msgpack(&body);
        assert_eq!(login.username, username);
        crate::features::common::authorize_enrollment_principal(
            &app.state,
            &bearer(&login.access_token),
        )
        .await
        .unwrap();
        let authenticated = SigninArtifacts {
            access_token: login.access_token,
            refresh_token: login.refresh_token,
            device_enrollment_token: login.device_enrollment_token,
            csrf_token: login.csrf_token,
            set_cookie_headers: Vec::new(),
        };
        let enrolled_headers = enroll_signin(&app, &authenticated).await;
        crate::features::common::authorize_principal(&app.state, &enrolled_headers)
            .await
            .unwrap();
        assert_eq!(active_session_count(&app, &username).await, 2);
        assert_eq!(generation(&app, user_id).await, 1);
        assert_eq!(snapshot(&app, unrelated_id).await, before);
    }
    app.shutdown().await;
}
