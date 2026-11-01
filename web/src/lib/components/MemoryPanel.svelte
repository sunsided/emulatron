<script lang="ts">
	import { hex2, hex4, type Machine } from '#lib/emulator.svelte.js';
	import Sheet from './Sheet.svelte';

	let { machine }: { machine: Machine } = $props();

	// The start address only seeds the view; the user moves it from there.
	// svelte-ignore state_referenced_locally
	const ramStart = machine.info.family === 'lc80' ? 0x2000 : 0x4000;
	let base = $state(ramStart);
	let input = $state(hex4(ramStart));
	const ROWS = 12;

	const rows = $derived.by(() => {
		machine.debug; // refresh with the debug snapshot
		const start = base & 0xfff0;
		const bytes = machine.emu.memory(start, ROWS * 16);
		const regions = machine.emu.regions(start, ROWS * 16);
		const cpu = machine.debug?.cpu;
		return Array.from({ length: ROWS }, (_, r) => ({
			addr: (start + r * 16) & 0xffff,
			cells: Array.from({ length: 16 }, (_, c) => {
				const i = r * 16 + c;
				const a = (start + i) & 0xffff;
				return { a, v: bytes[i], region: regions[i], pc: cpu?.pc === a, sp: cpu?.sp === a };
			})
		}));
	});

	function go(e: SubmitEvent) {
		e.preventDefault();
		const v = parseInt(input.replace(/h$/i, ''), 16);
		if (!Number.isNaN(v)) base = v & 0xffff;
	}
</script>

<Sheet title="Memory">
	<form onsubmit={go}>
		<label>Address <input bind:value={input} size="6" spellcheck="false" /></label>
		<button type="submit">Show</button>
		<button type="button" onclick={() => (base = (base - ROWS * 16) & 0xffff)} aria-label="Previous page">↑</button>
		<button type="button" onclick={() => (base = (base + ROWS * 16) & 0xffff)} aria-label="Next page">↓</button>
	</form>
	<table>
		<tbody>
			{#each rows as row (row.addr)}
				<tr>
					<th>{hex4(row.addr)}</th>
					{#each row.cells as c (c.a)}
						<td class={['r' + c.region, { pc: c.pc, sp: c.sp }]}>{hex2(c.v)}</td>
					{/each}
				</tr>
			{/each}
		</tbody>
	</table>
	<p class="legend"><span class="r1">RAM</span> <span class="r0">ROM</span> <span class="pc">PC</span> <span class="sp">SP</span></p>
</Sheet>

<style>
	form {
		display: flex;
		gap: 0.4rem;
		align-items: center;
		margin-bottom: 0.5rem;
	}
	input {
		font-family: var(--font-type);
		padding: 0.1rem 0.3rem;
		background: #f2f4ec;
		border: 1px solid var(--rule);
	}
	button {
		padding: 0.05rem 0.5rem;
		font-family: var(--font-label);
		background: var(--sheet);
		border: 1px solid var(--rule);
		border-bottom-width: 2px;
		border-radius: 3px;
		cursor: pointer;
	}
	table {
		border-collapse: collapse;
		font-size: 0.78rem;
	}
	th {
		padding-right: 0.7rem;
		font-weight: 400;
		color: var(--ink-soft);
		text-align: left;
	}
	td {
		padding: 0 0.22rem;
	}
	td:nth-child(9) {
		padding-right: 0.7rem;
	}
	.r0 {
		color: #2a5f59;
	}
	.r2 {
		color: #a7ad9e;
	}
	.pc {
		background: var(--orange);
	}
	.sp {
		outline: 1px solid var(--teal);
	}
	.legend {
		margin: 0.5rem 0 0;
		display: flex;
		gap: 0.8rem;
		font-size: 0.75rem;
	}
	.legend span {
		padding: 0 0.3rem;
	}
</style>
