use serde::{Deserialize, Serialize};

use crate::Bus;
use crate::alu::{self, parity, sz53, sz53p};
use crate::flags::*;

/// Programmer-visible register file plus interrupt state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registers {
    pub a: u8,
    pub f: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    /// Shadow registers A', F', B', C', D', E', H', L'.
    pub alt: [u8; 8],
    pub ix: u16,
    pub iy: u16,
    pub sp: u16,
    pub pc: u16,
    pub i: u8,
    pub r: u8,
    pub iff1: bool,
    pub iff2: bool,
    pub im: u8,
    /// Internal MEMPTR register (leaks into X/Y of `BIT n,(HL)`).
    pub wz: u16,
}

impl Registers {
    pub fn af(&self) -> u16 {
        u16::from_be_bytes([self.a, self.f])
    }
    pub fn bc(&self) -> u16 {
        u16::from_be_bytes([self.b, self.c])
    }
    pub fn de(&self) -> u16 {
        u16::from_be_bytes([self.d, self.e])
    }
    pub fn hl(&self) -> u16 {
        u16::from_be_bytes([self.h, self.l])
    }
    pub fn set_af(&mut self, v: u16) {
        [self.a, self.f] = v.to_be_bytes();
    }
    pub fn set_bc(&mut self, v: u16) {
        [self.b, self.c] = v.to_be_bytes();
    }
    pub fn set_de(&mut self, v: u16) {
        [self.d, self.e] = v.to_be_bytes();
    }
    pub fn set_hl(&mut self, v: u16) {
        [self.h, self.l] = v.to_be_bytes();
    }
}

/// The CPU: registers plus pin state and a T-state counter.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cpu {
    pub regs: Registers,
    pub halted: bool,
    /// Level of the /INT input (true = asserted).
    pub int_line: bool,
    nmi_line: bool,
    nmi_pending: bool,
    /// Set by EI: interrupts are not accepted until after the next instruction.
    ei_delay: bool,
    /// Total T-states executed since power-on.
    pub t: u64,
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

/// Index register selection when executing DD/FD prefixed opcodes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Idx {
    Hl,
    Ix,
    Iy,
}

impl Cpu {
    pub fn new() -> Self {
        let mut cpu = Cpu {
            regs: Registers::default(),
            halted: false,
            int_line: false,
            nmi_line: false,
            nmi_pending: false,
            ei_delay: false,
            t: 0,
        };
        cpu.reset();
        cpu
    }

    /// /RESET: clears PC, I, R, interrupt state. Other registers keep their
    /// (undefined) contents; we set AF and SP to FFFF like real silicon tends to.
    pub fn reset(&mut self) {
        let r = &mut self.regs;
        r.pc = 0;
        r.i = 0;
        r.r = 0;
        r.iff1 = false;
        r.iff2 = false;
        r.im = 0;
        r.a = 0xFF;
        r.f = 0xFF;
        r.sp = 0xFFFF;
        self.halted = false;
        self.nmi_pending = false;
        self.ei_delay = false;
    }

    /// Drive the /NMI input. NMI is edge triggered: a rising (asserting)
    /// edge latches a pending NMI.
    pub fn set_nmi(&mut self, asserted: bool) {
        if asserted && !self.nmi_line {
            self.nmi_pending = true;
        }
        self.nmi_line = asserted;
    }

    pub fn nmi_line(&self) -> bool {
        self.nmi_line
    }

    /// Execute one instruction (or accept one interrupt, or idle one HALT
    /// cycle). Returns the number of T-states consumed.
    pub fn step<B: Bus>(&mut self, bus: &mut B) -> u32 {
        let start = self.t;
        let blocked = self.ei_delay;
        self.ei_delay = false;

        if self.nmi_pending {
            self.nmi_pending = false;
            self.accept_nmi(bus);
        } else if self.int_line && self.regs.iff1 && !blocked {
            self.accept_int(bus);
        } else if self.halted {
            // HALT executes NOPs without advancing PC.
            let pc = self.regs.pc;
            self.fetch_at(bus, pc);
        } else {
            self.execute(bus);
        }
        (self.t - start) as u32
    }

    // ---- machine cycles -------------------------------------------------

    fn inc_r(&mut self) {
        let r = self.regs.r;
        self.regs.r = (r & 0x80) | (r.wrapping_add(1) & 0x7F);
    }

    fn fetch_at<B: Bus>(&mut self, bus: &mut B, addr: u16) -> u8 {
        let v = bus.fetch(addr, self.t);
        self.inc_r();
        bus.refresh(u16::from_be_bytes([self.regs.i, self.regs.r]), self.t + 2);
        self.t += 4;
        v
    }

    fn fetch_op<B: Bus>(&mut self, bus: &mut B) -> u8 {
        let pc = self.regs.pc;
        self.regs.pc = pc.wrapping_add(1);
        self.fetch_at(bus, pc)
    }

