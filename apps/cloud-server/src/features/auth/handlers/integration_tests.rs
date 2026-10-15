//! End-to-end auth transport tests for signin/refresh/logout.

use std::{sync::Arc, time::Duration};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{HeaderMap, HeaderValue, Request, StatusCode, header},
    response::Response,
};
use opaque_ke::{
    ClientLogin, ClientLoginFinishParameters, ClientRegistration,
    ClientRegistrationFinishParameters, CredentialResponse, RegistrationResponse,
};
use rand08::rngs::OsRng;
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower::util::ServiceExt;
use uuid::Uuid;

use crate::{
    app::router::build_router,
    features::{
        auth::{
            dto::{
                AccountRecoveryFinishRequest, AccountRecoveryFinishResponse,
                AccountRecoveryStartRequest, AccountRecoveryStartResponse, CsrfBootstrapResponse,
                DeviceAuthorizationApproveRequest, DeviceAuthorizationApproveResponse,
                DeviceAuthorizationInspectRequest, DeviceAuthorizationInspectResponse,
                DeviceAuthorizationStartRequest, DeviceAuthorizationStartResponse,
                DeviceAuthorizationStatus, DeviceAuthorizationTokenRequest,
                DeviceAuthorizationTokenResponse, LogoutRequest, LogoutResponse, RefreshRequest,
                RefreshResponse, SigninFinishRequest, SigninFinishResponse, SigninStartRequest,
                SigninStartResponse, SignupFinishRequest, SignupFinishResponse, SignupStartRequest,
                SignupStartResponse,
            },
            repositories::{consume_totp_backup_code, delete_passkey_for_user},
            services::support::{hash_account_recovery_code, normalize_recovery_code},
            transport::{CSRF_HEADER, REFRESH_TRANSPORT_HEADER},
        },
        common::ErrorResponse,
        devices::dto::{DevicePlatform, RegisterDeviceRequest},
        users::repositories::delete_user,
    },
    platform::{
        security::opaque::DefaultOpaqueSuite, state::AppState, state_store::InMemoryStore,
        test_support::test_config,
    },
};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();
static MIGRATIONS_READY: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();

const MSGPACK_CONTENT_TYPE: &str = "application/msgpack";
const BODY_LIMIT_BYTES: usize = 1024 * 1024;
const ALLOWED_ORIGIN: &str = "http://localhost:4173";

#[derive(Serialize)]
struct TestAccountPublicKeyBundle {
    version: u8,
    #[serde(with = "serde_bytes")]
    account_recovery_public_key: Vec<u8>,
}

struct TestApp {
    app: Router,
    state: AppState,
}

impl TestApp {
    async fn shutdown(self) {
        self.state.pool.close().await;
    }
}

enum SigninTransport {
    Body,
    Cookie,
}

struct SigninArtifacts {
    access_token: String,
    refresh_token: Option<String>,
    device_enrollment_token: String,
    csrf_token: Option<String>,
    set_cookie_headers: Vec<String>,
}

fn test_database_url() -> Option<String> {
    std::env::var("KAMORI_DATABASE_URL")
        .ok()
        .or_else(|| std::env::var("DATABASE_URL").ok())
}

async fn ensure_migrations(pool: &PgPool) {
    MIGRATIONS_READY
        .get_or_init(|| async {
            MIGRATOR.run(pool).await.expect("run migrations");
        })
        .await;
}

async fn setup_test_app() -> Option<TestApp> {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("error")),
        )
        .try_init();
    let Some(database_url) = test_database_url() else {
        eprintln!(
            "skipping auth integration tests: set KAMORI_DATABASE_URL or DATABASE_URL to enable"
        );
        return None;
    };

    let mut config = test_config();
    config.database_url = database_url.clone();

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("connect postgres");
    ensure_migrations(&pool).await;

    let state_store = Arc::new(InMemoryStore::new(Duration::from_secs(
        config.valkey_ttl_seconds,
    )));
    let state = AppState::new(pool, config, state_store).expect("build app state");
    let app = build_router(state.clone()).expect("build app router");
    Some(TestApp { app, state })
}

async fn post_msgpack<T: Serialize>(
    app: &Router,
    path: &str,
    payload: &T,
    extra_headers: &HeaderMap,
) -> Response {
    let mut body = Vec::new();
    payload
        .serialize(
            &mut rmp_serde::Serializer::new(&mut body)
                .with_struct_map()
                .with_human_readable(),
        )
        .expect("encode msgpack request");
    let mut builder = Request::builder()
        .method("POST")
        .uri(path)
        .header(header::CONTENT_TYPE, MSGPACK_CONTENT_TYPE)
        .header(header::ACCEPT, MSGPACK_CONTENT_TYPE);
    for (name, value) in extra_headers {
        builder = builder.header(name, value.clone());
    }

    let request = builder.body(Body::from(body)).expect("build HTTP request");
    app.clone().oneshot(request).await.expect("router response")
}

async fn split_response(response: Response) -> (StatusCode, HeaderMap, Vec<u8>) {
    let status = response.status();
    let headers = response.headers().clone();
    let body = to_bytes(response.into_body(), BODY_LIMIT_BYTES)
        .await
        .expect("read response body");
    (status, headers, body.to_vec())
}

fn decode_msgpack<T: DeserializeOwned>(body: &[u8]) -> T {
    let mut deserializer = rmp_serde::Deserializer::new(body).with_human_readable();
    T::deserialize(&mut deserializer).expect("decode msgpack response")
}

fn set_cookie_values(headers: &HeaderMap) -> Vec<String> {
    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok().map(str::to_owned))
        .collect()
}

