//! Multiplexed 7-segment LED display with persistence.
//!
//! The monitor ROMs light one digit at a time. We integrate, per segment, the
//! T-states it was lit and convert that to brightness when a frame is taken.
//! A digit that is shown 1/6 of the time (full multiplex duty) maps to
//! brightness 1.0; ghosting and flicker of badly timed programs show up
//! naturally.

use serde::{Deserialize, Serialize};

/// Segment bits in the canonical order used by front-ends.
pub const SEG_A: u8 = 0x01;
pub const SEG_B: u8 = 0x02;
pub const SEG_C: u8 = 0x04;
pub const SEG_D: u8 = 0x08;
pub const SEG_E: u8 = 0x10;
pub const SEG_F: u8 = 0x20;
pub const SEG_G: u8 = 0x40;
pub const SEG_DP: u8 = 0x80;

#[derive(Clone, Debug, PartialEq)]
pub struct DisplayFrame {
    /// `[digit][segment]` brightness, digit 0 = leftmost, segment a..g,dp.
    pub digits: Vec<[f32; 8]>,
}

impl DisplayFrame {
    /// Segments brighter than `threshold`, as a bit mask per digit.
    pub fn lit(&self, threshold: f32) -> Vec<u8> {
        self.digits
            .iter()
            .map(|d| {
                d.iter().enumerate().fold(
                    0u8,
                    |m, (i, &b)| if b > threshold { m | (1 << i) } else { m },
                )
            })
            .collect()
    }

    /// Best-effort decoding of the display into text (for tests and logs).
    pub fn text(&self) -> String {
        self.lit(0.25).iter().map(|&m| seg_char(m)).collect()
    }
}

/// Map a segment pattern to a printable character.
pub fn seg_char(m: u8) -> char {
    let dp = m & SEG_DP != 0;
    let c = match m & 0x7F {
        0x00 => ' ',
        0x3F => '0',
        0x06 => '1',
        0x5B => '2',
        0x4F => '3',
        0x66 => '4',
        0x6D => '5',
        0x7D => '6',
        0x07 => '7',
        0x7F => '8',
        0x6F => '9',
        0x77 => 'A',
        0x7C => 'b',
        0x39 => 'C',
        0x58 => 'c',
        0x5E => 'd',
        0x79 => 'E',
        0x71 => 'F',
        0x3D => 'G',
        0x76 => 'H',
        0x30 => 'I',
        0x74 => 'h',
        0x38 => 'L',
        0x54 => 'n',
        0x37 => 'Π',
        0x5C => 'o',
        0x73 => 'P',
        0x50 => 'r',
        0x78 => 't',
        0x3E => 'U',
        0x1C => 'u',
        0x6E => 'Y',
        0x40 => '-',
        0x08 => '_',
        _ => '?',
    };
    if dp && c == ' ' { '.' } else { c }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LedDisplay {
    digits: usize,
    /// Currently enabled digit lines (bit n = digit n from the left).
    select: u16,
    /// Currently driven segments (canonical order).
    segments: u8,
    since: u64,
    frame_start: u64,
    #[serde(skip)]
    acc: Vec<[u64; 8]>,
    #[serde(skip)]
    last: Option<DisplayFrame>,
}

impl LedDisplay {
    pub fn new(digits: usize) -> Self {
        LedDisplay {
            digits,
            select: 0,
            segments: 0,
            since: 0,
            frame_start: 0,
            acc: vec![[0; 8]; digits],
            last: None,
        }
    }

    pub fn digits(&self) -> usize {
        self.digits
    }

    fn integrate(&mut self, t: u64) {
        if self.acc.len() != self.digits {
            self.acc = vec![[0; 8]; self.digits];
        }
        let dt = t.saturating_sub(self.since);
        if dt > 0 && self.select != 0 && self.segments != 0 {
            for d in 0..self.digits {
                if self.select & (1 << d) != 0 {
                    for s in 0..8 {
                        if self.segments & (1 << s) != 0 {
                            self.acc[d][s] += dt;
                        }
                    }
                }
            }
        }
        self.since = t;
    }

    /// New drive state from time `t` on.
    pub fn drive(&mut self, t: u64, select: u16, segments: u8) {
        if select == self.select && segments == self.segments {
            return;
        }
        self.integrate(t);
        self.select = select;
        self.segments = segments;
    }

    /// Brightness since the previous frame. If no time passed (paused), the
    /// previous frame is returned unchanged.
    pub fn frame(&mut self, t: u64) -> DisplayFrame {
        self.integrate(t);
        let span = t.saturating_sub(self.frame_start);
        if span == 0 {
            if let Some(f) = &self.last {
                return f.clone();
            }
            return self.static_frame();
        }
        let gain = self.digits as f32 / span as f32;
        let digits = self
            .acc
            .iter()
            .map(|d| {
                let mut out = [0f32; 8];
                for (o, &a) in out.iter_mut().zip(d.iter()) {
                    *o = (a as f32 * gain).min(1.0);
                }
                out
            })
            .collect();
        for d in self.acc.iter_mut() {
            *d = [0; 8];
        }
        self.frame_start = t;
        let f = DisplayFrame { digits };
        self.last = Some(f.clone());
        f
    }

    /// What is lit right now (used when single stepping).
    pub fn static_frame(&self) -> DisplayFrame {
        let digits = (0..self.digits)
            .map(|d| {
                let mut out = [0f32; 8];
                if self.select & (1 << d) != 0 {
                    for (s, o) in out.iter_mut().enumerate() {
                        if self.segments & (1 << s) != 0 {
                            *o = 1.0;
                        }
                    }
                }
                out
            })
            .collect();
        DisplayFrame { digits }
    }

    /// Re-synchronise timing after loading a state.
    pub fn resync(&mut self, t: u64) {
        self.since = t;
        self.frame_start = t;
        self.acc = vec![[0; 8]; self.digits];
        self.last = None;
    }
}

/// Reorder bits: `out bit i = in bit map[i]`.
pub fn bitswap(v: u8, map: [u8; 8]) -> u8 {
    map.iter()
        .enumerate()
        .fold(0, |acc, (i, &src)| acc | (((v >> src) & 1) << i))
}
