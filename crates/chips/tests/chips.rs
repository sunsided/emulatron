//! Behavioural tests of the U855 PIO, U857 CTC and the daisy chain, written
//! against the Zilog PIO/CTC technical manuals.

use emulatron_chips::ctc::{
    CTRL_CONTROL, CTRL_COUNTER, CTRL_INT_ENABLE, CTRL_PRESCALE_256, CTRL_RESET, CTRL_RISING,
    CTRL_TC_FOLLOWS, CTRL_TRIGGER, ChannelState,
};
use emulatron_chips::{Ctc, DaisyDevice, IrqState, Pio, PioMode, PortId, daisy};

// ---- PIO ----------------------------------------------------------------

#[test]
fn pio_power_on_state() {
    let pio = Pio::new();
    for id in [PortId::A, PortId::B] {
        let p = pio.port(id);
        assert_eq!(p.mode, PioMode::Input);
        assert_eq!(p.drive_mask(), 0x00, "input mode drives nothing");
        assert!(!p.ie && !p.ip && !p.ius);
    }
    assert_eq!(pio.irq_state(), IrqState::Idle);
}

#[test]
fn pio_mode0_output_drives_all_pins() {
    let mut pio = Pio::new();
    pio.write(PortId::A, true, 0x0F); // mode 0
    pio.write(PortId::A, false, 0x5A);
    assert_eq!(pio.port(PortId::A).mode, PioMode::Output);
    assert_eq!(pio.pins(PortId::A), 0x5A);
    // External levels are ignored on driven pins; read returns the output latch.
    pio.set_input(PortId::A, 0xFF);
    assert_eq!(pio.pins(PortId::A), 0x5A);
    assert_eq!(pio.read(PortId::A, false), 0x5A);
    assert!(pio.port(PortId::A).rdy);
}

#[test]
fn pio_mode1_input_reads_pins_and_latches_output() {
    let mut pio = Pio::new();
    pio.write(PortId::B, true, 0x4F); // mode 1
    pio.set_input(PortId::B, 0x3C);
    assert_eq!(pio.read(PortId::B, false), 0x3C);
    // A data write in input mode only loads the latch.
    pio.write(PortId::B, false, 0xAA);
    assert_eq!(pio.port(PortId::B).output, 0xAA);
    assert_eq!(pio.pins(PortId::B), 0x3C);
    // Switching to output puts the latched value on the pins.
    pio.write(PortId::B, true, 0x0F);
    assert_eq!(pio.pins(PortId::B), 0xAA);
}

#[test]
fn pio_mode2_is_port_a_only() {
    let mut pio = Pio::new();
    pio.write(PortId::B, true, 0x8F);
    assert_eq!(
        pio.port(PortId::B).mode,
        PioMode::Input,
        "mode 2 rejected on B"
    );
    pio.write(PortId::A, true, 0x8F);
    assert_eq!(pio.port(PortId::A).mode, PioMode::Bidirectional);
}

#[test]
fn pio_ports_are_independent() {
    let mut pio = Pio::new();
    pio.write(PortId::A, true, 0x0F);
    pio.write(PortId::A, false, 0x11);
    pio.write(PortId::B, true, 0x0F);
    pio.write(PortId::B, false, 0x22);
    assert_eq!(pio.pins(PortId::A), 0x11);
    assert_eq!(pio.pins(PortId::B), 0x22);
}

#[test]
fn pio_control_register_reads_float() {
    let mut pio = Pio::new();
    assert_eq!(pio.read(PortId::A, true), 0xFF);
}

#[test]
fn pio_vector_and_interrupt_enable_word() {
    let mut pio = Pio::new();
    pio.write(PortId::B, true, 0x42); // bit 0 clear: vector
    assert_eq!(pio.port(PortId::B).vector, 0x42);
    pio.write(PortId::B, true, 0x83); // set IE flip-flop only
    assert!(pio.port(PortId::B).ie);
    pio.write(PortId::B, true, 0x03); // clear it
    assert!(!pio.port(PortId::B).ie);
}

