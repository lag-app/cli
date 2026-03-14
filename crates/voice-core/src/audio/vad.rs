// Copyright (c) 2026 Lag
// SPDX-License-Identifier: MIT

use tracing::debug;

const DEFAULT_THRESHOLD: f32 = 0.01;
const DEFAULT_HOLD_FRAMES: u32 = 15; // ~300ms at 20ms per frame

pub struct VoiceActivityDetector {
    threshold: f32,
    hold_frames: u32,
    current_hold: u32,
    speaking: bool,
}

impl VoiceActivityDetector {
    pub fn new() -> Self {
        Self {
            threshold: DEFAULT_THRESHOLD,
            hold_frames: DEFAULT_HOLD_FRAMES,
            current_hold: 0,
            speaking: false,
        }
    }

    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.threshold = threshold.clamp(0.0, 1.0);
        self
    }

    pub fn set_threshold(&mut self, threshold: f32) {
        self.threshold = threshold.clamp(0.0, 1.0);
    }

    /// Process a PCM frame and return whether the user is speaking.
    /// Uses RMS energy with hysteresis to prevent flickering.
    pub fn process_frame(&mut self, pcm: &[f32]) -> bool {
        if pcm.is_empty() {
            return self.speaking;
        }

        let rms = (pcm.iter().map(|s| s * s).sum::<f32>() / pcm.len() as f32).sqrt();

        if rms > self.threshold {
            self.current_hold = self.hold_frames;
            if !self.speaking {
                debug!(rms, threshold = self.threshold, "VAD: speech started");
            }
            self.speaking = true;
        } else if self.current_hold > 0 {
            self.current_hold -= 1;
        } else if self.speaking {
            debug!(rms, threshold = self.threshold, "VAD: speech ended");
            self.speaking = false;
        }

        self.speaking
    }

    pub fn is_speaking(&self) -> bool {
        self.speaking
    }

    pub fn reset(&mut self) {
        self.speaking = false;
        self.current_hold = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn silent_frame(len: usize) -> Vec<f32> {
        vec![0.0; len]
    }

    fn loud_frame(len: usize, amplitude: f32) -> Vec<f32> {
        vec![amplitude; len]
    }

    #[test]
    fn silent_frame_not_speaking() {
        let mut vad = VoiceActivityDetector::new();
        assert!(!vad.process_frame(&silent_frame(960)));
    }

    #[test]
    fn loud_frame_is_speaking() {
        let mut vad = VoiceActivityDetector::new();
        assert!(vad.process_frame(&loud_frame(960, 0.5)));
    }

    #[test]
    fn hold_timer_keeps_speaking() {
        let mut vad = VoiceActivityDetector::new().with_threshold(0.01);
        vad.process_frame(&loud_frame(960, 0.5));
        assert!(vad.is_speaking());

        for _ in 0..vad.hold_frames {
            assert!(vad.process_frame(&silent_frame(960)));
        }
    }

    #[test]
    fn hold_timer_expires() {
        let mut vad = VoiceActivityDetector::new().with_threshold(0.01);
        vad.process_frame(&loud_frame(960, 0.5));

        for _ in 0..=vad.hold_frames {
            vad.process_frame(&silent_frame(960));
        }
        assert!(!vad.is_speaking());
    }

    #[test]
    fn threshold_zero_always_speaking() {
        let mut vad = VoiceActivityDetector::new().with_threshold(0.0);
        assert!(vad.process_frame(&loud_frame(960, 0.0001)));
    }

    #[test]
    fn threshold_one_never_speaking() {
        let mut vad = VoiceActivityDetector::new().with_threshold(1.0);
        assert!(!vad.process_frame(&loud_frame(960, 0.99)));
    }

    #[test]
    fn empty_frame_no_panic() {
        let mut vad = VoiceActivityDetector::new();
        let result = vad.process_frame(&[]);
        assert!(!result);
    }

    #[test]
    fn reset_clears_state() {
        let mut vad = VoiceActivityDetector::new();
        vad.process_frame(&loud_frame(960, 0.5));
        assert!(vad.is_speaking());
        vad.reset();
        assert!(!vad.is_speaking());
    }
}