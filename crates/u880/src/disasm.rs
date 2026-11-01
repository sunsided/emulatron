//! Z80/U880 disassembler (Zilog mnemonics, hex with `h` suffix as printed in
//! GDR documentation).

const R: [&str; 8] = ["B", "C", "D", "E", "H", "L", "(HL)", "A"];
const RP: [&str; 4] = ["BC", "DE", "HL", "SP"];
const RP2: [&str; 4] = ["BC", "DE", "HL", "AF"];
const CC: [&str; 8] = ["NZ", "Z", "NC", "C", "PO", "PE", "P", "M"];
const ALU: [&str; 8] = [
    "ADD A,", "ADC A,", "SUB ", "SBC A,", "AND ", "XOR ", "OR ", "CP ",
];
const ROT: [&str; 8] = ["RLC", "RRC", "RL", "RR", "SLA", "SRA", "SLL", "SRL"];
const BLOCK: [[&str; 4]; 4] = [
    ["LDI", "CPI", "INI", "OUTI"],
    ["LDD", "CPD", "IND", "OUTD"],
    ["LDIR", "CPIR", "INIR", "OTIR"],
    ["LDDR", "CPDR", "INDR", "OTDR"],
];

/// A decoded instruction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instruction {
    pub addr: u16,
    pub len: u8,
    pub text: String,
}

struct Reader<F: Fn(u16) -> u8> {
    read: F,
    start: u16,
    pos: u16,
}

impl<F: Fn(u16) -> u8> Reader<F> {
    fn byte(&mut self) -> u8 {
        let v = (self.read)(self.pos);
        self.pos = self.pos.wrapping_add(1);
        v
    }
    fn word(&mut self) -> u16 {
        let lo = self.byte();
        let hi = self.byte();
        u16::from_le_bytes([lo, hi])
    }
    fn len(&self) -> u8 {
        self.pos.wrapping_sub(self.start) as u8
    }
}

fn hex8(v: u8) -> String {
    if v >= 0xA0 {
        format!("0{v:02X}h")
    } else {
        format!("{v:02X}h")
    }
}

fn hex16(v: u16) -> String {
    if v >= 0xA000 {
        format!("0{v:04X}h")
    } else {
        format!("{v:04X}h")
    }
}

fn disp(d: i8) -> String {
    if d < 0 {
        format!("-{:02X}h", -(d as i16))
    } else {
        format!("+{:02X}h", d)
    }
}

/// Disassemble one instruction at `addr` using `read` to fetch bytes.
pub fn disassemble(read: impl Fn(u16) -> u8, addr: u16) -> Instruction {
    let mut rd = Reader {
        read,
        start: addr,
        pos: addr,
    };
    let mut op = rd.byte();
    let mut ix: Option<&str> = None;
    while op == 0xDD || op == 0xFD {
        ix = Some(if op == 0xDD { "IX" } else { "IY" });
        op = rd.byte();
    }
    let text = match op {
        0xCB => decode_cb(&mut rd, ix),
        0xED => decode_ed(&mut rd),
        _ => decode_main(&mut rd, op, ix),
    };
    Instruction {
        addr,
        len: rd.len(),
        text,
    }
}

fn reg<F: Fn(u16) -> u8>(
    rd: &mut Reader<F>,
    r: u8,
    ix: Option<&str>,
    mem_disp: &mut Option<String>,
) -> String {
    match (r, ix) {
        (6, Some(x)) => {
            let s = mem_disp.get_or_insert_with(|| {
                let d = rd.byte() as i8;
                format!("({x}{})", disp(d))
            });
            s.clone()
        }
        (4, Some(x)) => format!("{x}H"),
        (5, Some(x)) => format!("{x}L"),
        _ => R[r as usize].to_string(),
    }
}

