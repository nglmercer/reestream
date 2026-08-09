use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::{RwLock, watch};
use tracing::info;

pub struct GracefulShutdown {
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
}

impl GracefulShutdown {
    pub fn new() -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            shutdown_tx,
            shutdown_rx,
        }
    }

    pub fn shutdown_signal(&self) -> watch::Receiver<bool> {
        self.shutdown_rx.clone()
    }

    pub fn is_shutdown(&self) -> bool {
        *self.shutdown_rx.borrow()
    }

    pub async fn trigger_shutdown(&self) {
        info!("Triggering graceful shutdown...");
        let _ = self.shutdown_tx.send(true);
    }

    pub async fn wait_for_shutdown(&self) {
        let mut rx = self.shutdown_rx.clone();
        while !*rx.borrow_and_update() {
            if rx.changed().await.is_err() {
                break;
            }
        }
    }

    pub async fn drain_timeout(&self, timeout: Duration) -> bool {
        info!("Draining in-flight operations (timeout: {:?})...", timeout);
        tokio::time::timeout(timeout, self.wait_for_shutdown())
            .await
            .is_ok()
    }
}

impl Default for GracefulShutdown {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn setup_signal_handlers(shutdown: Arc<GracefulShutdown>) {
    let shutdown_clone = shutdown.clone();

    tokio::spawn(async move {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
        let mut sigint =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt()).unwrap();
        let mut sighup =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup()).unwrap();

        tokio::select! {
            _ = sigterm.recv() => {
                info!("Received SIGTERM, initiating graceful shutdown");
                shutdown_clone.trigger_shutdown().await;
            }
            _ = sigint.recv() => {
                info!("Received SIGINT, initiating graceful shutdown");
                shutdown_clone.trigger_shutdown().await;
            }
            _ = sighup.recv() => {
                info!("Received SIGHUP, config reload requested");
            }
        }
    });
}

pub struct ConfigWatcher {
    path: PathBuf,
    last_modified: Option<std::time::SystemTime>,
}

impl ConfigWatcher {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            last_modified: None,
        }
    }

    pub fn check_changed(&mut self) -> bool {
        if let Ok(metadata) = std::fs::metadata(&self.path)
            && let Ok(modified) = metadata.modified()
        {
            let changed = self.last_modified.is_none_or(|last| modified > last);
            self.last_modified = Some(modified);
            return changed;
        }
        false
    }

    pub async fn watch_loop<F>(path: PathBuf, interval: Duration, on_change: F)
    where
        F: Fn() + Send + 'static,
    {
        let mut watcher = ConfigWatcher::new(path);
        loop {
            tokio::time::sleep(interval).await;
            if watcher.check_changed() {
                info!("Config file changed, triggering reload");
                on_change();
            }
        }
    }
}

pub struct RateLimiter {
    max_connections_per_second: u32,
    current_count: Arc<RwLock<u32>>,
    window_start: Arc<RwLock<std::time::Instant>>,
}

impl RateLimiter {
    pub fn new(max_connections_per_second: u32) -> Self {
        Self {
            max_connections_per_second,
            current_count: Arc::new(RwLock::new(0)),
            window_start: Arc::new(RwLock::new(std::time::Instant::now())),
        }
    }

    pub async fn try_acquire(&self) -> bool {
        let mut count = self.current_count.write().await;
        let mut window = self.window_start.write().await;

        let now = std::time::Instant::now();
        if now.duration_since(*window) >= Duration::from_secs(1) {
            *count = 0;
            *window = now;
        }

        if *count < self.max_connections_per_second {
            *count += 1;
            true
        } else {
            false
        }
    }
}

pub struct ConnectionPool {
    max_connections: usize,
    current: Arc<AtomicUsize>,
}

impl ConnectionPool {
    pub fn new(max_connections: usize) -> Self {
        Self {
            max_connections,
            current: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub async fn try_acquire(&self) -> Option<ConnectionGuard> {
        loop {
            let current = self.current.load(Ordering::Acquire);
            if current >= self.max_connections {
                return None;
            }
            if self
                .current
                .compare_exchange(current, current + 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Some(ConnectionGuard {
                    current: self.current.clone(),
                });
            }
        }
    }

    pub async fn active_connections(&self) -> usize {
        self.current.load(Ordering::Acquire)
    }

    pub fn max_connections(&self) -> usize {
        self.max_connections
    }

    pub async fn drain_timeout(&self, timeout: Duration) -> bool {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if self.active_connections().await == 0 {
                return true;
            }
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return false;
            }
            tokio::time::sleep((deadline - now).min(Duration::from_millis(25))).await;
        }
    }
}

pub struct ConnectionGuard {
    current: Arc<AtomicUsize>,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        let _ = self
            .current
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current.checked_sub(1)
            });
    }
}

pub struct BandwidthLimiter {
    max_bytes_per_second: u64,
    bytes_used: Arc<RwLock<u64>>,
    window_start: Arc<RwLock<std::time::Instant>>,
}

impl BandwidthLimiter {
    pub fn new(max_bytes_per_second: u64) -> Self {
        Self {
            max_bytes_per_second,
            bytes_used: Arc::new(RwLock::new(0)),
            window_start: Arc::new(RwLock::new(std::time::Instant::now())),
        }
    }

