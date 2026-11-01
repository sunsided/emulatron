//! U855 D — Z80 PIO clone.
//!
//! Modelled after the Zilog PIO technical manual: mode 0 (output), 1 (input),
//! 2 (bidirectional, port A only), 3 (bit control); interrupt control word,
//! mask, vector; daisy-chain interrupt logic. Handshake lines (ARDY/ASTB,
//! BRDY/BSTB) are tracked but none of the emulated boards wire them.

use serde::{Deserialize, Serialize};

use crate::daisy::{DaisyDevice, IrqState};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortId {
    A = 0,
    B = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PioMode {
    Output = 0,
    Input = 1,
    Bidirectional = 2,
    BitControl = 3,
}

impl PioMode {
    pub fn name(self) -> &'static str {
        match self {
            PioMode::Output => "OUT",
            PioMode::Input => "IN",
            PioMode::Bidirectional => "BIDI",
            PioMode::BitControl => "BIT",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum NextWord {
    Any,
    IoRegister,
    Mask,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PioPort {
    pub mode: PioMode,
    /// Output register.
    pub output: u8,
    /// Levels currently present on the port pins from the outside.
    pub input: u8,
    /// Mode 3 direction register: 1 = input.
    pub ior: u8,
    /// Mode 3 interrupt mask: 1 = bit not monitored.
    pub mask: u8,
    /// Last interrupt control word.
    pub icw: u8,
    pub vector: u8,
    /// Interrupt enable flip-flop.
    pub ie: bool,
    /// Interrupt pending.
    pub ip: bool,
    /// Interrupt under service.
    pub ius: bool,
    pub rdy: bool,
    next: NextWord,
    matched: bool,
}

impl Default for PioPort {
    fn default() -> Self {
        PioPort {
            mode: PioMode::Input,
            output: 0,
            input: 0xFF,
            ior: 0,
            mask: 0xFF,
            icw: 0,
            vector: 0,
            ie: false,
            ip: false,
            ius: false,
            rdy: false,
            next: NextWord::Any,
            matched: false,
        }
    }
}

impl PioPort {
    fn reset(&mut self) {
        let input = self.input;
        *self = PioPort {
            input,
            ..Default::default()
        };
    }

    /// Bits driven by the PIO onto the pins.
    pub fn drive_mask(&self) -> u8 {
        match self.mode {
            PioMode::Output | PioMode::Bidirectional => 0xFF,
            PioMode::Input => 0x00,
            PioMode::BitControl => !self.ior,
        }
    }

    /// Resulting pin levels: driven bits from the output register, the rest
    /// from the outside world.
    pub fn pins(&self) -> u8 {
        let m = self.drive_mask();
        (self.output & m) | (self.input & !m)
    }

    /// Evaluate the mode 3 interrupt logic equation.
    fn evaluate(&mut self) {
        if self.mode != PioMode::BitControl {
            return;
        }
        let sel = !self.mask & self.ior;
        let active_high = self.icw & 0x20 != 0;
        let and_logic = self.icw & 0x40 != 0;
        let lv = if active_high { self.input } else { !self.input } & sel;
        let m = if sel == 0 {
            false
        } else if and_logic {
            lv == sel
        } else {
            lv != 0
        };
        if m && !self.matched && self.ie {
            self.ip = true;
        }
        self.matched = m;
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Pio {
    pub ports: [PioPort; 2],
}

impl Pio {
    pub fn new() -> Self {
        Self::default()
    }

    /// /RESET (or M1 without RD/IORQ, as the real chip does it).
    pub fn reset(&mut self) {
        for p in &mut self.ports {
            p.reset();
        }
    }

    pub fn port(&self, id: PortId) -> &PioPort {
        &self.ports[id as usize]
    }

    /// Pin levels of a port as seen from the outside.
    pub fn pins(&self, id: PortId) -> u8 {
        self.ports[id as usize].pins()
    }

    /// Feed the levels present on the port pins.
    pub fn set_input(&mut self, id: PortId, levels: u8) {
        let p = &mut self.ports[id as usize];
        p.input = levels;
        p.evaluate();
    }

    /// CPU read of the data (`control == false`) or control register.
    pub fn read(&mut self, id: PortId, control: bool) -> u8 {
        let p = &mut self.ports[id as usize];
        if control {
            // The control register cannot be read back; the data bus floats.
            return 0xFF;
        }
        match p.mode {
            PioMode::Output => p.output,
            PioMode::Input | PioMode::Bidirectional => p.input,
            PioMode::BitControl => (p.input & p.ior) | (p.output & !p.ior),
        }
    }

    /// CPU write to the data or control register.
    pub fn write(&mut self, id: PortId, control: bool, v: u8) {
        if !control {
            let p = &mut self.ports[id as usize];
            p.output = v;
            if p.mode == PioMode::Output {
                p.rdy = true;
            }
            p.evaluate();
            return;
        }
        let p = &mut self.ports[id as usize];
        match p.next {
            NextWord::IoRegister => {
                p.ior = v;
                p.ie = p.icw & 0x80 != 0;
                p.next = NextWord::Any;
                p.evaluate();
            }
            NextWord::Mask => {
                p.mask = v;
                p.ie = p.icw & 0x80 != 0;
                p.next = NextWord::Any;
                p.evaluate();
            }
            NextWord::Any => {
                if v & 1 == 0 {
                    p.vector = v;
                } else {
                    match v & 0x0F {
                        0x0F => {
                            let mode = match v >> 6 {
                                0 => PioMode::Output,
                                1 => PioMode::Input,
                                2 if id == PortId::A => PioMode::Bidirectional,
                                2 => return, // invalid for port B
                                _ => PioMode::BitControl,
                            };
                            p.mode = mode;
                            match mode {
                                PioMode::Output => p.rdy = true,
                                PioMode::BitControl => {
                                    p.rdy = false;
                                    p.ie = false;
                                    p.matched = false;
                                    p.next = NextWord::IoRegister;
                                }
                                _ => {}
                            }
                        }
                        0x07 => {
                            p.icw = v;
                            if v & 0x10 != 0 {
                                p.ie = false;
                                p.ip = false;
                                p.matched = false;
                                p.next = NextWord::Mask;
                            } else {
                                p.ie = v & 0x80 != 0;
                                p.evaluate();
                            }
                        }
                        0x03 => {
                            p.icw = (v & 0x80) | (p.icw & 0x7F);
                            p.ie = v & 0x80 != 0;
                        }
                        _ => {} // invalid control word, ignored by the chip
                    }
                }
            }
        }
    }
}

impl DaisyDevice for Pio {
    fn irq_state(&self) -> IrqState {
        let mut state = IrqState::Idle;
        for p in &self.ports {
            if p.ius {
                return IrqState::UnderService;
            }
            if p.ie && p.ip {
                state = IrqState::Pending;
            }
        }
        state
    }

    fn irq_ack(&mut self) -> u8 {
        for p in &mut self.ports {
            if p.ie && p.ip {
                p.ip = false;
                p.ius = true;
                return p.vector;
            }
        }
        0xFF
    }

    fn irq_reti(&mut self) {
        for p in &mut self.ports {
            if p.ius {
                p.ius = false;
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_mode_pins() {
        let mut pio = Pio::new();
        pio.write(PortId::B, true, 0xCF); // mode 3
        pio.write(PortId::B, true, 0x01); // bit 0 input
        pio.set_input(PortId::B, 0x00);
        pio.write(PortId::B, false, 0xAA);
        assert_eq!(pio.pins(PortId::B), 0xAA);
        pio.set_input(PortId::B, 0x01);
        assert_eq!(pio.read(PortId::B, false), 0xAB);
    }

    #[test]
    fn bit_mode_interrupt() {
        let mut pio = Pio::new();
        pio.write(PortId::A, true, 0x20); // vector
        pio.write(PortId::A, true, 0xCF); // mode 3
        pio.write(PortId::A, true, 0xF0); // high nibble inputs
        pio.write(PortId::A, true, 0x97); // IE, OR, active low, mask follows
        pio.write(PortId::A, true, 0x0F); // monitor bits 4..7
        pio.set_input(PortId::A, 0xFF);
        assert_eq!(pio.irq_state(), IrqState::Idle);
        pio.set_input(PortId::A, 0xEF); // bit 4 low
        assert_eq!(pio.irq_state(), IrqState::Pending);
        assert_eq!(pio.irq_ack(), 0x20);
        assert_eq!(pio.irq_state(), IrqState::UnderService);
        pio.irq_reti();
        assert_eq!(pio.irq_state(), IrqState::Idle);
    }
}
