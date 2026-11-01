//! LC-80 — "Lerncomputer" by VEB Mikroelektronik "Karl Marx" Erfurt (1984).
//!
//! Hardware (2-ROM version, from the schematics as documented by MAME):
//! * UD880D CPU, ~900 kHz RC clock
//! * 2 KB ROM (2x U505D), 1 KB RAM (2x U214D) at 2000h
//! * 2x UD855D PIO, 1x UD857D CTC
//! * 6-digit VQE23 7-segment display, OUT and HALT LEDs, piezo speaker,
//!   cassette interface, 25-key keypad
//!
//! I/O (A0..A4 decoded):
//! * 0Ch–0Fh CTC (user ports labelled ECh–EFh on the keypad legend)
//! * 14h–17h PIO1 (F4h–F7h): port A = segments (active low), port B =
//!   PB0 tape in, PB1 tape/speaker/OUT LED, PB2–PB7 digit/column select
//!   (active low, PB2 = rightmost digit)
//! * 18h–1Bh PIO2 (F8h–FBh, free for the user): PB4–PB7 keyboard rows
//!
//! Daisy chain: CTC → PIO2 → PIO1.

use serde::{Deserialize, Serialize};
use u880::{Bus, Cpu};

use emulatron_chips::{Ctc, Pio, PortId, daisy};

use crate::display::{DisplayFrame, LedDisplay, bitswap};
use crate::machine::*;
use crate::roms::{RomImage, Variant};
use crate::tape::Tape;
use crate::trace::{BusTrace, CycleKind};

const fn key(
    label: &'static str,
    sub: &'static str,
    color: KeyColor,
    pos: (u8, u8),
    wire: KeyWire,
    chars: &'static str,
) -> KeyDef {
    KeyDef {
        label,
        sub,
        color,
        pos,
        wire,
        chars,
    }
}

use KeyColor::{Black, Cream, Red};
use KeyWire::{Matrix as M, Nmi, Reset};

/// Keypad as on the real device: 5x5, labels printed above the keys.
pub static KEYS: [KeyDef; 25] = [
    key("RES", "", Red, (0, 0), Reset, ""),
    key("ADR", "", Black, (0, 1), M(5, 3), ","),
    key("DAT", "", Black, (0, 2), M(5, 2), "."),
    key("+", "", Black, (0, 3), M(2, 1), "+="),
    key("-", "", Black, (0, 4), M(5, 0), "-"),
    key("NMI", "", Red, (1, 0), Nmi, ""),
    key("C", "", Cream, (1, 1), M(4, 3), "C"),
    key("D", "", Cream, (1, 2), M(4, 2), "D"),
    key("E", "", Cream, (1, 3), M(3, 1), "E"),
    key("F", "", Cream, (1, 4), M(4, 0), "F"),
    key("ST", "", Black, (2, 0), M(0, 2), "S"),
    key("8", "", Cream, (2, 1), M(3, 3), "8"),
    key("9", "", Cream, (2, 2), M(3, 2), "9"),
    key("A", "", Cream, (2, 3), M(4, 1), "A"),
    key("B", "", Cream, (2, 4), M(3, 0), "B"),
    key("LD", "", Black, (3, 0), M(0, 1), "L"),
    key("4", "", Cream, (3, 1), M(2, 3), "4"),
    key("5", "", Cream, (3, 2), M(2, 2), "5"),
    key("6", "", Cream, (3, 3), M(5, 1), "6"),
    key("7", "", Cream, (3, 4), M(2, 0), "7"),
    key("EX", "", Black, (4, 0), M(0, 3), "X\r"),
    key("0", "", Cream, (4, 1), M(1, 3), "0"),
    key("1", "", Cream, (4, 2), M(1, 2), "1"),
    key("2", "", Cream, (4, 3), M(1, 1), "2"),
    key("3", "", Cream, (4, 4), M(1, 0), "3"),
];

