//! Tests against the original monitor ROMs. Skipped when `roms/` is empty
//! (the dumps are not part of the repository).

use emulatron_machines::roms::RomImages;
use emulatron_machines::{Emulator, create};

fn emu(variant: &str) -> Option<Emulator> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../roms");
    let roms = RomImages::from_dir(std::path::Path::new(dir)).ok()?;
    match create(variant, &roms) {
        Ok(m) => {
            let mut e = Emulator::new(m);
            if let Some(v) = std::env::var("AUTOTYPE_MS").ok().filter(|v| !v.is_empty()) {
                let v = v.parse().unwrap();
                e.autotype.hold_ms = v;
                e.autotype.gap_ms = v;
            }
            Some(e)
        }
        Err(e) => {
            eprintln!("skipping: {e}");
            None
        }
    }
}

fn ms(e: &Emulator, ms: u64) -> u64 {
    e.machine.clock_hz() as u64 * ms / 1000
}

fn run_until_typed(e: &mut Emulator) {
    for _ in 0..1200 {
        if !e.autotype.active() {
            break;
        }
        let c = ms(e, 100);
        e.run(c);
    }
    assert!(!e.autotype.active(), "autotype did not finish");
    let c = ms(e, 300);
    e.run(c);
}

fn text(e: &mut Emulator) -> String {
    let c = ms(e, 20);
    e.run(c);
    e.machine.display_frame();
    let c = ms(e, 100);
    e.run(c);
    e.machine.display_frame().text()
}

/// The LC-80 greets with a scrolling "HALLO USEr OF LC-80" banner and only
/// takes keys once it is done (~15 s).
fn lc80_after_banner(e: &mut Emulator) {
    let mut seen = String::new();
    for _ in 0..160 {
        let c = ms(e, 100);
        e.run(c);
        let t = e.machine.display_frame().text();
        if !seen.ends_with(&t) {
            seen.push_str(&t);
            seen.push('|');
        }
    }
    assert!(seen.contains("HALL0 "), "banner: {seen}");
    assert!(seen.contains(" LC-80"), "banner: {seen}");
}

#[test]
fn lc80_boot_and_enter_bytes() {
    let Some(mut e) = emu("lc80") else { return };
    lc80_after_banner(&mut e);

    e.type_script("{ADR}2000{DAT}11+22+33+44+{ADR}2000")
        .unwrap();
    run_until_typed(&mut e);
    assert_eq!(
        (0..4)
            .map(|i| e.machine.peek(0x2000 + i))
            .collect::<Vec<_>>(),
        [0x11, 0x22, 0x33, 0x44]
    );
    let t = text(&mut e);
    assert!(t.starts_with("2000"), "display: {t:?}");
}

#[test]
fn lc80_types_and_runs_hello() {
    let Some(mut e) = emu("lc80") else { return };
    lc80_after_banner(&mut e);
    let script = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/lc80/hello.txt"
    ))
    .unwrap();
    e.type_script(&script).unwrap();
    run_until_typed(&mut e);
    assert_eq!(text(&mut e), "HELL0 ");
}

#[test]
fn poly880_boot_and_enter_bytes() {
    let Some(mut e) = emu("poly880") else { return };
    let c = ms(&e, 2_000);
    e.run(c);
    assert_eq!(text(&mut e), "P0LY-880");

    e.type_script("{MEM}4000{EXEC}11{EXEC}22{EXEC}33{EXEC}{MEM}4000{EXEC}")
        .unwrap();
    run_until_typed(&mut e);
    assert_eq!(
        (0..3)
            .map(|i| e.machine.peek(0x4000 + i))
            .collect::<Vec<_>>(),
        [0x11, 0x22, 0x33]
    );
    // Manual: MEM shows "ΠΠ", then address and contents.
    assert_eq!(text(&mut e), "ΠΠ400011");
}

#[test]
fn poly880_types_and_runs_hello() {
    let Some(mut e) = emu("poly880") else { return };
    let c = ms(&e, 2_000);
    e.run(c);
    let script = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/poly880/hello.txt"
    ))
    .unwrap();
    e.type_script(&script).unwrap();
    run_until_typed(&mut e);
    assert_eq!(text(&mut e), " HELL0  ");
}

