<script lang="ts">
	import { hex4, parseHex, type Machine } from '#lib/emulator.svelte.js';
	import Sheet from './Sheet.svelte';

	let { machine }: { machine: Machine } = $props();

	// svelte-ignore state_referenced_locally
	const ram = machine.info.ram_start;
	let from = $state(hex4(ram));
	// svelte-ignore state_referenced_locally
	let to = $state(hex4(machine.programEnd(ram)));
	let at = $state(hex4(ram));
	let copied = $state(false);

	const range = $derived.by(() => {
		const a = parseHex(from);
		const b = parseHex(to);
		return a !== null && b !== null && b > a ? { start: a, end: b } : null;
	});

	function detect() {
		const a = parseHex(from);
		if (a !== null) to = hex4(machine.programEnd(a));
	}

	function download() {
		if (!range) return;
		const bytes = machine.readBytes(range.start, range.end);
		const blob = new Blob([bytes as Uint8Array<ArrayBuffer>], { type: 'application/octet-stream' });
		const a = document.createElement('a');
		a.href = URL.createObjectURL(blob);
		a.download = `${machine.info.variant}-${hex4(range.start)}-${hex4(range.end - 1)}.bin`;
		a.click();
		URL.revokeObjectURL(a.href);
	}

	async function share() {
		if (!range) return;
		const link = machine.shareLink(range.start, machine.readBytes(range.start, range.end));
		try {
			await navigator.clipboard.writeText(link);
			copied = true;
			setTimeout(() => (copied = false), 2500);
			machine.notify('Link copied: it opens this machine, types the program in and runs it');
		} catch {
			prompt('Copy this link', link);
		}
	}

	async function pick(e: Event) {
		const input = e.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		if (file) await machine.receiveFile(file);
		input.value = '';
	}

	function place(mode: 'load' | 'type') {
		const up = machine.upload;
		const addr = parseHex(at);
		if (!up || addr === null) return;
		try {
			if (mode === 'load') {
				machine.loadBytes(addr, up.bytes);
				machine.notify(`${up.name} loaded at ${hex4(addr)}h`);
			} else {
				machine.typeBytes(addr, up.bytes);
				machine.notify(`Typing ${up.name} in at ${hex4(addr)}h`);
			}
		} catch (err) {
			machine.notify(err instanceof Error ? err.message : String(err));
		}
	}
</script>

<Sheet title="Programs" note="in and out of the machine">
	<h3>Take a program with you</h3>
	<div class="row">
		<label>From <input bind:value={from} size="5" spellcheck="false" /></label>
		<label>to <input bind:value={to} size="5" spellcheck="false" /></label>
		<button type="button" onclick={detect} title="Find where the program in RAM ends">Detect end</button>
	</div>
	<div class="row">
		<button type="button" onclick={download} disabled={!range}>Download .bin</button>
		<button type="button" onclick={share} disabled={!range}>{copied ? 'Link copied' : 'Copy share link'}</button>
		{#if range}<span class="size">{range.end - range.start} bytes</span>{/if}
	</div>

	<h3>Bring one in</h3>
	<p class="hint">A <code>.bin</code> to load or type in, or a <code>.txt</code> autotype script. You can also drop the file on the machine.</p>
	<div class="row">
		<label class="file">Choose file<input type="file" accept=".bin,.txt,.atp,application/octet-stream,text/plain" onchange={pick} /></label>
		{#if machine.upload}<span class="size">{machine.upload.name} · {machine.upload.bytes.length} bytes</span>{/if}
	</div>
	{#if machine.upload}
		<div class="row">
			<label>At <input bind:value={at} size="5" spellcheck="false" /></label>
			<button type="button" onclick={() => place('load')}>Load into RAM</button>
			<button type="button" class="primary" onclick={() => place('type')}>Type it in</button>
		</div>
	{/if}
</Sheet>

<style>
	h3 {
		margin: 0 0 0.35rem;
		font-family: var(--font-label);
		font-size: 1.02rem;
		font-weight: 600;
	}
	h3:not(:first-child) {
		margin-top: 0.9rem;
	}
	.row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.4rem 0.6rem;
		margin-bottom: 0.45rem;
	}
	label {
		display: flex;
		align-items: center;
		gap: 0.35rem;
		color: var(--ink-soft);
	}
	input {
		font-family: var(--font-type);
		padding: 0.1rem 0.3rem;
		background: #f2f4ec;
		border: 1px solid var(--rule);
		color: var(--ink);
	}
	button,
	.file {
		padding: 0.2rem 0.7rem;
		font-family: var(--font-label);
		font-size: 0.95rem;
		font-weight: 600;
		color: var(--ink);
		background: var(--sheet);
		border: 1px solid var(--rule);
		border-bottom-width: 3px;
		border-radius: 3px;
		cursor: pointer;
	}
	button:disabled {
		opacity: 0.45;
		cursor: default;
	}
	.primary {
		background: var(--orange);
		border-color: var(--orange-deep);
	}
	.file input {
		position: absolute;
		width: 1px;
		height: 1px;
		opacity: 0;
	}
	.file:focus-within {
		outline: 2px solid var(--focus);
	}
	.size,
	.hint {
		color: var(--ink-soft);
		font-size: 0.76rem;
	}
	.hint {
		margin: 0 0 0.45rem;
	}
	code {
		font-family: inherit;
		background: rgb(34 38 30 / 0.07);
		padding: 0 0.15rem;
	}
</style>
