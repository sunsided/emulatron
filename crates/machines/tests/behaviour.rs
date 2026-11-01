//! Machine-level behaviour with hand-assembled stub ROMs.

use emulatron_machines::roms::RomImages;
use emulatron_machines::tape::TapeState;
use emulatron_machines::{Emulator, Region, create};

fn image(code: &[(u16, &[u8])], size: usize) -> Vec<u8> {
    let mut v = vec![0xFF; size];
    for (at, bytes) in code {
        v[*at as usize..*at as usize + bytes.len()].copy_from_slice(bytes);
    }
    v
}

fn lc80(code: &[(u16, &[u8])]) -> Emulator {
    let mut r = RomImages::new();
    r.insert("bm075.d202", image(code, 0x400));
    r.insert("bm076.d203", vec![0xEE; 0x400]);
    Emulator::new(create("lc80", &r).unwrap())
}

fn poly(code: &[(u16, &[u8])]) -> Emulator {
    let mut r = RomImages::new();
    r.insert("bm039.i5", image(code, 0x400));
    r.insert("bm040.i6", vec![0xEE; 0x400]);
    Emulator::new(create("poly880", &r).unwrap())
}

fn key(e: &Emulator, label: &str) -> usize {
    e.machine.info().key_by_label(label).unwrap()
}

const SPIN: &[u8] = &[0x18, 0xFE]; // JR $

// ---- memory maps -------------------------------------------------------------

#[test]
fn lc80_memory_map_and_mirrors() {
    let e = lc80(&[(0, &[0x12, 0x34])]);
    let m = &e.machine;
    assert_eq!(m.peek(0x0000), 0x12);
    assert_eq!(m.peek(0x0400), 0x12, "1 KB ROM mirrored once");
    assert_eq!(m.peek(0x0800), 0xEE, "second ROM at 0800h");
    assert_eq!(m.peek(0x0C00), 0xEE);
    assert_eq!(m.region(0x1000), Region::Unmapped);
    assert_eq!(m.peek(0x1000), 0xFF, "open bus");
    assert_eq!(m.region(0x2000), Region::Ram);
    assert_eq!(m.region(0x2400), Region::Unmapped, "only 1 KB RAM");
    assert_eq!(m.region(0xC000), Region::Rom, "A14/A15 not decoded");
}

#[test]
fn lc80_ram_mirrors() {
    let mut e = lc80(&[]);
    assert!(e.machine.poke(0x2010, 0x42));
    assert_eq!(e.machine.peek(0x3010), 0x42, "mirrored at 3000h");
    assert_eq!(e.machine.peek(0xE010), 0x42, "and in the upper 32 KB");
    assert!(!e.machine.poke(0x0010, 0x42), "ROM is not writable");
}

#[test]
fn poly880_memory_map_and_mirrors() {
    let mut e = poly(&[(0, &[0x77])]);
    let m = &mut e.machine;
    assert_eq!(m.peek(0x0C00), 0x77, "socket mirrored 4x in its 4 KB slot");
    assert_eq!(m.peek(0x1000), 0xEE);
    assert_eq!(m.peek(0x2000), 0xFF, "empty sockets");
    assert!(m.poke(0x4001, 0x99));
    assert_eq!(m.peek(0x7C01), 0x99, "1 KB RAM mirrored up to 7FFFh");
    assert_eq!(m.region(0x8000), Region::Unmapped);
}

#[test]
fn cpu_writes_reach_ram_through_the_bus() {
    // LD A,5Ah ; LD (3123h),A ; JR $
    let mut e = lc80(&[(0, &[0x3E, 0x5A, 0x32, 0x23, 0x31, 0x18, 0xFE])]);
    e.run(100);
    assert_eq!(e.machine.peek(0x2123), 0x5A, "write via the mirror");
}

// ---- keys --------------------------------------------------------------------

