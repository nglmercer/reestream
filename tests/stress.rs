use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Barrier, RwLock};

#[tokio::test]
async fn test_stress_concurrent_connections_10() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let barrier = Arc::new(Barrier::new(10));

    let server_handle = tokio::spawn(async move {
        let mut handles = Vec::new();
        for _ in 0..10 {
            let (stream, _) = listener.accept().await.unwrap();
            handles.push(stream);
        }
        handles
    });

    let mut client_handles = Vec::new();
    for _ in 0..10 {
        let barrier = barrier.clone();
        client_handles.push(tokio::spawn(async move {
            let client = TcpStream::connect(addr).await.unwrap();
            barrier.wait().await;
            client
        }));
    }

    let server_streams = server_handle.await.unwrap();
    assert_eq!(server_streams.len(), 10);

    for handle in client_handles {
        let client = handle.await.unwrap();
        assert!(client.peer_addr().is_ok());
    }
}

#[tokio::test]
async fn test_stress_concurrent_connections_50() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        let mut count = 0;
        while count < 50 {
            if let Ok((_, _)) = listener.accept().await {
                count += 1;
            }
        }
        count
    });

    let mut client_handles = Vec::new();
    for _ in 0..50 {
        client_handles.push(tokio::spawn(async move { TcpStream::connect(addr).await }));
    }

    for handle in client_handles {
        assert!(handle.await.unwrap().is_ok());
    }

    let count = server_handle.await.unwrap();
    assert_eq!(count, 50);
}

#[tokio::test]
async fn test_stress_rapid_connect_disconnect() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        let mut count = 0;
        loop {
            tokio::select! {
                result = listener.accept() => {
                    if result.is_ok() {
                        count += 1;
                        if count >= 20 {
                            break;
                        }
                    }
                }
                _ = tokio::time::sleep(Duration::from_secs(5)) => break,
            }
        }
        count
    });

    for _ in 0..20 {
        let client = TcpStream::connect(addr).await.unwrap();
        drop(client);
    }

    let count = server_handle.await.unwrap();
    assert_eq!(count, 20);
}

#[tokio::test]
async fn test_stress_shared_platform_list_concurrent_access() {
    use reestream::config::{Orientation, Platform};
    use url::Url;

    let platform = Platform {
        url: Url::parse("rtmp://127.0.0.1:1935/app").unwrap(),
        key: "key".to_string(),
        orientation: Orientation::Horizontal,
    };
    let platforms = Arc::new(RwLock::new(vec![platform]));
    let barrier = Arc::new(Barrier::new(10));

    let mut handles = Vec::new();
    for i in 0..10 {
        let platforms = platforms.clone();
        let barrier = barrier.clone();
        handles.push(tokio::spawn(async move {
            barrier.wait().await;
            for _ in 0..100 {
                if i % 2 == 0 {
                    let guard = platforms.read().await;
                    let _ = guard.len();
                } else {
                    let mut guard = platforms.write().await;
                    guard.push(Platform {
                        url: Url::parse("rtmp://127.0.0.1/app").unwrap(),
                        key: format!("key-{}", i),
                        orientation: Orientation::Horizontal,
                    });
                }
            }
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let guard = platforms.read().await;
    assert!(guard.len() >= 10); // At least the initial + some writes
}

#[tokio::test]
async fn test_stress_channel_message_flood() {
    use bytes::Bytes;
    use tokio::sync::mpsc;

    let (tx, mut rx) = mpsc::channel::<Bytes>(100);
    let mut handles = Vec::new();

    // 5 producers, each sending 100 messages
    for producer_id in 0..5 {
        let tx = tx.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..100 {
                let data = Bytes::from(vec![producer_id, i as u8]);
                let _ = tx.try_send(data);
            }
        }));
    }

    drop(tx);

    // Consumer
    let consumer = tokio::spawn(async move {
        let mut count = 0;
        while rx.recv().await.is_some() {
            count += 1;
        }
        count
    });

    for handle in handles {
        handle.await.unwrap();
    }

    let received = consumer.await.unwrap();
    assert!(received > 0);
    assert!(received <= 500); // 5 * 100
}

#[tokio::test]
async fn test_stress_concurrent_handshake_attempts() {
    // Simulate 5 rapid sequential connections with handshake
    for _ in 0..5 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            reestream::server::handshake_and_create_server_session(&mut stream).await
        });

        let mut client = TcpStream::connect(addr).await.unwrap();
        use rml_rtmp::handshake::{Handshake, PeerType};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mut hs = Handshake::new(PeerType::Client);
        let c0_c1 = hs.generate_outbound_p0_and_p1().unwrap();
        client.write_all(&c0_c1).await.unwrap();

        let mut buf = [0u8; 4096];
        loop {
            let n = client.read(&mut buf).await.unwrap();
            if n == 0 {
                break;
            }
            match hs.process_bytes(&buf[..n]).unwrap() {
                rml_rtmp::handshake::HandshakeProcessResult::Completed {
                    response_bytes, ..
                } => {
                    if !response_bytes.is_empty() {
                        client.write_all(&response_bytes).await.unwrap();
                    }
                    break;
                }
                rml_rtmp::handshake::HandshakeProcessResult::InProgress { response_bytes } => {
                    if !response_bytes.is_empty() {
                        client.write_all(&response_bytes).await.unwrap();
                    }
                }
            }
        }

        let result = server_handle.await.unwrap();
        assert!(result.is_ok());
    }
}