fn decode_main<F: Fn(u16) -> u8>(rd: &mut Reader<F>, op: u8, ix: Option<&str>) -> String {
    let x = op >> 6;
    let y = (op >> 3) & 7;
    let z = op & 7;
    let p = (y >> 1) as usize;
    let q = y & 1;
    let hl = ix.unwrap_or("HL");
    let rp = |p: usize| {
        if p == 2 {
            hl.to_string()
        } else {
            RP[p].to_string()
        }
    };
    let mut md = None;

    match x {
        0 => match z {
            0 => match y {
                0 => "NOP".into(),
                1 => "EX AF,AF'".into(),
                2 | 3 => {
                    let d = rd.byte() as i8;
                    let t = rd.pos.wrapping_add(d as u16);
                    format!("{} {}", if y == 2 { "DJNZ" } else { "JR" }, hex16(t))
                }
                _ => {
                    let d = rd.byte() as i8;
                    let t = rd.pos.wrapping_add(d as u16);
                    format!("JR {},{}", CC[(y - 4) as usize], hex16(t))
                }
            },
            1 => {
                if q == 0 {
                    format!("LD {},{}", rp(p), hex16(rd.word()))
                } else {
                    format!("ADD {hl},{}", rp(p))
                }
            }
            2 => match (q, p) {
                (0, 0) => "LD (BC),A".into(),
                (0, 1) => "LD (DE),A".into(),
                (0, 2) => format!("LD ({}),{hl}", hex16(rd.word())),
                (0, _) => format!("LD ({}),A", hex16(rd.word())),
                (_, 0) => "LD A,(BC)".into(),
                (_, 1) => "LD A,(DE)".into(),
                (_, 2) => format!("LD {hl},({})", hex16(rd.word())),
                _ => format!("LD A,({})", hex16(rd.word())),
            },
            3 => format!("{} {}", if q == 0 { "INC" } else { "DEC" }, rp(p)),
            4 => format!("INC {}", reg(rd, y, ix, &mut md)),
            5 => format!("DEC {}", reg(rd, y, ix, &mut md)),
            6 => {
                let r = reg(rd, y, ix, &mut md);
                format!("LD {r},{}", hex8(rd.byte()))
            }
            _ => ["RLCA", "RRCA", "RLA", "RRA", "DAA", "CPL", "SCF", "CCF"][y as usize].into(),
        },
        1 => {
            if y == 6 && z == 6 {
                "HALT".into()
            } else {
                // With (IX+d) the other operand uses plain H/L.
                let (dst, src) = if y == 6 || z == 6 {
                    let d = reg(rd, y, if y == 6 { ix } else { None }, &mut md);
                    let s = reg(rd, z, if z == 6 { ix } else { None }, &mut md);
                    (d, s)
                } else {
                    (reg(rd, y, ix, &mut md), reg(rd, z, ix, &mut md))
                };
                format!("LD {dst},{src}")
            }
        }
        2 => format!("{}{}", ALU[y as usize], reg(rd, z, ix, &mut md)),
        _ => match z {
            0 => format!("RET {}", CC[y as usize]),
            1 => {
                if q == 0 {
                    let r = if p == 2 {
                        hl.to_string()
                    } else {
                        RP2[p].to_string()
                    };
                    format!("POP {r}")
                } else {
                    match p {
                        0 => "RET".into(),
                        1 => "EXX".into(),
                        2 => format!("JP ({hl})"),
                        _ => format!("LD SP,{hl}"),
                    }
                }
            }
            2 => format!("JP {},{}", CC[y as usize], hex16(rd.word())),
            3 => match y {
                0 => format!("JP {}", hex16(rd.word())),
                2 => format!("OUT ({}),A", hex8(rd.byte())),
                3 => format!("IN A,({})", hex8(rd.byte())),
                4 => format!("EX (SP),{hl}"),
                5 => "EX DE,HL".into(),
                6 => "DI".into(),
                7 => "EI".into(),
                _ => unreachable!(),
            },
            4 => format!("CALL {},{}", CC[y as usize], hex16(rd.word())),
            5 => {
                if q == 0 {
                    let r = if p == 2 {
                        hl.to_string()
                    } else {
                        RP2[p].to_string()
                    };
                    format!("PUSH {r}")
                } else {
                    format!("CALL {}", hex16(rd.word()))
                }
            }
            6 => format!("{}{}", ALU[y as usize], hex8(rd.byte())),
            _ => format!("RST {}", hex8(y * 8)),
        },
    }
}

