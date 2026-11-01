//! `emulatron` — terminal front-end for the LC-80 and POLY880 emulators.

mod app;
mod ui;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::Parser;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, KeyboardEnhancementFlags,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::{execute, terminal};

use emulatron_machines::roms::{self, RomImages};
use emulatron_machines::{Emulator, create};

#[derive(Parser, Debug)]
#[command(version, about = "LC-80 and POLY880 learning computer emulators")]
struct Args {
    /// Machine variant (lc80, lc80a, lc80e, lc80_2, poly880, poly880s).
    #[arg(short, long)]
    machine: Option<String>,

    /// Directory with ROM dumps (matched by SHA-1, then by file name).
    #[arg(short, long, env = "EMULATRON_ROMS", default_value = "roms")]
    roms: PathBuf,

    /// List known variants and which ROMs were found, then exit.
    #[arg(long)]
    list: bool,

    /// Restore a save state at start.
    #[arg(short, long)]
    state: Option<PathBuf>,

    /// Type in an autotype script at start (see README for syntax).
    #[arg(short, long)]
    autotype: Option<PathBuf>,

    /// Load a binary into RAM: FILE@ADDR (hex), e.g. prog.bin@2000.
    #[arg(short, long)]
    load: Vec<String>,

    /// Type a binary in through the monitor keypad: FILE@ADDR (hex).
    #[arg(short = 't', long)]
    type_bin: Vec<String>,

    /// Start with the block diagram overlay open (toggle with Ctrl-B).
    #[arg(long)]
    diagram: bool,

    /// Start with the memory view instead of the bus trace.
    #[arg(long)]
    memory: bool,

    /// Start paused.
    #[arg(long)]
    paused: bool,

    /// Headless: run N emulated milliseconds, print one rendered frame
    /// (WIDTHxHEIGHT via --size) as plain text and exit.
    #[arg(long, value_name = "MS")]
    dump_frame: Option<u64>,

    /// With --dump-frame: also write the frame as a colour HTML page
    /// (used for the README screenshots).
    #[arg(long, value_name = "FILE")]
    html: Option<PathBuf>,

    /// Terminal size for --dump-frame.
    #[arg(long, default_value = "160x50")]
    size: String,
}

fn parse_file_at(s: &str) -> Result<(PathBuf, u16)> {
    let (f, a) = s.rsplit_once('@').context("expected FILE@ADDR")?;
    let a = u16::from_str_radix(a.trim_start_matches("0x").trim_end_matches(['h', 'H']), 16)
        .with_context(|| format!("bad address '{a}'"))?;
    Ok((PathBuf::from(f), a))
}

fn list(roms: &RomImages) {
    let avail = roms.available_variants();
    for v in roms::VARIANTS {
        let ok = avail.iter().any(|a| a.id == v.id);
        println!("{} {:<9} {}", if ok { "✔" } else { "✘" }, v.id, v.name);
        if !ok {
            for f in v.files {
                println!(
                    "      needs {:<24} {} bytes  sha1 {}",
                    f.name, f.size, f.sha1
                );
            }
        }
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let roms = if args.roms.is_dir() {
        RomImages::from_dir(&args.roms)?
    } else {
        RomImages::new()
    };

    if args.list {
        list(&roms);
        return Ok(());
    }

    let variant = match &args.machine {
        Some(m) => m.clone(),
        None => match roms.available_variants().first() {
            Some(v) => v.id.to_string(),
            None => {
                eprintln!(
                    "No ROMs found in '{}'. Expected one of:\n",
                    args.roms.display()
                );
                list(&roms);
                bail!(
                    "put the ROM dumps into '{}' or pass --roms DIR",
                    args.roms.display()
                );
            }
        },
    };

    let v = roms::variant(&variant).with_context(|| format!("unknown variant '{variant}'"))?;
    let image = roms.build_image(v).map_err(anyhow::Error::msg)?;
    let machine = create(&variant, &roms).map_err(anyhow::Error::msg)?;
    let mut emu = Emulator::new(machine);

    let mut messages: Vec<String> = image.warnings;
    if let Some(p) = &args.state {
        let json =
            std::fs::read_to_string(p).with_context(|| format!("reading {}", p.display()))?;
        emu.machine.load_state(&json).map_err(anyhow::Error::msg)?;
        messages.push(format!("state restored from {}", p.display()));
    }
    for l in &args.load {
        let (f, a) = parse_file_at(l)?;
        let data = std::fs::read(&f).with_context(|| format!("reading {}", f.display()))?;
        emu.load_bytes(a, &data).map_err(anyhow::Error::msg)?;
        messages.push(format!("loaded {} bytes at {a:04X}h", data.len()));
    }
    for l in &args.type_bin {
        let (f, a) = parse_file_at(l)?;
        let data = std::fs::read(&f).with_context(|| format!("reading {}", f.display()))?;
        emu.type_bytes(a, &data).map_err(anyhow::Error::msg)?;
        messages.push(format!("typing {} bytes at {a:04X}h", data.len()));
    }
    if let Some(p) = &args.autotype {
        let script =
            std::fs::read_to_string(p).with_context(|| format!("reading {}", p.display()))?;
        emu.type_script(&script).map_err(anyhow::Error::msg)?;
        messages.push(format!("autotyping {}", p.display()));
    }

    let state_path = args
        .state
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("{variant}.snap.json")));
    let mut app = app::App::new(emu, state_path, !args.paused);
    if let Some(m) = messages.pop() {
        app.notify(m);
    }

    app.diagram = args.diagram;
    if args.memory {
        app.bottom = app::BottomView::Memory;
    }
    if let Some(ms) = args.dump_frame {
        return dump_frame(&mut app, ms, &args.size, args.html.as_deref());
    }

    let mut terminal = ratatui::init();
    execute!(std::io::stdout(), EnableMouseCapture)?;
    let enhanced = terminal::supports_keyboard_enhancement().unwrap_or(false);
    if enhanced {
        execute!(
            std::io::stdout(),
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                    | KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
            )
        )?;
    }
    app.key_release_events = enhanced;

    let result = run(&mut terminal, &mut app);

    if enhanced {
        let _ = execute!(std::io::stdout(), PopKeyboardEnhancementFlags);
    }
    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

