//! WebAssembly bindings for the browser front-end.
//!
//! The JS side owns timing (requestAnimationFrame) and the ROM files; this
//! crate wraps an [`Emulator`] and hands out plain, serialisable snapshots:
//! a cheap per-frame one ([`WebEmu::frame`]) and heavier ones for the
//! debugging panels that the UI polls less often.

use std::collections::BTreeSet;

use serde::Serialize;
use wasm_bindgen::prelude::*;

use emulatron_chips::ctc::{CTRL_COUNTER, CTRL_INT_ENABLE, ChannelState};
use emulatron_chips::{DaisyDevice, IrqState, PortId};
use emulatron_machines::roms::{self, RomImages};
use emulatron_machines::tape::AudioLog;
use emulatron_machines::trace::CycleKind;
use emulatron_machines::{Emulator, KeyColor, KeyWire, Region, StopReason, create};
use u880::disasm::disassemble;

fn to_js<T: Serialize>(v: &T) -> JsValue {
    v.serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .unwrap_or(JsValue::NULL)
}

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

// ---- ROM bundle --------------------------------------------------------------

/// The ROM files the page fetched; variants are built from it.
#[wasm_bindgen]
#[derive(Default)]
pub struct RomBundle {
    roms: RomImages,
}

#[derive(Serialize)]
struct VariantDto {
    id: &'static str,
    name: &'static str,
    family: &'static str,
    clock_hz: u32,
    available: bool,
}

#[wasm_bindgen]
impl RomBundle {
    #[wasm_bindgen(constructor)]
    pub fn new() -> RomBundle {
        RomBundle::default()
    }

    pub fn add(&mut self, name: &str, data: &[u8]) {
        self.roms.insert(name, data.to_vec());
    }

    /// All known variants and whether their ROMs are present.
    pub fn variants(&self) -> JsValue {
        let avail = self.roms.available_variants();
        let v: Vec<VariantDto> = roms::VARIANTS
            .iter()
            .map(|v| VariantDto {
                id: v.id,
                name: v.name,
                family: match v.family {
                    roms::Family::Lc80 => "lc80",
                    roms::Family::Poly880 => "poly880",
                },
                clock_hz: v.clock_hz,
                available: avail.iter().any(|a| a.id == v.id),
            })
            .collect();
        to_js(&v)
    }
}

// ---- DTOs --------------------------------------------------------------------------

#[derive(Serialize)]
struct KeyDto {
    label: &'static str,
    sub: &'static str,
    color: &'static str,
    row: u8,
    col: u8,
    chars: String,
    /// "matrix", "reset", "nmi" or "cycle" (handled by the UI).
    kind: &'static str,
}

#[derive(Serialize)]
struct GroupDto {
    label: &'static str,
    first: usize,
    count: usize,
}

#[derive(Serialize)]
struct InfoDto {
    family: &'static str,
    title: &'static str,
    variant: &'static str,
    variant_name: &'static str,
    clock_hz: u32,
    digits: usize,
    groups: Vec<GroupDto>,
    keys: Vec<KeyDto>,
    keypad_rows: u8,
    keypad_cols: u8,
    leds: Vec<(&'static str, &'static str)>,
    bus_leds: bool,
    /// User RAM: first address and size in bytes.
    ram_start: u16,
    ram_size: usize,
}

fn color_name(c: KeyColor) -> &'static str {
    match c {
        KeyColor::Red => "red",
        KeyColor::Black => "black",
        KeyColor::Cream => "cream",
        KeyColor::Orange => "orange",
        KeyColor::Grey => "grey",
        KeyColor::Teal => "teal",
    }
}

