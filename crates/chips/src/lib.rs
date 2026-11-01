//! Peripheral chips of the U880 family.
//!
//! * [`Pio`] — U855 D, clone of the Z80 PIO (parallel I/O, two ports)
//! * [`Ctc`] — U857 D, clone of the Z80 CTC (four counter/timer channels)
//! * [`daisy`] — the Z80 mode-2 interrupt daisy chain (IEI/IEO priority)
//!
//! Chips never call back into the board. Instead the board feeds them pin
//! levels ([`Pio::set_input`], [`Ctc::trigger`]) and inspects their outputs
//! ([`Pio::pins`], the return value of [`Ctc::tick`]). That keeps them plain
//! data that can be cloned, serialised for save states, and inspected by a
//! UI at any time.

pub mod ctc;
pub mod daisy;
pub mod pio;

pub use ctc::Ctc;
pub use daisy::{DaisyDevice, IrqState};
pub use pio::{Pio, PioMode, PortId};
