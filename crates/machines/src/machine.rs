use u880::Cpu;

use emulatron_chips::{Ctc, Pio};

use crate::display::DisplayFrame;
use crate::trace::BusTrace;

/// Index into [`MachineInfo::keys`].
pub type KeyId = usize;

/// Physical key colours, as on the real keypads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyColor {
    Red,
    Black,
    Cream,
    Orange,
    Grey,
    Teal,
}

/// What pressing a key does electrically.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyWire {
    /// Key in the scanned matrix: (column/select line, row/return line).
    Matrix(u8, u8),
    /// Pulls /RESET.
    Reset,
    /// Pulls /NMI.
    Nmi,
    /// Not (yet) connected in the emulation.
    None,
}

#[derive(Debug)]
pub struct KeyDef {
    pub label: &'static str,
    /// Secondary legend printed on/near the key (e.g. register names).
    pub sub: &'static str,
    pub color: KeyColor,
    /// Position on the physical keypad (row, column).
    pub pos: (u8, u8),
    pub wire: KeyWire,
    /// Characters that type this key (autotype and host keyboard).
    pub chars: &'static str,
}

#[derive(Debug)]
pub struct DigitGroup {
    pub label: &'static str,
    /// Display digit positions, left to right (0 = leftmost).
    pub first: usize,
    pub count: usize,
}

/// Static description of a machine for front-ends.
#[derive(Debug)]
pub struct MachineInfo {
    pub family: &'static str,
    pub title: &'static str,
    pub digits: usize,
    pub digit_groups: &'static [DigitGroup],
    pub keys: &'static [KeyDef],
    pub keypad_rows: u8,
    pub keypad_cols: u8,
    /// Discrete LEDs (name, colour) — brightness via [`Machine::leds`].
    pub leds: &'static [(&'static str, KeyColor)],
    /// Does the real device have bus-monitor LEDs (POLY880)?
    pub bus_leds: bool,
}

impl MachineInfo {
    /// Find the key typed by `c` (case-insensitive).
    pub fn key_for_char(&self, c: char) -> Option<KeyId> {
        let c = c.to_ascii_uppercase();
        self.keys
            .iter()
            .position(|k| k.chars.chars().any(|k| k.to_ascii_uppercase() == c))
    }

    pub fn key_by_label(&self, label: &str) -> Option<KeyId> {
        let l = label.trim();
        self.keys.iter().position(|k| {
            k.label.eq_ignore_ascii_case(l) || k.label.replace(' ', "").eq_ignore_ascii_case(l)
        })
    }
}

/// Address decoding description used for annotating the bus trace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    Rom,
    Ram,
    Unmapped,
}

/// Live, read-only view of the peripheral chips for visualisation.
pub struct Chips<'a> {
    /// (name, chip, port base address)
    pub pios: Vec<(&'static str, &'a Pio, u8)>,
    pub ctc: (&'static str, &'a Ctc, u8),
    /// Daisy chain order, highest priority first.
    pub daisy: &'static [&'static str],
}

/// Common interface of all emulated machines.
pub trait Machine {
    fn info(&self) -> &'static MachineInfo;
    fn variant(&self) -> &'static crate::roms::Variant;
    fn clock_hz(&self) -> u32 {
        self.variant().clock_hz
    }

    /// The RES key: a warm start. RAM keeps its contents, so the monitors
    /// skip their power-on greeting (and the LC-80 its tune).
    fn reset(&mut self);
    /// Switch off and on again: RAM is cleared, then a reset follows.
    fn power_cycle(&mut self);
    /// Execute one instruction; returns T-states.
    fn step(&mut self) -> u32;

    fn cpu(&self) -> &Cpu;
    fn cpu_mut(&mut self) -> &mut Cpu;
    /// Total T-states since power-on.
    fn cycles(&self) -> u64 {
        self.cpu().t
    }

    /// Side-effect free memory read for debuggers.
    fn peek(&self, addr: u16) -> u8;
    /// Write RAM directly (ROM writes are ignored). Returns false if not RAM.
    fn poke(&mut self, addr: u16, value: u8) -> bool;
    fn region(&self, addr: u16) -> Region;
    /// Human readable name of an I/O port ("PIO1 A data", ...).
    fn port_name(&self, port: u8) -> &'static str;

    fn set_key(&mut self, key: KeyId, down: bool);
    fn key_down(&self, key: KeyId) -> bool;

    /// Integrated LED segment brightness since the last call.
    fn display_frame(&mut self) -> DisplayFrame;
    /// Discrete LED brightness 0..1, in [`MachineInfo::leds`] order.
    fn leds(&mut self) -> Vec<f32>;

    fn trace(&self) -> &BusTrace;
    fn trace_mut(&mut self) -> &mut BusTrace;
    fn chips(&self) -> Chips<'_>;

    /// Current level of the tape output and speaker.
    fn tape(&mut self) -> &mut crate::tape::Tape;

    /// Share of bus cycles that addressed each chip since the last call,
    /// keyed by board designator (for "chips glow when used" displays).
    fn chip_activity(&mut self) -> Vec<(&'static str, f32)> {
        Vec::new()
    }

    /// Serialize the full machine state (excluding ROM) as JSON.
    fn save_state(&self) -> String;
    fn load_state(&mut self, json: &str) -> Result<(), String>;
}

/// Check a save state's `machine` field before decoding the rest, so that a
/// state for another machine gets a clear error instead of a field mismatch.
pub(crate) fn check_state_machine(json: &str, expected: &str) -> Result<(), String> {
    #[derive(serde::Deserialize)]
    struct Header {
        machine: String,
    }
    let h: Header = serde_json::from_str(json).map_err(|e| format!("not a save state: {e}"))?;
    if h.machine != expected {
        return Err(format!(
            "state is for '{}', running '{expected}'",
            h.machine
        ));
    }
    Ok(())
}
