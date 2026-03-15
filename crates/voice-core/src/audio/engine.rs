// Copyright (c) 2026 Lag
// SPDX-License-Identifier: MIT

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, Stream, StreamConfig, SupportedStreamConfigRange};
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::{HeapRb, HeapCons, HeapProd};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{debug, error, info};

pub const SAMPLE_RATE: u32 = 48000;
const CAPTURE_RING_SAMPLES: usize = 4800;   // 100ms - tight to avoid latency buildup
const PLAYBACK_RING_SAMPLES: usize = 24000; // 500ms - enough for network jitter absorption

fn pick_best_config(supported: &[SupportedStreamConfigRange]) -> (u16, u32) {
    if let Some(cfg) = supported.iter().find(|c| {
        c.min_sample_rate().0 <= SAMPLE_RATE && c.max_sample_rate().0 >= SAMPLE_RATE
    }) {
        return (cfg.channels(), SAMPLE_RATE);
    }
    if let Some(cfg) = supported.first() {
        let rate = cfg.max_sample_rate().0.min(48000).max(cfg.min_sample_rate().0);
        return (cfg.channels(), rate);
    }
    (2, SAMPLE_RATE)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDeviceInfo {
    pub name: String,
    pub is_default: bool,
}

pub struct AudioEngine {
    host: Host,
    input_device: Option<Device>,
    output_device: Option<Device>,
    input_stream: Option<Stream>,
    output_stream: Option<Stream>,
    capture_producer: Arc<Mutex<HeapProd<f32>>>,
    capture_consumer: Arc<Mutex<HeapCons<f32>>>,
    playback_producer: Arc<Mutex<HeapProd<f32>>>,
    playback_consumer: Arc<Mutex<HeapCons<f32>>>,
    playback_has_data: Arc<AtomicBool>,
    input_volume: Arc<Mutex<f32>>,
    output_volume: Arc<Mutex<f32>>,
}

// SAFETY: AudioEngine is always accessed behind a parking_lot::Mutex which
// provides synchronization. cpal types are !Send as a cross-platform
// precaution; WASAPI streams on Windows are safe when properly synchronized.
unsafe impl Send for AudioEngine {}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioEngine {
    pub fn new() -> Self {
        let host = cpal::default_host();

        let capture_rb = HeapRb::<f32>::new(CAPTURE_RING_SAMPLES);
        let (capture_prod, capture_cons) = capture_rb.split();

        let playback_rb = HeapRb::<f32>::new(PLAYBACK_RING_SAMPLES);
        let (playback_prod, playback_cons) = playback_rb.split();

        let input_device = host.default_input_device();
        let output_device = host.default_output_device();

        if let Some(ref dev) = input_device {
            info!(device = %dev.name().unwrap_or_default(), "Default input device");
        }
        if let Some(ref dev) = output_device {
            info!(device = %dev.name().unwrap_or_default(), "Default output device");
        }

        Self {
            host,
            input_device,
            output_device,
            input_stream: None,
            output_stream: None,
            capture_producer: Arc::new(Mutex::new(capture_prod)),
            capture_consumer: Arc::new(Mutex::new(capture_cons)),
            playback_producer: Arc::new(Mutex::new(playback_prod)),
            playback_consumer: Arc::new(Mutex::new(playback_cons)),
            playback_has_data: Arc::new(AtomicBool::new(false)),
            input_volume: Arc::new(Mutex::new(1.0)),
            output_volume: Arc::new(Mutex::new(1.0)),
        }
    }

    pub fn list_input_devices(&self) -> Vec<AudioDeviceInfo> {
        let default_name = self
            .host
            .default_input_device()
            .and_then(|d| d.name().ok())
            .unwrap_or_default();

        self.host
            .input_devices()
            .map(|devices| {
                devices
                    .filter_map(|d| {
                        let name = d.name().ok()?;
                        Some(AudioDeviceInfo {
                            is_default: name == default_name,
                            name,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn list_output_devices(&self) -> Vec<AudioDeviceInfo> {
        let default_name = self
            .host
            .default_output_device()
            .and_then(|d| d.name().ok())
            .unwrap_or_default();

        self.host
            .output_devices()
            .map(|devices| {
                devices
                    .filter_map(|d| {
                        let name = d.name().ok()?;
                        Some(AudioDeviceInfo {
                            is_default: name == default_name,
                            name,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn set_input_device(&mut self, device_name: &str) -> Result<()> {
        let device = self
            .host
            .input_devices()
            .map_err(|e| anyhow!("Failed to enumerate input devices: {}", e))?
            .find(|d| d.name().map(|n| n == device_name).unwrap_or(false))
            .ok_or_else(|| anyhow!("Input device '{}' not found", device_name))?;

        info!(device = device_name, "Input device set");
        self.input_device = Some(device);
        Ok(())
    }

    pub fn set_output_device(&mut self, device_name: &str) -> Result<()> {
        let device = self
            .host
            .output_devices()
            .map_err(|e| anyhow!("Failed to enumerate output devices: {}", e))?
            .find(|d| d.name().map(|n| n == device_name).unwrap_or(false))
            .ok_or_else(|| anyhow!("Output device '{}' not found", device_name))?;

        info!(device = device_name, "Output device set");
        self.output_device = Some(device);
        Ok(())
    }

    pub fn start_capture(&mut self) -> Result<()> {
        let device = self
            .input_device
            .as_ref()
            .ok_or_else(|| anyhow!("No input device available"))?;

        let supported = device
            .supported_input_configs()
            .map_err(|e| {
                anyhow!(
                    "Cannot access microphone ({}). On macOS, ensure your terminal app \
                     has microphone permission in System Settings > Privacy & Security > Microphone.",
                    e
                )
            })?
            .collect::<Vec<_>>();

        let (device_channels, device_rate) = pick_best_config(&supported);

        let config = StreamConfig {
            channels: device_channels,
            sample_rate: cpal::SampleRate(device_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        info!(channels = device_channels, sample_rate = device_rate, "Starting capture stream");

        let producer = Arc::clone(&self.capture_producer);
        let volume = Arc::clone(&self.input_volume);
        let ch = device_channels as usize;
        let needs_resample = device_rate != SAMPLE_RATE;
        let rate_ratio = SAMPLE_RATE as f64 / device_rate as f64;

        let stream = device.build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let vol = *volume.lock();
                let mut prod = producer.lock();

                if ch == 1 && !needs_resample {
                    for &s in data {
                        let _ = prod.try_push(s * vol);
                    }
                    return;
                }

                let mono_count = data.len() / ch;
                if needs_resample {
                    let out_len = (mono_count as f64 * rate_ratio) as usize;
                    for i in 0..out_len {
                        let src_pos = i as f64 / rate_ratio;
                        let src_idx = (src_pos as usize) * ch;
                        let frac = (src_pos - (src_pos as usize) as f64) as f32;
                        let s0 = if src_idx < data.len() {
                            if ch == 1 { data[src_idx] } else {
                                data[src_idx..][..ch].iter().sum::<f32>() / ch as f32
                            }
                        } else { 0.0 };
                        let s1 = if src_idx + ch < data.len() {
                            if ch == 1 { data[src_idx + ch] } else {
                                data[src_idx + ch..][..ch].iter().sum::<f32>() / ch as f32
                            }
                        } else { s0 };
                        let _ = prod.try_push((s0 + (s1 - s0) * frac) * vol);
                    }
                } else {
                    for i in 0..mono_count {
                        let base = i * ch;
                        let mono: f32 = data[base..base + ch].iter().sum::<f32>() / ch as f32;
                        let _ = prod.try_push(mono * vol);
                    }
                }
            },
            move |err| {
                error!(%err, "Input stream error");
            },
            None,
        )?;

        stream.play()?;
        info!("Capture stream started");
        self.input_stream = Some(stream);
        Ok(())
    }

    pub fn stop_capture(&mut self) -> Result<()> {
        if let Some(stream) = self.input_stream.take() {
            stream.pause()?;
            info!("Capture stream stopped");
        }
        Ok(())
    }

    pub fn start_playback(&mut self) -> Result<()> {
        let device = self
            .output_device
            .as_ref()
            .ok_or_else(|| anyhow!("No output device available"))?;

        let supported = device
            .supported_output_configs()
            .map_err(|e| anyhow!("Cannot access audio output: {}", e))?
            .collect::<Vec<_>>();

        let (device_channels, device_rate) = pick_best_config(&supported);

        let config = StreamConfig {
            channels: device_channels,
            sample_rate: cpal::SampleRate(device_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        info!(channels = device_channels, sample_rate = device_rate, "Starting playback stream");

        let consumer = Arc::clone(&self.playback_consumer);
        let volume = Arc::clone(&self.output_volume);
        let has_data = Arc::clone(&self.playback_has_data);
        let ch = device_channels as usize;
        let needs_resample = device_rate != SAMPLE_RATE;
        let rate_ratio = device_rate as f64 / SAMPLE_RATE as f64;

        // Decay factor per sample for smooth fade-out on underrun.
        // At 48kHz this reaches ~-40dB in ~5ms - fast enough to be inaudible
        // but smooth enough to avoid clicks.
        let decay: f32 = 0.995;

        let stream = device.build_output_stream(
            &config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let vol = *volume.lock();
                let mut cons = consumer.lock();

                let mut last_sample: f32 = 0.0;
                let mut underrunning = false;

                let out_frames = data.len() / ch;

                if !needs_resample {
                    for i in 0..out_frames {
                        let mono = match cons.try_pop() {
                            Some(s) => {
                                underrunning = false;
                                let v = s * vol;
                                last_sample = v;
                                v
                            }
                            None => {
                                underrunning = true;
                                last_sample *= decay;
                                last_sample
                            }
                        };
                        let base = i * ch;
                        for c in 0..ch {
                            data[base + c] = mono;
                        }
                    }
                } else {
                    let source_needed = (out_frames as f64 / rate_ratio).ceil() as usize + 2;
                    let mut src_buf: Vec<f32> = Vec::with_capacity(source_needed);
                    for _ in 0..source_needed {
                        match cons.try_pop() {
                            Some(s) => src_buf.push(s),
                            None => src_buf.push(0.0),
                        }
                    }
                    for i in 0..out_frames {
                        let src_pos = i as f64 / rate_ratio;
                        let idx = src_pos as usize;
                        let frac = (src_pos - idx as f64) as f32;
                        let s0 = src_buf.get(idx).copied().unwrap_or(0.0);
                        let s1 = src_buf.get(idx + 1).copied().unwrap_or(s0);
                        let s = (s0 + (s1 - s0) * frac) * vol;
                        let base = i * ch;
                        for c in 0..ch {
                            data[base + c] = s;
                        }
                    }
                }

                if underrunning {
                    has_data.store(false, Ordering::Relaxed);
                }
            },
            move |err| {
                error!(%err, "Output stream error");
            },
            None,
        )?;

        stream.play()?;
        info!("Playback stream started");
        self.output_stream = Some(stream);
        Ok(())
    }

    pub fn stop_playback(&mut self) -> Result<()> {
        if let Some(stream) = self.output_stream.take() {
            stream.pause()?;
            info!("Playback stream stopped");
        }
        Ok(())
    }

    pub fn read_capture_buffer(&self, buf: &mut [f32]) -> usize {
        let mut cons = self.capture_consumer.lock();
        let mut count = 0;
        for sample in buf.iter_mut() {
            match cons.try_pop() {
                Some(s) => {
                    *sample = s;
                    count += 1;
                }
                None => break,
            }
        }
        count
    }

    pub fn write_playback_buffer(&self, buf: &[f32]) {
        let mut prod = self.playback_producer.lock();
        for &sample in buf {
            let _ = prod.try_push(sample);
        }
        if !buf.is_empty() {
            self.playback_has_data.store(true, Ordering::Relaxed);
        }
    }

    pub fn set_input_volume(&self, vol: f32) {
        let vol = vol.clamp(0.0, 2.0);
        *self.input_volume.lock() = vol;
        debug!(volume = vol, "Input volume set");
    }

    pub fn set_output_volume(&self, vol: f32) {
        let vol = vol.clamp(0.0, 2.0);
        *self.output_volume.lock() = vol;
        debug!(volume = vol, "Output volume set");
    }

    pub fn buffer_stats(&self) -> AudioBufferStats {
        let cap_occ = self.capture_consumer.lock().occupied_len();
        let play_occ = self.playback_consumer.lock().occupied_len();
        AudioBufferStats {
            capture_buffered_samples: cap_occ,
            capture_buffer_capacity: CAPTURE_RING_SAMPLES,
            capture_buffered_ms: (cap_occ as f64 / SAMPLE_RATE as f64 * 1000.0) as u32,
            playback_buffered_samples: play_occ,
            playback_buffer_capacity: PLAYBACK_RING_SAMPLES,
            playback_buffered_ms: (play_occ as f64 / SAMPLE_RATE as f64 * 1000.0) as u32,
            input_device: self.input_device.as_ref().and_then(|d| d.name().ok()),
            output_device: self.output_device.as_ref().and_then(|d| d.name().ok()),
            input_volume: *self.input_volume.lock(),
            output_volume: *self.output_volume.lock(),
            capture_active: self.input_stream.is_some(),
            playback_active: self.output_stream.is_some(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioBufferStats {
    pub capture_buffered_samples: usize,
    pub capture_buffer_capacity: usize,
    pub capture_buffered_ms: u32,
    pub playback_buffered_samples: usize,
    pub playback_buffer_capacity: usize,
    pub playback_buffered_ms: u32,
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    pub input_volume: f32,
    pub output_volume: f32,
    pub capture_active: bool,
    pub playback_active: bool,
}