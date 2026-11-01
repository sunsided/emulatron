//! Poly-Computer 880 — VEB Polytechnik Karl-Marx-Stadt (1983).
//!
//! * U880D CPU at 7.3728 MHz / 8 = 921.6 kHz
//! * 4 ROM sockets of 1 KB at 0000h/1000h/2000h/3000h (each mirrored 4x),
//!   two populated (BM039, BM040); 1 KB RAM at 4000h mirrored to 7FFFh
//! * 2x U855 PIO, 1x U857 CTC
//! * 8-digit VQE23 display, 34-LED bus analyser, 27-key keypad
//!
//! I/O (address mask AFh):
//! * 80h–83h PIO1, 84h–87h PIO2 — "alternate" addressing: A0 = control/data,
//!   A1 = port B/A
//! * 88h–8Bh CTC
//! * A0h–AFh (write) CLDIG: digit select latch, also the keyboard columns
//!
//! PIO1 port A drives the segments; port B: PB1 tape in, PB2 tape out,
//! PB4/PB5/PB7 keyboard rows, PB6 SCON (releases the NMI latch).
//! CTC ZC/TO0 sets the NMI latch (used for booting and single-stepping),
//! ZC/TO2 clocks channel 3.
//!
//! Daisy chain: PIO1 → PIO2 → CTC.

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

use KeyColor::{Grey, Orange, Red, Teal};
use KeyWire::{Matrix as M, Nmi, Reset};

/// Index of the MCYCL / CYCL keys (handled by the front-end's bus stepper).
pub const KEY_MCYCL: KeyId = 25;
pub const KEY_CYCL: KeyId = 26;

/// Keypad as on the real panel: 4x4 hex block (orange, register names on
/// the keys), 2-column command block (grey, EXEC red), logic key column.
pub static KEYS: [KeyDef; 27] = [
    key("C", "PC", Orange, (0, 0), M(4, 2), "C"),
    key("D", "", Orange, (0, 1), M(5, 2), "D"),
    key("E", "", Orange, (0, 2), M(7, 2), "E"),
    key("F", "", Orange, (0, 3), M(6, 2), "F"),
    key("8", "IE", Orange, (1, 0), M(4, 1), "8"),
    key("9", "IX", Orange, (1, 1), M(5, 1), "9"),
    key("A", "IY", Orange, (1, 2), M(7, 1), "A"),
    key("B", "SP", Orange, (1, 3), M(6, 1), "B"),
    key("4", "AF'", Orange, (2, 0), M(0, 2), "4"),
    key("5", "BC'", Orange, (2, 1), M(3, 2), "5"),
    key("6", "DE'", Orange, (2, 2), M(1, 2), "6"),
    key("7", "HL'", Orange, (2, 3), M(2, 2), "7"),
    key("0", "AF", Orange, (3, 0), M(0, 1), "0"),
    key("1", "BC", Orange, (3, 1), M(3, 1), "1"),
    key("2", "DE", Orange, (3, 2), M(1, 1), "2"),
    key("3", "HL", Orange, (3, 3), M(2, 1), "3"),
    key("GO", "", Grey, (0, 5), M(0, 0), "G"),
    key("MEM", "", Grey, (0, 6), M(7, 0), "M"),
    key("STEP", "", Grey, (1, 5), M(6, 0), "S"),
    key("REG", "", Grey, (1, 6), M(4, 0), "R"),
    key("FCT", "", Grey, (2, 5), M(5, 0), "T"),
    key("BACK", "", Grey, (2, 6), M(3, 0), "K\x08"),
    key("EXEC", "", Red, (3, 5), M(2, 0), "X\r"),
    key("RES", "", Red, (0, 8), Reset, ""),
    key("MON", "", Orange, (1, 8), Nmi, ""),
    key("M CYCL", "", Teal, (2, 8), KeyWire::None, ""),
    key("CYCL", "", Teal, (3, 8), KeyWire::None, ""),
];

