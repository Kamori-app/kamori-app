//! Native HTTPS policy shared by FRB authentication and the sync runtime.
//!
//! Desktop and Apple platforms use reqwest's platform verifier. Android's
//! verifier needs JVM/Context initialization unavailable in the hosted FRB path;
//! use Mozilla's bundled public roots there, with strict webpki verification.

pub fn client_builder() -> reqwest::ClientBuilder {
    let builder = reqwest::Client::builder();
    #[cfg(target_os = "android")]
    let builder = android_client_builder(builder);
    builder
}

#[cfg(any(target_os = "android", test))]
fn android_client_builder(builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    let roots = rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    android_client_builder_with_roots(builder, roots)
}

#[cfg(any(target_os = "android", test))]
fn android_client_builder_with_roots(
    builder: reqwest::ClientBuilder,
    roots: rustls::RootCertStore,
) -> reqwest::ClientBuilder {
    let config = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("supported TLS protocol versions")
    .with_root_certificates(roots)
    .with_no_client_auth();
    builder.tls_backend_preconfigured(config)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::{sync::Arc, time::Duration};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // The fixture key is synthetic, public test data, and is never a client identity.
    pub(crate) async fn assert_https_policy(factory: fn() -> reqwest::ClientBuilder) {
        assert_https_policy_with_roots(factory, |builder, ca| builder.tls_certs_only([ca])).await;
    }

    async fn assert_https_policy_with_roots(
        factory: fn() -> reqwest::ClientBuilder,
        trust: fn(reqwest::ClientBuilder, reqwest::Certificate) -> reqwest::ClientBuilder,
    ) {
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let config = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(
                vec![
                    include_bytes!("../tests/fixtures/tls/server.der")
                        .to_vec()
                        .into(),
                ],
                rustls::pki_types::PrivatePkcs8KeyDer::from(
                    include_bytes!("../tests/fixtures/tls/server-key.der").to_vec(),
                )
                .into(),
            )
            .unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let acceptor = acceptor.clone();
                tokio::spawn(async move {
                    if let Ok(mut stream) = acceptor.accept(stream).await {
                        let mut request = [0; 4096];
                        if stream.read(&mut request).await.is_ok() {
                            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nsecure").await;
                            let _ = stream.shutdown().await;
                        }
                    }
                });
            }
        });
        let url = format!("https://localhost:{}/", address.port());
        let untrusted = factory()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let error = untrusted.get(&url).send().await.unwrap_err();
        assert!(
            format!("{error:?}").contains("Certificate"),
            "expected certificate validation: {error:?}"
        );
        let ca =
            reqwest::Certificate::from_pem(include_bytes!("../tests/fixtures/tls/ca.pem")).unwrap();
        let trusted = trust(factory(), ca)
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .resolve("wrong-host.invalid", address)
            .build()
            .unwrap();
        assert_eq!(
            trusted
                .get(&url)
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap(),
            "secure"
        );
        let wrong_host = format!("https://wrong-host.invalid:{}/", address.port());
        let error = trusted.get(wrong_host).send().await.unwrap_err();
        assert!(
            format!("{error:?}").contains("NotValidForName"),
            "expected hostname validation: {error:?}"
        );
        server.abort();
        server.await.unwrap_err();
    }

    #[tokio::test]
    async fn shared_native_factory_verifies_synthetic_ca_and_hostname() {
        assert_https_policy(client_builder).await;
    }

    #[tokio::test]
    async fn android_bundled_roots_build_without_jvm_and_reject_private_ca() {
        // Runs the Android connector on the host: no Java VM or system trust edits.
        assert_https_policy_with_roots(
            || android_client_builder(reqwest::Client::builder()),
            |builder, _| {
                let mut roots = rustls::RootCertStore::empty();
                roots
                    .add(
                        include_bytes!("../tests/fixtures/tls/ca.der")
                            .to_vec()
                            .into(),
                    )
                    .unwrap();
                android_client_builder_with_roots(builder, roots)
            },
        )
        .await;
    }
}
