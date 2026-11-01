//! Rendering: faceplates that resemble the real devices, plus the modern
//! views (CPU, bus, peripheral chips) the originals could not show.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Wrap};

use emulatron_chips::ctc::{CTRL_COUNTER, ChannelState};
use emulatron_chips::{DaisyDevice, IrqState, PioMode, PortId};
use emulatron_machines::trace::{self, CycleKind};
use emulatron_machines::{KeyColor, MachineInfo, Region};
use u880::disasm::disassemble;
use u880::flags;

use crate::app::{App, BottomView, speed_label};

// ---- palette -------------------------------------------------------------

const SEG_ON: (u8, u8, u8) = (110, 255, 70); // VQE 23: green
const SEG_OFF: (u8, u8, u8) = (22, 38, 22);
const RED_ON: (u8, u8, u8) = (255, 50, 35);
const RED_OFF: (u8, u8, u8) = (60, 16, 12);
const GREEN_ON: (u8, u8, u8) = (80, 255, 80);
const GREEN_OFF: (u8, u8, u8) = (14, 50, 14);
const POLY_PANEL: Color = Color::Rgb(56, 50, 46);
const POLY_TEXT: Color = Color::Rgb(225, 225, 220);
const BEZEL: Color = Color::Rgb(8, 8, 8);
const DIM: Color = Color::Rgb(110, 110, 110);
const ACCENT: Color = Color::Rgb(255, 200, 80);

fn lerp(off: (u8, u8, u8), on: (u8, u8, u8), b: f32) -> Color {
    let b = b.clamp(0.0, 1.0).powf(0.6);
    let m = |a: u8, c: u8| (a as f32 + (c as f32 - a as f32) * b) as u8;
    Color::Rgb(m(off.0, on.0), m(off.1, on.1), m(off.2, on.2))
}

fn key_colors(c: KeyColor) -> (Color, Color) {
    match c {
        KeyColor::Red => (Color::Rgb(185, 35, 30), Color::White),
        KeyColor::Black => (Color::Rgb(28, 28, 28), Color::Rgb(235, 235, 235)),
        KeyColor::Cream => (Color::Rgb(226, 216, 190), Color::Black),
        KeyColor::Orange => (Color::Rgb(230, 125, 40), Color::Black),
        KeyColor::Grey => (Color::Rgb(175, 175, 170), Color::Black),
        KeyColor::Teal => (Color::Rgb(40, 150, 140), Color::Black),
    }
}

/// Clipped string output.
fn put(buf: &mut Buffer, clip: Rect, x: u16, y: u16, s: &str, style: Style) {
    if y < clip.top() || y >= clip.bottom() || x >= clip.right() || x < clip.left() {
        return;
    }
    let w = (clip.right() - x) as usize;
    buf.set_stringn(x, y, s, w, style);
}

/// Paint a solid rectangle (clears glyphs underneath).
fn fill(buf: &mut Buffer, r: Rect, bg: Color) {
    let r = r.intersection(buf.area);
    let blank = " ".repeat(r.width as usize);
    for y in r.top()..r.bottom() {
        buf.set_string(r.x, y, &blank, Style::new().bg(bg));
    }
}

// ---- top level -----------------------------------------------------------

/// Width of the right-hand column (CPU, peripherals).
const SIDE_W: u16 = 40;
/// Rows the bus panel needs at minimum (border, bus LEDs, a few trace lines).
const BOTTOM_MIN: u16 = 10;
/// The POLY880 window strip shows bus LEDs and display side by side only
/// from this faceplate width on; below, they are stacked.
const POLY_SIDE_BY_SIDE: u16 = 121;

/// Smallest terminal (columns, rows) a machine's UI can be drawn in.
pub fn min_size(info: &MachineInfo) -> (u16, u16) {
    match info.family {
        // Bus panel LEDs (61) + side column; display + keypad fit in that.
        "lc80" => (102, faceplate_height(info, 1, 0) + BOTTOM_MIN + 1),
        // Keypad (81) + border, stacked window strip.
        _ => (123, faceplate_height(info, 1, 0) + BOTTOM_MIN + 1),
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let info = app.emu.machine.info();
    let area = f.area();
    app.key_rects.clear();
    app.button_rects.clear();
    let (min_w, min_h) = min_size(info);
    if area.width < min_w || area.height < min_h {
        draw_too_small(f, app, min_w, min_h);
        return;
    }
    let face_w = area.width - SIDE_W;
    // Key row pitch: 3 = full keycaps, 2 = one-row caps with a gap, 1 = packed.
    let key_h = [3, 2, 1]
        .into_iter()
        .find(|&k| faceplate_height(info, k, face_w) + BOTTOM_MIN < area.height)
        .unwrap_or(1);
    let face_h = faceplate_height(info, key_h, face_w);

    let [main, status] = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);
    let [top, bottom] =
        Layout::vertical([Constraint::Length(face_h), Constraint::Fill(1)]).areas(main);
    let [face, cpu] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(SIDE_W)]).areas(top);
    let [bus, chips] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(SIDE_W)]).areas(bottom);

    match info.family {
        "lc80" => draw_lc80(f, app, face, key_h),
        _ => draw_poly880(f, app, face, key_h),
    }
    draw_cpu(f, app, cpu);
    draw_bus(f, app, bus);
    draw_chips(f, app, chips);
    draw_status(f, app, status);

    if app.diagram {
        draw_diagram(f, app);
    }
    if app.help {
        draw_help(f, app);
    }
    if let Some(p) = &app.prompt {
        let w = area.width.saturating_sub(10).min(90);
        let r = Rect::new(
            area.x + (area.width - w) / 2,
            area.y + area.height / 2 - 2,
            w,
            3,
        );
        f.render_widget(Clear, r);
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .title(p.kind.title())
            .style(Style::new().fg(ACCENT));
        let para = Paragraph::new(format!("{}▏", p.buf))
            .block(block)
            .style(Style::new().fg(Color::White));
        f.render_widget(para, r);
    }
}

fn faceplate_height(info: &MachineInfo, key_h: u16, face_w: u16) -> u16 {
    let base = 2 + 1 + 5 + 1 + 1 + key_h * info.keypad_rows as u16 + 2;
    match info.family {
        "lc80" => base,
        _ if face_w < POLY_SIDE_BY_SIDE => base + 6,
        _ => base,
    }
}

