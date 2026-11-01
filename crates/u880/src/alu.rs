//! Flag-exact ALU helpers. All functions are pure: they take operands and the
//! current flags and return the result plus the new flags.

use crate::flags::*;

#[inline]
pub fn parity(v: u8) -> u8 {
    if v.count_ones() & 1 == 0 { PV } else { 0 }
}

/// S, Z and the undocumented X/Y bits from a result byte.
#[inline]
pub fn sz53(v: u8) -> u8 {
    (v & (S | Y | X)) | if v == 0 { Z } else { 0 }
}

#[inline]
pub fn sz53p(v: u8) -> u8 {
    sz53(v) | parity(v)
}

pub fn add8(a: u8, v: u8, carry: bool) -> (u8, u8) {
    let c = carry as u16;
    let r16 = a as u16 + v as u16 + c;
    let r = r16 as u8;
    let mut f = sz53(r) | ((a ^ v ^ r) & H);
    if (a ^ !v) & (a ^ r) & 0x80 != 0 {
        f |= PV;
    }
    if r16 > 0xFF {
        f |= C;
    }
    (r, f)
}

pub fn sub8(a: u8, v: u8, carry: bool) -> (u8, u8) {
    let c = carry as i16;
    let r16 = a as i16 - v as i16 - c;
    let r = r16 as u8;
    let mut f = sz53(r) | N | ((a ^ v ^ r) & H);
    if (a ^ v) & (a ^ r) & 0x80 != 0 {
        f |= PV;
    }
    if r16 < 0 {
        f |= C;
    }
    (r, f)
}

/// CP: like SUB but X/Y come from the operand and A is unchanged.
pub fn cp8(a: u8, v: u8) -> u8 {
    let (_, f) = sub8(a, v, false);
    (f & !(X | Y)) | (v & (X | Y))
}

pub fn and8(a: u8, v: u8) -> (u8, u8) {
    let r = a & v;
    (r, sz53p(r) | H)
}

pub fn xor8(a: u8, v: u8) -> (u8, u8) {
    let r = a ^ v;
    (r, sz53p(r))
}

pub fn or8(a: u8, v: u8) -> (u8, u8) {
    let r = a | v;
    (r, sz53p(r))
}

/// Dispatch for the eight ALU operations encoded in bits 3..5 of the opcode.
pub fn alu(op: u8, a: u8, v: u8, f: u8) -> (u8, u8) {
    match op & 7 {
        0 => add8(a, v, false),
        1 => add8(a, v, f & C != 0),
        2 => sub8(a, v, false),
        3 => sub8(a, v, f & C != 0),
        4 => and8(a, v),
        5 => xor8(a, v),
        6 => or8(a, v),
        _ => (a, cp8(a, v)),
    }
}

pub fn inc8(v: u8, f: u8) -> (u8, u8) {
    let r = v.wrapping_add(1);
    let mut nf = (f & C) | sz53(r);
    if v & 0x0F == 0x0F {
        nf |= H;
    }
    if v == 0x7F {
        nf |= PV;
    }
    (r, nf)
}

pub fn dec8(v: u8, f: u8) -> (u8, u8) {
    let r = v.wrapping_sub(1);
    let mut nf = (f & C) | sz53(r) | N;
    if v & 0x0F == 0 {
        nf |= H;
    }
    if v == 0x80 {
        nf |= PV;
    }
    (r, nf)
}

/// ADD HL,rr: S, Z, PV preserved.
pub fn add16(a: u16, b: u16, f: u8) -> (u16, u8) {
    let r32 = a as u32 + b as u32;
    let r = r32 as u16;
    let mut nf = (f & (S | Z | PV)) | (((a ^ b ^ r) >> 8) as u8 & H) | ((r >> 8) as u8 & (X | Y));
    if r32 > 0xFFFF {
        nf |= C;
    }
    (r, nf)
}

pub fn adc16(a: u16, b: u16, f: u8) -> (u16, u8) {
    let c = (f & C) as u32;
    let r32 = a as u32 + b as u32 + c;
    let r = r32 as u16;
    let hi = (r >> 8) as u8;
    let mut nf = (hi & (S | X | Y)) | (((a ^ b ^ r) >> 8) as u8 & H);
    if r == 0 {
        nf |= Z;
    }
    if (a ^ !b) & (a ^ r) & 0x8000 != 0 {
        nf |= PV;
    }
    if r32 > 0xFFFF {
        nf |= C;
    }
    (r, nf)
}

pub fn sbc16(a: u16, b: u16, f: u8) -> (u16, u8) {
    let c = (f & C) as i32;
    let r32 = a as i32 - b as i32 - c;
    let r = r32 as u16;
    let hi = (r >> 8) as u8;
    let mut nf = (hi & (S | X | Y)) | N | (((a ^ b ^ r) >> 8) as u8 & H);
    if r == 0 {
        nf |= Z;
    }
    if (a ^ b) & (a ^ r) & 0x8000 != 0 {
        nf |= PV;
    }
    if r32 < 0 {
        nf |= C;
    }
    (r, nf)
}

/// CB-prefixed rotate/shift group (op = bits 3..5 of the CB opcode).
pub fn rot(op: u8, v: u8, f: u8) -> (u8, u8) {
    let cin = f & C;
    let (r, cout) = match op & 7 {
        0 => (v.rotate_left(1), v >> 7),     // RLC
        1 => (v.rotate_right(1), v & 1),     // RRC
        2 => ((v << 1) | cin, v >> 7),       // RL
        3 => ((v >> 1) | (cin << 7), v & 1), // RR
        4 => (v << 1, v >> 7),               // SLA
        5 => ((v >> 1) | (v & 0x80), v & 1), // SRA
        6 => ((v << 1) | 1, v >> 7),         // SLL (undocumented)
        _ => (v >> 1, v & 1),                // SRL
    };
    (r, sz53p(r) | cout)
}

/// BIT b,v. `xy` supplies the source of the undocumented X/Y flags.
pub fn bit(b: u8, v: u8, f: u8, xy: u8) -> u8 {
    let set = v & (1 << b);
    let mut nf = (f & C) | H | (xy & (X | Y));
    if set == 0 {
        nf |= Z | PV;
    }
    if b == 7 && set != 0 {
        nf |= S;
    }
    nf
}

pub fn daa(a: u8, f: u8) -> (u8, u8) {
    let mut diff = 0u8;
    let mut carry = f & C;
    let lo = a & 0x0F;
    if f & H != 0 || lo > 9 {
        diff |= 0x06;
    }
    if carry != 0 || a > 0x99 {
        diff |= 0x60;
        carry = C;
    }
    let neg = f & N != 0;
    let r = if neg {
        a.wrapping_sub(diff)
    } else {
        a.wrapping_add(diff)
    };
    let h = if neg {
        if f & H != 0 && lo < 6 { H } else { 0 }
    } else if lo > 9 {
        H
    } else {
        0
    };
    (r, sz53p(r) | (f & N) | carry | h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_overflow() {
        let (r, f) = add8(0x7F, 1, false);
        assert_eq!(r, 0x80);
        assert_ne!(f & PV, 0);
        assert_ne!(f & H, 0);
        assert_ne!(f & S, 0);
    }

    #[test]
    fn sub_borrow() {
        let (r, f) = sub8(0x00, 1, false);
        assert_eq!(r, 0xFF);
        assert_ne!(f & C, 0);
        assert_ne!(f & N, 0);
    }

    #[test]
    fn daa_after_add() {
        let (r, f) = add8(0x15, 0x27, false);
        let (r, _) = daa(r, f);
        assert_eq!(r, 0x42);
    }
}