#[derive(Serialize)]
struct FrameDto {
    /// Brightness per digit, segments a..g, dp.
    digits: Vec<[f32; 8]>,
    leds: Vec<f32>,
    /// Bus line duty cycles: A0..A15, D0..D7, M1 MREQ IORQ RD WR HALT INT NMI.
    bus: Vec<f32>,
    /// Share of bus cycles per chip (board designators).
    chips: Vec<(&'static str, f32)>,
    keys_down: Vec<bool>,
    cycles: f64,
    pc: u16,
    halted: bool,
    /// Set when a breakpoint stopped the last run.
    breakpoint: Option<u16>,
    /// (remaining, total) autotype tokens while typing.
    autotype: Option<(usize, usize)>,
    /// The last bus cycle: (kind, address, data).
    last_cycle: Option<(&'static str, u16, u8)>,
}

#[derive(Serialize)]
struct CpuDto {
    af: u16,
    bc: u16,
    de: u16,
    hl: u16,
    af_: u16,
    bc_: u16,
    de_: u16,
    hl_: u16,
    ix: u16,
    iy: u16,
    sp: u16,
    pc: u16,
    i: u8,
    r: u8,
    f: u8,
    im: u8,
    iff1: bool,
    iff2: bool,
    halted: bool,
    int: bool,
    nmi: bool,
    t: f64,
}

#[derive(Serialize)]
struct InsDto {
    addr: u16,
    bytes: String,
    text: String,
    breakpoint: bool,
}

#[derive(Serialize)]
struct CycleDto {
    t: f64,
    kind: &'static str,
    addr: u16,
    data: u8,
    what: String,
}

#[derive(Serialize)]
struct PortDto {
    mode: &'static str,
    pins: u8,
    drive: u8,
    vector: u8,
    ie: bool,
    ip: bool,
    ius: bool,
}

#[derive(Serialize)]
struct PioDto {
    name: &'static str,
    base: u8,
    irq: &'static str,
    ports: Vec<PortDto>,
}

#[derive(Serialize)]
struct ChannelDto {
    mode: String,
    state: &'static str,
    tc: u8,
    down: u8,
    ie: bool,
    ip: bool,
    ius: bool,
    zc: f64,
}

#[derive(Serialize)]
struct CtcDto {
    name: &'static str,
    base: u8,
    vector: u8,
    irq: &'static str,
    channels: Vec<ChannelDto>,
}

#[derive(Serialize)]
struct ChipsDto {
    pios: Vec<PioDto>,
    ctc: CtcDto,
    daisy: Vec<(&'static str, &'static str)>,
}

fn irq_name(s: IrqState) -> &'static str {
    match s {
        IrqState::Idle => "idle",
        IrqState::Pending => "pending",
        IrqState::UnderService => "service",
    }
}

fn region_name(r: Region) -> &'static str {
    match r {
        Region::Rom => "ROM",
        Region::Ram => "RAM",
        Region::Unmapped => "—",
    }
}

// ---- emulator ------------------------------------------------------------------------

#[wasm_bindgen]
pub struct WebEmu {
    emu: Emulator,
    breakpoint_hit: Option<u16>,
}

#[wasm_bindgen]
impl WebEmu {
    #[wasm_bindgen(constructor)]
    pub fn new(roms: &RomBundle, variant: &str) -> Result<WebEmu, JsError> {
        let m = create(variant, &roms.roms).map_err(|e| JsError::new(&e))?;
        Ok(WebEmu {
            emu: Emulator::new(m),
            breakpoint_hit: None,
        })
    }

    pub fn info(&self) -> JsValue {
        let m = &self.emu.machine;
        let info = m.info();
        let v = m.variant();
        let keys = info
            .keys
            .iter()
            .map(|k| KeyDto {
                label: k.label,
                sub: k.sub,
                color: color_name(k.color),
                row: k.pos.0,
                col: k.pos.1,
                chars: k.chars.chars().filter(|c| !c.is_control()).collect(),
                kind: match k.wire {
                    KeyWire::Matrix(..) => "matrix",
                    KeyWire::Reset => "reset",
                    KeyWire::Nmi => "nmi",
                    KeyWire::None => "cycle",
                },
            })
            .collect();
        to_js(&InfoDto {
            family: info.family,
            title: info.title,
            variant: v.id,
            variant_name: v.name,
            clock_hz: v.clock_hz,
            digits: info.digits,
            groups: info
                .digit_groups
                .iter()
                .map(|g| GroupDto {
                    label: g.label,
                    first: g.first,
                    count: g.count,
                })
                .collect(),
            keys,
            keypad_rows: info.keypad_rows,
            keypad_cols: info.keypad_cols,
            leds: info
                .leds
                .iter()
                .map(|(n, c)| (*n, color_name(*c)))
                .collect(),
            bus_leds: info.bus_leds,
            ram_start: if info.family == "lc80" {
                0x2000
            } else {
                0x4000
            },
            ram_size: v.ram_size,
        })
    }

    /// Run for `cycles` T-states. Returns false if a breakpoint stopped it.
    pub fn run(&mut self, cycles: f64) -> bool {
        match self.emu.run(cycles.max(1.0) as u64) {
            StopReason::Budget => true,
            StopReason::Breakpoint(pc) => {
                self.breakpoint_hit = Some(pc);
                false
            }
        }
    }

    /// Execute one instruction; returns its T-states.
    pub fn step(&mut self) -> u32 {
        self.emu.machine.trace_mut().freeze();
        self.emu.step()
    }

    pub fn clock_hz(&self) -> u32 {
        self.emu.machine.clock_hz()
    }

    pub fn cycles(&self) -> f64 {
        self.emu.machine.cycles() as f64
    }