fn dump_frame(
    app: &mut app::App,
    ms: u64,
    size: &str,
    html: Option<&std::path::Path>,
) -> Result<()> {
    use ratatui::backend::TestBackend;
    let (w, h) = size.split_once('x').context("size must be WxH")?;
    let mut term = ratatui::Terminal::new(TestBackend::new(w.parse()?, h.parse()?))?;
    let frames = (ms / 20).max(1);
    for _ in 0..frames {
        app.tick(FRAME);
    }
    // Wall-clock speed is meaningless headless; show the nominal rate.
    app.emulated_hz = app.emu.machine.clock_hz() as f64 * app.speed_factor();
    term.draw(|f| ui::draw(f, app))?;
    eprintln!(
        "display: [{}]  leds: {:?}  glow: {:?}",
        app.frame.text(),
        app.leds,
        app.chip_glow
    );
    let buf = term.backend().buffer();
    for y in 0..buf.area.height {
        let line: String = (0..buf.area.width)
            .map(|x| buf[(x, y)].symbol().to_string())
            .collect();
        println!("{}", line.trim_end());
    }
    if let Some(path) = html {
        std::fs::write(path, buffer_html(buf, app.emu.machine.info().title))?;
    }
    Ok(())
}

/// Render a buffer as HTML with one span per run of equally styled cells.
fn buffer_html(buf: &ratatui::buffer::Buffer, title: &str) -> String {
    use ratatui::style::{Color, Modifier};
    const FG: &str = "#c9d1d9";
    const BG: &str = "#0d1117";
    fn css(c: Color, default: &str) -> String {
        match c {
            Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
            Color::Reset => default.into(),
            Color::Black => "#000000".into(),
            Color::Red => "#e5534b".into(),
            Color::Green => "#57ab5a".into(),
            Color::Yellow => "#c69026".into(),
            Color::Blue => "#539bf5".into(),
            Color::Magenta => "#b083f0".into(),
            Color::Cyan => "#39c5cf".into(),
            Color::Gray => "#adbac7".into(),
            Color::DarkGray => "#636e7b".into(),
            Color::White => "#ffffff".into(),
            Color::Indexed(i) => format!("var(--c{i}, {default})"),
            _ => default.into(),
        }
    }
    let esc = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    let mut out = format!(
        "<!doctype html><meta charset=utf-8><title>{}</title>\n<style>\
         body{{margin:0;background:{BG}}}\
         pre{{margin:0;padding:12px;display:inline-block;background:{BG};color:{FG};\
         font:15px/1.0 'DejaVu Sans Mono',monospace}}\
         span{{display:inline}}</style>\n<pre>",
        esc(title)
    );
    for y in 0..buf.area.height {
        let mut run = String::new();
        let mut style = None;
        for x in 0..buf.area.width {
            let cell = &buf[(x, y)];
            let mut fg = css(cell.fg, FG);
            let mut bg = css(cell.bg, BG);
            if cell.modifier.contains(Modifier::REVERSED) {
                std::mem::swap(&mut fg, &mut bg);
            }
            let bold = cell.modifier.contains(Modifier::BOLD);
            let st = (fg, bg, bold);
            if style.as_ref() != Some(&st) {
                if style.is_some() {
                    out.push_str(&esc(&run));
                    out.push_str("</span>");
                    run.clear();
                }
                out.push_str(&format!(
                    "<span style=\"color:{};background:{}{}\">",
                    st.0,
                    st.1,
                    if st.2 { ";font-weight:bold" } else { "" }
                ));
                style = Some(st);
            }
            run.push_str(cell.symbol());
        }
        out.push_str(&esc(&run));
        out.push_str("</span>\n");
    }
    out.push_str("</pre>\n");
    out
}

const FRAME: Duration = Duration::from_millis(20);

fn run(terminal: &mut ratatui::DefaultTerminal, app: &mut app::App) -> Result<()> {
    let mut next = Instant::now();
    while !app.quit {
        let start = Instant::now();
        app.tick(FRAME);
        app.emulated_time = start.elapsed();
        terminal.draw(|f| ui::draw(f, app))?;
        next += FRAME;
        let now = Instant::now();
        if next < now {
            next = now;
        }
        while let Some(timeout) = next.checked_duration_since(Instant::now()) {
            if !event::poll(timeout)? {
                break;
            }
            app.handle(event::read()?);
            if app.quit {
                break;
            }
        }
    }
    Ok(())
}