pub static INFO: MachineInfo = MachineInfo {
    family: "lc80",
    title: "LC-80",
    digits: 6,
    digit_groups: &[
        DigitGroup {
            label: "ADRESSE",
            first: 0,
            count: 4,
        },
        DigitGroup {
            label: "DATEN",
            first: 4,
            count: 2,
        },
    ],
    keys: &KEYS,
    keypad_rows: 5,
    keypad_cols: 5,
    leds: &[("OUT", KeyColor::Teal), ("HALT", KeyColor::Red)],
    bus_leds: false,
};

/// Address decoding of the different board revisions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Layout {
    /// 2x 1 KB ROM at 0000h and 0800h (each mirrored once), 1 KB RAM.
    TwoRoms,
    /// 2 KB EPROM at 0000h.
    Eprom2k,
    /// LC-80.2: 4 KB ROM, 4 KB RAM.
    Lc80_2,
    /// Export version: 8 KB ROM + 4 KB at 4000h (C000h), A15 decoded.
    Export,
}

#[derive(Serialize, Deserialize)]
struct Board {
    #[serde(skip)]
    rom: Vec<u8>,
    layout: Layout,
    ram: Vec<u8>,
    pio1: Pio,
    pio2: Pio,
    ctc: Ctc,
    /// Pressed keys per select column (bit n = row n).
    keys: [u8; 6],
    display: LedDisplay,
    out_led: LedDisplay,
    tape: Tape,
    #[serde(skip)]
    trace: BusTrace,
    /// Bus cycles per chip since the last `chip_activity` call:
    /// D202, D203, D204/205 (RAM), D206 (PIO1), D207 (PIO2), D208 (CTC), total.
    #[serde(skip)]
    activity: [u32; 7],
}

impl Board {
    fn decode(&self, addr: u16) -> (Region, usize) {
        match self.layout {
            Layout::TwoRoms => {
                let a = addr & 0x3FFF;
                match a {
                    0x0000..=0x07FF => (Region::Rom, (a & 0x03FF) as usize),
                    0x0800..=0x0FFF => (Region::Rom, (0x0800 | (a & 0x03FF)) as usize),
                    _ => self.decode_ram(a, 0x1000),
                }
            }
            Layout::Eprom2k => {
                let a = addr & 0x3FFF;
                if a < 0x0800 {
                    (Region::Rom, a as usize)
                } else {
                    self.decode_ram(a, 0x1000)
                }
            }
            Layout::Lc80_2 => {
                let a = addr & 0x3FFF;
                if a < 0x1000 {
                    (Region::Rom, a as usize)
                } else {
                    self.decode_ram(a, 0x1000)
                }
            }
            Layout::Export => {
                let a = addr & 0x7FFF;
                match a {
                    0x0000..=0x1FFF | 0x4000..=0x4FFF => (Region::Rom, a as usize),
                    _ => self.decode_ram(a & 0x3FFF, 0x1000),
                }
            }
        }
    }

    /// RAM sits at 2000h and is mirrored at 3000h.
    fn decode_ram(&self, a: u16, mirror: u16) -> (Region, usize) {
        let a = a & !mirror;
        let off = a.wrapping_sub(0x2000) as usize;
        if a >= 0x2000 && off < self.ram.len() {
            (Region::Ram, off)
        } else {
            (Region::Unmapped, 0)
        }
    }

    fn count_mem(&mut self, addr: u16) {
        self.activity[6] += 1;
        match self.decode(addr) {
            (Region::Rom, o) => self.activity[if o < 0x800 { 0 } else { 1 }] += 1,
            (Region::Ram, _) => self.activity[2] += 1,
            _ => {}
        }
    }

    fn count_io(&mut self, port: u8) {
        self.activity[6] += 1;
        match Self::io_target(port) {
            Some((0, _)) => self.activity[5] += 1,
            Some((1, _)) => self.activity[3] += 1,
            Some(_) => self.activity[4] += 1,
            None => {}
        }
    }

