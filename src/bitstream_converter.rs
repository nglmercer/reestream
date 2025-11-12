use byteorder::{BigEndian, ReadBytesExt};
use image::{ImageBuffer, Rgba};
use openh264::decoder::{Decoder, DecoderConfig, Flush};
use openh264::formats::YUVSource;
use std::error::Error;
use std::io::{Cursor, Read};
use tracing::{debug, error, info, trace, warn};

/// Parsed AVCDecoderConfigurationRecord (AVCC) information.
#[derive(Clone, Debug)]
pub struct AvccInfo {
    pub sps: Vec<Vec<u8>>, // SPS units as raw RBSP bytes (no start code)
    pub pps: Vec<Vec<u8>>, // PPS units as raw RBSP bytes (no start code)
    pub length_size: u8,   // bytes used for NALU length (1..4)
}

impl AvccInfo {
    /// Parse AVCC (sequence header) bytes (as present in RTMP AVC sequence header)
    pub fn from_avcc(avcc: &[u8]) -> Result<Self, Box<dyn Error>> {
        if avcc.len() < 7 {
            return Err("AVCC too short".into());
        }
        let mut rdr = Cursor::new(avcc);

        let _configuration_version = rdr.read_u8()?;
        let _profile = rdr.read_u8()?;
        let _compat = rdr.read_u8()?;
        let _level = rdr.read_u8()?;
        let length_size_byte = rdr.read_u8()?;
        let length_size_minus_one = length_size_byte & 0x03;
        let length_size = (length_size_minus_one + 1) as u8;

        let num_sps_byte = rdr.read_u8()?;
        let num_sps = (num_sps_byte & 0x1F) as usize;

        let mut sps_list = Vec::with_capacity(num_sps);
        for _ in 0..num_sps {
            let sps_len = rdr.read_u16::<BigEndian>()? as usize;
            if sps_len == 0 {
                continue;
            }
            let mut sps = vec![0u8; sps_len];
            rdr.read_exact(&mut sps)?;
            sps_list.push(sps);
        }

        let num_pps = rdr.read_u8()? as usize;
        let mut pps_list = Vec::with_capacity(num_pps);
        for _ in 0..num_pps {
            let pps_len = rdr.read_u16::<BigEndian>()? as usize;
            if pps_len == 0 {
                continue;
            }
            let mut pps = vec![0u8; pps_len];
            rdr.read_exact(&mut pps)?;
            pps_list.push(pps);
        }

        Ok(AvccInfo {
            sps: sps_list,
            pps: pps_list,
            length_size,
        })
    }

    /// Build Annex-B buffer with SPS/PPS start-coded (0x00000001) concatenated
    pub fn sps_pps_annexb(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for sps in &self.sps {
            out.extend_from_slice(&[0, 0, 0, 1]);
            out.extend_from_slice(sps);
        }
        for pps in &self.pps {
            out.extend_from_slice(&[0, 0, 0, 1]);
            out.extend_from_slice(pps);
        }
        out
    }
}

/// Convert length-prefixed (N bytes length) payload to Annex-B (start codes 0x00000001).
pub fn convert_length_prefixed_to_annexb(
    payload: &[u8],
    length_size: u8,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut rdr = Cursor::new(payload);
    let mut out = Vec::<u8>::new();
    while (rdr.position() as usize) < payload.len() {
        // read length
        let size = match length_size {
            1 => rdr.read_u8()? as usize,
            2 => rdr.read_u16::<BigEndian>()? as usize,
            3 => {
                let b1 = rdr.read_u8()? as usize;
                let b2 = rdr.read_u8()? as usize;
                let b3 = rdr.read_u8()? as usize;
                (b1 << 16) | (b2 << 8) | b3
            }
            4 => rdr.read_u32::<BigEndian>()? as usize,
            _ => return Err("unsupported length_size".into()),
        };
        if size == 0 {
            continue;
        }
        if (rdr.position() as usize) + size > payload.len() {
            return Err("malformed length-prefixed payload".into());
        }
        let mut nal = vec![0u8; size];
        rdr.read_exact(&mut nal)?;
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(&nal);
    }
    Ok(out)
}

