<svelte:options namespace="svg" />

<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { Machine } from '#lib/emulator.svelte.js';
	import { tooltip } from '#lib/tooltip.svelte.js';

	// A pressable key on a faceplate: holds the machine key while the pointer
	// (or Enter/Space on a focused key) is down.
	let {
		machine,
		index,
		label,
		x,
		y,
		w,
		h,
		tip,
		children
	}: {
		machine: Machine;
		index: number;
		label: string;
		x: number;
		y: number;
		w: number;
		h: number;
		tip?: string;
		children: Snippet;
	} = $props();

	let held = false;

	function down(e: PointerEvent) {
		(e.currentTarget as Element).setPointerCapture(e.pointerId);
		held = true;
		machine.press(index, true);
	}

	function up() {
		if (!held) return;
		held = false;
		machine.press(index, false);
	}

	function keydown(e: KeyboardEvent) {
		if ((e.key === 'Enter' || e.key === ' ') && !e.repeat) {
			e.preventDefault();
			e.stopPropagation();
			held = true;
			machine.press(index, true);
		}
	}

	function keyup(e: KeyboardEvent) {
		if (e.key === 'Enter' || e.key === ' ') {
			e.stopPropagation();
			up();
		}
	}
</script>

<g
	class="key"
	role="button"
	tabindex="0"
	aria-label={label}
	onpointerdown={down}
	onpointerup={up}
	onpointercancel={up}
	onlostpointercapture={up}
	onkeydown={keydown}
	onkeyup={keyup}
	onblur={up}
	{@attach tooltip(tip)}
>
	<rect {x} {y} width={w} height={h} fill="transparent" />
	{@render children()}
	<rect class="focus" {x} {y} width={w} height={h} rx="4" />
</g>

<style>
	.key {
		cursor: pointer;
		outline: none;
		touch-action: none;
	}
	.focus {
		fill: none;
		stroke: none;
	}
	.key:focus-visible .focus {
		stroke: var(--focus, #ffd166);
		stroke-width: 2;
	}
</style>
