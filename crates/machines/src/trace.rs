//! Bus cycle recorder.
//!
//! Every machine cycle the CPU performs is recorded into a ring buffer, and
//! per-line "high time" counters are accumulated so a UI can show what the
//! bus LEDs of a POLY880 would look like at full speed (a dim glow whose
//! brightness is the line's duty cycle).

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CycleKind {
    /// M1 opcode fetch.
    Fetch,
    MemRead,
    MemWrite,
    IoRead,
    IoWrite,
    /// Interrupt acknowledge (M1 + IORQ).
    IntAck,
}

impl CycleKind {
    pub fn short(self) -> &'static str {
        match self {
            CycleKind::Fetch => "M1",
            CycleKind::MemRead => "MR",
            CycleKind::MemWrite => "MW",
            CycleKind::IoRead => "IOR",
            CycleKind::IoWrite => "IOW",
            CycleKind::IntAck => "INTA",
        }
    }

    pub fn is_io(self) -> bool {
        matches!(
            self,
            CycleKind::IoRead | CycleKind::IoWrite | CycleKind::IntAck
        )
    }

    /// Active control lines during this cycle.
    pub fn controls(self) -> Controls {
        use CycleKind::*;
        Controls {
            m1: matches!(self, Fetch | IntAck),
            mreq: matches!(self, Fetch | MemRead | MemWrite),
            iorq: matches!(self, IoRead | IoWrite | IntAck),
            rd: matches!(self, Fetch | MemRead | IoRead),
            wr: matches!(self, MemWrite | IoWrite),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Controls {
    pub m1: bool,
    pub mreq: bool,
    pub iorq: bool,
    pub rd: bool,
    pub wr: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BusCycle {
    /// T-state at the start of the cycle.
    pub t: u64,
    pub kind: CycleKind,
    pub addr: u16,
    pub data: u8,
}

/// Indices into [`BusActivity::lines`] after the 16 address and 8 data lines.
pub const LINE_M1: usize = 24;
pub const LINE_MREQ: usize = 25;
pub const LINE_IORQ: usize = 26;
pub const LINE_RD: usize = 27;
pub const LINE_WR: usize = 28;
pub const LINE_HALT: usize = 29;
pub const LINE_INT: usize = 30;
pub const LINE_NMI: usize = 31;
pub const LINE_COUNT: usize = 32;

/// Duty-cycle of each bus line over a frame (0..1). Address/data: fraction
/// of cycles the line was high. Control lines: fraction they were active.
#[derive(Clone, Debug)]
pub struct BusActivity {
    pub lines: [f32; LINE_COUNT],
    pub cycles: u64,
}

const RING: usize = 4096;

#[derive(Clone, Debug)]
pub struct BusTrace {
    ring: Vec<BusCycle>,
    head: usize,
    len: usize,
    pub enabled: bool,
    /// Weighted (by T-states) high counts per line for the current frame.
    acc: [u64; LINE_COUNT],
    acc_t: u64,
    /// Last full-frame activity, kept while the CPU is paused.
    last_activity: Option<BusActivity>,
    pub last: Option<BusCycle>,
    pub halt: bool,
    pub int: bool,
    pub nmi: bool,
}

impl Default for BusTrace {
    fn default() -> Self {
        BusTrace {
            ring: Vec::with_capacity(RING),
            head: 0,
            len: 0,
            enabled: true,
            acc: [0; LINE_COUNT],
            acc_t: 0,
            last_activity: None,
            last: None,
            halt: false,
            int: false,
            nmi: false,
        }
    }
}

impl BusTrace {
    pub fn record(&mut self, t: u64, kind: CycleKind, addr: u16, data: u8) {
        let c = BusCycle {
            t,
            kind,
            addr,
            data,
        };
        self.last = Some(c);
        if !self.enabled {
            return;
        }
        if self.ring.len() < RING {
            self.ring.push(c);
        } else {
            self.ring[self.head] = c;
        }
        self.head = (self.head + 1) % RING;
        self.len = (self.len + 1).min(RING);

        let w = match kind {
            CycleKind::MemRead | CycleKind::MemWrite => 3,
            CycleKind::IntAck => 6,
            _ => 4,
        };
        // For I/O only the low byte is decoded but the full address is driven.
        for bit in 0..16 {
            if addr & (1 << bit) != 0 {
                self.acc[bit] += w;
            }
        }
        for bit in 0..8 {
            if data & (1 << bit) != 0 {
                self.acc[16 + bit] += w;
            }
        }
        let ctl = kind.controls();
        for (on, line) in [
            (ctl.m1, LINE_M1),
            (ctl.mreq, LINE_MREQ),
            (ctl.iorq, LINE_IORQ),
            (ctl.rd, LINE_RD),
            (ctl.wr, LINE_WR),
            (self.halt, LINE_HALT),
            (self.int, LINE_INT),
            (self.nmi, LINE_NMI),
        ] {
            if on {
                self.acc[line] += w;
            }
        }
        self.acc_t += w;
    }

    /// Most recent cycles, oldest first.
    pub fn recent(&self, n: usize) -> Vec<BusCycle> {
        let n = n.min(self.len);
        (0..n)
            .map(|i| self.ring[(self.head + RING - n + i) % RING])
            .collect()
    }

    pub fn clear(&mut self) {
        self.ring.clear();
        self.head = 0;
        self.len = 0;
    }

    /// Duty cycles since the previous call. If nothing ran, returns the
    /// previous frame's values (or the static pin levels of the last cycle).
    pub fn take_activity(&mut self) -> BusActivity {
        if self.acc_t == 0 {
            if let Some(a) = &self.last_activity {
                return a.clone();
            }
            return self.static_levels();
        }
        let mut lines = [0f32; LINE_COUNT];
        for (l, a) in lines.iter_mut().zip(self.acc.iter()) {
            *l = *a as f32 / self.acc_t as f32;
        }
        let act = BusActivity {
            lines,
            cycles: self.acc_t,
        };
        self.acc = [0; LINE_COUNT];
        self.acc_t = 0;
        self.last_activity = Some(act.clone());
        act
    }

    /// Pin levels frozen at the last cycle (what you see in single-step).
    pub fn static_levels(&self) -> BusActivity {
        levels(self.last.as_ref(), self.halt, self.int, self.nmi)
    }

    /// Forget the averaged frame so the next read shows static levels
    /// (used when single-stepping).
    pub fn freeze(&mut self) {
        self.acc = [0; LINE_COUNT];
        self.acc_t = 0;
        self.last_activity = None;
    }
}

/// Static pin levels during one bus cycle (for single-cycle stepping).
pub fn levels(c: Option<&BusCycle>, halt: bool, int: bool, nmi: bool) -> BusActivity {
    let mut lines = [0f32; LINE_COUNT];
    if let Some(c) = c {
        for (bit, l) in lines.iter_mut().take(16).enumerate() {
            *l = ((c.addr >> bit) & 1) as f32;
        }
        for (bit, l) in lines[16..24].iter_mut().enumerate() {
            *l = ((c.data >> bit) & 1) as f32;
        }
        let ctl = c.kind.controls();
        lines[LINE_M1] = ctl.m1 as u8 as f32;
        lines[LINE_MREQ] = ctl.mreq as u8 as f32;
        lines[LINE_IORQ] = ctl.iorq as u8 as f32;
        lines[LINE_RD] = ctl.rd as u8 as f32;
        lines[LINE_WR] = ctl.wr as u8 as f32;
    }
    lines[LINE_HALT] = halt as u8 as f32;
    lines[LINE_INT] = int as u8 as f32;
    lines[LINE_NMI] = nmi as u8 as f32;
    BusActivity { lines, cycles: 0 }
}
