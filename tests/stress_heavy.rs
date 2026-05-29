use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stress_50_concurrent_listeners() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let counter = Arc::new(RwLock::new(0u32));

    let server_handle = {
        let counter = counter.clone();
        tokio::spawn(async move {
            for _ in 0..50 {
                let (stream, _) = listener.accept().await.unwrap();
                let counter = counter.clone();
                tokio::spawn(async move {
                    let _ = stream;
                    *counter.write().await += 1;
                });
            }
        })
    };

    let start = Instant::now();
    let mut handles = Vec::new();

    for _ in 0..50 {
        let handle = tokio::spawn(async move {
            let _stream = TcpStream::connect(addr).await.unwrap();
            tokio::time::sleep(Duration::from_millis(10)).await;
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    server_handle.abort();
    let elapsed = start.elapsed();

    let count = *counter.read().await;
    assert!(count > 0, "Should have handled some connections");
    assert!(
        elapsed < Duration::from_secs(10),
        "Should complete within 10 seconds"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stress_rapid_connect_disconnect() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        for _ in 0..50 {
            let (stream, _) = listener.accept().await.unwrap();
            drop(stream);
        }
    });

    let start = Instant::now();

    for _ in 0..50 {
        let stream = TcpStream::connect(addr).await.unwrap();
        drop(stream);
    }

    let elapsed = start.elapsed();
    server_handle.abort();

    assert!(
        elapsed < Duration::from_secs(5),
        "Rapid connect/disconnect should be fast"
    );
}

#[cfg(feature = "core")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stress_concurrent_config_reads() {
    use reestream::config::ConfigBuilder;

    let config = Arc::new(ConfigBuilder::new().stream_key("test").build());

    let mut handles = Vec::new();

    for _ in 0..5 {
        let config = config.clone();
        let handle = tokio::spawn(async move {
            for _ in 0..100 {
                let _ = config.validate();
                let _ = config.to_toml();
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }
}

#[cfg(any(feature = "hls", feature = "api"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stress_platform_list_contention() {
    use reestream::http_server::stream::StreamManager;

    let manager = Arc::new(StreamManager::new());

    for i in 0..5 {
        manager
            .add_platform(
                format!("Platform {i}"),
                format!("rtmp://server{i}"),
                format!("key{i}"),
            )
            .await;
    }

    let mut handles = Vec::new();

    for _ in 0..5 {
        let manager = manager.clone();
        let handle = tokio::spawn(async move {
            for _ in 0..100 {
                let _ = manager.get_platforms().await;
            }
        });
        handles.push(handle);
    }

    for _ in 0..5 {
        let manager = manager.clone();
        let handle = tokio::spawn(async move {
            for _ in 0..100 {
                let _ = manager.toggle_platform("fake", true).await;
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let platforms = manager.get_platforms().await;
    assert_eq!(platforms.len(), 5);
}