/// Configure port A for mode 3. Inputs start low so the logic equation is
/// false when interrupts get enabled.
fn bit_mode(pio: &mut Pio, ior: u8, icw: u8, mask: u8) {
    pio.set_input(PortId::A, 0x00);
    pio.write(PortId::A, true, 0x10); // vector
    pio.write(PortId::A, true, 0xCF); // mode 3
    pio.write(PortId::A, true, ior);
    pio.write(PortId::A, true, icw | 0x17); // ICW, mask follows
    pio.write(PortId::A, true, mask);
}

#[test]
fn pio_mode3_and_logic_active_high() {
    let mut pio = Pio::new();
    // Inputs 0..3, IE, AND, active high, monitor bits 0 and 1.
    bit_mode(&mut pio, 0x0F, 0xE0, !0x03);
    pio.set_input(PortId::A, 0x01);
    assert_eq!(pio.irq_state(), IrqState::Idle, "AND needs both bits");
    pio.set_input(PortId::A, 0x03);
    assert_eq!(pio.irq_state(), IrqState::Pending);
}

#[test]
fn pio_mode3_ignores_masked_bits_and_outputs() {
    let mut pio = Pio::new();
    // Inputs 0..3, IE, OR, active high, monitor bit 2 only.
    bit_mode(&mut pio, 0x0F, 0xA0, !0x04);
    pio.set_input(PortId::A, 0x0B); // bits 0,1,3 high, 2 low
    assert_eq!(pio.irq_state(), IrqState::Idle);
    pio.set_input(PortId::A, 0xF0); // only output bits high
    assert_eq!(pio.irq_state(), IrqState::Idle);
    pio.set_input(PortId::A, 0x04);
    assert_eq!(pio.irq_state(), IrqState::Pending);
}

#[test]
fn pio_mode3_interrupts_on_edge_not_level() {
    let mut pio = Pio::new();
    bit_mode(&mut pio, 0x01, 0xA0, 0xFE);
    pio.set_input(PortId::A, 0x01);
    assert_eq!(pio.irq_ack(), 0x10);
    pio.irq_reti();
    // Condition still true: no new interrupt without going false first.
    pio.set_input(PortId::A, 0x01);
    assert_eq!(pio.irq_state(), IrqState::Idle);
    pio.set_input(PortId::A, 0x00);
    pio.set_input(PortId::A, 0x01);
    assert_eq!(pio.irq_state(), IrqState::Pending);
}

#[test]
fn pio_mode3_condition_true_at_enable_interrupts() {
    // Pull-ups make all inputs high; enabling an OR/high monitor fires at once.
    let mut pio = Pio::new();
    pio.write(PortId::A, true, 0xCF);
    pio.write(PortId::A, true, 0x01);
    pio.write(PortId::A, true, 0xB7);
    pio.write(PortId::A, true, 0xFE);
    assert_eq!(pio.irq_state(), IrqState::Pending);
}

#[test]
fn pio_mode3_disabled_interrupts_stay_quiet() {
    let mut pio = Pio::new();
    bit_mode(&mut pio, 0x01, 0x20, 0xFE); // IE bit clear
    pio.set_input(PortId::A, 0x01);
    assert_eq!(pio.irq_state(), IrqState::Idle);
}

#[test]
fn pio_port_a_has_priority_over_b() {
    let mut pio = Pio::new();
    for (id, vec) in [(PortId::A, 0x20), (PortId::B, 0x30)] {
        pio.set_input(id, 0x00);
        pio.write(id, true, vec);
        pio.write(id, true, 0xCF);
        pio.write(id, true, 0x01);
        pio.write(id, true, 0xB7);
        pio.write(id, true, 0xFE);
    }
    pio.set_input(PortId::B, 0x01);
    pio.set_input(PortId::A, 0x01);
    assert_eq!(pio.irq_ack(), 0x20);
    // A under service blocks B until RETI.
    assert_eq!(pio.irq_state(), IrqState::UnderService);
    pio.irq_reti();
    assert_eq!(pio.irq_state(), IrqState::Pending);
    assert_eq!(pio.irq_ack(), 0x30);
}

