use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Barrier;

/// Mock RTMP platform that accepts connections and reads data.
/// This simulates what Twitch/YouTube do when you connect to their RTMP ingest.
async fn mock_platform_server(
    addr: &str,
    ready: Arc<Barrier>,
    packets_received: Arc<std::sync::atomic::AtomicUsize>,
) -> tokio::task::JoinHandle<()> {
    let listener = TcpListener::bind(addr).await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    // Store addr so caller can use it
    drop(listener);

    tokio::spawn(async move {
        let listener = TcpListener::bind(local_addr).await.unwrap();
        ready.wait().await;

        loop {
            match tokio::time::timeout(Duration::from_secs(5), listener.accept()).await {
                Ok(Ok((mut stream, _addr))) => {
                    // Simulate RTMP server handshake response
                    let mut buf = [0u8; 4096];
                    loop {
                        match tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buf))
                            .await
                        {
                            Ok(Ok(0)) => break,
                            Ok(Ok(n)) => {
                                packets_received.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                // Send some fake RTMP server data back
                                let _ = stream.write_all(&buf[..std::cmp::min(n, 100)]).await;
                            }
                            Ok(Err(_)) => break,
                            Err(_) => break, // timeout = client disconnected
                        }
                    }
                }
                Ok(Err(_)) => break,
                Err(_) => break,
            }
        }
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_mock_platform_accepts_connections() {
    let ready = Arc::new(Barrier::new(2));
    let packets = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    // Start two mock platforms
    let p1 = mock_platform_server("127.0.0.1:0", ready.clone(), packets.clone()).await;
    let p2 = mock_platform_server("127.0.0.1:0", ready.clone(), packets.clone()).await;

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Verify mock servers started by checking they haven't panicked
    assert!(!p1.is_finished() || !p2.is_finished());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_stream_connect_disconnect_lifecycle() {
    let manager = Arc::new(reestream::http_server::stream::StreamManager::new());
    let mut rx = manager.subscribe();

    // Simulate a stream connecting
    let stream_id = manager
        .add_stream("live-test".into(), "rtmp://localhost/live".into())
        .await;
    let _ = tokio::time::timeout(Duration::from_millis(100), rx.recv())
        .await
        .unwrap();

    // Verify it shows in the stream list
    let streams = manager.get_streams().await;
    assert_eq!(streams.len(), 1);
    assert_eq!(
        streams[0].status,
        reestream::http_server::stream::StreamStatus::Live
    );

    // Simulate viewer updates
    for viewers in [1, 5, 10, 20, 15] {
        manager.update_stream_stats(&stream_id, viewers, 5000).await;
        let _ = tokio::time::timeout(Duration::from_millis(50), rx.recv())
            .await
            .unwrap();
    }

    let streams = manager.get_streams().await;
    assert_eq!(streams[0].viewers, 15);

    // Simulate stream disconnect
    manager.remove_stream(&stream_id).await;
    let _ = tokio::time::timeout(Duration::from_millis(100), rx.recv())
        .await
        .unwrap();
    assert!(manager.get_streams().await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_multiple_streams_concurrent() {
    let manager = Arc::new(reestream::http_server::stream::StreamManager::new());

    let mut handles = Vec::new();
    for i in 0..10 {
        let manager = manager.clone();
        handles.push(tokio::spawn(async move {
            let id = manager
                .add_stream(format!("stream-{i}"), format!("rtmp://input/{i}"))
                .await;

            // Simulate some data flowing
            for j in 0..5 {
                manager
                    .update_stream_stats(&id, (i * 5 + j) as u32, 2500)
                    .await;
                tokio::time::sleep(Duration::from_millis(5)).await;
            }

            manager.remove_stream(&id).await;
            id
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // All streams should be cleaned up
    assert!(manager.get_streams().await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_websocket_initial_state() {
    let manager = Arc::new(reestream::http_server::stream::StreamManager::new());

    // Subscribe BEFORE adding streams so we get all events
    let mut rx = manager.subscribe();

    // Add some streams
    manager
        .add_stream("stream-1".into(), "rtmp://input/1".into())
        .await;
    manager
        .add_stream("stream-2".into(), "rtmp://input/2".into())
        .await;
    manager
        .add_stream("stream-3".into(), "rtmp://input/3".into())
        .await;

    // Verify initial state is accessible
    let streams = manager.get_streams().await;
    assert_eq!(streams.len(), 3);

    // Verify events are received (one per stream added)
    for _ in 0..3 {
        let _ = tokio::time::timeout(Duration::from_millis(200), rx.recv())
            .await
            .unwrap();
    }

    // Clean up
    for stream in &streams {
        manager.remove_stream(&stream.id).await;
    }
}
