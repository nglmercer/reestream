use bytes::Bytes;

fn is_video_sequence_header(data: &[u8]) -> bool {
    data.len() > 1 && data[0] == 0x17 && data[1] == 0x00
}

fn is_audio_sequence_header(data: &[u8]) -> bool {
    data.len() > 1 && (data[0] & 0xF0) == 0xA0 && data[1] == 0x00
}

#[cfg(test)]
mod fuzz_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn fuzz_video_header_detection(data in prop::collection::vec(any::<u8>(), 0..1024)) {
            let bytes = Bytes::from(data.clone());
            let result = is_video_sequence_header(&bytes);
            if data.len() > 1 && data[0] == 0x17 && data[1] == 0x00 {
                prop_assert!(result);
            } else {
                prop_assert!(!result);
            }
        }

        #[test]
        fn fuzz_audio_header_detection(data in prop::collection::vec(any::<u8>(), 0..1024)) {
            let bytes = Bytes::from(data.clone());
            let result = is_audio_sequence_header(&bytes);
            if data.len() > 1 && (data[0] & 0xF0) == 0xA0 && data[1] == 0x00 {
                prop_assert!(result);
            } else {
                prop_assert!(!result);
            }
        }

        #[test]
        fn fuzz_header_detection_no_panic(data in prop::collection::vec(any::<u8>(), 0..4096)) {
            let bytes = Bytes::from(data);
            let _ = is_video_sequence_header(&bytes);
            let _ = is_audio_sequence_header(&bytes);
        }

        #[cfg(feature = "core")]
        #[test]
        fn fuzz_config_parse_no_panic(s in ".*") {
            let _ = s.parse::<reestream::config::Config>();
        }

        #[cfg(any(feature = "hls", feature = "api"))]
        #[test]
        fn fuzz_flv_tag_no_panic(
            tag_type in any::<u8>(),
            timestamp in any::<u32>(),
            data in prop::collection::vec(any::<u8>(), 0..1024),
        ) {
            let _ = reestream::http_server::flv::build_flv_tag(tag_type, timestamp, &data);
        }

        #[cfg(feature = "core")]
        #[test]
        fn fuzz_ip_cidr_match(
            octets in prop::array::uniform4(any::<u8>()),
            prefix in 0u8..=32,
        ) {
            let ip = std::net::IpAddr::V4(std::net::Ipv4Addr::from(octets));
            let entry = reestream::security::IpEntry {
                ip: format!("{}.{}.{}.{}.{}/{}", octets[0], octets[1], octets[2], octets[3], 0, prefix),
                label: None,
            };
            let _ = entry.matches(&ip);
        }

        #[cfg(feature = "ffmpeg")]
        #[test]
        fn fuzz_watermark_position_no_panic(
            margin in 0u32..1000,
        ) {
            use reestream::ffmpeg::processing::WatermarkPosition;
            let positions = [
                WatermarkPosition::TopLeft,
                WatermarkPosition::TopRight,
                WatermarkPosition::BottomLeft,
                WatermarkPosition::BottomRight,
                WatermarkPosition::Center,
            ];
            for pos in &positions {
                let _ = pos.to_overlay(margin);
            }
        }
    }
}
