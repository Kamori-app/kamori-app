//! OPAQUE protocol helpers for password-based authentication.
use crate::platform::state_store::StateStore;
use anyhow::{Result, anyhow};
use base64::Engine as _;
use opaque_ke::argon2::Argon2;
use opaque_ke::ciphersuite::CipherSuite;
use opaque_ke::key_exchange::tripledh::TripleDh;
use opaque_ke::{
    CredentialFinalization, CredentialRequest, CredentialResponse, RegistrationRequest,
    RegistrationResponse, RegistrationUpload, Ristretto255, ServerLogin, ServerLoginParameters,
    ServerRegistration, ServerSetup,
};
use rand08::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sha2_opaque::Sha512;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// Default OPAQUE ciphersuite for server-side flows.
pub struct DefaultOpaqueSuite;

impl CipherSuite for DefaultOpaqueSuite {
    type OprfCs = Ristretto255;
    type KeyExchange = TripleDh<Ristretto255, Sha512>;
    type Ksf = Argon2<'static>;
}

/// Result of a completed OPAQUE login.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OpaqueLoginResult {
    /// Derived session key bytes.
    pub session_key: Vec<u8>,
}

/// Server response and opaque handle for one isolated login exchange.
pub struct OpaqueLoginStart {
    /// Random handle required by the finish call.
    pub flow_id: uuid::Uuid,
    /// Serialized OPAQUE credential response.
    pub message: Vec<u8>,
}

/// In-memory OPAQUE server state.
pub struct OpaqueServer {
    setup: ServerSetup<DefaultOpaqueSuite>,
    state_store: Arc<dyn StateStore>,
}

impl OpaqueServer {
    /// Creates an ephemeral OPAQUE server instance for tests and explicit local use.
    pub fn new(state_store: Arc<dyn StateStore>) -> Result<Self> {
        let mut rng = OsRng;
        let setup = ServerSetup::<DefaultOpaqueSuite>::new(&mut rng);
        Ok(Self { setup, state_store })
    }

    /// Restores a deployment-owned setup from its canonical serialized bytes.
    pub fn from_serialized_setup(
        state_store: Arc<dyn StateStore>,
        serialized: &[u8],
    ) -> Result<Self> {
        let setup = ServerSetup::<DefaultOpaqueSuite>::deserialize(serialized)
            .map_err(|e| anyhow!("deserialize OPAQUE server setup: {e:?}"))?;
        Ok(Self { setup, state_store })
    }

    /// Loads a standard-base64 setup file, or creates an ephemeral setup only when allowed.
    pub fn load(
        state_store: Arc<dyn StateStore>,
        setup_path: Option<&str>,
        allow_ephemeral: bool,
    ) -> Result<Self> {
        let Some(setup_path) = setup_path else {
            if allow_ephemeral {
                return Self::new(state_store);
            }
            return Err(anyhow!("persistent OPAQUE server setup is required"));
        };
        let encoded = std::fs::read_to_string(Path::new(setup_path))
            .map_err(|e| anyhow!("read OPAQUE server setup file {setup_path:?}: {e}"))?;
        let serialized = base64::engine::general_purpose::STANDARD
            .decode(encoded.trim())
            .map_err(|_| anyhow!("OPAQUE server setup file must contain standard base64"))?;
        Self::from_serialized_setup(state_store, &serialized)
    }

    /// Generates canonical bytes for one new deployment setup.
    pub fn generate_serialized_setup() -> Vec<u8> {
        let mut rng = OsRng;
        ServerSetup::<DefaultOpaqueSuite>::new(&mut rng)
            .serialize()
            .to_vec()
    }

    /// Returns a non-secret fingerprint used to detect mismatched app nodes.
    pub fn setup_fingerprint(&self) -> [u8; 32] {
        Sha256::digest(self.setup.serialize()).into()
    }

    /// Starts OPAQUE registration and returns the server message bytes.
    pub async fn registration_start(
        &self,
        username: &str,
        request_bytes: &[u8],
    ) -> Result<Vec<u8>> {
        let request = RegistrationRequest::<DefaultOpaqueSuite>::deserialize(request_bytes)
            .map_err(|e| anyhow!("deserialize registration request: {e:?}"))?;

        let start_result = ServerRegistration::<DefaultOpaqueSuite>::start(
            &self.setup,
            request,
            username.as_bytes(),
        )
        .map_err(|e| anyhow!("opaque registration start failed: {e:?}"))?;

        Ok(start_result.message.serialize().to_vec())
    }

