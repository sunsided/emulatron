//! Application state and input handling.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::{Position, Rect};

use emulatron_machines::display::DisplayFrame;
use emulatron_machines::trace::{self, BusActivity, BusCycle};
use emulatron_machines::{Emulator, KeyId, KeyWire, StopReason};

pub const SPEEDS: [f64; 8] = [0.01, 0.1, 0.25, 0.5, 1.0, 2.0, 10.0, f64::INFINITY];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BottomView {
    Trace,
    Memory,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    Autotype,
    MemoryAddress,
    Breakpoint,
}

/// UI commands, bound to function keys and toolbar buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Help,
    RunStop,
    Step,
    CycleMode,
    Save,
    Load,
    Autotype,
    Speed,
    Reset,
    Nmi,
    Breakpoint,
    MemAddr,
    ToggleView,
    Diagram,
    Quit,
}

impl Action {
    pub fn from_fkey(n: u8) -> Action {
        use Action::*;
        [
            Help, RunStop, Step, CycleMode, Save, Load, Autotype, Speed, Reset, Nmi, Breakpoint,
            MemAddr,
        ][(n - 1) as usize]
    }
}

pub struct Prompt {
    pub kind: PromptKind,
    pub buf: String,
}

impl PromptKind {
    pub fn title(self) -> &'static str {
        match self {
            PromptKind::Autotype => "Autotype — file path or script text (e.g. {ADR}2000{DAT}3E)",
            PromptKind::MemoryAddress => "Memory view address (hex)",
            PromptKind::Breakpoint => "Toggle breakpoint at address (hex)",
        }
    }
}

/// Single machine-cycle stepping, like the POLY880's MCYCL/CYCL keys.
/// We execute a whole instruction, then reveal its bus cycles one by one.
#[derive(Default)]
pub struct CycleStepper {
    pub queue: VecDeque<BusCycle>,
    pub current: Option<BusCycle>,
    /// Whether the emulation was running when the mode was entered; leaving
    /// the mode (releasing WAIT on the real machine) resumes it.
    pub resume: bool,
}

pub struct App {
    pub emu: Emulator,
    pub running: bool,
    pub speed: usize,
    pub quit: bool,
    pub help: bool,
    /// Block diagram overlay (as printed on the POLY880's panel).
    pub diagram: bool,
    pub prompt: Option<Prompt>,
    pub message: Option<(String, Instant)>,
    pub bottom: BottomView,
    pub mem_addr: u16,
    pub state_path: PathBuf,
    pub key_release_events: bool,
    /// Rendered key positions (filled in by the UI each frame).
    pub key_rects: Vec<(Rect, KeyId)>,
    /// Rendered toolbar buttons (filled in by the UI each frame).
    pub button_rects: Vec<(Rect, Action)>,
    mouse_key: Option<KeyId>,
    host_keys: HashMap<KeyCode, KeyId>,
    pub cycle_step: Option<CycleStepper>,

    // Per-frame snapshots taken before drawing.
    pub frame: DisplayFrame,
    pub leds: Vec<f32>,
    pub activity: BusActivity,
    /// Smoothed per-chip bus activity (0..1) by board designator.
    pub chip_glow: HashMap<&'static str, f32>,
    pub emulated_hz: f64,
    pub emulated_time: Duration,
    last_tick: Instant,
    last_cycles: u64,
    esc_armed: bool,
}

impl App {
    pub fn new(emu: Emulator, state_path: PathBuf, running: bool) -> Self {
        let digits = emu.machine.info().digits;
        App {
            mem_addr: if emu.machine.info().family == "lc80" {
                0x2000
            } else {
                0x4000
            },
            emu,
            running,
            speed: 4,
            quit: false,
            help: false,
            diagram: false,
            prompt: None,
            message: None,
            bottom: BottomView::Trace,
            state_path,
            key_release_events: false,
            key_rects: Vec::new(),
            button_rects: Vec::new(),
            mouse_key: None,
            host_keys: HashMap::new(),
            cycle_step: None,
            frame: DisplayFrame {
                digits: vec![[0.0; 8]; digits],
            },
            leds: Vec::new(),
            activity: trace::levels(None, false, false, false),
            chip_glow: HashMap::new(),
            emulated_hz: 0.0,
            emulated_time: Duration::ZERO,
            last_tick: Instant::now(),
            last_cycles: 0,
            esc_armed: false,
        }
    }

