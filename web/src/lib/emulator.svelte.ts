// Browser-side controller around the WebAssembly emulator: loads ROMs,
// drives the machine from requestAnimationFrame and exposes reactive
// snapshots for the faceplates and the debugging panels.

import init, { RomBundle, WebEmu } from './wasm/emulatron.js';
import wasmUrl from './wasm/emulatron_bg.wasm?url';

const romUrls = import.meta.glob('../../../roms/**/*', {
	query: '?url',
	import: 'default',
	eager: true
}) as Record<string, string>;

export type Family = 'lc80' | 'poly880';

export interface Variant {
	id: string;
	name: string;
	family: Family;
	clock_hz: number;
	available: boolean;
}

export interface KeyInfo {
	label: string;
	sub: string;
	color: 'red' | 'black' | 'cream' | 'orange' | 'grey' | 'teal';
	row: number;
	col: number;
	chars: string;
	kind: 'matrix' | 'reset' | 'nmi' | 'cycle';
}

export interface MachineInfo {
	family: Family;
	title: string;
	variant: string;
	variant_name: string;
	clock_hz: number;
	digits: number;
	groups: { label: string; first: number; count: number }[];
	keys: KeyInfo[];
	leds: [string, string][];
	bus_leds: boolean;
	ram_start: number;
	ram_size: number;
}

export interface Frame {
	digits: number[][];
	leds: number[];
	bus: number[];
	chips: [string, number][];
	keys_down: boolean[];
	cycles: number;
	pc: number;
	halted: boolean;
	breakpoint: number | null;
	autotype: [number, number] | null;
	last_cycle: [string, number, number] | null;
}

export interface Cpu {
	af: number;
	bc: number;
	de: number;
	hl: number;
	af_: number;
	bc_: number;
	de_: number;
	hl_: number;
	ix: number;
	iy: number;
	sp: number;
	pc: number;
	i: number;
	r: number;
	f: number;
	im: number;
	iff1: boolean;
	iff2: boolean;
	halted: boolean;
	int: boolean;
	nmi: boolean;
	t: number;
}

export interface Instruction {
	addr: number;
	bytes: string;
	text: string;
	breakpoint: boolean;
}

export interface BusCycle {
	t: number;
	kind: string;
	addr: number;
	data: number;
	what: string;
}

export interface Chips {
	pios: {
		name: string;
		base: number;
		irq: string;
		ports: { mode: string; pins: number; drive: number; vector: number; ie: boolean; ip: boolean; ius: boolean }[];
	}[];
	ctc: {
		name: string;
		base: number;
		vector: number;
		irq: string;
		channels: { mode: string; state: string; tc: number; down: number; ie: boolean; ip: boolean; ius: boolean; zc: number }[];
	};
	daisy: [string, string][];
}

export interface Debug {
	cpu: Cpu;
	disasm: Instruction[];
	trace: BusCycle[];
	chips: Chips;
}

let bundle: Promise<RomBundle> | null = null;

// ---- audio ---------------------------------------------------------------
// One shared chain: the pin's square wave goes through a high-pass (removes
// the DC of an idle pin) and a soft low-pass (a piezo is no hi-fi speaker).

let audioCtx: AudioContext | null = null;
let audioIn: AudioNode | null = null;
let nextStart = 0;

function audio(): AudioContext {
	if (!audioCtx) {
		audioCtx = new AudioContext();
		const hp = audioCtx.createBiquadFilter();
		hp.type = 'highpass';
		hp.frequency.value = 40;
		const lp = audioCtx.createBiquadFilter();
		lp.type = 'lowpass';
		lp.frequency.value = 6500;
		const gain = audioCtx.createGain();
		gain.gain.value = 0.5;
		hp.connect(lp).connect(gain).connect(audioCtx.destination);
		audioIn = hp;
	}
	return audioCtx;
}

function play(samples: Float32Array) {
	const ctx = audioCtx;
	if (!ctx || !audioIn || samples.length === 0) return;
	const buffer = ctx.createBuffer(1, samples.length, ctx.sampleRate);
	buffer.copyToChannel(samples as Float32Array<ArrayBuffer>, 0);
	const src = ctx.createBufferSource();
	src.buffer = buffer;
	src.connect(audioIn);
	// Queue chunks back to back with a little latency; resync if we drift.
	let start = Math.max(ctx.currentTime + 0.04, nextStart);
	if (start - ctx.currentTime > 0.25) start = ctx.currentTime + 0.04;
	src.start(start);
	nextStart = start + buffer.duration;
}

