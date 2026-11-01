<svelte:options namespace="svg" />

<script lang="ts">
	let {
		x,
		y,
		r = 5,
		level,
		color = 'red'
	}: { x: number; y: number; r?: number; level: number; color?: 'red' | 'green' } = $props();

	const on = $derived(Math.min(1, Math.pow(Math.max(0, level), 0.6)));
</script>

<circle cx={x} cy={y} {r} class={['lens', color]} />
{#if on > 0.02}
	<circle cx={x} cy={y} {r} class={['lit', color]} style:opacity={on} />
{/if}
<circle cx={x - r * 0.35} cy={y - r * 0.35} r={r * 0.3} class="shine" />

<style>
	.lens.red {
		fill: #3a1210;
	}
	.lens.green {
		fill: #10301a;
	}
	.lit {
		filter: url(#led-glow);
	}
	.lit.red {
		fill: #ff4a32;
	}
	.lit.green {
		fill: #6dff5a;
	}
	.shine {
		fill: rgb(255 255 255 / 0.25);
		pointer-events: none;
	}
</style>