    /// Finishes OPAQUE registration and returns the password file bytes.
    pub async fn registration_finish(
        &self,
        _username: &str,
        upload_bytes: &[u8],
    ) -> Result<Vec<u8>> {
        let upload = RegistrationUpload::<DefaultOpaqueSuite>::deserialize(upload_bytes)
            .map_err(|e| anyhow!("deserialize registration upload: {e:?}"))?;

        let password_file = ServerRegistration::<DefaultOpaqueSuite>::finish(upload);
        Ok(password_file.serialize().to_vec())
    }

    /// Starts OPAQUE login and returns the server message bytes.
    pub async fn login_start(
        &self,
        username: &str,
        request_bytes: &[u8],
        password_file_bytes: Option<&[u8]>,
    ) -> Result<OpaqueLoginStart> {
        let request = CredentialRequest::<DefaultOpaqueSuite>::deserialize(request_bytes)
            .map_err(|e| anyhow!("deserialize credential request: {e:?}"))?;

        let password_file = password_file_bytes
            .map(ServerRegistration::<DefaultOpaqueSuite>::deserialize)
            .transpose()
            .map_err(|e| anyhow!("deserialize password file: {e:?}"))?;

        let mut rng = OsRng;
        let start_result = ServerLogin::<DefaultOpaqueSuite>::start(
            &mut rng,
            &self.setup,
            password_file,
            request,
            username.as_bytes(),
            ServerLoginParameters::default(),
        )
        .map_err(|e| anyhow!("opaque login start failed: {e:?}"))?;

        let state_bytes = start_result.state.serialize().to_vec();
        let flow_id = uuid::Uuid::new_v4();
        let key = login_state_key(username, flow_id);
        self.state_store
            .put(&key, &state_bytes, Duration::from_secs(0))
            .await
            .map_err(map_store_error)?;

        Ok(OpaqueLoginStart {
            flow_id,
            message: start_result.message.serialize().to_vec(),
        })
    }

    /// Finishes OPAQUE login and returns the session key.
    pub async fn login_finish(
        &self,
        username: &str,
        flow_id: uuid::Uuid,
        finish_bytes: &[u8],
    ) -> Result<OpaqueLoginResult> {
        let finish = CredentialFinalization::<DefaultOpaqueSuite>::deserialize(finish_bytes)
            .map_err(|e| anyhow!("deserialize credential finalization: {e:?}"))?;

        let key = login_state_key(username, flow_id);
        let state_bytes = self
            .state_store
            .take(&key)
            .await
            .map_err(map_store_error)?
            .ok_or_else(|| anyhow!("missing login state for user"))?;

        let state = ServerLogin::<DefaultOpaqueSuite>::deserialize(&state_bytes)
            .map_err(|e| anyhow!("deserialize login state: {e:?}"))?;

        let finish_result = state
            .finish(finish, ServerLoginParameters::default())
            .map_err(|e| anyhow!("opaque login finish failed: {e:?}"))?;

        Ok(OpaqueLoginResult {
            session_key: finish_result.session_key.to_vec(),
        })
    }

    /// Exposes the server setup for external consumers.
    pub fn export_setup(&self) -> &ServerSetup<DefaultOpaqueSuite> {
        &self.setup
    }
}

/// Decodes a registration response from raw bytes.
#[allow(dead_code)]
pub fn decode_registration_response(
    bytes: &[u8],
) -> Result<RegistrationResponse<DefaultOpaqueSuite>> {
    RegistrationResponse::<DefaultOpaqueSuite>::deserialize(bytes)
        .map_err(|e| anyhow!("deserialize registration response: {e:?}"))
}

/// Decodes a credential response from raw bytes.
#[allow(dead_code)]
pub fn decode_credential_response(bytes: &[u8]) -> Result<CredentialResponse<DefaultOpaqueSuite>> {
    CredentialResponse::<DefaultOpaqueSuite>::deserialize(bytes)
        .map_err(|e| anyhow!("deserialize credential response: {e:?}"))
}

fn login_state_key(username: &str, flow_id: uuid::Uuid) -> String {
    let user_scope = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(Sha256::digest(username.as_bytes()));
    format!("opaque:login:{user_scope}:{flow_id}")
}

