use bytes::Bytes;

// RTMP packet type constants
const RTMP_TYPE_AUDIO: u8 = 0x08;
const RTMP_TYPE_VIDEO: u8 = 0x09;
const RTMP_TYPE_DATA: u8 = 0x12;

// FLV video frame types
const FLV_KEYFRAME: u8 = 0x10;
const FLV_INTERFRAME: u8 = 0x20;
const FLV_CODEC_AVC: u8 = 0x07;

// FLV AVC packet types
const AVC_SEQUENCE_HEADER: u8 = 0x00;
const AVC_NALU: u8 = 0x01;

fn make_video_header(is_keyframe: bool, avc_packet_type: u8) -> Bytes {
    let frame_type = if is_keyframe {
        FLV_KEYFRAME
    } else {
        FLV_INTERFRAME
    };
    Bytes::from(vec![
        frame_type | FLV_CODEC_AVC,
        avc_packet_type,
        0x00,
        0x00,
        0x00,
    ])
}

fn make_audio_header() -> Bytes {
    // AAC, 44kHz, 16-bit, stereo
    Bytes::from(vec![0xAF, AVC_SEQUENCE_HEADER, 0x12, 0x10])
}

#[test]
fn test_rtmp_video_keyframe_header() {
    let data = make_video_header(true, AVC_SEQUENCE_HEADER);
    assert_eq!(data[0], 0x17); // keyframe + AVC
    assert_eq!(data[1], 0x00); // sequence header
}

#[test]
fn test_rtmp_video_interframe() {
    let data = make_video_header(false, AVC_NALU);
    assert_eq!(data[0], 0x27); // interframe + AVC
    assert_eq!(data[1], 0x01); // NALU
}

#[test]
fn test_rtmp_audio_aac_header() {
    let data = make_audio_header();
    assert_eq!(data[0], 0xAF); // AAC, 44kHz, 16-bit, stereo
    assert_eq!(data[1], 0x00); // sequence header
}

#[test]
fn test_flv_video_packet_structure() {
    // Simulate a complete FLV video tag
    // FLV tag: [type(1)][datasize(3)][timestamp(3)][ts_ext(1)][streamid(3)][data(N)]
    let mut tag = Vec::new();
    tag.push(RTMP_TYPE_VIDEO); // byte 0: tag type
    tag.extend_from_slice(&[0x00, 0x00, 0x05]); // bytes 1-3: data size (5)
    tag.extend_from_slice(&[0x00, 0x00, 0x00]); // bytes 4-6: timestamp
    tag.push(0x00); // byte 7: timestamp extended
    tag.extend_from_slice(&[0x00, 0x00, 0x00]); // bytes 8-10: stream ID
    // Video data starts at byte 11
    tag.extend_from_slice(&[0x17, 0x00, 0x00, 0x00, 0x00]); // bytes 11-15: video data

    assert_eq!(tag[0], RTMP_TYPE_VIDEO);
    assert_eq!(tag[11], 0x17); // keyframe + AVC
    assert_eq!(tag[12], 0x00); // sequence header
}

#[test]
fn test_flv_audio_packet_structure() {
    let mut tag = Vec::new();
    tag.push(RTMP_TYPE_AUDIO); // byte 0: tag type
    tag.extend_from_slice(&[0x00, 0x00, 0x04]); // bytes 1-3: data size (4)
    tag.extend_from_slice(&[0x00, 0x00, 0x00]); // bytes 4-6: timestamp
    tag.push(0x00); // byte 7: timestamp extended
    tag.extend_from_slice(&[0x00, 0x00, 0x00]); // bytes 8-10: stream ID
    // Audio data starts at byte 11
    tag.extend_from_slice(&[0xAF, 0x00, 0x12, 0x10]); // bytes 11-14: audio data

    assert_eq!(tag[0], RTMP_TYPE_AUDIO);
    assert_eq!(tag[11], 0xAF);
    assert_eq!(tag[12], 0x00); // AAC sequence header
}

#[test]
fn test_rtmp_types() {
    assert_eq!(RTMP_TYPE_AUDIO, 8);
    assert_eq!(RTMP_TYPE_VIDEO, 9);
    assert_eq!(RTMP_TYPE_DATA, 18);
}

#[test]
fn test_video_sequence_header_detection_from_real_data() {
    // Real AVC decoder configuration record
    let mut data = vec![0x17, 0x00, 0x00, 0x00, 0x00];
    // AVCDecoderConfigurationRecord
    data.push(0x01); // version
    data.push(0x64); // profile (High)
    data.push(0x00); // compatibility
    data.push(0x1E); // level (3.0)
    data.push(0xFF); // NALU length size - 1
    data.push(0xE1); // num SPS
    data.extend_from_slice(&[0x00, 0x19]); // SPS length (25 bytes)
    data.extend_from_slice(&[0x67; 25]); // SPS data (placeholder)
    data.push(0x01); // num PPS
    data.extend_from_slice(&[0x00, 0x09]); // PPS length (9 bytes)
    data.extend_from_slice(&[0x68; 9]); // PPS data (placeholder)

    let bytes = Bytes::from(data);
    assert!(bytes.len() > 1);
    assert_eq!(bytes[0], 0x17);
    assert_eq!(bytes[1], 0x00);
}

#[test]
fn test_audio_sequence_header_aac_specific_config() {
    // AAC AudioSpecificConfig
    let mut data = vec![0xAF, 0x00];
    // AudioSpecificConfig (2 bytes for AAC-LC)
    data.push(0x12); // 5 bits audioObjectType (2=AAC-LC) + 3 bits samplingFreqIndex (4=44100)
    data.push(0x10); // 4 bits samplingFreqIndex cont + 3 bits channelConfig (2=stereo) + padding

    let bytes = Bytes::from(data);
    assert_eq!(bytes[0] & 0xF0, 0xA0); // audio flag
    assert_eq!(bytes[1], 0x00); // sequence header
}
