// What the faceplate keys do, for tooltips. Taken from the monitors'
// behaviour and the POLY880 operating manual; where a function is not
// documented here, the tooltip only names the keyboard shortcut.

import type { KeyInfo } from './emulator.svelte.js';

const LC80: Record<string, string> = {
	RES: 'Reset (warm start: RAM is kept)',
	NMI: 'Non-maskable interrupt: back to the monitor',
	ADR: 'Enter an address',
	DAT: 'Enter data at the address',
	'+': 'Next address',
	'-': 'Previous address',
	EX: 'Execute: run the program at the address'
};

const POLY880: Record<string, string> = {
	RES: 'Reset (warm start: RAM is kept)',
	MON: 'Back to the monitor (NMI)',
	MEM: 'Show and change memory',
	REG: 'Show and change registers',
	GO: 'Start a program: GO, address, EXEC, EXEC',
	EXEC: 'Confirm, or go to the next address',
	BACK: 'Go back one address or register',
	STEP: 'Run a single instruction',
	FCT: 'Further monitor functions',
	'M CYCL': 'Machine-cycle mode: the CPU waits on every bus cycle',
	CYCL: 'Release the next bus cycle (in M CYCL mode)'
};

function shortcut(key: KeyInfo): string | null {
	if (key.label === 'EX' || key.label === 'EXEC') return `${key.chars[0] ?? ''} or Enter`;
	if (key.label === 'BACK') return `${key.chars[0] ?? ''} or Backspace`;
	return key.chars ? key.chars[0] : null;
}

export function keyTip(family: string, key: KeyInfo): string {
	const table = family === 'lc80' ? LC80 : POLY880;
	let what = table[key.label];
	if (!what && /^[0-9A-F]$/.test(key.label)) {
		what = key.sub ? `Hex digit ${key.label} · register ${key.sub}` : `Hex digit ${key.label}`;
	}
	const sc = shortcut(key);
	const parts = [key.label === what ? null : key.label, what, sc ? `key ${sc}` : null].filter(Boolean);
	return parts.join(' · ');
}