    pub fn set_key(&mut self, key: usize, down: bool) {
        self.emu.machine.set_key(key, down);
    }

    /// Integrated display/LED/bus state since the previous call.
    pub fn frame(&mut self, running: bool) -> JsValue {
        let m = &mut self.emu.machine;
        let digits = m.display_frame().digits;
        let leds = m.leds();
        let chips = m.chip_activity();
        let tr = m.trace_mut();
        let bus = if running {
            tr.take_activity()
        } else {
            tr.static_levels()
        };
        let last_cycle = tr.last.map(|c| (c.kind.short(), c.addr, c.data));
        let keys = m.info().keys.len();
        let keys_down = (0..keys).map(|k| m.key_down(k)).collect();
        let cpu = m.cpu();
        let dto = FrameDto {
            digits,
            leds,
            bus: bus.lines.to_vec(),
            chips,
            keys_down,
            cycles: cpu.t as f64,
            pc: cpu.regs.pc,
            halted: cpu.halted,
            breakpoint: self.breakpoint_hit.take(),
            autotype: self
                .emu
                .autotype
                .active()
                .then(|| self.emu.autotype.progress()),
            last_cycle,
        };
        to_js(&dto)
    }

    pub fn cpu(&self) -> JsValue {
        let c = self.emu.machine.cpu();
        let r = &c.regs;
        let alt = |i: usize| u16::from_be_bytes([r.alt[i], r.alt[i + 1]]);
        to_js(&CpuDto {
            af: r.af(),
            bc: r.bc(),
            de: r.de(),
            hl: r.hl(),
            af_: alt(0),
            bc_: alt(2),
            de_: alt(4),
            hl_: alt(6),
            ix: r.ix,
            iy: r.iy,
            sp: r.sp,
            pc: r.pc,
            i: r.i,
            r: r.r,
            f: r.f,
            im: r.im,
            iff1: r.iff1,
            iff2: r.iff2,
            halted: c.halted,
            int: c.int_line,
            nmi: c.nmi_line(),
            t: c.t as f64,
        })
    }

