use bytes::{BufMut, Bytes, BytesMut};
use image::{ImageBuffer, Rgba};
use openh264::OpenH264API;
use openh264::encoder::{BitRate, Encoder, EncoderConfig};
use openh264::formats::{RgbSliceU8, YUVBuffer, YUVSource};
use rml_rtmp::sessions::StreamMetadata;
use std::error::Error;

use crate::bitstream_converter::{BitstreamConverter, convert_annexb_to_length_prefixed};

pub struct VideoProcessor {
    encoder: Encoder,
    converter: BitstreamConverter,
    orig_width: Option<u32>,
    orig_height: Option<u32>,
    decoder_ready: bool,
}

impl VideoProcessor {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let encoder =
            Encoder::with_api_config(OpenH264API::from_source(), EncoderConfig::default())?;
        let converter = BitstreamConverter::new()?;

        Ok(Self {
            encoder,
            converter,
            orig_width: None,
            orig_height: None,
            decoder_ready: false,
        })
    }

    pub fn update_metadata(&mut self, metadata: &StreamMetadata) -> Result<(), Box<dyn Error>> {
        if let Some(w) = metadata.video_width {
            self.orig_width = Some(w);
        }
        if let Some(h) = metadata.video_height {
            self.orig_height = Some(h);
        }

        let config = EncoderConfig::default()
            .bitrate(BitRate::from_bps(
                metadata.video_bitrate_kbps.unwrap_or(2500) * 1000,
            ))
            .skip_frames(false);

        self.encoder = Encoder::with_api_config(OpenH264API::from_source(), config)?;

        Ok(())
    }

    pub async fn process_rtmp_video_tag(
        &mut self,
        data: Bytes,
    ) -> Result<Option<Bytes>, Box<dyn Error>> {
        if data.len() < 5 {
            return Ok(None);
        }

        let first = data[0];
        let frame_type = (first >> 4) & 0x0F; // 1=keyframe, 2=inter frame
        let codec_id = first & 0x0F;

        // Solo H.264/AVC (codec id 7)
        if codec_id != 7 {
            return Ok(None);
        }

        let avc_packet_type = data[1];
        let cts = ((data[2] as u32) << 16) | ((data[3] as u32) << 8) | (data[4] as u32);

        match avc_packet_type {
            0 => {
                // Sequence header (AVCDecoderConfigurationRecord)
                let avcc_bytes = data.slice(5..);
                // Actualizar AVCC en el converter (parsea SPS/PPS)
                self.converter.set_avcc_from_bytes(&avcc_bytes)?;
                self.decoder_ready = true;
                // Reenviamos el sequence header sin modificación
                return Ok(Some(data));
            }
            1 => {
                // NALUs (length-prefixed)
                if !self.decoder_ready {
                    // Aún no tenemos AVCC procesado; pasar sin modificar
                    return Ok(Some(data));
                }

                let payload = data.slice(5..);
                let is_keyframe = frame_type == 1;

                // Convertir a Annex-B (inserta SPS/PPS en keyframes si es necesario)
                let annexb = self
                    .converter
                    .convert_rtmp_avc_payload_to_annexb(&payload, is_keyframe)?;

                if annexb.is_empty() {
                    // no conversion result, pasar original
                    return Ok(Some(data));
                }

                // Decodificar -> procesar -> re-encodear
                let processed_annexb =
                    match self.decode_process_reencode(&annexb) {
                        Ok(v) => v,
                        e => {
                            // en caso de error en pipeline de imagen, pasar original
                            tracing::debug!("Error al decodear: {e:?}");
                            return Ok(Some(data));
                        }
                    };

                // Convertir back a length-prefixed (para RTMP) usando length_size del AVCC
                let length_size = self.converter.length_size();
                let nals_len_prefixed =
                    convert_annexb_to_length_prefixed(&processed_annexb, length_size);

                if nals_len_prefixed.is_empty() {
                    return Ok(Some(data));
                }

                let mut out = BytesMut::with_capacity(5 + nals_len_prefixed.len());
                out.put_u8(first);
                out.put_u8(1u8); // AVC NALU
                out.put_u8(((cts >> 16) & 0xff) as u8);
                out.put_u8(((cts >> 8) & 0xff) as u8);
                out.put_u8((cts & 0xff) as u8);
                out.extend_from_slice(&nals_len_prefixed);

                return Ok(Some(out.freeze()));
            }
            2 => {
                // End of sequence: pasar tal cual
                return Ok(Some(data));
            }
            _ => {
                // desconocido
                return Ok(None);
            }
        }
    }

    /// Decodifica Annex-B usando el converter, aplica el procesamiento de imagen y re-encodea.
    /// Retorna Annex-B NALUs del encoder (listas de NALUs con start codes).
    fn decode_process_reencode(&mut self, annexb: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
        // Usar el converter para decodificar Annex-B a ImageBuffer RGBA
        match self.converter.decode_annexb_to_rgba(annexb) {
            Ok(Some(frame_img)) => {
                // Aquí es donde se debe aplicar la modificación de la imagen:
                // frame_img: ImageBuffer<Rgba<u8>, Vec<u8>>
                let dynimg = image::DynamicImage::ImageRgba8(frame_img);

                // Aplicar proceso de imagen (sustituir o editar process_image según necesidad)
                let dynimg_processed = process_image(dynimg);

                // Convertir a RGB y preparar para el encoder
                let rgb_img = dynimg_processed.to_rgb8();
                let (w, h) = rgb_img.dimensions();
                let rgb = rgb_img.into_raw();

                // Convertir RGB -> YUV y encodear con openh264
                let rgb_src = RgbSliceU8::new(&rgb, (w as _, h as _));
                let yuv_src = YUVBuffer::from_rgb8_source(rgb_src);
                let encoded_annexb = self.encoder.encode(&yuv_src)?;

                Ok(encoded_annexb.to_vec())
            }
            Ok(None) => {
                // El decoder no produjo frame aún (buffering). Hacemos passthrough del Annex-B original.
                Ok(annexb.to_vec())
            }
            Err(e) => Err(e),
        }
    }
}

/// Lugar central para aplicar la modificación de imagen.
/// La imagen retornada será re-encodeada y retransmitida.
fn process_image(img: image::DynamicImage) -> image::DynamicImage {
    img.huerotate(24).flipv()
}

#[allow(dead_code)]
fn inspect_nalus(annexb: &[u8]) -> Vec<(u8, usize)> {
    let mut out = Vec::new();
    let mut i = 0usize;

    while i + 3 < annexb.len() {
        let start_code_len = if annexb[i..i + 4] == [0, 0, 0, 1] {
            4
        } else if i + 2 < annexb.len() && annexb[i..i + 3] == [0, 0, 1] {
            3
        } else {
            i += 1;
            continue;
        };

        i += start_code_len;
        let start = i;
        let mut j = i;

        while j + 3 < annexb.len() {
            if annexb[j..j + 4] == [0, 0, 0, 1]
                || (j + 2 < annexb.len() && annexb[j..j + 3] == [0, 0, 1])
            {
                break;
            }
            j += 1;
        }

        if start < annexb.len() {
            let nal = &annexb[start..j];
            if !nal.is_empty() {
                let nal_type = nal[0] & 0x1F;
                out.push((nal_type, nal.len()));
            }
        }
        i = j;
    }

    out
}
