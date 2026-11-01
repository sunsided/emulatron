# emulatron-chips

The two peripheral chips of the U880 (the GDR's Z80) family, plus the
interrupt daisy chain that connects them:

- **U855 D**, the Z80 PIO: two 8-bit ports in output, input, bidirectional
  (port A) or bit-control mode, with the mode-3 AND/OR interrupt logic,
  masks and vectors.
- **U857 D**, the Z80 CTC: four channels as timers (system clock / 16 or
  / 256, optionally started by a trigger edge) or counters, with time
  constant reloads and per-channel vectors.
- The IEI/IEO **daisy chain**: priority, an interrupt under service blocks
  the devices behind it, RETI ends the innermost one.

They were written for [emulatron](https://github.com/sunsided/emulatron),
an emulator of the LC-80 and Poly-Computer 880 learning computers, but they
know nothing about either board. A chip never calls back into its
surroundings: the board writes registers, feeds pin levels in
(`Pio::set_input`, `Ctc::trigger`) and reads them back out (`Pio::pins`,
the zero counts returned by `Ctc::tick`). Everything is plain data, so it
can be cloned, inspected by a debugger and serialised with serde for save
states.

```rust
use emulatron_chips::{daisy, Ctc, Pio, PortId};

// PIO port A in mode 0 (output), as a CPU would program it with OUT.
let mut pio = Pio::new();
pio.write(PortId::A, true, 0x0F); // control: mode 0
pio.write(PortId::A, false, 0x5A); // data
assert_eq!(pio.pins(PortId::A), 0x5A);

// CTC channel 0 as a timer: system clock / 16, time constant 4,
// interrupts enabled, vector base 10h.
let mut ctc = Ctc::new();
ctc.write(0, 0x10); // vector
ctc.write(0, 0x87); // control: interrupt, timer /16, time constant follows, reset
ctc.write(0, 4); // time constant
let zero_counts: u32 = (0..64).map(|_| ctc.tick(1)[0]).sum();
assert_eq!(zero_counts, 1); // 16 × 4 clocks

// The daisy chain decides who gets to interrupt: here the CTC sits first.
assert!(daisy::int_line(&[&ctc, &pio]));
assert_eq!(daisy::ack(&mut [&mut ctc, &mut pio]), Some((0, 0x10)));
```

Timing is driven by the caller: `Ctc::tick` takes the number of system
clocks that passed (e.g. the T-states of the last instruction), so the chips
work with any CPU core.

Licensed under the [EUPL-1.2](https://github.com/sunsided/emulatron/blob/main/LICENSE).
