use axum::{http::StatusCode, response::IntoResponse};
use bytes::{BufMut, Bytes, BytesMut};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct FlvState {
    pub segments: Arc<RwLock<Vec<Bytes>>>,
}

impl Default for FlvState {
    fn default() -> Self {
        Self {
            segments: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

impl FlvState {
    pub async fn push_data(&self, data: Bytes) {
        let mut segments = self.segments.write().await;
        segments.push(data);
        if segments.len() > 1000 {
            segments.drain(..500);
        }
    }

    pub async fn get_data(&self) -> Vec<Bytes> {
        self.segments.read().await.clone()
    }
}

pub fn build_flv_header() -> Bytes {
    let mut buf = BytesMut::with_capacity(13);
    buf.extend_from_slice(b"FLV");
    buf.put_u8(1);
    buf.put_u8(0x05);
    buf.put_u32(9);
    buf.put_u32(0);
    buf.freeze()
}

pub fn build_flv_tag(tag_type: u8, timestamp: u32, data: &[u8]) -> Bytes {
    let data_size = data.len() as u32;
    let mut buf = BytesMut::with_capacity(11 + data_size as usize + 4);

    buf.put_u8(tag_type);
    buf.put_u8((data_size >> 16) as u8);
    buf.put_u8((data_size >> 8) as u8);
    buf.put_u8(data_size as u8);
    buf.put_u8((timestamp >> 16) as u8);
    buf.put_u8((timestamp >> 8) as u8);
    buf.put_u8(timestamp as u8);
    buf.put_u8((timestamp >> 24) as u8);
    buf.put_u8(0);
    buf.put_u8(0);
    buf.put_u8(0);
    buf.extend_from_slice(data);

    let prev_tag_size = 11 + data_size;
    buf.put_u32(prev_tag_size);

    buf.freeze()
}

pub async fn flv_stream_impl(state: FlvState) -> impl IntoResponse {
    let data = state.get_data().await;
    let mut response = Vec::new();
    response.extend_from_slice(&build_flv_header());
    for segment in &data {
        response.extend_from_slice(segment);
    }

    (
        StatusCode::OK,
        [
            ("content-type", "video/x-flv"),
            ("cache-control", "no-cache"),
        ],
        response,
    )
}

pub async fn flv_health() -> impl IntoResponse {
    StatusCode::OK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flv_header() {
        let header = build_flv_header();
        assert_eq!(header.len(), 13);
        assert_eq!(&header[..3], b"FLV");
        assert_eq!(header[3], 1);
    }

    #[test]
    fn test_flv_tag_video() {
        let data = vec![0x17, 0x00, 0x00, 0x00, 0x00];
        let tag = build_flv_tag(0x09, 1000, &data);
        assert_eq!(tag[0], 0x09);
        assert!(tag.len() > 11 + data.len());
    }

    #[test]
    fn test_flv_tag_audio() {
        let data = vec![0xAF, 0x00, 0x12, 0x10];
        let tag = build_flv_tag(0x08, 500, &data);
        assert_eq!(tag[0], 0x08);
    }

    #[tokio::test]
    async fn test_flv_state_push_and_get() {
        let state = FlvState::default();
        state.push_data(Bytes::from_static(&[0x01, 0x02])).await;
        state.push_data(Bytes::from_static(&[0x03, 0x04])).await;
        let data = state.get_data().await;
        assert_eq!(data.len(), 2);
    }

    #[tokio::test]
    async fn test_flv_state_buffer_limit() {
        let state = FlvState::default();
        for i in 0..1100 {
            state.push_data(Bytes::from(vec![i as u8])).await;
        }
        let data = state.get_data().await;
        assert_eq!(data.len(), 600);
    }

    #[test]
    fn test_flv_header_signature() {
        let header = build_flv_header();
        assert_eq!(header[0], b'F');
        assert_eq!(header[1], b'L');
        assert_eq!(header[2], b'V');
    }

    #[test]
    fn test_flv_tag_timestamp_encoding() {
        let tag = build_flv_tag(0x09, 0x01020304, &[0xAA]);
        assert_eq!(tag[4], 0x02); // timestamp >> 16
        assert_eq!(tag[5], 0x03); // timestamp >> 8
        assert_eq!(tag[6], 0x04); // timestamp & 0xFF
        assert_eq!(tag[7], 0x01); // timestamp >> 24
    }
}
