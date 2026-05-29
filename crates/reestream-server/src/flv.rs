use axum::{
    body::Body,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use bytes::{BufMut, Bytes, BytesMut};
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};

#[derive(Clone)]
pub struct FlvState {
    pub segments: Arc<RwLock<Vec<Bytes>>>,
    pub tx: broadcast::Sender<Bytes>,
}

impl Default for FlvState {
    fn default() -> Self {
        let (tx, _) = broadcast::channel(1024);
        Self {
            segments: Arc::new(RwLock::new(Vec::new())),
            tx,
        }
    }
}

impl FlvState {
    pub async fn push_data(&self, data: Bytes) {
        let _ = self.tx.send(data.clone());
        let mut segments = self.segments.write().await;
        segments.push(data);
        if segments.len() > 1000 {
            segments.drain(..500);
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Bytes> {
        self.tx.subscribe()
    }

    pub async fn get_recent(&self) -> Vec<Bytes> {
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

pub fn flv_stream_response(state: FlvState) -> Response {
    let header = build_flv_header();
    let mut rx = state.subscribe();

    let stream = async_stream::stream! {
        yield Ok::<_, std::convert::Infallible>(header);
        loop {
            match rx.recv().await {
                Ok(chunk) => yield Ok(chunk),
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    };

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "video/x-flv")
        .header("cache-control", "no-cache")
        .header("transfer-encoding", "chunked")
        .body(Body::from_stream(stream))
        .unwrap()
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
        let data = state.get_recent().await;
        assert_eq!(data.len(), 2);
    }

    #[tokio::test]
    async fn test_flv_state_buffer_limit() {
        let state = FlvState::default();
        for i in 0..1100 {
            state.push_data(Bytes::from(vec![i as u8])).await;
        }
        let data = state.get_recent().await;
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

    #[tokio::test]
    async fn test_flv_stream_response_content_type() {
        let state = FlvState::default();
        let response = flv_stream_response(state);
        let headers = response.headers();
        assert_eq!(
            headers.get("content-type").unwrap().to_str().unwrap(),
            "video/x-flv"
        );
        assert_eq!(
            headers.get("cache-control").unwrap().to_str().unwrap(),
            "no-cache"
        );
    }

    #[tokio::test]
    async fn test_flv_tag_wrapping_preserves_data() {
        // Simulate what the DataBus→FlvState bridge does
        let state = FlvState::default();

        // Create a video sequence header (0x17 0x00 = AVC sequence header)
        let video_data = vec![0x17, 0x00, 0x00, 0x00, 0x00, 0x01, 0x64, 0x00, 0x1E];
        let tag = build_flv_tag(0x09, 1000, &video_data);

        // Push the wrapped tag
        state.push_data(tag.clone()).await;

        let data = state.get_recent().await;
        assert_eq!(data.len(), 1);

        // Verify the tag structure
        let tag_data = &data[0];
        assert_eq!(tag_data[0], 0x09); // Video tag type
        assert_eq!(tag_data[4], 0x00); // Timestamp byte 2 (1000 = 0x3E8)
        assert_eq!(tag_data[5], 0x03); // Timestamp byte 1
        assert_eq!(tag_data[6], 0xE8); // Timestamp byte 0

        // Verify data size encoding
        let data_size =
            ((tag_data[1] as u32) << 16) | ((tag_data[2] as u32) << 8) | (tag_data[3] as u32);
        assert_eq!(data_size, video_data.len() as u32);
    }

    #[tokio::test]
    async fn test_flv_stream_with_multiple_tags() {
        let state = FlvState::default();

        // Push video sequence header
        let video_header = build_flv_tag(0x09, 0, &[0x17, 0x00, 0x00, 0x00, 0x00]);
        state.push_data(video_header).await;

        // Push audio sequence header
        let audio_header = build_flv_tag(0x08, 0, &[0xAF, 0x00, 0x12, 0x10]);
        state.push_data(audio_header).await;

        // Push a video frame
        let video_frame = build_flv_tag(0x09, 100, &[0x17, 0x01, 0x00, 0x00, 0x00]);
        state.push_data(video_frame).await;

        let data = state.get_recent().await;
        assert_eq!(data.len(), 3);

        // First tag should be video
        assert_eq!(data[0][0], 0x09);
        // Second tag should be audio
        assert_eq!(data[1][0], 0x08);
        // Third tag should be video
        assert_eq!(data[2][0], 0x09);
    }

    #[tokio::test]
    async fn test_flv_tag_prev_tag_size() {
        let data = vec![0x17, 0x00, 0x00, 0x00, 0x00];
        let tag = build_flv_tag(0x09, 1000, &data);

        // The last 4 bytes should be the previous tag size (11 header + data.len())
        let tag_len = tag.len();
        let prev_tag_size = ((tag[tag_len - 4] as u32) << 24)
            | ((tag[tag_len - 3] as u32) << 16)
            | ((tag[tag_len - 2] as u32) << 8)
            | (tag[tag_len - 1] as u32);
        assert_eq!(prev_tag_size, 11 + data.len() as u32);
    }
}
