mod common;

use common::mock_rtmp::{MockRtmpClient, MockRtmpServer};
use std::time::Duration;

#[tokio::test]
async fn test_mock_server_bind() {
    let server = MockRtmpServer::bind().await;
    assert!(server.addr.port() > 0);
}

#[tokio::test]
async fn test_mock_client_connect() {
    let server = MockRtmpServer::bind().await;
    let client = MockRtmpClient::connect(server.addr).await;
    assert!(client.is_ok());
}

#[tokio::test]
async fn test_mock_handshake_roundtrip() {
    let server = MockRtmpServer::bind().await;
    let addr = server.addr;

    let server_handle = tokio::spawn(async move {
        let mut session = server.accept().await;
        session.perform_handshake().await
    });

    let mut client = MockRtmpClient::connect(addr).await.unwrap();
    let result = client.perform_handshake().await;
    assert!(result.is_ok(), "Client handshake should succeed");

    let server_result = server_handle.await.unwrap();
    assert!(server_result.is_ok(), "Server handshake should succeed");
}

#[tokio::test]
async fn test_mock_server_accept_timeout() {
    let server = MockRtmpServer::bind().await;
    let result = server.accept_with_timeout(Duration::from_millis(50)).await;
    assert!(result.is_none(), "Should timeout when no client connects");
}

#[tokio::test]
async fn test_mock_multiple_clients() {
    let server = MockRtmpServer::bind().await;
    let addr = server.addr;

    let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(3);

    // Accept all connections in background
    let server_handle = tokio::spawn(async move {
        for _ in 0..3 {
            let mut session = server.accept().await;
            session.perform_handshake().await.unwrap();
            tx.send(()).await.unwrap();
        }
    });

    // Connect clients
    for _ in 0..3 {
        let mut client = MockRtmpClient::connect(addr).await.unwrap();
        client.perform_handshake().await.unwrap();
        rx.recv().await.unwrap();
    }

    server_handle.await.unwrap();
}

#[tokio::test]
async fn test_mock_client_disconnect() {
    let server = MockRtmpServer::bind().await;
    let addr = server.addr;

    let server_handle = tokio::spawn(async move {
        let mut session = server
            .accept_with_timeout(Duration::from_secs(2))
            .await
            .unwrap();
        session.perform_handshake().await
    });

    let mut client = MockRtmpClient::connect(addr).await.unwrap();
    client.perform_handshake().await.unwrap();
    client.disconnect().await;

    let _ = server_handle.await;
}
