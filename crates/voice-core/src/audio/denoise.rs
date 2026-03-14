// Copyright (c) 2026 Lag
// SPDX-License-Identifier: MIT

use nnnoiseless::DenoiseState;
use tracing::debug;

const DENOISE_FRAME_SIZE: usize = DenoiseState::FRAME_SIZE; // 480 samples = 10ms at 48kHz

pub struct Denoiser {
    state: Box<DenoiseState<'static>>,
}

impl Denoiser {
    pub fn new() -> Self {
        debug!("Noise suppression initialized (nnnoiseless, 48kHz)");
        Self {
            state: DenoiseState::new(),
        }
    }

    /// Process a 10ms frame (480 samples) in-place.
    pub fn process(&mut self, input: &mut [f32]) {
        debug_assert!(
            input.len() >= DENOISE_FRAME_SIZE,
            "Denoise frame must be at least {} samples, got {}",
            DENOISE_FRAME_SIZE,
            input.len()
        );

        let mut output = [0.0f32; DENOISE_FRAME_SIZE];
        self.state
            .process_frame(&mut output, &input[..DENOISE_FRAME_SIZE]);
        input[..DENOISE_FRAME_SIZE].copy_from_slice(&output);
    }

    /// Process a 20ms frame (960 samples) by splitting into two 10ms sub-frames.
    pub fn process_frame_20ms(&mut self, input: &mut [f32]) {
        debug_assert!(
            input.len() >= DENOISE_FRAME_SIZE * 2,
            "20ms frame must be at least {} samples, got {}",
            DENOISE_FRAME_SIZE * 2,
            input.len()
        );

        self.process(&mut input[..DENOISE_FRAME_SIZE]);
        let second_start = DENOISE_FRAME_SIZE;
        let second_end = second_start + DENOISE_FRAME_SIZE;
        let mut second_half = [0.0f32; DENOISE_FRAME_SIZE];
        second_half.copy_from_slice(&input[second_start..second_end]);
        self.process(&mut second_half);
        input[second_start..second_end].copy_from_slice(&second_half);
    }
}