    fn rd<B: Bus>(&mut self, bus: &mut B, addr: u16) -> u8 {
        let v = bus.read(addr, self.t);
        self.t += 3;
        v
    }

    fn wr<B: Bus>(&mut self, bus: &mut B, addr: u16, v: u8) {
        bus.write(addr, v, self.t);
        self.t += 3;
    }

    fn io_in<B: Bus>(&mut self, bus: &mut B, port: u16) -> u8 {
        let v = bus.input(port, self.t);
        self.t += 4;
        v
    }

    fn io_out<B: Bus>(&mut self, bus: &mut B, port: u16, v: u8) {
        bus.output(port, v, self.t);
        self.t += 4;
    }

    #[inline]
    fn internal(&mut self, n: u64) {
        self.t += n;
    }

    fn imm8<B: Bus>(&mut self, bus: &mut B) -> u8 {
        let pc = self.regs.pc;
        self.regs.pc = pc.wrapping_add(1);
        self.rd(bus, pc)
    }

    fn imm16<B: Bus>(&mut self, bus: &mut B) -> u16 {
        let lo = self.imm8(bus);
        let hi = self.imm8(bus);
        u16::from_le_bytes([lo, hi])
    }

    fn push<B: Bus>(&mut self, bus: &mut B, v: u16) {
        let [hi, lo] = v.to_be_bytes();
        self.regs.sp = self.regs.sp.wrapping_sub(1);
        self.wr(bus, self.regs.sp, hi);
        self.regs.sp = self.regs.sp.wrapping_sub(1);
        self.wr(bus, self.regs.sp, lo);
    }

    fn pop<B: Bus>(&mut self, bus: &mut B) -> u16 {
        let lo = self.rd(bus, self.regs.sp);
        self.regs.sp = self.regs.sp.wrapping_add(1);
        let hi = self.rd(bus, self.regs.sp);
        self.regs.sp = self.regs.sp.wrapping_add(1);
        u16::from_le_bytes([lo, hi])
    }

    // ---- interrupts -----------------------------------------------------

    fn leave_halt<B: Bus>(&mut self, bus: &mut B) {
        if self.halted {
            self.halted = false;
            bus.halt(false);
        }
    }

    fn accept_nmi<B: Bus>(&mut self, bus: &mut B) {
        self.leave_halt(bus);
        self.regs.iff1 = false;
        // Dummy opcode fetch (ignored), then one extra T-state.
        let pc = self.regs.pc;
        self.fetch_at(bus, pc);
        self.internal(1);
        self.push(bus, pc);
        self.regs.pc = 0x0066;
        self.regs.wz = 0x0066;
    }

    fn accept_int<B: Bus>(&mut self, bus: &mut B) {
        self.leave_halt(bus);
        self.regs.iff1 = false;
        self.regs.iff2 = false;
        // Acknowledge cycle: M1 with two automatic wait states.
        let v = bus.int_ack(self.t);
        self.inc_r();
        bus.refresh(u16::from_be_bytes([self.regs.i, self.regs.r]), self.t + 4);
        self.t += 6;
        let pc = self.regs.pc;
        match self.regs.im {
            2 => {
                self.internal(1);
                self.push(bus, pc);
                let vec = u16::from_be_bytes([self.regs.i, v]);
                let lo = self.rd(bus, vec);
                let hi = self.rd(bus, vec.wrapping_add(1));
                self.regs.pc = u16::from_le_bytes([lo, hi]);
            }
            1 => {
                self.internal(1);
                self.push(bus, pc);
                self.regs.pc = 0x0038;
            }
            _ => {
                // IM 0: the device supplies an opcode. Only RST is supported,
                // which is what every real U880 system puts on the bus.
                self.internal(1);
                self.push(bus, pc);
                self.regs.pc = if v & 0xC7 == 0xC7 {
                    (v & 0x38) as u16
                } else {
                    0x0038
                };
            }
        }
        self.regs.wz = self.regs.pc;
    }

    // ---- register helpers -------------------------------------------------

    fn idx_reg(&self, idx: Idx) -> u16 {
        match idx {
            Idx::Hl => self.regs.hl(),
            Idx::Ix => self.regs.ix,
            Idx::Iy => self.regs.iy,
        }
    }

    fn set_idx_reg(&mut self, idx: Idx, v: u16) {
        match idx {
            Idx::Hl => self.regs.set_hl(v),
            Idx::Ix => self.regs.ix = v,
            Idx::Iy => self.regs.iy = v,
        }
    }

