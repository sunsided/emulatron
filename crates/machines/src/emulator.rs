use std::collections::BTreeSet;

use crate::autotype::{self, Autotype};
use crate::machine::Machine;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    /// Ran the requested number of T-states.
    Budget,
    /// About to execute an instruction at a breakpoint.
    Breakpoint(u16),
}

/// A machine plus the conveniences the originals lacked.
pub struct Emulator {
    pub machine: Box<dyn Machine>,
    pub autotype: Autotype,
    pub breakpoints: BTreeSet<u16>,
}

impl Emulator {
    pub fn new(machine: Box<dyn Machine>) -> Self {
        Emulator {
            machine,
            autotype: Autotype::default(),
            breakpoints: BTreeSet::new(),
        }
    }

    /// Run for `cycles` T-states (or until a breakpoint). The instruction at
    /// the current PC is always executed, so resuming from a breakpoint works.
    pub fn run(&mut self, cycles: u64) -> StopReason {
        let target = self.machine.cycles() + cycles;
        let mut first = true;
        while self.machine.cycles() < target {
            self.autotype.service(self.machine.as_mut());
            let until = self
                .autotype
                .next_event()
                .map_or(target, |t| t.clamp(self.machine.cycles() + 1, target));
            while self.machine.cycles() < until {
                if !first && !self.breakpoints.is_empty() {
                    let pc = self.machine.cpu().regs.pc;
                    if self.breakpoints.contains(&pc) && !self.machine.cpu().halted {
                        return StopReason::Breakpoint(pc);
                    }
                }
                first = false;
                self.machine.step();
            }
        }
        self.autotype.service(self.machine.as_mut());
        StopReason::Budget
    }

    /// Execute exactly one instruction.
    pub fn step(&mut self) -> u32 {
        self.autotype.service(self.machine.as_mut());
        self.machine.step()
    }

    pub fn type_script(&mut self, script: &str) -> Result<(), String> {
        let tokens = autotype::parse(self.machine.info(), script)?;
        let now = self.machine.cycles();
        self.autotype.start(tokens, now);
        Ok(())
    }

    /// Copy bytes straight into RAM (the instant way).
    pub fn load_bytes(&mut self, addr: u16, bytes: &[u8]) -> Result<(), String> {
        for (i, &b) in bytes.iter().enumerate() {
            let a = addr.wrapping_add(i as u16);
            if !self.machine.poke(a, b) {
                return Err(format!("address {a:04X}h is not RAM"));
            }
        }
        Ok(())
    }

    /// Type bytes in through the monitor (the authentic way).
    pub fn type_bytes(&mut self, addr: u16, bytes: &[u8]) -> Result<(), String> {
        let script = autotype::enter_bytes(self.machine.info().family, addr, bytes);
        self.type_script(&script)
    }
}
