use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;

#[tokio::test]
async fn test_tcp_listener_bind_and_accept() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Connect a client
    let client = TcpStream::connect(addr).await.unwrap();
    let (server_stream, peer) = listener.accept().await.unwrap();

    assert_eq!(peer.ip().to_string(), "127.0.0.1");
    assert!(client.peer_addr().is_ok());
    assert!(server_stream.peer_addr().is_ok());

    drop(client);
    drop(server_stream);
}

#[tokio::test]
async fn test_multiple_concurrent_connections() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let mut handles = Vec::new();
    for _ in 0..5 {
        handles.push(tokio::spawn(async move {
            TcpStream::connect(addr).await.unwrap()
        }));
    }

    // Accept all connections
    for _ in 0..5 {
        let (stream, _) = listener.accept().await.unwrap();
        drop(stream);
    }

    // All clients should have connected successfully
    for handle in handles {
        let client = handle.await.unwrap();
        assert!(client.peer_addr().is_ok());
    }
}

#[tokio::test]
async fn test_graceful_shutdown_with_ctrl_c_simulation() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let platforms = Arc::new(RwLock::new(vec![]));

    let server_handle = tokio::spawn(async move {
        let platforms = platforms;
        // Simulate the main loop with a timeout instead of ctrl_c
        let shutdown = tokio::time::sleep(Duration::from_millis(100));
        tokio::pin!(shutdown);

        loop {
            tokio::select! {
                _ = &mut shutdown => {
                    break;
                }
                accept = listener.accept() => {
                    if let Ok((socket, _)) = accept {
                        let _ = socket.set_nodelay(true);
                        let platforms = platforms.clone();
                        let stream_key = "test-key".to_string();
                        let (_, pev) = tokio::sync::broadcast::channel(1);
                        tokio::spawn(async move {
                            let _ = reestream::client::handle_publisher(
                                socket,
                                platforms,
                                stream_key,
                                None,
                                None,
                                pev,
                            )
                            .await;
                        });
                    }
                }
            }
        }
    });

    // Connect a client before shutdown
    let _client = TcpStream::connect(addr).await.unwrap();

    // Wait for server to shut down
    let result = tokio::time::timeout(Duration::from_secs(5), server_handle).await;
    assert!(result.is_ok(), "Server should shut down within timeout");
    assert!(result.unwrap().is_ok());
}

#[tokio::test]
async fn test_platform_list_shared_across_connections() {
    use reestream::config::Platform;
    use url::Url;

    let platform = Platform {
        enabled: true,
        url: Url::parse("rtmp://127.0.0.1:1999/app").unwrap(),
        key: "test-key".to_string(),
        orientation: reestream::config::Orientation::Horizontal,
    };

    let platforms = Arc::new(RwLock::new(vec![platform]));
    let platforms_clone = platforms.clone();

    // Verify shared state
    {
        let guard = platforms.read().await;
        assert_eq!(guard.len(), 1);
        assert_eq!(guard[0].key, "test-key");
    }

    // Modify through clone
    {
        let mut guard = platforms_clone.write().await;
        guard.push(Platform {
            enabled: true,
            url: Url::parse("rtmp://127.0.0.1:2000/app").unwrap(),
            key: "key2".to_string(),
            orientation: reestream::config::Orientation::Vertical,
        });
    }

    // Verify both see the change
    {
        let guard = platforms.read().await;
        assert_eq!(guard.len(), 2);
    }
}

#[tokio::test]
async fn test_socket_nodelay() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let client = TcpStream::connect(addr).await.unwrap();
    let (server_stream, _) = listener.accept().await.unwrap();

    // set_nodelay should succeed on valid sockets
    assert!(client.set_nodelay(true).is_ok());
    assert!(server_stream.set_nodelay(true).is_ok());

    drop(client);
    drop(server_stream);
}
