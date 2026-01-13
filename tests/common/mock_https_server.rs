//! Mock HTTPS server for testing TLS certificate handling in Lua HTTP functions.

use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair,
    KeyUsagePurpose, SanType,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Once};
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

// Ensure the crypto provider is installed exactly once
static INIT_CRYPTO: Once = Once::new();

fn init_crypto() {
    INIT_CRYPTO.call_once(|| {
        rustls::crypto::ring::default_provider()
            .install_default()
            .expect("Failed to install default crypto provider");
    });
}

/// Generated certificate files for testing
pub struct CertFiles {
    /// Temporary directory holding the certificate files (must stay alive)
    pub _temp_dir: TempDir,
    /// Path to CA certificate (PEM)
    pub ca_cert: PathBuf,
    /// Path to server certificate (PEM) - written but not used by tests
    pub _server_cert: PathBuf,
    /// Path to server private key (PEM) - written but not used by tests
    pub _server_key: PathBuf,
    /// Path to client certificate (PEM)
    pub client_cert: PathBuf,
    /// Path to client private key (PEM)
    pub client_key: PathBuf,
    /// DER-encoded CA certificate for rustls
    pub ca_cert_der: CertificateDer<'static>,
    /// DER-encoded server certificate for rustls
    pub server_cert_der: CertificateDer<'static>,
    /// DER-encoded server private key for rustls
    pub server_key_der: PrivateKeyDer<'static>,
}

impl CertFiles {
    /// Generate a complete set of test certificates:
    /// - CA certificate (self-signed)
    /// - Server certificate (signed by CA)
    /// - Client certificate (signed by CA)
    pub fn generate() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let temp_dir = TempDir::new()?;

        // Generate CA key pair and certificate
        let ca_key = KeyPair::generate()?;
        let mut ca_params = CertificateParams::default();
        ca_params
            .distinguished_name
            .push(DnType::CommonName, "Test CA");
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        let ca_cert = ca_params.self_signed(&ca_key)?;

        // Generate server key pair and certificate
        let server_key = KeyPair::generate()?;
        let mut server_params = CertificateParams::default();
        server_params
            .distinguished_name
            .push(DnType::CommonName, "localhost");
        server_params.subject_alt_names = vec![
            SanType::DnsName("localhost".try_into()?),
            SanType::IpAddress(std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))),
        ];
        server_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        server_params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyEncipherment,
        ];
        let server_cert = server_params.signed_by(&server_key, &ca_cert, &ca_key)?;

        // Generate client key pair and certificate
        let client_key = KeyPair::generate()?;
        let mut client_params = CertificateParams::default();
        client_params
            .distinguished_name
            .push(DnType::CommonName, "Test Client");
        client_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
        client_params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        let client_cert = client_params.signed_by(&client_key, &ca_cert, &ca_key)?;

        // Write certificates to files
        let ca_cert_path = temp_dir.path().join("ca.pem");
        let server_cert_path = temp_dir.path().join("server.pem");
        let server_key_path = temp_dir.path().join("server-key.pem");
        let client_cert_path = temp_dir.path().join("client.pem");
        let client_key_path = temp_dir.path().join("client-key.pem");

        std::fs::File::create(&ca_cert_path)?.write_all(ca_cert.pem().as_bytes())?;
        std::fs::File::create(&server_cert_path)?.write_all(server_cert.pem().as_bytes())?;
        std::fs::File::create(&server_key_path)?
            .write_all(server_key.serialize_pem().as_bytes())?;
        std::fs::File::create(&client_cert_path)?.write_all(client_cert.pem().as_bytes())?;
        std::fs::File::create(&client_key_path)?
            .write_all(client_key.serialize_pem().as_bytes())?;

        // Keep DER versions for rustls server config
        let ca_cert_der = CertificateDer::from(ca_cert.der().to_vec());
        let server_cert_der = CertificateDer::from(server_cert.der().to_vec());
        let server_key_der = PrivateKeyDer::try_from(server_key.serialize_der()).unwrap();

        Ok(Self {
            _temp_dir: temp_dir,
            ca_cert: ca_cert_path,
            _server_cert: server_cert_path,
            _server_key: server_key_path,
            client_cert: client_cert_path,
            client_key: client_key_path,
            ca_cert_der,
            server_cert_der,
            server_key_der,
        })
    }
}