    /// 8-bit register by encoding (0..7 except 6). With an index prefix,
    /// H/L map to the undocumented IXH/IXL/IYH/IYL.
    fn reg8(&self, r: u8, idx: Idx) -> u8 {
        let rg = &self.regs;
        match r {
            0 => rg.b,
            1 => rg.c,
            2 => rg.d,
            3 => rg.e,
            4 => (self.idx_reg(idx) >> 8) as u8,
            5 => self.idx_reg(idx) as u8,
            7 => rg.a,
            _ => unreachable!("(HL) is not a register"),
        }
    }

    fn set_reg8(&mut self, r: u8, idx: Idx, v: u8) {
        match r {
            0 => self.regs.b = v,
            1 => self.regs.c = v,
            2 => self.regs.d = v,
            3 => self.regs.e = v,
            4 => {
                let x = self.idx_reg(idx);
                self.set_idx_reg(idx, (x & 0x00FF) | ((v as u16) << 8));
            }
            5 => {
                let x = self.idx_reg(idx);
                self.set_idx_reg(idx, (x & 0xFF00) | v as u16);
            }
            7 => self.regs.a = v,
            _ => unreachable!("(HL) is not a register"),
        }
    }

    /// Register pair table `rp`: BC, DE, HL/IX/IY, SP.
    fn rp(&self, p: u8, idx: Idx) -> u16 {
        match p {
            0 => self.regs.bc(),
            1 => self.regs.de(),
            2 => self.idx_reg(idx),
            _ => self.regs.sp,
        }
    }

    fn set_rp(&mut self, p: u8, idx: Idx, v: u16) {
        match p {
            0 => self.regs.set_bc(v),
            1 => self.regs.set_de(v),
            2 => self.set_idx_reg(idx, v),
            _ => self.regs.sp = v,
        }
    }

    /// Register pair table `rp2`: BC, DE, HL/IX/IY, AF.
    fn rp2(&self, p: u8, idx: Idx) -> u16 {
        if p == 3 {
            self.regs.af()
        } else {
            self.rp(p, idx)
        }
    }

    fn set_rp2(&mut self, p: u8, idx: Idx, v: u16) {
        if p == 3 {
            self.regs.set_af(v)
        } else {
            self.set_rp(p, idx, v)
        }
    }

    /// Effective address of the `(HL)` operand; with an index prefix reads the
    /// displacement and spends `delay` internal T-states adding it.
    fn mem_operand<B: Bus>(&mut self, bus: &mut B, idx: Idx, delay: u64) -> u16 {
        match idx {
            Idx::Hl => self.regs.hl(),
            _ => {
                let d = self.imm8(bus) as i8;
                self.internal(delay);
                let a = self.idx_reg(idx).wrapping_add(d as u16);
                self.regs.wz = a;
                a
            }
        }
    }

    fn cond(&self, y: u8) -> bool {
        let f = self.regs.f;
        match y & 7 {
            0 => f & Z == 0,
            1 => f & Z != 0,
            2 => f & C == 0,
            3 => f & C != 0,
            4 => f & PV == 0,
            5 => f & PV != 0,
            6 => f & S == 0,
            _ => f & S != 0,
        }
    }

    // ---- execution ------------------------------------------------------

    fn execute<B: Bus>(&mut self, bus: &mut B) {
        let mut op = self.fetch_op(bus);
        let mut idx = Idx::Hl;
        loop {
            match op {
                0xDD => idx = Idx::Ix,
                0xFD => idx = Idx::Iy,
                _ => break,
            }
            op = self.fetch_op(bus);
        }
        match op {
            0xCB => self.exec_cb(bus, idx),
            0xED => {
                let op = self.fetch_op(bus);
                self.exec_ed(bus, op);
            }
            _ => self.exec_main(bus, op, idx),
        }
    }

