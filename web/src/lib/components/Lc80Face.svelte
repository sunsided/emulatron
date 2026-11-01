<script lang="ts">
	import type { Machine } from '#lib/emulator.svelte.js';
	import SevenSeg from './SevenSeg.svelte';
	import Led from './Led.svelte';
	import SvgDefs from './SvgDefs.svelte';
	import KeyHit from './KeyHit.svelte';

	let { machine }: { machine: Machine } = $props();

	const frame = $derived(machine.frame);
	const keys = $derived(machine.info.keys);
	const singleRom = $derived(machine.info.variant !== 'lc80');

	// Chips on the board: designator, type, pins (sets the width), position.
	const CHIPS = $derived([
		{ id: 'D201', type: 'U 880 D', pins: 40, x: 474, y: 150 },
		{ id: 'D206', type: 'U 855 D', pins: 40, x: 474, y: 200 },
		{ id: 'D207', type: 'U 855 D', pins: 40, x: 474, y: 250 },
		{ id: 'D208', type: 'U 857 D', pins: 28, x: 474, y: 300 },
		{ id: 'D202', type: singleRom ? 'EPROM' : 'U 505 D', pins: 24, x: 474, y: 350 },
		{ id: 'D203', type: 'U 505 D', pins: 24, x: 574, y: 350, hidden: singleRom },
		{ id: 'D204', type: 'U 214 D', pins: 18, x: 474, y: 400, glow: 'RAM' },
		{ id: 'D205', type: 'U 214 D', pins: 18, x: 556, y: 400, glow: 'RAM' }
	]);

	const keyColor: Record<string, [string, string]> = {
		red: ['#c8322a', '#8f1f19'],
		black: ['#2b2b2d', '#121213'],
		cream: ['#ece5d2', '#b9b19c']
	};

	const KX = 748;
	const KY = 418;
	const PX = 37;
	const PY = 26;

	function chipFill(g: number) {
		const a = [30, 29, 28];
		const b = [96, 78, 36];
		const m = (i: number) => Math.round(a[i] + (b[i] - a[i]) * Math.min(1, g));
		return `rgb(${m(0)} ${m(1)} ${m(2)})`;
	}
</script>