/** Load the wasm module and all ROM files (once). */
export function loadRoms(): Promise<RomBundle> {
	bundle ??= (async () => {
		await init({ module_or_path: wasmUrl });
		const roms = new RomBundle();
		const files = Object.entries(romUrls).filter(([path]) => !path.endsWith('.md'));
		await Promise.all(
			files.map(async ([path, url]) => {
				const res = await fetch(url);
				roms.add(path.split('/').pop()!, new Uint8Array(await res.arrayBuffer()));
			})
		);
		return roms;
	})();
	return bundle;
}

export const SPEEDS = [0.1, 0.25, 0.5, 1, 2, 5, 10];

/** Bus line indices (see the Rust BusTrace). */
export const LINE = { M1: 24, MREQ: 25, IORQ: 26, RD: 27, WR: 28, HALT: 29, INT: 30, NMI: 31 } as const;

function controls(kind: string) {
	return {
		m1: kind === 'M1' || kind === 'INTA',
		mreq: kind === 'M1' || kind === 'MR' || kind === 'MW',
		iorq: kind === 'IOR' || kind === 'IOW' || kind === 'INTA',
		rd: kind === 'M1' || kind === 'MR' || kind === 'IOR',
		wr: kind === 'MW' || kind === 'IOW'
	};
}

export class Machine {
	readonly emu: WebEmu;
	readonly info: MachineInfo;
	readonly variants: Variant[];

	running = $state(true);
	speed = $state(1);
	frame = $state.raw<Frame | null>(null);
	debug = $state.raw<Debug | null>(null);
	glow = $state.raw<Record<string, number>>({});
	hz = $state(0);
	message = $state<string | null>(null);

	/** The autotype script being edited (shared by the typing and program panels). */
	script = $state('');

	/** A program file picked or dropped, waiting to be loaded or typed in. */
	upload = $state.raw<{ name: string; bytes: Uint8Array } | null>(null);

	/** Speaker (LC-80) or tape output (POLY880) audible. */
	sound = $state(false);

	/** POLY880 M CYCL mode: reveal one bus cycle per CYCL press. */
	cycleMode = $state(false);
	currentCycle = $state.raw<BusCycle | null>(null);
	#cycleQueue: BusCycle[] = [];

	#lastDebug = 0;
	#hzCycles = 0;
	#hzTime = 0;
	#messageTimer: ReturnType<typeof setTimeout> | undefined;

	constructor(roms: RomBundle, variant: string) {
		this.emu = new WebEmu(roms, variant);
		this.info = this.emu.info() as MachineInfo;
		this.variants = roms.variants() as Variant[];
		this.frame = this.emu.frame(false) as Frame;
		this.refreshDebug();
	}