fn draw_too_small(f: &mut Frame, app: &App, min_w: u16, min_h: u16) {
    let area = f.area();
    let info = app.emu.machine.info();
    let warn = |ok: bool| {
        if ok {
            Style::new().fg(Color::Green)
        } else {
            Style::new().fg(Color::Red).add_modifier(Modifier::BOLD)
        }
    };
    let lines = vec![
        Line::from(Span::styled(
            "Terminal too small",
            Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!("{} needs at least {min_w} × {min_h}", info.title)),
        Line::from(vec![
            Span::raw("current: "),
            Span::styled(format!("{}", area.width), warn(area.width >= min_w)),
            Span::raw(" × "),
            Span::styled(format!("{}", area.height), warn(area.height >= min_h)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Resize the terminal · Ctrl-Q quits",
            Style::new().fg(DIM),
        )),
    ];
    let h = lines.len() as u16;
    let y = area.y + area.height.saturating_sub(h) / 2;
    let r = Rect::new(area.x, y, area.width, h.min(area.height));
    f.render_widget(
        Paragraph::new(lines).alignment(ratatui::layout::Alignment::Center),
        r,
    );
}

// ---- 7-segment digits and LEDs ------------------------------------------

/// Draw one digit (6 x 5 cells). `segs` is brightness for a..g, dp.
fn draw_digit(buf: &mut Buffer, clip: Rect, x: u16, y: u16, segs: &[f32; 8]) {
    let c = |i: usize| Style::new().fg(lerp(SEG_OFF, SEG_ON, segs[i])).bg(BEZEL);
    let [a, b, cc, d, e, ff, g, dp] = [0, 1, 2, 3, 4, 5, 6, 7].map(c);
    put(buf, clip, x, y, " ", a);
    put(buf, clip, x + 1, y, "━━━", a);
    put(buf, clip, x, y + 1, "┃", ff);
    put(buf, clip, x + 4, y + 1, "┃", b);
    put(buf, clip, x + 1, y + 2, "━━━", g);
    put(buf, clip, x, y + 3, "┃", e);
    put(buf, clip, x + 4, y + 3, "┃", cc);
    put(buf, clip, x + 1, y + 4, "━━━", d);
    put(buf, clip, x + 5, y + 4, "▪", dp);
}

fn draw_display(buf: &mut Buffer, clip: Rect, x: u16, y: u16, app: &App) {
    let n = app.frame.digits.len() as u16;
    fill(buf, Rect::new(x - 1, y, n * 7 + 1, 5), BEZEL);
    for (i, d) in app.frame.digits.iter().enumerate() {
        draw_digit(buf, clip, x + i as u16 * 7, y, d);
    }
}

fn led(buf: &mut Buffer, clip: Rect, x: u16, y: u16, b: f32, red: bool, bg: Color) {
    let (off, on) = if red {
        (RED_OFF, RED_ON)
    } else {
        (GREEN_OFF, GREEN_ON)
    };
    put(
        buf,
        clip,
        x,
        y,
        "●",
        Style::new().fg(lerp(off, on, b)).bg(bg),
    );
}

// ---- keypads ---------------------------------------------------------------

fn draw_keypad(buf: &mut Buffer, clip: Rect, x: u16, y: u16, app: &mut App, key_h: u16, kw: u16) {
    let info = app.emu.machine.info();
    let compact = key_h < 3;
    let kh = key_h;
    for (id, k) in info.keys.iter().enumerate() {
        let kx = x + k.pos.1 as u16 * (kw + 1);
        let ky = y + k.pos.0 as u16 * kh;
        let r = Rect::new(kx, ky, kw, if compact { 1 } else { 2 });
        let down = app.emu.machine.key_down(id);
        let (mut bg, mut fg) = key_colors(k.color);
        if down {
            bg = Color::Rgb(255, 230, 120);
            fg = Color::Black;
        }
        if r.intersection(clip) != r {
            continue;
        }
        fill(buf, r, bg);
        let label = format!("{:^w$}", k.label, w = kw as usize);
        put(
            buf,
            clip,
            kx,
            ky,
            &label,
            Style::new().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
        );
        if !compact {
            let sub = format!("{:^w$}", k.sub, w = kw as usize);
            let shade = match bg {
                Color::Rgb(r, g, b) => Color::Rgb(
                    r.saturating_sub(30),
                    g.saturating_sub(30),
                    b.saturating_sub(30),
                ),
                c => c,
            };
            put(buf, clip, kx, ky + 1, &sub, Style::new().fg(fg).bg(shade));
        }
        app.key_rects.push((r, id));
    }
}

// ---- LC-80 -------------------------------------------------------------------

// Folder, board and keypad colours of the LC-80.
const FOLDER: Color = Color::Rgb(66, 40, 27);
const FOLDER_GRAIN: Color = Color::Rgb(84, 54, 36);
const SPINE: Color = Color::Rgb(44, 26, 17);
const PCB: Color = Color::Rgb(138, 96, 54);
const SILK: Color = Color::Rgb(232, 224, 200);
const TRACK: Color = Color::Rgb(184, 170, 138);
const ALU: Color = Color::Rgb(182, 184, 182);
const HOUSING: Color = Color::Rgb(20, 20, 21);
const CHIP_IDLE: (u8, u8, u8) = (34, 32, 31);
const CHIP_HOT: (u8, u8, u8) = (110, 92, 48);

/// Cheap deterministic "random" for textures.
fn grain(x: u16, y: u16) -> u32 {
    let mut h = (x as u32).wrapping_mul(374_761_393) ^ (y as u32).wrapping_mul(668_265_263);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^ (h >> 16)
}

/// A DIP package: legs, body with notch and type, legs.
fn draw_chip(buf: &mut Buffer, clip: Rect, x: u16, y: u16, pins: u16, text: &str, glow: f32) {
    let w = pins / 2;
    if x + w > clip.right() || y + 3 > clip.bottom() {
        return;
    }
    let legs = Style::new().fg(Color::Rgb(200, 200, 205)).bg(PCB);
    put(buf, clip, x, y, &"┬".repeat(w as usize), legs);
    put(buf, clip, x, y + 2, &"┴".repeat(w as usize), legs);
    let body = lerp(CHIP_IDLE, CHIP_HOT, glow);
    let label = format!("◖{:^w$}", text, w = w as usize - 1);
    let fg = lerp((150, 150, 150), (255, 240, 200), glow);
    put(buf, clip, x, y + 1, &label, Style::new().fg(fg).bg(body));
}

fn draw_lc80(f: &mut Frame, app: &mut App, area: Rect, key_h: u16) {
    let compact = key_h < 3;
    let buf = f.buffer_mut();
    let area = area.intersection(buf.area);

    // The open leatherette folder with a little grain.
    fill(buf, area, FOLDER);
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if grain(x, y).is_multiple_of(9) {
                put(
                    buf,
                    area,
                    x,
                    y,
                    "·",
                    Style::new().fg(FOLDER_GRAIN).bg(FOLDER),
                );
            }
        }
    }

    // Inner cover (left) with the aluminium label, then the spine.
    let cover_w: u16 = if area.width >= 110 { 22 } else { 0 };
    if cover_w > 0 {
        let label = [
            "VOR FEUCHTIGKEIT",
            "SCHÜTZEN!",
            "KEEP DRY!",
            "",
            "RFT",
            "veb mikroelektronik",
            "»karl marx« erfurt",
            "G-5403.500",
            "MADE IN GDR",
            "Nr. 0 4 7 1 1",
        ];
        let lw = 20;
        let lx = area.x + (cover_w - lw) / 2;
        let ly = area.y + area.height.saturating_sub(label.len() as u16 + 2) / 2;
        let plate = Rect::new(lx, ly, lw, label.len() as u16 + 2).intersection(area);
        fill(buf, plate, ALU);
        for (i, l) in label.iter().enumerate() {
            let mut st = Style::new().fg(Color::Rgb(25, 25, 25)).bg(ALU);
            if *l == "RFT" {
                st = st.add_modifier(Modifier::BOLD);
            }
            put(
                buf,
                area,
                lx,
                ly + 1 + i as u16,
                &format!("{l:^w$}", w = lw as usize),
                st,
            );
        }
        put(
            buf,
            area,
            lx,
            ly,
            "◦",
            Style::new().fg(Color::Rgb(120, 120, 120)).bg(ALU),
        );
        put(
            buf,
            area,
            lx + lw - 1,
            ly,
            "◦",
            Style::new().fg(Color::Rgb(120, 120, 120)).bg(ALU),
        );
        let sx = area.x + cover_w;
        for y in area.top()..area.bottom() {
            put(buf, area, sx, y, "▐", Style::new().fg(SPINE).bg(FOLDER));
            put(
                buf,
                area,
                sx + 1,
                y,
                if y.is_multiple_of(2) { "┊" } else { " " },
                Style::new().fg(FOLDER_GRAIN).bg(SPINE),
            );
        }
    }

    // The printed circuit board.
    let pcb = Rect::new(
        area.x + cover_w + if cover_w > 0 { 3 } else { 1 },
        area.y + 1,
        area.width
            .saturating_sub(cover_w + if cover_w > 0 { 4 } else { 2 }),
        area.height.saturating_sub(2),
    );
    fill(buf, pcb, PCB);
    let hole = Style::new().fg(Color::Rgb(60, 40, 25)).bg(PCB);
    for (x, y) in [
        (pcb.left(), pcb.top()),
        (pcb.right() - 1, pcb.top()),
        (pcb.left(), pcb.bottom() - 1),
        (pcb.right() - 1, pcb.bottom() - 1),
    ] {
        put(buf, pcb, x, y, "◎", hole);
    }
    let silk = Style::new().fg(SILK).bg(PCB);
    let track = Style::new().fg(TRACK).bg(PCB);

    // Right column: LEDs + display on top, keypad housing below.
    let kw: u16 = 7;
    let pad_w = 5 * (kw + 1) + 1;
    let right_w = (6 * 7 + 10).max(pad_w + 2);
    let rx = pcb.right().saturating_sub(right_w + 1);
    let dy = pcb.y + 1;
    put(buf, pcb, rx, dy, "OUT", silk);
    led(
        buf,
        pcb,
        rx + 5,
        dy,
        *app.leds.first().unwrap_or(&0.0),
        false,
        PCB,
    );
    put(buf, pcb, rx, dy + 2, "HALT", silk);
    led(
        buf,
        pcb,
        rx + 5,
        dy + 2,
        *app.leds.get(1).unwrap_or(&0.0),
        true,
        PCB,
    );
    let disp_x = rx + 10;
    draw_display(buf, pcb, disp_x, dy, app);
    for m in 0..3u16 {
        put(
            buf,
            pcb,
            disp_x + m * 14 + 1,
            dy + 5,
            "WF VQE 23 C",
            Style::new().fg(Color::Rgb(205, 190, 160)).bg(PCB),
        );
    }

    // Calculator-style keypad: legends printed on the housing above each key.
    let kh: u16 = key_h;
    let ky = dy + 7;
    let kx = pcb.right().saturating_sub(pad_w + 2);
    let housing = Rect::new(kx, ky, pad_w + 1, 5 * kh + 1).intersection(pcb);
    fill(buf, housing, HOUSING);
    put(
        buf,
        pcb,
        kx + pad_w / 2 - 3,
        ky.saturating_sub(1),
        " LC 80 ",
        Style::new()
            .fg(Color::Black)
            .bg(ALU)
            .add_modifier(Modifier::BOLD),
    );
    let info = app.emu.machine.info();
    for (id, k) in info.keys.iter().enumerate() {
        let x = kx + 1 + k.pos.1 as u16 * (kw + 1);
        let y = ky + 1 + k.pos.0 as u16 * kh;
        let down = app.emu.machine.key_down(id);
        let (mut cap, mut fg) = key_colors(k.color);
        if down {
            cap = Color::Rgb(255, 230, 120);
            fg = Color::Black;
        }
        let rect = Rect::new(x, y, kw, if compact { 1 } else { 2 });
        if rect.intersection(pcb) != rect {
            continue;
        }
        if compact {
            put(
                buf,
                pcb,
                x,
                y,
                &format!("{:^w$}", k.label, w = kw as usize),
                Style::new().fg(fg).bg(cap),
            );
        } else {
            let legend = Style::new().fg(Color::Rgb(215, 215, 210)).bg(HOUSING);
            put(
                buf,
                pcb,
                x,
                y,
                &format!("{:^w$}", k.label, w = kw as usize),
                legend,
            );
            let shade = match cap {
                Color::Rgb(r, g, b) => Color::Rgb(
                    r.saturating_sub(40),
                    g.saturating_sub(40),
                    b.saturating_sub(40),
                ),
                c => c,
            };
            put(
                buf,
                pcb,
                x,
                y + 1,
                &" ".repeat(kw as usize - 1),
                Style::new().bg(cap),
            );
            put(
                buf,
                pcb,
                x + kw - 1,
                y + 1,
                "▌",
                Style::new().fg(shade).bg(HOUSING),
            );
        }
        app.key_rects.push((rect, id));
    }

    // Left part of the board: the chips, with traces running to the right.
    let cx = pcb.x + 3;
    let chips_w = kx.min(rx).saturating_sub(cx + 2);
    put(
        buf,
        pcb,
        pcb.x + 2,
        pcb.y,
        "LERNCOMPUTER  LC 80",
        silk.add_modifier(Modifier::BOLD),
    );
    let glow = |n: &str| app.chip_glow.get(n).copied().unwrap_or(0.0);
    let single_rom =
        app.emu.machine.variant().files.len() == 1 || app.emu.machine.variant().id == "lc80e";
    let rom_type = if single_rom { "EPROM" } else { "U505 D" };
    let rows: [[(&str, u16, String, f32); 2]; 4] = [
        [
            ("D201", 40, "D201 U880 D".into(), glow("D201")),
            ("D208", 28, "D208 U857".into(), glow("D208")),
        ],
        [
            ("D206", 40, "D206 U855 D".into(), glow("D206")),
            ("D202", 24, rom_type.into(), glow("D202")),
        ],
        [
            ("D207", 40, "D207 U855 D".into(), glow("D207")),
            (
                "D203",
                24,
                if single_rom {
                    "—".into()
                } else {
                    rom_type.into()
                },
                glow("D203"),
            ),
        ],
        [
            ("D204", 18, "U214 D".into(), glow("RAM")),
            ("D205", 18, "U214 D".into(), glow("RAM")),
        ],
    ];
    if chips_w >= 20 {
        let mut y = pcb.y + 2;
        for row in &rows {
            if y + 3 > pcb.bottom() - 1 {
                break;
            }
            let mut x = cx;
            for (_, pins, text, g) in row {
                if x + pins / 2 > cx + chips_w {
                    break;
                }
                if text != "—" {
                    draw_chip(buf, pcb, x, y, *pins, text, *g);
                }
                x += pins / 2 + 2;
            }
            // Traces from the chip row to the display/keypad side.
            if x < rx {
                put(
                    buf,
                    pcb,
                    x,
                    y + 1,
                    &"─".repeat((rx - x - 1) as usize),
                    track,
                );
            }
            y += 4;
        }
        // Piezo speaker can next to the RAMs.
        let py = pcb.y + 2 + 12 + 1;
        if py + 1 < pcb.bottom() && chips_w >= 30 {
            let sp = Style::new()
                .fg(Color::Rgb(30, 30, 30))
                .bg(Color::Rgb(120, 120, 120));
            put(buf, pcb, cx + 22, py, "(●)", sp);
            put(buf, pcb, cx + 26, py, "B1", silk);
        }
        // Port legend from the keypad panel.
        let legend = [
            "PIO │ AD BD AC BC",
            "ADR │ F8 F9 FA FB",
            "CTC │ 0  1  2  3",
            "ADR │ EC ED EE EF",
        ];
        let ly = pcb.y + 19;
        if ly + 4 < pcb.bottom() {
            for (i, l) in legend.iter().enumerate() {
                put(buf, pcb, cx, ly + i as u16, l, silk);
            }
        }
    }
    // Edge connector (system bus).
    let ey = pcb.bottom() - 1;
    if !compact && chips_w >= 20 {
        put(buf, pcb, cx, ey, "X1 ", silk);
        put(
            buf,
            pcb,
            cx + 3,
            ey,
            &"▌".repeat((chips_w.min(30)) as usize),
            Style::new().fg(Color::Rgb(214, 170, 60)).bg(PCB),
        );
    }
}