<svg viewBox="0 0 1000 600" role="img" aria-label="LC-80 learning computer, opened in its folder">
	<SvgDefs />

	<!-- Cardboard box -->
	<rect x="6" y="6" width="988" height="588" rx="6" fill="#c9b285" />
	<rect x="18" y="18" width="964" height="564" rx="4" fill="#b59d6f" />

	<!-- Folder: left cover, spine, right half -->
	<rect x="30" y="30" width="390" height="540" rx="10" fill="url(#leather)" />
	<rect x="30" y="30" width="390" height="540" rx="10" fill="url(#grain)" />
	<rect x="44" y="44" width="362" height="512" rx="6" fill="none" stroke="#3a2015" stroke-dasharray="3 4" opacity="0.6" />
	<rect x="420" y="30" width="20" height="540" fill="#3d2318" />
	<rect x="440" y="30" width="530" height="540" rx="10" fill="url(#leather)" />

	<!-- The aluminium plate on the inside cover -->
	<g class="plate" transform="translate(70 450)">
		<rect width="300" height="88" rx="2" fill="url(#alu)" />
		<text x="10" y="20">VOR FEUCHTIGKEIT SCHÜTZEN!</text>
		<text x="10" y="34">KEEP DRY!</text>
		<text x="10" y="48">CRAINT L'HUMIDITÉ!</text>
		<text x="10" y="62">PRESERVESE DE HUMEDAD!</text>
		<text x="170" y="20">RFT veb mikroelektronik</text>
		<text x="170" y="34">»karl marx« erfurt</text>
		<text x="170" y="48">G-5403.500</text>
		<text x="170" y="62">MADE IN GDR</text>
		<text x="170" y="78">Nr. 04711</text>
	</g>

	<!-- Board -->
	<rect x="452" y="44" width="506" height="512" rx="4" fill="#a39265" />
	<g class="traces">
		<!-- a bus bundle from the chips to the edge connector -->
		<path d="M630 163 H652 V136 H934 V214 H946" />
		<path d="M630 213 H660 V144 H926 V230 H946" />
		<path d="M630 263 H668 V152 H918 V246 H946" />
		<path d="M586 313 H676 V160 H910 V262 H946" />
		<path d="M640 363 H684 V168 H902 V278 H946" />
		<path d="M622 413 H692 V176 H894 V294 H946" />
		<path d="M520 120 V136 M600 100 V128 H640" />
		<path d="M500 470 V545 H930 V440" />
	</g>
	{#each [[462, 54], [946, 54], [462, 546], [946, 546]] as [cx, cy] (cx * 1000 + cy)}
		<circle {cx} {cy} r="5" fill="#d9d6cc" stroke="#6d6247" />
	{/each}

	<!-- Power: heat sink, regulator, capacitor -->
	<g>
		{#each [0, 1, 2, 3, 4, 5] as i (i)}
			<rect x={468 + i * 11} y="62" width="7" height="58" fill="#1b1b1c" />
		{/each}
		<rect x="468" y="84" width="62" height="14" fill="#2a2a2b" />
		<circle cx="499" cy="91" r="10" fill="#b8b7b0" />
		<rect x="548" y="70" width="24" height="44" rx="10" fill="#3b7fa8" />
		<rect x="548" y="70" width="24" height="10" rx="5" fill="#9fb6c4" />
		<rect x="600" y="70" width="34" height="30" rx="3" fill="#2a2a2a" />
		<rect x="660" y="70" width="34" height="30" rx="3" fill="#2a2a2a" />
		<circle cx="617" cy="85" r="9" fill="#4a4a4a" />
		<circle cx="677" cy="85" r="9" fill="#4a4a4a" />
	</g>

	<!-- Piezo speaker -->
	<g transform="translate(888 112)">
		<circle r="42" fill="#d3d1c8" stroke="#8d8a80" />
		<circle r="30" fill="#c4c2b8" />
		{#each [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11] as i (i)}
			<circle cx={Math.cos((i * Math.PI) / 6) * 20} cy={Math.sin((i * Math.PI) / 6) * 20} r="2.6" fill="#77746b" />
		{/each}
		<circle r="4" fill="#77746b" />
	</g>

	<!-- Chips, glowing with bus activity -->
	{#each CHIPS as chip (chip.id)}
		{#if !chip.hidden}
			{@const g = machine.glow[chip.glow ?? chip.id] ?? 0}
			{@const w = chip.pins * 3.8}
			<g class="chip" transform="translate({chip.x} {chip.y})">
				{#if g > 0.05}
					<rect x="-4" y="-4" width={w + 8} height="34" rx="4" fill="#ffb347" opacity={g * 0.35} filter="url(#chip-glow)" />
				{/if}
				{#each Array.from({ length: chip.pins / 2 }, (_, i) => i) as i (i)}
					<rect x={4 + i * 7.6} y="-3" width="3" height="4" fill="#d6d6d2" />
					<rect x={4 + i * 7.6} y="25" width="3" height="4" fill="#d6d6d2" />
				{/each}
				<rect width={w} height="26" rx="2" fill={chipFill(g)} />
				<circle cx="8" cy="13" r="3" fill="#111" opacity="0.6" />
				<text x={w / 2} y="17" class="chip-type" style:opacity={0.55 + g * 0.45}>{chip.type}</text>
				<text x="0" y="-7" class="silk">{chip.id}</text>
			</g>
		{/if}
	{/each}

	<!-- A few TTL glue chips (decoding), not individually emulated -->
	{#each [[474, 460], [540, 460], [606, 460], [474, 505], [540, 505]] as [x, y] (x * 1000 + y)}
		<rect {x} {y} width="52" height="20" rx="2" fill="#262524" />
	{/each}

	<!-- LEDs and display -->
	<Led x={714} y={218} r={5} level={frame?.leds[0] ?? 0} color="green" />
	<text x="724" y="222" class="silk">OUT</text>
	<Led x={768} y={218} r={5} level={frame?.leds[1] ?? 0} color="red" />
	<text x="778" y="222" class="silk">HALT</text>

	<rect x="704" y="236" width="236" height="74" rx="3" fill="#0b0d0b" />
	{#each [0, 1, 2] as m (m)}
		<rect x={710 + m * 77} y="242" width="72" height="62" rx="2" fill="#101611" />
	{/each}
	{#each frame?.digits ?? [] as segs, i (i)}
		<SevenSeg x={716 + i * 38.5} y={248} scale={0.74} {segs} />
	{/each}
	<text x="822" y="324" class="silk mid">VQE 23</text>

	<!-- Keypad housing -->
	<g>
		<rect x="726" y="334" width="214" height="214" rx="9" fill="#1c1c1e" />
		<rect x="782" y="342" width="102" height="22" rx="2" fill="#f2f1ec" />
		<text x="833" y="359" class="nameplate">LC80</text>
		<g class="legend">
			<text x="744" y="380">PIO</text><text x="772" y="380">AD BD AC BC</text>
			<text x="744" y="391">ADR</text><text x="772" y="391">F8 F9 FA FB</text>
			<text x="744" y="402">CTC</text><text x="772" y="402">0  1  2  3</text>
			<text x="744" y="413">ADR</text><text x="772" y="413">EC ED EE EF</text>
		</g>
		{#each keys as key, i (key.label)}
			{@const x = KX + key.col * PX}
			{@const y = KY + key.row * PY}
			{@const down = frame?.keys_down[i] ?? false}
			{@const [face, edge] = keyColor[key.color] ?? keyColor.cream}
			<KeyHit {machine} index={i} label={key.label} x={x - 2} y={y - 2} w={32} h={24}>
				<text x={x + 13} y={y + 6} class="key-legend">{key.label}</text>
				<rect x={x} y={y + 9} width="28" height="12" rx="2" fill={edge} />
				<rect x={x} y={y + (down ? 10 : 8)} width="28" height="11" rx="2" fill={down ? '#f6d873' : face} />
			</KeyHit>
		{/each}
	</g>

	<!-- Edge connector X1 -->
	<g>
		<rect x="946" y="190" width="10" height="250" fill="#6f6345" />
		{#each Array.from({ length: 29 }, (_, i) => i) as i (i)}
			<rect x="947" y={194 + i * 8.4} width="9" height="5" fill="#d6ad4f" />
		{/each}
	</g>
</svg>

<style>
	svg {
		display: block;
		width: 100%;
		height: auto;
		font-family: var(--font-label);
		user-select: none;
	}
	.plate text {
		font-size: 10.5px;
		fill: #1c1c1c;
		letter-spacing: 0.02em;
	}
	.traces path {
		fill: none;
		stroke: #d9d3bd;
		stroke-width: 2.4;
		opacity: 0.45;
	}
	.chip-type {
		font-size: 11px;
		fill: #f3e9cf;
		text-anchor: middle;
		letter-spacing: 0.06em;
	}
	.silk {
		font-size: 10px;
		fill: #f0ead8;
		opacity: 0.85;
	}
	.silk.mid {
		text-anchor: middle;
	}
	.nameplate {
		font-size: 17px;
		font-weight: 600;
		text-anchor: middle;
		fill: #111;
		letter-spacing: 0.08em;
	}
	.legend text {
		font-size: 8.5px;
		fill: #e7e7e2;
		letter-spacing: 0.05em;
	}
	.key-legend {
		font-size: 8.5px;
		fill: #e7e7e2;
		text-anchor: middle;
		pointer-events: none;
	}
</style>