#[test]
fn lc80_res_key_holds_the_cpu() {
    let mut e = lc80(&[(0, &[0x3C, 0x18, 0xFD])]); // INC A ; JR -3
    e.run(1000);
    let res = key(&e, "RES");
    e.machine.set_key(res, true);
    assert_eq!(e.machine.cpu().regs.pc, 0);
    let t = e.machine.cycles();
    e.run(1000);
    assert_eq!(e.machine.cpu().regs.pc, 0, "held in reset");
    assert!(e.machine.cycles() > t, "time still passes");
    e.machine.set_key(res, false);
    e.run(100);
    assert_ne!(e.machine.cpu().regs.pc, 0);
}

#[test]
fn lc80_nmi_key_enters_0066h() {
    let mut e = lc80(&[(0, &[0x31, 0x00, 0x24, 0x18, 0xFE]), (0x66, &[0x76])]); // LD SP ; JR $ / HALT
    e.run(100);
    e.machine.set_key(key(&e, "NMI"), true);
    e.run(100);
    assert!(e.machine.cpu().halted);
    assert_eq!(e.machine.cpu().regs.pc, 0x67);
    assert_eq!(e.machine.leds()[1], 1.0, "HALT LED");
}

#[test]
fn poly880_mon_key_and_scon_release() {
    // NMI handler: count in RAM, then release the latch via PB6 and spin.
    let mut e = poly(&[
        (
            0,
            &[
                0x31, 0x00, 0x44, 0x3E, 0xCF, 0xD3, 0x83, 0x3E, 0x00, 0xD3, 0x83, 0x18, 0xFE,
            ],
        ),
        // 66h: INC (4000h) ; LD A,40h ; OUT (82h),A ; RETN
        (
            0x66,
            &[0x21, 0x00, 0x40, 0x34, 0x3E, 0x40, 0xD3, 0x82, 0xED, 0x45],
        ),
    ]);
    let mon = key(&e, "MON");
    e.run(500);
    e.machine.set_key(mon, true);
    e.run(500);
    e.machine.set_key(mon, false);
    e.run(500);
    e.machine.set_key(mon, true);
    e.run(500);
    assert_eq!(e.machine.peek(0x4000), 2, "two edges, two NMIs");
}

// ---- outputs -----------------------------------------------------------------

#[test]
fn lc80_out_led_and_tape_follow_pb1() {
    // PIO1 B mode 0; toggle PB1 every ~1000 T.
    let mut e = lc80(&[(
        0,
        &[
            0x3E, 0x0F, 0xD3, 0xF7, // PIO1 B output
            0x3E, 0xFF, // loop: LD A,FFh
            0xEE, 0x02, // XOR 02h  (A1)  -> flips PB1 each pass via (A) below
            0xD3, 0xF5, // OUT (F5h),A
            0x06, 0x40, 0x10, 0xFE, // delay
            0x3E, 0xFF, 0xD3, 0xF5, // PB1 high
            0x06, 0x40, 0x10, 0xFE, 0x18, 0xEC, // JR loop
        ],
    )]);
    e.machine.tape().record(0);
    e.run(200_000);
    let led = e.machine.leds()[0];
    assert!(led > 0.3 && led < 0.7, "OUT LED about half on: {led}");
    let tape = e.machine.tape();
    assert_eq!(tape.state, TapeState::Recording);
    assert!(
        tape.edges.len() > 100,
        "square wave recorded: {}",
        tape.edges.len()
    );
}

#[test]
fn lc80_tape_playback_reaches_pio1_pb0() {
    // PIO1 B mode 3 with PB0 input; copy PB0 to RAM continuously.
    let mut e = lc80(&[(
        0,
        &[
            0x3E, 0xCF, 0xD3, 0xF7, 0x3E, 0x01, 0xD3, 0xF7, // mode 3, PB0 in
            0xDB, 0xF5, 0xE6, 0x01, 0x32, 0x00, 0x20, 0x18, 0xF7, // loop
        ],
    )]);
    let t = e.machine.cycles();
    let tape = e.machine.tape();
    tape.edges = vec![10_000, 10_000];
    tape.play(t);
    e.run(5_000);
    assert_eq!(e.machine.peek(0x2000), 0);
    e.run(10_000);
    assert_eq!(e.machine.peek(0x2000), 1);
    e.run(10_000);
    assert_eq!(e.machine.peek(0x2000), 0);
}

