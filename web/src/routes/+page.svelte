<script lang="ts">
	import { fade } from 'svelte/transition';
	import { loadRoms, Machine, type Family, type Variant } from '#lib/emulator.svelte.js';
	import type { RomBundle } from '#lib/wasm/emulatron.js';
	import DeviceSwitcher from '#lib/components/DeviceSwitcher.svelte';
	import ControlBar from '#lib/components/ControlBar.svelte';
	import Lc80Face from '#lib/components/Lc80Face.svelte';
	import Poly880Face from '#lib/components/Poly880Face.svelte';
	import CpuPanel from '#lib/components/CpuPanel.svelte';
	import BusPanel from '#lib/components/BusPanel.svelte';
	import ChipsPanel from '#lib/components/ChipsPanel.svelte';
	import MemoryPanel from '#lib/components/MemoryPanel.svelte';
	import TypingPanel from '#lib/components/TypingPanel.svelte';

	let family = $state<Family>('lc80');
	let roms = $state.raw<RomBundle | null>(null);
	let variants = $state.raw<Variant[]>([]);
	let failure = $state<string | null>(null);

	// One machine per device, kept alive while you look at the other one.
	let machines = $state.raw<Partial<Record<Family, Machine>>>({});
	const machine = $derived(machines[family]);

	loadRoms()
		.then((r) => {
			roms = r;
			variants = r.variants() as Variant[];
			machines = { lc80: new Machine(r, 'lc80'), poly880: new Machine(r, 'poly880') };
		})
		.catch((e) => (failure = e instanceof Error ? e.message : String(e)));

	function switchVariant(id: string) {
		if (!roms || !machine) return;
		const next = new Machine(roms, id);
		machine.destroy();
		machines = { ...machines, [family]: next };
	}

	// Drive the visible machine from the display refresh.
	$effect(() => {
		const m = machine;
		if (!m) return;
		let last = performance.now();
		let raf = requestAnimationFrame(function loop(now) {
			m.tick((now - last) / 1000, now);
			last = now;
			raf = requestAnimationFrame(loop);
		});
		return () => cancelAnimationFrame(raf);
	});

	// Host keyboard: keys are held for as long as you hold them.
	const held = new Map<string, number>();

	function hostKey(e: KeyboardEvent): number {
		if (!machine) return -1;
		if (e.key === 'Enter') return machine.keyByLabel(machine.info.family === 'lc80' ? 'EX' : 'EXEC');
		if (e.key === 'Backspace') return machine.keyByLabel('BACK');
		if (e.key.length === 1) return machine.keyForChar(e.key);
		return -1;
	}

	function typingInto(e: KeyboardEvent) {
		const t = e.target as HTMLElement | null;
		return !!t && (t.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT', 'BUTTON'].includes(t.tagName) || t.getAttribute('role') === 'button');
	}

	function keydown(e: KeyboardEvent) {
		if (e.ctrlKey || e.metaKey || e.altKey || typingInto(e) || e.repeat) return;
		const k = hostKey(e);
		if (k < 0 || !machine) return;
		e.preventDefault();
		held.set(e.code, k);
		machine.press(k, true);
	}

	function keyup(e: KeyboardEvent) {
		const k = held.get(e.code);
		if (k === undefined || !machine) return;
		held.delete(e.code);
		machine.press(k, false);
	}

	function releaseAll() {
		for (const k of held.values()) machine?.press(k, false);
		held.clear();
	}
</script>

<svelte:window onkeydown={keydown} onkeyup={keyup} onblur={releaseAll} />

<header class="masthead">
	<div class="title">
		<h1>emulatron</h1>
		<p>Two learning computers from the GDR, emulated down to the bus. Type on your keyboard or press the keys.</p>
	</div>
	<DeviceSwitcher selected={family} onselect={(f) => ((family = f), releaseAll())} />
</header>

<main>
	{#if failure}
		<p class="failure">The emulator could not start: {failure}</p>
	{:else if !machine}
		<p class="loading">Loading the ROMs…</p>
	{:else}
		{#key machine}
			<div class="workspace">
			<div class="stage" in:fade={{ duration: 220 }}>
				<div class="scroller">
					<div class={['device', machine.info.family]}>
						{#if machine.info.family === 'lc80'}
							<Lc80Face {machine} />
						{:else}
							<Poly880Face {machine} />
						{/if}
					</div>
				</div>
				<ControlBar {machine} {variants} onvariant={switchVariant} />
				<p class="howto">
					{#if machine.info.family === 'lc80'}
						The monitor greets you with a scrolling <q>HALLO USEr OF LC-80</q> and ignores keys until it's done. Then
						<kbd>,</kbd> is ADR, <kbd>.</kbd> is DAT, <kbd>+</kbd>/<kbd>-</kbd> step and <kbd>Enter</kbd> is EX.
					{:else}
						<kbd>M</kbd> MEM, <kbd>Enter</kbd> EXEC, <kbd>Backspace</kbd> BACK, <kbd>G</kbd> GO, <kbd>R</kbd> REG,
						<kbd>S</kbd> STEP, <kbd>T</kbd> FCT. The block diagram on the panel lights up as the chips are used.
					{/if}
				</p>
			</div>

			<div class="bench" aria-label="Internals">
				<CpuPanel {machine} />
				<BusPanel {machine} />
				<ChipsPanel {machine} />
				<TypingPanel {machine} />
				<MemoryPanel {machine} />
			</div>
			</div>
		{/key}
	{/if}
</main>

<footer>
	<p>
		Written from scratch in Rust and compiled to WebAssembly; the same emulator also runs in a terminal.
		<a href="https://github.com/sunsided/emulatron">Source on GitHub</a>, licensed under the EUPL-1.2.
	</p>
	<p>
		Hardware reference: the MAME drivers <code>lc80.cpp</code> and <code>poly880.cpp</code>. ROM dumps from the MAME sets
		(not covered by the licence). Photos: Florian Schäffer, Wikimedia Commons, CC BY-SA 4.0.
	</p>
</footer>

<style>
	.masthead,
	main,
	footer {
		max-width: 1760px;
		margin: 0 auto;
		padding-inline: clamp(0.75rem, 2.5vw, 2rem);
	}
	.masthead {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: 1rem 2rem;
		padding-block: 1.1rem 1rem;
	}
	h1 {
		margin: 0;
		font-size: clamp(2.3rem, 4.5vw, 3.4rem);
		font-weight: 700;
		line-height: 0.9;
		letter-spacing: -0.01em;
	}
	.title p {
		margin: 0.4rem 0 0;
		max-width: 34rem;
		font-family: var(--font-type);
		font-size: 0.85rem;
		color: var(--ink-soft);
	}
	.workspace {
		display: grid;
		gap: 1.5rem;
		align-items: start;
	}
	@media (min-width: 1200px) {
		.workspace {
			grid-template-columns: minmax(620px, 1.6fr) minmax(400px, 1fr);
		}
		/* The machine stays in view while you read its internals. */
		.stage {
			position: sticky;
			top: 0.8rem;
		}
	}
	.stage {
		display: grid;
		gap: 0.9rem;
		min-width: 0;
	}
	.scroller {
		overflow-x: auto;
		padding-bottom: 0.4rem;
	}
	.device {
		min-width: 620px;
		margin-inline: auto;
		border-radius: 18px;
		filter: drop-shadow(0 12px 16px rgb(34 38 30 / 0.26));
	}
	/* Keep the whole device and its controls on screen. */
	.device.lc80 {
		max-width: max(620px, calc((100vh - 230px) * 1000 / 600));
	}
	.device.poly880 {
		max-width: max(620px, calc((100vh - 230px) * 1000 / 640));
	}
	.howto {
		margin: 0;
		max-width: 68ch;
		font-family: var(--font-type);
		font-size: 0.82rem;
		color: var(--ink-soft);
	}
	kbd {
		font-family: var(--font-type);
		padding: 0 0.3em;
		border: 1px solid var(--rule);
		border-bottom-width: 2px;
		border-radius: 2px;
		background: var(--sheet);
		color: var(--ink);
	}
	.bench {
		container-type: inline-size;
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(340px, 1fr));
		gap: 1rem;
		align-items: start;
		min-width: 0;
	}
	.loading,
	.failure {
		padding: 4rem 0;
		font-family: var(--font-type);
	}
	.failure {
		color: #a3271f;
	}
	footer {
		margin-top: 2.5rem;
		padding-block: 1.2rem 2rem;
		border-top: 2px solid var(--ink);
		font-family: var(--font-type);
		font-size: 0.78rem;
		color: var(--ink-soft);
	}
	footer p {
		margin: 0 0 0.4rem;
		max-width: 80ch;
	}
	footer a {
		color: var(--ink);
	}
	code {
		font-family: inherit;
	}
</style>