#[test]
fn pio_reset_keeps_external_levels() {
    let mut pio = Pio::new();
    pio.set_input(PortId::A, 0x12);
    pio.write(PortId::A, true, 0x0F);
    pio.write(PortId::A, false, 0xFF);
    pio.reset();
    assert_eq!(pio.port(PortId::A).mode, PioMode::Input);
    assert_eq!(pio.pins(PortId::A), 0x12);
}

// ---- CTC ----------------------------------------------------------------

const TIMER: u8 = CTRL_CONTROL | CTRL_RESET | CTRL_TC_FOLLOWS;

fn run(ctc: &mut Ctc, cycles: u32) -> [u32; 4] {
    let mut total = [0; 4];
    for _ in 0..cycles {
        let f = ctc.tick(1);
        for i in 0..4 {
            total[i] += f[i];
        }
    }
    total
}

#[test]
fn ctc_timer_period_prescale_16_and_256() {
    let mut ctc = Ctc::new();
    ctc.write(0, TIMER);
    ctc.write(0, 4);
    ctc.write(1, TIMER | CTRL_PRESCALE_256);
    ctc.write(1, 2);
    let fired = run(&mut ctc, 16 * 4 * 8);
    assert_eq!(fired[0], 8, "16*4 clocks per period");
    assert_eq!(fired[1], 1, "256*2 clocks per period");
}

#[test]
fn ctc_time_constant_zero_means_256() {
    let mut ctc = Ctc::new();
    ctc.write(2, TIMER);
    ctc.write(2, 0);
    assert_eq!(run(&mut ctc, 16 * 256 - 1)[2], 0);
    assert_eq!(run(&mut ctc, 1)[2], 1);
}

#[test]
fn ctc_read_returns_down_counter() {
    let mut ctc = Ctc::new();
    ctc.write(0, TIMER);
    ctc.write(0, 10);
    assert_eq!(ctc.read(0), 10);
    run(&mut ctc, 16 * 3);
    assert_eq!(ctc.read(0), 7);
}

#[test]
fn ctc_waits_for_time_constant_after_reset() {
    let mut ctc = Ctc::new();
    ctc.write(0, TIMER);
    assert_eq!(ctc.channels[0].state, ChannelState::Stopped);
    assert_eq!(run(&mut ctc, 10_000)[0], 0);
}

#[test]
fn ctc_software_reset_stops_channel() {
    let mut ctc = Ctc::new();
    ctc.write(0, TIMER);
    ctc.write(0, 1);
    assert!(run(&mut ctc, 32)[0] > 0);
    ctc.write(0, CTRL_CONTROL | CTRL_RESET);
    assert_eq!(run(&mut ctc, 1000)[0], 0);
}

#[test]
fn ctc_timer_trigger_start() {
    let mut ctc = Ctc::new();
    ctc.write(0, TIMER | CTRL_TRIGGER | CTRL_RISING);
    ctc.write(0, 1);
    assert_eq!(ctc.channels[0].state, ChannelState::WaitTrigger);
    assert_eq!(run(&mut ctc, 100)[0], 0);
    ctc.trigger(0, true);
    assert_eq!(ctc.channels[0].state, ChannelState::Running);
    assert_eq!(run(&mut ctc, 16)[0], 1);
}

#[test]
fn ctc_counter_counts_selected_edge() {
    let mut ctc = Ctc::new();
    ctc.write(3, TIMER | CTRL_COUNTER); // falling edge
    ctc.write(3, 2);
    assert!(!ctc.trigger(3, true), "rising edge ignored");
    assert!(!ctc.trigger(3, false));
    assert!(!ctc.trigger(3, true));
    assert!(ctc.trigger(3, false), "second falling edge reaches zero");
    // Counters ignore the system clock.
    assert_eq!(run(&mut ctc, 10_000)[3], 0);
}

#[test]
fn ctc_new_time_constant_applies_at_next_reload() {
    let mut ctc = Ctc::new();
    ctc.write(0, TIMER);
    ctc.write(0, 2);
    ctc.write(0, CTRL_CONTROL | CTRL_TC_FOLLOWS); // no reset: keep running
    ctc.write(0, 5);
    assert_eq!(run(&mut ctc, 32)[0], 1, "old period finishes");
    assert_eq!(run(&mut ctc, 16 * 5)[0], 1, "then the new one");
}