// ---- POLY880 -----------------------------------------------------------------

fn draw_bus_leds(buf: &mut Buffer, clip: Rect, x: u16, y: u16, app: &App, bg: Color, fg: Color) {
    let l = &app.activity.lines;
    let text = Style::new().fg(fg).bg(bg);
    put(buf, clip, x, y, "ADRESSBUS", text);
    let mut cx = x;
    for bit in (0..16).rev() {
        led(buf, clip, cx, y + 1, l[bit], true, bg);
        cx += if bit % 4 == 0 { 3 } else { 2 };
    }
    let dx = cx + 1;
    put(buf, clip, dx, y, "DATENBUS", text);
    let mut cx = dx;
    for bit in (0..8).rev() {
        led(buf, clip, cx, y + 1, l[16 + bit], true, bg);
        cx += if bit % 4 == 0 { 3 } else { 2 };
    }
    let mut num = x;
    for bit in (0..16).rev() {
        put(
            buf,
            clip,
            num,
            y + 2,
            &format!("{:X}", bit),
            Style::new().fg(DIM).bg(bg),
        );
        num += if bit % 4 == 0 { 3 } else { 2 };
    }
    let mut num = dx;
    for bit in (0..8).rev() {
        put(
            buf,
            clip,
            num,
            y + 2,
            &format!("{bit}"),
            Style::new().fg(DIM).bg(bg),
        );
        num += if bit % 4 == 0 { 3 } else { 2 };
    }
    // Control bus.
    let cy = y + 3;
    put(buf, clip, x, cy, "STEUERBUS", text);
    let names = [
        ("M1", trace::LINE_M1),
        ("MREQ", trace::LINE_MREQ),
        ("IORQ", trace::LINE_IORQ),
        ("RD", trace::LINE_RD),
        ("WR", trace::LINE_WR),
        ("HALT", trace::LINE_HALT),
        ("INT", trace::LINE_INT),
        ("NMI", trace::LINE_NMI),
    ];
    let mut cx = x + 10;
    for (i, (n, line)) in names.iter().enumerate() {
        if i == 6 {
            cx += 2;
        }
        led(buf, clip, cx, cy, l[*line], true, bg);
        put(buf, clip, cx + 1, cy, n, Style::new().fg(DIM).bg(bg));
        cx += n.len() as u16 + 3;
    }
}