    fn exec_main<B: Bus>(&mut self, bus: &mut B, op: u8, idx: Idx) {
        let x = op >> 6;
        let y = (op >> 3) & 7;
        let z = op & 7;
        let p = y >> 1;
        let q = y & 1;

        match x {
            0 => match z {
                0 => match y {
                    0 => {}
                    1 => {
                        let r = &mut self.regs;
                        std::mem::swap(&mut r.a, &mut r.alt[0]);
                        std::mem::swap(&mut r.f, &mut r.alt[1]);
                    }
                    2 => {
                        self.internal(1);
                        let d = self.imm8(bus) as i8;
                        self.regs.b = self.regs.b.wrapping_sub(1);
                        if self.regs.b != 0 {
                            self.internal(5);
                            self.regs.pc = self.regs.pc.wrapping_add(d as u16);
                            self.regs.wz = self.regs.pc;
                        }
                    }
                    3 => {
                        let d = self.imm8(bus) as i8;
                        self.internal(5);
                        self.regs.pc = self.regs.pc.wrapping_add(d as u16);
                        self.regs.wz = self.regs.pc;
                    }
                    _ => {
                        let d = self.imm8(bus) as i8;
                        if self.cond(y - 4) {
                            self.internal(5);
                            self.regs.pc = self.regs.pc.wrapping_add(d as u16);
                            self.regs.wz = self.regs.pc;
                        }
                    }
                },
                1 => {
                    if q == 0 {
                        let v = self.imm16(bus);
                        self.set_rp(p, idx, v);
                    } else {
                        let a = self.idx_reg(idx);
                        let b = self.rp(p, idx);
                        let (r, f) = alu::add16(a, b, self.regs.f);
                        self.internal(7);
                        self.regs.wz = a.wrapping_add(1);
                        self.set_idx_reg(idx, r);
                        self.regs.f = f;
                    }
                }
                2 => match (q, p) {
                    (0, 0) => {
                        let a = self.regs.bc();
                        self.wr(bus, a, self.regs.a);
                        self.regs.wz = u16::from_be_bytes([self.regs.a, a.wrapping_add(1) as u8]);
                    }
                    (0, 1) => {
                        let a = self.regs.de();
                        self.wr(bus, a, self.regs.a);
                        self.regs.wz = u16::from_be_bytes([self.regs.a, a.wrapping_add(1) as u8]);
                    }
                    (0, 2) => {
                        let a = self.imm16(bus);
                        let [hi, lo] = self.idx_reg(idx).to_be_bytes();
                        self.wr(bus, a, lo);
                        self.wr(bus, a.wrapping_add(1), hi);
                        self.regs.wz = a.wrapping_add(1);
                    }
                    (0, _) => {
                        let a = self.imm16(bus);
                        self.wr(bus, a, self.regs.a);
                        self.regs.wz = u16::from_be_bytes([self.regs.a, a.wrapping_add(1) as u8]);
                    }
                    (_, 0) => {
                        let a = self.regs.bc();
                        self.regs.a = self.rd(bus, a);
                        self.regs.wz = a.wrapping_add(1);
                    }
                    (_, 1) => {
                        let a = self.regs.de();
                        self.regs.a = self.rd(bus, a);
                        self.regs.wz = a.wrapping_add(1);
                    }
                    (_, 2) => {
                        let a = self.imm16(bus);
                        let lo = self.rd(bus, a);
                        let hi = self.rd(bus, a.wrapping_add(1));
                        self.set_idx_reg(idx, u16::from_le_bytes([lo, hi]));
                        self.regs.wz = a.wrapping_add(1);
                    }
                    _ => {
                        let a = self.imm16(bus);
                        self.regs.a = self.rd(bus, a);
                        self.regs.wz = a.wrapping_add(1);
                    }
                },
                3 => {
                    self.internal(2);
                    let v = self.rp(p, idx);
                    let v = if q == 0 {
                        v.wrapping_add(1)
                    } else {
                        v.wrapping_sub(1)
                    };
                    self.set_rp(p, idx, v);
                }
                4 | 5 => {
                    let f = self.regs.f;
                    let calc = |v| {
                        if z == 4 {
                            alu::inc8(v, f)
                        } else {
                            alu::dec8(v, f)
                        }
                    };
                    if y == 6 {
                        let a = self.mem_operand(bus, idx, 5);
                        let v = self.rd(bus, a);
                        self.internal(1);
                        let (r, nf) = calc(v);
                        self.wr(bus, a, r);
                        self.regs.f = nf;
                    } else {
                        let (r, nf) = calc(self.reg8(y, idx));
                        self.set_reg8(y, idx, r);
                        self.regs.f = nf;
                    }
                }
                6 => {
                    if y == 6 {
                        let a = self.mem_operand(bus, idx, 0);
                        let n = self.imm8(bus);
                        if idx != Idx::Hl {
                            self.internal(2);
                        }
                        self.wr(bus, a, n);
                    } else {
                        let n = self.imm8(bus);
                        self.set_reg8(y, idx, n);
                    }
                }
                _ => {
                    let r = &mut self.regs;
                    let a = r.a;
                    let f = r.f;
                    let keep = f & (S | Z | PV);
                    match y {
                        0 => {
                            r.a = a.rotate_left(1);
                            r.f = keep | (r.a & (X | Y)) | (a >> 7);
                        }
                        1 => {
                            r.a = a.rotate_right(1);
                            r.f = keep | (r.a & (X | Y)) | (a & 1);
                        }
                        2 => {
                            r.a = (a << 1) | (f & C);
                            r.f = keep | (r.a & (X | Y)) | (a >> 7);
                        }
                        3 => {
                            r.a = (a >> 1) | ((f & C) << 7);
                            r.f = keep | (r.a & (X | Y)) | (a & 1);
                        }
                        4 => (r.a, r.f) = alu::daa(a, f),
                        5 => {
                            r.a = !a;
                            r.f = (f & (S | Z | PV | C)) | H | N | (r.a & (X | Y));
                        }
                        6 => r.f = (f & (S | Z | PV)) | C | (a & (X | Y)),
                        _ => {
                            let h = if f & C != 0 { H } else { 0 };
                            r.f = ((f & (S | Z | PV | C)) | h | (a & (X | Y))) ^ C;
                        }
                    }
                }
            },
            1 => {
                if y == 6 && z == 6 {
                    self.halted = true;
                    bus.halt(true);
                } else if z == 6 {
                    let a = self.mem_operand(bus, idx, 5);
                    let v = self.rd(bus, a);
                    self.set_reg8(y, Idx::Hl, v);
                } else if y == 6 {
                    let a = self.mem_operand(bus, idx, 5);
                    let v = self.reg8(z, Idx::Hl);
                    self.wr(bus, a, v);
                } else {
                    let v = self.reg8(z, idx);
                    self.set_reg8(y, idx, v);
                }
            }
            2 => {
                let v = if z == 6 {
                    let a = self.mem_operand(bus, idx, 5);
                    self.rd(bus, a)
                } else {
                    self.reg8(z, idx)
                };
                let (a, f) = alu::alu(y, self.regs.a, v, self.regs.f);
                self.regs.a = a;
                self.regs.f = f;
            }
            _ => match z {
                0 => {
                    self.internal(1);
                    if self.cond(y) {
                        self.regs.pc = self.pop(bus);
                        self.regs.wz = self.regs.pc;
                    }
                }
                1 => {
                    if q == 0 {
                        let v = self.pop(bus);
                        self.set_rp2(p, idx, v);
                    } else {
                        match p {
                            0 => {
                                self.regs.pc = self.pop(bus);
                                self.regs.wz = self.regs.pc;
                            }
                            1 => {
                                let r = &mut self.regs;
                                std::mem::swap(&mut r.b, &mut r.alt[2]);
                                std::mem::swap(&mut r.c, &mut r.alt[3]);
                                std::mem::swap(&mut r.d, &mut r.alt[4]);
                                std::mem::swap(&mut r.e, &mut r.alt[5]);
                                std::mem::swap(&mut r.h, &mut r.alt[6]);
                                std::mem::swap(&mut r.l, &mut r.alt[7]);
                            }
                            2 => self.regs.pc = self.idx_reg(idx),
                            _ => {
                                self.internal(2);
                                self.regs.sp = self.idx_reg(idx);
                            }
                        }
                    }
                }
                2 => {
                    let a = self.imm16(bus);
                    self.regs.wz = a;
                    if self.cond(y) {
                        self.regs.pc = a;
                    }
                }
                3 => match y {
                    0 => {
                        let a = self.imm16(bus);
                        self.regs.pc = a;
                        self.regs.wz = a;
                    }
                    1 => unreachable!("CB handled by caller"),
                    2 => {
                        let n = self.imm8(bus);
                        let port = u16::from_be_bytes([self.regs.a, n]);
                        self.io_out(bus, port, self.regs.a);
                        self.regs.wz = u16::from_be_bytes([self.regs.a, n.wrapping_add(1)]);
                    }
                    3 => {
                        let n = self.imm8(bus);
                        let port = u16::from_be_bytes([self.regs.a, n]);
                        self.regs.a = self.io_in(bus, port);
                        self.regs.wz = port.wrapping_add(1);
                    }
                    4 => {
                        let sp = self.regs.sp;
                        let lo = self.rd(bus, sp);
                        let hi = self.rd(bus, sp.wrapping_add(1));
                        self.internal(1);
                        let [vh, vl] = self.idx_reg(idx).to_be_bytes();
                        self.wr(bus, sp.wrapping_add(1), vh);
                        self.wr(bus, sp, vl);
                        self.internal(2);
                        let v = u16::from_le_bytes([lo, hi]);
                        self.set_idx_reg(idx, v);
                        self.regs.wz = v;
                    }
                    5 => {
                        let r = &mut self.regs;
                        std::mem::swap(&mut r.d, &mut r.h);
                        std::mem::swap(&mut r.e, &mut r.l);
                    }
                    6 => {
                        self.regs.iff1 = false;
                        self.regs.iff2 = false;
                    }
                    _ => {
                        self.regs.iff1 = true;
                        self.regs.iff2 = true;
                        self.ei_delay = true;
                    }
                },
                4 => {
                    let a = self.imm16(bus);
                    self.regs.wz = a;
                    if self.cond(y) {
                        self.internal(1);
                        let pc = self.regs.pc;
                        self.push(bus, pc);
                        self.regs.pc = a;
                    }
                }
                5 => {
                    if q == 0 {
                        self.internal(1);
                        let v = self.rp2(p, idx);
                        self.push(bus, v);
                    } else {
                        // p == 0: CALL nn (prefixes are consumed by the caller).
                        let a = self.imm16(bus);
                        self.internal(1);
                        let pc = self.regs.pc;
                        self.push(bus, pc);
                        self.regs.pc = a;
                        self.regs.wz = a;
                    }
                }
                6 => {
                    let n = self.imm8(bus);
                    let (a, f) = alu::alu(y, self.regs.a, n, self.regs.f);
                    self.regs.a = a;
                    self.regs.f = f;
                }
                _ => {
                    self.internal(1);
                    let pc = self.regs.pc;
                    self.push(bus, pc);
                    self.regs.pc = (y as u16) * 8;
                    self.regs.wz = self.regs.pc;
                }
            },
        }
    }

