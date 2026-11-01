<svelte:options namespace="svg" />

<script lang="ts">
	// One VQE 23-style digit, 40 × 68 units before scaling. Segment order
	// a b c d e f g dp, brightness 0..1 per segment.
	let { x = 0, y = 0, scale = 1, segs }: { x?: number; y?: number; scale?: number; segs: number[] } = $props();

	const SEGMENTS = [
		'M9,2 L31,2 L35,6 L31,10 L9,10 L5,6 Z', // a
		'M36,8 L40,12 L40,28 L36,32 L32,28 L32,12 Z', // b
		'M36,36 L40,40 L40,56 L36,60 L32,56 L32,40 Z', // c
		'M9,58 L31,58 L35,62 L31,66 L9,66 L5,62 Z', // d
		'M4,36 L8,40 L8,56 L4,60 L0,56 L0,40 Z', // e
		'M4,8 L8,12 L8,28 L4,32 L0,28 L0,12 Z', // f
		'M9,30 L31,30 L35,34 L31,38 L9,38 L5,34 Z' // g
	];
</script>

<g transform="translate({x} {y}) scale({scale}) skewX(-6)">
	{#each SEGMENTS as d, i (i)}
		<path {d} class="off" />
		{#if segs[i] > 0.02}
			<path {d} class="on" style:opacity={Math.min(1, Math.pow(segs[i], 0.6))} />
		{/if}
	{/each}
	<circle cx="45" cy="63" r="3" class="off" />
	{#if segs[7] > 0.02}
		<circle cx="45" cy="63" r="3" class="on" style:opacity={Math.min(1, Math.pow(segs[7], 0.6))} />
	{/if}
</g>

<style>
	.off {
		fill: var(--seg-off, #16281a);
	}
	.on {
		fill: var(--seg-on, #8dff6a);
		filter: url(#seg-glow);
	}
</style>
