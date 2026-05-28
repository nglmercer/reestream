use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

#[tokio::test]
async fn test_read_timeout_on_idle_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        // Server accepts but never sends anything
        tokio::time::sleep(Duration::from_secs(5)).await;
        let _ = stream.write_all(b"late response").await;
    });

    let mut client = TcpStream::connect(addr).await.unwrap();
    let mut buf = [0u8; 64];

    // Should timeout waiting for data
    let result = timeout(Duration::from_millis(200), client.read(&mut buf)).await;
    assert!(result.is_err(), "Should timeout on idle connection");

    server_handle.abort();
}

#[tokio::test]
async fn test_connect_timeout_to_unreachable() {
    // Try connecting to a non-routable address
    let result = timeout(
        Duration::from_millis(500),
        TcpStream::connect("192.0.2.1:12345"), // RFC 5737 TEST-NET
    )
    .await;

    // Should either timeout or connection refused
    match result {
        Ok(Ok(_)) => panic!("Should not connect to TEST-NET"),
        Ok(Err(_)) => {} // Connection refused or similar
        Err(_) => {}     // Timeout
    }
}

#[tokio::test]
async fn test_write_timeout_on_full_buffer() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        // Accept but never read - this will cause the send buffer to fill
        tokio::time::sleep(Duration::from_secs(5)).await;
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf).await;
    });

    let mut client = TcpStream::connect(addr).await.unwrap();
    client.set_nodelay(true).unwrap();

    // Write until buffer fills
    let data = vec![0u8; 65536];
    let mut total_written = 0;
    loop {
        match timeout(Duration::from_millis(100), client.write_all(&data)).await {
            Ok(Ok(())) => total_written += data.len(),
            Ok(Err(_)) => break,
            Err(_) => break, // Timeout - buffer full
        }
        if total_written > 10 * 1024 * 1024 {
            break; // Safety limit
        }
    }

    assert!(total_written > 0);
    server_handle.abort();
}

#[tokio::test]
async fn test_partial_read_handling() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        // Send data in small chunks with delays
        for i in 0..5 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let _ = stream.write_all(&[i]).await;
        }
    });

    let mut client = TcpStream::connect(addr).await.unwrap();
    let mut received = Vec::new();

    // Read with timeout - should get partial data
    loop {
        let mut buf = [0u8; 64];
        match timeout(Duration::from_millis(300), client.read(&mut buf)).await {
            Ok(Ok(0)) => break,
            Ok(Ok(n)) => received.extend_from_slice(&buf[..n]),
            Ok(Err(_)) => break,
            Err(_) => break, // Timeout
        }
    }

    assert!(!received.is_empty());
    server_handle.await.unwrap();
}

#[tokio::test]
async fn test_graceful_close_detection() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        drop(stream); // Close immediately
    });

    let mut client = TcpStream::connect(addr).await.unwrap();
    let mut buf = [0u8; 64];

    // Should detect EOF quickly
    let result = timeout(Duration::from_secs(1), client.read(&mut buf)).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().unwrap(), 0); // EOF

    server_handle.await.unwrap();
}

#[tokio::test]
async fn test_concurrent_timeout_handling() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        for _ in 0..3 {
            let (stream, _) = listener.accept().await.unwrap();
            // Drop immediately
            drop(stream);
        }
    });

    let mut handles = Vec::new();
    for _ in 0..3 {
        handles.push(tokio::spawn(async move {
            let mut client = TcpStream::connect(addr).await.unwrap();
            let mut buf = [0u8; 64];
            let result = timeout(Duration::from_millis(500), client.read(&mut buf)).await;
            match result {
                Ok(Ok(0)) => true,  // EOF detected
                Ok(Ok(_)) => false, // Unexpected data
                Ok(Err(_)) => true, // Error (connection reset)
                Err(_) => true,     // Timeout is acceptable
            }
        }));
    }

    for handle in handles {
        assert!(handle.await.unwrap());
    }

    server_handle.await.unwrap();
}