fn cookie_value(set_cookies: &[String], name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    for cookie in set_cookies {
        let Some(rest) = cookie.strip_prefix(&prefix) else {
            continue;
        };
        let value = rest.split(';').next().unwrap_or_default();
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

fn has_cleared_cookie(set_cookies: &[String], name: &str) -> bool {
    let prefix = format!("{name}=;");
    set_cookies.iter().any(|cookie| cookie.starts_with(&prefix))
}

fn combined_cookie_header(
    refresh_cookie_name: &str,
    refresh_cookie: &str,
    csrf_cookie_name: &str,
    csrf_cookie: &str,
) -> String {
    format!("{refresh_cookie_name}={refresh_cookie}; {csrf_cookie_name}={csrf_cookie}")
}

async fn prepare_signup_request(
    app: &TestApp,
    username: &str,
    password: &str,
    signup_request_id: Uuid,
) -> SignupFinishRequest {
    let mut rng = OsRng;
    let registration_start =
        ClientRegistration::<DefaultOpaqueSuite>::start(&mut rng, password.as_bytes())
            .expect("opaque registration start");

    let signup_start_response = post_msgpack(
        &app.app,
        "/auth/signup/start",
        &SignupStartRequest {
            username: username.to_string(),
            opaque_start_request: registration_start.message.serialize().to_vec(),
        },
        &HeaderMap::new(),
    )
    .await;
    let (status, _headers, body) = split_response(signup_start_response).await;
    assert_eq!(status, StatusCode::OK);
    let signup_start: SignupStartResponse = decode_msgpack(&body);
    let registration_response = RegistrationResponse::<DefaultOpaqueSuite>::deserialize(
        &signup_start.opaque_server_message,
    )
    .expect("deserialize registration response");

    let registration_finish = registration_start
        .state
        .finish(
            &mut rng,
            password.as_bytes(),
            registration_response,
            ClientRegistrationFinishParameters::default(),
        )
        .expect("opaque registration finish");

    SignupFinishRequest {
        signup_request_id,
        username: username.to_string(),
        opaque_finish_request: registration_finish.message.serialize().to_vec(),
        encrypted_master_key: vec![1; 49],
        public_key_bundle: rmp_serde::to_vec_named(&TestAccountPublicKeyBundle {
            version: 2,
            account_recovery_public_key:
                crypto_core_lib::CryptoEngine::derive_account_recovery_keypair(&[4_u8; 32])
                    .public_key
                    .to_vec(),
        })
        .expect("encode account public key bundle"),
        recovery_verifier: vec![7; 32],
    }
}

async fn register_user(app: &TestApp, username: &str, password: &str) {
    let request = prepare_signup_request(app, username, password, Uuid::new_v4()).await;
    let signup_finish_response =
        post_msgpack(&app.app, "/auth/signup/finish", &request, &HeaderMap::new()).await;
    let (status, _headers, body) = split_response(signup_finish_response).await;
    assert_eq!(status, StatusCode::OK);
    let _signup_finish: SignupFinishResponse = decode_msgpack(&body);
}

#[tokio::test]
async fn signup_finish_is_exact_retry_idempotent_after_registration_closes() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let username = format!("it-signup-retry-{}", Uuid::new_v4());
    let request_id = Uuid::new_v4();
    let request = prepare_signup_request(&app, &username, "P@ssword123!", request_id).await;
    let first = post_msgpack(&app.app, "/auth/signup/finish", &request, &HeaderMap::new()).await;
    let (status, _, body) = split_response(first).await;
    assert_eq!(status, StatusCode::OK);
    let first: SignupFinishResponse = decode_msgpack(&body);

    let mut closed_config = app.state.config.clone();
    closed_config.registration_enabled = false;
    let closed_store = Arc::new(InMemoryStore::new(Duration::from_secs(
        closed_config.valkey_ttl_seconds,
    )));
    let closed_state = AppState::new(app.state.pool.clone(), closed_config, closed_store)
        .expect("build registration-closed state");
    let retry = crate::features::auth::services::signup_finish(&closed_state, request.clone())
        .await
        .expect("exact retry after registration closes");
    assert_eq!(retry.user_id, first.user_id);

    let mut conflicting = request;
    conflicting.encrypted_master_key[0] ^= 1;
    let error = crate::features::auth::services::signup_finish(&closed_state, conflicting)
        .await
        .expect_err("request id reuse with different data must fail");
    assert_eq!(error.0, StatusCode::CONFLICT);

    app.shutdown().await;
}

async fn signin_user(
    app: &TestApp,
    username: &str,
    password: &str,
    transport: SigninTransport,
) -> SigninArtifacts {
    let pending = pending_signin(app, username, password).await;
    let mut headers = HeaderMap::new();
    if matches!(transport, SigninTransport::Cookie) {
        headers.insert(REFRESH_TRANSPORT_HEADER, HeaderValue::from_static("cookie"));
    }
    let signin_finish_response =
        post_msgpack(&app.app, "/auth/signin/finish", &pending, &headers).await;

    let (status, response_headers, body) = split_response(signin_finish_response).await;
    assert_eq!(status, StatusCode::OK);
    let payload: SigninFinishResponse = decode_msgpack(&body);
    assert!(payload.access_token.is_some());
    assert!(payload.totp_verified);
    assert!(payload.totp_continuation_token.is_none());

    SigninArtifacts {
        access_token: payload.access_token.expect("access token"),
        refresh_token: payload.refresh_token,
        device_enrollment_token: payload
            .device_enrollment_token
            .expect("device enrollment token"),
        csrf_token: payload.csrf_token,
        set_cookie_headers: set_cookie_values(&response_headers),
    }
}

#[tokio::test]
async fn external_browser_device_authorization_is_explicit_and_retry_safe() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let username = format!("it-device-flow-{}", Uuid::new_v4());
    let password = "P@ssword123!";
    register_user(&app, &username, password).await;
    let signin = signin_user(&app, &username, password, SigninTransport::Body).await;
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&[23_u8; 32]);
    let hpke_key = crypto_core_lib::CryptoEngine::generate_x25519_keypair();
    let mut enrollment_headers = HeaderMap::new();
    enrollment_headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", signin.access_token))
            .expect("authorization header"),
    );
    let enrollment = post_msgpack(
        &app.app,
        "/devices",
        &RegisterDeviceRequest {
            enrollment_token: signin.device_enrollment_token.clone(),
            device_id: Uuid::new_v4(),
            signing_public_key: signing_key.verifying_key().to_bytes().to_vec(),
            hpke_public_key: hpke_key.public_key.to_vec(),
            encrypted_name: vec![1_u8],
            platform: DevicePlatform::Web,
        },
        &enrollment_headers,
    )
    .await;
    let (status, _, _) = split_response(enrollment).await;
    assert_eq!(status, StatusCode::OK);

    let start_response = post_msgpack(
        &app.app,
        "/auth/device-authorization/start",
        &DeviceAuthorizationStartRequest {
            hpke_public_key: vec![7; 32],
        },
        &HeaderMap::new(),
    )
    .await;
    let (status, _, body) = split_response(start_response).await;
    assert_eq!(status, StatusCode::OK);
    let started: DeviceAuthorizationStartResponse = decode_msgpack(&body);
    assert!(started.verification_uri.contains(&started.user_code));

    let pending_response = post_msgpack(
        &app.app,
        "/auth/device-authorization/token",
        &DeviceAuthorizationTokenRequest {
            flow_id: started.flow_id,
            device_secret: started.device_secret.clone(),
        },
        &HeaderMap::new(),
    )
    .await;
    let (status, _, body) = split_response(pending_response).await;
    assert_eq!(status, StatusCode::OK);
    let pending: DeviceAuthorizationTokenResponse = decode_msgpack(&body);
    assert_eq!(pending.status, DeviceAuthorizationStatus::Pending);
    assert!(pending.access_token.is_none());

    let mut approve_headers = HeaderMap::new();
    approve_headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", signin.access_token))
            .expect("authorization header"),
    );
    let inspect_response = post_msgpack(
        &app.app,
        "/auth/device-authorization/inspect",
        &DeviceAuthorizationInspectRequest {
            user_code: started.user_code.clone(),
        },
        &approve_headers,
    )
    .await;
    let (status, _, body) = split_response(inspect_response).await;
    if status != StatusCode::OK {
        let error: ErrorResponse = decode_msgpack(&body);
        panic!("device authorization inspect failed with {status}: {error:?}");
    }
    let inspected: DeviceAuthorizationInspectResponse = decode_msgpack(&body);
    assert_eq!(inspected.flow_id, started.flow_id);
    assert_eq!(inspected.hpke_public_key, vec![7; 32]);

    let encrypted_master_key_package = vec![9; 96];
    let approve_response = post_msgpack(
        &app.app,
        "/auth/device-authorization/approve",
        &DeviceAuthorizationApproveRequest {
            user_code: started.user_code.clone(),
            encrypted_master_key_package: encrypted_master_key_package.clone(),
        },
        &approve_headers,
    )
    .await;
    let (status, _, body) = split_response(approve_response).await;
    assert_eq!(status, StatusCode::OK);
    let approved: DeviceAuthorizationApproveResponse = decode_msgpack(&body);
    assert!(approved.approved);

    let token_request = DeviceAuthorizationTokenRequest {
        flow_id: started.flow_id,
        device_secret: started.device_secret,
    };
    let token_response = post_msgpack(
        &app.app,
        "/auth/device-authorization/token",
        &token_request,
        &HeaderMap::new(),
    )
    .await;
    let (status, _, body) = split_response(token_response).await;
    if status != StatusCode::OK {
        let error: ErrorResponse = decode_msgpack(&body);
        panic!("device authorization token failed with {status}: {error:?}");
    }
    let token: DeviceAuthorizationTokenResponse = decode_msgpack(&body);
    assert_eq!(token.status, DeviceAuthorizationStatus::Approved);
    assert_eq!(token.username.as_deref(), Some(username.as_str()));
    assert!(token.access_token.is_some());
    assert!(token.refresh_token.is_some());
    assert_eq!(
        token.encrypted_master_key_package,
        encrypted_master_key_package
    );

    let retry_response = post_msgpack(
        &app.app,
        "/auth/device-authorization/token",
        &token_request,
        &HeaderMap::new(),
    )
    .await;
    let (status, _, body) = split_response(retry_response).await;
    assert_eq!(status, StatusCode::OK);
    let retry: DeviceAuthorizationTokenResponse = decode_msgpack(&body);
    assert_eq!(retry.status, DeviceAuthorizationStatus::Approved);
    assert_eq!(retry.refresh_token, token.refresh_token);
    assert_eq!(retry.refresh_token_id, token.refresh_token_id);
    assert_eq!(retry.device_enrollment_token, token.device_enrollment_token);

    // Credential-policy changes can invalidate pending proofs while keeping
    // existing sessions active. An exact token retry still resolves that session.
    sqlx::query("UPDATE users SET auth_generation = auth_generation + 1 WHERE username = $1")
        .bind(&username)
        .execute(&app.state.pool)
        .await
        .unwrap();
    // Revoking the approving browser does not break exact retries of the
    // already-created active native session.
    let response = post_msgpack(
        &app.app,
        "/auth/logout",
        &LogoutRequest {
            refresh_token: signin.refresh_token,
        },
        &HeaderMap::new(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = post_msgpack(
        &app.app,
        "/auth/device-authorization/token",
        &token_request,
        &HeaderMap::new(),
    )
    .await;
    let (status, _, body) = split_response(response).await;
    assert_eq!(status, StatusCode::OK);
    let retry: DeviceAuthorizationTokenResponse = decode_msgpack(&body);
    assert_eq!(retry.refresh_token, token.refresh_token);
    let response = post_msgpack(
        &app.app,
        "/auth/logout",
        &LogoutRequest {
            refresh_token: token.refresh_token,
        },
        &HeaderMap::new(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = post_msgpack(
        &app.app,
        "/auth/device-authorization/token",
        &token_request,
        &HeaderMap::new(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(active_session_count(&app, &username).await, 0);

    app.shutdown().await;
}

#[tokio::test]
async fn cookie_signin_refresh_logout_sets_and_clears_refresh_and_csrf_cookies() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let username = format!("it-cookie-{}", Uuid::new_v4());
    let password = "P@ssword123!";
    register_user(&app, &username, password).await;
    let signin = signin_user(&app, &username, password, SigninTransport::Cookie).await;
    assert!(signin.refresh_token.is_none());

    let refresh_cookie_name = app.state.config.web_refresh_cookie_name.clone();
    let csrf_cookie_name = app.state.config.web_csrf_cookie_name.clone();
    let refresh_cookie = cookie_value(&signin.set_cookie_headers, &refresh_cookie_name)
        .expect("refresh cookie from signin");
    let csrf_cookie = cookie_value(&signin.set_cookie_headers, &csrf_cookie_name)
        .expect("csrf cookie from signin");
    assert_eq!(signin.csrf_token.as_deref(), Some(csrf_cookie.as_str()));

    let cookie_header = combined_cookie_header(
        &refresh_cookie_name,
        &refresh_cookie,
        &csrf_cookie_name,
        &csrf_cookie,
    );
    let mut bootstrap_headers = HeaderMap::new();
    bootstrap_headers.insert(REFRESH_TRANSPORT_HEADER, HeaderValue::from_static("cookie"));
    bootstrap_headers.insert(header::ORIGIN, HeaderValue::from_static(ALLOWED_ORIGIN));
    bootstrap_headers.insert(
        header::COOKIE,
        HeaderValue::from_str(&cookie_header).expect("cookie header"),
    );
    let bootstrap_response = post_msgpack(
        &app.app,
        "/auth/csrf",
        &std::collections::HashMap::<String, String>::new(),
        &bootstrap_headers,
    )
    .await;
    let (status, _, body) = split_response(bootstrap_response).await;
    assert_eq!(status, StatusCode::OK);
    let bootstrap: CsrfBootstrapResponse = decode_msgpack(&body);
    assert_eq!(bootstrap.csrf_token, csrf_cookie);

    let mut refresh_headers = HeaderMap::new();
    refresh_headers.insert(REFRESH_TRANSPORT_HEADER, HeaderValue::from_static("cookie"));
    refresh_headers.insert(header::ORIGIN, HeaderValue::from_static(ALLOWED_ORIGIN));
    refresh_headers.insert(
        CSRF_HEADER,
        HeaderValue::from_str(&csrf_cookie).expect("csrf header"),
    );
    refresh_headers.insert(
        header::COOKIE,
        HeaderValue::from_str(&cookie_header).expect("cookie header"),
    );

    let refresh_response = post_msgpack(
        &app.app,
        "/auth/refresh",
        &RefreshRequest {
            refresh_token: None,
            rotation_request_id: Uuid::new_v4(),
        },
        &refresh_headers,
    )
    .await;
    let (status, response_headers, body) = split_response(refresh_response).await;
    assert_eq!(status, StatusCode::OK);
    let refresh_payload: RefreshResponse = decode_msgpack(&body);
    assert!(refresh_payload.refresh_token.is_none());

    let refresh_set_cookies = set_cookie_values(&response_headers);
    let rotated_refresh_cookie =
        cookie_value(&refresh_set_cookies, &refresh_cookie_name).expect("rotated refresh cookie");
    let rotated_csrf_cookie =
        cookie_value(&refresh_set_cookies, &csrf_cookie_name).expect("rotated csrf cookie");
    assert_eq!(
        refresh_payload.csrf_token.as_deref(),
        Some(rotated_csrf_cookie.as_str())
    );

    let mut logout_headers = HeaderMap::new();
    let authorization = format!("Bearer {}", refresh_payload.access_token);
    logout_headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&authorization).expect("authorization header"),
    );
    logout_headers.insert(REFRESH_TRANSPORT_HEADER, HeaderValue::from_static("cookie"));
    logout_headers.insert(header::ORIGIN, HeaderValue::from_static(ALLOWED_ORIGIN));
    logout_headers.insert(
        CSRF_HEADER,
        HeaderValue::from_str(&rotated_csrf_cookie).expect("csrf header"),
    );
    let logout_cookie_header = combined_cookie_header(
        &refresh_cookie_name,
        &rotated_refresh_cookie,
        &csrf_cookie_name,
        &rotated_csrf_cookie,
    );
    logout_headers.insert(
        header::COOKIE,
        HeaderValue::from_str(&logout_cookie_header).expect("cookie header"),
    );

    let logout_response = post_msgpack(
        &app.app,
        "/auth/logout",
        &LogoutRequest {
            refresh_token: None,
        },
        &logout_headers,
    )
    .await;
    let (status, response_headers, body) = split_response(logout_response).await;
    assert_eq!(status, StatusCode::OK);
    let logout_payload: LogoutResponse = decode_msgpack(&body);
    assert!(logout_payload.revoked);

    let clear_cookies = set_cookie_values(&response_headers);
    assert!(has_cleared_cookie(&clear_cookies, &refresh_cookie_name));
    assert!(has_cleared_cookie(&clear_cookies, &csrf_cookie_name));

    app.shutdown().await;
}

#[tokio::test]
async fn cookie_refresh_rejects_csrf_mismatch() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let username = format!("it-csrf-{}", Uuid::new_v4());
    let password = "P@ssword123!";
    register_user(&app, &username, password).await;
    let signin = signin_user(&app, &username, password, SigninTransport::Cookie).await;

    let refresh_cookie_name = app.state.config.web_refresh_cookie_name.clone();
    let csrf_cookie_name = app.state.config.web_csrf_cookie_name.clone();
    let refresh_cookie = cookie_value(&signin.set_cookie_headers, &refresh_cookie_name)
        .expect("refresh cookie from signin");
    let csrf_cookie = cookie_value(&signin.set_cookie_headers, &csrf_cookie_name)
        .expect("csrf cookie from signin");

    let mut refresh_headers = HeaderMap::new();
    refresh_headers.insert(REFRESH_TRANSPORT_HEADER, HeaderValue::from_static("cookie"));
    refresh_headers.insert(header::ORIGIN, HeaderValue::from_static(ALLOWED_ORIGIN));
    refresh_headers.insert(CSRF_HEADER, HeaderValue::from_static("mismatch-token"));
    let cookie_header = combined_cookie_header(
        &refresh_cookie_name,
        &refresh_cookie,
        &csrf_cookie_name,
        &csrf_cookie,
    );
    refresh_headers.insert(
        header::COOKIE,
        HeaderValue::from_str(&cookie_header).expect("cookie header"),
    );

    let refresh_response = post_msgpack(
        &app.app,
        "/auth/refresh",
        &RefreshRequest {
            refresh_token: None,
            rotation_request_id: Uuid::new_v4(),
        },
        &refresh_headers,
    )
    .await;
    let (status, _headers, body) = split_response(refresh_response).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let error: ErrorResponse = decode_msgpack(&body);
    assert_eq!(error.message, "csrf token mismatch");

    app.shutdown().await;
}

#[tokio::test]
async fn cookie_refresh_rejects_missing_origin_or_referer() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let username = format!("it-origin-{}", Uuid::new_v4());
    let password = "P@ssword123!";
    register_user(&app, &username, password).await;
    let signin = signin_user(&app, &username, password, SigninTransport::Cookie).await;

    let refresh_cookie_name = app.state.config.web_refresh_cookie_name.clone();
    let csrf_cookie_name = app.state.config.web_csrf_cookie_name.clone();
    let refresh_cookie = cookie_value(&signin.set_cookie_headers, &refresh_cookie_name)
        .expect("refresh cookie from signin");
    let csrf_cookie = cookie_value(&signin.set_cookie_headers, &csrf_cookie_name)
        .expect("csrf cookie from signin");

    let mut refresh_headers = HeaderMap::new();
    refresh_headers.insert(REFRESH_TRANSPORT_HEADER, HeaderValue::from_static("cookie"));
    refresh_headers.insert(
        CSRF_HEADER,
        HeaderValue::from_str(&csrf_cookie).expect("csrf header"),
    );
    let cookie_header = combined_cookie_header(
        &refresh_cookie_name,
        &refresh_cookie,
        &csrf_cookie_name,
        &csrf_cookie,
    );
    refresh_headers.insert(
        header::COOKIE,
        HeaderValue::from_str(&cookie_header).expect("cookie header"),
    );

    let refresh_response = post_msgpack(
        &app.app,
        "/auth/refresh",
        &RefreshRequest {
            refresh_token: None,
            rotation_request_id: Uuid::new_v4(),
        },
        &refresh_headers,
    )
    .await;
    let (status, _headers, body) = split_response(refresh_response).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let error: ErrorResponse = decode_msgpack(&body);
    assert_eq!(error.message, "origin or referer is required");

    app.shutdown().await;
}

