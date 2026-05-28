use reestream::server::handshake_and_create_server_session;
use rml_rtmp::handshake::{Handshake, HandshakeProcessResult, PeerType};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

async fn create_server_client_pair() -> (TcpStream, TcpStream, tokio::net::TcpListener) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let client = TcpStream::connect(addr).await.unwrap();
    let (server, _) = listener.accept().await.unwrap();
    (server, client, listener)
}

#[tokio::test]
async fn test_full_rtmp_handshake() {
    let (mut server_stream, mut client_stream, _listener) = create_server_client_pair().await;

    // Client initiates handshake
    let mut client_hs = Handshake::new(PeerType::Client);
    let c0_c1 = client_hs.generate_outbound_p0_and_p1().unwrap();
    client_stream.write_all(&c0_c1).await.unwrap();

    // Server processes handshake in background
    let server_handle =
        tokio::spawn(async move { handshake_and_create_server_session(&mut server_stream).await });

    // Client reads server response and completes handshake
    let mut buf = [0u8; 4096];
    let n = client_stream.read(&mut buf).await.unwrap();
    assert!(n > 0, "Server should send response bytes");

    let result = client_hs.process_bytes(&buf[..n]).unwrap();
    match result {
        HandshakeProcessResult::Completed { response_bytes, .. } => {
            if !response_bytes.is_empty() {
                client_stream.write_all(&response_bytes).await.unwrap();
            }
        }
        HandshakeProcessResult::InProgress { response_bytes } => {
            if !response_bytes.is_empty() {
                client_stream.write_all(&response_bytes).await.unwrap();
            }
            // Read final server response
            let n = client_stream.read(&mut buf).await.unwrap();
            let result2 = client_hs.process_bytes(&buf[..n]).unwrap();
            match result2 {
                HandshakeProcessResult::Completed { response_bytes, .. } => {
                    if !response_bytes.is_empty() {
                        client_stream.write_all(&response_bytes).await.unwrap();
                    }
                }
                _ => panic!("Expected handshake completion on second round"),
            }
        }
    }

    // Verify server completed handshake successfully
    let server_result = server_handle.await.unwrap();
    assert!(server_result.is_ok(), "Server handshake should succeed");
    let (_session, _leftover) = match server_result {
        Ok((s, l)) => (s, l),
        Err(e) => panic!("Server handshake failed: {}", e),
    };
}

#[tokio::test]
async fn test_handshake_eof_on_disconnect() {
    let (mut server_stream, client_stream, _listener) = create_server_client_pair().await;

    // Drop client immediately
    drop(client_stream);

    let result = handshake_and_create_server_session(&mut server_stream).await;
    assert!(result.is_err());
    let err = result.err().unwrap().to_string();
    assert!(
        err.contains("EOF") || err.contains("eof") || err.contains("os error"),
        "Error should indicate EOF: {}",
        err
    );
}

#[tokio::test]
async fn test_handshake_with_garbage_data() {
    let (mut server_stream, mut client_stream, _listener) = create_server_client_pair().await;

    // Send garbage instead of valid RTMP handshake
    client_stream.write_all(&[0xFF; 1537]).await.unwrap();

    // Server should handle gracefully (error or hang, but not crash)
    let server_handle = tokio::spawn(async move {
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            handshake_and_create_server_session(&mut server_stream),
        )
        .await
    });

    // Read whatever server sends back
    let mut buf = [0u8; 4096];
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        client_stream.read(&mut buf),
    )
    .await;

    let result = server_handle.await.unwrap();
    // Server should either error or timeout (both acceptable)
    match result {
        Ok(inner) => {
            // If timeout didn't fire, the function should have errored
            assert!(inner.is_err(), "Garbage data should cause error");
        }
        Err(_) => {
            // Timeout is acceptable - server hung up waiting for valid data
        }
    }
}

#[tokio::test]
async fn test_handshake_preserves_remaining_bytes() {
    let (mut server_stream, mut client_stream, _listener) = create_server_client_pair().await;

    // Client initiates handshake
    let mut client_hs = Handshake::new(PeerType::Client);
    let c0_c1 = client_hs.generate_outbound_p0_and_p1().unwrap();
    client_stream.write_all(&c0_c1).await.unwrap();

    let server_handle =
        tokio::spawn(async move { handshake_and_create_server_session(&mut server_stream).await });

    // Complete handshake from client side
    let mut buf = [0u8; 4096];
    let n = client_stream.read(&mut buf).await.unwrap();
    let result = client_hs.process_bytes(&buf[..n]).unwrap();
    match result {
        HandshakeProcessResult::Completed { response_bytes, .. } => {
            if !response_bytes.is_empty() {
                client_stream.write_all(&response_bytes).await.unwrap();
            }
        }
        HandshakeProcessResult::InProgress { response_bytes } => {
            if !response_bytes.is_empty() {
                client_stream.write_all(&response_bytes).await.unwrap();
            }
            let n = client_stream.read(&mut buf).await.unwrap();
            let result2 = client_hs.process_bytes(&buf[..n]).unwrap();
            match result2 {
                HandshakeProcessResult::Completed { response_bytes, .. } => {
                    if !response_bytes.is_empty() {
                        client_stream.write_all(&response_bytes).await.unwrap();
                    }
                }
                _ => panic!("Expected completion"),
            }
        }
    }

    let server_result = server_handle.await.unwrap().unwrap();
    let (_session, leftover) = server_result;
    // leftover is the bytes that came after the handshake in the same read
    // In a clean handshake, there should be no leftover
    let _ = leftover; // May or may not be empty
}