    /// Disassemble `count` instructions starting at `addr`.
    pub fn disasm(&self, addr: u16, count: usize) -> JsValue {
        let m = &self.emu.machine;
        let mut pc = addr;
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            let ins = disassemble(|a| m.peek(a), pc);
            let bytes = (0..ins.len)
                .map(|k| format!("{:02X}", m.peek(pc.wrapping_add(k as u16))))
                .collect();
            out.push(InsDto {
                addr: pc,
                bytes,
                text: ins.text,
                breakpoint: self.emu.breakpoints.contains(&pc),
            });
            pc = pc.wrapping_add(ins.len as u16);
        }
        to_js(&out)
    }

    /// The most recent bus cycles, oldest first, annotated.
    pub fn trace(&self, count: usize) -> JsValue {
        let m = &self.emu.machine;
        let out: Vec<CycleDto> = m
            .trace()
            .recent(count)
            .into_iter()
            .map(|c| {
                let what = match c.kind {
                    CycleKind::Fetch => {
                        format!(
                            "{}  {}",
                            region_name(m.region(c.addr)),
                            disassemble(|a| m.peek(a), c.addr).text
                        )
                    }
                    CycleKind::MemRead | CycleKind::MemWrite => {
                        region_name(m.region(c.addr)).to_string()
                    }
                    CycleKind::IoRead | CycleKind::IoWrite => m.port_name(c.addr as u8).to_string(),
                    CycleKind::IntAck => "interrupt vector".to_string(),
                };
                CycleDto {
                    t: c.t as f64,
                    kind: c.kind.short(),
                    addr: c.addr,
                    data: c.data,
                    what,
                }
            })
            .collect();
        to_js(&out)
    }

    /// `len` bytes of memory from `addr` (side-effect free).
    pub fn memory(&self, addr: u16, len: usize) -> Vec<u8> {
        (0..len)
            .map(|i| self.emu.machine.peek(addr.wrapping_add(i as u16)))
            .collect()
    }

    /// 0 = ROM, 1 = RAM, 2 = unmapped, per byte.
    pub fn regions(&self, addr: u16, len: usize) -> Vec<u8> {
        (0..len)
            .map(
                |i| match self.emu.machine.region(addr.wrapping_add(i as u16)) {
                    Region::Rom => 0,
                    Region::Ram => 1,
                    Region::Unmapped => 2,
                },
            )
            .collect()
    }

    pub fn chips(&self) -> JsValue {
        let chips = self.emu.machine.chips();
        let pios = chips
            .pios
            .iter()
            .map(|(name, pio, base)| PioDto {
                name,
                base: *base,
                irq: irq_name(pio.irq_state()),
                ports: [PortId::A, PortId::B]
                    .iter()
                    .map(|&id| {
                        let p = pio.port(id);
                        PortDto {
                            mode: p.mode.name(),
                            pins: p.pins(),
                            drive: p.drive_mask(),
                            vector: p.vector,
                            ie: p.ie,
                            ip: p.ip,
                            ius: p.ius,
                        }
                    })
                    .collect(),
            })
            .collect::<Vec<_>>();
        let (cname, ctc, cbase) = chips.ctc;
        let channels = ctc
            .channels
            .iter()
            .map(|c| ChannelDto {
                mode: if c.control & CTRL_COUNTER != 0 {
                    "counter".into()
                } else {
                    format!("timer /{}", c.prescaler())
                },
                state: match c.state {
                    ChannelState::Stopped => "stopped",
                    ChannelState::WaitTrigger => "trigger",
                    ChannelState::Running => "running",
                },
                tc: c.tc,
                down: c.down as u8,
                ie: c.control & CTRL_INT_ENABLE != 0,
                ip: c.ip,
                ius: c.ius,
                zc: c.zc_count as f64,
            })
            .collect();
        let daisy = chips
            .daisy
            .iter()
            .map(|n| {
                let st = if *n == cname {
                    ctc.irq_state()
                } else {
                    chips
                        .pios
                        .iter()
                        .find(|p| p.0 == *n)
                        .map(|p| p.1.irq_state())
                        .unwrap_or(IrqState::Idle)
                };
                (*n, irq_name(st))
            })
            .collect();
        to_js(&ChipsDto {
            pios,
            ctc: CtcDto {
                name: cname,
                base: cbase,
                vector: ctc.vector,
                irq: irq_name(ctc.irq_state()),
                channels,
            },
            daisy,
        })
    }

    /// Start or stop collecting the speaker/tape output for audio.
    pub fn set_audio(&mut self, on: bool) {
        let now = self.emu.machine.cycles();
        let tape = self.emu.machine.tape();
        tape.audio = on.then(|| AudioLog::new(now, tape.out_level));
    }

    /// Samples (mono, -1..1) of the speaker since the previous call.
    pub fn take_audio(&mut self, sample_rate: f64) -> Vec<f32> {
        let now = self.emu.machine.cycles();
        let clock = self.emu.machine.clock_hz();
        match &mut self.emu.machine.tape().audio {
            Some(a) => a.render(now, clock, sample_rate, 0.2, 0.25),
            None => Vec::new(),
        }
    }

    pub fn reset(&mut self) {
        self.emu.machine.reset();
    }

    /// Switch off and on: cleared RAM, cold start.
    pub fn power_cycle(&mut self) {
        self.emu.autotype.cancel(self.emu.machine.as_mut());
        self.emu.machine.power_cycle();
        self.emu.machine.trace_mut().clear();
    }

    pub fn type_script(&mut self, script: &str) -> Result<(), JsError> {
        self.emu.type_script(script).map_err(|e| JsError::new(&e))
    }

    /// The autotype script that enters `bytes` at `addr` through the monitor.
    pub fn enter_script(&self, addr: u16, bytes: &[u8]) -> String {
        emulatron_machines::autotype::enter_bytes(self.emu.machine.info().family, addr, bytes)
    }

    pub fn type_bytes(&mut self, addr: u16, bytes: &[u8]) -> Result<(), JsError> {
        self.emu
            .type_bytes(addr, bytes)
            .map_err(|e| JsError::new(&e))
    }

    pub fn load_bytes(&mut self, addr: u16, bytes: &[u8]) -> Result<(), JsError> {
        self.emu
            .load_bytes(addr, bytes)
            .map_err(|e| JsError::new(&e))
    }

    pub fn cancel_autotype(&mut self) {
        self.emu.autotype.cancel(self.emu.machine.as_mut());
    }

    pub fn save_state(&self) -> String {
        self.emu.machine.save_state()
    }

    pub fn load_state(&mut self, json: &str) -> Result<(), JsError> {
        self.emu
            .machine
            .load_state(json)
            .map_err(|e| JsError::new(&e))?;
        self.emu.machine.trace_mut().clear();
        Ok(())
    }

    /// Toggle a breakpoint; returns whether it is now set.
    pub fn toggle_breakpoint(&mut self, addr: u16) -> bool {
        if self.emu.breakpoints.remove(&addr) {
            false
        } else {
            self.emu.breakpoints.insert(addr);
            true
        }
    }

    pub fn breakpoints(&self) -> Vec<u16> {
        self.emu
            .breakpoints
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}