pub static INFO: MachineInfo = MachineInfo {
    family: "poly880",
    title: "POLY-COMPUTER 880",
    digits: 8,
    digit_groups: &[
        DigitGroup {
            label: "BETR.-ART",
            first: 0,
            count: 2,
        },
        DigitGroup {
            label: "REGISTER / ADRESSE",
            first: 2,
            count: 4,
        },
        DigitGroup {
            label: "INHALT / DATEN",
            first: 6,
            count: 2,
        },
    ],
    keys: &KEYS,
    keypad_rows: 4,
    keypad_cols: 9,
    leds: &[("MCYCL", KeyColor::Red)],
    bus_leds: true,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Layout {
    /// 4 sockets x 1 KB, each mirrored within its 4 KB slot.
    Sockets,
    /// SC1 hack: 16 KB linear ROM.
    Linear,
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
    /// CLDIG latch: digit select and keyboard column drive.
    cldig: u8,
    /// Pressed keys per return line (bit n = column n).
    keys: [u8; 3],
    /// NMI flip-flop set by CTC ZC/TO0, cleared by PIO1 PB6 (SCON).
    nmi_latch: bool,
    display: LedDisplay,
    tape: Tape,
    #[serde(skip)]
    trace: BusTrace,
    /// Bus cycles per block since the last `chip_activity` call:
    /// ROM, RAM, PIO1, PIO2, CTC, total.
    #[serde(skip)]
    activity: [u32; 6],
}

impl Board {
    fn decode(&self, addr: u16) -> (Region, usize) {
        match addr {
            0x4000..=0x7FFF => (Region::Ram, (addr & 0x03FF) as usize),
            0x0000..=0x3FFF => match self.layout {
                Layout::Sockets => (Region::Rom, (addr & 0x33FF) as usize),
                Layout::Linear => (Region::Rom, addr as usize),
            },
            _ => (Region::Unmapped, 0),
        }
    }

    fn count_mem(&mut self, addr: u16) {
        self.activity[5] += 1;
        match self.decode(addr).0 {
            Region::Rom => self.activity[0] += 1,
            Region::Ram => self.activity[1] += 1,
            Region::Unmapped => {}
        }
    }

    fn count_io(&mut self, port: u8) {
        self.activity[5] += 1;
        match Self::io_target(port) {
            Some((1, _)) => self.activity[2] += 1,
            Some((2, _)) => self.activity[3] += 1,
            Some((0, _)) => self.activity[4] += 1,
            _ => {}
        }
    }

    fn peek(&self, addr: u16) -> u8 {
        match self.decode(addr) {
            (Region::Rom, o) => self.rom[o],
            (Region::Ram, o) => self.ram[o],
            _ => 0xFF,
        }
    }

    fn update_display(&mut self, t: u64) {
        // PA: E D C DP G A F B, active high.
        let segs = bitswap(self.pio1.pins(PortId::A), [5, 7, 2, 1, 0, 6, 4, 3]);
        // CLDIG bit 0 = rightmost digit.
        let select = (0..8).fold(0u16, |m, i| {
            if self.cldig & (1 << i) != 0 {
                m | (1 << (7 - i))
            } else {
                m
            }
        });
        self.display.drive(t, select, segs);
    }

    fn update_inputs(&mut self, t: u64) {
        let mut v = 0x4C | ((self.tape.input(t) as u8) << 1);
        if self.keys[0] & self.cldig != 0 {
            v |= 0x10;
        }
        if self.keys[1] & self.cldig != 0 {
            v |= 0x20;
        }
        if self.keys[2] & self.cldig != 0 {
            v |= 0x80;
        }
        self.pio1.set_input(PortId::B, v);
    }

    fn pio1_b_changed(&mut self, t: u64) {
        let pb = self.pio1.pins(PortId::B);
        self.tape.output(t, pb & 0x04 != 0);
        if self.nmi_latch && pb & 0x40 != 0 {
            self.nmi_latch = false;
        }
    }

    fn io_target(port: u8) -> Option<(u8, usize)> {
        let p = port & 0xAF;
        match p {
            0x80..=0x83 => Some((1, (p & 3) as usize)),
            0x84..=0x87 => Some((2, (p & 3) as usize)),
            0x88..=0x8B => Some((0, (p & 3) as usize)),
            0xA0..=0xAF => Some((3, 0)),
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

/// Alternate PIO addressing: A0 = control/data, A1 = port B/A.
fn pio_reg(r: usize) -> (PortId, bool) {
    (if r & 2 == 0 { PortId::A } else { PortId::B }, r & 1 != 0)
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
        self.update_inputs(t);
        let v = match Self::io_target(port as u8) {
            Some((0, ch)) => self.ctc.read(ch),
            Some((3, _)) | None => 0xFF,
            Some((n, r)) => {
                let (id, ctl) = pio_reg(r);
                self.pio_mut(n).read(id, ctl)
            }
        };
        self.trace.record(t, CycleKind::IoRead, port, v);
        v
    }

    fn output(&mut self, port: u16, value: u8, t: u64) {
        self.count_io(port as u8);
        self.trace.record(t, CycleKind::IoWrite, port, value);
        match Self::io_target(port as u8) {
            Some((0, ch)) => self.ctc.write(ch, value),
            Some((3, _)) => {
                self.cldig = value;
                self.update_display(t + 3);
                self.update_inputs(t);
            }
            Some((n, r)) => {
                let (id, ctl) = pio_reg(r);
                self.pio_mut(n).write(id, ctl, value);
                if n == 1 {
                    self.update_display(t + 3);
                    self.pio1_b_changed(t + 3);
                }
            }
            None => {}
        }
    }

    fn int_ack(&mut self, t: u64) -> u8 {
        let mut chain: [&mut dyn daisy::DaisyDevice; 3] =
            [&mut self.pio1, &mut self.pio2, &mut self.ctc];
        let v = daisy::ack(&mut chain).map(|(_, v)| v).unwrap_or(0xFF);
        self.trace.record(t, CycleKind::IntAck, 0, v);
        v
    }

    fn reti(&mut self) {
        let mut chain: [&mut dyn daisy::DaisyDevice; 3] =
            [&mut self.pio1, &mut self.pio2, &mut self.ctc];
        daisy::reti(&mut chain);
    }

    fn halt(&mut self, halted: bool) {
        self.trace.halt = halted;
    }
}

pub struct Poly880 {
    variant: &'static Variant,
    cpu: Cpu,
    board: Board,
    pressed: [bool; 27],
}

#[derive(Serialize, Deserialize)]
struct SaveState {
    machine: String,
    cpu: Cpu,
    board: Board,
    pressed: Vec<bool>,
}

impl Poly880 {
    pub fn new(variant: &'static Variant, rom: RomImage) -> Self {
        let layout = if variant.id == "poly880s" {
            Layout::Linear
        } else {
            Layout::Sockets
        };
        let board = Board {
            rom: rom.data,
            layout,
            ram: vec![0; variant.ram_size],
            pio1: Pio::new(),
            pio2: Pio::new(),
            ctc: Ctc::new(),
            cldig: 0,
            keys: [0; 3],
            nmi_latch: false,
            display: LedDisplay::new(8),
            tape: Tape::default(),
            trace: BusTrace::default(),
            activity: [0; 6],
        };
        let mut m = Poly880 {
            variant,
            cpu: Cpu::new(),
            board,
            pressed: [false; 27],
        };
        m.reset();
        m
    }

    fn nmi_level(&self) -> bool {
        self.board.nmi_latch || self.pressed[24]
    }
}

impl Machine for Poly880 {
    fn info(&self) -> &'static MachineInfo {
        &INFO
    }

    fn variant(&self) -> &'static Variant {
        self.variant
    }

    fn reset(&mut self) {
        self.cpu.reset();
        let b = &mut self.board;
        b.pio1.reset();
        b.pio2.reset();
        b.ctc.reset();
        b.nmi_latch = false;
        b.trace.halt = false;
        let t = self.cpu.t;
        b.update_display(t);
        b.update_inputs(t);
        let nmi = self.nmi_level();
        self.cpu.set_nmi(nmi);
    }

    fn step(&mut self) -> u32 {
        if self.pressed[23] {
            self.cpu.t += 4;
            return 4;
        }
        let b = &mut self.board;
        b.trace.int = self.cpu.int_line;
        b.trace.nmi = self.cpu.nmi_line();
        let n = self.cpu.step(b);
        let fired = b.ctc.tick(n);
        for _ in 0..fired[2] {
            b.ctc.pulse(3);
        }
        if fired[0] > 0 && !b.nmi_latch {
            b.nmi_latch = true;
        }
        self.cpu.int_line = daisy::int_line(&[&b.pio1, &b.pio2, &b.ctc]);
        let nmi = self.nmi_level();
        self.cpu.set_nmi(nmi);
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
        const PIO1: [&str; 4] = ["PIO1 A data", "PIO1 A ctrl", "PIO1 B data", "PIO1 B ctrl"];
        const PIO2: [&str; 4] = ["PIO2 A data", "PIO2 A ctrl", "PIO2 B data", "PIO2 B ctrl"];
        const CTC: [&str; 4] = ["CTC ch0", "CTC ch1", "CTC ch2", "CTC ch3"];
        match Board::io_target(port) {
            Some((0, r)) => CTC[r],
            Some((1, r)) => PIO1[r],
            Some((2, r)) => PIO2[r],
            Some((_, _)) => "CLDIG",
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
                let k = &mut self.board.keys[row as usize];
                if down {
                    *k |= 1 << col
                } else {
                    *k &= !(1 << col)
                }
                let t = self.cpu.t;
                self.board.update_inputs(t);
            }
            KeyWire::Reset => {
                if down {
                    self.reset();
                }
            }
            KeyWire::Nmi => {
                let nmi = self.nmi_level();
                self.cpu.set_nmi(nmi);
            }
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
        vec![0.0]
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
                ("PIO1", &self.board.pio1, 0x80),
                ("PIO2", &self.board.pio2, 0x84),
            ],
            ctc: ("CTC", &self.board.ctc, 0x88),
            daisy: &["PIO1", "PIO2", "CTC"],
        }
    }

    fn chip_activity(&mut self) -> Vec<(&'static str, f32)> {
        let a = std::mem::take(&mut self.board.activity);
        let total = a[5].max(1) as f32;
        let mut v = vec![("CPU", if a[5] > 0 { 1.0 } else { 0.0 })];
        for (name, n) in ["ROM", "RAM", "PIO1", "PIO2", "CTC"].iter().zip(a) {
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
        Ok(())
    }
}