fn map_store_error(err: crate::platform::state_store::StateStoreError) -> anyhow::Error {
    anyhow!("state store error: {err}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::state_store::InMemoryStore;
    use opaque_ke::{
        ClientLogin, ClientLoginFinishParameters, ClientRegistration,
        ClientRegistrationFinishParameters, CredentialResponse, RegistrationResponse,
    };
    use rand08::rngs::OsRng;
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn serialized_setup_survives_server_restart() {
        let serialized = OpaqueServer::generate_serialized_setup();
        let first = OpaqueServer::from_serialized_setup(
            Arc::new(InMemoryStore::new(Duration::from_secs(300))),
            &serialized,
        )
        .expect("first server");
        let restarted = OpaqueServer::from_serialized_setup(
            Arc::new(InMemoryStore::new(Duration::from_secs(300))),
            &serialized,
        )
        .expect("restarted server");

        assert_eq!(first.setup_fingerprint(), restarted.setup_fingerprint());
    }

    #[test]
    fn registration_flow_roundtrip() {
        let store = Arc::new(InMemoryStore::new(Duration::from_secs(300)));
        let server = OpaqueServer::new(store).expect("server");
        let mut rng = OsRng;
        let password = b"password123";

        let client_start = ClientRegistration::<DefaultOpaqueSuite>::start(&mut rng, password)
            .expect("client start");
        let registration_request_bytes = client_start.message.serialize().to_vec();

        let rt = tokio::runtime::Runtime::new().expect("rt");
        let server_response_bytes = rt
            .block_on(server.registration_start("alice", &registration_request_bytes))
            .expect("server start");

        let client_finish = client_start
            .state
            .finish(
                &mut rng,
                password,
                RegistrationResponse::<DefaultOpaqueSuite>::deserialize(&server_response_bytes)
                    .expect("deserialize"),
                ClientRegistrationFinishParameters::default(),
            )
            .expect("client finish");
        let upload_bytes = client_finish.message.serialize().to_vec();

        let password_file_bytes = rt
            .block_on(server.registration_finish("alice", &upload_bytes))
            .expect("server finish");

        assert!(!password_file_bytes.is_empty());
    }

    #[test]
    fn login_flow_roundtrip() {
        let store = Arc::new(InMemoryStore::new(Duration::from_secs(300)));
        let server = OpaqueServer::new(store).expect("server");
        let mut rng = OsRng;
        let password = b"password123";

        let client_start = ClientRegistration::<DefaultOpaqueSuite>::start(&mut rng, password)
            .expect("client start");
        let registration_request_bytes = client_start.message.serialize().to_vec();
        let rt = tokio::runtime::Runtime::new().expect("rt");
        let server_response_bytes = rt
            .block_on(server.registration_start("alice", &registration_request_bytes))
            .expect("server start");

        let client_finish = client_start
            .state
            .finish(
                &mut rng,
                password,
                RegistrationResponse::<DefaultOpaqueSuite>::deserialize(&server_response_bytes)
                    .expect("deserialize"),
                ClientRegistrationFinishParameters::default(),
            )
            .expect("client finish");
        let upload_bytes = client_finish.message.serialize().to_vec();

        let password_file_bytes = rt
            .block_on(server.registration_finish("alice", &upload_bytes))
            .expect("server finish");

        let client_login = ClientLogin::<DefaultOpaqueSuite>::start(&mut rng, password)
            .expect("client login start");
        let credential_request_bytes = client_login.message.serialize().to_vec();

        let server_login = rt
            .block_on(server.login_start(
                "alice",
                &credential_request_bytes,
                Some(&password_file_bytes),
            ))
            .expect("server login start");

        let client_finish = client_login
            .state
            .finish(
                &mut rng,
                password,
                CredentialResponse::<DefaultOpaqueSuite>::deserialize(&server_login.message)
                    .expect("deserialize"),
                ClientLoginFinishParameters::default(),
            )
            .expect("client login finish");

        let credential_finalization_bytes = client_finish.message.serialize().to_vec();
        let server_finish = rt
            .block_on(server.login_finish(
                "alice",
                server_login.flow_id,
                &credential_finalization_bytes,
            ))
            .expect("server login finish");

        assert_eq!(
            server_finish.session_key,
            client_finish.session_key.to_vec()
        );
    }
}