#[test]
fn ctc_vectors_per_channel_and_priority() {
    let mut ctc = Ctc::new();
    ctc.write(0, 0xA8);
    for ch in [3, 1] {
        ctc.write(ch, TIMER | CTRL_INT_ENABLE);
        ctc.write(ch, 1);
    }
    run(&mut ctc, 16);
    assert_eq!(ctc.irq_ack(), 0xAA, "channel 1 before 3");
    assert_eq!(ctc.irq_state(), IrqState::UnderService);
    ctc.irq_reti();
    assert_eq!(ctc.irq_ack(), 0xAE);
}

#[test]
fn ctc_disabling_interrupts_clears_pending() {
    let mut ctc = Ctc::new();
    ctc.write(0, TIMER | CTRL_INT_ENABLE);
    ctc.write(0, 1);
    run(&mut ctc, 16);
    assert_eq!(ctc.irq_state(), IrqState::Pending);
    ctc.write(0, CTRL_CONTROL);
    assert_eq!(ctc.irq_state(), IrqState::Idle);
}

#[test]
fn ctc_counts_zero_crossings() {
    let mut ctc = Ctc::new();
    ctc.write(0, TIMER);
    ctc.write(0, 1);
    run(&mut ctc, 16 * 7);
    assert_eq!(ctc.channels[0].zc_count, 7);
}

// ---- daisy chain ---------------------------------------------------------

/// Scriptable daisy-chain device.
struct Dev {
    state: IrqState,
    vector: u8,
    acked: u32,
    retis: u32,
}

impl Dev {
    fn new(vector: u8) -> Self {
        Dev {
            state: IrqState::Idle,
            vector,
            acked: 0,
            retis: 0,
        }
    }
}

impl DaisyDevice for Dev {
    fn irq_state(&self) -> IrqState {
        self.state
    }
    fn irq_ack(&mut self) -> u8 {
        self.acked += 1;
        self.state = IrqState::UnderService;
        self.vector
    }
    fn irq_reti(&mut self) {
        self.retis += 1;
        self.state = IrqState::Idle;
    }
}

#[test]
fn daisy_highest_priority_wins() {
    let mut a = Dev::new(0x10);
    let mut b = Dev::new(0x20);
    b.state = IrqState::Pending;
    a.state = IrqState::Pending;
    assert!(daisy::int_line(&[&a, &b]));
    assert_eq!(daisy::ack(&mut [&mut a, &mut b]), Some((0, 0x10)));
    assert_eq!(b.acked, 0);
}

#[test]
fn daisy_service_blocks_lower_priority() {
    let mut a = Dev::new(0x10);
    let mut b = Dev::new(0x20);
    a.state = IrqState::UnderService;
    b.state = IrqState::Pending;
    assert!(!daisy::int_line(&[&a, &b]), "IEO low blocks b");
    assert_eq!(daisy::ack(&mut [&mut a, &mut b]), None);
    daisy::reti(&mut [&mut a, &mut b]);
    assert_eq!((a.retis, b.retis), (1, 0));
    assert!(daisy::int_line(&[&a, &b]));
}

#[test]
fn daisy_higher_priority_can_nest() {
    let mut a = Dev::new(0x10);
    let mut b = Dev::new(0x20);
    b.state = IrqState::UnderService;
    a.state = IrqState::Pending;
    assert!(daisy::int_line(&[&a, &b]), "a sits above b's IEO");
    assert_eq!(daisy::ack(&mut [&mut a, &mut b]), Some((0, 0x10)));
    // RETI ends the innermost (highest priority) service first.
    daisy::reti(&mut [&mut a, &mut b]);
    assert_eq!((a.retis, b.retis), (1, 0));
}

#[test]
fn daisy_idle_chain() {
    let a = Dev::new(0);
    assert!(!daisy::int_line(&[&a]));
}
