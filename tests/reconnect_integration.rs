use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

#[tokio::test]
async fn test_reconnection_channel_send_receive() {
    let (tx, mut rx) = mpsc::channel::<(usize, String)>(10);

    // Simulate reconnection event
    tx.send((0, "reconnected".to_string())).await.unwrap();
    let (index, msg) = rx.recv().await.unwrap();
    assert_eq!(index, 0);
    assert_eq!(msg, "reconnected");
}

#[tokio::test]
async fn test_reconnection_channel_multiple_platforms() {
    let (tx, mut rx) = mpsc::channel::<(usize, String)>(10);

    // Simulate reconnection for multiple platforms
    for i in 0..3 {
        tx.send((i, format!("platform-{}", i))).await.unwrap();
    }

    for i in 0..3 {
        let (index, msg) = rx.recv().await.unwrap();
        assert_eq!(index, i);
        assert_eq!(msg, format!("platform-{}", i));
    }
}

#[tokio::test]
async fn test_reconnection_channel_closed_sender() {
    let (tx, mut rx) = mpsc::channel::<(usize, String)>(10);

    tx.send((0, "before-close".to_string())).await.unwrap();
    drop(tx);

    let (index, msg) = rx.recv().await.unwrap();
    assert_eq!(index, 0);
    assert_eq!(msg, "before-close");

    // Channel should be closed now
    assert!(rx.recv().await.is_none());
}

#[tokio::test]
async fn test_tcp_reconnection_pattern() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // First connection
    let mut client1 = TcpStream::connect(addr).await.unwrap();
    let (mut server1, _) = listener.accept().await.unwrap();

    // Send data on first connection
    client1.write_all(b"hello").await.unwrap();
    let mut buf = [0u8; 64];
    let n = server1.read(&mut buf).await.unwrap();
    assert_eq!(&buf[..n], b"hello");

    // Drop first connection (simulate disconnect)
    drop(client1);
    drop(server1);

    // Second connection (simulate reconnect)
    let mut client2 = TcpStream::connect(addr).await.unwrap();
    let (mut server2, _) = listener.accept().await.unwrap();

    // Send data on second connection
    client2.write_all(b"reconnected").await.unwrap();
    let n = server2.read(&mut buf).await.unwrap();
    assert_eq!(&buf[..n], b"reconnected");

    drop(client2);
    drop(server2);
}

#[tokio::test]
async fn test_reconnection_with_data_buffering() {
    use bytes::Bytes;
    use std::collections::VecDeque;

    // Simulate buffering during reconnection
    let mut buffer: VecDeque<Bytes> = VecDeque::new();
    let max_buffer = 256;

    // Buffer data while disconnected
    for i in 0..300u16 {
        if buffer.len() >= max_buffer {
            buffer.pop_front();
        }
        buffer.push_back(Bytes::from(vec![i as u8]));
    }

    assert_eq!(buffer.len(), max_buffer);
    // First 44 items should have been evicted (300 - 256 = 44)
    assert_eq!(buffer.front().unwrap()[0], 44);
    assert_eq!(buffer.back().unwrap()[0], 43); // 299 % 256 = 43
}

#[tokio::test]
async fn test_reconnection_timeout() {
    let (tx, mut rx) = mpsc::channel::<()>(1);

    // Simulate timeout waiting for reconnection
    let result = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await;
    assert!(result.is_err(), "Should timeout when no reconnection happens");

    // Now send reconnection
    tx.send(()).await.unwrap();
    let result = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await;
    assert!(result.is_ok(), "Should receive reconnection event");
}

#[tokio::test]
async fn test_multiple_reconnection_attempts() {
    let (tx, mut rx) = mpsc::channel::<usize>(10);

    // Simulate multiple failed reconnection attempts followed by success
    let handle = tokio::spawn(async move {
        for attempt in 0..5 {
            tokio::time::sleep(Duration::from_millis(20)).await;
            // Simulate reconnection attempt
            if attempt == 4 {
                // Success on 5th attempt
                tx.send(attempt).await.unwrap();
                break;
            }
        }
    });

    let result = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().unwrap(), 4);

    handle.await.unwrap();
}
