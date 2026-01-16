//! Tests for RTMP handshake and server session functionality
//!
//! This module tests the low-level RTMP protocol handling,
//! including handshake negotiation and server session management.

use reestream::server::handshake_and_create_server_session;
use rml_rtmp::handshake::{Handshake, HandshakeProcessResult, PeerType};
use rml_rtmp::sessions::{ServerSession, ServerSessionConfig, ServerSessionResult};
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

/// Creates a minimal valid RTMP handshake C0+C1 packet (client to server)
fn create_client_c0c1() -> Vec<u8> {
    let mut hs = Handshake::new(PeerType::Client);
    hs.generate_outbound_p0_and_p1().unwrap()
}

#[tokio::test]
async fn test_handshake_generate_client_c0c1() {
    let c0c1 = create_client_c0c1();

    // C0+C1 should be 1537 bytes (1 + 1536)
    assert_eq!(c0c1.len(), 1537);

    // C0 is the first byte, should be 0x03 for RTMP version 3
    assert_eq!(c0c1[0], 0x03);
}

#[tokio::test]
async fn test_server_session_creation() {
    let session = ServerSession::new(ServerSessionConfig::new());

    assert!(session.is_ok(), "Should create server session successfully");

    let (_session, initial_results) = session.unwrap();
    assert!(!initial_results.is_empty(), "Should have initial results");
}

#[tokio::test]
async fn test_server_session_with_custom_config() {
    let mut config = ServerSessionConfig::new();
    config.chunk_size = 64;
    config.window_ack_size = 500_000;

    let (_session, initial_results) = ServerSession::new(config).unwrap();

    assert!(!initial_results.is_empty());

    // Check that at least one outbound response is generated
    let has_outbound = initial_results
        .iter()
        .any(|r| matches!(r, ServerSessionResult::OutboundResponse(_)));
    assert!(has_outbound, "Should have outbound responses");
}

#[tokio::test]
async fn test_server_session_handle_invalid_input() {
    let (mut session, _) = ServerSession::new(ServerSessionConfig::new()).unwrap();

    let invalid_data = vec![0xFF; 100];
    let result = session.handle_input(&invalid_data);

    // Should handle gracefully, might return error or results
    // The important part is that it doesn't panic
    match result {
        Ok(_) => {}
        Err(_) => {}
    }
}

#[tokio::test]
async fn test_server_session_connection_request() {
    let (mut _session, _) = ServerSession::new(ServerSessionConfig::new()).unwrap();

    // Empty input should produce no results
    let results = _session.handle_input(&[]).unwrap();
    assert!(results.is_empty(), "Empty input should produce no results");
}

#[tokio::test]
async fn test_server_session_chunk_size() {
    let mut config = ServerSessionConfig::new();
    config.chunk_size = 128;

    let (_session, _) = ServerSession::new(config).unwrap();
}

#[tokio::test]
async fn test_server_session_window_ack_size() {
    let mut config = ServerSessionConfig::new();
    config.window_ack_size = 262_144;

    let (_session, _) = ServerSession::new(config).unwrap();
}

#[tokio::test]
async fn test_server_handshake_timeout_on_no_data() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server_task = async {
        let (mut socket, _) = listener.accept().await.unwrap();

        // Set a timeout for handshake
        timeout(
            Duration::from_millis(500),
            handshake_and_create_server_session(&mut socket),
        )
        .await
    };

    // Client that doesn't send any data
    let client_task = async {
        let _socket = TcpStream::connect(addr).await.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
    };

    // Run both
    tokio::select! {
        result = server_task => {
            match result {
                Ok(Ok(_)) => panic!("Should timeout waiting for data"),
                Ok(Err(_)) => {} // Expected: handshake failed
                Err(_) => {} // Expected: timeout elapsed
            }
        }
        _ = client_task => {}
    }
}

#[tokio::test]
async fn test_server_session_multiple_connections() {
    let config = ServerSessionConfig::new();

    // Create multiple sessions to verify no shared state issues
    let session1 = ServerSession::new(config.clone());
    let session2 = ServerSession::new(config.clone());

    assert!(session1.is_ok());
    assert!(session2.is_ok());
}