    pub fn notify(&mut self, msg: impl Into<String>) {
        self.message = Some((msg.into(), Instant::now()));
    }

    pub fn speed_factor(&self) -> f64 {
        SPEEDS[self.speed]
    }

    /// Advance emulation by one UI frame and take display/LED snapshots.
    pub fn tick(&mut self, frame: Duration) {
        if self.running && self.cycle_step.is_none() {
            let clock = self.emu.machine.clock_hz() as f64;
            let factor = self.speed_factor();
            let stop = if factor.is_finite() {
                let cycles = (clock * frame.as_secs_f64() * factor).max(1.0) as u64;
                self.emu.run(cycles)
            } else {
                // As fast as possible within ~80% of the frame budget.
                let deadline = Instant::now() + frame.mul_f64(0.8);
                let chunk = (clock / 100.0) as u64;
                let mut r = StopReason::Budget;
                while Instant::now() < deadline && r == StopReason::Budget {
                    r = self.emu.run(chunk);
                }
                r
            };
            if let StopReason::Breakpoint(pc) = stop {
                self.running = false;
                self.notify(format!("breakpoint at {pc:04X}h"));
            }
        }

        let now = Instant::now();
        let dt = now.duration_since(self.last_tick).as_secs_f64();
        if dt >= 0.5 {
            let c = self.emu.machine.cycles();
            self.emulated_hz = (c - self.last_cycles) as f64 / dt;
            self.last_cycles = c;
            self.last_tick = now;
        }

        self.frame = self.emu.machine.display_frame();
        self.leds = self.emu.machine.leds();
        let ran = self.running && self.cycle_step.is_none();
        for (name, share) in self.emu.machine.chip_activity() {
            let target = if ran { share.sqrt() } else { 0.0 };
            let g = self.chip_glow.entry(name).or_insert(0.0);
            *g = *g * 0.7 + target * 0.3;
        }
        let tr = self.emu.machine.trace_mut();
        self.activity = match &self.cycle_step {
            Some(cs) => trace::levels(cs.current.as_ref(), tr.halt, tr.int, tr.nmi),
            None if self.running => tr.take_activity(),
            None => tr.static_levels(),
        };
        if self.cycle_step.is_some()
            && !self.leds.is_empty()
            && self.emu.machine.info().family == "poly880"
        {
            self.leds[0] = 1.0;
        }
        if let Some((_, t)) = &self.message
            && t.elapsed() > Duration::from_secs(5)
        {
            self.message = None;
        }
    }

    fn step_instruction(&mut self) {
        self.running = false;
        self.emu.machine.trace_mut().freeze();
        self.emu.step();
    }

    fn toggle_cycle_mode(&mut self) {
        if let Some(cs) = self.cycle_step.take() {
            self.running = cs.resume;
            self.emu.machine.trace_mut().freeze();
            self.notify(if cs.resume {
                "machine-cycle stepping off, running"
            } else {
                "machine-cycle stepping off"
            });
        } else {
            let resume = self.running;
            self.running = false;
            self.emu.machine.trace_mut().freeze();
            self.cycle_step = Some(CycleStepper {
                resume,
                ..Default::default()
            });
            self.notify("machine-cycle stepping: F3 / CYCL advances one bus cycle");
        }
    }

    fn step_cycle(&mut self) {
        let Some(cs) = &mut self.cycle_step else {
            return;
        };
        if cs.queue.is_empty() {
            let t0 = self.emu.machine.cycles();
            self.emu.step();
            cs.queue = self
                .emu
                .machine
                .trace()
                .recent(64)
                .into_iter()
                .filter(|c| c.t >= t0)
                .collect();
        }
        cs.current = cs.queue.pop_front();
    }

    fn save_state(&mut self) {
        let json = self.emu.machine.save_state();
        match std::fs::write(&self.state_path, json) {
            Ok(()) => self.notify(format!("state saved to {}", self.state_path.display())),
            Err(e) => self.notify(format!("save failed: {e}")),
        }
    }

    fn load_state(&mut self) {
        let r = std::fs::read_to_string(&self.state_path)
            .map_err(|e| e.to_string())
            .and_then(|j| self.emu.machine.load_state(&j));
        match r {
            Ok(()) => {
                self.emu.machine.trace_mut().clear();
                self.notify(format!("state loaded from {}", self.state_path.display()))
            }
            Err(e) => self.notify(format!("load failed: {e}")),
        }
    }

