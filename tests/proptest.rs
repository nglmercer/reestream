use bytes::Bytes;
use proptest::prelude::*;

fn is_video_sequence_header(data: &[u8]) -> bool {
    data.len() > 1 && data[0] == 0x17 && data[1] == 0x00
}

fn is_audio_sequence_header(data: &[u8]) -> bool {
    data.len() > 1 && (data[0] & 0xF0) == 0xA0 && data[1] == 0x00
}

proptest! {
    #[test]
    fn test_video_header_never_panics(data in any::<Vec<u8>>()) {
        let bytes = Bytes::from(data.clone());
        let _ = is_video_sequence_header(&data);
        let _ = is_video_sequence_header(&bytes);
    }

    #[test]
    fn test_audio_header_never_panics(data in any::<Vec<u8>>()) {
        let bytes = Bytes::from(data.clone());
        let _ = is_audio_sequence_header(&data);
        let _ = is_audio_sequence_header(&bytes);
    }

    #[test]
    fn test_video_header_requires_minimum_length(
        byte0 in 0x00u8..=0xFF,
        byte1 in 0x00u8..=0xFF,
    ) {
        // Single byte should always be false
        assert!(!is_video_sequence_header(&[byte0]));
        // Two bytes with correct pattern should be true
        let result = is_video_sequence_header(&[byte0, byte1]);
        assert_eq!(result, byte0 == 0x17 && byte1 == 0x00);
    }

    #[test]
    fn test_audio_header_requires_minimum_length(
        byte0 in 0x00u8..=0xFF,
        byte1 in 0x00u8..=0xFF,
    ) {
        assert!(!is_audio_sequence_header(&[byte0]));
        let result = is_audio_sequence_header(&[byte0, byte1]);
        assert_eq!(result, (byte0 & 0xF0) == 0xA0 && byte1 == 0x00);
    }

    #[test]
    fn test_buffer_overflow_protection(
        items in prop::collection::vec(any::<u8>(), 0..1024),
    ) {
        use std::collections::VecDeque;
        let max_size = 256usize;
        let mut buffer: VecDeque<Bytes> = VecDeque::new();

        for item in &items {
            if buffer.len() >= max_size {
                buffer.pop_front();
            }
            buffer.push_back(Bytes::from(vec![*item]));
        }

        assert!(buffer.len() <= max_size);
    }

    #[test]
    fn test_config_parse_never_panics(input in ".*") {
        let _: Result<reestream::config::Config, _> = input.parse();
    }

    #[test]
    fn test_url_parsing_never_panics(input in ".*") {
        let _ = url::Url::parse(&input);
    }

    #[test]
    fn test_rtmp_timestamp_values(
        val in 0u32..=u32::MAX,
    ) {
        let ts = rml_rtmp::time::RtmpTimestamp::new(val);
        assert_eq!(ts.value, val);
    }

    #[test]
    fn test_channel_buffer_capacity(
        capacity in 1usize..1024,
        count in 0usize..2048,
    ) {
        use tokio::sync::mpsc;
        let (tx, mut rx) = mpsc::channel::<Bytes>(capacity);

        let mut sent = 0;
        for i in 0..count {
            if tx.try_send(Bytes::from(vec![i as u8])).is_ok() {
                sent += 1;
            }
        }

        assert!(sent <= capacity);
        assert!(sent <= count);

        // Drain
        let mut received = 0;
        while rx.try_recv().is_ok() {
            received += 1;
        }
        assert_eq!(received, sent);
    }

    #[test]
    fn test_error_display_never_panics(msg in ".*") {
        use reestream::error::RelayError;
        let errors = vec![
            RelayError::Handshake(msg.clone()),
            RelayError::Session(msg.clone()),
            RelayError::Connection(msg.clone()),
            RelayError::Timeout(msg.clone()),
            RelayError::InvalidConfig(msg.clone()),
            RelayError::PublishRejected(msg.clone()),
        ];
        for err in errors {
            let _ = err.to_string();
            let _ = format!("{:?}", err);
        }
    }
}