	destroy() {
		clearTimeout(this.#messageTimer);
		this.emu.set_audio(false);
		this.emu.free();
	}

	notify(msg: string) {
		this.message = msg;
		clearTimeout(this.#messageTimer);
		this.#messageTimer = setTimeout(() => (this.message = null), 4000);
	}

	/** Advance by `dt` seconds of wall-clock time (called per animation frame). */
	tick(dt: number, now: number) {
		const active = this.running && !this.cycleMode;
		if (active) {
			const cycles = this.info.clock_hz * Math.min(dt, 0.1) * this.speed;
			if (!this.emu.run(cycles)) this.running = false;
			if (this.sound && audioCtx) {
				const samples = this.emu.take_audio(audioCtx.sampleRate);
				// Other speeds would change the pitch; stay quiet there.
				if (this.speed === 1) play(samples);
			}
		}
		const frame = this.emu.frame(active) as Frame;
		if (frame.breakpoint !== null) {
			this.notify(`Stopped at breakpoint ${hex4(frame.breakpoint)}h`);
		}
		this.frame = frame;

		const glow: Record<string, number> = {};
		for (const [name, share] of frame.chips) {
			const target = active ? Math.sqrt(share) : 0;
			glow[name] = (this.glow[name] ?? 0) * 0.75 + target * 0.25;
		}
		this.glow = glow;

		if (now - this.#hzTime > 500) {
			this.hz = ((frame.cycles - this.#hzCycles) / (now - this.#hzTime)) * 1000;
			this.#hzCycles = frame.cycles;
			this.#hzTime = now;
		}
		if (!active || now - this.#lastDebug > 120) {
			this.#lastDebug = now;
			this.refreshDebug();
		}
	}

	refreshDebug() {
		const cpu = this.emu.cpu() as Cpu;
		this.debug = {
			cpu,
			disasm: this.emu.disasm(cpu.pc, 14) as Instruction[],
			trace: this.emu.trace(18) as BusCycle[],
			chips: this.emu.chips() as Chips
		};
	}

	/** Bus line levels to show: live duty cycles, or the revealed cycle. */
	busLines(): number[] {
		const f = this.frame;
		if (!f) return new Array(32).fill(0);
		const c = this.currentCycle;
		if (!this.cycleMode || !c) return f.bus;
		const lines = new Array(32).fill(0);
		for (let b = 0; b < 16; b++) lines[b] = (c.addr >> b) & 1;
		for (let b = 0; b < 8; b++) lines[16 + b] = (c.data >> b) & 1;
		const k = controls(c.kind);
		lines[LINE.M1] = +k.m1;
		lines[LINE.MREQ] = +k.mreq;
		lines[LINE.IORQ] = +k.iorq;
		lines[LINE.RD] = +k.rd;
		lines[LINE.WR] = +k.wr;
		lines[LINE.HALT] = f.bus[LINE.HALT];
		lines[LINE.INT] = f.bus[LINE.INT];
		lines[LINE.NMI] = f.bus[LINE.NMI];
		return lines;
	}

	toggleSound() {
		this.sound = !this.sound;
		if (this.sound) {
			const ctx = audio();
			void ctx.resume();
		}
		this.emu.set_audio(this.sound);
	}

	toggleRun() {
		this.running = !this.running;
		if (this.running) this.leaveCycleMode(true);
	}

	step() {
		this.running = false;
		this.emu.step();
		this.frame = this.emu.frame(false) as Frame;
		this.refreshDebug();
	}

	toggleCycleMode() {
		if (this.cycleMode) {
			this.leaveCycleMode(true);
		} else {
			this.cycleMode = true;
			this.#cycleQueue = [];
			this.currentCycle = null;
			this.notify('M CYCL: each CYCL press shows the next bus cycle');
		}
	}

	leaveCycleMode(resume: boolean) {
		if (!this.cycleMode) return;
		this.cycleMode = false;
		this.currentCycle = null;
		this.#cycleQueue = [];
		if (resume) this.running = true;
	}

	stepCycle() {
		if (!this.cycleMode) this.toggleCycleMode();
		if (this.#cycleQueue.length === 0) {
			const t0 = this.emu.cycles();
			this.emu.step();
			this.#cycleQueue = (this.emu.trace(64) as BusCycle[]).filter((c) => c.t >= t0);
			this.frame = this.emu.frame(false) as Frame;
		}
		this.currentCycle = this.#cycleQueue.shift() ?? null;
		this.refreshDebug();
	}

	/** A key on the faceplate went down or up. */
	press(index: number, down: boolean) {
		const key = this.info.keys[index];
		if (key.kind === 'cycle') {
			if (!down) return;
			if (key.label === 'M CYCL') this.toggleCycleMode();
			else this.stepCycle();
			return;
		}
		if (key.kind === 'reset' && down) this.leaveCycleMode(true);
		this.emu.set_key(index, down);
	}

	keyForChar(c: string): number {
		const u = c.toUpperCase();
		return this.info.keys.findIndex((k) => k.chars.toUpperCase().includes(u));
	}

	keyByLabel(label: string): number {
		return this.info.keys.findIndex((k) => k.label === label);
	}

	typeScript(script: string) {
		this.emu.type_script(script);
		this.running = true;
		this.leaveCycleMode(false);
	}

	cancelTyping() {
		this.emu.cancel_autotype();
	}

	/** Write bytes straight into RAM. */
	loadBytes(addr: number, bytes: Uint8Array) {
		this.emu.load_bytes(addr, bytes);
		this.refreshDebug();
	}

	/**
	 * Type bytes in through the monitor, the way it was done back then.
	 * On a freshly booted LC-80 this first waits for the greeting banner,
	 * during which the monitor ignores keys.
	 */
	typeBytes(addr: number, bytes: Uint8Array, run = false): string {
		let script = this.emu.enter_script(addr, bytes);
		if (run) {
			script +=
				this.info.family === 'lc80'
					? `\n{ADR}${hex4(addr)}{EX}   # and run it`
					: `\n{GO}${hex4(addr)}{EXEC}{EXEC}   # and run it`;
		}
		if (this.info.family === 'lc80') {
			const elapsedMs = (this.emu.cycles() / this.info.clock_hz) * 1000;
			const wait = Math.ceil(15500 - elapsedMs);
			if (wait > 0) script = `{wait ${wait}}   # let the banner finish\n${script}`;
		}
		this.script = script;
		this.typeScript(script);
		return script;
	}

	/** End address (exclusive) of the program at `start`: up to the first run
	 * of 16 identical fill bytes (00 or FF), within RAM. */
	programEnd(start: number): number {
		const ramEnd = this.info.ram_start + this.info.ram_size;
		const len = Math.max(0, ramEnd - start);
		const bytes = this.emu.memory(start, len);
		for (let i = 0; i + 16 <= bytes.length; i++) {
			const v = bytes[i];
			if ((v === 0x00 || v === 0xff) && bytes.subarray(i, i + 16).every((b) => b === v)) return start + i;
		}
		return ramEnd;
	}

	readBytes(start: number, end: number): Uint8Array {
		return this.emu.memory(start, Math.max(0, end - start));
	}

	/** A link that boots this machine and types the program in. */
	shareLink(start: number, bytes: Uint8Array): string {
		const url = new URL(location.href);
		url.search = '';
		url.hash = '';
		url.searchParams.set('machine', this.info.variant);
		url.searchParams.set('at', hex4(start));
		url.searchParams.set('hex', Array.from(bytes, hex2).join(''));
		url.searchParams.set('run', '1');
		return url.toString();
	}

	/** Accept a file from outside: scripts go to the typing panel, anything
	 * else is treated as a binary program. */
	async receiveFile(file: File) {
		if (/\.(txt|atp)$/i.test(file.name)) {
			this.script = await file.text();
			this.notify(`Script ${file.name} is ready in the typing panel`);
		} else {
			this.upload = { name: file.name, bytes: new Uint8Array(await file.arrayBuffer()) };
			this.notify(`${file.name}: ${this.upload.bytes.length} bytes, choose Load or Type it in`);
		}
	}

	/** Off and on again: cleared RAM, so the monitor greets you again. */
	powerCycle() {
		this.emu.power_cycle();
		this.leaveCycleMode(true);
		this.running = true;
		this.notify('Switched off and on again');
	}

	reset() {
		this.emu.reset();
		this.leaveCycleMode(true);
		this.running = true;
	}

	toggleBreakpoint(addr: number) {
		const set = this.emu.toggle_breakpoint(addr);
		this.notify(`Breakpoint ${hex4(addr)}h ${set ? 'set' : 'removed'}`);
		this.refreshDebug();
	}

	saveState(): string {
		return this.emu.save_state();
	}

	loadState(json: string) {
		this.emu.load_state(json);
		this.frame = this.emu.frame(false) as Frame;
		this.refreshDebug();
	}
}

export const hex2 = (v: number) => v.toString(16).toUpperCase().padStart(2, '0');
export const hex4 = (v: number) => v.toString(16).toUpperCase().padStart(4, '0');

/** A program passed in the page URL (see `Machine.shareLink`). */
export interface SharedProgram {
	variant: string;
	addr: number;
	bytes: Uint8Array;
	run: boolean;
}

export function programFromUrl(url: URL): SharedProgram | null {
	const variant = url.searchParams.get('machine');
	const at = url.searchParams.get('at');
	const hex = url.searchParams.get('hex');
	if (!variant || !at || !hex || !/^([0-9a-f]{2})+$/i.test(hex)) return null;
	const addr = parseInt(at, 16);
	if (Number.isNaN(addr)) return null;
	const bytes = new Uint8Array(hex.length / 2);
	for (let i = 0; i < bytes.length; i++) bytes[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
	return { variant, addr: addr & 0xffff, bytes, run: url.searchParams.get('run') === '1' };
}

export function parseHex(text: string): number | null {
	const v = parseInt(text.trim().replace(/^0x/i, '').replace(/h$/i, ''), 16);
	return Number.isNaN(v) ? null : v & 0xffff;
}