    fn exec_cb<B: Bus>(&mut self, bus: &mut B, idx: Idx) {
        let (op, addr) = if idx == Idx::Hl {
            (self.fetch_op(bus), self.regs.hl())
        } else {
            // DD CB d op: displacement and opcode are plain memory reads.
            let d = self.imm8(bus) as i8;
            let op = self.imm8(bus);
            self.internal(2);
            let a = self.idx_reg(idx).wrapping_add(d as u16);
            self.regs.wz = a;
            (op, a)
        };
        let x = op >> 6;
        let y = (op >> 3) & 7;
        let z = op & 7;
        let mem = idx != Idx::Hl || z == 6;

        let v = if mem {
            let v = self.rd(bus, addr);
            self.internal(1);
            v
        } else {
            self.reg8(z, Idx::Hl)
        };

        let f = self.regs.f;
        let result = match x {
            0 => {
                let (r, nf) = alu::rot(y, v, f);
                self.regs.f = nf;
                r
            }
            1 => {
                let xy = if mem { (self.regs.wz >> 8) as u8 } else { v };
                self.regs.f = alu::bit(y, v, f, xy);
                return;
            }
            2 => v & !(1 << y),
            _ => v | (1 << y),
        };

        if mem {
            self.wr(bus, addr, result);
            // Undocumented: DD CB d op with z != 6 also copies to a register.
            if idx != Idx::Hl && z != 6 {
                self.set_reg8(z, Idx::Hl, result);
            }
        } else {
            self.set_reg8(z, Idx::Hl, result);
        }
    }

