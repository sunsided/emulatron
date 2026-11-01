//! Debug helper: boot a variant from ./roms, optionally autotype a script,
//! and print the display every N ms.
//! cargo run --release -p emulatron-machines --example probe -- lc80 20000 250 [script]
use emulatron_machines::{Emulator, create, roms::RomImages};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let roms = RomImages::from_dir(std::path::Path::new("roms")).unwrap();
    let mut e = Emulator::new(create(&a[1], &roms).unwrap());
    let total: u64 = a[2].parse().unwrap();
    let step: u64 = a[3].parse().unwrap();
    let hz = e.machine.clock_hz() as u64;
    let mut last = String::new();
    let mut t = 0;
    let mut typed = false;
    while t < total {
        if !typed
            && t >= std::env::var("TYPE_AT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1000)
        {
            if let Some(s) = a.get(4) {
                e.type_script(s).unwrap();
            }
            typed = true;
        }
        e.run(hz * step / 1000);
        t += step;
        let d = e.machine.display_frame().text();
        let pc = e.machine.cpu().regs.pc;
        if d != last {
            println!("{t:>6} ms  [{d}]  pc={pc:04X}");
            last = d;
        }
    }
    if let Some(dump) = a.get(5) {
        let base = u16::from_str_radix(dump, 16).unwrap();
        for row in 0..8u16 {
            let r = base + row * 16;
            let bytes: Vec<String> = (0..16)
                .map(|i| format!("{:02X}", e.machine.peek(r + i)))
                .collect();
            println!("{r:04X}: {}", bytes.join(" "));
        }
    }
}
