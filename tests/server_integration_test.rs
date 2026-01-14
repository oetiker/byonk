//! Server integration tests that test the actual server behavior.
//!
//! These tests start a real TCP server and verify behavior that can only
//! be tested with actual network connections.

use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use byonk::assets::AssetLoader;
use byonk::server::{build_router, create_app_state};

/// Start a test server on an available port and return the port number.
async fn start_test_server() -> u16 {
    let asset_loader = Arc::new(AssetLoader::new(None, None, None));
    let state = create_app_state(asset_loader).expect("Failed to create app state");
    let app = build_router(state);

    // Bind to port 0 to get an available port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind");
    let port = listener.local_addr().unwrap().port();

    // Spawn the server in the background
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });

    // Give the server a moment to start
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    port
}

/// Verify that the server closes connections after each response.
///
/// ESP32 HTTPClient defaults to keep-alive but never reuses connections,
/// causing orphaned connections to accumulate. We send "Connection: close"
/// to prevent this.
///
/// This test verifies the server actually closes the TCP connection after
/// responding, not just that the header is present.
#[tokio::test]
async fn test_server_closes_connection_after_response() {
    let port = start_test_server().await;

    // Connect to the server
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .expect("Failed to connect");

    // Send a simple HTTP request
    let request = "GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: keep-alive\r\n\r\n";
    stream
        .write_all(request.as_bytes())
        .await
        .expect("Failed to write request");

    // Read the response
    let mut response = vec![0u8; 4096];
    let n = stream
        .read(&mut response)
        .await
        .expect("Failed to read response");
    assert!(n > 0, "Should have received a response");

    let response_str = String::from_utf8_lossy(&response[..n]);
    assert!(
        response_str.contains("HTTP/1.1 200"),
        "Should get 200 OK response"
    );
    assert!(
        response_str.to_lowercase().contains("connection: close"),
        "Response should have Connection: close header"
    );

    // Try to read again - should get EOF (0 bytes) if server closed the connection
    // We set a short timeout to avoid hanging if the connection is still open
    let read_result = tokio::time::timeout(
        tokio::time::Duration::from_millis(100),
        stream.read(&mut response),
    )
    .await;

    match read_result {
        Ok(Ok(0)) => {
            // EOF - connection was closed by server (expected)
        }
        Ok(Ok(_n)) => {
            panic!("Server sent unexpected data after response - connection should be closed");
        }
        Ok(Err(e)) => {
            // Connection reset or similar - also indicates server closed
            // This is acceptable behavior
            assert!(
                e.kind() == std::io::ErrorKind::ConnectionReset
                    || e.kind() == std::io::ErrorKind::BrokenPipe,
                "Unexpected error kind: {:?}",
                e.kind()
            );
        }
        Err(_) => {
            panic!("Timeout waiting for connection close - server may not be closing connections");
        }
    }
}

/// Verify that multiple requests each get their own connection.
/// This confirms the server isn't keeping connections alive.
#[tokio::test]
async fn test_server_does_not_reuse_connections() {
    let port = start_test_server().await;

    // Make first request
    let mut stream1 = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .expect("Failed to connect");

    stream1
        .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .expect("Failed to write");

    let mut buf = vec![0u8; 4096];
    let n = stream1.read(&mut buf).await.expect("Failed to read");
    assert!(n > 0);
    let response1 = String::from_utf8_lossy(&buf[..n]);
    assert!(response1.contains("200 OK"));

    // Try to read more - should be closed
    let n = stream1.read(&mut buf).await.expect("Failed to read");
    assert_eq!(n, 0, "First connection should be closed after response");

    // Make second request - should work fine with a new connection
    let mut stream2 = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .expect("Failed to connect for second request");

    stream2
        .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .expect("Failed to write second request");

    let n = stream2.read(&mut buf).await.expect("Failed to read");
    assert!(n > 0);
    let response2 = String::from_utf8_lossy(&buf[..n]);
    assert!(
        response2.contains("200 OK"),
        "Second request should succeed"
    );
}
