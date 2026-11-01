<script lang="ts">
	import { hex2, hex4, type Machine } from '#lib/emulator.svelte.js';
	import Sheet from './Sheet.svelte';

	let { machine }: { machine: Machine } = $props();

	const cpu = $derived(machine.debug?.cpu);
	const FLAGS = ['S', 'Z', 'Y', 'H', 'X', 'P', 'N', 'C'];
</script>

<Sheet title="Processor" note="U880 · click an address for a breakpoint">
	{#if cpu}
		<dl class="regs">
			{#each [['AF', cpu.af, "AF'", cpu.af_], ['BC', cpu.bc, "BC'", cpu.bc_], ['DE', cpu.de, "DE'", cpu.de_], ['HL', cpu.hl, "HL'", cpu.hl_], ['IX', cpu.ix, 'IY', cpu.iy], ['SP', cpu.sp, 'PC', cpu.pc]] as [a, av, b, bv] (a)}
				<dt>{a}</dt><dd>{hex4(av as number)}</dd>
				<dt>{b}</dt><dd>{hex4(bv as number)}</dd>
			{/each}
		</dl>
		<p class="flags" aria-label="Flags">
			{#each FLAGS as f, i (f)}
				<span class={{ set: (cpu.f >> (7 - i)) & 1 }}>{f}</span>
			{/each}
		</p>
		<p class="misc">
			I {hex2(cpu.i)} · R {hex2(cpu.r)} · IM {cpu.im} · IFF {+cpu.iff1}{+cpu.iff2}
			{#if cpu.halted}<b>HALT</b>{/if}
			{#if cpu.int}<b>INT</b>{/if}
			{#if cpu.nmi}<b>NMI</b>{/if}
		</p>
		<ol class="disasm">
			{#each machine.debug?.disasm ?? [] as ins, i (ins.addr)}
				<li class={{ current: i === 0 }}>
					<button
						class={['addr', { bp: ins.breakpoint }]}
						onclick={() => machine.toggleBreakpoint(ins.addr)}
						aria-label="Toggle breakpoint at {hex4(ins.addr)}">{hex4(ins.addr)}</button
					>
					<span class="bytes">{ins.bytes}</span>
					<span class="text">{ins.text}</span>
				</li>
			{/each}
		</ol>
	{/if}
</Sheet>

<style>
	.regs {
		display: grid;
		grid-template-columns: 2.4em 1fr 2.4em 1fr;
		margin: 0 0 0.4rem;
		row-gap: 0.05rem;
	}
	dt {
		color: var(--ink-soft);
	}
	dd {
		margin: 0;
		font-weight: 700;
	}
	.flags {
		display: flex;
		gap: 0.25rem;
		margin: 0.3rem 0;
	}
	.flags span {
		width: 1.35em;
		text-align: center;
		border: 1px solid var(--rule);
		color: var(--ink-soft);
	}
	.flags .set {
		background: var(--ink);
		border-color: var(--ink);
		color: var(--sheet);
	}
	.misc {
		margin: 0 0 0.6rem;
		color: var(--ink-soft);
	}
	.misc b {
		margin-left: 0.5em;
		color: var(--orange-deep);
	}
	.disasm {
		list-style: none;
		margin: 0;
		padding: 0.4rem 0 0;
		border-top: 1px dashed var(--rule);
	}
	li {
		display: grid;
		grid-template-columns: 3.4em 5.8em 1fr;
		gap: 0.4rem;
		white-space: nowrap;
	}
	li.current {
		background: rgb(226 112 43 / 0.18);
	}
	.addr {
		padding: 0 0 0 0.9em;
		background: none;
		border: none;
		font: inherit;
		text-align: left;
		cursor: pointer;
		position: relative;
	}
	.addr::before {
		content: '';
		position: absolute;
		left: 0.15em;
		top: 0.45em;
		width: 0.5em;
		height: 0.5em;
		border-radius: 50%;
		border: 1px solid transparent;
	}
	.addr:hover::before {
		border-color: #c8322a;
	}
	.addr.bp::before {
		background: #c8322a;
		border-color: #c8322a;
	}
	.bytes {
		color: var(--ink-soft);
		overflow: hidden;
	}
	.text {
		overflow: hidden;
		text-overflow: ellipsis;
	}
</style>