fn draw_poly880(f: &mut Frame, app: &mut App, area: Rect, key_h: u16) {
    let block = Block::bordered()
        .border_type(BorderType::Thick)
        .title(" POLY-COMPUTER 880 ")
        .title_bottom(Line::from(" VEB Polytechnik Karl-Marx-Stadt · SCOLI ").right_aligned())
        .style(Style::new().bg(POLY_PANEL).fg(POLY_TEXT));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let buf = f.buffer_mut();

    // Window strip: bus analyser and display, side by side if there is room.
    let stacked = area.width < POLY_SIDE_BY_SIDE;
    let strip_h = if stacked { 13 } else { 7 };
    let strip = Rect::new(inner.x, inner.y, inner.width, strip_h).intersection(inner);
    fill(buf, strip, BEZEL);
    draw_bus_leds(buf, strip, inner.x + 1, inner.y, app, BEZEL, POLY_TEXT);
    let mcycl = *app.leds.first().unwrap_or(&0.0);
    led(buf, strip, inner.x + 1, inner.y + 5, mcycl, true, BEZEL);
    put(
        buf,
        strip,
        inner.x + 3,
        inner.y + 5,
        "MCYCL",
        Style::new().fg(DIM).bg(BEZEL),
    );

    let (disp_x, disp_y) = if stacked {
        (inner.x + inner.width.saturating_sub(8 * 7) / 2, inner.y + 6)
    } else {
        (inner.right().saturating_sub(8 * 7 + 1), inner.y)
    };
    let info = app.emu.machine.info();
    for g in info.digit_groups {
        let gx = disp_x + g.first as u16 * 7;
        let w = (g.count * 7) - 1;
        let label = format!("{:─^w$}", format!(" {} ", g.label));
        put(
            buf,
            strip,
            gx,
            disp_y,
            &label,
            Style::new().fg(DIM).bg(BEZEL),
        );
    }
    draw_display(buf, inner, disp_x, disp_y + 1, app);

    // Keypad.
    let kw = 8;
    let ky = inner.y + strip_h + 1;
    let pad_w = 9 * (kw + 1);
    let kx = inner.x + inner.width.saturating_sub(pad_w) / 2;
    draw_keypad(buf, inner, kx, ky, app, key_h, kw);
}

// ---- CPU -------------------------------------------------------------------

fn draw_cpu(f: &mut Frame, app: &App, area: Rect) {
    let m = &app.emu.machine;
    let cpu = m.cpu();
    let r = &cpu.regs;
    let title = format!(" U880 · {:.1} kHz ", m.clock_hz() as f64 / 1000.0);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(title);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let lbl = Style::new().fg(DIM);
    let val = Style::new().fg(Color::White);
    let pair = |n: &'static str, v: u16, n2: &'static str, v2: u16| {
        Line::from(vec![
            Span::styled(format!("{n:<3}"), lbl),
            Span::styled(format!("{v:04X}  "), val),
            Span::styled(format!("{n2:<4}"), lbl),
            Span::styled(format!("{v2:04X}"), val),
        ])
    };
    let alt = |i: usize| u16::from_be_bytes([r.alt[i], r.alt[i + 1]]);
    let mut lines = vec![
        pair("AF", r.af(), "AF'", alt(0)),
        pair("BC", r.bc(), "BC'", alt(2)),
        pair("DE", r.de(), "DE'", alt(4)),
        pair("HL", r.hl(), "HL'", alt(6)),
        pair("IX", r.ix, "IY", r.iy),
        pair("SP", r.sp, "PC", r.pc),
    ];
    let fl: Vec<Span> = [
        ("S", flags::S),
        ("Z", flags::Z),
        ("Y", flags::Y),
        ("H", flags::H),
        ("X", flags::X),
        ("P", flags::PV),
        ("N", flags::N),
        ("C", flags::C),
    ]
    .iter()
    .map(|(n, b)| {
        let on = r.f & b != 0;
        Span::styled(
            format!("{n} "),
            if on {
                Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)
            } else {
                lbl
            },
        )
    })
    .collect();
    lines.push(Line::from(fl));
    lines.push(Line::from(vec![
        Span::styled("I ", lbl),
        Span::styled(format!("{:02X} ", r.i), val),
        Span::styled("R ", lbl),
        Span::styled(format!("{:02X} ", r.r), val),
        Span::styled("IM ", lbl),
        Span::styled(format!("{} ", r.im), val),
        Span::styled("IFF ", lbl),
        Span::styled(format!("{}{}", r.iff1 as u8, r.iff2 as u8), val),
        Span::styled(
            if cpu.halted { "  HALT" } else { "" },
            Style::new().fg(Color::Red),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("T ", lbl),
        Span::styled(format!("{}", cpu.t), val),
        Span::styled(
            format!(
                "  {}{}",
                if cpu.int_line { "INT " } else { "" },
                if cpu.nmi_line() { "NMI" } else { "" }
            ),
            Style::new().fg(Color::Red),
        ),
    ]));
    lines.push(Line::from(Span::styled(
        "─".repeat(inner.width as usize),
        lbl,
    )));

    let mut pc = r.pc;
    let rows = inner.height.saturating_sub(lines.len() as u16);
    for i in 0..rows {
        let ins = disassemble(|a| m.peek(a), pc);
        let bytes: String = (0..ins.len)
            .map(|k| format!("{:02X}", m.peek(pc.wrapping_add(k as u16))))
            .collect();
        let bp = app.emu.breakpoints.contains(&pc);
        let cur = i == 0;
        let style = if cur {
            Style::new().fg(Color::Black).bg(ACCENT)
        } else {
            val
        };
        lines.push(Line::from(vec![
            Span::styled(if bp { "●" } else { " " }, Style::new().fg(Color::Red)),
            Span::styled(format!("{pc:04X} {bytes:<8} {}", ins.text), style),
        ]));
        pc = pc.wrapping_add(ins.len as u16);
    }
    f.render_widget(Paragraph::new(lines), inner);
}