    fn exec_ed<B: Bus>(&mut self, bus: &mut B, op: u8) {
        let x = op >> 6;
        let y = (op >> 3) & 7;
        let z = op & 7;
        let p = y >> 1;
        let q = y & 1;

        match (x, z) {
            (1, 0) => {
                let bc = self.regs.bc();
                let v = self.io_in(bus, bc);
                self.regs.wz = bc.wrapping_add(1);
                if y != 6 {
                    self.set_reg8(y, Idx::Hl, v);
                }
                self.regs.f = (self.regs.f & C) | sz53p(v);
            }
            (1, 1) => {
                let bc = self.regs.bc();
                let v = if y == 6 { 0 } else { self.reg8(y, Idx::Hl) };
                self.io_out(bus, bc, v);
                self.regs.wz = bc.wrapping_add(1);
            }
            (1, 2) => {
                let hl = self.regs.hl();
                let rr = self.rp(p, Idx::Hl);
                let (r, f) = if q == 0 {
                    alu::sbc16(hl, rr, self.regs.f)
                } else {
                    alu::adc16(hl, rr, self.regs.f)
                };
                self.internal(7);
                self.regs.wz = hl.wrapping_add(1);
                self.regs.set_hl(r);
                self.regs.f = f;
            }
            (1, 3) => {
                let a = self.imm16(bus);
                if q == 0 {
                    let [hi, lo] = self.rp(p, Idx::Hl).to_be_bytes();
                    self.wr(bus, a, lo);
                    self.wr(bus, a.wrapping_add(1), hi);
                } else {
                    let lo = self.rd(bus, a);
                    let hi = self.rd(bus, a.wrapping_add(1));
                    self.set_rp(p, Idx::Hl, u16::from_le_bytes([lo, hi]));
                }
                self.regs.wz = a.wrapping_add(1);
            }
            (1, 4) => {
                let (a, f) = alu::sub8(0, self.regs.a, false);
                self.regs.a = a;
                self.regs.f = f;
            }
            (1, 5) => {
                self.regs.iff1 = self.regs.iff2;
                self.regs.pc = self.pop(bus);
                self.regs.wz = self.regs.pc;
                if y == 1 {
                    bus.reti();
                }
            }
            (1, 6) => self.regs.im = [0, 0, 1, 2, 0, 0, 1, 2][y as usize],
            (1, 7) => match y {
                0 => {
                    self.internal(1);
                    self.regs.i = self.regs.a;
                }
                1 => {
                    self.internal(1);
                    self.regs.r = self.regs.a;
                }
                2 | 3 => {
                    self.internal(1);
                    let v = if y == 2 { self.regs.i } else { self.regs.r };
                    self.regs.a = v;
                    self.regs.f = (self.regs.f & C) | sz53(v) | if self.regs.iff2 { PV } else { 0 };
                }
                4 | 5 => {
                    let hl = self.regs.hl();
                    let v = self.rd(bus, hl);
                    self.internal(4);
                    let a = self.regs.a;
                    let (nv, na) = if y == 4 {
                        // RRD
                        ((a << 4) | (v >> 4), (a & 0xF0) | (v & 0x0F))
                    } else {
                        // RLD
                        ((v << 4) | (a & 0x0F), (a & 0xF0) | (v >> 4))
                    };
                    self.wr(bus, hl, nv);
                    self.regs.a = na;
                    self.regs.f = (self.regs.f & C) | sz53p(na);
                    self.regs.wz = hl.wrapping_add(1);
                }
                _ => {}
            },
            (2, 0..=3) if y >= 4 => self.exec_block(bus, y, z),
            _ => {} // NONI: behaves as an 8 T-state NOP.
        }
    }