/// Convert Annex-B back to length-prefixed NALUs with provided length_size.
/// This function will filter out SPS/PPS/SEI (parameter sets) if you want them omitted
/// from the returned AVC payload (useful to avoid duplicating params when sending RTMP).
pub fn convert_annexb_to_length_prefixed(annexb: &[u8], length_size: u8) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0usize;

    while i < annexb.len() {
        // detect start-code (4 or 3 bytes)
        let start_code_len = if i + 3 < annexb.len() && annexb[i..i + 4] == [0, 0, 0, 1] {
            4
        } else if i + 2 < annexb.len() && annexb[i..i + 3] == [0, 0, 1] {
            3
        } else {
            i += 1;
            continue;
        };

        i += start_code_len;
        let start = i;

        // find next start code
        let mut j = i;
        while j < annexb.len() {
            if j + 3 < annexb.len()
                && (annexb[j..j + 4] == [0, 0, 0, 1]
                    || (j + 2 < annexb.len() && annexb[j..j + 3] == [0, 0, 1]))
            {
                break;
            }
            j += 1;
        }

        if start >= annexb.len() {
            break;
        }

        let nal = &annexb[start..j];
        if nal.is_empty() {
            i = j;
            continue;
        }

        // skip parameter sets and AUD and SEI if desired
        let nal_type = nal[0] & 0x1F;
        if nal_type == 7 || nal_type == 8 || nal_type == 6 || nal_type == 9 {
            i = j;
            continue;
        }

        let nal_len = nal.len() as u32;
        match length_size {
            1 => out.push(nal_len as u8),
            2 => {
                out.push((nal_len >> 8) as u8);
                out.push(nal_len as u8);
            }
            3 => {
                out.push((nal_len >> 16) as u8);
                out.push((nal_len >> 8) as u8);
                out.push(nal_len as u8);
            }
            4 => {
                out.push((nal_len >> 24) as u8);
                out.push((nal_len >> 16) as u8);
                out.push((nal_len >> 8) as u8);
                out.push(nal_len as u8);
            }
            _ => panic!("Invalid length_size"),
        }
        out.extend_from_slice(nal);
        i = j;
    }

    out
}

/// BitstreamConverter encapsulates AVCC state, SPS/PPS insertion logic and a decoder instance.
pub struct BitstreamConverter {
    pub avcc: Option<AvccInfo>,
    /// Annex-B bytes for sps+pps (constructed from avcc when available)
    sps_pps_annexb: Vec<u8>,
    /// Flags to track whether we've seen SPS/PPS in-stream (to avoid re-inserting redundantly)
    sps_seen: bool,
    pps_seen: bool,
    /// openh264 decoder instance used to decode Annex-B NALUs into frames
    decoder: Decoder,
}

impl BitstreamConverter {
    /// Create a new (empty) converter. Decoder is created but AVCC is not loaded yet.
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let decoder_cfg = DecoderConfig::new().flush_after_decode(Flush::NoFlush);
        let decoder = Decoder::with_api_config(openh264::OpenH264API::from_source(), decoder_cfg)?;

