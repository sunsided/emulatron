//! Cassette interface.
//!
//! Both machines drive the tape with a single output bit and read a single
//! comparator input bit. We record the output as a list of level-change
//! intervals (in T-states), which is exact and machine-speed independent,
//! and can play such a recording back into the input bit. Conversion
//! to/from WAV lives in the front-ends.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TapeState {
    Stopped,
    Recording,
    Playing,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tape {
    pub state: TapeState,
    /// Durations between level changes, in T-states. First level is low.
    pub edges: Vec<u32>,
    /// Current output level (also feeds the speaker on the LC-80).
    pub out_level: bool,
    last_out_t: u64,
    play_pos: usize,
    play_t: u64,
    play_level: bool,
    /// Output toggles since power-on (speaker activity indicator).
    pub toggles: u64,
    /// Level changes for audio playback; only collected while enabled.
    #[serde(skip)]
    pub audio: Option<AudioLog>,
}

/// Output level changes waiting to be rendered as audio samples.
#[derive(Clone, Debug, Default)]
pub struct AudioLog {
    edges: std::collections::VecDeque<u64>,
    /// T-state up to which samples have been rendered.
    rendered_t: f64,
    level: bool,
}

/// Keep at most this many pending level changes (a few seconds of tones).
const AUDIO_EDGE_CAP: usize = 1 << 17;

impl AudioLog {
    pub fn new(now: u64, level: bool) -> Self {
        AudioLog {
            edges: Default::default(),
            rendered_t: now as f64,
            level,
        }
    }

    /// Render the pin level from the last call up to `now` as samples of a
    /// square wave (+/- `amp`). Gaps longer than `max_s` seconds (e.g. after
    /// a pause) are skipped instead of rendered.
    pub fn render(
        &mut self,
        now: u64,
        clock_hz: u32,
        sample_rate: f64,
        amp: f32,
        max_s: f64,
    ) -> Vec<f32> {
        let step = clock_hz as f64 / sample_rate;
        let span = now as f64 - self.rendered_t;
        if span > max_s * clock_hz as f64 {
            // Too far behind: jump ahead, applying the skipped level changes.
            let skip_to = now as f64 - max_s * clock_hz as f64;
            while let Some(&e) = self.edges.front() {
                if e as f64 > skip_to {
                    break;
                }
                self.edges.pop_front();
                self.level = !self.level;
            }
            self.rendered_t = skip_to;
        }
        let n = ((now as f64 - self.rendered_t) / step).max(0.0) as usize;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            let t = self.rendered_t;
            while let Some(&e) = self.edges.front() {
                if e as f64 > t {
                    break;
                }
                self.edges.pop_front();
                self.level = !self.level;
            }
            out.push(if self.level { amp } else { -amp });
            self.rendered_t += step;
        }
        out
    }
}

impl Default for Tape {
    fn default() -> Self {
        Tape {
            state: TapeState::Stopped,
            edges: Vec::new(),
            out_level: false,
            last_out_t: 0,
            play_pos: 0,
            play_t: 0,
            play_level: false,
            toggles: 0,
            audio: None,
        }
    }
}

impl Tape {
    pub fn record(&mut self, t: u64) {
        self.state = TapeState::Recording;
        self.edges.clear();
        self.last_out_t = t;
    }

    pub fn play(&mut self, t: u64) {
        self.state = TapeState::Playing;
        self.play_pos = 0;
        self.play_t = t;
        self.play_level = false;
    }

    pub fn stop(&mut self) {
        self.state = TapeState::Stopped;
    }

    /// Machine wrote the tape output bit at time `t`.
    pub fn output(&mut self, t: u64, level: bool) {
        if level == self.out_level {
            return;
        }
        self.out_level = level;
        self.toggles += 1;
        if let Some(a) = &mut self.audio
            && a.edges.len() < AUDIO_EDGE_CAP
        {
            a.edges.push_back(t);
        }
        if self.state == TapeState::Recording {
            let dt = t.saturating_sub(self.last_out_t).min(u32::MAX as u64) as u32;
            self.edges.push(dt);
        }
        self.last_out_t = t;
    }

    /// Level presented to the machine's tape input at time `t`.
    pub fn input(&mut self, t: u64) -> bool {
        if self.state != TapeState::Playing {
            return false;
        }
        while self.play_pos < self.edges.len()
            && t >= self.play_t + self.edges[self.play_pos] as u64
        {
            self.play_t += self.edges[self.play_pos] as u64;
            self.play_pos += 1;
            self.play_level = !self.play_level;
        }
        if self.play_pos >= self.edges.len() {
            self.state = TapeState::Stopped;
        }
        self.play_level
    }

    /// Playback/record position as (current, total) in T-states.
    pub fn progress(&self) -> (u64, u64) {
        let total: u64 = self.edges.iter().map(|&e| e as u64).sum();
        let cur: u64 = self.edges[..self.play_pos.min(self.edges.len())]
            .iter()
            .map(|&e| e as u64)
            .sum();
        (cur, total)
    }
}