    fn peek(&self, addr: u16) -> u8 {
        match self.decode(addr) {
            (Region::Rom, o) => self.rom[o],
            (Region::Ram, o) => self.ram[o],
            _ => 0xFF,
        }
    }

    /// Recompute everything that depends on PIO outputs.
    fn update_outputs(&mut self, t: u64) {
        let pa = self.pio1.pins(PortId::A);
        let pb = self.pio1.pins(PortId::B);
        // PA: B F A G DP C E D, active low.
        let segs = bitswap(!pa, [2, 0, 5, 7, 6, 1, 3, 4]);
        // PB2 = rightmost digit .. PB7 = leftmost, active low.
        let cols = (!pb >> 2) & 0x3F;
        let select = (0..6).fold(0u16, |m, i| {
            if cols & (1 << i) != 0 {
                m | (1 << (5 - i))
            } else {
                m
            }
        });
        self.display.drive(t, select, segs);
        self.out_led.drive(t, if pb & 0x02 == 0 { 1 } else { 0 }, 1);
        self.tape.output(t, pb & 0x02 != 0);
        self.update_keys();
    }

    fn update_keys(&mut self) {
        let cols = (!self.pio1.pins(PortId::B) >> 2) & 0x3F;
        let mut rows = 0u8;
        for (i, k) in self.keys.iter().enumerate() {
            if cols & (1 << i) != 0 {
                rows |= k;
            }
        }
        self.pio2.set_input(PortId::B, (rows << 4) ^ 0xF0);
    }

    fn io_target(port: u8) -> Option<(u8, usize)> {
        match port & 0x1F {
            0x0C..=0x0F => Some((0, (port & 3) as usize)),
            0x14..=0x17 => Some((1, (port & 3) as usize)),
            0x18..=0x1B => Some((2, (port & 3) as usize)),
            _ => None,
        }
    }

    fn pio_mut(&mut self, n: u8) -> &mut Pio {
        if n == 1 {
            &mut self.pio1
        } else {
            &mut self.pio2
        }
    }
}

/// Standard PIO addressing: A0 = port B/A, A1 = control/data.
fn pio_reg(r: usize) -> (PortId, bool) {
    (if r & 1 == 0 { PortId::A } else { PortId::B }, r & 2 != 0)
}

impl Bus for Board {
    fn fetch(&mut self, addr: u16, t: u64) -> u8 {
        self.count_mem(addr);
        let v = self.peek(addr);
        self.trace.record(t, CycleKind::Fetch, addr, v);
        v
    }

    fn read(&mut self, addr: u16, t: u64) -> u8 {
        self.count_mem(addr);
        let v = self.peek(addr);
        self.trace.record(t, CycleKind::MemRead, addr, v);
        v
    }

    fn write(&mut self, addr: u16, value: u8, t: u64) {
        self.count_mem(addr);
        if let (Region::Ram, o) = self.decode(addr) {
            self.ram[o] = value;
        }
        self.trace.record(t, CycleKind::MemWrite, addr, value);
    }

    fn input(&mut self, port: u16, t: u64) -> u8 {
        self.count_io(port as u8);
        let tape = self.tape.input(t) as u8;
        self.pio1.set_input(PortId::B, tape);
        let v = match Self::io_target(port as u8) {
            Some((0, ch)) => self.ctc.read(ch),
            Some((n, r)) => {
                let (id, ctl) = pio_reg(r);
                self.pio_mut(n).read(id, ctl)
            }
            None => 0xFF,
        };
        self.trace.record(t, CycleKind::IoRead, port, v);
        v
    }

    fn output(&mut self, port: u16, value: u8, t: u64) {
        self.count_io(port as u8);
        self.trace.record(t, CycleKind::IoWrite, port, value);
        match Self::io_target(port as u8) {
            Some((0, ch)) => self.ctc.write(ch, value),
            Some((n, r)) => {
                let (id, ctl) = pio_reg(r);
                self.pio_mut(n).write(id, ctl, value);
                if n == 1 {
                    self.update_outputs(t + 3);
                }
            }
            None => {}
        }
    }

