<script lang="ts">
	import { tip } from '#lib/tooltip.svelte.js';
</script>

{#if tip.text}
	<div class={['tip', { below: tip.below }]} role="tooltip" style:left="{tip.x}px" style:top="{tip.y}px">
		{tip.text}
	</div>
{/if}

<style>
	.tip {
		position: fixed;
		z-index: 100;
		max-width: 18rem;
		padding: 0.3rem 0.55rem;
		transform: translate(-50%, calc(-100% - 9px));
		font-family: var(--font-type);
		font-size: 0.76rem;
		line-height: 1.35;
		color: var(--sheet);
		background: var(--ink);
		border-radius: 2px;
		box-shadow: 0 3px 8px rgb(34 38 30 / 0.3);
		pointer-events: none;
		white-space: pre-line;
		animation: appear 120ms ease-out;
	}
	.tip::after {
		content: '';
		position: absolute;
		left: 50%;
		top: 100%;
		border: 5px solid transparent;
		border-top-color: var(--ink);
		transform: translateX(-50%);
	}
	.tip.below {
		transform: translate(-50%, 9px);
	}
	.tip.below::after {
		top: auto;
		bottom: 100%;
		border-top-color: transparent;
		border-bottom-color: var(--ink);
	}
	@keyframes appear {
		from {
			opacity: 0;
		}
	}
</style>