    /// Press or release a machine key (from keyboard, mouse or autotype).
    fn machine_key(&mut self, key: KeyId, down: bool) {
        let def = &self.emu.machine.info().keys[key];
        if def.wire == KeyWire::None {
            if down {
                match def.label {
                    "M CYCL" => self.toggle_cycle_mode(),
                    "CYCL" => self.step_cycle(),
                    _ => {}
                }
            }
            return;
        }
        self.emu.machine.set_key(key, down);
    }

    /// A key from a terminal that cannot report releases: type it as a tap.
    fn tap(&mut self, key: KeyId) {
        if self.emu.machine.info().keys[key].wire == KeyWire::None {
            self.machine_key(key, true);
            return;
        }
        let now = self.emu.machine.cycles();
        self.emu
            .autotype
            .start(vec![emulatron_machines::autotype::Token::Key(key)], now);
        if !self.running {
            self.notify("emulation paused — key queued");
        }
    }

    pub fn handle(&mut self, ev: Event) {
        match ev {
            Event::Key(k) => self.handle_key(k),
            Event::Mouse(m) => self.handle_mouse(m),
            _ => {}
        }
    }

    fn handle_mouse(&mut self, m: MouseEvent) {
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let p = Position::new(m.column, m.row);
                if self.prompt.is_some() {
                    return;
                }
                if let Some(&(_, a)) = self.button_rects.iter().find(|(r, _)| r.contains(p)) {
                    self.action(a);
                    return;
                }
                if self.help || self.diagram {
                    self.help = false;
                    self.diagram = false;
                    return;
                }
                if let Some(&(_, k)) = self.key_rects.iter().find(|(r, _)| r.contains(p)) {
                    self.mouse_key = Some(k);
                    self.machine_key(k, true);
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if let Some(k) = self.mouse_key.take() {
                    self.machine_key(k, false);
                }
            }
            _ => {}
        }
    }

    fn handle_prompt(&mut self, k: KeyEvent) {
        let Some(p) = &mut self.prompt else { return };
        match k.code {
            KeyCode::Esc => self.prompt = None,
            KeyCode::Backspace => {
                p.buf.pop();
            }
            KeyCode::Char(c) => p.buf.push(c),
            KeyCode::Enter => {
                let p = self.prompt.take().unwrap();
                let text = p.buf.trim().to_string();
                match p.kind {
                    PromptKind::Autotype => {
                        let path = PathBuf::from(&text);
                        let script = if path.is_file() {
                            match std::fs::read_to_string(&path) {
                                Ok(s) => s,
                                Err(e) => return self.notify(format!("{e}")),
                            }
                        } else {
                            text
                        };
                        match self.emu.type_script(&script) {
                            Ok(()) => {
                                self.running = true;
                                self.notify("autotyping…")
                            }
                            Err(e) => self.notify(format!("autotype: {e}")),
                        }
                    }
                    PromptKind::MemoryAddress => match parse_hex(&text) {
                        Some(a) => {
                            self.mem_addr = a;
                            self.bottom = BottomView::Memory;
                        }
                        None => self.notify("bad address"),
                    },
                    PromptKind::Breakpoint => match parse_hex(&text) {
                        Some(a) => {
                            if self.emu.breakpoints.remove(&a) {
                                self.notify(format!("breakpoint {a:04X}h removed"));
                            } else {
                                self.emu.breakpoints.insert(a);
                                self.notify(format!("breakpoint {a:04X}h set"));
                            }
                        }
                        None => self.notify("bad address"),
                    },
                }
            }
            _ => {}
        }
    }

    fn handle_key(&mut self, k: KeyEvent) {
        if self.prompt.is_some() {
            if k.kind != KeyEventKind::Release {
                self.handle_prompt(k);
            }
            return;
        }

        // Releases (only with keyboard enhancement).
        if k.kind == KeyEventKind::Release {
            if let Some(key) = self.host_keys.remove(&k.code) {
                self.machine_key(key, false);
            }
            return;
        }
        if k.kind == KeyEventKind::Repeat {
            return;
        }

        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        if k.code != KeyCode::Esc {
            self.esc_armed = false;
        }
        match k.code {
            KeyCode::Char('c' | 'q') if ctrl => self.quit = true,
            KeyCode::Char('b') if ctrl => self.action(Action::Diagram),
            KeyCode::Esc => {
                if self.help {
                    self.help = false;
                } else if self.diagram {
                    self.diagram = false;
                } else if self.esc_armed {
                    self.quit = true;
                } else {
                    self.esc_armed = true;
                    self.notify("press Esc again to quit");
                }
            }
            KeyCode::F(n @ 1..=12) => self.action(Action::from_fkey(n)),
            KeyCode::Tab => self.action(Action::ToggleView),
            KeyCode::PageUp => self.mem_addr = self.mem_addr.wrapping_sub(0x40),
            KeyCode::PageDown => self.mem_addr = self.mem_addr.wrapping_add(0x40),
            KeyCode::Delete if self.emu.autotype.active() => {
                self.emu.autotype.cancel(self.emu.machine.as_mut());
                self.notify("autotype cancelled");
            }
            code => {
                let c = match code {
                    KeyCode::Char(c) => Some(c),
                    KeyCode::Enter => Some('\r'),
                    KeyCode::Backspace => Some('\x08'),
                    _ => None,
                };
                if let Some(key) = c.and_then(|c| self.emu.machine.info().key_for_char(c)) {
                    if self.key_release_events {
                        self.host_keys.insert(code, key);
                        self.machine_key(key, true);
                    } else {
                        self.tap(key);
                    }
                }
            }
        }
    }

    /// Execute a UI action (from a function key or a toolbar click).
    pub fn action(&mut self, a: Action) {
        match a {
            Action::Help => self.help = !self.help,
            Action::RunStop => {
                self.running = !self.running;
                if self.running {
                    self.cycle_step = None;
                }
                self.emu.machine.trace_mut().freeze();
            }
            Action::Step => {
                if self.cycle_step.is_some() {
                    self.step_cycle()
                } else {
                    self.step_instruction()
                }
            }
            Action::CycleMode => self.toggle_cycle_mode(),
            Action::Save => self.save_state(),
            Action::Load => self.load_state(),
            Action::Autotype => self.open_prompt(PromptKind::Autotype),
            Action::Speed => {
                self.speed = (self.speed + 1) % SPEEDS.len();
                self.notify(format!("speed: {}", speed_label(self.speed_factor())));
            }
            Action::Reset => self.press_label("RES"),
            Action::Nmi => {
                let l = if self.emu.machine.info().family == "lc80" {
                    "NMI"
                } else {
                    "MON"
                };
                self.press_label(l);
            }
            Action::Breakpoint => self.open_prompt(PromptKind::Breakpoint),
            Action::MemAddr => self.open_prompt(PromptKind::MemoryAddress),
            Action::ToggleView => {
                self.bottom = match self.bottom {
                    BottomView::Trace => BottomView::Memory,
                    BottomView::Memory => BottomView::Trace,
                }
            }
            Action::Diagram => self.diagram = !self.diagram,
            Action::Quit => self.quit = true,
        }
    }

    fn open_prompt(&mut self, kind: PromptKind) {
        self.prompt = Some(Prompt {
            kind,
            buf: String::new(),
        });
    }

    /// RES/NMI from function keys: hold for a short while via autotype.
    fn press_label(&mut self, label: &str) {
        if let Some(k) = self.emu.machine.info().key_by_label(label) {
            self.tap(k);
            if label == "RES" {
                self.running = true;
                self.cycle_step = None;
            }
        }
    }
}

