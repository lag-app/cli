// Copyright (c) 2026 Lag
// SPDX-License-Identifier: MIT

use anyhow::{anyhow, Result};
use audiopus::coder::{Decoder, Encoder};
use audiopus::packet::Packet;
use audiopus::{Application, Bitrate, Channels, MutSignals, SampleRate, Signal};
use tracing::debug;

const SAMPLE_RATE: SampleRate = SampleRate::Hz48000;
const OPUS_CHANNELS: Channels = Channels::Mono;
const FRAME_SIZE: usize = 960; // 20ms at 48kHz
const ENCODE_BUF_SIZE: usize = 4000;

pub struct OpusCodec {
    encoder: Encoder,
    decoder: Decoder,
    encode_buf: Vec<u8>,
    frame_size: usize,
}

impl OpusCodec {
    pub fn new() -> Result<Self> {
        let mut encoder = Encoder::new(SAMPLE_RATE, OPUS_CHANNELS, Application::Voip)
            .map_err(|e| anyhow!("Failed to create Opus encoder: {}", e))?;

        encoder
            .set_bitrate(Bitrate::BitsPerSecond(64000))
            .map_err(|e| anyhow!("Failed to set bitrate: {}", e))?;
        encoder
            .set_signal(Signal::Voice)
            .map_err(|e| anyhow!("Failed to set signal type: {}", e))?;
        encoder
            .set_inband_fec(true)
            .map_err(|e| anyhow!("Failed to enable inband FEC: {}", e))?;
        encoder
            .set_packet_loss_perc(5)
            .map_err(|e| anyhow!("Failed to set packet loss percentage: {}", e))?;

        let decoder = Decoder::new(SAMPLE_RATE, OPUS_CHANNELS)
            .map_err(|e| anyhow!("Failed to create Opus decoder: {}", e))?;

        debug!("Opus codec initialized: 48kHz mono, 64kbps, VoIP mode");

        Ok(Self {
            encoder,
            decoder,
            encode_buf: vec![0u8; ENCODE_BUF_SIZE],
            frame_size: FRAME_SIZE,
        })
    }

    pub fn encode(&mut self, pcm: &[f32]) -> Result<&[u8]> {
        let len = self
            .encoder
            .encode_float(pcm, &mut self.encode_buf)
            .map_err(|e| anyhow!("Opus encode error: {}", e))?;
        Ok(&self.encode_buf[..len])
    }

    pub fn decode(&mut self, opus_data: &[u8], pcm_out: &mut [f32]) -> Result<usize> {
        let packet =
            Packet::try_from(opus_data).map_err(|e| anyhow!("Invalid Opus packet: {}", e))?;
        let signals =
            MutSignals::try_from(pcm_out).map_err(|e| anyhow!("Invalid PCM buffer: {}", e))?;
        let samples = self
            .decoder
            .decode_float(Some(packet), signals, false)
            .map_err(|e| anyhow!("Opus decode error: {}", e))?;
        Ok(samples)
    }

    pub fn decode_loss(&mut self, pcm_out: &mut [f32]) -> Result<usize> {
        let signals =
            MutSignals::try_from(pcm_out).map_err(|e| anyhow!("Invalid PCM buffer: {}", e))?;
        let samples = self
            .decoder
            .decode_float(None, signals, false)
            .map_err(|e| anyhow!("Opus PLC decode error: {}", e))?;
        Ok(samples)
    }

    pub fn frame_size(&self) -> usize {
        self.frame_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codec_creation() {
        OpusCodec::new().expect("OpusCodec::new() should succeed");
    }

    #[test]
    fn encode_silence() {
        let mut codec = OpusCodec::new().unwrap();
        let silence = vec![0.0f32; 960];
        let encoded = codec.encode(&silence).unwrap();
        assert!(!encoded.is_empty());
    }

    #[test]
    fn encode_decode_roundtrip() {
        let mut codec = OpusCodec::new().unwrap();
        let silence = vec![0.0f32; 960];
        let encoded = codec.encode(&silence).unwrap().to_vec();
        let mut decoded = vec![0.0f32; 960];
        let samples = codec.decode(&encoded, &mut decoded).unwrap();
        assert_eq!(samples, 960);
    }

    #[test]
    fn decode_loss_concealment() {
        let mut codec = OpusCodec::new().unwrap();
        // First encode/decode a frame so the decoder has state
        let silence = vec![0.0f32; 960];
        let encoded = codec.encode(&silence).unwrap().to_vec();
        let mut buf = vec![0.0f32; 960];
        codec.decode(&encoded, &mut buf).unwrap();
        // Now test PLC
        let mut plc_buf = vec![0.0f32; 960];
        let samples = codec.decode_loss(&mut plc_buf).unwrap();
        assert_eq!(samples, 960);
    }

    #[test]
    fn frame_size_is_960() {
        let codec = OpusCodec::new().unwrap();
        assert_eq!(codec.frame_size(), 960);
    }

    #[test]
    fn encode_sine_wave() {
        let mut codec = OpusCodec::new().unwrap();
        let sine: Vec<f32> = (0..960)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 48000.0).sin() * 0.5)
            .collect();
        let encoded = codec.encode(&sine).unwrap();
        assert!(!encoded.is_empty());
    }
}
