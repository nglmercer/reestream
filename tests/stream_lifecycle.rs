use std::sync::Arc;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_stream_lifecycle_register_unregister() {
    let manager = Arc::new(reestream::http_server::stream::StreamManager::new());
    let mut rx = manager.subscribe();

    // Register
    let id = manager
        .add_stream("test-stream".into(), "rtmp://input/live".into())
        .await;
    assert!(!id.is_empty());

    // Verify stream exists
    let streams = manager.get_streams().await;
    assert_eq!(streams.len(), 1);
    assert_eq!(streams[0].name, "test-stream");
    assert_eq!(
        streams[0].status,
        reestream::http_server::stream::StreamStatus::Live
    );

    // Verify WebSocket event was sent
    let event = tokio::time::timeout(Duration::from_millis(100), rx.recv())
        .await
        .unwrap()
        .unwrap();
    match event {
        reestream::http_server::stream::StreamEvent::Started {
            id: evt_id,
            name,
            input_url,
        } => {
            assert_eq!(evt_id, id);
            assert_eq!(name, "test-stream");
            assert_eq!(input_url, "rtmp://input/live");
        }
        _ => panic!("Expected StreamEvent::Started"),
    }

    // Unregister
    assert!(manager.remove_stream(&id).await);
    assert!(manager.get_streams().await.is_empty());

    // Verify stop event
    let event = tokio::time::timeout(Duration::from_millis(100), rx.recv())
        .await
        .unwrap()
        .unwrap();
    match event {
        reestream::http_server::stream::StreamEvent::Stopped { id: evt_id } => {
            assert_eq!(evt_id, id);
        }
        _ => panic!("Expected StreamEvent::Stopped"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_stream_stats_update() {
    let manager = Arc::new(reestream::http_server::stream::StreamManager::new());
    let mut rx = manager.subscribe();

    let id = manager
        .add_stream("live".into(), "rtmp://input".into())
        .await;

    // Consume the Started event
    let _ = tokio::time::timeout(Duration::from_millis(100), rx.recv())
        .await
        .unwrap();

    // Update stats
    manager.update_stream_stats(&id, 150, 5000).await;

    let streams = manager.get_streams().await;
    assert_eq!(streams[0].viewers, 150);
    assert_eq!(streams[0].bitrate, 5000);

    // Verify update event
    let event = tokio::time::timeout(Duration::from_millis(100), rx.recv())
        .await
        .unwrap()
        .unwrap();
    match event {
        reestream::http_server::stream::StreamEvent::Updated {
            id: evt_id,
            viewers,
            bitrate,
        } => {
            assert_eq!(evt_id, id);
            assert_eq!(viewers, 150);
            assert_eq!(bitrate, 5000);
        }
        _ => panic!("Expected StreamEvent::Updated"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_stream_registrar_trait() {
    use reestream::client::StreamRegistrar;

    let manager = Arc::new(reestream::http_server::stream::StreamManager::new());
    let registrar: Arc<dyn StreamRegistrar> = manager.clone();

    let id = registrar
        .register_stream("via-trait".into(), "rtmp://test".into())
        .await;
    assert!(!id.is_empty());

    let streams = manager.get_streams().await;
    assert_eq!(streams.len(), 1);
    assert_eq!(streams[0].name, "via-trait");

    registrar.unregister_stream(&id).await;
    assert!(manager.get_streams().await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_websocket_events_stream_lifecycle() {
    let manager = Arc::new(reestream::http_server::stream::StreamManager::new());
    let mut rx = manager.subscribe();

    // Simulate a stream connecting and disconnecting
    let id1 = manager
        .add_stream("stream-1".into(), "rtmp://live/stream1".into())
        .await;
    let _ = tokio::time::timeout(Duration::from_millis(50), rx.recv())
        .await
        .unwrap();

    let id2 = manager
        .add_stream("stream-2".into(), "rtmp://live/stream2".into())
        .await;
    let _ = tokio::time::timeout(Duration::from_millis(50), rx.recv())
        .await
        .unwrap();

    // Both streams live
    assert_eq!(manager.get_streams().await.len(), 2);

    // Disconnect stream-1
    manager.remove_stream(&id1).await;
    let _ = tokio::time::timeout(Duration::from_millis(50), rx.recv())
        .await
        .unwrap();
    let streams = manager.get_streams().await;
    assert_eq!(streams.len(), 1);
    assert_eq!(streams[0].id, id2);

    // Disconnect stream-2
    manager.remove_stream(&id2).await;
    let _ = tokio::time::timeout(Duration::from_millis(50), rx.recv())
        .await
        .unwrap();
    assert!(manager.get_streams().await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_multiple_subscribers_receive_events() {
    let manager = Arc::new(reestream::http_server::stream::StreamManager::new());

    let mut rx1 = manager.subscribe();
    let mut rx2 = manager.subscribe();

    let id = manager
        .add_stream("shared".into(), "rtmp://input".into())
        .await;

    // Both subscribers should receive the event
    let event1 = tokio::time::timeout(Duration::from_millis(100), rx1.recv())
        .await
        .unwrap()
        .unwrap();
    let event2 = tokio::time::timeout(Duration::from_millis(100), rx2.recv())
        .await
        .unwrap()
        .unwrap();

    match (&event1, &event2) {
        (
            reestream::http_server::stream::StreamEvent::Started { id: id1, .. },
            reestream::http_server::stream::StreamEvent::Started { id: id2, .. },
        ) => {
            assert_eq!(id1, &id);
            assert_eq!(id2, &id);
        }
        _ => panic!("Both subscribers should receive StreamEvent::Started"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_platform_config_sync() {
    let dir = std::env::temp_dir().join("reestream_test_platform_sync");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("config.toml");

    // Start with config containing 1 platform
    std::fs::write(
        &path,
        r#"rtmp_addr = "0.0.0.0"
rtmp_port = 1935
stream_key = "test-key"

[[platform]]
url = "rtmp://twitch.tv/app"
key = "tw-key"
orientation = "horizontal"
"#,
    )
    .unwrap();

    // Add a platform via config
    reestream::setup::add_platform_to_config(
        &path,
        "rtmp://youtube.com/live2",
        "yt-key",
        "vertical",
    )
    .unwrap();

    let config = reestream::config::Config::from_file(&path).unwrap();
    assert_eq!(config.platform.as_ref().unwrap().len(), 2);
    assert_eq!(config.platform.as_ref().unwrap()[1].key, "yt-key");

    // Edit a platform via config
    reestream::setup::update_platform_in_config(
        &path,
        0,
        Some("rtmp://kick.tv/app"),
        Some("kick-key"),
        Some("horizontal"),
    )
    .unwrap();

    let config = reestream::config::Config::from_file(&path).unwrap();
    assert_eq!(config.platform.as_ref().unwrap()[0].key, "kick-key");
    assert!(
        config.platform.as_ref().unwrap()[0]
            .url
            .host_str()
            .unwrap()
            .contains("kick")
    );

    // Remove a platform via config
    reestream::setup::remove_platform_from_config(&path, 1).unwrap();

    let config = reestream::config::Config::from_file(&path).unwrap();
    assert_eq!(config.platform.as_ref().unwrap().len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_config_full_roundtrip() {
    let dir = std::env::temp_dir().join("reestream_test_config_roundtrip");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("config.toml");

    // Read initial config
    std::fs::write(
        &path,
        r#"rtmp_addr = "0.0.0.0"
rtmp_port = 1935
stream_key = "initial-key"
"#,
    )
    .unwrap();

    let config = reestream::setup::read_config(&path).unwrap();
    assert_eq!(config.stream_key, "initial-key");

    // Update fields
    let config =
        reestream::setup::update_config_fields(&path, None, Some(8080), Some("updated-key"))
            .unwrap();
    assert_eq!(config.rtmp_port, 8080);
    assert_eq!(config.stream_key, "updated-key");

    // Verify persisted
    let config = reestream::setup::read_config(&path).unwrap();
    assert_eq!(config.rtmp_port, 8080);
    assert_eq!(config.stream_key, "updated-key");

    // Reset stream key
    let new_key = reestream::setup::reset_stream_key(&path).unwrap();
    assert!(!new_key.is_empty());
    assert_ne!(new_key, "updated-key");

    let config = reestream::setup::read_config(&path).unwrap();
    assert_eq!(config.stream_key, new_key);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_stream_registration() {
    let manager = Arc::new(reestream::http_server::stream::StreamManager::new());

    let mut handles = Vec::new();
    for i in 0..20 {
        let manager = manager.clone();
        let handle = tokio::spawn(async move {
            manager
                .add_stream(format!("stream-{i}"), format!("rtmp://input/{i}"))
                .await
        });
        handles.push(handle);
    }

    let mut ids = Vec::new();
    for handle in handles {
        ids.push(handle.await.unwrap());
    }

    let streams = manager.get_streams().await;
    assert_eq!(streams.len(), 20);

    // Remove half
    for id in &ids[..10] {
        manager.remove_stream(id).await;
    }

    assert_eq!(manager.get_streams().await.len(), 10);
}
