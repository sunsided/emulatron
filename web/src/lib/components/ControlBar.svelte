<script lang="ts">
	import { SPEEDS, type Machine, type Variant } from '#lib/emulator.svelte.js';

	let {
		machine,
		variants,
		onvariant
	}: { machine: Machine; variants: Variant[]; onvariant: (id: string) => void } = $props();

	const poly = $derived(machine.info.family === 'poly880');
	const nmiLabel = $derived(poly ? 'MON' : 'NMI');
	const own = $derived(variants.filter((v) => v.family === machine.info.family && v.available));

	function tap(label: string) {
		const i = machine.keyByLabel(label);
		if (i < 0) return;
		machine.press(i, true);
		setTimeout(() => machine.press(i, false), 120);
	}

	const khz = $derived((machine.hz / 1000).toFixed(machine.hz < 100_000 ? 1 : 0));
</script>

<div class="bar">
	<div class="group">
		<button class="primary" onclick={() => machine.toggleRun()} aria-pressed={machine.running && !machine.cycleMode}>
			{machine.running && !machine.cycleMode ? 'Pause' : 'Run'}
		</button>
		<button onclick={() => machine.step()}>Step</button>
		{#if poly}
			<button class={{ on: machine.cycleMode }} onclick={() => machine.toggleCycleMode()} aria-pressed={machine.cycleMode}>
				M CYCL
			</button>
			<button disabled={!machine.cycleMode} onclick={() => machine.stepCycle()}>CYCL</button>
		{/if}
	</div>

	<div class="group" role="group" aria-label="Speed">
		{#each SPEEDS as s (s)}
			<button class={['speed', { on: machine.speed === s }]} aria-pressed={machine.speed === s} onclick={() => (machine.speed = s)}>
				{s < 1 ? s : `${s}×`}
			</button>
		{/each}
	</div>

	<div class="group">
		<button onclick={() => machine.reset()}>RES</button>
		<button onclick={() => tap(nmiLabel)}>{nmiLabel}</button>
	</div>

	{#if own.length > 1}
		<label class="variant">
			ROM set
			<select value={machine.info.variant} onchange={(e) => onvariant(e.currentTarget.value)}>
				{#each own as v (v.id)}
					<option value={v.id}>{v.name}</option>
				{/each}
			</select>
		</label>
	{/if}

	<p class="status" aria-live="polite">
		<span>{khz} kHz</span>
		{#if machine.message}<span class="msg">{machine.message}</span>{/if}
	</p>
</div>

<style>
	.bar {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.6rem 1.2rem;
	}
	.group {
		display: flex;
		gap: 0.3rem;
	}
	button {
		min-width: 2.6rem;
		padding: 0.3rem 0.75rem;
		font-size: 1rem;
		font-weight: 600;
		letter-spacing: 0.02em;
		background: var(--sheet);
		border: 1px solid var(--rule);
		border-bottom-width: 3px;
		border-radius: 3px;
		cursor: pointer;
	}
	button:active {
		transform: translateY(1px);
		border-bottom-width: 2px;
	}
	button:disabled {
		opacity: 0.45;
		cursor: default;
	}
	button.primary {
		min-width: 5rem;
		background: var(--orange);
		border-color: var(--orange-deep);
		color: #1e1109;
	}
	button.on {
		background: var(--teal);
		border-color: #1d6f62;
		color: #f1f7f4;
	}
	button.speed {
		min-width: 2.4rem;
		padding-inline: 0.45rem;
		font-weight: 500;
	}
	.variant {
		display: flex;
		align-items: center;
		gap: 0.45rem;
		color: var(--ink-soft);
	}
	select {
		padding: 0.25rem 0.4rem;
		background: var(--sheet);
		border: 1px solid var(--rule);
		border-radius: 3px;
	}
	.status {
		margin: 0 0 0 auto;
		display: flex;
		gap: 1rem;
		font-family: var(--font-type);
		font-size: 0.85rem;
		color: var(--ink-soft);
	}
	.msg {
		color: var(--orange-deep);
	}
</style>