/// Watch the display for `ms` and collect every distinct picture.
fn pictures(e: &mut Emulator, ms_total: u64) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for _ in 0..ms_total / 250 {
        let c = ms(e, 250);
        e.run(c);
        let t = e.machine.display_frame().text();
        if seen.last() != Some(&t) {
            seen.push(t);
        }
    }
    seen
}

#[test]
fn every_variant_boots_its_monitor() {
    let cases = [
        ("lc80", 12_000, "HALL0 "),
        ("lc80a", 12_000, "HALL0 "),
        ("lc80e", 12_000, " U5Er "),
        ("lc80_2", 3_000, "LC-802"),
        ("poly880", 2_000, "P0LY-880"),
        ("poly880s", 2_000, "  _     "),
    ];
    for (v, wait, expect) in cases {
        let Some(mut e) = emu(v) else { continue };
        let seen = pictures(&mut e, wait);
        assert!(seen.iter().any(|p| p == expect), "{v}: {seen:?}");
        assert!(!e.machine.cpu().halted, "{v} must not crash into HALT");
    }
}

/// Save state + identical input = identical machine, cycle for cycle.
#[test]
fn restored_state_replays_deterministically() {
    let Some(mut a) = emu("poly880") else { return };
    let c = ms(&a, 1_500);
    a.run(c);
    let state = a.machine.save_state();
    let mut b = emu("poly880").unwrap();
    b.machine.load_state(&state).unwrap();

    let script = "{MEM}4000{EXEC}DE{EXEC}AD{EXEC}BE{EXEC}EF{EXEC}";
    for e in [&mut a, &mut b] {
        e.type_script(script).unwrap();
        let c = ms(e, 5_000);
        e.run(c);
    }
    assert_eq!(a.machine.cycles(), b.machine.cycles());
    assert_eq!(a.machine.cpu().regs, b.machine.cpu().regs);
    let ram = |e: &Emulator| {
        (0x4000..0x4400)
            .map(|x| e.machine.peek(x))
            .collect::<Vec<_>>()
    };
    assert_eq!(ram(&a), ram(&b));
    assert_eq!(&ram(&a)[..4], &[0xDE, 0xAD, 0xBE, 0xEF]);
}

/// Typing into the LC-80 during its banner is unreliable on the real monitor;
/// the emulator reproduces that, so scripts must wait for the banner.
#[test]
fn lc80_monitor_keys_after_banner_only() {
    let Some(mut e) = emu("lc80") else { return };
    lc80_after_banner(&mut e);
    e.type_script("{ADR}2100{DAT}5A+").unwrap();
    run_until_typed(&mut e);
    assert_eq!(e.machine.peek(0x2100), 0x5A);
    assert!(text(&mut e).starts_with("2101"), "address advanced by +");
}

/// The LC-80 plays a tune through the piezo (PIO1 PB1) while its banner
/// scrolls; the audio log turns those pin changes into samples.
#[test]
fn lc80_boot_tune_reaches_the_speaker() {
    use emulatron_machines::tape::AudioLog;
    let Some(mut e) = emu("lc80") else { return };
    let level = e.machine.tape().out_level;
    e.machine.tape().audio = Some(AudioLog::new(0, level));
    let mut changes = 0;
    for _ in 0..15 {
        let c = ms(&e, 1_000);
        e.run(c);
        let now = e.machine.cycles();
        let clock = e.machine.clock_hz();
        let samples = e
            .machine
            .tape()
            .audio
            .as_mut()
            .unwrap()
            .render(now, clock, 44_100.0, 0.2, 2.0);
        assert!(samples.len() >= 44_000, "a second of audio per second");
        changes += samples.windows(2).filter(|w| w[0] != w[1]).count();
    }
    let toggles = e.machine.tape().toggles;
    assert!(
        toggles > 5_000,
        "the banner plays a tune: {toggles} toggles"
    );
    assert!(
        changes > 5_000,
        "and it is audible: {changes} level changes"
    );
}
