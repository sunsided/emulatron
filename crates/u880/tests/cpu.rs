//! CPU behaviour beyond ZEX: interrupts, HALT, NMI, R register, bus cycle
//! order and timing, and disassembler/execution consistency.

use u880::disasm::disassemble;
use u880::{Bus, Cpu};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cycle {
    Fetch(u16),
    Read(u16),
    Write(u16, u8),
    In(u16),
    Out(u16, u8),
    IntAck,
}

struct TestBus {
    mem: Vec<u8>,
    log: Vec<(u64, Cycle)>,
    vector: u8,
    retis: u32,
    halt_changes: Vec<bool>,
    refresh: Vec<u16>,
}

impl TestBus {
    fn new(program: &[u8]) -> Self {
        let mut mem = vec![0; 0x10000];
        mem[..program.len()].copy_from_slice(program);
        TestBus {
            mem,
            log: vec![],
            vector: 0xFF,
            retis: 0,
            halt_changes: vec![],
            refresh: vec![],
        }
    }
    fn cycles(&self) -> Vec<Cycle> {
        self.log.iter().map(|(_, c)| *c).collect()
    }
}

impl Bus for TestBus {
    fn fetch(&mut self, addr: u16, t: u64) -> u8 {
        self.log.push((t, Cycle::Fetch(addr)));
        self.mem[addr as usize]
    }
    fn read(&mut self, addr: u16, t: u64) -> u8 {
        self.log.push((t, Cycle::Read(addr)));
        self.mem[addr as usize]
    }
    fn write(&mut self, addr: u16, v: u8, t: u64) {
        self.log.push((t, Cycle::Write(addr, v)));
        self.mem[addr as usize] = v;
    }
    fn input(&mut self, port: u16, t: u64) -> u8 {
        self.log.push((t, Cycle::In(port)));
        0x5A
    }
    fn output(&mut self, port: u16, v: u8, t: u64) {
        self.log.push((t, Cycle::Out(port, v)));
    }
    fn int_ack(&mut self, t: u64) -> u8 {
        self.log.push((t, Cycle::IntAck));
        self.vector
    }
    fn refresh(&mut self, ir: u16, _t: u64) {
        self.refresh.push(ir);
    }
    fn reti(&mut self) {
        self.retis += 1;
    }
    fn halt(&mut self, h: bool) {
        self.halt_changes.push(h);
    }
}

fn cpu_at(sp: u16) -> Cpu {
    let mut c = Cpu::new();
    c.regs.sp = sp;
    c
}

// ---- reset & registers ---------------------------------------------------

#[test]
fn reset_state() {
    let mut c = Cpu::new();
    c.regs.pc = 0x1234;
    c.regs.iff1 = true;
    c.regs.im = 2;
    c.regs.i = 0x55;
    c.halted = true;
    c.reset();
    assert_eq!(c.regs.pc, 0);
    assert_eq!((c.regs.i, c.regs.r, c.regs.im), (0, 0, 0));
    assert!(!c.regs.iff1 && !c.regs.iff2 && !c.halted);
    assert_eq!(c.regs.sp, 0xFFFF);
}

#[test]
fn r_register_counts_m1_cycles_and_keeps_bit7() {
    // LD A,80h ; LD R,A ; NOP ; DD-prefixed NOP (two M1 cycles)
    let mut bus = TestBus::new(&[0x3E, 0x80, 0xED, 0x4F, 0x00, 0xDD, 0x00]);
    let mut c = Cpu::new();
    for _ in 0..2 {
        c.step(&mut bus);
    }
    assert_eq!(c.regs.r, 0x80);
    c.step(&mut bus);
    assert_eq!(c.regs.r, 0x81);
    c.step(&mut bus);
    assert_eq!(c.regs.r, 0x83, "prefix counts as an M1");
    c.regs.r = 0xFF;
    c.regs.pc = 4;
    c.step(&mut bus);
    assert_eq!(c.regs.r, 0x80, "only the low 7 bits wrap");
}

#[test]
fn refresh_puts_i_and_r_on_the_bus() {
    // LD A,12h ; LD I,A ; NOP
    let mut bus = TestBus::new(&[0x3E, 0x12, 0xED, 0x47, 0x00]);
    let mut c = Cpu::new();
    for _ in 0..3 {
        c.step(&mut bus);
    }
    assert_eq!(*bus.refresh.last().unwrap(), 0x1200 | c.regs.r as u16);
}