        Ok(BitstreamConverter {
            avcc: None,
            sps_pps_annexb: Vec::new(),
            sps_seen: false,
            pps_seen: false,
            decoder,
        })
    }

    /// Process an AVCC (sequence header) payload and update internal state.
    /// avcc_payload should be the AVCDecoderConfigurationRecord bytes (exactly what's in RTMP sequence header).
    pub fn set_avcc_from_bytes(&mut self, avcc_payload: &[u8]) -> Result<(), Box<dyn Error>> {
        let info = AvccInfo::from_avcc(avcc_payload)?;
        trace!(
            "Parsed AVCC: sps={} pps={} length_size={}",
            info.sps.len(),
            info.pps.len(),
            info.length_size
        );
        self.sps_pps_annexb = info.sps_pps_annexb();
        self.avcc = Some(info);
        // Reset seen flags so we may inject params before the next IDR if necessary
        self.sps_seen = false;
        self.pps_seen = false;
        Ok(())
    }

    /// Convert an RTMP AVC payload (payload starting at byte 5 of RTMP Video Tag's body)
    /// where the payload is length-prefixed NALUs (AVC NALUs) into Annex-B bytes and insert
    /// SPS/PPS before IDR frames when necessary (uses avcc info).
    ///
    /// `frame_is_keyframe` should be true when the RTMP video tag indicates a keyframe (0x17).
    pub fn convert_rtmp_avc_payload_to_annexb(
        &mut self,
        payload: &[u8],
        frame_is_keyframe: bool,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        let length_size = self.avcc.as_ref().map(|a| a.length_size).unwrap_or(4u8);
        let mut annexb = convert_length_prefixed_to_annexb(payload, length_size)?;

        // If keyframe, ensure SPS/PPS are present (prepend if we have them from avcc)
        if frame_is_keyframe {
            if !self.sps_pps_annexb.is_empty() {
                // Prepend SPS/PPS if annexb doesn't already contain SPS (scan)
                let mut has_sps = false;
                let mut i = 0usize;
                while i + 3 < annexb.len() {
                    if &annexb[i..i + 4] == [0, 0, 0, 1] {
                        let start = i + 4;
                        if start < annexb.len() {
                            let nal_type = annexb[start] & 0x1F;
                            if nal_type == 7 {
                                has_sps = true;
                                break;
                            }
                        }
                        i += 4;
                    } else {
                        i += 1;
                    }
                }
                if !has_sps {
                    let mut new = Vec::with_capacity(self.sps_pps_annexb.len() + annexb.len());
                    new.extend_from_slice(&self.sps_pps_annexb);
                    new.extend_from_slice(&annexb);
                    annexb = new;
                }
            } else {
                // If we don't have avcc SPS/PPS, we still may get them inline in-stream.
                // Nothing to do here — conversion already leaves Annex-B NALUs.
            }
        }

        Ok(annexb)
    }

    /// Decode Annex-B bytes with the internal openh264 decoder and return an RGBA ImageBuffer.
    /// If decoder outputs None, returns Ok(None).
    pub fn decode_annexb_to_rgba(
        &mut self,
        annexb: &[u8],
    ) -> Result<Option<ImageBuffer<Rgba<u8>, Vec<u8>>>, Box<dyn Error>> {
        match self.decoder.decode(annexb) {
            Ok(Some(frame)) => {
                if frame.rgba8_len() == 0 {
                    return Ok(None);
                }
                let (width, height) = frame.dimensions();
                let y_len = frame.y().len();
                let u_len = frame.u().len();
                let v_len = frame.v().len();

                let mut rgba = vec![0u8; width * height * 4];
                frame.write_rgba8(&mut rgba);

                if u_len == v_len && u_len * 4 == y_len {
                    // u and v each are width*height/4 -> I420 / YUV420p
                    let yuv = yuv::YuvPlanarImageMut::alloc(
                        width as _,
                        height as _,
                        yuv::YuvChromaSubsampling::Yuv420,
                    );
                    let yuv = yuv::YuvPlanarImage {
                        y_plane: yuv.y_plane.borrow(),
                        y_stride: yuv.y_stride,
                        u_plane: yuv.u_plane.borrow(),
                        u_stride: yuv.u_stride,
                        v_plane: yuv.v_plane.borrow(),
                        v_stride: yuv.v_stride,
                        width: yuv.width,
                        height: yuv.height,
                    };
                    yuv::yuv420_to_rgba(
                        &yuv,
                        &mut rgba,
                        (width * 4) as _,
                        yuv::YuvRange::Full,
                        yuv::YuvStandardMatrix::Bt601,
                    )
                    .inspect_err(|e| error!("Cannot convert yuv to rgba: {e}"))
                    .unwrap();
                } else if u_len == y_len / 2 && v_len == 0 {
                    // single interleaved UV plane expected -> NV12 (but decoder API usually exposes single UV plane)
                    let yuv = yuv::YuvBiPlanarImageMut::alloc(
                        width as _,
                        height as _,
                        yuv::YuvChromaSubsampling::Yuv420,
                    );
                    let yuv = yuv::YuvBiPlanarImage::from_mut(&yuv);
                    yuv::yuv_nv12_to_rgba(
                        &yuv,
                        &mut rgba,
                        (width * 4) as _,
                        yuv::YuvRange::Full,
                        yuv::YuvStandardMatrix::Bt601,
                        yuv::YuvConversionMode::Balanced,
                    )
                    .inspect_err(|e| error!("Cannot convert yuv to rgba: {e}"))
                    .unwrap();
                } else if u_len == y_len && v_len == y_len {
                    // full chroma -> YUV444
                    let yuv = yuv::YuvPlanarImageMut::alloc(
                        width as _,
                        height as _,
                        yuv::YuvChromaSubsampling::Yuv420,
                    );
                    let yuv = yuv::YuvPlanarImage {
                        y_plane: yuv.y_plane.borrow(),
                        y_stride: yuv.y_stride,
                        u_plane: yuv.u_plane.borrow(),
                        u_stride: yuv.u_stride,
                        v_plane: yuv.v_plane.borrow(),
                        v_stride: yuv.v_stride,
                        width: yuv.width,
                        height: yuv.height,
                    };
                    yuv::yuv444_to_rgba(
                        &yuv,
                        &mut rgba,
                        (width * 4) as _,
                        yuv::YuvRange::Full,
                        yuv::YuvStandardMatrix::Bt601,
                    )
                    .inspect_err(|e| error!("Cannot convert yuv to rgba: {e}"))
                    .unwrap();
                }

                let img =
                    ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(width as u32, height as u32, rgba)
                        .ok_or("failed to create ImageBuffer from decoded frame")?;
                let img = image::imageops::flip_vertical(&img);
                Ok(Some(img))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                error!("openh264 decode error: {:?}", e);
                Err(Box::new(e))
            }
        }
    }

    /// Convenience: if you have the full RTMP Video Tag body (including the first 5 bytes),
    /// call this to process it: it will parse sequence headers, update avcc, convert payload(s),
    /// and optionally decode the Annex-B into an image when possible.
    ///
    /// Returns:
    /// - Ok(Some(Image)) when a frame was decoded and returned as ImageBuffer
    /// - Ok(None) when nothing decoded (sequence header or decode returned none) — but conversion and AVCC parsing still happen
    /// - Err on fatal parsing/decoding errors
    pub fn handle_rtmp_video_tag_and_decode(
        &mut self,
        full_tag_body: &[u8],
    ) -> Result<Option<ImageBuffer<Rgba<u8>, Vec<u8>>>, Box<dyn Error>> {
        // Must be at least 5 bytes header (frame+codec, avc_packet_type, cts 3 bytes)
        if full_tag_body.len() < 5 {
            return Ok(None);
        }

        let first = full_tag_body[0];
        let codec_id = first & 0x0F;
        if codec_id != 7 {
            // not H264/AVC
            return Ok(None);
        }

        let avc_packet_type = full_tag_body[1];
        // let _cts = ((full_tag_body[2] as u32) << 16) | ((full_tag_body[3] as u32) << 8) | (full_tag_body[4] as u32);
        let payload = &full_tag_body[5..];

        match avc_packet_type {
            0 => {
                // AVC sequence header
                // payload is AVCDecoderConfigurationRecord
                self.set_avcc_from_bytes(payload)?;
                info!("AVC sequence header processed");
                return Ok(None);
            }
            1 => {
                // NALUs (length-prefixed)
                let frame_type = (first >> 4) & 0x0F;
                let is_keyframe = frame_type == 1; // 1 == keyframe (0x17 typical)

                let annexb = self.convert_rtmp_avc_payload_to_annexb(payload, is_keyframe)?;
                debug!("Converted RTMP payload -> Annex-B ({} bytes)", annexb.len());

                // Try decode: returns Some(image) when a frame is produced
                let decoded = self.decode_annexb_to_rgba(&annexb)?;
                Ok(decoded)
            }
            2 => {
                // AVC end of sequence
                Ok(None)
            }
            _ => {
                warn!("unknown avc_packet_type {}", avc_packet_type);
                Ok(None)
            }
        }
    }

    /// Expose internal Avcc length_size (useful for re-packing back to RTMP)
    pub fn length_size(&self) -> u8 {
        self.avcc.as_ref().map(|a| a.length_size).unwrap_or(4)
    }
}
