//! Z80 interrupt daisy chain.
//!
//! Devices are wired IEI→IEO in priority order. A device with an interrupt
//! under service pulls IEO low and so blocks every device further down the
//! chain until it sees RETI.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IrqState {
    /// Nothing to report, IEO follows IEI.
    Idle,
    /// Requesting an interrupt (/INT low).
    Pending,
    /// An interrupt is being serviced; IEO is held low.
    UnderService,
}

pub trait DaisyDevice {
    fn irq_state(&self) -> IrqState;
    /// Interrupt acknowledge: return the vector and enter "under service".
    fn irq_ack(&mut self) -> u8;
    /// RETI decoded on the bus.
    fn irq_reti(&mut self);
}

/// Level of the shared /INT line (true = asserted).
pub fn int_line(chain: &[&dyn DaisyDevice]) -> bool {
    for dev in chain {
        match dev.irq_state() {
            IrqState::UnderService => return false,
            IrqState::Pending => return true,
            IrqState::Idle => {}
        }
    }
    false
}

/// Find the highest-priority requesting device and acknowledge it.
/// Returns `(device index, vector)`.
pub fn ack(chain: &mut [&mut dyn DaisyDevice]) -> Option<(usize, u8)> {
    for (i, dev) in chain.iter_mut().enumerate() {
        match dev.irq_state() {
            IrqState::UnderService => return None,
            IrqState::Pending => return Some((i, dev.irq_ack())),
            IrqState::Idle => {}
        }
    }
    None
}

/// RETI goes to the highest-priority device that is under service.
pub fn reti(chain: &mut [&mut dyn DaisyDevice]) {
    for dev in chain.iter_mut() {
        if dev.irq_state() == IrqState::UnderService {
            dev.irq_reti();
            return;
        }
    }
}