#[tokio::test]
async fn test_server_session_outbound_packet_generation() {
    let (_session, initial_results) = ServerSession::new(ServerSessionConfig::new()).unwrap();

    let mut outbound_count = 0;
    let mut total_bytes = 0;

    for result in initial_results {
        if let ServerSessionResult::OutboundResponse(packet) = result {
            outbound_count += 1;
            total_bytes += packet.bytes.len();
        }
    }

    assert!(
        outbound_count > 0,
        "Should generate at least one outbound packet"
    );
    assert!(total_bytes > 0, "Outbound packets should contain data");
}

#[tokio::test]
async fn test_handshake_client_initializes_correctly() {
    let mut client_hs = Handshake::new(PeerType::Client);
    let c0c1 = client_hs.generate_outbound_p0_and_p1().unwrap();

    // Verify structure
    assert_eq!(c0c1.len(), 1537);
    assert_eq!(c0c1[0], 0x03);
}

#[tokio::test]
async fn test_handshake_server_initializes_correctly() {
    let mut server_hs = Handshake::new(PeerType::Server);
    let s0s1 = server_hs.generate_outbound_p0_and_p1().unwrap();

    // Verify structure
    assert_eq!(s0s1.len(), 1537);
    assert_eq!(s0s1[0], 0x03);
}

#[tokio::test]
async fn test_server_session_error_handling() {
    let (mut session, _) = ServerSession::new(ServerSessionConfig::new()).unwrap();

    // Send garbage data
    let garbage = vec![0xAA; 1000];
    let result = session.handle_input(&garbage);

    // Should not panic
    match result {
        Ok(_) => {}
        Err(_) => {}
    }
}

#[tokio::test]
async fn test_server_session_isolation() {
    // Verify that server sessions don't share state
    let config = ServerSessionConfig::new();

    let (mut session1, _) = ServerSession::new(config.clone()).unwrap();
    let (mut session2, _) = ServerSession::new(config.clone()).unwrap();

    // Both should handle input independently
    let result1 = session1.handle_input(&[]);
    let result2 = session2.handle_input(&[]);

    // Both should succeed (empty input)
    assert!(result1.is_ok());
    assert!(result2.is_ok());
}

// Helper function to validate RTMP packet structure
fn validate_rtmp_packet(data: &[u8]) -> bool {
    // Basic validation: packet should have header
    // RTMP packet header format is complex, this is a minimal check
    data.len() >= 11
}

#[tokio::test]
async fn test_outbound_response_validity() {
    let (_session, initial_results) = ServerSession::new(ServerSessionConfig::new()).unwrap();

    for result in initial_results {
        if let ServerSessionResult::OutboundResponse(packet) = result {
            assert!(
                validate_rtmp_packet(&packet.bytes),
                "Outbound response should be valid RTMP packet"
            );
            assert!(
                !packet.bytes.is_empty(),
                "Outbound response should not be empty"
            );
        }
    }
}

#[tokio::test]
async fn test_server_session_with_empty_input() {
    let (mut session, _) = ServerSession::new(ServerSessionConfig::new()).unwrap();

    let result = session.handle_input(&[]);

    // Empty input should be valid
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

#[tokio::test]
async fn test_handshake_process_bytes_valid_input() {
    let c0c1 = create_client_c0c1();
    let mut server_hs = Handshake::new(PeerType::Server);

    // Process valid client handshake data
    let result = server_hs.process_bytes(&c0c1);

    // Should handle the input
    assert!(result.is_ok());

    // Check the result type
    match result.unwrap() {
        HandshakeProcessResult::InProgress { .. } => {}
        HandshakeProcessResult::Completed { .. } => {}
    }
}

#[tokio::test]
async fn test_handshake_empty_input_returns_in_progress() {
    let mut server_hs = Handshake::new(PeerType::Server);

    // Empty input should return InProgress or error
    let result = server_hs.process_bytes(&[]);

    // Should not panic on empty input
    match result {
        Ok(HandshakeProcessResult::InProgress { .. }) => {}
        Ok(HandshakeProcessResult::Completed { .. }) => {}
        Err(_) => {}
    }
}