    fn int_ack(&mut self, t: u64) -> u8 {
        let mut chain: [&mut dyn daisy::DaisyDevice; 3] =
            [&mut self.ctc, &mut self.pio2, &mut self.pio1];
        let v = daisy::ack(&mut chain).map(|(_, v)| v).unwrap_or(0xFF);
        self.trace.record(t, CycleKind::IntAck, 0, v);
        v
    }

    fn reti(&mut self) {
        let mut chain: [&mut dyn daisy::DaisyDevice; 3] =
            [&mut self.ctc, &mut self.pio2, &mut self.pio1];
        daisy::reti(&mut chain);
    }

    fn halt(&mut self, halted: bool) {
        self.trace.halt = halted;
    }
}

pub struct Lc80 {
    variant: &'static Variant,
    cpu: Cpu,
    board: Board,
    pressed: [bool; 25],
}

#[derive(Serialize, Deserialize)]
struct SaveState {
    machine: String,
    cpu: Cpu,
    board: Board,
    pressed: Vec<bool>,
}

impl Lc80 {
    pub fn new(variant: &'static Variant, rom: RomImage) -> Self {
        let layout = match variant.id {
            "lc80a" => Layout::Eprom2k,
            "lc80_2" => Layout::Lc80_2,
            "lc80e" => Layout::Export,
            _ => Layout::TwoRoms,
        };
        let board = Board {
            rom: rom.data,
            layout,
            ram: vec![0; variant.ram_size],
            pio1: Pio::new(),
            pio2: Pio::new(),
            ctc: Ctc::new(),
            keys: [0; 6],
            display: LedDisplay::new(6),
            out_led: LedDisplay::new(1),
            tape: Tape::default(),
            trace: BusTrace::default(),
            activity: [0; 7],
        };
        let mut m = Lc80 {
            variant,
            cpu: Cpu::new(),
            board,
            pressed: [false; 25],
        };
        m.reset();
        m
    }

    fn reset_held(&self) -> bool {
        self.pressed[0]
    }
}