#[test]
fn lc80_ctc_interrupt_through_daisy_chain() {
    // IM 2 with I=03h, CTC vector 10h -> table at 0310h -> handler 0080h.
    let mut e = lc80(&[
        (
            0,
            &[
                0x31, 0x00, 0x24, 0x3E, 0x03, 0xED, 0x47, 0xED, 0x5E, // SP, I, IM 2
                0x3E, 0x10, 0xD3, 0xEC, // CTC vector
                0x3E, 0x87, 0xD3, 0xEC, 0x3E, 0x10, 0xD3, 0xEC, // ch0: int, timer/16, tc=16
                0xFB, 0x18, 0xFE, // EI ; JR $
            ],
        ),
        (0x80, &[0x21, 0x00, 0x20, 0x34, 0xFB, 0xED, 0x4D]), // INC (2000h) ; EI ; RETI
        (0x310, &[0x80, 0x00]),
    ]);
    e.run(16 * 16 * 10 + 200);
    let n = e.machine.peek(0x2000);
    assert!((9..=10).contains(&n), "one interrupt per 256 T: {n}");
}

#[test]
fn chip_activity_reports_shares() {
    let mut e = lc80(&[(0, SPIN)]);
    e.run(10_000);
    let act = e.machine.chip_activity();
    let get = |n: &str| act.iter().find(|(k, _)| *k == n).unwrap().1;
    assert_eq!(get("D201"), 1.0);
    assert!(get("D202") > 0.99, "everything comes from the first ROM");
    assert_eq!(get("D203"), 0.0);
    assert_eq!(get("D206"), 0.0);
    let act = e.machine.chip_activity();
    assert_eq!(act[0].1, 0.0, "counters reset by reading");
}

#[test]
fn port_names() {
    let e = lc80(&[]);
    assert_eq!(e.machine.port_name(0xF4), "PIO1 A data");
    assert_eq!(e.machine.port_name(0xFB), "PIO2 B ctrl");
    assert_eq!(e.machine.port_name(0xEE), "CTC ch2");
    assert_eq!(e.machine.port_name(0x00), "-");
    let p = poly(&[]);
    assert_eq!(
        p.machine.port_name(0x81),
        "PIO1 A ctrl",
        "alternate addressing"
    );
    assert_eq!(p.machine.port_name(0x82), "PIO1 B data");
    assert_eq!(p.machine.port_name(0xA5), "CLDIG");
    assert_eq!(p.machine.port_name(0x8B), "CTC ch3");
}

// ---- save states ----------------------------------------------------------------

#[test]
fn save_state_is_validated() {
    let e = lc80(&[(0, SPIN)]);
    let state = e.machine.save_state();
    let mut p = poly(&[(0, SPIN)]);
    assert!(p.machine.load_state(&state).unwrap_err().contains("lc80"));
    assert!(p.machine.load_state("{not json").is_err());
    let mut l = lc80(&[(0, SPIN)]);
    let tampered = state.replacen("\"machine\":\"lc80\"", "\"machine\":\"lc80a\"", 1);
    assert!(l.machine.load_state(&tampered).is_err());
}

#[test]
fn save_state_keeps_rom_and_restores_everything_else() {
    let mut e = lc80(&[(0, &[0x21, 0x00, 0x20, 0x34, 0x18, 0xFD])]);
    e.run(3000);
    let s = e.machine.save_state();
    let (pc, t, ram) = (
        e.machine.cpu().regs.pc,
        e.machine.cycles(),
        e.machine.peek(0x2000),
    );
    e.run(3000);
    e.machine.load_state(&s).unwrap();
    assert_eq!(
        (
            e.machine.cpu().regs.pc,
            e.machine.cycles(),
            e.machine.peek(0x2000)
        ),
        (pc, t, ram)
    );
    assert_eq!(e.machine.peek(0), 0x21, "ROM untouched");
    assert!(!s.contains("\"rom\""), "ROM not serialised");
}