    pub async fn try_send(&self, bytes: u64) -> bool {
        let mut used = self.bytes_used.write().await;
        let mut window = self.window_start.write().await;

        let now = std::time::Instant::now();
        if now.duration_since(*window) >= Duration::from_secs(1) {
            *used = 0;
            *window = now;
        }

        if *used + bytes <= self.max_bytes_per_second {
            *used += bytes;
            true
        } else {
            false
        }
    }

    pub async fn bytes_used(&self) -> u64 {
        *self.bytes_used.read().await
    }
}

pub struct ViewerLimit {
    max_viewers: u32,
    current: Arc<RwLock<u32>>,
}

impl ViewerLimit {
    pub fn new(max_viewers: u32) -> Self {
        Self {
            max_viewers,
            current: Arc::new(RwLock::new(0)),
        }
    }

    pub async fn try_add_viewer(&self) -> bool {
        let mut current = self.current.write().await;
        if *current < self.max_viewers {
            *current += 1;
            true
        } else {
            false
        }
    }

    pub async fn remove_viewer(&self) {
        let mut current = self.current.write().await;
        if *current > 0 {
            *current -= 1;
        }
    }

    pub async fn current_viewers(&self) -> u32 {
        *self.current.read().await
    }

    pub fn max_viewers(&self) -> u32 {
        self.max_viewers
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_graceful_shutdown_default() {
        let shutdown = GracefulShutdown::new();
        assert!(!shutdown.is_shutdown());
    }

    #[tokio::test]
    async fn test_graceful_shutdown_trigger() {
        let shutdown = GracefulShutdown::new();
        shutdown.trigger_shutdown().await;
        assert!(shutdown.is_shutdown());
    }

    #[tokio::test]
    async fn test_graceful_shutdown_wait() {
        let shutdown = Arc::new(GracefulShutdown::new());
        let s = shutdown.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            s.trigger_shutdown().await;
        });
        shutdown.wait_for_shutdown().await;
        assert!(shutdown.is_shutdown());
    }

    #[tokio::test]
    async fn test_graceful_shutdown_drain_timeout() {
        let shutdown = GracefulShutdown::new();
        let result = shutdown.drain_timeout(Duration::from_millis(10)).await;
        assert!(!result);
    }

    #[tokio::test]
    async fn test_rate_limiter_allows() {
        let limiter = RateLimiter::new(10);
        assert!(limiter.try_acquire().await);
    }

    #[tokio::test]
    async fn test_rate_limiter_blocks() {
        let limiter = RateLimiter::new(2);
        assert!(limiter.try_acquire().await);
        assert!(limiter.try_acquire().await);
        assert!(!limiter.try_acquire().await);
    }

    #[tokio::test]
    async fn test_connection_pool_basic() {
        let pool = ConnectionPool::new(5);
        let guard = pool.try_acquire().await;
        assert!(guard.is_some());
        assert_eq!(pool.active_connections().await, 1);
    }

    #[tokio::test]
    async fn test_connection_pool_exhausted() {
        let pool = ConnectionPool::new(1);
        let _guard = pool.try_acquire().await.unwrap();
        assert!(pool.try_acquire().await.is_none());
    }

    #[tokio::test]
    async fn test_connection_pool_guard_drop() {
        let pool = ConnectionPool::new(5);
        {
            let _guard = pool.try_acquire().await.unwrap();
            assert_eq!(pool.active_connections().await, 1);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(pool.active_connections().await, 0);
    }

    #[tokio::test]
    async fn test_connection_pool_enforces_limit_until_guard_drops() {
        let pool = Arc::new(ConnectionPool::new(1));
        let guard = pool.try_acquire().await.unwrap();
        assert!(pool.try_acquire().await.is_none());

        let release = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            drop(guard);
        });
        assert!(pool.drain_timeout(Duration::from_secs(1)).await);
        release.await.unwrap();
        assert!(pool.try_acquire().await.is_some());
    }

    #[tokio::test]
    async fn test_bandwidth_limiter_allows() {
        let limiter = BandwidthLimiter::new(1024);
        assert!(limiter.try_send(512).await);
        assert!(limiter.try_send(512).await);
    }

    #[tokio::test]
    async fn test_bandwidth_limiter_blocks() {
        let limiter = BandwidthLimiter::new(100);
        assert!(limiter.try_send(100).await);
        assert!(!limiter.try_send(1).await);
    }

    #[tokio::test]
    async fn test_viewer_limit() {
        let limit = ViewerLimit::new(2);
        assert!(limit.try_add_viewer().await);
        assert!(limit.try_add_viewer().await);
        assert!(!limit.try_add_viewer().await);
        limit.remove_viewer().await;
        assert!(limit.try_add_viewer().await);
    }

    #[test]
    fn test_config_watcher_check_changed() {
        let dir = std::env::temp_dir().join("reestream_test_watcher");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test.toml");
        std::fs::write(&path, "test").unwrap();
        let mut watcher = ConfigWatcher::new(path.clone());
        assert!(watcher.check_changed());
        assert!(!watcher.check_changed());
        std::fs::write(&path, "test2").unwrap();
        assert!(watcher.check_changed());
        let _ = std::fs::remove_file(&path);
    }
}
