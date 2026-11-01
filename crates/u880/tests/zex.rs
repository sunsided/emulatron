//! Runs Frank Cringle's Z80 instruction exercisers (ZEXDOC / ZEXALL) under a
//! minimal CP/M BDOS shim. The binaries are GPL and not part of this repo;
//! fetch them with `scripts/fetch-zex.sh`. Tests are ignored by default
//! because they take a while: `cargo test -p u880 --release -- --ignored`.

use std::path::Path;

use u880::{Bus, Cpu};

struct CpmBus {
    mem: Box<[u8; 0x10000]>,
}

impl Bus for CpmBus {
    fn read(&mut self, addr: u16, _t: u64) -> u8 {
        self.mem[addr as usize]
    }
    fn write(&mut self, addr: u16, value: u8, _t: u64) {
        self.mem[addr as usize] = value;
    }
    fn input(&mut self, _port: u16, _t: u64) -> u8 {
        0xFF
    }
    fn output(&mut self, _port: u16, _value: u8, _t: u64) {}
}

fn run(name: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/zex")
        .join(name);
    let Ok(image) = std::fs::read(&path) else {
        eprintln!(
            "skipping: {} not found (run scripts/fetch-zex.sh)",
            path.display()
        );
        return;
    };
    let mut bus = CpmBus {
        mem: Box::new([0; 0x10000]),
    };
    bus.mem[0x100..0x100 + image.len()].copy_from_slice(&image);
    bus.mem[0x0005] = 0xC9; // BDOS entry: RET
    bus.mem[0x0006] = 0x00; // top of TPA used by ZEX for SP
    bus.mem[0x0007] = 0xF0;
    let mut cpu = Cpu::new();
    cpu.regs.pc = 0x100;
    cpu.regs.sp = 0xF000;

    let mut out = String::new();
    loop {
        match cpu.regs.pc {
            0x0000 => break,
            0x0005 => match cpu.regs.c {
                2 => {
                    out.push(cpu.regs.e as char);
                    print!("{}", cpu.regs.e as char);
                }
                9 => {
                    let mut a = cpu.regs.de();
                    while bus.mem[a as usize] != b'$' {
                        out.push(bus.mem[a as usize] as char);
                        print!("{}", bus.mem[a as usize] as char);
                        a = a.wrapping_add(1);
                    }
                }
                _ => {}
            },
            _ => {}
        }
        cpu.step(&mut bus);
    }
    println!();
    assert!(!out.contains("ERROR"), "{name} reported errors:\n{out}");
}

#[test]
#[ignore]
fn zexdoc() {
    run("zexdoc.com");
}

#[test]
#[ignore]
fn zexall() {
    run("zexall.com");
}