impl Machine for Lc80 {
    fn info(&self) -> &'static MachineInfo {
        &INFO
    }

    fn variant(&self) -> &'static Variant {
        self.variant
    }

    fn reset(&mut self) {
        self.cpu.reset();
        self.board.pio1.reset();
        self.board.pio2.reset();
        self.board.ctc.reset();
        self.board.trace.halt = false;
        let t = self.cpu.t;
        self.board.update_outputs(t);
    }

    fn power_cycle(&mut self) {
        self.board.ram.fill(0);
        self.reset();
    }

    fn step(&mut self) -> u32 {
        if self.reset_held() {
            // /RESET held low: the CPU idles, the bus floats.
            self.cpu.t += 4;
            return 4;
        }
        let b = &mut self.board;
        b.trace.int = self.cpu.int_line;
        b.trace.nmi = self.cpu.nmi_line();
        let n = self.cpu.step(b);
        // ZC/TO outputs go to the user bus only.
        b.ctc.tick(n);
        self.cpu.int_line = daisy::int_line(&[&b.ctc, &b.pio2, &b.pio1]);
        n
    }

    fn cpu(&self) -> &Cpu {
        &self.cpu
    }

    fn cpu_mut(&mut self) -> &mut Cpu {
        &mut self.cpu
    }

    fn peek(&self, addr: u16) -> u8 {
        self.board.peek(addr)
    }

    fn poke(&mut self, addr: u16, value: u8) -> bool {
        if let (Region::Ram, o) = self.board.decode(addr) {
            self.board.ram[o] = value;
            true
        } else {
            false
        }
    }

    fn region(&self, addr: u16) -> Region {
        self.board.decode(addr).0
    }

    fn port_name(&self, port: u8) -> &'static str {
        const PIO1: [&str; 4] = ["PIO1 A data", "PIO1 B data", "PIO1 A ctrl", "PIO1 B ctrl"];
        const PIO2: [&str; 4] = ["PIO2 A data", "PIO2 B data", "PIO2 A ctrl", "PIO2 B ctrl"];
        const CTC: [&str; 4] = ["CTC ch0", "CTC ch1", "CTC ch2", "CTC ch3"];
        match Board::io_target(port) {
            Some((0, r)) => CTC[r],
            Some((1, r)) => PIO1[r],
            Some((_, r)) => PIO2[r],
            None => "-",
        }
    }

    fn set_key(&mut self, key: KeyId, down: bool) {
        let Some(def) = KEYS.get(key) else { return };
        if self.pressed[key] == down {
            return;
        }
        self.pressed[key] = down;
        match def.wire {
            KeyWire::Matrix(col, row) => {
                let k = &mut self.board.keys[col as usize];
                if down {
                    *k |= 1 << row
                } else {
                    *k &= !(1 << row)
                }
                self.board.update_keys();
            }
            KeyWire::Reset => {
                if down {
                    self.reset();
                }
            }
            KeyWire::Nmi => self.cpu.set_nmi(down),
            KeyWire::None => {}
        }
    }

    fn key_down(&self, key: KeyId) -> bool {
        self.pressed.get(key).copied().unwrap_or(false)
    }

    fn display_frame(&mut self) -> DisplayFrame {
        self.board.display.frame(self.cpu.t)
    }

    fn leds(&mut self) -> Vec<f32> {
        let out = self.board.out_led.frame(self.cpu.t).digits[0][0];
        vec![out, if self.cpu.halted { 1.0 } else { 0.0 }]
    }

    fn trace(&self) -> &BusTrace {
        &self.board.trace
    }

    fn trace_mut(&mut self) -> &mut BusTrace {
        &mut self.board.trace
    }

    fn chips(&self) -> Chips<'_> {
        Chips {
            pios: vec![
                ("PIO1", &self.board.pio1, 0xF4),
                ("PIO2", &self.board.pio2, 0xF8),
            ],
            ctc: ("CTC", &self.board.ctc, 0xEC),
            daisy: &["CTC", "PIO2", "PIO1"],
        }
    }

    fn chip_activity(&mut self) -> Vec<(&'static str, f32)> {
        let a = std::mem::take(&mut self.board.activity);
        let total = a[6].max(1) as f32;
        let mut v = vec![("D201", if a[6] > 0 { 1.0 } else { 0.0 })];
        for (name, n) in ["D202", "D203", "RAM", "D206", "D207", "D208"]
            .iter()
            .zip(a)
        {
            v.push((name, n as f32 / total));
        }
        v
    }

    fn tape(&mut self) -> &mut Tape {
        &mut self.board.tape
    }

    fn save_state(&self) -> String {
        #[derive(Serialize)]
        struct Ref<'a> {
            machine: &'a str,
            cpu: &'a Cpu,
            board: &'a Board,
            pressed: &'a [bool],
        }
        serde_json::to_string(&Ref {
            machine: self.variant.id,
            cpu: &self.cpu,
            board: &self.board,
            pressed: &self.pressed,
        })
        .expect("state serialises")
    }

    fn load_state(&mut self, json: &str) -> Result<(), String> {
        crate::machine::check_state_machine(json, self.variant.id)?;
        let s: SaveState = serde_json::from_str(json).map_err(|e| e.to_string())?;
        if s.board.ram.len() != self.board.ram.len() {
            return Err("RAM size mismatch".into());
        }
        let rom = std::mem::take(&mut self.board.rom);
        self.cpu = s.cpu;
        self.board = s.board;
        self.board.rom = rom;
        for (i, p) in s.pressed.iter().enumerate().take(self.pressed.len()) {
            self.pressed[i] = *p;
        }
        let t = self.cpu.t;
        self.board.display.resync(t);
        self.board.out_led.resync(t);
        Ok(())
    }
}
