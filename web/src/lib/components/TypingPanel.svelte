<script lang="ts">
	import type { Machine } from '#lib/emulator.svelte.js';
	import Sheet from './Sheet.svelte';
	import { tooltip } from '#lib/tooltip.svelte.js';

	let { machine }: { machine: Machine } = $props();

	const examples = import.meta.glob('../../../../examples/**/*.txt', {
		query: '?raw',
		import: 'default',
		eager: true
	}) as Record<string, string>;

	const own = $derived(
		Object.entries(examples)
		.filter(([path]) => path.includes(`/examples/${machine.info.family}/`))
		.map(([path, text]) => ({ name: path.split('/').pop()!.replace(/\.txt$/, ''), text }))
	);

	// Start with the hello example the first time this machine is shown.
	// svelte-ignore state_referenced_locally
	if (!machine.script) machine.script = own.find((e) => e.name === 'hello')?.text ?? '';
	let error = $state<string | null>(null);
	const storageKey = $derived(`emulatron:${machine.info.variant}`);
	// Bumped on save so the derived below re-reads localStorage.
	let saves = $state(0);
	const hasSaved = $derived.by(() => {
		void saves;
		try {
			return localStorage.getItem(storageKey) !== null;
		} catch {
			return false;
		}
	});

	const progress = $derived(machine.frame?.autotype);

	function type() {
		error = null;
		try {
			machine.typeScript(machine.script);
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		}
	}

	function save() {
		try {
			localStorage.setItem(storageKey, machine.saveState());
			saves++;
			machine.notify('State saved in this browser');
		} catch (e) {
			machine.notify(`Saving failed: ${e}`);
		}
	}

	function restore() {
		try {
			const json = localStorage.getItem(storageKey);
			if (json) {
				machine.loadState(json);
				machine.notify('State restored');
			}
		} catch (e) {
			machine.notify(`Restoring failed: ${e instanceof Error ? e.message : e}`);
		}
	}

	function download() {
		const blob = new Blob([machine.saveState()], { type: 'application/json' });
		const a = document.createElement('a');
		a.href = URL.createObjectURL(blob);
		a.download = `${machine.info.variant}.snap.json`;
		a.click();
		URL.revokeObjectURL(a.href);
	}

	async function upload(e: Event) {
		const file = (e.currentTarget as HTMLInputElement).files?.[0];
		if (!file) return;
		try {
			machine.loadState(await file.text());
			machine.notify(`Loaded ${file.name}`);
		} catch (err) {
			machine.notify(`Loading failed: ${err instanceof Error ? err.message : err}`);
		}
	}
</script>

<Sheet title="Typing and states" note="what we once typed in by hand">
	<label class="pick">
		Example
		<select onchange={(e) => (machine.script = own.find((x) => x.name === e.currentTarget.value)?.text ?? machine.script)}>
			{#each own as ex (ex.name)}
				<option value={ex.name} selected={ex.name === 'hello'}>{ex.name}</option>
			{/each}
		</select>
	</label>
	<textarea bind:value={machine.script} rows="7" spellcheck="false" aria-label="Autotype script"></textarea>
	<p class="hint">
		Keys by their character, <code>{'{LABEL}'}</code> for any key, <code>{'{wait 500}'}</code> in milliseconds, <code>#</code> for
		comments.
	</p>
	<div class="row">
		{#if progress}
			<button onclick={() => machine.cancelTyping()}>Stop typing</button>
			<span class="progress">
				<span style:width="{((progress[1] - progress[0]) / progress[1]) * 100}%"></span>
			</span>
			<span>{progress[1] - progress[0]} / {progress[1]}</span>
		{:else}
			<button class="primary" {@attach tooltip('Press the keys in the script, one after the other')} onclick={type}>Type the script</button>
		{/if}
	</div>
	{#if error}<p class="error">{error}</p>{/if}

	<h3>Save state</h3>
	<div class="row">
		<button {@attach tooltip('Remember the whole machine state in this browser')} onclick={save}>Save</button>
		<button {@attach tooltip('Go back to the state saved in this browser')} onclick={restore} disabled={!hasSaved}>Restore</button>
		<button {@attach tooltip('Save the machine state as a file')} onclick={download}>Download</button>
		<label class="upload">Load file<input type="file" accept=".json,application/json" onchange={upload} /></label>
	</div>
</Sheet>

<style>
	.pick {
		display: flex;
		gap: 0.5rem;
		align-items: center;
		margin-bottom: 0.4rem;
		font-family: var(--font-label);
		color: var(--ink-soft);
	}
	select {
		font-family: var(--font-type);
		background: #f2f4ec;
		border: 1px solid var(--rule);
	}
	textarea {
		width: 100%;
		padding: 0.4rem 0.5rem;
		font-family: var(--font-type);
		font-size: 0.78rem;
		line-height: 1.4;
		background: #f2f4ec;
		border: 1px solid var(--rule);
		resize: vertical;
	}
	.hint {
		margin: 0.3rem 0 0.6rem;
		color: var(--ink-soft);
		font-size: 0.74rem;
	}
	code {
		font-family: inherit;
		background: rgb(34 38 30 / 0.07);
		padding: 0 0.15rem;
	}
	.row {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		flex-wrap: wrap;
	}
	button,
	.upload {
		padding: 0.2rem 0.7rem;
		font-family: var(--font-label);
		font-size: 0.95rem;
		font-weight: 600;
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
	.upload input {
		position: absolute;
		width: 1px;
		height: 1px;
		opacity: 0;
	}
	.upload:focus-within {
		outline: 2px solid var(--focus);
	}
	.progress {
		flex: 1;
		min-width: 4rem;
		height: 0.5rem;
		background: rgb(34 38 30 / 0.1);
	}
	.progress span {
		display: block;
		height: 100%;
		background: var(--teal);
	}
	h3 {
		margin: 0.9rem 0 0.35rem;
		font-family: var(--font-label);
		font-size: 1.02rem;
		font-weight: 600;
	}
	.error {
		color: #a3271f;
		margin: 0.4rem 0 0;
	}
</style>
