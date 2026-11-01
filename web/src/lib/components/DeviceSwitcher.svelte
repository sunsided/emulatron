<script lang="ts">
	import type { Family } from '#lib/emulator.svelte.js';
	import lc80Photo from '../../../../docs/photos/lc80.jpg';
	import polyPhoto from '../../../../docs/photos/poly880.jpg';

	let { selected, onselect }: { selected: Family; onselect: (f: Family) => void } = $props();

	const DEVICES: { id: Family; name: string; maker: string; photo: string; alt: string }[] = [
		{
			id: 'lc80',
			name: 'LC-80',
			maker: 'VEB Mikroelektronik »Karl Marx« Erfurt, 1984',
			photo: lc80Photo,
			alt: 'An opened LC-80: a board in a brown folder'
		},
		{
			id: 'poly880',
			name: 'Poly-Computer 880',
			maker: 'VEB Polytechnik Karl-Marx-Stadt, 1983',
			photo: polyPhoto,
			alt: 'A Poly-Computer 880 in its aluminium case'
		}
	];
</script>

<nav aria-label="Choose a computer">
	{#each DEVICES as d (d.id)}
		<button class={['card', { active: selected === d.id }]} aria-pressed={selected === d.id} onclick={() => onselect(d.id)}>
			<img src={d.photo} alt={d.alt} width="160" height="104" />
			<span class="name">{d.name}</span>
			<span class="maker">{d.maker}</span>
		</button>
	{/each}
</nav>

<style>
	nav {
		display: flex;
		gap: 0.9rem;
		flex-wrap: wrap;
	}
	.card {
		display: grid;
		grid-template-columns: 96px 1fr;
		grid-template-rows: auto 1fr;
		column-gap: 0.8rem;
		align-items: start;
		width: 19.5rem;
		max-width: 100%;
		flex: 1 1 15rem;
		padding: 0.55rem;
		text-align: left;
		background: var(--sheet);
		border: 1px solid var(--sheet-edge);
		border-radius: 3px;
		box-shadow: 0 1px 0 rgb(255 255 255 / 0.6) inset, 0 2px 0 var(--sheet-edge);
		cursor: pointer;
		opacity: 0.72;
		transition:
			opacity 150ms,
			transform 150ms,
			box-shadow 150ms;
	}
	.card:hover {
		opacity: 0.95;
	}
	.card.active {
		opacity: 1;
		transform: translateY(-2px);
		box-shadow:
			0 1px 0 rgb(255 255 255 / 0.6) inset,
			0 4px 0 var(--orange);
	}
	img {
		grid-row: 1 / 3;
		width: 96px;
		height: 64px;
		object-fit: cover;
		border-radius: 2px;
		filter: saturate(0.85);
	}
	.name {
		font-size: 1.3rem;
		font-weight: 600;
		line-height: 1.1;
	}
	.maker {
		font-family: var(--font-type);
		font-size: 0.72rem;
		color: var(--ink-soft);
		line-height: 1.3;
	}
</style>