/// Mock HTTPS server for testing TLS connections
pub struct MockHttpsServer {
    /// Server address
    pub addr: SocketAddr,
    /// Generated certificates
    pub certs: CertFiles,
    /// Shutdown signal sender
    shutdown_tx: Option<oneshot::Sender<()>>,
    /// Server task handle
    _handle: Option<tokio::task::JoinHandle<()>>,
}

impl MockHttpsServer {
    /// Start a mock HTTPS server that accepts connections with the generated server certificate.
    /// The server responds to all requests with a simple JSON response.
    pub async fn start() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Self::start_with_client_auth(false).await
    }

    /// Start a mock HTTPS server that requires client certificate authentication.
    pub async fn start_with_client_auth(
        require_client_cert: bool,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Initialize crypto provider
        init_crypto();

        let certs = CertFiles::generate()?;

        // Build TLS config
        let mut server_config = if require_client_cert {
            // Require client certificate verification using our CA
            let mut root_store = rustls::RootCertStore::empty();
            root_store.add(certs.ca_cert_der.clone())?;

            let client_verifier =
                rustls::server::WebPkiClientVerifier::builder(Arc::new(root_store))
                    .build()
                    .map_err(|e| format!("Failed to build client verifier: {}", e))?;

            rustls::ServerConfig::builder()
                .with_client_cert_verifier(client_verifier)
                .with_single_cert(
                    vec![certs.server_cert_der.clone()],
                    certs.server_key_der.clone_key(),
                )
                .map_err(|e| format!("Failed to build server config: {}", e))?
        } else {
            rustls::ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(
                    vec![certs.server_cert_der.clone()],
                    certs.server_key_der.clone_key(),
                )
                .map_err(|e| format!("Failed to build server config: {}", e))?
        };

        // Enable HTTP/1.1
        server_config.alpn_protocols = vec![b"http/1.1".to_vec()];

        let tls_acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config));

        // Bind to a random available port
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;

        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

        // Spawn the server task
        let handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        break;
                    }
                    accept_result = listener.accept() => {
                        if let Ok((stream, _)) = accept_result {
                            let acceptor = tls_acceptor.clone();
                            tokio::spawn(async move {
                                if let Ok(tls_stream) = acceptor.accept(stream).await {
                                    // Handle the TLS connection
                                    handle_connection(tls_stream).await;
                                }
                            });
                        }
                    }
                }
            }
        });

        Ok(Self {
            addr,
            certs,
            shutdown_tx: Some(shutdown_tx),
            _handle: Some(handle),
        })
    }

    /// Get the base URL of the mock server
    pub fn url(&self) -> String {
        format!("https://127.0.0.1:{}", self.addr.port())
    }
}

impl Drop for MockHttpsServer {
    fn drop(&mut self) {
        // Send shutdown signal if not already sent
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

/// Handle a TLS connection by reading the HTTP request and sending a response
async fn handle_connection(mut stream: tokio_rustls::server::TlsStream<tokio::net::TcpStream>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut buf = [0u8; 4096];
    if stream.read(&mut buf).await.is_ok() {
        // Parse the request to get the path
        let request = String::from_utf8_lossy(&buf);
        let path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/");

        // Generate response based on path
        let (status, body) = match path {
            "/health" => ("200 OK", r#"{"status": "healthy"}"#),
            "/data" => ("200 OK", r#"{"message": "Hello from HTTPS!"}"#),
            _ => ("200 OK", r#"{"path": "unknown"}"#),
        };

        let response = format!(
            "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            status,
            body.len(),
            body
        );

        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.flush().await;
    }
}
