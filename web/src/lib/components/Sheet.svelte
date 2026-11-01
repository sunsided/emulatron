<script lang="ts">
	import type { Snippet } from 'svelte';

	// A sheet from the workbench: typewritten content under a printed heading.
	let { title, note, children }: { title: string; note?: string; children: Snippet } = $props();
</script>

<section class="sheet">
	<header>
		<h2>{title}</h2>
		{#if note}<p>{note}</p>{/if}
	</header>
	<div class="body">
		{@render children()}
	</div>
</section>

<style>
	.sheet {
		background: var(--sheet);
		border: 1px solid var(--sheet-edge);
		border-radius: 2px;
		box-shadow: 0 2px 0 var(--sheet-edge);
		min-width: 0;
	}
	header {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 1rem;
		padding: 0.5rem 0.9rem 0.35rem;
		border-bottom: 2px solid var(--ink);
	}
	h2 {
		margin: 0;
		font-size: 1.25rem;
		font-weight: 600;
		letter-spacing: 0.01em;
	}
	p {
		margin: 0;
		font-family: var(--font-type);
		font-size: 0.75rem;
		color: var(--ink-soft);
	}
	.body {
		padding: 0.7rem 0.9rem 0.9rem;
		font-family: var(--font-type);
		font-size: 0.82rem;
		line-height: 1.45;
	}
</style>
