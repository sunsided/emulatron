//! Board wiring tests with tiny hand-assembled ROMs (no original ROMs needed).

use emulatron_machines::{Emulator, create, roms::RomImages};

fn rom(name: &str, code: &[u8], size: usize) -> RomImages {
    let mut data = code.to_vec();
    data.resize(size, 0xFF);
    let mut r = RomImages::new();
    r.insert(name, data);
    r
}

fn lc80_with(code: &[u8]) -> Emulator {
    let mut r = rom("bm075.d202", code, 0x400);
    r.insert("bm076.d203", vec![0xFF; 0x400]);
    Emulator::new(create("lc80", &r).unwrap())
}

#[test]
fn lc80_display_wiring() {
    // PIO1 A/B mode 0; PA = ~(b|c) = '1'; PB2 low = rightmost digit.
    let code = [
        0x3E, 0x0F, 0xD3, 0xF6, 0xD3, 0xF7, // LD A,0Fh; OUT (F6),A; OUT (F7),A
        0x3E, 0xDE, 0xD3, 0xF4, // LD A,DEh; OUT (F4),A
        0x3E, 0xFB, 0xD3, 0xF5, // LD A,FBh; OUT (F5),A
        0x18, 0xFE, // JR $
    ];
    let mut emu = lc80_with(&code);
    emu.run(10_000);
    let f = emu.machine.display_frame();
    assert_eq!(f.text(), "     1");
}

#[test]
fn lc80_keyboard_matrix() {
    // Select column 1 (PB3 low), read PIO2 B into A, store at 2000h, loop.
    let code = [
        0x3E, 0x0F, 0xD3, 0xF7, // PIO1 B output
        0x3E, 0x4F, 0xD3, 0xFB, // PIO2 B input
        0x3E, 0xF7, 0xD3, 0xF5, // PB3 low
        0xDB, 0xF9, // IN A,(F9h)
        0x32, 0x00, 0x20, // LD (2000h),A
        0x18, 0xF9, // JR to IN
    ];
    let mut emu = lc80_with(&code);
    let k1 = emu.machine.info().key_by_label("1").unwrap();
    emu.run(1000);
    assert_eq!(emu.machine.peek(0x2000), 0xF0);
    emu.machine.set_key(k1, true);
    emu.run(1000);
    // '1' is column 1, row 2 -> PB6 low.
    assert_eq!(emu.machine.peek(0x2000), 0xB0);
    // A key in another column is invisible.
    emu.machine.set_key(k1, false);
    let k4 = emu.machine.info().key_by_label("4").unwrap();
    emu.machine.set_key(k4, true);
    emu.run(1000);
    assert_eq!(emu.machine.peek(0x2000), 0xF0);
}

#[test]
fn poly880_display_wiring() {
    let code = [
        0x3E, 0x0F, 0xD3, 0x81, // PIO1 A mode 0 (alt addressing: 81h = A ctrl)
        0x3E, 0xB3, 0xD3, 0x80, // '2'
        0x3E, 0x80, 0xD3, 0xA0, // CLDIG bit 7 = leftmost digit
        0x18, 0xFE,
    ];
    let mut r = rom("bm039.i5", &code, 0x400);
    r.insert("bm040.i6", vec![0xFF; 0x400]);
    let mut emu = Emulator::new(create("poly880", &r).unwrap());
    emu.run(10_000);
    assert_eq!(emu.machine.display_frame().text(), "2       ");
}

#[test]
fn poly880_ctc_nmi_latch() {
    // CTC ch0 timer /16, tc=4 -> ZC0 fires -> NMI to 0066h which stores a marker.
    let mut code = vec![
        0x31, 0x00, 0x44, // LD SP,4400h
        0x3E, 0x07, 0xD3, 0x88, // CTC ch0: timer, /16, tc follows, reset
        0x3E, 0x04, 0xD3, 0x88, // tc = 4
        0x18, 0xFE, // JR $
    ];
    code.resize(0x66, 0);
    code.extend_from_slice(&[0x3E, 0x55, 0x32, 0x00, 0x40, 0x18, 0xFE]); // LD A,55h; LD (4000h),A; JR $
    let mut r = rom("bm039.i5", &code, 0x400);
    r.insert("bm040.i6", vec![0xFF; 0x400]);
    let mut emu = Emulator::new(create("poly880", &r).unwrap());
    emu.run(2000);
    assert_eq!(emu.machine.peek(0x4000), 0x55);
}

#[test]
fn save_and_restore() {
    let code = [0x21, 0x00, 0x20, 0x34, 0x18, 0xFD]; // LD HL,2000h; loop: INC (HL); JR loop
    let mut emu = lc80_with(&code);
    emu.run(5000);
    let state = emu.machine.save_state();
    let v = emu.machine.peek(0x2000);
    emu.run(5000);
    assert_ne!(emu.machine.peek(0x2000), v);
    emu.machine.load_state(&state).unwrap();
    assert_eq!(emu.machine.peek(0x2000), v);
}

#[test]
fn autotype_presses_keys() {
    let code = [0x18, 0xFE];
    let mut emu = lc80_with(&code);
    emu.type_script("{ADR} 2000 # comment\n{wait 10}").unwrap();
    let adr = emu.machine.info().key_by_label("ADR").unwrap();
    emu.run(100);
    assert!(emu.machine.key_down(adr));
    emu.run(900_000);
    assert!(!emu.autotype.active());
}

/// The example programs (normally typed in via autotype) display HELLO when
/// jumped to from a stub ROM.
#[test]
fn example_hello_programs() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples");

    let mut emu = lc80_with(&[0xC3, 0x00, 0x20]);
    let prog = std::fs::read(format!("{root}/lc80/hello.bin")).unwrap();
    emu.load_bytes(0x2000, &prog).unwrap();
    emu.run(100_000);
    emu.machine.display_frame();
    emu.run(100_000);
    assert_eq!(emu.machine.display_frame().text(), "HELL0 ");

    let mut r = rom("bm039.i5", &[0xC3, 0x00, 0x40], 0x400);
    r.insert("bm040.i6", vec![0xFF; 0x400]);
    let mut emu = Emulator::new(create("poly880", &r).unwrap());
    let prog = std::fs::read(format!("{root}/poly880/hello.bin")).unwrap();
    emu.load_bytes(0x4000, &prog).unwrap();
    emu.run(100_000);
    emu.machine.display_frame();
    emu.run(100_000);
    assert_eq!(emu.machine.display_frame().text(), " HELL0  ");

    // Scripts parse for their machines.
    for (m, f) in [("lc80", "lc80/hello.txt"), ("poly880", "poly880/hello.txt")] {
        let script = std::fs::read_to_string(format!("{root}/{f}")).unwrap();
        let info = if m == "lc80" {
            &emulatron_machines::lc80::INFO
        } else {
            &emulatron_machines::poly880::INFO
        };
        emulatron_machines::autotype::parse(info, &script).unwrap();
    }
}