#[test]
fn exchange_instructions() {
    // EX AF,AF' ; EXX ; EX DE,HL
    let mut bus = TestBus::new(&[0x08, 0xD9, 0xEB]);
    let mut c = Cpu::new();
    c.regs.set_af(0x1111);
    c.regs.set_bc(0x2222);
    c.regs.set_de(0x3333);
    c.regs.set_hl(0x4444);
    c.regs.alt = [0xAA, 0xAA, 0xBB, 0xBB, 0xCC, 0xCC, 0xDD, 0xDD];
    c.step(&mut bus);
    assert_eq!(c.regs.af(), 0xAAAA);
    c.step(&mut bus);
    assert_eq!(
        (c.regs.bc(), c.regs.de(), c.regs.hl()),
        (0xBBBB, 0xCCCC, 0xDDDD)
    );
    c.step(&mut bus);
    assert_eq!((c.regs.de(), c.regs.hl()), (0xDDDD, 0xCCCC));
    assert_eq!(c.regs.alt, [0x11, 0x11, 0x22, 0x22, 0x33, 0x33, 0x44, 0x44]);
}

// ---- bus cycles and timing -----------------------------------------------

#[test]
fn call_bus_sequence_and_timing() {
    let mut bus = TestBus::new(&[0xCD, 0x34, 0x12]);
    let mut c = cpu_at(0x8000);
    assert_eq!(c.step(&mut bus), 17);
    assert_eq!(
        bus.log,
        vec![
            (0, Cycle::Fetch(0)),
            (4, Cycle::Read(1)),
            (7, Cycle::Read(2)),
            (11, Cycle::Write(0x7FFF, 0x00)),
            (14, Cycle::Write(0x7FFE, 0x03)),
        ]
    );
    assert_eq!(c.regs.pc, 0x1234);
}

#[test]
fn io_address_bus_contents() {
    // LD A,12h ; OUT (34h),A ; IN A,(56h) ; LD BC,789Ah ; OUT (C),A ; IN E,(C)
    let prog = [
        0x3E, 0x12, 0xD3, 0x34, 0xDB, 0x56, 0x01, 0x9A, 0x78, 0xED, 0x79, 0xED, 0x58,
    ];
    let mut bus = TestBus::new(&prog);
    let mut c = Cpu::new();
    let times: Vec<u32> = (0..6).map(|_| c.step(&mut bus)).collect();
    assert_eq!(times, [7, 11, 11, 10, 12, 12]);
    let io: Vec<Cycle> = bus
        .cycles()
        .into_iter()
        .filter(|c| matches!(c, Cycle::In(_) | Cycle::Out(..)))
        .collect();
    assert_eq!(
        io,
        [
            Cycle::Out(0x1234, 0x12),
            Cycle::In(0x1256),
            Cycle::Out(0x789A, 0x5A),
            Cycle::In(0x789A)
        ]
    );
    assert_eq!(c.regs.e, 0x5A);
}

#[test]
fn instruction_timings() {
    // (program, expected T-states of the first instruction)
    let cases: &[(&[u8], u32)] = &[
        (&[0x00], 4),
        (&[0x7E], 7),                 // LD A,(HL)
        (&[0x36, 0x00], 10),          // LD (HL),n
        (&[0x34], 11),                // INC (HL)
        (&[0x09], 11),                // ADD HL,BC
        (&[0x03], 6),                 // INC BC
        (&[0xC5], 11),                // PUSH BC
        (&[0xC1], 10),                // POP BC
        (&[0xE3], 19),                // EX (SP),HL
        (&[0xF9], 6),                 // LD SP,HL
        (&[0x18, 0x00], 12),          // JR
        (&[0x20, 0x00], 12),          // JR NZ taken (Z clear after reset? F=FF -> not taken)
        (&[0xC3, 0, 0], 10),          // JP
        (&[0xC9], 10),                // RET
        (&[0xC7], 11),                // RST 0
        (&[0xCB, 0x00], 8),           // RLC B
        (&[0xCB, 0x06], 15),          // RLC (HL)
        (&[0xCB, 0x46], 12),          // BIT 0,(HL)
        (&[0xED, 0x44], 8),           // NEG
        (&[0xED, 0x42], 15),          // SBC HL,BC
        (&[0xED, 0x43, 0, 0], 20),    // LD (nn),BC
        (&[0xED, 0x67], 18),          // RRD
        (&[0xED, 0xA0], 16),          // LDI
        (&[0xED, 0x57], 9),           // LD A,I
        (&[0xDD, 0x21, 0, 0], 14),    // LD IX,nn
        (&[0xDD, 0x7E, 0], 19),       // LD A,(IX+d)
        (&[0xDD, 0xE5], 15),          // PUSH IX
        (&[0xDD, 0xE9], 8),           // JP (IX)
        (&[0xDD, 0xCB, 0, 0x06], 23), // RLC (IX+d)
        (&[0xDD, 0xCB, 0, 0x46], 20), // BIT 0,(IX+d)
        (&[0xDD, 0x00], 8),           // prefixed NOP
    ];
    for (prog, t) in cases {
        let mut bus = TestBus::new(prog);
        let mut c = cpu_at(0x8000);
        c.regs.set_hl(0x4000);
        c.regs.f = 0x40; // Z set: JR NZ not taken
        let expect = if prog[0] == 0x20 { 7 } else { *t };
        assert_eq!(c.step(&mut bus), expect, "{prog:02X?}");
    }
}

