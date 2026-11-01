//! The learning computers themselves.
//!
//! * [`lc80::Lc80`] — LC-80, VEB Mikroelektronik "Karl Marx" Erfurt, 1984
//! * [`poly880::Poly880`] — Poly-Computer 880, VEB Polytechnik Karl-Marx-Stadt, 1983
//!
//! Both implement [`Machine`], which is what the front-ends (TUI, WASM) talk
//! to. [`Emulator`] wraps a machine with the "modern conveniences": autotype,
//! breakpoints and save states.

pub mod autotype;
pub mod display;
mod emulator;
pub mod lc80;
mod machine;
pub mod poly880;
pub mod roms;
pub mod tape;
pub mod trace;

pub use emulator::{Emulator, StopReason};
pub use machine::*;

/// Create a machine by its identifier (see [`roms::VARIANTS`]).
pub fn create(variant: &str, roms: &roms::RomImages) -> Result<Box<dyn Machine>, String> {
    let v = roms::variant(variant).ok_or_else(|| format!("unknown machine variant '{variant}'"))?;
    let image = roms.build_image(v)?;
    Ok(match v.family {
        roms::Family::Lc80 => Box::new(lc80::Lc80::new(v, image)),
        roms::Family::Poly880 => Box::new(poly880::Poly880::new(v, image)),
    })
}
