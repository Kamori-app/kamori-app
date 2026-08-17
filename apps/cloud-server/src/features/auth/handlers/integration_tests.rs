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
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower::util::ServiceExt;
use uuid::Uuid;

use crate::{
    app::router::build_router,
    features::{
        auth::{
            dto::{
                AccountRecoveryFinishRequest, AccountRecoveryFinishResponse,
                AccountRecoveryStartRequest, AccountRecoveryStartResponse,
                DeviceAuthorizationApproveRequest, DeviceAuthorizationApproveResponse,
                DeviceAuthorizationStartRequest, DeviceAuthorizationStartResponse,
                DeviceAuthorizationStatus, DeviceAuthorizationTokenRequest,
                DeviceAuthorizationTokenResponse, LogoutRequest, LogoutResponse, RefreshRequest,
                RefreshResponse, SigninFinishRequest, SigninFinishResponse, SigninStartRequest,
                SigninStartResponse, SignupFinishRequest, SignupFinishResponse, SignupStartRequest,
                SignupStartResponse,
            },
            repositories::consume_totp_backup_code,
            services::support::{hash_account_recovery_code, normalize_recovery_code},
            transport::{CSRF_HEADER, REFRESH_TRANSPORT_HEADER},
        },
        common::ErrorResponse,
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
    let body = rmp_serde::to_vec_named(payload).expect("encode msgpack request");
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
    rmp_serde::from_slice(body).expect("decode msgpack response")
}

fn decode_json<T: DeserializeOwned>(body: &[u8]) -> T {
    serde_json::from_slice(body).expect("decode json response")
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

async fn register_user(app: &TestApp, username: &str, password: &str) {
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

    let signup_finish_response = post_msgpack(
        &app.app,
        "/auth/signup/finish",
        &SignupFinishRequest {
            username: username.to_string(),
            opaque_finish_request: registration_finish.message.serialize().to_vec(),
            encrypted_master_key: vec![1; 49],
            public_key_bundle: vec![4, 5, 6],
            recovery_verifier: vec![7; 32],
        },
        &HeaderMap::new(),
    )
    .await;
    let (status, _headers, body) = split_response(signup_finish_response).await;
    assert_eq!(status, StatusCode::OK);
    let _signup_finish: SignupFinishResponse = decode_msgpack(&body);
}

async fn signin_user(
    app: &TestApp,
    username: &str,
    password: &str,
    transport: SigninTransport,
) -> SigninArtifacts {
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

    let mut headers = HeaderMap::new();
    if matches!(transport, SigninTransport::Cookie) {
        headers.insert(REFRESH_TRANSPORT_HEADER, HeaderValue::from_static("cookie"));
    }
    let signin_finish_response = post_msgpack(
        &app.app,
        "/auth/signin/finish",
        &SigninFinishRequest {
            username: username.to_string(),
            opaque_flow_id: signin_start.opaque_flow_id,
            opaque_finish_request: login_finish.message.serialize().to_vec(),
            totp_code: None,
            preauth_token: None,
        },
        &headers,
    )
    .await;

    let (status, response_headers, body) = split_response(signin_finish_response).await;
    assert_eq!(status, StatusCode::OK);
    let payload: SigninFinishResponse = decode_msgpack(&body);
    assert!(payload.access_token.is_some());
    assert!(payload.totp_verified);
    assert!(payload.preauth_token.is_none());

    SigninArtifacts {
        access_token: payload.access_token.expect("access token"),
        refresh_token: payload.refresh_token,
        set_cookie_headers: set_cookie_values(&response_headers),
    }
}

#[tokio::test]
async fn external_browser_device_authorization_is_explicit_and_single_use() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let username = format!("it-device-flow-{}", Uuid::new_v4());
    let password = "P@ssword123!";
    register_user(&app, &username, password).await;
    let signin = signin_user(&app, &username, password, SigninTransport::Body).await;

    let start_response = post_msgpack(
        &app.app,
        "/auth/device-authorization/start",
        &DeviceAuthorizationStartRequest {},
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
    let approve_response = post_msgpack(
        &app.app,
        "/auth/device-authorization/approve",
        &DeviceAuthorizationApproveRequest {
            user_code: started.user_code.clone(),
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
    assert_eq!(status, StatusCode::OK);
    let token: DeviceAuthorizationTokenResponse = decode_msgpack(&body);
    assert_eq!(token.status, DeviceAuthorizationStatus::Approved);
    assert_eq!(token.username.as_deref(), Some(username.as_str()));
    assert!(token.access_token.is_some());
    assert!(token.refresh_token.is_some());

    let replay_response = post_msgpack(
        &app.app,
        "/auth/device-authorization/token",
        &token_request,
        &HeaderMap::new(),
    )
    .await;
    let (status, _, _) = split_response(replay_response).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

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

    let mut refresh_headers = HeaderMap::new();
    refresh_headers.insert(REFRESH_TRANSPORT_HEADER, HeaderValue::from_static("cookie"));
    refresh_headers.insert(header::ORIGIN, HeaderValue::from_static(ALLOWED_ORIGIN));
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
    let (status, response_headers, body) = split_response(refresh_response).await;
    assert_eq!(status, StatusCode::OK);
    let refresh_payload: RefreshResponse = decode_msgpack(&body);
    assert!(refresh_payload.refresh_token.is_none());

    let refresh_set_cookies = set_cookie_values(&response_headers);
    let rotated_refresh_cookie =
        cookie_value(&refresh_set_cookies, &refresh_cookie_name).expect("rotated refresh cookie");
    let rotated_csrf_cookie =
        cookie_value(&refresh_set_cookies, &csrf_cookie_name).expect("rotated csrf cookie");

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
    let error: ErrorResponse = decode_json(&body);
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
    let error: ErrorResponse = decode_json(&body);
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

    let replay = post_msgpack(
        &app.app,
        "/auth/account-recovery/finish",
        &finish_request,
        &HeaderMap::new(),
    )
    .await;
    assert_eq!(split_response(replay).await.0, StatusCode::UNAUTHORIZED);

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
