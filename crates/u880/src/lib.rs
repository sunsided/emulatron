//! U880 CPU core.
//!
//! The U880 is the GDR's pin- and opcode-compatible clone of the Zilog Z80.
//! This core executes one instruction per [`Cpu::step`] call but reports
//! every machine cycle (opcode fetch, memory read/write, I/O read/write,
//! interrupt acknowledge) to the [`Bus`] in program order, and counts
//! T-states exactly. That is enough to drive peripheral chips and to
//! visualise what happens on the system bus.

mod alu;
mod cpu;
pub mod disasm;

pub use cpu::{Cpu, Registers};

/// Flag bits of the F register.
pub mod flags {
    pub const C: u8 = 0x01;
    pub const N: u8 = 0x02;
    pub const PV: u8 = 0x04;
    pub const X: u8 = 0x08;
    pub const H: u8 = 0x10;
    pub const Y: u8 = 0x20;
    pub const Z: u8 = 0x40;
    pub const S: u8 = 0x80;
}

/// The system bus as seen from the CPU.
///
/// Each method corresponds to one machine cycle. `t` is the CPU's T-state
/// counter at the *start* of that machine cycle, which lets the board keep
/// peripherals and traces in sync without the CPU knowing about them.
pub trait Bus {
    /// M1 opcode fetch (M1, MREQ, RD active).
    fn fetch(&mut self, addr: u16, t: u64) -> u8 {
        self.read(addr, t)
    }
    /// Memory read (MREQ, RD).
    fn read(&mut self, addr: u16, t: u64) -> u8;
    /// Memory write (MREQ, WR).
    fn write(&mut self, addr: u16, value: u8, t: u64);
    /// I/O read (IORQ, RD). The full 16-bit address is put on the bus.
    fn input(&mut self, port: u16, t: u64) -> u8;
    /// I/O write (IORQ, WR).
    fn output(&mut self, port: u16, value: u8, t: u64);
    /// Interrupt acknowledge (M1 + IORQ). Returns the byte the interrupting
    /// device places on the data bus (vector for IM 2, opcode for IM 0).
    fn int_ack(&mut self, _t: u64) -> u8 {
        0xFF
    }
    /// Refresh cycle following each M1 (`I` in the high byte, `R` low).
    fn refresh(&mut self, _ir: u16, _t: u64) {}
    /// The CPU fetched `ED 4D` (RETI). Z80 peripherals decode this from the
    /// bus to end their interrupt-under-service state.
    fn reti(&mut self) {}
    /// HALT output changed.
    fn halt(&mut self, _halted: bool) {}
}