fn decode_cb<F: Fn(u16) -> u8>(rd: &mut Reader<F>, ix: Option<&str>) -> String {
    let (op, operand) = match ix {
        Some(x) => {
            let d = rd.byte() as i8;
            let op = rd.byte();
            (op, format!("({x}{})", disp(d)))
        }
        None => {
            let op = rd.byte();
            (op, R[(op & 7) as usize].to_string())
        }
    };
    let y = (op >> 3) & 7;
    let z = op & 7;
    let copy = if ix.is_some() && z != 6 {
        format!(",{}", R[z as usize])
    } else {
        String::new()
    };
    match op >> 6 {
        0 => format!("{} {operand}{copy}", ROT[y as usize]),
        1 => format!("BIT {y},{operand}"),
        2 => format!("RES {y},{operand}{copy}"),
        _ => format!("SET {y},{operand}{copy}"),
    }
}

fn decode_ed<F: Fn(u16) -> u8>(rd: &mut Reader<F>) -> String {
    let op = rd.byte();
    let x = op >> 6;
    let y = (op >> 3) & 7;
    let z = op & 7;
    let p = (y >> 1) as usize;
    let q = y & 1;
    match (x, z) {
        (1, 0) => {
            if y == 6 {
                "IN (C)".into()
            } else {
                format!("IN {},(C)", R[y as usize])
            }
        }
        (1, 1) => {
            if y == 6 {
                "OUT (C),0".into()
            } else {
                format!("OUT (C),{}", R[y as usize])
            }
        }
        (1, 2) => format!("{} HL,{}", if q == 0 { "SBC" } else { "ADC" }, RP[p]),
        (1, 3) => {
            let a = hex16(rd.word());
            if q == 0 {
                format!("LD ({a}),{}", RP[p])
            } else {
                format!("LD {},({a})", RP[p])
            }
        }
        (1, 4) => "NEG".into(),
        (1, 5) => if y == 1 { "RETI" } else { "RETN" }.into(),
        (1, 6) => format!("IM {}", [0, 0, 1, 2, 0, 0, 1, 2][y as usize]),
        (1, 7) => [
            "LD I,A", "LD R,A", "LD A,I", "LD A,R", "RRD", "RLD", "NOP*", "NOP*",
        ][y as usize]
            .into(),
        (2, 0..=3) if y >= 4 => BLOCK[(y - 4) as usize][z as usize].into(),
        _ => format!("DB 0EDh,{}", hex8(op)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dis(bytes: &[u8]) -> (String, u8) {
        let b = bytes.to_vec();
        let i = disassemble(|a| *b.get(a as usize).unwrap_or(&0), 0);
        (i.text, i.len)
    }

    #[test]
    fn basics() {
        assert_eq!(dis(&[0x3E, 0x12]), ("LD A,12h".into(), 2));
        assert_eq!(
            dis(&[0xDD, 0x36, 0xFE, 0x42]),
            ("LD (IX-02h),42h".into(), 4)
        );
        assert_eq!(dis(&[0xDD, 0x66, 0x01]), ("LD H,(IX+01h)".into(), 3));
        assert_eq!(dis(&[0xFD, 0xCB, 0x02, 0x46]), ("BIT 0,(IY+02h)".into(), 4));
        assert_eq!(dis(&[0xED, 0xB0]), ("LDIR".into(), 2));
        assert_eq!(dis(&[0xCD, 0x34, 0x12]), ("CALL 1234h".into(), 3));
        assert_eq!(dis(&[0x18, 0xFE]), ("JR 0000h".into(), 2));
        assert_eq!(dis(&[0xD3, 0xF8]), ("OUT (0F8h),A".into(), 2));
    }
}
