<script lang="ts">
	import { hex2, type Machine } from '#lib/emulator.svelte.js';
	import Sheet from './Sheet.svelte';

	let { machine }: { machine: Machine } = $props();

	const chips = $derived(machine.debug?.chips);
	const BITS = [7, 6, 5, 4, 3, 2, 1, 0];
</script>

<Sheet title="Peripherals" note="U855 PIO · U857 CTC">
	{#if chips}
		<p class="daisy" aria-label="Interrupt daisy chain">
			<span class="lbl">Daisy chain</span>
			{#each chips.daisy as [name, state], i (name)}
				{#if i > 0}<span class="arrow">→</span>{/if}
				<span class={['dev', state]}>{name}</span>
			{/each}
		</p>
		{#each chips.pios as pio (pio.name)}
			<div class="chip">
				<h3>{pio.name} <small>{hex2(pio.base)}h</small></h3>
				{#each pio.ports as port, i (i)}
					<div class="port">
						<span class="pname">{i === 0 ? 'A' : 'B'}</span>
						<span class="mode">{port.mode}</span>
						<span class="bits">
							{#each BITS as b (b)}
								<span
									class={['bit', (port.drive >> b) & 1 ? 'out' : 'in', { hi: (port.pins >> b) & 1 }]}
									title="P{i === 0 ? 'A' : 'B'}{b}">{(port.pins >> b) & 1}</span
								>
							{/each}
						</span>
						<span class="flags">
							v{hex2(port.vector)}{port.ie ? ' IE' : ''}{port.ip ? ' IP' : ''}{port.ius ? ' IUS' : ''}
						</span>
					</div>
				{/each}
			</div>
		{/each}
		<div class="chip">
			<h3>{chips.ctc.name} <small>{hex2(chips.ctc.base)}h · vector {hex2(chips.ctc.vector)}</small></h3>
			<table>
				<tbody>
					{#each chips.ctc.channels as ch, i (i)}
						<tr class={ch.state}>
							<td>{i}</td>
							<td>{ch.mode}</td>
							<td class="state">{ch.state}</td>
							<td>tc {hex2(ch.tc)}</td>
							<td>n {hex2(ch.down)}</td>
							<td class="flags">{ch.ie ? 'IE' : ''}{ch.ip ? ' IP' : ''}{ch.ius ? ' IUS' : ''}</td>
							<td class="zc">{ch.zc}×</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
		<p class="key"><span class="bit out hi">1</span> output <span class="bit in hi">1</span> input</p>
	{/if}
</Sheet>

<style>
	.daisy {
		display: flex;
		flex-wrap: wrap;
		gap: 0.35rem;
		align-items: center;
		margin: 0 0 0.7rem;
	}
	.lbl {
		color: var(--ink-soft);
		margin-right: 0.3rem;
	}
	.dev {
		padding: 0 0.35rem;
		border: 1px solid var(--rule);
	}
	.dev.pending {
		background: var(--orange);
		border-color: var(--orange-deep);
	}
	.dev.service {
		background: var(--teal);
		border-color: #1d6f62;
		color: #f1f7f4;
	}
	.arrow {
		color: var(--ink-soft);
	}
	.chip {
		margin-bottom: 0.7rem;
	}
	h3 {
		margin: 0 0 0.2rem;
		font-family: var(--font-label);
		font-size: 1.02rem;
		font-weight: 600;
	}
	h3 small {
		font-family: var(--font-type);
		font-size: 0.75rem;
		font-weight: 400;
		color: var(--ink-soft);
	}
	.port {
		display: grid;
		grid-template-columns: 1.2em 3.2em auto 1fr;
		gap: 0.5rem;
		align-items: center;
	}
	.mode,
	.pname {
		color: var(--ink-soft);
	}
	.bits {
		display: flex;
		gap: 2px;
	}
	.bit {
		display: inline-block;
		width: 1.2em;
		text-align: center;
		font-weight: 700;
		border-radius: 1px;
	}
	.bit.out {
		color: #7d4a1f;
		background: rgb(226 112 43 / 0.12);
	}
	.bit.out.hi {
		background: var(--orange);
		color: #1e1109;
	}
	.bit.in {
		color: #2a5f59;
		background: rgb(43 138 128 / 0.12);
	}
	.bit.in.hi {
		background: var(--teal);
		color: #f1f7f4;
	}
	.flags {
		color: var(--ink-soft);
		white-space: nowrap;
	}
	table {
		border-collapse: collapse;
		width: 100%;
	}
	td {
		padding: 0 0.6rem 0 0;
		white-space: nowrap;
	}
	tr.stopped {
		color: var(--ink-soft);
	}
	tr.running .state {
		color: var(--teal);
		font-weight: 700;
	}
	.zc {
		text-align: right;
		color: var(--ink-soft);
	}
	.key {
		margin: 0.3rem 0 0;
		color: var(--ink-soft);
		display: flex;
		gap: 0.4rem;
		align-items: center;
	}
</style>