#[test]
fn conditional_timing_taken_and_not_taken() {
    for (f, call, ret, jr) in [(0x00, 17, 11, 12), (0x40, 10, 5, 7)] {
        for (prog, expect) in [
            (vec![0xC4, 0, 0], call),
            (vec![0xC0], ret),
            (vec![0x20, 0], jr),
        ] {
            let mut bus = TestBus::new(&prog);
            let mut c = cpu_at(0x8000);
            c.regs.f = f;
            assert_eq!(c.step(&mut bus), expect, "{prog:02X?} F={f:02X}");
        }
    }
}

#[test]
fn block_repeat_timing() {
    // LD BC,2 ; LDIR
    let mut bus = TestBus::new(&[0x01, 0x02, 0x00, 0xED, 0xB0]);
    let mut c = Cpu::new();
    c.regs.set_hl(0x1000);
    c.regs.set_de(0x2000);
    c.step(&mut bus);
    assert_eq!(c.step(&mut bus), 21, "repeat");
    assert_eq!(c.regs.pc, 3, "PC rewinds onto LDIR");
    assert_eq!(c.step(&mut bus), 16, "last iteration");
    assert_eq!(c.regs.pc, 5);
}

// ---- interrupts ------------------------------------------------------------

#[test]
fn int_ignored_while_disabled() {
    let mut bus = TestBus::new(&[0x00; 4]);
    let mut c = Cpu::new();
    c.int_line = true;
    c.step(&mut bus);
    assert_eq!(c.regs.pc, 1);
    assert!(!bus.cycles().contains(&Cycle::IntAck));
}

#[test]
fn ei_delays_by_one_instruction() {
    // IM 1 ; EI ; NOP ; NOP
    let mut bus = TestBus::new(&[0xED, 0x56, 0xFB, 0x00, 0x00]);
    let mut c = cpu_at(0x8000);
    c.int_line = true;
    c.step(&mut bus); // IM 1
    c.step(&mut bus); // EI
    c.step(&mut bus); // NOP still executes
    assert_eq!(c.regs.pc, 4);
    assert_eq!(c.step(&mut bus), 13, "IM 1 acceptance");
    assert_eq!(c.regs.pc, 0x38);
    assert!(!c.regs.iff1 && !c.regs.iff2);
    assert_eq!(bus.mem[0x7FFE], 4, "return address pushed");
}

#[test]
fn im0_executes_rst_from_bus() {
    let mut bus = TestBus::new(&[0xFB, 0x00, 0x00]);
    bus.vector = 0xD7; // RST 10h
    let mut c = cpu_at(0x8000);
    c.step(&mut bus);
    c.int_line = true;
    c.step(&mut bus);
    c.step(&mut bus);
    assert_eq!(c.regs.pc, 0x10);
}