pub fn speed_label(f: f64) -> String {
    if f.is_infinite() {
        "max".into()
    } else if f >= 1.0 {
        format!("{f:.0}×")
    } else {
        format!("{f}×")
    }
}

fn parse_hex(s: &str) -> Option<u16> {
    u16::from_str_radix(
        s.trim()
            .trim_start_matches("0x")
            .trim_end_matches(['h', 'H']),
        16,
    )
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use emulatron_machines::create;
    use emulatron_machines::roms::RomImages;

    fn app(variant: &str) -> Option<App> {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../roms");
        let roms = RomImages::from_dir(std::path::Path::new(dir)).ok()?;
        let m = create(variant, &roms).ok()?;
        Some(App::new(
            Emulator::new(m),
            std::env::temp_dir().join(format!("emulatron-test-{variant}.json")),
            true,
        ))
    }

    fn press(app: &mut App, code: KeyCode) {
        app.handle(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
    }

    fn ctrl(app: &mut App, c: char) {
        app.handle(Event::Key(KeyEvent::new(
            KeyCode::Char(c),
            KeyModifiers::CONTROL,
        )));
    }

    fn type_text(app: &mut App, s: &str) {
        for c in s.chars() {
            press(app, KeyCode::Char(c));
        }
    }

    fn frames(app: &mut App, n: usize) {
        for _ in 0..n {
            app.tick(Duration::from_millis(20));
        }
    }

    #[test]
    fn host_keys_tap_machine_keys() {
        let Some(mut a) = app("poly880") else { return };
        let exec = a.emu.machine.info().key_by_label("EXEC").unwrap();
        press(&mut a, KeyCode::Enter);
        frames(&mut a, 1);
        assert!(a.emu.machine.key_down(exec), "Enter = EXEC, held briefly");
        frames(&mut a, 10);
        assert!(!a.emu.machine.key_down(exec), "and released again");
        // Characters without a key are ignored.
        press(&mut a, KeyCode::Char('Z'));
        assert!(!a.emu.autotype.active());
    }

    #[test]
    fn key_release_events_hold_keys() {
        let Some(mut a) = app("lc80") else { return };
        a.key_release_events = true;
        let k = a.emu.machine.info().key_by_label("5").unwrap();
        press(&mut a, KeyCode::Char('5'));
        frames(&mut a, 20);
        assert!(a.emu.machine.key_down(k), "held until released");
        a.handle(Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('5'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        )));
        assert!(!a.emu.machine.key_down(k));
    }

    #[test]
    fn quitting_needs_double_esc_or_ctrl_q() {
        let Some(mut a) = app("lc80") else { return };
        press(&mut a, KeyCode::Esc);
        assert!(!a.quit);
        press(&mut a, KeyCode::Char('1'));
        press(&mut a, KeyCode::Esc);
        assert!(!a.quit, "another key in between disarms");
        press(&mut a, KeyCode::Esc);
        assert!(a.quit);
        let Some(mut a) = app("lc80") else { return };
        ctrl(&mut a, 'q');
        assert!(a.quit);
    }

    #[test]
    fn ctrl_b_toggles_the_block_diagram_and_esc_closes_it() {
        let Some(mut a) = app("poly880") else { return };
        ctrl(&mut a, 'b');
        assert!(a.diagram);
        ctrl(&mut a, 'b');
        assert!(!a.diagram);
        ctrl(&mut a, 'b');
        press(&mut a, KeyCode::Esc);
        assert!(
            !a.diagram && !a.quit,
            "Esc closes the overlay, doesn't arm quit"
        );
        press(&mut a, KeyCode::Esc);
        assert!(!a.quit);
    }

    #[test]
    fn esc_closes_help_first() {
        let Some(mut a) = app("lc80") else { return };
        press(&mut a, KeyCode::F(1));
        assert!(a.help);
        press(&mut a, KeyCode::Esc);
        press(&mut a, KeyCode::Esc);
        assert!(!a.help && !a.quit);
    }

    #[test]
    fn breakpoint_and_memory_prompts() {
        let Some(mut a) = app("lc80") else { return };
        press(&mut a, KeyCode::F(11));
        type_text(&mut a, "0895h");
        press(&mut a, KeyCode::Enter);
        assert!(a.emu.breakpoints.contains(&0x0895));
        frames(&mut a, 600);
        assert!(!a.running, "breakpoint stopped the run");
        assert_eq!(a.emu.machine.cpu().regs.pc, 0x0895);
        press(&mut a, KeyCode::F(11));
        type_text(&mut a, "895");
        press(&mut a, KeyCode::Enter);
        assert!(a.emu.breakpoints.is_empty(), "toggled off");

        press(&mut a, KeyCode::F(12));
        type_text(&mut a, "23x0");
        press(&mut a, KeyCode::Backspace);
        press(&mut a, KeyCode::Backspace);
        type_text(&mut a, "00");
        press(&mut a, KeyCode::Enter);
        assert_eq!(a.mem_addr, 0x2300);
        assert!(a.bottom == BottomView::Memory);

        press(&mut a, KeyCode::F(7));
        press(&mut a, KeyCode::Esc);
        assert!(a.prompt.is_none() && !a.quit, "Esc cancels a prompt");
    }

    #[test]
    fn autotype_prompt_accepts_inline_scripts() {
        let Some(mut a) = app("poly880") else { return };
        frames(&mut a, 50);
        press(&mut a, KeyCode::F(7));
        type_text(&mut a, "{MEM}4000{EXEC}42{EXEC}");
        press(&mut a, KeyCode::Enter);
        assert!(a.emu.autotype.active());
        frames(&mut a, 200);
        assert_eq!(a.emu.machine.peek(0x4000), 0x42);

        press(&mut a, KeyCode::F(7));
        type_text(&mut a, "{NOPE}");
        press(&mut a, KeyCode::Enter);
        assert!(a.message.as_ref().unwrap().0.contains("no key"));
    }

    #[test]
    fn save_and_load_state_keys() {
        let Some(mut a) = app("lc80") else { return };
        frames(&mut a, 10);
        press(&mut a, KeyCode::F(5));
        let t = a.emu.machine.cycles();
        frames(&mut a, 10);
        press(&mut a, KeyCode::F(6));
        assert_eq!(a.emu.machine.cycles(), t);
        let _ = std::fs::remove_file(&a.state_path);
    }

    #[test]
    fn poly880_keypad_cycle_keys_step_bus_cycles() {
        let Some(mut a) = app("poly880") else { return };
        frames(&mut a, 20);
        let mcycl = a.emu.machine.info().key_by_label("M CYCL").unwrap();
        let cycl = a.emu.machine.info().key_by_label("CYCL").unwrap();
        a.machine_key(mcycl, true);
        a.machine_key(mcycl, false);
        assert!(a.cycle_step.is_some() && !a.running);
        frames(&mut a, 1);
        assert_eq!(a.leds[0], 1.0, "MCYCL LED lit");
        let t0 = a.emu.machine.cycles();
        a.machine_key(cycl, true);
        let t1 = a.emu.machine.cycles();
        assert!(t1 > t0, "first CYCL executes an instruction");
        let first = a.cycle_step.as_ref().unwrap().current.unwrap();
        assert_eq!(first.kind, emulatron_machines::trace::CycleKind::Fetch);
        // Further presses reveal the remaining cycles of that instruction
        // before the next one runs.
        let pending = a.cycle_step.as_ref().unwrap().queue.len();
        for _ in 0..pending {
            a.machine_key(cycl, true);
        }
        assert_eq!(a.emu.machine.cycles(), t1);
        frames(&mut a, 1);
        let c = a.cycle_step.as_ref().unwrap().current.unwrap();
        assert_eq!(
            a.activity.lines[0],
            (c.addr & 1) as f32,
            "bus LEDs show that cycle"
        );
        a.machine_key(mcycl, true);
        assert!(a.cycle_step.is_none() && a.running);
    }

    #[test]
    fn mouse_holds_key_until_release() {
        let Some(mut a) = app("lc80") else { return };
        let k = a.emu.machine.info().key_by_label("ADR").unwrap();
        let r = Rect::new(10, 10, 5, 1);
        a.key_rects = vec![(r, k)];
        let ev = |kind| {
            Event::Mouse(MouseEvent {
                kind,
                column: 11,
                row: 10,
                modifiers: KeyModifiers::NONE,
            })
        };
        a.handle(ev(MouseEventKind::Down(MouseButton::Left)));
        frames(&mut a, 30);
        assert!(a.emu.machine.key_down(k));
        a.handle(ev(MouseEventKind::Up(MouseButton::Left)));
        assert!(!a.emu.machine.key_down(k));
    }

    #[test]
    fn speed_cycles_through_settings() {
        let Some(mut a) = app("lc80") else { return };
        let start = a.speed;
        for _ in 0..SPEEDS.len() {
            press(&mut a, KeyCode::F(8));
        }
        assert_eq!(a.speed, start);
        a.speed = 1; // 0.1x
        let t = a.emu.machine.cycles();
        frames(&mut a, 50); // one second
        let ran = a.emu.machine.cycles() - t;
        assert!((80_000..=100_000).contains(&ran), "0.1x of 900 kHz: {ran}");
    }
}