// ---- bus -------------------------------------------------------------------

fn draw_bus(f: &mut Frame, app: &App, area: Rect) {
    let title = match app.bottom {
        BottomView::Trace => " Bus · trace (Tab: memory) ",
        BottomView::Memory => " Memory (Tab: trace · PgUp/PgDn · F12 goto) ",
    };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(title);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let m = &app.emu.machine;
    let info = m.info();

    let mut y = inner.y;
    if !info.bus_leds {
        // The LC-80 had no bus LEDs; show them anyway, it's the modern twist.
        let buf = f.buffer_mut();
        draw_bus_leds(buf, inner, inner.x + 1, y, app, Color::Reset, Color::Gray);
        y += 5;
    }
    let rest = Rect::new(inner.x, y, inner.width, inner.bottom().saturating_sub(y));
    match app.bottom {
        BottomView::Trace => draw_trace(f, app, rest),
        BottomView::Memory => draw_memory(f, app, rest),
    }
}

fn draw_trace(f: &mut Frame, app: &App, area: Rect) {
    let m = &app.emu.machine;
    let cycles = m.trace().recent(area.height as usize);
    let current = app.cycle_step.as_ref().and_then(|c| c.current);
    let lines: Vec<Line> = cycles
        .iter()
        .map(|c| {
            let (what, style) = match c.kind {
                CycleKind::Fetch => {
                    let ins = disassemble(|a| m.peek(a), c.addr);
                    (
                        format!("{:<5} {}", region(m.region(c.addr)), ins.text),
                        Color::Rgb(140, 200, 255),
                    )
                }
                CycleKind::MemRead => (region(m.region(c.addr)).to_string(), Color::Gray),
                CycleKind::MemWrite => (
                    region(m.region(c.addr)).to_string(),
                    Color::Rgb(255, 170, 90),
                ),
                CycleKind::IoRead => (
                    m.port_name(c.addr as u8).to_string(),
                    Color::Rgb(120, 230, 160),
                ),
                CycleKind::IoWrite => (
                    m.port_name(c.addr as u8).to_string(),
                    Color::Rgb(230, 230, 110),
                ),
                CycleKind::IntAck => ("interrupt vector".to_string(), Color::Rgb(255, 90, 90)),
            };
            let mark = if Some(*c) == current { "▶" } else { " " };
            Line::from(vec![
                Span::styled(mark, Style::new().fg(ACCENT)),
                Span::styled(format!("{:>10} ", c.t), Style::new().fg(DIM)),
                Span::styled(format!("{:<4} ", c.kind.short()), Style::new().fg(style)),
                Span::styled(
                    format!("{:04X} {:02X}  ", c.addr, c.data),
                    Style::new().fg(Color::White),
                ),
                Span::styled(what, Style::new().fg(style)),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(lines), area);
}

fn region(r: Region) -> &'static str {
    match r {
        Region::Rom => "ROM",
        Region::Ram => "RAM",
        Region::Unmapped => "---",
    }
}

fn draw_memory(f: &mut Frame, app: &App, area: Rect) {
    let m = &app.emu.machine;
    let r = &m.cpu().regs;
    let mut lines = Vec::new();
    for row in 0..area.height {
        let base = (app.mem_addr & 0xFFF0).wrapping_add(row * 16);
        let mut spans = vec![Span::styled(format!("{base:04X}  "), Style::new().fg(DIM))];
        let mut ascii = String::new();
        for i in 0..16u16 {
            let a = base.wrapping_add(i);
            let v = m.peek(a);
            let mut st = match m.region(a) {
                Region::Rom => Style::new().fg(Color::Rgb(140, 200, 255)),
                Region::Ram => Style::new().fg(Color::White),
                Region::Unmapped => Style::new().fg(DIM),
            };
            if a == r.pc {
                st = st.bg(ACCENT).fg(Color::Black);
            } else if a == r.sp {
                st = st.bg(Color::Rgb(90, 40, 120));
            } else if a == r.hl() {
                st = st.bg(Color::Rgb(30, 80, 60));
            }
            spans.push(Span::styled(format!("{v:02X}"), st));
            spans.push(Span::raw(if i == 7 { "  " } else { " " }));
            ascii.push(if (0x20..0x7F).contains(&v) {
                v as char
            } else {
                '·'
            });
        }
        spans.push(Span::styled(ascii, Style::new().fg(DIM)));
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), area);
}

// ---- chips -------------------------------------------------------------------

fn irq_span(state: IrqState) -> Span<'static> {
    match state {
        IrqState::Idle => Span::styled("idle", Style::new().fg(DIM)),
        IrqState::Pending => {
            Span::styled("INT ", Style::new().fg(ACCENT).add_modifier(Modifier::BOLD))
        }
        IrqState::UnderService => Span::styled(
            "IUS ",
            Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
    }
}

fn draw_chips(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(" Peripherals ");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let lbl = Style::new().fg(DIM);
    let chips = app.emu.machine.chips();
    let mut lines = Vec::new();

    for (name, pio, base) in &chips.pios {
        lines.push(Line::from(vec![
            Span::styled(
                format!("{name} "),
                Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("{base:02X}h  "), lbl),
            irq_span(pio.irq_state()),
        ]));
        for id in [PortId::A, PortId::B] {
            let p = pio.port(id);
            let pins = p.pins();
            let drive = p.drive_mask();
            let mut spans = vec![Span::styled(
                format!(
                    " {} {:<4}",
                    if id == PortId::A { "A" } else { "B" },
                    p.mode.name()
                ),
                lbl,
            )];
            for bit in (0..8).rev() {
                let out = drive & (1 << bit) != 0;
                let hi = pins & (1 << bit) != 0;
                let c = match (out, hi) {
                    (true, true) => Color::Rgb(255, 200, 80),
                    (true, false) => Color::Rgb(110, 85, 35),
                    (false, true) => Color::Rgb(100, 220, 255),
                    (false, false) => Color::Rgb(35, 80, 95),
                };
                spans.push(Span::styled(
                    if hi { "1" } else { "0" },
                    Style::new().fg(c).add_modifier(Modifier::BOLD),
                ));
            }
            let mut extra = format!(" v{:02X}", p.vector);
            if p.ie {
                extra.push_str(" IE");
            }
            if p.ip {
                extra.push_str(" IP");
            }
            if p.ius {
                extra.push_str(" IUS");
            }
            if p.mode == PioMode::BitControl {
                extra.push_str(&format!(" m{:02X}", p.mask));
            }
            spans.push(Span::styled(extra, lbl));
            lines.push(Line::from(spans));
        }
    }

    let (cname, ctc, cbase) = chips.ctc;
    lines.push(Line::from(vec![
        Span::styled(
            format!("{cname} "),
            Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{cbase:02X}h  vec {:02X}  ", ctc.vector), lbl),
        irq_span(ctc.irq_state()),
    ]));
    for (i, c) in ctc.channels.iter().enumerate() {
        let mode = if c.control & CTRL_COUNTER != 0 {
            "CNT ".to_string()
        } else {
            format!("T/{:<3}", c.prescaler())
        };
        let state = match c.state {
            ChannelState::Stopped => Span::styled("stop", lbl),
            ChannelState::WaitTrigger => Span::styled("trig", Style::new().fg(ACCENT)),
            ChannelState::Running => Span::styled("run ", Style::new().fg(Color::Green)),
        };
        let mut flags = String::new();
        if c.control & 0x80 != 0 {
            flags.push_str(" IE");
        }
        if c.ip {
            flags.push_str(" IP");
        }
        if c.ius {
            flags.push_str(" IUS");
        }
        lines.push(Line::from(vec![
            Span::styled(format!(" {i} {mode} "), lbl),
            state,
            Span::styled(
                format!(" tc{:02X} n{:02X}", c.tc, c.down as u8),
                Style::new().fg(Color::White),
            ),
            Span::styled(format!("{flags} zc{}", c.zc_count), lbl),
        ]));
    }

    let mut daisy = vec![Span::styled("Daisy ", lbl)];
    for (i, n) in chips.daisy.iter().enumerate() {
        if i > 0 {
            daisy.push(Span::styled(" → ", lbl));
        }
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
        let color = match st {
            IrqState::Idle => Color::White,
            IrqState::Pending => ACCENT,
            IrqState::UnderService => Color::Red,
        };
        daisy.push(Span::styled(*n, Style::new().fg(color)));
    }
    lines.push(Line::from(daisy));
    drop(chips);

    let tape = app.emu.machine.tape();
    let (cur, total) = tape.progress();
    lines.push(Line::from(vec![
        Span::styled("Tape ", lbl),
        Span::styled(format!("{:?} ", tape.state), Style::new().fg(Color::White)),
        Span::styled(format!("out {} ", tape.out_level as u8), lbl),
        Span::styled(format!("{}/{} kT", cur / 1000, total / 1000), lbl),
    ]));
    if app.emu.autotype.active() {
        let (rem, tot) = app.emu.autotype.progress();
        lines.push(Line::from(Span::styled(
            format!("Autotype {}/{} (Del cancels)", tot - rem, tot),
            Style::new().fg(ACCENT),
        )));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

// ---- status & help -------------------------------------------------------------

fn draw_status(f: &mut Frame, app: &mut App, area: Rect) {
    use crate::app::Action;
    let mut rects: Vec<(Rect, Action)> = Vec::new();
    let buf = f.buffer_mut();
    let mut x = area.x;
    let y = area.y;
    let right = area.right();

    let (state, style) = if app.cycle_step.is_some() {
        (
            " M-CYCLE ",
            Style::new().bg(Color::Rgb(40, 150, 140)).fg(Color::Black),
        )
    } else if app.running {
        (" RUN ", Style::new().bg(Color::Green).fg(Color::Black))
    } else {
        (" STOP ", Style::new().bg(Color::Red).fg(Color::White))
    };
    buf.set_string(x, y, state, style);
    rects.push((Rect::new(x, y, state.len() as u16, 1), Action::RunStop));
    x += state.len() as u16;
    let speed = format!(
        " {} · {:.0} kHz ",
        speed_label(app.speed_factor()),
        app.emulated_hz / 1000.0
    );
    buf.set_string(x, y, &speed, Style::new());
    rects.push((
        Rect::new(x, y, speed.chars().count() as u16, 1),
        Action::Speed,
    ));
    x += speed.chars().count() as u16 + 1;

    let nmi = if app.emu.machine.info().family == "lc80" {
        "NMI "
    } else {
        "MON "
    };
    let running = app.running;
    let cycle = app.cycle_step.is_some();
    let help = app.help;
    let trace_view = app.bottom == crate::app::BottomView::Trace;
    let diagram = app.diagram;
    let sound = app.sound;
    let buttons: [(&str, &str, Action, bool); 16] = [
        ("F1", "Help ", Action::Help, help),
        (
            "F2",
            if running { "Stop " } else { "Run " },
            Action::RunStop,
            false,
        ),
        ("F3", "Step ", Action::Step, false),
        ("F4", "Cycle ", Action::CycleMode, cycle),
        ("F5", "Save ", Action::Save, false),
        ("F6", "Load ", Action::Load, false),
        ("F7", "Type ", Action::Autotype, false),
        ("F8", "Speed ", Action::Speed, false),
        ("F9", "RES ", Action::Reset, false),
        ("F10", nmi, Action::Nmi, false),
        ("F11", "Break ", Action::Breakpoint, false),
        ("F12", "Mem ", Action::MemAddr, false),
        (
            "Tab",
            if trace_view { "Memory " } else { "Trace " },
            Action::ToggleView,
            false,
        ),
        ("^B", "Diagram ", Action::Diagram, diagram),
        ("^S", "Sound ", Action::Sound, sound),
        ("^Q", "Quit ", Action::Quit, false),
    ];
    // Clickable badge + button helper.
    let mut button =
        |buf: &mut Buffer, x: &mut u16, key: &str, label: &str, a: Action, active: bool| {
            let w = (key.len() + label.len()) as u16;
            if *x + w > right {
                return;
            }
            let key_style = Style::new().fg(Color::Black).bg(Color::Rgb(90, 170, 200));
            let label_style = if active {
                Style::new().fg(Color::Black).bg(ACCENT)
            } else {
                Style::new().fg(Color::White).bg(Color::Rgb(45, 45, 50))
            };
            buf.set_string(*x, y, key, key_style);
            buf.set_string(*x + key.len() as u16, y, label, label_style);
            rects.push((Rect::new(*x, y, w, 1), a));
            *x += w + 1;
        };

    for (k, l, a, active) in buttons {
        button(buf, &mut x, k, l, a, active);
    }
    app.button_rects = rects;
    if let Some((msg, _)) = &app.message {
        put(
            buf,
            area,
            x + 1,
            y,
            &format!("│ {msg}"),
            Style::new().fg(ACCENT),
        );
    }
}

// ---- block diagram -------------------------------------------------------

/// One block of the diagram: position, size, text lines, which activity
/// counter makes it glow, and whether it is drawn dashed (not populated).
struct DiagramBox {
    x: u16,
    y: u16,
    w: u16,
    h: u16,
    lines: Vec<String>,
    glow: Option<&'static str>,
    dashed: bool,
}

fn diagram_box(
    x: u16,
    y: u16,
    w: u16,
    h: u16,
    lines: &[&str],
    glow: Option<&'static str>,
) -> DiagramBox {
    let lines = lines
        .iter()
        .flat_map(|s| s.split('\n'))
        .map(str::to_string)
        .collect();
    DiagramBox {
        x,
        y,
        w,
        h,
        lines,
        glow,
        dashed: false,
    }
}

/// The block diagram printed on the POLY880's panel ("TAKT, CPU, 2K-ROM,
/// 1K-RAM, PIO 1/2, CTC on the BUSSYSTEM, into the bus analyser"), drawn
/// live: blocks glow with their share of bus cycles. The LC-80 gets the same
/// picture with its own parts, which was never printed on that device.
fn draw_diagram(f: &mut Frame, app: &App) {
    let info = app.emu.machine.info();
    let v = app.emu.machine.variant();
    let poly = info.family == "poly880";
    let glow = |name: &str| -> f32 {
        let g = |n: &str| app.chip_glow.get(n).copied().unwrap_or(0.0);
        match (poly, name) {
            (true, n) => g(n),
            (false, "CPU") => g("D201"),
            (false, "ROM") => g("D202").max(g("D203")),
            (false, "RAM") => g("RAM"),
            (false, "PIO1") => g("D206"),
            (false, "PIO2") => g("D207"),
            (false, "CTC") => g("D208"),
            _ => 0.0,
        }
    };

    let rom_k = v.files.iter().map(|f| f.size).sum::<usize>() / 1024;
    let ram_k = v.ram_size / 1024;
    let rom = format!("{rom_k}K-");
    let ram = format!("{ram_k}K-");
    let (cpu_part, rom_part, ram_part, pio1, pio2, ctc) = if poly {
        ("U880D", "U505D", "U202D", "U855D", "U855D", "U857D")
    } else {
        let rom_part = if v.files.len() == 2 && v.id == "lc80" {
            "2×U505D"
        } else {
            "EPROM"
        };
        (
            "D201",
            rom_part,
            "2×U214D",
            "U855D\nD206",
            "U855D\nD207",
            "U857D\nD208",
        )
    };

    let low_h = if poly { 4 } else { 5 };
    let mut boxes = vec![
        diagram_box(2, 0, 8, 3, &["TAKT"], None),
        diagram_box(
            1,
            4,
            10,
            6,
            &["CPU", "", if poly { "" } else { "U880D" }, cpu_part],
            Some("CPU"),
        ),
        diagram_box(1, 11, 10, 4, &["NETZTEIL", "+5 V"], None),
        diagram_box(13, 0, 10, 5, &[&rom, "ROM", rom_part], Some("ROM")),
        diagram_box(25, 0, 10, 5, &[&ram, "RAM", ram_part], Some("RAM")),
        diagram_box(13, 8, 10, low_h, &["PIO 1", pio1], Some("PIO1")),
        diagram_box(25, 8, 10, low_h, &["PIO 2", pio2], Some("PIO2")),
        diagram_box(37, 8, 12, low_h, &["CTC", ctc], Some("CTC")),
    ];
    if poly {
        let mut exp = diagram_box(37, 0, 12, 5, &["Erweiterung", "durch", "Anwender"], None);
        exp.dashed = true;
        boxes.push(exp);
    } else {
        boxes.push(diagram_box(37, 0, 12, 5, &["X1", "System-", "bus"], None));
    }
    let analyser = if poly {
        ["BUS-", "Analysator", "SCHALTER", "ANZEIGEN"]
    } else {
        ["", "", "", ""]
    };

    // Overlay frame.
    let (w, h) = (72u16, 19u16);
    let area = f.area();
    let r = Rect::new(
        area.x + area.width.saturating_sub(w) / 2,
        area.y + area.height.saturating_sub(h) / 2,
        w.min(area.width),
        h.min(area.height),
    );
    f.render_widget(Clear, r);
    let title = " Blockschaltbild ";
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(title)
        .title_bottom(Line::from(" ^B / Esc schließt ").right_aligned())
        .style(Style::new().bg(POLY_PANEL).fg(POLY_TEXT));
    let inner = block.inner(r);
    f.render_widget(block, r);
    let buf = f.buffer_mut();
    let (ox, oy) = (inner.x + 1, inner.y + 1);
    let line_col = |g: f32| lerp((150, 145, 140), (255, 236, 160), g);
    let bg = POLY_PANEL;

    // The bus bar with its taps, glowing with the CPU.
    let bus = Style::new().fg(line_col(glow("CPU"))).bg(bg);
    let bar_y = oy + 6;
    put(buf, inner, ox + 11, bar_y, &"═".repeat(41), bus);
    put(
        buf,
        inner,
        ox + 21,
        bar_y,
        " BUSSYSTEM ",
        bus.add_modifier(Modifier::BOLD),
    );
    put(buf, inner, ox + 52, bar_y, "▶", bus);
    for x in [18u16, 30, 43] {
        put(buf, inner, ox + x, bar_y - 1, "║", bus);
        put(buf, inner, ox + x, bar_y + 1, "║", bus);
    }
    let thin = Style::new().fg(line_col(0.0)).bg(bg);
    put(buf, inner, ox + 5, oy + 3, "│", thin);
    put(buf, inner, ox + 5, oy + 10, "│", thin);

    for b in &boxes {
        let g = b.glow.map(glow).unwrap_or(0.0);
        let st = Style::new().fg(line_col(g)).bg(bg);
        let (h_line, v_line) = if b.dashed {
            ("┄", "┆")
        } else {
            ("─", "│")
        };
        let (x, y, bw) = (ox + b.x, oy + b.y, b.w as usize);
        put(
            buf,
            inner,
            x,
            y,
            &format!("┌{}┐", h_line.repeat(bw - 2)),
            st,
        );
        for row in 1..b.h - 1 {
            put(buf, inner, x, y + row, v_line, st);
            put(buf, inner, x + b.w - 1, y + row, v_line, st);
        }
        put(
            buf,
            inner,
            x,
            y + b.h - 1,
            &format!("└{}┘", h_line.repeat(bw - 2)),
            st,
        );
        for (i, l) in b.lines.iter().enumerate().take((b.h - 2) as usize) {
            let text = format!("{l:^w$}", w = bw - 2);
            let tst = if i == 0 {
                st.add_modifier(Modifier::BOLD)
            } else {
                st
            };
            put(buf, inner, x + 1, y + 1 + i as u16, &text, tst);
        }
    }

    // Bus analyser (or LC-80: just the live bus), showing the last cycle.
    let (ax, ay, aw) = (ox + 54, oy + 2, 14u16);
    let st = Style::new().fg(line_col(glow("CPU"))).bg(bg);
    put(
        buf,
        inner,
        ax,
        ay,
        &format!("┌{}┐", "─".repeat(aw as usize - 2)),
        st,
    );
    let tr = app.emu.machine.trace();
    let (a, d, ctl) = match tr.last {
        Some(c) => {
            let k = c.kind.controls();
            let names: Vec<&str> = [
                (k.m1, "M1"),
                (k.mreq, "MREQ"),
                (k.iorq, "IORQ"),
                (k.rd, "RD"),
                (k.wr, "WR"),
            ]
            .iter()
            .filter(|(on, _)| *on)
            .map(|(_, n)| *n)
            .collect();
            (
                format!("A {:04X}", c.addr),
                format!("D {:02X}", c.data),
                names.join(" "),
            )
        }
        None => ("A ----".into(), "D --".into(), String::new()),
    };
    let rows: Vec<String> = if poly {
        analyser
            .iter()
            .map(|s| s.to_string())
            .chain(["".into(), a, d, ctl])
            .collect()
    } else {
        vec![
            "BUS".into(),
            "(live)".into(),
            "".into(),
            a,
            d,
            ctl,
            "".into(),
            "".into(),
        ]
    };
    for (i, l) in rows.iter().enumerate() {
        put(buf, inner, ax, ay + 1 + i as u16, "│", st);
        let tst = if i == 0 {
            st.add_modifier(Modifier::BOLD)
        } else {
            st
        };
        put(
            buf,
            inner,
            ax + 1,
            ay + 1 + i as u16,
            &format!("{l:^w$}", w = aw as usize - 2),
            tst,
        );
        put(buf, inner, ax + aw - 1, ay + 1 + i as u16, "│", st);
    }
    put(
        buf,
        inner,
        ax,
        ay + 1 + rows.len() as u16,
        &format!("└{}┘", "─".repeat(aw as usize - 2)),
        st,
    );
}

fn draw_help(f: &mut Frame, app: &App) {
    let area = f.area();
    let w = 78.min(area.width.saturating_sub(4));
    let h = 30.min(area.height.saturating_sub(2));
    let r = Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    );
    f.render_widget(Clear, r);
    let info = app.emu.machine.info();
    let keys: Vec<String> = info
        .keys
        .iter()
        .filter(|k| !k.chars.is_empty())
        .map(|k| {
            let c: String = k.chars.chars().filter(|c| !c.is_control()).collect();
            format!("{}={}", k.label, if c.is_empty() { "⏎".into() } else { c })
        })
        .filter(|s| !s.starts_with(|c: char| c.is_ascii_hexdigit() && s.as_bytes()[1] == b'='))
        .collect();
    let text = vec![
        Line::from(Span::styled(
            info.title,
            Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("Machine keys: 0-9 A-F as labelled, and"),
        Line::from(format!("  {}", keys.join("  "))),
        Line::from("  Click keys with the mouse to hold them."),
        Line::from(""),
        Line::from("F1  this help             F2  run / stop"),
        Line::from("F3  step instruction (or bus cycle in M-cycle mode)"),
        Line::from("F4  M-cycle stepping on/off (POLY880 MCYCL/CYCL)"),
        Line::from("F5  save state            F6  load state"),
        Line::from("F7  autotype file or text F8  speed"),
        Line::from(format!(
            "F9  RES                   F10 {}",
            if info.family == "lc80" { "NMI" } else { "MON" }
        )),
        Line::from("F11 toggle breakpoint     F12 memory view address"),
        Line::from("Tab trace / memory        Del cancel autotype"),
        Line::from("Ctrl-B block diagram      Ctrl-S sound (at 1× speed)"),
        Line::from("Esc Esc / Ctrl-Q quit"),
        Line::from(""),
        Line::from("Autotype syntax: key characters, {LABEL} e.g. {ADR} {EXEC} {RES},"),
        Line::from("{wait 500} in ms; whitespace ignored; # comments."),
        Line::from(""),
        Line::from(Span::styled(
            if app.key_release_events {
                "Your terminal reports key releases: keys are held while pressed."
            } else {
                "Your terminal can't report key releases: keys are tapped."
            },
            Style::new().fg(DIM),
        )),
    ];
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .title(" Help ");
    f.render_widget(
        Paragraph::new(text).block(block).wrap(Wrap { trim: false }),
        r,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Action;
    use emulatron_machines::roms::RomImages;
    use emulatron_machines::{Emulator, create};
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    fn click(app: &mut App, x: u16, y: u16) {
        for kind in [
            MouseEventKind::Down(MouseButton::Left),
            MouseEventKind::Up(MouseButton::Left),
        ] {
            app.handle(Event::Mouse(MouseEvent {
                kind,
                column: x,
                row: y,
                modifiers: KeyModifiers::NONE,
            }));
        }
    }

    fn app_for(variant: &str) -> Option<App> {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../roms");
        let roms = RomImages::from_dir(std::path::Path::new(dir)).ok()?;
        let m = create(variant, &roms).ok()?;
        Some(App::new(Emulator::new(m), "/dev/null".into(), true))
    }

    fn screen(term: &ratatui::Terminal<TestBackend>) -> String {
        let buf = term.backend().buffer();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Every variant renders at every size without panicking, shows the
    /// size hint exactly below its minimum, and the real UI from there on.
    #[test]
    fn renders_at_all_sizes() {
        for v in emulatron_machines::roms::VARIANTS {
            let Some(mut app) = app_for(v.id) else {
                continue;
            };
            for _ in 0..10 {
                app.tick(std::time::Duration::from_millis(20));
            }
            let (mw, mh) = min_size(app.emu.machine.info());
            let sizes = [
                (1, 1),
                (20, 5),
                (mw - 1, mh),
                (mw, mh - 1),
                (mw, mh),
                (mw + 1, mh + 1),
                (130, 45),
                (160, 50),
                (220, 70),
            ];
            for (i, (w, h)) in sizes.into_iter().enumerate() {
                // Every other size with the block diagram overlay open.
                app.diagram = i % 2 == 1;
                let mut term = ratatui::Terminal::new(TestBackend::new(w, h)).unwrap();
                term.draw(|f| draw(f, &mut app)).unwrap();
                let text = screen(&term);
                let small = w < mw || h < mh;
                assert_eq!(
                    text.contains("Terminal too small"),
                    small && h >= 3,
                    "{} at {w}x{h}",
                    v.id
                );
                if app.diagram && !small {
                    assert!(text.contains("BUSSYSTEM"), "{} at {w}x{h}: diagram", v.id);
                }
                if !small {
                    assert!(
                        !app.key_rects.is_empty(),
                        "{} at {w}x{h}: keys clickable",
                        v.id
                    );
                    assert!(!app.button_rects.is_empty(), "{} at {w}x{h}: toolbar", v.id);
                    let keys = app.emu.machine.info().keys.len();
                    assert_eq!(
                        app.key_rects.len(),
                        keys,
                        "{} at {w}x{h}: all keys drawn",
                        v.id
                    );
                } else {
                    assert!(app.key_rects.is_empty() && app.button_rects.is_empty());
                }
            }
        }
    }

    #[test]
    fn toolbar_buttons_are_clickable() {
        let roms = RomImages::from_dir(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../roms"
        )))
        .unwrap();
        let Ok(m) = create("lc80", &roms) else { return };
        let mut app = App::new(Emulator::new(m), "/dev/null".into(), true);
        let mut term = ratatui::Terminal::new(TestBackend::new(170, 45)).unwrap();
        term.draw(|f| draw(f, &mut app)).unwrap();

        let rect = |app: &App, a: Action| app.button_rects.iter().find(|(_, b)| *b == a).unwrap().0;

        let r = rect(&app, Action::RunStop);
        assert_eq!(r.y, 44);
        assert!(app.running);
        click(&mut app, r.x + 1, r.y);
        assert!(!app.running);

        let r = rect(&app, Action::Help);
        click(&mut app, r.x, r.y);
        assert!(app.help);

        // M-CYCL mode stops free running; leaving it resumes (WAIT released).
        let r = rect(&app, Action::RunStop);
        click(&mut app, r.x, r.y);
        assert!(app.running);
        term.draw(|f| draw(f, &mut app)).unwrap();
        let r = rect(&app, Action::CycleMode);
        click(&mut app, r.x, r.y);
        assert!(!app.running && app.cycle_step.is_some());
        term.draw(|f| draw(f, &mut app)).unwrap();
        let r = rect(&app, Action::CycleMode);
        click(&mut app, r.x, r.y);
        assert!(app.running && app.cycle_step.is_none());

        let speed = app.speed;
        let r = rect(&app, Action::Speed);
        click(&mut app, r.x + 2, r.y);
        assert_ne!(app.speed, speed);
    }
}
