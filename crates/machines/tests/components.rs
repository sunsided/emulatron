//! Unit-level tests of the machine building blocks.

use emulatron_machines::autotype::{self, Token};
use emulatron_machines::display::{self, LedDisplay, bitswap, seg_char};
use emulatron_machines::roms::{self, RomImages, sha1_hex};
use emulatron_machines::tape::{Tape, TapeState};
use emulatron_machines::trace::{self, BusTrace, CycleKind};
use emulatron_machines::{Emulator, StopReason, create, lc80, poly880};

// ---- display -------------------------------------------------------------

#[test]
fn display_full_multiplex_duty_is_full_brightness() {
    let mut d = LedDisplay::new(4);
    // Each digit lit 1/4 of the time with "8".
    let mut t = 0;
    for _ in 0..100 {
        for digit in 0..4 {
            d.drive(t, 1 << digit, 0x7F);
            t += 100;
        }
    }
    let f = d.frame(t);
    for digit in &f.digits {
        assert!((digit[0] - 1.0).abs() < 0.02, "{digit:?}");
        assert_eq!(digit[7], 0.0, "DP never lit");
    }
    assert_eq!(f.text(), "8888");
}

#[test]
fn display_partial_duty_is_dimmer_and_ghosts_are_faint() {
    let mut d = LedDisplay::new(2);
    let mut t = 0;
    for _ in 0..100 {
        d.drive(t, 0b01, display::SEG_A);
        t += 90;
        d.drive(t, 0b10, display::SEG_A); // brief ghost on digit 1
        t += 10;
        d.drive(t, 0, 0);
        t += 100;
    }
    let f = d.frame(t);
    assert!(
        f.digits[0][0] > 0.8 && f.digits[0][0] < 0.95,
        "{}",
        f.digits[0][0]
    );
    assert!(f.digits[1][0] < 0.15);
    assert_eq!(f.lit(0.5), vec![display::SEG_A, 0]);
}

#[test]
fn display_paused_keeps_last_frame_and_static_frame_shows_current_drive() {
    let mut d = LedDisplay::new(2);
    d.drive(0, 0b01, 0x06);
    d.drive(1000, 0b10, 0x5B);
    let f1 = d.frame(2000);
    assert_eq!(d.frame(2000), f1, "no time passed: same frame");
    assert_eq!(d.static_frame().text(), " 2");
    d.resync(5000);
    assert_eq!(d.frame(5000).text(), " 2", "after resync: static picture");
}

#[test]
fn seg_char_and_bitswap() {
    assert_eq!(seg_char(0x3F), '0');
    assert_eq!(seg_char(0x76), 'H');
    assert_eq!(seg_char(0x30), 'I');
    assert_eq!(seg_char(0x37), 'Π');
    assert_eq!(seg_char(0x80), '.');
    assert_eq!(seg_char(0x00), ' ');
    assert_eq!(seg_char(0x01), '?');
    assert_eq!(bitswap(0b0000_0001, [7, 6, 5, 4, 3, 2, 1, 0]), 0b1000_0000);
    assert_eq!(bitswap(0xA5, [0, 1, 2, 3, 4, 5, 6, 7]), 0xA5);
}

// ---- tape ----------------------------------------------------------------

#[test]
fn tape_records_and_plays_back_edges() {
    let mut tape = Tape::default();
    tape.record(1000);
    for (i, t) in [1100u64, 1300, 1400, 1800].iter().enumerate() {
        tape.output(*t, i % 2 == 0);
    }
    tape.output(1900, false); // no change: ignored
    assert_eq!(tape.edges, vec![100, 200, 100, 400]);
    tape.stop();

    tape.play(5000);
    assert_eq!(tape.state, TapeState::Playing);
    let levels: Vec<bool> = [5050, 5150, 5350, 5450, 5850]
        .iter()
        .map(|&t| tape.input(t))
        .collect();
    assert_eq!(levels, [false, true, false, true, false]);
    assert_eq!(tape.state, TapeState::Stopped, "stops at the end");
    assert_eq!(tape.progress(), (800, 800));
}

