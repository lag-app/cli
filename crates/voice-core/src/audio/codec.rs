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
        let packet = Packet::try_from(opus_data)
            .map_err(|e| anyhow!("Invalid Opus packet: {}", e))?;
        let signals = MutSignals::try_from(pcm_out)
            .map_err(|e| anyhow!("Invalid PCM buffer: {}", e))?;
        let samples = self
            .decoder
            .decode_float(Some(packet), signals, false)
            .map_err(|e| anyhow!("Opus decode error: {}", e))?;
        Ok(samples)
    }

    pub fn decode_loss(&mut self, pcm_out: &mut [f32]) -> Result<usize> {
        let signals = MutSignals::try_from(pcm_out)
            .map_err(|e| anyhow!("Invalid PCM buffer: {}", e))?;
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