#[test]
fn halt_waits_for_interrupt_and_resumes_after_it() {
    // IM 1 ; EI ; HALT ; NOP
    let mut bus = TestBus::new(&[0xED, 0x56, 0xFB, 0x76, 0x00]);
    let mut c = cpu_at(0x8000);
    for _ in 0..3 {
        c.step(&mut bus);
    }
    assert!(c.halted);
    assert_eq!(bus.halt_changes, [true]);
    for _ in 0..5 {
        assert_eq!(c.step(&mut bus), 4, "HALT idles with NOP cycles");
        assert_eq!(c.regs.pc, 4, "PC stays after HALT");
    }
    c.int_line = true;
    c.step(&mut bus);
    assert!(!c.halted);
    assert_eq!(bus.halt_changes, [true, false]);
    assert_eq!(c.regs.pc, 0x38);
    assert_eq!(
        bus.mem[0x7FFE], 4,
        "RETI target is the instruction after HALT"
    );
}

#[test]
fn nmi_is_edge_triggered_and_preserves_iff2() {
    let mut bus = TestBus::new(&[0xFB, 0x00, 0x00, 0x00]);
    bus.mem[0x66] = 0x00;
    let mut c = cpu_at(0x8000);
    c.step(&mut bus); // EI
    c.set_nmi(true);
    assert_eq!(c.step(&mut bus), 11);
    assert_eq!(c.regs.pc, 0x66);
    assert!(!c.regs.iff1, "IFF1 cleared");
    assert!(c.regs.iff2, "IFF2 remembers");
    // Holding the line does not retrigger.
    c.step(&mut bus);
    assert_eq!(c.regs.pc, 0x67);
    c.set_nmi(false);
    c.set_nmi(true);
    c.step(&mut bus);
    assert_eq!(c.regs.pc, 0x66, "new edge, new NMI");
}

#[test]
fn nmi_beats_int_and_retn_restores_iff1() {
    let mut bus = TestBus::new(&[0xFB, 0x00]);
    bus.mem[0x66] = 0xED;
    bus.mem[0x67] = 0x45; // RETN
    let mut c = cpu_at(0x8000);
    c.step(&mut bus);
    c.int_line = true;
    c.set_nmi(true);
    c.step(&mut bus);
    assert_eq!(c.regs.pc, 0x66);
    c.step(&mut bus); // RETN (INT blocked: IFF1 clear)
    assert!(c.regs.iff1);
    assert_eq!(c.regs.pc, 1);
}

#[test]
fn reti_is_signalled_to_the_bus() {
    let mut bus = TestBus::new(&[0xED, 0x4D]);
    let mut c = cpu_at(0x8000);
    c.step(&mut bus);
    assert_eq!(bus.retis, 1);
    let mut bus = TestBus::new(&[0xED, 0x45]);
    c = cpu_at(0x8000);
    c.step(&mut bus);
    assert_eq!(bus.retis, 0, "RETN is not RETI");
}

// ---- disassembler consistency ----------------------------------------------

fn is_flow(text: &str) -> bool {
    ["JP", "JR", "CALL", "RET", "RST", "DJNZ", "HALT"]
        .iter()
        .any(|m| text.starts_with(m))
        || text.ends_with('R')
            && (text.starts_with("LD")
                || text.starts_with("CP")
                || text.starts_with("IN")
                || text.starts_with("OT"))
}

/// For every non-branching opcode, the disassembler's length must equal how
/// far the CPU advances PC.
#[test]
fn disassembler_lengths_match_execution() {
    let mut programs: Vec<Vec<u8>> = Vec::new();
    for op in 0..=255u8 {
        programs.push(vec![op]);
        programs.push(vec![0xCB, op]);
        programs.push(vec![0xED, op]);
        programs.push(vec![0xDD, op]);
        programs.push(vec![0xFD, op]);
        programs.push(vec![0xDD, 0xCB, 0x05, op]);
        programs.push(vec![0xFD, 0xCB, 0xFB, op]);
    }
    let mut checked = 0;
    for prog in programs {
        let mut bytes = prog.clone();
        bytes.extend_from_slice(&[0x01, 0x02, 0x03]);
        let ins = disassemble(|a| *bytes.get(a as usize).unwrap_or(&0), 0);
        if is_flow(&ins.text) || ins.text == "EI" {
            continue;
        }
        let mut bus = TestBus::new(&bytes);
        let mut c = cpu_at(0x8000);
        c.regs.set_hl(0x4000);
        c.regs.ix = 0x4000;
        c.regs.iy = 0x4000;
        c.step(&mut bus);
        assert_eq!(c.regs.pc, ins.len as u16, "{prog:02X?} '{}'", ins.text);
        checked += 1;
    }
    assert!(checked > 1500, "only {checked} opcodes checked");
}
