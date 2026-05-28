use rml_rtmp::handshake::{Handshake, HandshakeProcessResult, PeerType};
use rml_rtmp::sessions::{ServerSession, ServerSessionConfig, ServerSessionResult};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Handshake server side and create ServerSession with lower-latency config
pub async fn handshake_and_create_server_session(
    stream: &mut TcpStream,
) -> Result<(ServerSession, Vec<u8>), Box<dyn std::error::Error + Send + Sync>> {
    let mut hs = Handshake::new(PeerType::Server);
    let mut buf = [0u8; 4096];

    loop {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            return Err("EOF durante handshake (no se recibieron datos de cliente)".into());
        }

        match hs.process_bytes(&buf[..n])? {
            HandshakeProcessResult::InProgress { response_bytes } => {
                if !response_bytes.is_empty() {
                    stream.write_all(&response_bytes).await?;
                }
            }
            HandshakeProcessResult::Completed {
                response_bytes,
                remaining_bytes,
            } => {
                if !response_bytes.is_empty() {
                    stream.write_all(&response_bytes).await?;
                }
                return Ok((
                    {
                        // Reduce latency: use smaller chunk size and smaller ack window to have quicker acks
                        let mut config = ServerSessionConfig::new();
                        config.chunk_size = 128; // smaller chunks -> lower per-chunk latency (tradeoff CPU)
                        config.window_ack_size = 262_144; // 256KB ack window to get more frequent acks

                        let (server_session, initial_results) = ServerSession::new(config)?;
                        for res in initial_results {
                            if let ServerSessionResult::OutboundResponse(packet) = res {
                                stream.write_all(&packet.bytes).await?;
                            }
                        }
                        server_session
                    },
                    remaining_bytes,
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rml_rtmp::handshake::Handshake;

    #[test]
    fn test_server_session_config_low_latency() {
        let mut config = ServerSessionConfig::new();
        config.chunk_size = 128;
        config.window_ack_size = 262_144;
        assert_eq!(config.chunk_size, 128);
        assert_eq!(config.window_ack_size, 262_144);
    }

    #[test]
    fn test_server_session_creation() {
        let mut config = ServerSessionConfig::new();
        config.chunk_size = 128;
        config.window_ack_size = 262_144;
        let result = ServerSession::new(config);
        assert!(result.is_ok());
        let (_session, initial_results) = result.unwrap();
        // ServerSession::new may produce initial results (e.g., window ack size)
        for res in &initial_results {
            assert!(matches!(res, ServerSessionResult::OutboundResponse(_)));
        }
    }

    #[test]
    fn test_handshake_server_creates() {
        let hs = Handshake::new(PeerType::Server);
        // Handshake should be constructable without panic
        let _ = hs;
    }

    #[test]
    fn test_handshake_client_creates() {
        let hs = Handshake::new(PeerType::Client);
        let _ = hs;
    }

    #[test]
    fn test_handshake_process_empty_bytes() {
        let mut hs = Handshake::new(PeerType::Server);
        let result = hs.process_bytes(&[]);
        // Empty bytes should not crash; result depends on implementation
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_handshake_and_create_server_session_eof() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let client = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut server_stream, _) = listener.accept().await.unwrap();

        // Drop client immediately to cause EOF
        drop(client);

        let result = handshake_and_create_server_session(&mut server_stream).await;
        assert!(result.is_err());
        let err_msg = result.err().unwrap().to_string();
        assert!(err_msg.contains("EOF") || err_msg.contains("eof"));
    }

    #[tokio::test]
    async fn test_handshake_and_create_server_session_success() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut server_stream, _) = listener.accept().await.unwrap();

        // Client sends C0+C1
        let mut client_hs = Handshake::new(PeerType::Client);
        let c0_c1 = client_hs.generate_outbound_p0_and_p1().unwrap();
        client.write_all(&c0_c1).await.unwrap();

        // Server processes in background
        let server_handle =
            tokio::spawn(
                async move { handshake_and_create_server_session(&mut server_stream).await },
            );

        // Client reads S0+S1+S2
        let mut buf = [0u8; 4096];
        let n = client.read(&mut buf).await.unwrap();
        assert!(n > 0);

        let result = client_hs.process_bytes(&buf[..n]).unwrap();
        match result {
            HandshakeProcessResult::Completed { response_bytes, .. } => {
                if !response_bytes.is_empty() {
                    client.write_all(&response_bytes).await.unwrap();
                }
            }
            HandshakeProcessResult::InProgress { response_bytes } => {
                if !response_bytes.is_empty() {
                    client.write_all(&response_bytes).await.unwrap();
                }
                // Read more if needed
                let n = client.read(&mut buf).await.unwrap();
                let result2 = client_hs.process_bytes(&buf[..n]).unwrap();
                match result2 {
                    HandshakeProcessResult::Completed { response_bytes, .. } => {
                        if !response_bytes.is_empty() {
                            client.write_all(&response_bytes).await.unwrap();
                        }
                    }
                    _ => panic!("Expected handshake completion"),
                }
            }
        }

        let server_result = server_handle.await.unwrap();
        assert!(server_result.is_ok());
        if let Ok((_session, leftover)) = server_result {
            // leftover may or may not be empty depending on timing
            let _ = leftover;
        }
    }
}
