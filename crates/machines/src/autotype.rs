//! Autotype: let the emulator type in programs, the way we once did by hand.
//!
//! Script syntax:
//! * every character that a key types (see [`crate::KeyDef::chars`]), e.g.
//!   hex digits, `+ - , .` on the LC-80, `M X K G R S T` on the POLY880
//! * `{LABEL}` presses a key by its label: `{ADR}`, `{EXEC}`, `{RES}`, `{MON}`
//! * `{wait 500}` pauses for 500 ms of emulated time
//! * whitespace is ignored; `#` starts a comment until the end of the line
//!
//! Timing is in emulated T-states, so typing is deterministic and works at
//! any emulation speed.

use std::collections::VecDeque;

use crate::machine::{KeyId, Machine, MachineInfo};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Token {
    Key(KeyId),
    /// Milliseconds.
    Wait(u32),
}

pub fn parse(info: &MachineInfo, script: &str) -> Result<Vec<Token>, String> {
    let mut out = Vec::new();
    let mut chars = script.chars().peekable();
    let mut line = 1;
    while let Some(c) = chars.next() {
        match c {
            '\n' => line += 1,
            c if c.is_whitespace() => {}
            '#' => {
                while let Some(&n) = chars.peek() {
                    if n == '\n' {
                        break;
                    }
                    chars.next();
                }
            }
            '{' => {
                let mut name = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(c) => name.push(c),
                        None => return Err(format!("line {line}: unterminated '{{'")),
                    }
                }
                let lower = name.trim().to_ascii_lowercase();
                if let Some(ms) = lower.strip_prefix("wait") {
                    let ms = ms.trim().trim_end_matches("ms").trim();
                    let ms = ms
                        .parse()
                        .map_err(|_| format!("line {line}: bad wait '{name}'"))?;
                    out.push(Token::Wait(ms));
                } else {
                    let k = info
                        .key_by_label(&name)
                        .ok_or_else(|| format!("line {line}: no key '{name}'"))?;
                    out.push(Token::Key(k));
                }
            }
            c => {
                let k = info
                    .key_for_char(c)
                    .ok_or_else(|| format!("line {line}: no key types '{c}'"))?;
                out.push(Token::Key(k));
            }
        }
    }
    Ok(out)
}

/// Generate a script that types `bytes` into memory at `addr` using the
/// machine's monitor.
pub fn enter_bytes(family: &str, addr: u16, bytes: &[u8]) -> String {
    let mut s = String::new();
    match family {
        "lc80" => {
            // ADR aaaa DAT dd + dd + ...
            s.push_str(&format!("{{ADR}}{addr:04X}{{DAT}}"));
            for b in bytes {
                s.push_str(&format!("{b:02X}+"));
            }
        }
        _ => {
            // MEM aaaa EXEC dd EXEC dd EXEC ...
            s.push_str(&format!("{{MEM}}{addr:04X}{{EXEC}}"));
            for b in bytes {
                s.push_str(&format!("{b:02X}{{EXEC}}"));
            }
        }
    }
    s
}

#[derive(Debug)]
pub struct Autotype {
    pending: VecDeque<Token>,
    held: Option<(KeyId, u64)>,
    next_t: u64,
    /// Key hold time in ms.
    pub hold_ms: u32,
    /// Pause between keys in ms.
    pub gap_ms: u32,
    total: usize,
}

impl Default for Autotype {
    fn default() -> Self {
        Autotype {
            pending: VecDeque::new(),
            held: None,
            next_t: 0,
            hold_ms: 70,
            gap_ms: 70,
            total: 0,
        }
    }
}

impl Autotype {
    pub fn start(&mut self, tokens: Vec<Token>, now: u64) {
        self.pending.extend(tokens);
        self.total = self.pending.len() + self.held.is_some() as usize;
        if self.held.is_none() {
            self.next_t = self.next_t.max(now);
        }
    }

    pub fn cancel(&mut self, m: &mut dyn Machine) {
        self.pending.clear();
        if let Some((k, _)) = self.held.take() {
            m.set_key(k, false);
        }
    }

    pub fn active(&self) -> bool {
        self.held.is_some() || !self.pending.is_empty()
    }

    /// (remaining, total) tokens.
    pub fn progress(&self) -> (usize, usize) {
        (
            self.pending.len() + self.held.is_some() as usize,
            self.total,
        )
    }

    fn ms(m: &dyn Machine, ms: u32) -> u64 {
        m.clock_hz() as u64 * ms as u64 / 1000
    }

    /// Apply all key events due at the machine's current time.
    pub fn service(&mut self, m: &mut dyn Machine) {
        let now = m.cycles();
        loop {
            if let Some((k, release)) = self.held {
                if now < release {
                    return;
                }
                m.set_key(k, false);
                self.held = None;
                self.next_t = release + Self::ms(m, self.gap_ms);
            }
            if now < self.next_t {
                return;
            }
            match self.pending.pop_front() {
                None => return,
                Some(Token::Key(k)) => {
                    m.set_key(k, true);
                    self.held = Some((k, now + Self::ms(m, self.hold_ms)));
                }
                Some(Token::Wait(ms)) => self.next_t = now + Self::ms(m, ms),
            }
        }
    }

    /// T-state of the next scheduled event, if any.
    pub fn next_event(&self) -> Option<u64> {
        match self.held {
            Some((_, r)) => Some(r),
            None if !self.pending.is_empty() => Some(self.next_t),
            None => None,
        }
    }
}