#[tokio::test]
async fn body_transport_refresh_and_logout_remain_compatible() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let username = format!("it-body-{}", Uuid::new_v4());
    let password = "P@ssword123!";
    register_user(&app, &username, password).await;
    let signin = signin_user(&app, &username, password, SigninTransport::Body).await;

    assert!(signin.refresh_token.is_some());
    assert!(signin.set_cookie_headers.is_empty());
    let body_refresh_token = signin.refresh_token.expect("body refresh token");
    let rotation_request_id = Uuid::new_v4();

    let refresh_response = post_msgpack(
        &app.app,
        "/auth/refresh",
        &RefreshRequest {
            refresh_token: Some(body_refresh_token.clone()),
            rotation_request_id,
        },
        &HeaderMap::new(),
    )
    .await;
    let (status, response_headers, body) = split_response(refresh_response).await;
    assert_eq!(status, StatusCode::OK);
    let refresh_payload: RefreshResponse = decode_msgpack(&body);
    assert!(refresh_payload.refresh_token.is_some());
    assert!(set_cookie_values(&response_headers).is_empty());
    let rotated_refresh_token = refresh_payload
        .refresh_token
        .clone()
        .expect("rotated refresh token");

    sqlx::query(
        "UPDATE refresh_tokens SET rotated_at = now() - interval '1 day' WHERE replaced_by_token_id = $1",
    )
    .bind(refresh_payload.refresh_token_id.expect("replacement token id"))
    .execute(&app.state.pool)
    .await
    .expect("age exact retry mapping beyond the former grace window");

    let retry_response = post_msgpack(
        &app.app,
        "/auth/refresh",
        &RefreshRequest {
            refresh_token: Some(body_refresh_token),
            rotation_request_id,
        },
        &HeaderMap::new(),
    )
    .await;
    let (retry_status, _, retry_body) = split_response(retry_response).await;
    assert_eq!(retry_status, StatusCode::OK);
    let retry_payload: RefreshResponse = decode_msgpack(&retry_body);
    assert_eq!(retry_payload.refresh_token, refresh_payload.refresh_token);
    assert_eq!(
        retry_payload.refresh_token_id,
        refresh_payload.refresh_token_id
    );

    let mut logout_headers = HeaderMap::new();
    let authorization = format!("Bearer {}", refresh_payload.access_token);
    logout_headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&authorization).expect("authorization header"),
    );

    let logout_response = post_msgpack(
        &app.app,
        "/auth/logout",
        &LogoutRequest {
            refresh_token: Some(rotated_refresh_token),
        },
        &logout_headers,
    )
    .await;
    let (status, response_headers, body) = split_response(logout_response).await;
    assert_eq!(status, StatusCode::OK);
    let logout_payload: LogoutResponse = decode_msgpack(&body);
    assert!(logout_payload.revoked);
    assert!(set_cookie_values(&response_headers).is_empty());

    app.shutdown().await;
}