    fn exec_block<B: Bus>(&mut self, bus: &mut B, y: u8, z: u8) {
        let dec = y & 1 == 1;
        let repeat = y >= 6;
        let step = |v: u16| {
            if dec {
                v.wrapping_sub(1)
            } else {
                v.wrapping_add(1)
            }
        };

        match z {
            0 => {
                // LDI/LDD/LDIR/LDDR
                let hl = self.regs.hl();
                let de = self.regs.de();
                let v = self.rd(bus, hl);
                self.wr(bus, de, v);
                self.internal(2);
                self.regs.set_hl(step(hl));
                self.regs.set_de(step(de));
                let bc = self.regs.bc().wrapping_sub(1);
                self.regs.set_bc(bc);
                let n = v.wrapping_add(self.regs.a);
                let mut f = (self.regs.f & (S | Z | C)) | (n & X) | ((n << 4) & Y);
                if bc != 0 {
                    f |= PV;
                }
                self.regs.f = f;
                if repeat && bc != 0 {
                    self.internal(5);
                    self.regs.pc = self.regs.pc.wrapping_sub(2);
                    self.regs.wz = self.regs.pc.wrapping_add(1);
                }
            }
            1 => {
                // CPI/CPD/CPIR/CPDR
                let hl = self.regs.hl();
                let v = self.rd(bus, hl);
                self.internal(5);
                self.regs.set_hl(step(hl));
                let bc = self.regs.bc().wrapping_sub(1);
                self.regs.set_bc(bc);
                let a = self.regs.a;
                let r = a.wrapping_sub(v);
                let h = (a ^ v ^ r) & H;
                let n = r.wrapping_sub(if h != 0 { 1 } else { 0 });
                let mut f = (self.regs.f & C) | N | h | (r & S) | (n & X) | ((n << 4) & Y);
                if r == 0 {
                    f |= Z;
                }
                if bc != 0 {
                    f |= PV;
                }
                self.regs.f = f;
                self.regs.wz = step(self.regs.wz);
                if repeat && bc != 0 && r != 0 {
                    self.internal(5);
                    self.regs.pc = self.regs.pc.wrapping_sub(2);
                    self.regs.wz = self.regs.pc.wrapping_add(1);
                }
            }
            2 => {
                // INI/IND/INIR/INDR
                self.internal(1);
                let bc = self.regs.bc();
                let v = self.io_in(bus, bc);
                let hl = self.regs.hl();
                self.wr(bus, hl, v);
                self.regs.wz = step(bc);
                self.regs.b = self.regs.b.wrapping_sub(1);
                self.regs.set_hl(step(hl));
                let c = if dec {
                    self.regs.c.wrapping_sub(1)
                } else {
                    self.regs.c.wrapping_add(1)
                };
                self.block_io_flags(v, c);
                if repeat && self.regs.b != 0 {
                    self.internal(5);
                    self.regs.pc = self.regs.pc.wrapping_sub(2);
                }
            }
            _ => {
                // OUTI/OUTD/OTIR/OTDR
                self.internal(1);
                let hl = self.regs.hl();
                let v = self.rd(bus, hl);
                self.regs.b = self.regs.b.wrapping_sub(1);
                let bc = self.regs.bc();
                self.io_out(bus, bc, v);
                self.regs.wz = step(bc);
                self.regs.set_hl(step(hl));
                let l = self.regs.l;
                self.block_io_flags(v, l);
                if repeat && self.regs.b != 0 {
                    self.internal(5);
                    self.regs.pc = self.regs.pc.wrapping_sub(2);
                }
            }
        }
    }

