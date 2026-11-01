<script lang="ts">
	import { hex2, hex4, LINE, type Machine } from '#lib/emulator.svelte.js';
	import Sheet from './Sheet.svelte';

	let { machine }: { machine: Machine } = $props();

	// The LC-80 has no bus LEDs of its own; show them here instead.
	const showLeds = $derived(!machine.info.bus_leds);
	const bus = $derived(machine.busLines());
	const CONTROL = [
		['M1', LINE.M1],
		['MREQ', LINE.MREQ],
		['IORQ', LINE.IORQ],
		['RD', LINE.RD],
		['WR', LINE.WR],
		['HALT', LINE.HALT],
		['INT', LINE.INT],
		['NMI', LINE.NMI]
	] as const;
	const current = $derived(machine.currentCycle);
</script>

{#snippet led(level: number, label: string)}
	<span class="led" style:--level={Math.pow(Math.min(1, level), 0.6)} title={label}></span>
{/snippet}

<Sheet title="Bus" note="every machine cycle, newest last">
	{#if showLeds}
		<div class="leds" aria-label="Bus lines">
			<span class="row-label">A</span>
			<span class="row">
				{#each Array.from({ length: 16 }, (_, i) => 15 - i) as b (b)}
					{@render led(bus[b], `A${b}`)}{#if b % 4 === 0 && b > 0}<i></i>{/if}
				{/each}
			</span>
			<span class="row-label">D</span>
			<span class="row">
				{#each Array.from({ length: 8 }, (_, i) => 7 - i) as b (b)}
					{@render led(bus[16 + b], `D${b}`)}{#if b === 4}<i></i>{/if}
				{/each}
			</span>
			<span class="row-label"></span>
			<span class="row ctl">
				{#each CONTROL as [n, l] (n)}
					<span class="ctl-item">{@render led(bus[l], n)}{n}</span>
				{/each}
			</span>
		</div>
	{/if}
	<table>
		<tbody>
			{#each machine.debug?.trace ?? [] as c (c.t)}
				<tr class={[c.kind, { current: current && current.t === c.t }]}>
					<td class="t">{c.t}</td>
					<td class="kind">{c.kind}</td>
					<td>{hex4(c.addr)}</td>
					<td>{hex2(c.data)}</td>
					<td class="what">{c.what}</td>
				</tr>
			{/each}
		</tbody>
	</table>
</Sheet>

<style>
	.leds {
		display: grid;
		grid-template-columns: 1.2em 1fr;
		gap: 0.3rem 0.4rem;
		align-items: center;
		margin-bottom: 0.7rem;
		padding: 0.55rem 0.6rem;
		background: #10120f;
		border-radius: 2px;
		color: #b9beb4;
	}
	.row {
		display: flex;
		align-items: center;
		gap: 0.3rem;
		flex-wrap: wrap;
	}
	.row i {
		width: 0.4rem;
	}
	.row-label {
		font-size: 0.72rem;
	}
	.ctl {
		gap: 0.7rem;
		font-size: 0.68rem;
	}
	.ctl-item {
		display: flex;
		align-items: center;
		gap: 0.25rem;
	}
	.led {
		width: 0.62rem;
		height: 0.62rem;
		border-radius: 50%;
		background: color-mix(in srgb, #ff4a32 calc(var(--level) * 100%), #3a1210);
		box-shadow: 0 0 calc(var(--level) * 7px) rgb(255 74 50 / calc(var(--level) * 0.9));
	}
	table {
		width: 100%;
		border-collapse: collapse;
		table-layout: fixed;
	}
	td {
		padding: 0 0.3rem 0 0;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	td.t {
		width: 6.5em;
		color: var(--ink-soft);
		text-align: right;
		padding-right: 0.8em;
	}
	td.kind {
		width: 3em;
		font-weight: 700;
	}
	td:nth-child(3) {
		width: 3.4em;
	}
	td:nth-child(4) {
		width: 2.2em;
	}
	tr.MW .kind,
	tr.IOW .kind {
		color: var(--orange-deep);
	}
	tr.IOR .kind,
	tr.INTA .kind {
		color: var(--teal);
	}
	tr.current {
		background: rgb(43 138 128 / 0.2);
	}
	.what {
		color: var(--ink-soft);
	}
</style>