#[test]
fn audio_log_renders_a_square_wave() {
    use emulatron_machines::tape::AudioLog;
    // 1 MHz clock, 10 kHz sample rate: 100 T-states per sample.
    let mut tape = Tape::default();
    tape.audio = Some(AudioLog::new(0, false));
    // A 1 kHz tone: toggle every 500 T-states.
    for i in 1..=20u64 {
        tape.output(i * 500, i % 2 == 1);
    }
    let s = tape
        .audio
        .as_mut()
        .unwrap()
        .render(10_000, 1_000_000, 10_000.0, 0.5, 1.0);
    assert_eq!(s.len(), 100);
    // 5 samples low, 5 high, ... (edge at 500 is applied to the sample at 500)
    assert_eq!(
        &s[..12],
        &[
            -0.5, -0.5, -0.5, -0.5, -0.5, 0.5, 0.5, 0.5, 0.5, 0.5, -0.5, -0.5
        ]
    );
    // Nothing new: nothing rendered.
    assert!(
        tape.audio
            .as_mut()
            .unwrap()
            .render(10_000, 1_000_000, 10_000.0, 0.5, 1.0)
            .is_empty()
    );
}

#[test]
fn audio_log_skips_long_gaps() {
    use emulatron_machines::tape::AudioLog;
    let mut log = AudioLog::new(0, false);
    // Ten seconds later at 1 MHz, but only 0.1 s may be rendered.
    let s = log.render(10_000_000, 1_000_000, 8_000.0, 0.5, 0.1);
    assert_eq!(s.len(), 800);
}

#[test]
fn tape_counts_toggles_without_recording() {
    let mut tape = Tape::default();
    tape.output(10, true);
    tape.output(20, false);
    assert_eq!(tape.toggles, 2);
    assert!(tape.edges.is_empty());
    assert!(!tape.input(30), "not playing: input low");
}

// ---- trace ---------------------------------------------------------------

#[test]
fn trace_ring_keeps_most_recent_in_order() {
    let mut tr = BusTrace::default();
    for i in 0..5000u64 {
        tr.record(i, CycleKind::MemRead, i as u16, 0);
    }
    let r = tr.recent(3);
    assert_eq!(
        r.iter().map(|c| c.t).collect::<Vec<_>>(),
        [4997, 4998, 4999]
    );
    assert_eq!(tr.recent(10_000).len(), 4096);
    tr.clear();
    assert!(tr.recent(10).is_empty());
}

#[test]
fn trace_activity_is_a_duty_cycle() {
    let mut tr = BusTrace::default();
    tr.record(0, CycleKind::Fetch, 0xFFFF, 0x00); // 4 T
    tr.record(4, CycleKind::Fetch, 0x0000, 0xFF); // 4 T
    let a = tr.take_activity();
    assert!((a.lines[15] - 0.5).abs() < 1e-6, "A15 high half the time");
    assert!((a.lines[16] - 0.5).abs() < 1e-6, "D0 high half the time");
    assert_eq!(a.lines[trace::LINE_M1], 1.0);
    assert_eq!(a.lines[trace::LINE_WR], 0.0);
    // Paused: the last frame is kept.
    assert_eq!(tr.take_activity().lines, a.lines);
    tr.freeze();
    assert_eq!(
        tr.take_activity().lines[16],
        1.0,
        "frozen: static levels of last cycle"
    );
}

#[test]
fn trace_control_lines_per_cycle_kind() {
    use CycleKind::*;
    let c = |k: CycleKind| {
        let x = k.controls();
        (x.m1, x.mreq, x.iorq, x.rd, x.wr)
    };
    assert_eq!(c(Fetch), (true, true, false, true, false));
    assert_eq!(c(MemWrite), (false, true, false, false, true));
    assert_eq!(c(IoRead), (false, false, true, true, false));
    assert_eq!(c(IntAck), (true, false, true, false, false));
    assert!(IoWrite.is_io() && !MemRead.is_io());
}

// ---- autotype --------------------------------------------------------------

#[test]
fn autotype_parse_syntax() {
    let info = &lc80::INFO;
    let k = |l: &str| Token::Key(info.key_by_label(l).unwrap());
    let toks = autotype::parse(info, "{adr}2a # comment {EX}\n,{Wait 250ms}.{  EX }").unwrap();
    assert_eq!(
        toks,
        vec![
            k("ADR"),
            k("2"),
            k("A"),
            k("ADR"),
            Token::Wait(250),
            k("DAT"),
            k("EX")
        ]
    );
}

#[test]
fn autotype_parse_errors_name_the_line() {
    let info = &poly880::INFO;
    assert!(
        autotype::parse(info, "M\n\n?")
            .unwrap_err()
            .contains("line 3")
    );
    assert!(
        autotype::parse(info, "{NOPE}")
            .unwrap_err()
            .contains("no key")
    );
    assert!(
        autotype::parse(info, "{wait x}")
            .unwrap_err()
            .contains("bad wait")
    );
    assert!(
        autotype::parse(info, "{EXEC")
            .unwrap_err()
            .contains("unterminated")
    );
    assert!(
        autotype::parse(info, "{M CYCL}{mcycl}").is_ok(),
        "labels with spaces"
    );
}

