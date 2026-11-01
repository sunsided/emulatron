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