    fn block_io_flags(&mut self, v: u8, k: u8) {
        let b = self.regs.b;
        let sum = v as u16 + k as u16;
        let mut f = sz53(b);
        if v & 0x80 != 0 {
            f |= N;
        }
        if sum > 0xFF {
            f |= H | C;
        }
        f |= parity((sum as u8 & 7) ^ b);
        self.regs.f = f;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Ram {
        mem: Vec<u8>,
        io: Vec<(u16, u8)>,
    }

    impl Bus for Ram {
        fn read(&mut self, addr: u16, _t: u64) -> u8 {
            self.mem[addr as usize]
        }
        fn write(&mut self, addr: u16, value: u8, _t: u64) {
            self.mem[addr as usize] = value;
        }
        fn input(&mut self, port: u16, _t: u64) -> u8 {
            port as u8
        }
        fn output(&mut self, port: u16, value: u8, _t: u64) {
            self.io.push((port, value));
        }
    }

    fn run(program: &[u8], steps: usize) -> (Cpu, Ram, u64) {
        let mut ram = Ram {
            mem: vec![0; 0x10000],
            io: vec![],
        };
        ram.mem[..program.len()].copy_from_slice(program);
        let mut cpu = Cpu::new();
        let mut t = 0;
        for _ in 0..steps {
            t += cpu.step(&mut ram) as u64;
        }
        (cpu, ram, t)
    }

    #[test]
    fn ld_add_and_timing() {
        // LD A,5 ; ADD A,3 ; LD (8000h),A
        let (cpu, ram, t) = run(&[0x3E, 5, 0xC6, 3, 0x32, 0x00, 0x80], 3);
        assert_eq!(cpu.regs.a, 8);
        assert_eq!(ram.mem[0x8000], 8);
        assert_eq!(t, 7 + 7 + 13);
    }

    #[test]
    fn indexed_timing() {
        // LD IX,1000h ; LD (IX+5),42h ; INC (IX+5)
        let (_, ram, t) = run(
            &[0xDD, 0x21, 0x00, 0x10, 0xDD, 0x36, 5, 0x42, 0xDD, 0x34, 5],
            3,
        );
        assert_eq!(ram.mem[0x1005], 0x43);
        assert_eq!(t, 14 + 19 + 23);
    }

    #[test]
    fn djnz_loop() {
        // LD B,3 ; loop: DJNZ loop ; HALT
        let (cpu, _, t) = run(&[0x06, 3, 0x10, 0xFE, 0x76], 5);
        assert_eq!(cpu.regs.b, 0);
        assert!(cpu.halted);
        assert_eq!(t, 7 + 13 + 13 + 8 + 4);
    }

    #[test]
    fn ldir_copies() {
        // LD HL,10h ; LD DE,20h ; LD BC,3 ; LDIR
        let mut prog = vec![
            0x21, 0x10, 0x00, 0x11, 0x20, 0x00, 0x01, 0x03, 0x00, 0xED, 0xB0,
        ];
        prog.resize(0x10, 0);
        prog.extend_from_slice(&[1, 2, 3]);
        let (cpu, ram, _) = run(&prog, 6);
        assert_eq!(&ram.mem[0x20..0x23], &[1, 2, 3]);
        assert_eq!(cpu.regs.bc(), 0);
    }

    #[test]
    fn im2_interrupt() {
        struct IntBus {
            mem: Vec<u8>,
        }
        impl Bus for IntBus {
            fn read(&mut self, a: u16, _: u64) -> u8 {
                self.mem[a as usize]
            }
            fn write(&mut self, a: u16, v: u8, _: u64) {
                self.mem[a as usize] = v;
            }
            fn input(&mut self, _: u16, _: u64) -> u8 {
                0
            }
            fn output(&mut self, _: u16, _: u8, _: u64) {}
            fn int_ack(&mut self, _: u64) -> u8 {
                0x10
            }
        }
        let mut bus = IntBus {
            mem: vec![0; 0x10000],
        };
        // LD SP,8000h ; LD A,20h ; LD I,A ; IM 2 ; EI ; NOP ; NOP
        let prog = [
            0x31, 0x00, 0x80, 0x3E, 0x20, 0xED, 0x47, 0xED, 0x5E, 0xFB, 0x00, 0x00,
        ];
        bus.mem[..prog.len()].copy_from_slice(&prog);
        bus.mem[0x2010] = 0x34;
        bus.mem[0x2011] = 0x12;
        let mut cpu = Cpu::new();
        for _ in 0..5 {
            cpu.step(&mut bus);
        }
        cpu.int_line = true;
        // EI delay: one more instruction executes before the interrupt.
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.pc, 0x000B);
        let t = cpu.step(&mut bus);
        assert_eq!(t, 19);
        assert_eq!(cpu.regs.pc, 0x1234);
    }
}