#[test]
fn autotype_enter_bytes_scripts() {
    assert_eq!(
        autotype::enter_bytes("lc80", 0x2000, &[0x3E, 0x01]),
        "{ADR}2000{DAT}3E+01+"
    );
    assert_eq!(
        autotype::enter_bytes("poly880", 0x4000, &[0xFF]),
        "{MEM}4000{EXEC}FF{EXEC}"
    );
}

fn stub_lc80() -> Emulator {
    let mut r = RomImages::new();
    r.insert("bm075.d202", vec![0x18, 0xFE]); // JR $
    r.insert("bm076.d203", vec![0xFF; 0x400]);
    Emulator::new(create("lc80", &r).unwrap())
}

#[test]
fn autotype_timing_is_in_emulated_time() {
    let mut e = stub_lc80();
    e.autotype.hold_ms = 10;
    e.autotype.gap_ms = 20;
    e.type_script("12{wait 100}3").unwrap();
    let ms = |m: u64| 900 * m; // 900 kHz
    let k = |l: &str| e.machine.info().key_by_label(l).unwrap();
    let (k1, k2, k3) = (k("1"), k("2"), k("3"));
    e.run(ms(5));
    assert!(e.machine.key_down(k1));
    e.run(ms(10));
    assert!(!e.machine.key_down(k1), "released after 10 ms");
    e.run(ms(20));
    assert!(e.machine.key_down(k2), "next key after the gap");
    e.run(ms(30));
    assert!(!e.machine.key_down(k3), "wait 100 ms in between");
    e.run(ms(100));
    assert!(e.machine.key_down(k3));
    assert_eq!(e.autotype.progress().0, 1);
    e.autotype.cancel(e.machine.as_mut());
    assert!(!e.machine.key_down(k3) && !e.autotype.active());
}

// ---- ROM sets ----------------------------------------------------------------

#[test]
fn roms_match_by_sha1_before_name() {
    let mut r = RomImages::new();
    r.insert("whatever.bin", vec![1; 0x400]);
    assert!(r.available_variants().is_empty());
    let v = roms::variant("lc80").unwrap();
    let err = r.build_image(v).err().unwrap();
    assert!(err.contains("bm075.d202"));

    // Wrong content under the right name: accepted with a warning.
    r.insert("BM075.D202", vec![2; 0x400]);
    r.insert("bm076.d203", vec![3; 0x200]);
    let img = r.build_image(v).unwrap();
    assert_eq!(img.warnings.len(), 3, "{:?}", img.warnings);
    assert_eq!(img.data[0x0000], 2);
    assert_eq!(img.data[0x0800], 3);
    assert_eq!(img.data[0x0A00], 0xFF, "short ROM leaves the rest blank");
}

#[test]
fn roms_variant_lookup_and_sha1() {
    assert!(roms::variant("POLY880").is_some());
    assert!(roms::variant("zx81").is_none());
    assert_eq!(sha1_hex(b""), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
    assert!(create("zx81", &RomImages::new()).is_err());
}

// ---- emulator ------------------------------------------------------------------

#[test]
fn breakpoints_stop_before_execution_and_resume() {
    let mut r = RomImages::new();
    // 0: NOP ; 1: NOP ; 2: JR 0
    r.insert("bm075.d202", vec![0x00, 0x00, 0x18, 0xFC]);
    r.insert("bm076.d203", vec![0xFF; 0x400]);
    let mut e = Emulator::new(create("lc80", &r).unwrap());
    e.breakpoints.insert(0x0002);
    assert_eq!(e.run(10_000), StopReason::Breakpoint(2));
    assert_eq!(e.machine.cpu().regs.pc, 2);
    // Resuming executes the breakpointed instruction and stops on the next round.
    assert_eq!(e.run(10_000), StopReason::Breakpoint(2));
    let t0 = e.machine.cycles();
    e.breakpoints.clear();
    assert_eq!(e.run(1000), StopReason::Budget);
    assert!(e.machine.cycles() >= t0 + 1000);
}

#[test]
fn load_bytes_refuses_rom() {
    let mut e = stub_lc80();
    assert!(e.load_bytes(0x2000, &[1, 2, 3]).is_ok());
    assert_eq!(e.machine.peek(0x2001), 2);
    assert!(e.load_bytes(0x0000, &[1]).unwrap_err().contains("0000h"));
    assert!(
        e.load_bytes(0x23FF, &[1, 2]).is_err(),
        "runs past the end of RAM"
    );
}
