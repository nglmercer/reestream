use bytes::Bytes;
use std::sync::{Arc, RwLock as StdRwLock};
use tokio::sync::broadcast;

pub struct DataBus {
    tx: broadcast::Sender<DataPacket>,
    active_stream: Arc<StdRwLock<Option<String>>>,
}

#[derive(Debug, Clone)]
pub struct DataPacket {
    pub stream_id: String,
    pub data: Bytes,
    pub is_video: bool,
    pub timestamp_ms: u32,
}

impl DataBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1024);
        Self {
            tx,
            active_stream: Arc::new(StdRwLock::new(None)),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<DataPacket> {
        self.tx.subscribe()
    }

    pub fn send(&self, packet: DataPacket) {
        let _ = self.tx.send(packet);
    }
}

impl Default for DataBus {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for DataBus {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            active_stream: self.active_stream.clone(),
        }
    }
}

impl reestream_core::client::DataPublisher for DataBus {
    fn publish(&self, stream_id: &str, data: Bytes, is_video: bool, timestamp_ms: u32) {
        if self
            .active_stream
            .read()
            .ok()
            .and_then(|active| active.clone())
            .is_some_and(|active| active != stream_id)
        {
            return;
        }
        self.send(DataPacket {
            stream_id: stream_id.to_string(),
            data,
            is_video,
            timestamp_ms,
        });
    }

    fn try_activate_stream(&self, stream_id: &str) -> bool {
        let Ok(mut active) = self.active_stream.write() else {
            return false;
        };
        match active.as_deref() {
            Some(current) => current == stream_id,
            None => {
                *active = Some(stream_id.to_string());
                true
            }
        }
    }

    fn deactivate_stream(&self, stream_id: &str) {
        if let Ok(mut active) = self.active_stream.write()
            && active.as_deref() == Some(stream_id)
        {
            *active = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_bus_creation() {
        let bus = DataBus::new();
        let _rx = bus.subscribe();
    }

    #[test]
    fn test_data_bus_send_receive() {
        let bus = DataBus::new();
        let mut rx = bus.subscribe();

        bus.send(DataPacket {
            stream_id: "test".into(),
            data: Bytes::from_static(&[0x17, 0x00]),
            is_video: true,
            timestamp_ms: 0,
        });

        let packet = rx.try_recv().unwrap();
        assert_eq!(packet.stream_id, "test");
        assert!(packet.is_video);
    }

    #[test]
    fn test_data_bus_allows_one_active_stream_and_releases_it() {
        let bus = DataBus::new();
        let mut rx = bus.subscribe();
        let publisher = &bus as &dyn reestream_core::client::DataPublisher;

        assert!(publisher.try_activate_stream("first"));
        assert!(!publisher.try_activate_stream("second"));
        publisher.publish("second", Bytes::from_static(b"ignored"), true, 0);
        publisher.publish("first", Bytes::from_static(b"accepted"), true, 1);
        let packet = rx.try_recv().unwrap();
        assert_eq!(packet.stream_id, "first");
        assert_eq!(packet.data, Bytes::from_static(b"accepted"));

        publisher.deactivate_stream("first");
        assert!(publisher.try_activate_stream("second"));
    }
}
