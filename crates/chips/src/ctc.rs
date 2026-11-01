//! U857 D — Z80 CTC clone.
//!
//! Four channels; each can run as a timer (system clock / 16 or / 256) or a
//! counter (edges on CLK/TRG). Channels 0..2 have a ZC/TO output that pulses
//! when the down-counter reaches zero; channel 3 has none.

use serde::{Deserialize, Serialize};

use crate::daisy::{DaisyDevice, IrqState};

pub const CTRL_CONTROL: u8 = 0x01;
pub const CTRL_RESET: u8 = 0x02;
pub const CTRL_TC_FOLLOWS: u8 = 0x04;
pub const CTRL_TRIGGER: u8 = 0x08;
pub const CTRL_RISING: u8 = 0x10;
pub const CTRL_PRESCALE_256: u8 = 0x20;
pub const CTRL_COUNTER: u8 = 0x40;
pub const CTRL_INT_ENABLE: u8 = 0x80;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelState {
    /// After reset: waiting for a time constant.
    Stopped,
    /// Timer mode with CLK/TRG start: waiting for the trigger edge.
    WaitTrigger,
    Running,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CtcChannel {
    pub control: u8,
    /// Time constant register (0 means 256).
    pub tc: u8,
    /// Down-counter (1..=256).
    pub down: u16,
    pub state: ChannelState,
    pub wait_tc: bool,
    prescale_acc: u32,
    trg_level: bool,
    pub ip: bool,
    pub ius: bool,
    /// Number of zero counts since power-on (for visualisation).
    pub zc_count: u64,
}

impl Default for CtcChannel {
    fn default() -> Self {
        CtcChannel {
            control: CTRL_RESET,
            tc: 0,
            down: 256,
            state: ChannelState::Stopped,
            wait_tc: false,
            prescale_acc: 0,
            trg_level: false,
            ip: false,
            ius: false,
            zc_count: 0,
        }
    }
}

impl CtcChannel {
    fn tc16(&self) -> u16 {
        if self.tc == 0 { 256 } else { self.tc as u16 }
    }

    pub fn is_counter(&self) -> bool {
        self.control & CTRL_COUNTER != 0
    }

    pub fn prescaler(&self) -> u32 {
        if self.control & CTRL_PRESCALE_256 != 0 {
            256
        } else {
            16
        }
    }

    /// Decrement once. Returns true on zero count.
    fn count(&mut self) -> bool {
        self.down -= 1;
        if self.down == 0 {
            self.down = self.tc16();
            self.zc_count += 1;
            if self.control & CTRL_INT_ENABLE != 0 {
                self.ip = true;
            }
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Ctc {
    pub channels: [CtcChannel; 4],
    /// Interrupt vector base (bits 3..7).
    pub vector: u8,
}

impl Ctc {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        for ch in &mut self.channels {
            let trg = ch.trg_level;
            *ch = CtcChannel {
                trg_level: trg,
                ..Default::default()
            };
        }
    }

    pub fn read(&mut self, ch: usize) -> u8 {
        self.channels[ch & 3].down as u8
    }

    pub fn write(&mut self, ch: usize, v: u8) {
        let ch = ch & 3;
        let c = &mut self.channels[ch];
        if c.wait_tc {
            c.tc = v;
            c.wait_tc = false;
            if c.state == ChannelState::Stopped {
                c.down = c.tc16();
                c.prescale_acc = 0;
                c.state = if !c.is_counter() && c.control & CTRL_TRIGGER != 0 {
                    ChannelState::WaitTrigger
                } else {
                    ChannelState::Running
                };
            }
            return;
        }
        if v & CTRL_CONTROL == 0 {
            if ch == 0 {
                self.vector = v & 0xF8;
            }
            return;
        }
        c.control = v;
        if v & CTRL_INT_ENABLE == 0 {
            c.ip = false;
        }
        if v & CTRL_RESET != 0 {
            c.state = ChannelState::Stopped;
        }
        if v & CTRL_TC_FOLLOWS != 0 {
            c.wait_tc = true;
        }
    }

    /// Advance timers by `cycles` system clocks. Returns a bit mask of the
    /// channels whose ZC/TO fired (bit n = channel n), plus the number of
    /// pulses per channel.
    pub fn tick(&mut self, cycles: u32) -> [u32; 4] {
        let mut fired = [0u32; 4];
        for (i, c) in self.channels.iter_mut().enumerate() {
            if c.state != ChannelState::Running || c.is_counter() {
                continue;
            }
            let pre = c.prescaler();
            c.prescale_acc += cycles;
            while c.prescale_acc >= pre {
                c.prescale_acc -= pre;
                if c.count() {
                    fired[i] += 1;
                }
            }
        }
        fired
    }

    /// Drive the CLK/TRG input of a channel. Returns true if this produced a
    /// zero count (ZC/TO pulse).
    pub fn trigger(&mut self, ch: usize, level: bool) -> bool {
        let c = &mut self.channels[ch & 3];
        let prev = c.trg_level;
        c.trg_level = level;
        let rising = c.control & CTRL_RISING != 0;
        let edge = if rising {
            !prev && level
        } else {
            prev && !level
        };
        if !edge {
            return false;
        }
        match c.state {
            ChannelState::WaitTrigger => {
                c.state = ChannelState::Running;
                false
            }
            ChannelState::Running if c.is_counter() => c.count(),
            _ => false,
        }
    }

    /// Convenience: a full pulse (edge in the active direction and back).
    pub fn pulse(&mut self, ch: usize) -> bool {
        let rising = self.channels[ch & 3].control & CTRL_RISING != 0;
        let a = self.trigger(ch, rising);
        let b = self.trigger(ch, !rising);
        a || b
    }
}

impl DaisyDevice for Ctc {
    fn irq_state(&self) -> IrqState {
        let mut state = IrqState::Idle;
        for c in &self.channels {
            if c.ius {
                return IrqState::UnderService;
            }
            if c.ip {
                state = IrqState::Pending;
            }
        }
        state
    }

    fn irq_ack(&mut self) -> u8 {
        for (i, c) in self.channels.iter_mut().enumerate() {
            if c.ip {
                c.ip = false;
                c.ius = true;
                return self.vector | ((i as u8) << 1);
            }
        }
        0xFF
    }

    fn irq_reti(&mut self) {
        for c in &mut self.channels {
            if c.ius {
                c.ius = false;
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_fires() {
        let mut ctc = Ctc::new();
        ctc.write(0, 0x10); // vector
        ctc.write(1, 0x87 & !CTRL_TRIGGER); // int, timer /16, tc follows, reset
        ctc.write(1, 10);
        let mut total = 0;
        for _ in 0..160 {
            total += ctc.tick(1)[1];
        }
        assert_eq!(total, 1);
        assert_eq!(ctc.irq_state(), IrqState::Pending);
        assert_eq!(ctc.irq_ack(), 0x12);
    }

    #[test]
    fn counter_mode() {
        let mut ctc = Ctc::new();
        ctc.write(3, 0x57); // counter, rising, tc follows, reset, no int
        ctc.write(3, 2);
        assert!(!ctc.pulse(3));
        assert!(ctc.pulse(3));
        assert_eq!(ctc.irq_state(), IrqState::Idle);
    }
}