#[tokio::test]
async fn deleting_only_passkey_preserves_password_authentication() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let username = format!("it-last-passkey-{}", Uuid::new_v4());
    let password = "P@ssword123!";
    register_user(&app, &username, password).await;
    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(&username)
        .fetch_one(&app.state.pool)
        .await
        .expect("load registered user");
    let passkey_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO user_passkeys (id, user_id, credential_id, passkey_data, encrypted_name) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(passkey_id)
    .bind(user_id)
    .bind(vec![31_u8; 32])
    .bind(vec![32_u8])
    .bind(vec![33_u8])
    .execute(&app.state.pool)
    .await
    .expect("insert only passkey");

    let mut tx = app.state.pool.begin().await.unwrap();
    assert!(
        delete_passkey_for_user(&mut tx, user_id, passkey_id)
            .await
            .expect("delete only passkey")
    );
    tx.commit().await.unwrap();
    let remaining: i64 =
        sqlx::query_scalar("SELECT count(*) FROM user_passkeys WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(&app.state.pool)
            .await
            .expect("count remaining passkeys");
    assert_eq!(remaining, 0);

    let signin = signin_user(&app, &username, password, SigninTransport::Body).await;
    assert!(!signin.access_token.is_empty());

    app.shutdown().await;
}

#[tokio::test]
async fn account_recovery_returns_current_space_keys_and_revokes_old_credentials() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let username = format!("it-recovery-{}", Uuid::new_v4());
    register_user(&app, &username, "Old-P@ssword123!").await;
    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(&username)
        .fetch_one(&app.state.pool)
        .await
        .expect("load registered user");
    let workspace_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM workspaces WHERE owner_user_id = $1 AND kind = 'personal' AND deleted_at IS NULL",
    )
    .bind(user_id)
    .fetch_one(&app.state.pool)
    .await
    .expect("load signup personal workspace");
    let space_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO security_spaces (id, workspace_id, owner_user_id, created_by, encrypted_metadata) VALUES ($1, $2, $3, $3, $4)",
    )
    .bind(space_id)
    .bind(workspace_id)
    .bind(user_id)
    .bind(vec![2_u8])
    .execute(&app.state.pool)
    .await
    .expect("insert space");
    sqlx::query(
        "INSERT INTO security_space_members (id, space_id, user_id, role, key_epoch) VALUES ($1, $2, $3, 'owner', 1)",
    )
    .bind(Uuid::new_v4())
    .bind(space_id)
    .bind(user_id)
    .execute(&app.state.pool)
    .await
    .expect("insert membership");
    sqlx::query(
        "INSERT INTO devices (id, user_id, signing_public_key, hpke_public_key, encrypted_name, platform) VALUES ($1, $2, $3, $4, $5, 'web')",
    )
    .bind(device_id)
    .bind(user_id)
    .bind(vec![3_u8; 32])
    .bind(vec![4_u8; 32])
    .bind(vec![5_u8])
    .execute(&app.state.pool)
    .await
    .expect("insert device");
    sqlx::query(
        "INSERT INTO security_space_device_keys (space_id, user_id, device_id, key_epoch, encrypted_key_package) VALUES ($1, $2, $3, 1, $4)",
    )
    .bind(space_id)
    .bind(user_id)
    .bind(device_id)
    .bind(vec![6_u8; 49])
    .execute(&app.state.pool)
    .await
    .expect("insert device package");
    let recovery_package = vec![9_u8; 49];
    sqlx::query(
        "INSERT INTO security_space_recovery_keys (space_id, user_id, key_epoch, encrypted_key_package) VALUES ($1, $2, 1, $3)",
    )
    .bind(space_id)
    .bind(user_id)
    .bind(&recovery_package)
    .execute(&app.state.pool)
    .await
    .expect("insert recovery package");
    sqlx::query(
        "INSERT INTO user_passkeys (id, user_id, credential_id, passkey_data, encrypted_name) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(vec![10_u8; 32])
    .bind(vec![11_u8])
    .bind(vec![12_u8])
    .execute(&app.state.pool)
    .await
    .expect("insert passkey");

    let mut rng = OsRng;
    let registration =
        ClientRegistration::<DefaultOpaqueSuite>::start(&mut rng, b"New-P@ssword123!")
            .expect("new opaque registration start");
    let response = post_msgpack(
        &app.app,
        "/auth/account-recovery/start",
        &AccountRecoveryStartRequest {
            username: username.clone(),
            recovery_verifier: vec![7; 32],
            opaque_start_request: registration.message.serialize().to_vec(),
        },
        &HeaderMap::new(),
    )
    .await;
    let (status, _, body) = split_response(response).await;
    assert_eq!(status, StatusCode::OK);
    let recovery_start: AccountRecoveryStartResponse = decode_msgpack(&body);
    let registration_response = RegistrationResponse::<DefaultOpaqueSuite>::deserialize(
        &recovery_start.opaque_server_message,
    )
    .expect("deserialize recovery registration response");
    let registration_finish = registration
        .state
        .finish(
            &mut rng,
            b"New-P@ssword123!",
            registration_response,
            ClientRegistrationFinishParameters::default(),
        )
        .expect("finish recovery registration");
    let finish_request = AccountRecoveryFinishRequest {
        recovery_token: recovery_start.recovery_token,
        opaque_finish_request: registration_finish.message.serialize().to_vec(),
        encrypted_master_key: vec![13_u8; 64],
    };
    let response = post_msgpack(
        &app.app,
        "/auth/account-recovery/finish",
        &finish_request,
        &HeaderMap::new(),
    )
    .await;
    let (status, _, body) = split_response(response).await;
    assert_eq!(status, StatusCode::OK);
    let recovered: AccountRecoveryFinishResponse = decode_msgpack(&body);
    assert!(recovered.changed);
    assert!(recovered.totp_disabled);
    assert_eq!(recovered.space_key_packages.len(), 1);
    assert_eq!(recovered.space_key_packages[0].space_id, space_id);
    assert_eq!(
        recovered.space_key_packages[0].encrypted_key_package,
        recovery_package
    );

    let active_devices: i64 =
        sqlx::query_scalar("SELECT count(*) FROM devices WHERE user_id = $1 AND status = 'active'")
            .bind(user_id)
            .fetch_one(&app.state.pool)
            .await
            .expect("count devices");
    let passkeys: i64 = sqlx::query_scalar("SELECT count(*) FROM user_passkeys WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(&app.state.pool)
        .await
        .expect("count passkeys");
    let device_packages: i64 =
        sqlx::query_scalar("SELECT count(*) FROM security_space_device_keys WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(&app.state.pool)
            .await
            .expect("count device packages");
    assert_eq!((active_devices, passkeys, device_packages), (0, 0, 0));

    let token_hash: [u8; 32] = Sha256::digest(finish_request.recovery_token.as_bytes()).into();
    sqlx::query(
        "UPDATE account_recovery_attempts SET expires_at = now() - interval '1 minute' WHERE token_hash = $1",
    )
    .bind(token_hash.as_slice())
    .execute(&app.state.pool)
    .await
    .expect("expire completed recovery attempt");

    let replay = post_msgpack(
        &app.app,
        "/auth/account-recovery/finish",
        &finish_request,
        &HeaderMap::new(),
    )
    .await;
    let (status, _, body) = split_response(replay).await;
    assert_eq!(status, StatusCode::OK);
    let replayed: AccountRecoveryFinishResponse = decode_msgpack(&body);
    assert!(replayed.changed);
    assert_eq!(replayed.space_key_packages.len(), 1);

    let mut conflicting_request = finish_request;
    conflicting_request.encrypted_master_key[0] ^= 1;
    let conflict = post_msgpack(
        &app.app,
        "/auth/account-recovery/finish",
        &conflicting_request,
        &HeaderMap::new(),
    )
    .await;
    assert_eq!(split_response(conflict).await.0, StatusCode::UNAUTHORIZED);

    app.shutdown().await;
}

#[tokio::test]
async fn totp_backup_code_can_only_be_consumed_once() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let username = format!("it-backup-code-{}", Uuid::new_v4());
    register_user(&app, &username, "P@ssword123!").await;
    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(&username)
        .fetch_one(&app.state.pool)
        .await
        .expect("load registered user");
    let canonical = normalize_recovery_code("ABCD-EFGH-JKLM-NPQR").expect("canonical backup code");
    let code_hash = hash_account_recovery_code(&canonical);
    sqlx::query("INSERT INTO account_recovery_codes (id, user_id, code_hash) VALUES ($1, $2, $3)")
        .bind(Uuid::new_v4())
        .bind(user_id)
        .bind(&code_hash)
        .execute(&app.state.pool)
        .await
        .expect("insert backup code");

    assert!(
        consume_totp_backup_code(&app.state.pool, user_id, &code_hash)
            .await
            .unwrap()
    );
    assert!(
        !consume_totp_backup_code(&app.state.pool, user_id, &code_hash)
            .await
            .unwrap()
    );
    app.shutdown().await;
}

#[tokio::test]
async fn account_deletion_anonymizes_authentication_and_recovery_material() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let username = format!("it-delete-{}", Uuid::new_v4());
    register_user(&app, &username, "P@ssword123!").await;
    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(&username)
        .fetch_one(&app.state.pool)
        .await
        .expect("load registered user");
    sqlx::query(
        "INSERT INTO account_recovery_attempts (token_hash, user_id, expires_at) VALUES ($1, $2, now() + interval '10 minutes')",
    )
    .bind(vec![42_u8; 32])
    .bind(user_id)
    .execute(&app.state.pool)
    .await
    .expect("insert recovery attempt");

    let mut tx = app.state.pool.begin().await.unwrap();
    assert!(
        delete_user(
            &mut tx,
            crate::features::auth::repositories::AuthenticationBinding {
                user_id,
                generation: 0,
                session_id: None
            }
        )
        .await
        .expect("delete account")
    );
    tx.commit().await.unwrap();
    let state: (bool, bool, bool, bool, bool) = sqlx::query_as(
        r#"
        SELECT deleted_at IS NOT NULL,
               opaque_record IS NULL,
               encrypted_master_key IS NULL,
               public_key_bundle IS NULL,
               recovery_verifier_hash IS NULL
        FROM users WHERE id = $1
        "#,
    )
    .bind(user_id)
    .fetch_one(&app.state.pool)
    .await
    .expect("load anonymized account");
    assert_eq!(state, (true, true, true, true, true));
    let attempts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM account_recovery_attempts WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(&app.state.pool)
            .await
            .expect("count recovery attempts");
    assert_eq!(attempts, 0);
    let signup_completions: i64 =
        sqlx::query_scalar("SELECT count(*) FROM signup_completions WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(&app.state.pool)
            .await
            .expect("count signup idempotency records");
    assert_eq!(signup_completions, 0);

    app.shutdown().await;
}

async fn pending_signin(app: &TestApp, username: &str, password: &str) -> SigninFinishRequest {
    let mut rng = OsRng;
    let login_start = ClientLogin::<DefaultOpaqueSuite>::start(&mut rng, password.as_bytes())
        .expect("opaque login start");

    let signin_start_response = post_msgpack(
        &app.app,
        "/auth/signin/start",
        &SigninStartRequest {
            username: username.to_string(),
            opaque_start_request: login_start.message.serialize().to_vec(),
        },
        &HeaderMap::new(),
    )
    .await;
    let (status, _headers, body) = split_response(signin_start_response).await;
    assert_eq!(status, StatusCode::OK);
    let signin_start: SigninStartResponse = decode_msgpack(&body);

    let credential_response =
        CredentialResponse::<DefaultOpaqueSuite>::deserialize(&signin_start.opaque_server_message)
            .expect("deserialize credential response");
    let login_finish = login_start
        .state
        .finish(
            &mut rng,
            password.as_bytes(),
            credential_response,
            ClientLoginFinishParameters::default(),
        )
        .expect("opaque login finish");

    SigninFinishRequest {
        username: username.to_string(),
        opaque_flow_id: signin_start.opaque_flow_id,
        opaque_finish_request: login_finish.message.serialize().to_vec(),
        totp_code: None,
    }
}

async fn reset_deferred_account(app: &TestApp, username: &str, recovery: bool) {
    let request = prepare_signup_request(app, username, "New-P@ssword123!", Uuid::new_v4()).await;
    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.state.pool)
        .await
        .unwrap();
    if recovery {
        use crate::features::auth::repositories::{
            AccountRecoveryReset, apply_account_recovery_reset, create_account_recovery_attempt,
        };
        let token_hash = [42; 32];
        // Distinct durable recovery token for each synthetic account.
        let token_hash: [u8; 32] =
            Sha256::digest([token_hash.as_slice(), user_id.as_bytes()].concat()).into();
        create_account_recovery_attempt(
            &app.state.pool,
            &token_hash,
            user_id,
            time::OffsetDateTime::now_utc() + time::Duration::minutes(5),
        )
        .await
        .unwrap();
        apply_account_recovery_reset(
            &app.state.pool,
            AccountRecoveryReset {
                user_id,
                token_hash: &token_hash,
                request_hash: &[1; 32],
                opaque_record: &request.opaque_finish_request,
                encrypted_master_key: &[13; 64],
            },
        )
        .await
        .unwrap();
    } else {
        let mut tx = app.state.pool.begin().await.unwrap();
        crate::features::auth::repositories::update_user_password_file_and_revoke_refresh_sessions(
            &mut tx,
            user_id,
            &request.opaque_finish_request,
            &[13; 64],
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
    }
}

async fn active_session_count(app: &TestApp, username: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM refresh_tokens rt JOIN users u ON u.id = rt.user_id WHERE u.username = $1 AND rt.revoked_at IS NULL AND rt.expires_at > now()")
        .bind(username).fetch_one(&app.state.pool).await.unwrap()
}

async fn stale_opaque_regression(recovery: bool, totp: bool) {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let username = format!("it-deferred-{}", Uuid::new_v4());
    register_user(&app, &username, "P@ssword123!").await;
    if totp {
        let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
            .bind(&username)
            .fetch_one(&app.state.pool)
            .await
            .unwrap();
        let ciphertext = crate::platform::secret_box::encrypt_user_totp(
            &app.state.config.auth_totp_kek,
            &user_id.to_string(),
            "JBSWY3DPEHPK3PXP",
        )
        .unwrap();
        sqlx::query("UPDATE users SET totp_secret_ciphertext = $2 WHERE id = $1")
            .bind(user_id)
            .bind(ciphertext)
            .execute(&app.state.pool)
            .await
            .unwrap();
    }
    let pending = pending_signin(&app, &username, "P@ssword123!").await;
    let continuation = if totp {
        let response =
            post_msgpack(&app.app, "/auth/signin/finish", &pending, &HeaderMap::new()).await;
        let (status, _, body) = split_response(response).await;
        assert_eq!(status, StatusCode::OK);
        let result: SigninFinishResponse = decode_msgpack(&body);
        assert!(result.access_token.is_none());
        assert!(result.encrypted_master_key.is_empty());
        result.totp_continuation_token
    } else {
        None
    };
    reset_deferred_account(&app, &username, recovery).await;
    let response = if let Some(continuation_token) = continuation {
        post_msgpack(
            &app.app,
            "/auth/signin/totp",
            &crate::features::auth::dto::SigninTotpRequest {
                continuation_token,
                totp_code: crate::platform::security::auth::generate_totp_for_test(
                    "JBSWY3DPEHPK3PXP",
                    time::OffsetDateTime::now_utc(),
                ),
            },
            &HeaderMap::new(),
        )
        .await
    } else {
        post_msgpack(&app.app, "/auth/signin/finish", &pending, &HeaderMap::new()).await
    };
    assert_eq!(
        active_session_count(&app, &username).await,
        0,
        "stale proof minted a new active session"
    );
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    if totp && !recovery {
        sqlx::query("UPDATE users SET totp_secret_ciphertext = NULL WHERE username = $1")
            .bind(&username)
            .execute(&app.state.pool)
            .await
            .unwrap();
    }
    signin_user(&app, &username, "New-P@ssword123!", SigninTransport::Body).await;
    app.shutdown().await;
}

#[tokio::test]
async fn opaque_login_started_before_recovery_is_rejected() {
    stale_opaque_regression(true, false).await;
}
#[tokio::test]
async fn opaque_login_started_before_password_change_is_rejected() {
    stale_opaque_regression(false, false).await;
}
#[tokio::test]
async fn stale_totp_continuation_after_password_change_is_rejected() {
    stale_opaque_regression(false, true).await;
}
#[tokio::test]
async fn stale_totp_continuation_after_recovery_is_rejected() {
    stale_opaque_regression(true, true).await;
}

async fn stale_device_regression(reset: Option<bool>) {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let username = format!("it-deferred-device-{}", Uuid::new_v4());
    register_user(&app, &username, "P@ssword123!").await;
    let signin = signin_user(&app, &username, "P@ssword123!", SigninTransport::Body).await;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", signin.access_token)).unwrap(),
    );
    let enrollment = post_msgpack(
        &app.app,
        "/devices",
        &RegisterDeviceRequest {
            enrollment_token: signin.device_enrollment_token,
            device_id: Uuid::new_v4(),
            signing_public_key: ed25519_dalek::SigningKey::from_bytes(&[23; 32])
                .verifying_key()
                .to_bytes()
                .to_vec(),
            hpke_public_key: crypto_core_lib::CryptoEngine::generate_x25519_keypair()
                .public_key
                .to_vec(),
            encrypted_name: vec![1],
            platform: DevicePlatform::Web,
        },
        &headers,
    )
    .await;
    assert_eq!(enrollment.status(), StatusCode::OK);
    let response = post_msgpack(
        &app.app,
        "/auth/device-authorization/start",
        &DeviceAuthorizationStartRequest {
            hpke_public_key: vec![7; 32],
        },
        &HeaderMap::new(),
    )
    .await;
    let (_, _, body) = split_response(response).await;
    let started: DeviceAuthorizationStartResponse = decode_msgpack(&body);
    let response = post_msgpack(
        &app.app,
        "/auth/device-authorization/approve",
        &DeviceAuthorizationApproveRequest {
            user_code: started.user_code,
            encrypted_master_key_package: vec![9; 96],
        },
        &headers,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    if let Some(recovery) = reset {
        reset_deferred_account(&app, &username, recovery).await;
    } else {
        let response = post_msgpack(
            &app.app,
            "/auth/logout",
            &LogoutRequest {
                refresh_token: signin.refresh_token,
            },
            &HeaderMap::new(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }
    let response = post_msgpack(
        &app.app,
        "/auth/device-authorization/token",
        &DeviceAuthorizationTokenRequest {
            flow_id: started.flow_id,
            device_secret: started.device_secret,
        },
        &HeaderMap::new(),
    )
    .await;
    assert_eq!(
        active_session_count(&app, &username).await,
        0,
        "stale approval minted a new active session"
    );
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    signin_user(
        &app,
        &username,
        if reset.is_some() {
            "New-P@ssword123!"
        } else {
            "P@ssword123!"
        },
        SigninTransport::Body,
    )
    .await;
    app.shutdown().await;
}
#[tokio::test]
async fn device_approval_before_recovery_is_rejected() {
    stale_device_regression(Some(true)).await;
}
#[tokio::test]
async fn device_approval_before_password_change_is_rejected() {
    stale_device_regression(Some(false)).await;
}
#[tokio::test]
async fn device_approval_from_revoked_session_is_rejected() {
    stale_device_regression(None).await;
}

async fn enroll_signin(app: &TestApp, signin: &SigninArtifacts) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", signin.access_token)).unwrap(),
    );
    let response = post_msgpack(
        &app.app,
        "/devices",
        &RegisterDeviceRequest {
            enrollment_token: signin.device_enrollment_token.clone(),
            device_id: Uuid::new_v4(),
            signing_public_key: ed25519_dalek::SigningKey::from_bytes(&[23; 32])
                .verifying_key()
                .to_bytes()
                .to_vec(),
            hpke_public_key: crypto_core_lib::CryptoEngine::generate_x25519_keypair()
                .public_key
                .to_vec(),
            encrypted_name: vec![1],
            platform: DevicePlatform::Web,
        },
        &headers,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    headers
}

async fn pending_reauth(
    app: &TestApp,
    headers: &HeaderMap,
    password: &str,
    action: crate::features::auth::dto::ReauthAction,
) -> crate::features::auth::dto::ReauthFinishRequest {
    use crate::features::auth::dto::{
        ReauthFinishRequest, ReauthStartRequest, ReauthStartResponse,
    };
    let mut rng = OsRng;
    let started = ClientLogin::<DefaultOpaqueSuite>::start(&mut rng, password.as_bytes()).unwrap();
    let response = post_msgpack(
        &app.app,
        "/auth/reauth/start",
        &ReauthStartRequest {
            action,
            opaque_start_request: started.message.serialize().to_vec(),
        },
        headers,
    )
    .await;
    let (status, _, body) = split_response(response).await;
    assert_eq!(status, StatusCode::OK);
    let response: ReauthStartResponse = decode_msgpack(&body);
    let finished = started
        .state
        .finish(
            &mut rng,
            password.as_bytes(),
            CredentialResponse::<DefaultOpaqueSuite>::deserialize(&response.opaque_server_message)
                .unwrap(),
            ClientLoginFinishParameters::default(),
        )
        .unwrap();
    ReauthFinishRequest {
        action,
        opaque_flow_id: response.opaque_flow_id,
        opaque_finish_request: finished.message.serialize().to_vec(),
        totp_code: None,
    }
}

async fn reauth_proof(
    app: &TestApp,
    headers: &HeaderMap,
    password: &str,
    action: crate::features::auth::dto::ReauthAction,
) -> String {
    let pending = pending_reauth(app, headers, password, action).await;
    let response = post_msgpack(&app.app, "/auth/reauth/finish", &pending, headers).await;
    let (status, _, body) = split_response(response).await;
    assert_eq!(status, StatusCode::OK);
    decode_msgpack::<crate::features::auth::dto::ReauthFinishResponse>(&body).reauth_token
}

#[tokio::test]
async fn deferred_enrollment_and_reauth_cannot_cross_recovery() {
    use crate::features::auth::dto::{
        ReauthAction, TotpSetupFinishRequest, TotpSetupStartRequest, TotpSetupStartResponse,
    };
    let Some(mut app) = setup_test_app().await else {
        return;
    };
    app.state.config.enable_totp = true;
    app.app = build_router(app.state.clone()).unwrap();
    let username = format!("it-deferred-grants-{}", Uuid::new_v4());
    register_user(&app, &username, "P@ssword123!").await;
    let old_signin = signin_user(&app, &username, "P@ssword123!", SigninTransport::Body).await;
    let old_headers = enroll_signin(&app, &old_signin).await;
    let old_reauth = pending_reauth(
        &app,
        &old_headers,
        "P@ssword123!",
        ReauthAction::SecuritySettings,
    )
    .await;
    let old_proof = reauth_proof(
        &app,
        &old_headers,
        "P@ssword123!",
        ReauthAction::SecuritySettings,
    )
    .await;
    let setup_proof = reauth_proof(
        &app,
        &old_headers,
        "P@ssword123!",
        ReauthAction::SecuritySettings,
    )
    .await;
    let response = post_msgpack(
        &app.app,
        "/auth/totp/setup/start",
        &TotpSetupStartRequest {
            reauth_token: setup_proof,
        },
        &old_headers,
    )
    .await;
    let (status, _, body) = split_response(response).await;
    assert_eq!(status, StatusCode::OK);
    let setup: TotpSetupStartResponse = decode_msgpack(&body);
    reset_deferred_account(&app, &username, true).await;
    let fresh = signin_user(&app, &username, "New-P@ssword123!", SigninTransport::Body).await;
    let mut fresh_headers = HeaderMap::new();
    fresh_headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", fresh.access_token)).unwrap(),
    );
    let device_id = Uuid::new_v4();
    let response = post_msgpack(
        &app.app,
        "/devices",
        &RegisterDeviceRequest {
            enrollment_token: old_signin.device_enrollment_token,
            device_id,
            signing_public_key: ed25519_dalek::SigningKey::from_bytes(&[23; 32])
                .verifying_key()
                .to_bytes()
                .to_vec(),
            hpke_public_key: crypto_core_lib::CryptoEngine::generate_x25519_keypair()
                .public_key
                .to_vec(),
            encrypted_name: vec![1],
            platform: DevicePlatform::Web,
        },
        &fresh_headers,
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let inserted: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM devices WHERE id = $1)")
        .bind(device_id)
        .fetch_one(&app.state.pool)
        .await
        .unwrap();
    assert!(!inserted);
    let fresh_headers = enroll_signin(&app, &fresh).await;
    let response = post_msgpack(&app.app, "/auth/reauth/finish", &old_reauth, &fresh_headers).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = post_msgpack(
        &app.app,
        "/auth/totp/setup/start",
        &TotpSetupStartRequest {
            reauth_token: old_proof,
        },
        &fresh_headers,
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = post_msgpack(
        &app.app,
        "/auth/totp/setup/finish",
        &TotpSetupFinishRequest {
            flow_id: setup.flow_id,
            code: crate::platform::security::auth::generate_totp_for_test(
                &setup.manual_entry_key,
                time::OffsetDateTime::now_utc(),
            ),
        },
        &fresh_headers,
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let enabled: bool = sqlx::query_scalar(
        "SELECT totp_secret_ciphertext IS NOT NULL FROM users WHERE username = $1",
    )
    .bind(&username)
    .fetch_one(&app.state.pool)
    .await
    .unwrap();
    assert!(!enabled);
    // A newly authenticated proof still completes the real HTTP password path.
    let proof = reauth_proof(
        &app,
        &fresh_headers,
        "New-P@ssword123!",
        ReauthAction::ChangePassword,
    )
    .await;
    let registration =
        prepare_signup_request(&app, &username, "Third-P@ssword123!", Uuid::new_v4()).await;
    let response = post_msgpack(
        &app.app,
        "/auth/password/change/finish",
        &crate::features::auth::dto::PasswordChangeFinishRequest {
            reauth_token: proof,
            opaque_finish_request: registration.opaque_finish_request,
            encrypted_master_key: vec![14; 64],
        },
        &fresh_headers,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(active_session_count(&app, &username).await, 0);
    signin_user(&app, &username, "Third-P@ssword123!", SigninTransport::Body).await;
    app.shutdown().await;
}

#[tokio::test]
async fn login_final_validation_waits_for_concurrent_password_reset() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let username = format!("it-auth-lock-{}", Uuid::new_v4());
    register_user(&app, &username, "P@ssword123!").await;
    let pending = pending_signin(&app, &username, "P@ssword123!").await;
    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(&username)
        .fetch_one(&app.state.pool)
        .await
        .unwrap();
    let registration =
        prepare_signup_request(&app, &username, "New-P@ssword123!", Uuid::new_v4()).await;
    let mut reset_tx = app.state.pool.begin().await.unwrap();
    let blocker_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *reset_tx)
        .await
        .unwrap();
    crate::features::auth::repositories::update_user_password_file_and_revoke_refresh_sessions(
        &mut reset_tx,
        user_id,
        &registration.opaque_finish_request,
        &[13; 64],
    )
    .await
    .unwrap();
    let router = app.app.clone();
    let finishing = tokio::spawn(async move {
        post_msgpack(&router, "/auth/signin/finish", &pending, &HeaderMap::new()).await
    });
    // Observe the real lock wait rather than relying on a sleep to order the race.
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let blocked: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid)))").bind(blocker_pid).fetch_one(&app.state.pool).await.unwrap();
            if blocked { break; }
            assert!(!finishing.is_finished(), "login returned before reset released the user lock");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("login waits for reset transaction");
    reset_tx.commit().await.unwrap();
    let response = finishing.await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(active_session_count(&app, &username).await, 0);
    signin_user(&app, &username, "New-P@ssword123!", SigninTransport::Body).await;
    app.shutdown().await;
}
